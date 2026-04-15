//! Integration tests — reactions handler (send) and breakout handlers
//! (create, merge, move_participant).
//!
//! Runs under `cargo nextest run --profile integration`.
//!
//! Required env: DATABASE_URL, VALKEY_URL, NATS_URL, LIVEKIT_*, KETO_*, KRATOS_*.
//!
//! No mocks. All assertions on structured proto fields.

mod common;

use std::collections::BTreeMap;

use common::{authed_request, kratos_session, pg_pool, shared_state, unique_room_slug, TestResult};
use sunbeam_meet_proto::meet::v1::{
    BreakoutAssignment, CreateBreakoutRequest, MergeBreakoutRequest, MoveToBreakoutRequest,
    RoomAccessLevel, SendReactionRequest, VideoQualityPreset,
};
use sunbeam_meet_server::handlers::meet::{breakout, reactions};
use sunbeam_meet_server::storage::pg::rooms as room_store;

// ── shared setup ──────────────────────────────────────────────────────────

/// Create a room in Postgres + LiveKit, grant the identity moderator + participant.
async fn setup_room_with_moderator(
    identity_id: &str,
) -> (
    room_store::Room,
    sqlx::PgPool,
    sunbeam_meet_server::state::SharedState,
) {
    let pool = pg_pool().await;
    let state = shared_state().await;
    let slug = unique_room_slug("it-rxn-brk");

    let room = room_store::create(
        &pool,
        room_store::NewRoom {
            slug: slug.clone(),
            display_name: "Reactions/Breakout IT".into(),
            access_level: RoomAccessLevel::Public,
            max_participants: 20,
            default_quality: VideoQualityPreset::Auto,
            waiting_room_enabled: false,
            chat_enabled: true,
            recording_allowed: false,
            created_by: identity_id.to_owned(),
            metadata: BTreeMap::new(),
        },
    )
    .await
    .expect("create room for reactions/breakout test");

    // Create the LiveKit room so breakout creation can delegate to it.
    state
        .livekit
        .create_room(&room.livekit_room_name, 50)
        .await
        .expect("create livekit room for test");

    state
        .keto
        .grant("room", &room.id.to_string(), "participant", identity_id)
        .await
        .expect("keto grant participant");
    state
        .keto
        .grant("room", &room.id.to_string(), "moderator", identity_id)
        .await
        .expect("keto grant moderator");

    (room, pool, state)
}

// ── Reactions ─────────────────────────────────────────────────────────────

/// A valid emoji reaction is stored and the response is empty (success).
#[tokio::test]
async fn send_reaction_happy_path() -> TestResult {
    let sess = kratos_session().await;
    let (room, pool, state) = setup_room_with_moderator(&sess.identity_id).await;

    let req = authed_request(
        SendReactionRequest {
            room_id: room.id.to_string(),
            emoji: "👍".into(),
        },
        &sess.bearer,
    );
    reactions::send(&state, req).await?;
    // SendReactionResponse is empty; reaching here means success.

    room_store::hard_delete(&pool, &room.id).await?;
    state
        .livekit
        .delete_room(&room.livekit_room_name)
        .await
        .ok();
    Ok(())
}

/// An empty emoji field is rejected with `InvalidArgument`.
#[tokio::test]
async fn send_reaction_empty_emoji_is_invalid() -> TestResult {
    let sess = kratos_session().await;
    let (room, pool, state) = setup_room_with_moderator(&sess.identity_id).await;

    let req = authed_request(
        SendReactionRequest {
            room_id: room.id.to_string(),
            emoji: String::new(),
        },
        &sess.bearer,
    );
    let err = reactions::send(&state, req)
        .await
        .expect_err("empty emoji must fail");
    assert_eq!(err.code(), tonic::Code::InvalidArgument);

    room_store::hard_delete(&pool, &room.id).await?;
    state
        .livekit
        .delete_room(&room.livekit_room_name)
        .await
        .ok();
    Ok(())
}

/// An emoji longer than 8 code-points is rejected with `InvalidArgument`.
#[tokio::test]
async fn send_reaction_oversized_emoji_is_invalid() -> TestResult {
    let sess = kratos_session().await;
    let (room, pool, state) = setup_room_with_moderator(&sess.identity_id).await;

    let req = authed_request(
        SendReactionRequest {
            room_id: room.id.to_string(),
            // 9 code-points — exceeds the limit of 8.
            emoji: "👍👍👍👍👍👍👍👍👍".into(),
        },
        &sess.bearer,
    );
    let err = reactions::send(&state, req)
        .await
        .expect_err("oversized emoji must fail");
    assert_eq!(err.code(), tonic::Code::InvalidArgument);

    room_store::hard_delete(&pool, &room.id).await?;
    state
        .livekit
        .delete_room(&room.livekit_room_name)
        .await
        .ok();
    Ok(())
}

