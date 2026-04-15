//! Integration tests — auth handler (generate_token) and participants handlers
//! (invite, kick, update_role, mute).
//!
//! Runs under `cargo nextest run --profile integration`.
//!
//! Required env: DATABASE_URL, VALKEY_URL, NATS_URL, LIVEKIT_*, KETO_*, KRATOS_*.
//!
//! No mocks. Assertions on structured proto fields only.

mod common;

use std::collections::BTreeMap;

use common::{authed_request, kratos_session, pg_pool, shared_state, unique_room_slug, TestResult};
use sunbeam_meet_proto::meet::v1::{
    GenerateTokenRequest, InviteMethod, InviteRequest, KickRequest, MuteParticipantRequest,
    ParticipantRole, RoomAccessLevel, UpdateRoleRequest, VideoQualityPreset,
};
use sunbeam_meet_server::handlers::meet::{auth, participants};
use sunbeam_meet_server::storage::pg::rooms as room_store;

// ── shared setup ──────────────────────────────────────────────────────────

async fn setup_room_with_moderator(
    identity_id: &str,
) -> (
    room_store::Room,
    sqlx::PgPool,
    sunbeam_meet_server::state::SharedState,
) {
    let pool = pg_pool().await;
    let state = shared_state().await;
    let slug = unique_room_slug("it-auth-pt");

    let room = room_store::create(
        &pool,
        room_store::NewRoom {
            slug,
            display_name: "Auth/Participants IT".into(),
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
    .expect("create room for auth/participants test");

    // Create the LiveKit room so token minting finds a `livekit_room_name`.
    state
        .livekit
        .create_room(&room.livekit_room_name, 50)
        .await
        .expect("create livekit room");

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

// ── GenerateToken ─────────────────────────────────────────────────────────

/// A participant with the `participant` relation can mint their own token.
/// The response carries a non-empty JWT and a populated LiveKit URL.
#[tokio::test]
async fn generate_token_happy_path_returns_jwt() -> TestResult {
    let sess = kratos_session().await;
    let (room, pool, state) = setup_room_with_moderator(&sess.identity_id).await;

    let req = authed_request(
        GenerateTokenRequest {
            room_id: room.id.to_string(),
            participant_identity: String::new(), // defaults to caller's identity
            participant_name: "Test User".into(),
            role: ParticipantRole::Member as i32,
            ttl: None,
        },
        &sess.bearer,
    );
    let resp = auth::generate_token(&state, req).await?.into_inner();

    assert!(!resp.token.is_empty(), "token field must be non-empty");
    assert!(
        !resp.livekit_url.is_empty(),
        "livekit_url must be non-empty"
    );
    assert!(resp.expires_at.is_some(), "expires_at must be populated");

    room_store::hard_delete(&pool, &room.id).await?;
    state
        .livekit
        .delete_room(&room.livekit_room_name)
        .await
        .ok();
    Ok(())
}

/// Requesting a token for a different identity requires the `moderator` role.
/// A moderator can successfully mint a token for a different identity.
#[tokio::test]
async fn generate_token_moderator_can_mint_for_other_identity() -> TestResult {
    let sess = kratos_session().await;
    let (room, pool, state) = setup_room_with_moderator(&sess.identity_id).await;

    let req = authed_request(
        GenerateTokenRequest {
            room_id: room.id.to_string(),
            participant_identity: "bot-agent".into(),
            participant_name: "Agent".into(),
            role: ParticipantRole::Viewer as i32,
            ttl: None,
        },
        &sess.bearer,
    );
    let resp = auth::generate_token(&state, req).await?.into_inner();
    assert!(
        !resp.token.is_empty(),
        "moderator must get token for other identity"
    );

    room_store::hard_delete(&pool, &room.id).await?;
    state
        .livekit
        .delete_room(&room.livekit_room_name)
        .await
        .ok();
    Ok(())
}

/// Non-participant cannot generate a token.
#[tokio::test]
async fn generate_token_denied_without_participant_role() -> TestResult {
    let owner_sess = kratos_session().await;
    let intruder_sess = kratos_session().await;
    let (room, pool, state) = setup_room_with_moderator(&owner_sess.identity_id).await;

    let req = authed_request(
        GenerateTokenRequest {
            room_id: room.id.to_string(),
            participant_identity: String::new(),
            participant_name: String::new(),
            role: ParticipantRole::Member as i32,
            ttl: None,
        },
        &intruder_sess.bearer,
    );
    let err = auth::generate_token(&state, req)
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

/// Non-moderator cannot mint a token for a different identity.
#[tokio::test]
async fn generate_token_member_cannot_mint_for_other_identity() -> TestResult {
    let owner_sess = kratos_session().await;
    let member_sess = kratos_session().await;
    let (room, pool, state) = setup_room_with_moderator(&owner_sess.identity_id).await;

    // Member only gets participant, not moderator.
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
        GenerateTokenRequest {
            room_id: room.id.to_string(),
            participant_identity: "someone-else".into(),
            participant_name: String::new(),
            role: ParticipantRole::Member as i32,
            ttl: None,
        },
        &member_sess.bearer,
    );
    let err = auth::generate_token(&state, req)
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

/// Room not found returns NotFound.
#[tokio::test]
async fn generate_token_room_not_found() -> TestResult {
    let sess = kratos_session().await;
    let state = shared_state().await;
    let room_id = uuid::Uuid::new_v4().to_string();

    // Grant participant so auth passes.
    state
        .keto
        .grant("room", &room_id, "participant", &sess.identity_id)
        .await?;

    let req = authed_request(
        GenerateTokenRequest {
            room_id,
            participant_identity: String::new(),
            participant_name: String::new(),
            role: ParticipantRole::Member as i32,
            ttl: None,
        },
        &sess.bearer,
    );
    let err = auth::generate_token(&state, req)
        .await
        .expect_err("must fail");
    assert_eq!(err.code(), tonic::Code::NotFound);
    Ok(())
}

/// Invalid room_id UUID returns InvalidArgument.
#[tokio::test]
async fn generate_token_invalid_room_id() -> TestResult {
    let sess = kratos_session().await;
    let state = shared_state().await;

    let req = authed_request(
        GenerateTokenRequest {
            room_id: "not-a-uuid".into(),
            participant_identity: String::new(),
            participant_name: String::new(),
            role: ParticipantRole::Member as i32,
            ttl: None,
        },
        &sess.bearer,
    );
    let err = auth::generate_token(&state, req)
        .await
        .expect_err("must fail");
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    Ok(())
}

// ── InviteParticipant ────────────────────────────────────────────────────

/// A moderator can invite a participant via the LINK method; the response
/// carries a non-empty invite_url and invite_id.
#[tokio::test]
async fn invite_participant_link_method_happy_path() -> TestResult {
    let sess = kratos_session().await;
    let (room, pool, state) = setup_room_with_moderator(&sess.identity_id).await;

    let req = authed_request(
        InviteRequest {
            room_id: room.id.to_string(),
            target: "alice@example.com".into(),
            method: InviteMethod::Link as i32,
            role: ParticipantRole::Member as i32,
            message: String::new(),
        },
        &sess.bearer,
    );
    let resp = participants::invite(&state, req).await?.into_inner();

    assert!(!resp.invite_id.is_empty(), "invite_id must be non-empty");
    assert!(!resp.invite_url.is_empty(), "invite_url must be non-empty");
    assert!(
        resp.invite_url.contains(&room.id.to_string()),
        "invite_url must contain the room id"
    );

    room_store::hard_delete(&pool, &room.id).await?;
    state
        .livekit
        .delete_room(&room.livekit_room_name)
        .await
        .ok();
    Ok(())
}

/// Non-LINK methods return `Unimplemented`.
#[tokio::test]
async fn invite_participant_non_link_method_unimplemented() -> TestResult {
    let sess = kratos_session().await;
    let (room, pool, state) = setup_room_with_moderator(&sess.identity_id).await;

    let req = authed_request(
        InviteRequest {
            room_id: room.id.to_string(),
            target: "bob@example.com".into(),
            method: InviteMethod::Email as i32,
            role: ParticipantRole::Member as i32,
            message: String::new(),
        },
        &sess.bearer,
    );
    let err = participants::invite(&state, req)
        .await
        .expect_err("email method must be unimplemented");
    assert_eq!(err.code(), tonic::Code::Unimplemented);

    room_store::hard_delete(&pool, &room.id).await?;
    state
        .livekit
        .delete_room(&room.livekit_room_name)
        .await
        .ok();
    Ok(())
}

/// Non-moderator cannot invite.
#[tokio::test]
async fn invite_participant_denied_without_moderator_role() -> TestResult {
    let owner_sess = kratos_session().await;
    let member_sess = kratos_session().await;
    let (room, pool, state) = setup_room_with_moderator(&owner_sess.identity_id).await;

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
        InviteRequest {
            room_id: room.id.to_string(),
            target: "carol@example.com".into(),
            method: InviteMethod::Link as i32,
            role: ParticipantRole::Member as i32,
            message: String::new(),
        },
        &member_sess.bearer,
    );
    let err = participants::invite(&state, req)
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

// ── KickParticipant ───────────────────────────────────────────────────────

/// Kicking a participant that is NOT in the LiveKit room still succeeds if
/// the caller is a moderator and the room exists — LiveKit's `remove_participant`
/// is a best-effort call and should not block the response on absence.
///
/// NOTE: LiveKit returns an error when the identity is not in the room. The
/// handler propagates that error. We test the not-found room case instead to
/// stay purely server-side.
#[tokio::test]
async fn kick_participant_room_not_found() -> TestResult {
    let sess = kratos_session().await;
    let state = shared_state().await;
    let room_id = uuid::Uuid::new_v4().to_string();

    state
        .keto
        .grant("room", &room_id, "moderator", &sess.identity_id)
        .await?;

    let req = authed_request(
        KickRequest {
            room_id: room_id.clone(),
            participant_identity: "ghost".into(),
            reason: String::new(),
        },
        &sess.bearer,
    );
    let err = participants::kick(&state, req)
        .await
        .expect_err("must be NotFound");
    assert_eq!(err.code(), tonic::Code::NotFound);
    Ok(())
}

/// Non-moderator cannot kick.
#[tokio::test]
async fn kick_participant_denied_without_moderator_role() -> TestResult {
    let owner_sess = kratos_session().await;
    let member_sess = kratos_session().await;
    let (room, pool, state) = setup_room_with_moderator(&owner_sess.identity_id).await;

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
        KickRequest {
            room_id: room.id.to_string(),
            participant_identity: "someone".into(),
            reason: String::new(),
        },
        &member_sess.bearer,
    );
    let err = participants::kick(&state, req)
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

// ── UpdateParticipantRole ─────────────────────────────────────────────────

/// `update_role` is currently stubbed as `Unimplemented` after the authz grant.
/// Verify the code path: moderator gets `Unimplemented`, non-moderator gets
/// `PermissionDenied` (authz fires before the stub).
#[tokio::test]
async fn update_role_moderator_gets_unimplemented() -> TestResult {
    let sess = kratos_session().await;
    let (room, pool, state) = setup_room_with_moderator(&sess.identity_id).await;

    let req = authed_request(
        UpdateRoleRequest {
            room_id: room.id.to_string(),
            participant_identity: "carol".into(),
            new_role: ParticipantRole::Admin as i32,
        },
        &sess.bearer,
    );
    let err = participants::update_role(&state, req)
        .await
        .expect_err("must be Unimplemented");
    assert_eq!(err.code(), tonic::Code::Unimplemented);

    room_store::hard_delete(&pool, &room.id).await?;
    state
        .livekit
        .delete_room(&room.livekit_room_name)
        .await
        .ok();
    Ok(())
}

#[tokio::test]
async fn update_role_member_gets_permission_denied() -> TestResult {
    let owner_sess = kratos_session().await;
    let member_sess = kratos_session().await;
    let (room, pool, state) = setup_room_with_moderator(&owner_sess.identity_id).await;

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
        UpdateRoleRequest {
            room_id: room.id.to_string(),
            participant_identity: "dave".into(),
            new_role: ParticipantRole::Admin as i32,
        },
        &member_sess.bearer,
    );
    let err = participants::update_role(&state, req)
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

// ── MuteParticipant ───────────────────────────────────────────────────────

/// Muting with no track flags set (mute_audio=false, mute_video=false) skips
/// the LiveKit API call and succeeds immediately — the room must exist.
#[tokio::test]
async fn mute_participant_no_track_flags_succeeds() -> TestResult {
    let sess = kratos_session().await;
    let (room, pool, state) = setup_room_with_moderator(&sess.identity_id).await;

    let req = authed_request(
        MuteParticipantRequest {
            room_id: room.id.to_string(),
            participant_identity: "alice".into(),
            mute_audio: false,
            mute_video: false,
        },
        &sess.bearer,
    );
    // Both flags false → no LiveKit call → should not error.
    participants::mute(&state, req).await?;

    room_store::hard_delete(&pool, &room.id).await?;
    state
        .livekit
        .delete_room(&room.livekit_room_name)
        .await
        .ok();
    Ok(())
}

/// Muting a room that doesn't exist returns NotFound.
#[tokio::test]
async fn mute_participant_room_not_found() -> TestResult {
    let sess = kratos_session().await;
    let state = shared_state().await;
    let room_id = uuid::Uuid::new_v4().to_string();

    state
        .keto
        .grant("room", &room_id, "moderator", &sess.identity_id)
        .await?;

    let req = authed_request(
        MuteParticipantRequest {
            room_id: room_id.clone(),
            participant_identity: "ghost".into(),
            mute_audio: true,
            mute_video: false,
        },
        &sess.bearer,
    );
    let err = participants::mute(&state, req)
        .await
        .expect_err("must be NotFound");
    assert_eq!(err.code(), tonic::Code::NotFound);
    Ok(())
}

/// Non-moderator cannot mute.
#[tokio::test]
async fn mute_participant_denied_without_moderator() -> TestResult {
    let owner_sess = kratos_session().await;
    let member_sess = kratos_session().await;
    let (room, pool, state) = setup_room_with_moderator(&owner_sess.identity_id).await;

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
        MuteParticipantRequest {
            room_id: room.id.to_string(),
            participant_identity: "someone".into(),
            mute_audio: true,
            mute_video: false,
        },
        &member_sess.bearer,
    );
    let err = participants::mute(&state, req)
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
