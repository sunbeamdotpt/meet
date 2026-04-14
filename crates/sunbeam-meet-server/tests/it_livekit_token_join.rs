//! Integration test — token mint verified by a real LiveKit room join.
//!
//! Runs under `cargo nextest run --profile integration`.
//!
//! Required env: `LIVEKIT_URL`, `LIVEKIT_API_KEY`, `LIVEKIT_API_SECRET`.
//!
//! Flow:
//!   1. sunbeam-meet mints a JWT for `room = test-xxx`, `identity = probe`.
//!   2. Connect to LiveKit as that participant (via LiveKit server API).
//!   3. List participants via the LiveKit API — probe must be present.
//!   4. Leave, delete the room, done.
//!
//! We assert on the structured API response (`ListParticipantsResponse`),
//! never on log text or error messages.

mod common;

use common::{env_required, unique_room_slug, wait_for, TestResult, DEFAULT_WAIT};
use std::time::Duration;
use sunbeam_meet_proto::meet::v1::ParticipantRole;
use sunbeam_meet_server::clients::livekit::{
    self as lk,
    grants::{self, RoomScope},
};

#[tokio::test]
async fn minted_token_actually_joins_livekit_room() -> TestResult {
    let url = env_required("LIVEKIT_URL");
    let key = env_required("LIVEKIT_API_KEY");
    let secret = env_required("LIVEKIT_API_SECRET");

    let client = lk::Client::connect(&url, &key, &secret).await?;
    let room_name = unique_room_slug("it-lk-join");

    // Create the room explicitly so list-participants has a target even if
    // nobody joins yet.
    client.create_room(&room_name, 60).await?;

    // Mint a participant token.
    let grant = grants::for_role(
        ParticipantRole::Member,
        RoomScope {
            room: room_name.clone(),
            identity: "probe-1".into(),
            name: "Probe".into(),
        },
    );
    let token = client.mint_token(&grant, Duration::from_secs(300))?;

    // Use the LiveKit client SDK (through `clients::livekit::probe_join`) to
    // actually connect with the token. This verifies the JWT is valid end
    // to end, not just structurally.
    let probe = client.probe_join(&room_name, &token).await?;

    // Poll list-participants until the probe shows up.
    let found = wait_for(DEFAULT_WAIT, || async {
        let resp = client
            .list_participants(&room_name)
            .await
            .map_err(|e| e.to_string())?;
        Ok(resp.into_iter().find(|p| p.identity == "probe-1"))
    })
    .await
    .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.into() })?;

    assert_eq!(found.identity, "probe-1");
    assert_eq!(found.name, "Probe");

    probe.disconnect().await?;
    client.delete_room(&room_name).await?;
    Ok(())
}

#[tokio::test]
async fn hidden_agent_token_does_not_appear_in_public_participant_list() -> TestResult {
    let url = env_required("LIVEKIT_URL");
    let key = env_required("LIVEKIT_API_KEY");
    let secret = env_required("LIVEKIT_API_SECRET");

    let client = lk::Client::connect(&url, &key, &secret).await?;
    let room_name = unique_room_slug("it-lk-hidden");
    client.create_room(&room_name, 60).await?;

    let grant = grants::hidden(RoomScope {
        room: room_name.clone(),
        identity: "agent-whisper".into(),
        name: "Whisper".into(),
    });
    let token = client.mint_token(&grant, Duration::from_secs(300))?;
    let probe = client.probe_join(&room_name, &token).await?;

    // Give the join a moment, then list. `hidden=true` means LiveKit's
    // participant iterator excludes this identity from the visible list
    // (the server-side API still sees it, depending on flag — we assert the
    // public iterator view here).
    tokio::time::sleep(Duration::from_secs(2)).await;
    let visible = client.list_participants_visible(&room_name).await?;
    assert!(
        !visible.iter().any(|p| p.identity == "agent-whisper"),
        "hidden participant must not appear in the public list",
    );

    probe.disconnect().await?;
    client.delete_room(&room_name).await?;
    Ok(())
}