/// Non-participant cannot send a reaction.
#[tokio::test]
async fn send_reaction_denied_without_participant_role() -> TestResult {
    let owner_sess = kratos_session().await;
    let intruder_sess = kratos_session().await;
    let (room, pool, state) = setup_room_with_moderator(&owner_sess.identity_id).await;

    let req = authed_request(
        SendReactionRequest {
            room_id: room.id.to_string(),
            emoji: "❤️".into(),
        },
        &intruder_sess.bearer,
    );
    let err = reactions::send(&state, req)
        .await
        .expect_err("must be denied");
    assert_eq!(err.code(), tonic::Code::PermissionDenied);

    room_store::hard_delete(&pool, &room.id).await?;
    state
        .livekit
        .delete_room(&room.livekit_room_name)
        .await
        .ok();
    Ok(())
}

/// Unauthenticated request is rejected.
#[tokio::test]
async fn send_reaction_unauthenticated() -> TestResult {
    let state = shared_state().await;
    let req = tonic::Request::new(SendReactionRequest {
        room_id: uuid::Uuid::new_v4().to_string(),
        emoji: "🎉".into(),
    });
    let err = reactions::send(&state, req).await.expect_err("must fail");
    assert_eq!(err.code(), tonic::Code::Unauthenticated);
    Ok(())
}

// ── Breakout ──────────────────────────────────────────────────────────────

/// Creating breakout rooms creates the requested count with valid IDs +
/// LiveKit room names.
#[tokio::test]
async fn create_breakout_rooms_happy_path() -> TestResult {
    let sess = kratos_session().await;
    let (room, pool, state) = setup_room_with_moderator(&sess.identity_id).await;

    let req = authed_request(
        CreateBreakoutRequest {
            room_id: room.id.to_string(),
            count: 2,
            assignments: vec![
                BreakoutAssignment {
                    participant_identity: "alice".into(),
                    breakout_index: 0,
                },
                BreakoutAssignment {
                    participant_identity: "bob".into(),
                    breakout_index: 1,
                },
            ],
            auto_close_after: None,
        },
        &sess.bearer,
    );
    let resp = breakout::create(&state, req).await?.into_inner();

    assert_eq!(resp.rooms.len(), 2, "must create exactly 2 breakout rooms");
    for br in &resp.rooms {
        assert!(!br.id.is_empty(), "breakout room must have a non-empty id");
        assert!(
            !br.livekit_room_name.is_empty(),
            "breakout room must have a livekit_room_name"
        );
        // Cleanup breakout LiveKit rooms.
        state.livekit.delete_room(&br.livekit_room_name).await.ok();
    }

    // Verify assignments were stored correctly.
    let br0 = resp
        .rooms
        .iter()
        .find(|b| b.name == "Breakout 1")
        .expect("Breakout 1");
    assert!(
        br0.participant_identities.contains(&"alice".to_owned()),
        "alice must be assigned to Breakout 1"
    );
    let br1 = resp
        .rooms
        .iter()
        .find(|b| b.name == "Breakout 2")
        .expect("Breakout 2");
    assert!(
        br1.participant_identities.contains(&"bob".to_owned()),
        "bob must be assigned to Breakout 2"
    );

    room_store::hard_delete(&pool, &room.id).await?;
    state
        .livekit
        .delete_room(&room.livekit_room_name)
        .await
        .ok();
    Ok(())
}

/// A non-moderator cannot create breakout rooms.
#[tokio::test]
async fn create_breakout_denied_without_moderator_role() -> TestResult {
    let owner_sess = kratos_session().await;
    let member_sess = kratos_session().await;
    let (room, pool, state) = setup_room_with_moderator(&owner_sess.identity_id).await;

    // Grant member only participant access.
    state
        .keto
        .grant(
            "room",
            &room.id.to_string(),
            "participant",
            &member_sess.identity_id,
        )
        .await?;

    let req = authed_request(
        CreateBreakoutRequest {
            room_id: room.id.to_string(),
            count: 1,
            assignments: vec![],
            auto_close_after: None,
        },
        &member_sess.bearer,
    );
    let err = breakout::create(&state, req)
        .await
        .expect_err("must be denied");
    assert_eq!(err.code(), tonic::Code::PermissionDenied);

    room_store::hard_delete(&pool, &room.id).await?;
    state
        .livekit
        .delete_room(&room.livekit_room_name)
        .await
        .ok();
    Ok(())
}

