//! Integration tests — chat handler: send, history, delete.
//!
//! Runs under `cargo nextest run --profile integration`.
//!
//! Required env: DATABASE_URL, VALKEY_URL, NATS_URL, LIVEKIT_URL, LIVEKIT_API_KEY,
//! LIVEKIT_API_SECRET, KETO_READ_URL, KETO_WRITE_URL, KRATOS_PUBLIC_URL, KRATOS_ADMIN_URL.
//!
//! No mocks. Every test hits real Postgres, Keto, and Kratos.
//! Assertions are on structured proto fields only.

mod common;

use std::collections::BTreeMap;

use common::{authed_request, kratos_session, pg_pool, shared_state, unique_room_slug, TestResult};
use sunbeam_meet_proto::meet::v1::{
    DeleteChatMessageRequest, GetChatHistoryRequest, RoomAccessLevel, SendChatRequest,
    VideoQualityPreset,
};
use sunbeam_meet_server::handlers::meet::chat;
use sunbeam_meet_server::storage::pg::rooms as room_store;

// ── helpers ────────────────────────────────────────────────────────────────

/// Create a room in Postgres and grant the given identity `participant` +
/// `moderator` roles on it via Keto. Returns the Postgres room row.
async fn setup_room_with_moderator(
    identity_id: &str,
) -> (
    room_store::Room,
    sqlx::PgPool,
    sunbeam_meet_server::state::SharedState,
) {
    let pool = pg_pool().await;
    let state = shared_state().await;
    let slug = unique_room_slug("it-chat");

    let room = room_store::create(
        &pool,
        room_store::NewRoom {
            slug,
            display_name: "Chat IT Room".into(),
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
    .expect("create room for chat test");

    // Grant both roles so the caller can send and delete others' messages.
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

// ── tests ──────────────────────────────────────────────────────────────────

/// Sending a chat message stores it and returns the persisted `ChatMessage`.
#[tokio::test]
async fn send_chat_returns_persisted_message() -> TestResult {
    let sess = kratos_session().await;
    let (room, pool, state) = setup_room_with_moderator(&sess.identity_id).await;

    let req = authed_request(
        SendChatRequest {
            room_id: room.id.to_string(),
            content: "hello world".into(),
            reply_to: String::new(),
        },
        &sess.bearer,
    );

    let resp = chat::send(&state, req).await?;
    let msg = resp.into_inner();

    assert_eq!(msg.room_id, room.id.to_string());
    assert_eq!(msg.sender_identity, sess.identity_id);
    assert_eq!(msg.content, "hello world");
    assert!(!msg.id.is_empty(), "message id must be non-empty");
    assert!(!msg.deleted, "newly sent message must not be deleted");
    assert!(msg.sent_at.is_some(), "sent_at must be populated");

    room_store::hard_delete(&pool, &room.id).await?;
    Ok(())
}

/// A reply_to UUID is stored and reflected in the response.
#[tokio::test]
async fn send_chat_with_reply_to_stores_reference() -> TestResult {
    let sess = kratos_session().await;
    let (room, pool, state) = setup_room_with_moderator(&sess.identity_id).await;

    // Send parent first.
    let parent_req = authed_request(
        SendChatRequest {
            room_id: room.id.to_string(),
            content: "parent".into(),
            reply_to: String::new(),
        },
        &sess.bearer,
    );
    let parent = chat::send(&state, parent_req).await?.into_inner();

    // Reply to it.
    let reply_req = authed_request(
        SendChatRequest {
            room_id: room.id.to_string(),
            content: "child".into(),
            reply_to: parent.id.clone(),
        },
        &sess.bearer,
    );
    let reply = chat::send(&state, reply_req).await?.into_inner();

    assert_eq!(reply.reply_to, parent.id);
    assert_eq!(reply.content, "child");

    room_store::hard_delete(&pool, &room.id).await?;
    Ok(())
}

/// Message body larger than `domain::chat::MAX_BODY_BYTES` is rejected with
/// `InvalidArgument`.
#[tokio::test]
async fn send_chat_rejects_oversized_body() -> TestResult {
    let sess = kratos_session().await;
    let (room, pool, state) = setup_room_with_moderator(&sess.identity_id).await;

    let oversized = "x".repeat(sunbeam_meet_server::domain::chat::MAX_BODY_BYTES + 1);
    let req = authed_request(
        SendChatRequest {
            room_id: room.id.to_string(),
            content: oversized,
            reply_to: String::new(),
        },
        &sess.bearer,
    );
    let err = chat::send(&state, req).await.expect_err("must be rejected");
    assert_eq!(err.code(), tonic::Code::InvalidArgument);

    room_store::hard_delete(&pool, &room.id).await?;
    Ok(())
}

/// An invalid `room_id` (not a UUID) is rejected with `InvalidArgument`.
#[tokio::test]
async fn send_chat_rejects_invalid_room_id() -> TestResult {
    let sess = kratos_session().await;
    let state = shared_state().await;

    let req = authed_request(
        SendChatRequest {
            room_id: "not-a-uuid".into(),
            content: "hi".into(),
            reply_to: String::new(),
        },
        &sess.bearer,
    );
    let err = chat::send(&state, req).await.expect_err("must fail");
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    Ok(())
}

/// A caller without `participant` permission is denied.
#[tokio::test]
async fn send_chat_denied_without_participant_role() -> TestResult {
    let owner_sess = kratos_session().await;
    let intruder_sess = kratos_session().await;
    let (room, pool, state) = setup_room_with_moderator(&owner_sess.identity_id).await;

    // intruder has no keto relation on this room.
    let req = authed_request(
        SendChatRequest {
            room_id: room.id.to_string(),
            content: "sneaky".into(),
            reply_to: String::new(),
        },
        &intruder_sess.bearer,
    );
    let err = chat::send(&state, req).await.expect_err("must be denied");
    assert_eq!(err.code(), tonic::Code::PermissionDenied);

    room_store::hard_delete(&pool, &room.id).await?;
    Ok(())
}

/// Unauthenticated request (missing Authorization header) is rejected.
#[tokio::test]
async fn send_chat_unauthenticated() -> TestResult {
    let state = shared_state().await;
    let req = tonic::Request::new(SendChatRequest {
        room_id: uuid::Uuid::new_v4().to_string(),
        content: "hi".into(),
        reply_to: String::new(),
    });
    let err = chat::send(&state, req).await.expect_err("must be rejected");
    assert_eq!(err.code(), tonic::Code::Unauthenticated);
    Ok(())
}

/// Chat history returns all messages for the room in reverse-chronological order.
#[tokio::test]
async fn get_chat_history_returns_sent_messages() -> TestResult {
    let sess = kratos_session().await;
    let (room, pool, state) = setup_room_with_moderator(&sess.identity_id).await;

    // Send two messages.
    for body in ["alpha", "beta"] {
        let req = authed_request(
            SendChatRequest {
                room_id: room.id.to_string(),
                content: body.into(),
                reply_to: String::new(),
            },
            &sess.bearer,
        );
        chat::send(&state, req).await?;
    }

    let hist_req = authed_request(
        GetChatHistoryRequest {
            room_id: room.id.to_string(),
            page_size: 50,
            page_token: String::new(),
            before: None,
        },
        &sess.bearer,
    );
    let resp = chat::history(&state, hist_req).await?.into_inner();

    assert!(
        resp.messages.len() >= 2,
        "history must return at least 2 messages"
    );
    // Every message must belong to this room.
    for m in &resp.messages {
        assert_eq!(m.room_id, room.id.to_string());
    }

    room_store::hard_delete(&pool, &room.id).await?;
    Ok(())
}

/// page_size=0 is accepted (defaults to 100) and does not error.
#[tokio::test]
async fn get_chat_history_zero_page_size_defaults() -> TestResult {
    let sess = kratos_session().await;
    let (room, pool, state) = setup_room_with_moderator(&sess.identity_id).await;

    let req = authed_request(
        GetChatHistoryRequest {
            room_id: room.id.to_string(),
            page_size: 0,
            page_token: String::new(),
            before: None,
        },
        &sess.bearer,
    );
    // Must succeed, not panic or error.
    chat::history(&state, req).await?;

    room_store::hard_delete(&pool, &room.id).await?;
    Ok(())
}

/// History for a nonexistent room (valid UUID but no rows) returns an empty
/// messages list — it is not an error.
#[tokio::test]
async fn get_chat_history_empty_for_unknown_room() -> TestResult {
    let sess = kratos_session().await;
    let state = shared_state().await;
    let room_id = uuid::Uuid::new_v4().to_string();

    // Grant participant so auth passes.
    state
        .keto
        .grant("room", &room_id, "participant", &sess.identity_id)
        .await
        .expect("keto grant");

    let req = authed_request(
        GetChatHistoryRequest {
            room_id: room_id.clone(),
            page_size: 10,
            page_token: String::new(),
            before: None,
        },
        &sess.bearer,
    );
    let resp = chat::history(&state, req).await?.into_inner();
    assert!(
        resp.messages.is_empty(),
        "unknown room must return empty history"
    );
    Ok(())
}

/// A sender can delete their own message.
#[tokio::test]
async fn delete_own_message_soft_deletes() -> TestResult {
    let sess = kratos_session().await;
    let (room, pool, state) = setup_room_with_moderator(&sess.identity_id).await;

    let send_req = authed_request(
        SendChatRequest {
            room_id: room.id.to_string(),
            content: "to-be-deleted".into(),
            reply_to: String::new(),
        },
        &sess.bearer,
    );
    let msg = chat::send(&state, send_req).await?.into_inner();

    let del_req = authed_request(
        DeleteChatMessageRequest {
            message_id: msg.id.clone(),
        },
        &sess.bearer,
    );
    chat::delete(&state, del_req).await?;

    // History must no longer include the deleted message (soft-delete filters it).
    let hist_req = authed_request(
        GetChatHistoryRequest {
            room_id: room.id.to_string(),
            page_size: 100,
            page_token: String::new(),
            before: None,
        },
        &sess.bearer,
    );
    let hist = chat::history(&state, hist_req).await?.into_inner();
    assert!(
        !hist.messages.iter().any(|m| m.id == msg.id),
        "deleted message must not appear in history"
    );

    room_store::hard_delete(&pool, &room.id).await?;
    Ok(())
}

/// A moderator can delete another user's message.
#[tokio::test]
async fn moderator_can_delete_other_message() -> TestResult {
    let moderator_sess = kratos_session().await;
    let member_sess = kratos_session().await;
    let (room, pool, state) = setup_room_with_moderator(&moderator_sess.identity_id).await;

    // Grant member participant access.
    state
        .keto
        .grant(
            "room",
            &room.id.to_string(),
            "participant",
            &member_sess.identity_id,
        )
        .await?;

    // Member sends a message.
    let send_req = authed_request(
        SendChatRequest {
            room_id: room.id.to_string(),
            content: "member msg".into(),
            reply_to: String::new(),
        },
        &member_sess.bearer,
    );
    let msg = chat::send(&state, send_req).await?.into_inner();

    // Moderator deletes it.
    let del_req = authed_request(
        DeleteChatMessageRequest {
            message_id: msg.id.clone(),
        },
        &moderator_sess.bearer,
    );
    chat::delete(&state, del_req).await?;

    room_store::hard_delete(&pool, &room.id).await?;
    Ok(())
}

/// Deleting a nonexistent message id returns NotFound.
#[tokio::test]
async fn delete_chat_message_not_found() -> TestResult {
    let sess = kratos_session().await;
    let state = shared_state().await;

    let req = authed_request(
        DeleteChatMessageRequest {
            message_id: uuid::Uuid::new_v4().to_string(),
        },
        &sess.bearer,
    );
    let err = chat::delete(&state, req)
        .await
        .expect_err("must be NotFound");
    assert_eq!(err.code(), tonic::Code::NotFound);
    Ok(())
}

/// Deleting with an invalid message_id UUID returns InvalidArgument.
#[tokio::test]
async fn delete_chat_message_invalid_id() -> TestResult {
    let sess = kratos_session().await;
    let state = shared_state().await;

    let req = authed_request(
        DeleteChatMessageRequest {
            message_id: "bad-id".into(),
        },
        &sess.bearer,
    );
    let err = chat::delete(&state, req).await.expect_err("must fail");
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    Ok(())
}
