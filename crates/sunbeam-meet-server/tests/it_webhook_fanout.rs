//! Integration test — LiveKit webhook ingest → JoinRoom fan-out.
//!
//! Required env: `LIVEKIT_API_KEY`, `LIVEKIT_API_SECRET`, `NATS_URL`.
//!
//! Stands up the sunbeam-meet axum app in-process, opens two subscribers on
//! the fan-out hub (simulating two connected `JoinRoom` bidi streams), then
//! POSTs a signed webhook to `/webhooks/livekit`. Both subscribers must
//! receive the translated MeetServerMessage.

mod common;

use common::{env_required, unique_room_slug, TestResult};
use jsonwebtoken::{encode, EncodingKey, Header};
use serde::Serialize;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use sunbeam_meet_proto::meet::v1::meet_server_message::Payload;
use sunbeam_meet_server::stream::join_room::{Hub, SubscribeOptions};
use sunbeam_meet_server::webhooks::livekit as hook;
use tokio::time::timeout;

#[derive(Serialize)]
struct Claims {
    iss: String,
    iat: u64,
    exp: u64,
    sha256: String,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[tokio::test]
async fn webhook_delivers_participant_joined_to_all_subscribers() -> TestResult {
    let api_key = env_required("LIVEKIT_API_KEY");
    let api_secret = env_required("LIVEKIT_API_SECRET");

    // In-process hub + webhook ingestor.
    let hub = Hub::new();
    let ingestor = hook::Ingestor::new(hub.clone(), api_secret.clone());

    let room_id = unique_room_slug("it-fanout");

    let mut sub_a = hub.subscribe(&room_id, SubscribeOptions::default()).await?;
    let mut sub_b = hub.subscribe(&room_id, SubscribeOptions::default()).await?;

    // Build a LiveKit-shaped webhook body. We use the typed helper so the
    // wire encoding matches the production decoder rather than drifting.
    let body = hook::test_helpers::participant_joined_event(&room_id, "identity-1", "Display One");
    let body_bytes = serde_json::to_vec(&body)?;
    let sha256 = hook::test_helpers::sha256_b64(&body_bytes);

    let claims = Claims {
        iss: api_key.clone(),
        iat: now(),
        exp: now() + 60,
        sha256,
    };
    let token = encode(
        &Header::new(jsonwebtoken::Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(api_secret.as_bytes()),
    )?;

    ingestor.handle(&token, &body_bytes).await?;

    for (label, rx) in [("a", &mut sub_a), ("b", &mut sub_b)] {
        let got = timeout(Duration::from_secs(2), rx.recv())
            .await
            .unwrap_or_else(|_| panic!("{label} timed out"))?;
        match got.payload {
            Some(Payload::ParticipantJoined(pj)) => {
                let p = pj.participant.expect("participant field");
                assert_eq!(p.identity, "identity-1", "{label} identity");
                assert_eq!(p.display_name, "Display One", "{label} display name");
            }
            other => panic!("{label} got unexpected payload: {other:?}"),
        }
    }

    Ok(())
}
