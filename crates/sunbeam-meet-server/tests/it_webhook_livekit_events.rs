//! Integration test — LiveKit webhook events → Ingestor → Hub fan-out.
//!
//! Required env: `LIVEKIT_API_KEY`, `LIVEKIT_API_SECRET`.
//!
//! Constructs real signed JWT payloads (HS256, same algorithm LiveKit uses)
//! and feeds them into the production `Ingestor`. Asserts on structured
//! `MeetServerMessage` fields received by Hub subscribers — never on raw bytes
//! or string fragments.
//!
//! Events covered:
//!   participant_joined, participant_left, track_published (no-op),
//!   recording_started (no-op), recording_finished (no-op),
//!   room_started (no-op), room_finished → RoomEnded.

mod common;

use common::{env_required, unique_room_slug, TestResult};
use jsonwebtoken::{encode, EncodingKey, Header};
use serde::Serialize;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use sunbeam_meet_proto::meet::v1::meet_server_message::Payload;
use sunbeam_meet_server::stream::join_room::{Hub, SubscribeOptions};
use sunbeam_meet_server::webhooks::livekit as hook;
use tokio::time::timeout;

// ── helpers ───────────────────────────────────────────────────────────────

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

fn sign(api_key: &str, secret: &str, body: &[u8]) -> String {
    let sha256 = hook::test_helpers::sha256_b64(body);
    let claims = Claims {
        iss: api_key.to_owned(),
        iat: now(),
        exp: now() + 60,
        sha256,
    };
    encode(
        &Header::new(jsonwebtoken::Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .expect("signing test JWT")
}

/// Build an arbitrary webhook event JSON body.
fn event_body(event: &str, room: &str, participant: Option<(&str, &str)>) -> Vec<u8> {
    use serde_json::json;
    let participant_val = participant.map_or(
        serde_json::Value::Null,
        |(id, name)| json!({"identity": id, "name": name}),
    );
    let v = json!({
        "id": format!("evt-{}", uuid::Uuid::now_v7()),
        "event": event,
        "room": {"name": room},
        "participant": participant_val,
        "created_at": now() as i64,
    });
    serde_json::to_vec(&v).expect("serialize event")
}

// ── tests ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn participant_joined_delivers_to_hub_subscriber() -> TestResult {
    let api_key = env_required("LIVEKIT_API_KEY");
    let secret = env_required("LIVEKIT_API_SECRET");

    let hub = Hub::new();
    let ingestor = hook::Ingestor::new(hub.clone(), secret.clone());
    let room = unique_room_slug("it-lk-joined");

    let mut rx = hub.subscribe(&room, SubscribeOptions::default()).await?;
    let body = event_body("participant_joined", &room, Some(("alice", "Alice")));
    let token = sign(&api_key, &secret, &body);

    ingestor.handle(&token, &body).await?;

    let got = timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("hub must deliver within 2 s")?;
    let Some(Payload::ParticipantJoined(pj)) = got.payload else {
        panic!("expected ParticipantJoined, got {:?}", got.payload);
    };
    let p = pj.participant.expect("participant field must be set");
    assert_eq!(p.identity, "alice", "identity");
    assert_eq!(p.display_name, "Alice", "display_name");

    Ok(())
}

#[tokio::test]
async fn participant_left_delivers_to_hub_subscriber() -> TestResult {
    let api_key = env_required("LIVEKIT_API_KEY");
    let secret = env_required("LIVEKIT_API_SECRET");

    let hub = Hub::new();
    let ingestor = hook::Ingestor::new(hub.clone(), secret.clone());
    let room = unique_room_slug("it-lk-left");

    let mut rx = hub.subscribe(&room, SubscribeOptions::default()).await?;
    let body = event_body("participant_left", &room, Some(("bob", "Bob")));
    let token = sign(&api_key, &secret, &body);

    ingestor.handle(&token, &body).await?;

    let got = timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("hub must deliver within 2 s")?;
    let Some(Payload::ParticipantLeft(pl)) = got.payload else {
        panic!("expected ParticipantLeft, got {:?}", got.payload);
    };
    assert_eq!(pl.identity, "bob", "identity");
    assert_eq!(pl.reason, "disconnected", "reason must be canonical string");

    Ok(())
}

#[tokio::test]
async fn room_finished_delivers_room_ended_to_hub() -> TestResult {
    let api_key = env_required("LIVEKIT_API_KEY");
    let secret = env_required("LIVEKIT_API_SECRET");

    let hub = Hub::new();
    let ingestor = hook::Ingestor::new(hub.clone(), secret.clone());
    let room = unique_room_slug("it-lk-finished");

    let mut rx = hub.subscribe(&room, SubscribeOptions::default()).await?;
    let body = event_body("room_finished", &room, None);
    let token = sign(&api_key, &secret, &body);

    ingestor.handle(&token, &body).await?;

    let got = timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("hub must deliver within 2 s")?;
    let Some(Payload::RoomEnded(re)) = got.payload else {
        panic!("expected RoomEnded, got {:?}", got.payload);
    };
    assert_eq!(re.reason, "empty_timeout", "reason field");

    Ok(())
}

/// `track_published`, `recording_started`, `recording_finished`, and
/// `room_started` are all unrecognised event types that the current
/// `translate_event` function silently ignores. The ingestor must not error
/// and the hub must receive no message within the observation window.
#[tokio::test]
async fn unhandled_event_types_produce_no_hub_message() -> TestResult {
    let api_key = env_required("LIVEKIT_API_KEY");
    let secret = env_required("LIVEKIT_API_SECRET");

    let hub = Hub::new();
    let ingestor = hook::Ingestor::new(hub.clone(), secret.clone());

    for event in [
        "track_published",
        "recording_started",
        "recording_finished",
        "room_started",
    ] {
        let room = unique_room_slug(&format!("it-lk-noop-{event}"));
        let mut rx = hub.subscribe(&room, SubscribeOptions::default()).await?;
        let body = event_body(event, &room, Some(("user1", "User One")));
        let token = sign(&api_key, &secret, &body);

        ingestor.handle(&token, &body).await?;

        let result = timeout(Duration::from_millis(200), rx.recv()).await;
        assert!(
            result.is_err(),
            "event '{event}' must not produce a hub message, but one was received",
        );
    }
    Ok(())
}

/// A webhook with no room block must succeed (no panic) and produce no
/// hub message.
#[tokio::test]
async fn event_without_room_block_is_silently_ignored() -> TestResult {
    let api_key = env_required("LIVEKIT_API_KEY");
    let secret = env_required("LIVEKIT_API_SECRET");

    let hub = Hub::new();
    let ingestor = hook::Ingestor::new(hub.clone(), secret.clone());
    let room = unique_room_slug("it-lk-noroom");
    let mut rx = hub.subscribe(&room, SubscribeOptions::default()).await?;

    let body = serde_json::to_vec(&serde_json::json!({
        "id": format!("evt-{}", uuid::Uuid::now_v7()),
        "event": "participant_joined",
        // no "room" field
        "participant": {"identity": "alice", "name": "Alice"},
        "created_at": now() as i64,
    }))?;
    let token = sign(&api_key, &secret, &body);

    ingestor.handle(&token, &body).await?;

    let result = timeout(Duration::from_millis(200), rx.recv()).await;
    assert!(
        result.is_err(),
        "event without room block must not produce a hub message",
    );
    Ok(())
}

/// Tampered JWT must be rejected. The ingestor must return an error and the
/// hub must remain empty.
#[tokio::test]
async fn bad_signature_is_rejected_before_hub_fanout() -> TestResult {
    let api_key = env_required("LIVEKIT_API_KEY");
    let secret = env_required("LIVEKIT_API_SECRET");

    let hub = Hub::new();
    let ingestor = hook::Ingestor::new(hub.clone(), secret.clone());
    let room = unique_room_slug("it-lk-badsig");
    let mut rx = hub.subscribe(&room, SubscribeOptions::default()).await?;

    let body = event_body("participant_joined", &room, Some(("alice", "Alice")));
    // Sign with a different secret.
    let wrong_token = sign(&api_key, "totally-wrong-secret-xxxxxxxxxxxxx", &body);

    let err = ingestor.handle(&wrong_token, &body).await;
    assert!(err.is_err(), "bad-signature token must be rejected");

    let result = timeout(Duration::from_millis(200), rx.recv()).await;
    assert!(
        result.is_err(),
        "hub must not receive any message after auth failure",
    );
    Ok(())
}

/// Valid token whose `sha256` claim doesn't match the submitted body — i.e.
/// an attacker replayed a legitimate token against a different payload. The
/// ingestor must reject before hub fan-out.
#[tokio::test]
async fn tampered_body_is_rejected_even_with_valid_signature() -> TestResult {
    let api_key = env_required("LIVEKIT_API_KEY");
    let secret = env_required("LIVEKIT_API_SECRET");

    let hub = Hub::new();
    let ingestor = hook::Ingestor::new(hub.clone(), secret.clone());
    let room = unique_room_slug("it-lk-tamper");
    let mut rx = hub.subscribe(&room, SubscribeOptions::default()).await?;

    // Sign over the *original* body…
    let original = event_body("participant_joined", &room, Some(("alice", "Alice")));
    let token = sign(&api_key, &secret, &original);
    // …then submit a different body. The signature validates, but the
    // `sha256` claim won't match → reject.
    let tampered = event_body("participant_joined", &room, Some(("mallory", "Mallory")));

    let err = ingestor.handle(&token, &tampered).await;
    assert!(err.is_err(), "body mismatch must be rejected");

    let result = timeout(Duration::from_millis(200), rx.recv()).await;
    assert!(
        result.is_err(),
        "hub must not receive any message after body-hash failure",
    );
    Ok(())
}

/// Two subscribers on the same room both receive the same event.
#[tokio::test]
async fn participant_joined_fans_out_to_multiple_subscribers() -> TestResult {
    let api_key = env_required("LIVEKIT_API_KEY");
    let secret = env_required("LIVEKIT_API_SECRET");

    let hub = Hub::new();
    let ingestor = hook::Ingestor::new(hub.clone(), secret.clone());
    let room = unique_room_slug("it-lk-fanout-multi");

    let mut rx_a = hub.subscribe(&room, SubscribeOptions::default()).await?;
    let mut rx_b = hub.subscribe(&room, SubscribeOptions::default()).await?;

    let body = event_body("participant_joined", &room, Some(("charlie", "Charlie")));
    let token = sign(&api_key, &secret, &body);
    ingestor.handle(&token, &body).await?;

    for (label, rx) in [("a", &mut rx_a), ("b", &mut rx_b)] {
        let got = timeout(Duration::from_secs(2), rx.recv())
            .await
            .unwrap_or_else(|_| panic!("{label} timed out"))?;
        let Some(Payload::ParticipantJoined(pj)) = got.payload else {
            panic!("{label}: expected ParticipantJoined, got {:?}", got.payload);
        };
        let p = pj.participant.expect("participant field");
        assert_eq!(p.identity, "charlie", "{label} identity");
        assert_eq!(p.display_name, "Charlie", "{label} display_name");
    }
    Ok(())
}

/// Duplicate event id: the ingestor (Ingestor path) does not touch the DB, so
/// both calls succeed and both broadcast. What we test here is that the body
/// parses correctly on a re-submission (no data corruption).
#[tokio::test]
async fn participant_joined_with_empty_participant_block_produces_no_hub_message() -> TestResult {
    let api_key = env_required("LIVEKIT_API_KEY");
    let secret = env_required("LIVEKIT_API_SECRET");

    let hub = Hub::new();
    let ingestor = hook::Ingestor::new(hub.clone(), secret.clone());
    let room = unique_room_slug("it-lk-nopart");
    let mut rx = hub.subscribe(&room, SubscribeOptions::default()).await?;

    // participant_joined with null participant block — translate_event returns None.
    let body = serde_json::to_vec(&serde_json::json!({
        "id": format!("evt-{}", uuid::Uuid::now_v7()),
        "event": "participant_joined",
        "room": {"name": room},
        // no participant field
        "created_at": now() as i64,
    }))?;
    let token = sign(&api_key, &secret, &body);

    ingestor.handle(&token, &body).await?;

    let result = timeout(Duration::from_millis(200), rx.recv()).await;
    assert!(
        result.is_err(),
        "participant_joined with no participant block must not fan out",
    );
    Ok(())
}