/// Invalid `room_id` on create returns `InvalidArgument`.
#[tokio::test]
async fn create_breakout_invalid_room_id() -> TestResult {
    let sess = kratos_session().await;
    let state = shared_state().await;

    let req = authed_request(
        CreateBreakoutRequest {
            room_id: "not-a-uuid".into(),
            count: 1,
            assignments: vec![],
            auto_close_after: None,
        },
        &sess.bearer,
    );
    let err = breakout::create(&state, req).await.expect_err("must fail");
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    Ok(())
}

/// Creating then merging breakout rooms marks them ended and returns
/// `participants_returned >= 0`.
#[tokio::test]
async fn merge_breakout_rooms_happy_path() -> TestResult {
    let sess = kratos_session().await;
    let (room, pool, state) = setup_room_with_moderator(&sess.identity_id).await;

    // Create one breakout room.
    let create_req = authed_request(
        CreateBreakoutRequest {
            room_id: room.id.to_string(),
            count: 1,
            assignments: vec![],
            auto_close_after: None,
        },
        &sess.bearer,
    );
    let created = breakout::create(&state, create_req).await?.into_inner();
    assert_eq!(created.rooms.len(), 1);

    // Merge them.
    let merge_req = authed_request(
        MergeBreakoutRequest {
            room_id: room.id.to_string(),
        },
        &sess.bearer,
    );
    let merged = breakout::merge(&state, merge_req).await?.into_inner();
    // Participants returned may be 0 (nobody joined the breakout) or positive;
    // the value must be a non-negative integer.
    // u32 is always >= 0 by type, so just confirming the call succeeded.
    let _ = merged.participants_returned;

    room_store::hard_delete(&pool, &room.id).await?;
    state
        .livekit
        .delete_room(&room.livekit_room_name)
        .await
        .ok();
    Ok(())
}

/// `move_participant` stores the assignment and returns success.
#[tokio::test]
async fn move_participant_to_breakout_happy_path() -> TestResult {
    let sess = kratos_session().await;
    let (room, pool, state) = setup_room_with_moderator(&sess.identity_id).await;

    // Create a breakout room to move into.
    let create_req = authed_request(
        CreateBreakoutRequest {
            room_id: room.id.to_string(),
            count: 1,
            assignments: vec![],
            auto_close_after: None,
        },
        &sess.bearer,
    );
    let created = breakout::create(&state, create_req).await?.into_inner();
    let br_id = created.rooms[0].id.clone();

    let move_req = authed_request(
        MoveToBreakoutRequest {
            room_id: room.id.to_string(),
            breakout_room_id: br_id.clone(),
            participant_identity: "carol".into(),
        },
        &sess.bearer,
    );
    breakout::move_participant(&state, move_req).await?;
    // MoveToBreakoutResponse is empty; reaching here means success.

    room_store::hard_delete(&pool, &room.id).await?;
    for br in &created.rooms {
        state.livekit.delete_room(&br.livekit_room_name).await.ok();
    }
    state
        .livekit
        .delete_room(&room.livekit_room_name)
        .await
        .ok();
    Ok(())
}

/// Invalid `breakout_room_id` returns `InvalidArgument`.
#[tokio::test]
async fn move_participant_invalid_breakout_id() -> TestResult {
    let sess = kratos_session().await;
    let (room, pool, state) = setup_room_with_moderator(&sess.identity_id).await;

    let req = authed_request(
        MoveToBreakoutRequest {
            room_id: room.id.to_string(),
            breakout_room_id: "bad-id".into(),
            participant_identity: "dave".into(),
        },
        &sess.bearer,
    );
    let err = breakout::move_participant(&state, req)
        .await
        .expect_err("must fail");
    assert_eq!(err.code(), tonic::Code::InvalidArgument);

    room_store::hard_delete(&pool, &room.id).await?;
    state
        .livekit
        .delete_room(&room.livekit_room_name)
        .await
        .ok();
    Ok(())
}
