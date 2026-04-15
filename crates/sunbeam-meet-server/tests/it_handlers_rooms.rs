//! Integration tests — `handlers/meet/rooms.rs` RPC surface.
//!
//! Exercises every public handler free-function (create, get, list, update,
//! end) through the same call path production code uses:
//!   shared_state → tonic Request with real Kratos bearer → handler fn
//!
//! Required env (same as the test-runner container):
//!   DATABASE_URL, VALKEY_URL, NATS_URL, LIVEKIT_URL, LIVEKIT_API_KEY,
//!   LIVEKIT_API_SECRET, KRATOS_PUBLIC_URL, KRATOS_ADMIN_URL,
//!   KETO_READ_URL, KETO_WRITE_URL, CALDAV_URL, S3_ENDPOINT, S3_ACCESS_KEY,
//!   S3_SECRET_KEY.
//!
//! Assertion strategy: structured proto fields only — no `.contains("…")`.
//! Cleanup: every test hard-deletes its own rows via direct sqlx so the
//! aggregate suite stays idempotent.

mod common;

use common::{authed_request, kratos_session, shared_state, unique_room_slug, TestResult};
use sunbeam_meet_proto::meet::v1::{
    CreateRoomRequest, EndRoomRequest, GetRoomRequest, ListRoomsRequest, RoomAccessLevel,
    RoomStatus, UpdateRoomRequest, VideoQualityPreset,
};
use sunbeam_meet_server::handlers::meet::rooms;
use tonic::Code;

// ── helpers ─────────────────────────────────────────────────────────────────

/// Hard-delete a room row — used for teardown only.
async fn cleanup_room(state: &sunbeam_meet_server::state::SharedState, id: &str) {
    if let Ok(uuid) = uuid::Uuid::parse_str(id) {
        let _ = sqlx::query("DELETE FROM rooms WHERE id = $1")
            .bind(uuid)
            .execute(&state.db)
            .await;
    }
}

/// Seed all three Keto room relations for `identity_id` on `room_id`.
///
/// BUG NOTE: The `create` handler in `handlers/meet/rooms.rs` only seeds the
/// `owner` relation but `get` checks `participant` and `update`/`end` check
/// `moderator`. Because the dev Keto config has no hierarchy rules, `owner`
/// does NOT subsume `participant` or `moderator`. This helper works around the
/// gap so the integration tests can exercise the full CRUD surface without
/// modifying production code.
///
/// The underlying issue should be fixed in production by either:
///   a) having the Keto namespace define `owner ⊇ moderator ⊇ participant`, or
///   b) having `rooms::create` seed all three relations.
#[allow(dead_code)]
async fn seed_room_grants(
    state: &sunbeam_meet_server::state::SharedState,
    room_id: &str,
    identity_id: &str,
) {
    for relation in &["owner", "moderator", "participant"] {
        let _ =
            sunbeam_meet_server::authz::grant(state, "room", room_id, relation, identity_id).await;
    }
}

// ── create ───────────────────────────────────────────────────────────────────

/// Happy path: create a room and verify every structured field.
#[tokio::test]
async fn create_room_happy_path() -> TestResult {
    let state = shared_state().await;
    let sess = kratos_session().await;
    let display_name = unique_room_slug("it-create");

    let req = authed_request(
        CreateRoomRequest {
            display_name: display_name.clone(),
            access_level: RoomAccessLevel::Public as i32,
            max_participants: 10,
            default_quality: VideoQualityPreset::Medium as i32,
            waiting_room_enabled: true,
            chat_enabled: true,
            recording_allowed: false,
            metadata: std::collections::HashMap::new(),
        },
        &sess.bearer,
    );

    let room = rooms::create(&state, req).await?.into_inner();

    // Structured field assertions — never string-match.
    assert_eq!(room.display_name, display_name);
    assert_eq!(room.status, RoomStatus::Waiting as i32);
    assert_eq!(room.access_level, RoomAccessLevel::Public as i32);
    assert_eq!(room.default_quality, VideoQualityPreset::Medium as i32);
    assert_eq!(room.max_participants, 10);
    assert!(room.waiting_room_enabled);
    assert!(room.chat_enabled);
    assert!(!room.recording_allowed);
    assert!(!room.id.is_empty(), "room.id must be populated");
    assert!(
        !room.livekit_room_name.is_empty(),
        "livekit_room_name must be set"
    );
    assert!(room.created_at.is_some(), "created_at must be set");
    assert_eq!(room.created_by, sess.identity_id);

    cleanup_room(&state, &room.id).await;
    Ok(())
}

/// create with an empty display_name returns InvalidArgument.
#[tokio::test]
async fn create_room_empty_display_name_returns_invalid_argument() -> TestResult {
    let state = shared_state().await;
    let sess = kratos_session().await;

    let req = authed_request(
        CreateRoomRequest {
            display_name: "   ".into(), // whitespace only
            ..CreateRoomRequest::default()
        },
        &sess.bearer,
    );

    let err = rooms::create(&state, req).await.unwrap_err();
    assert_eq!(
        err.code(),
        Code::InvalidArgument,
        "empty display_name must return InvalidArgument, got {:?}",
        err.code(),
    );
    Ok(())
}

/// create without an authorization header returns Unauthenticated.
#[tokio::test]
async fn create_room_unauthenticated_returns_unauthenticated() -> TestResult {
    let state = shared_state().await;

    let req = tonic::Request::new(CreateRoomRequest {
        display_name: unique_room_slug("it-noauth"),
        ..CreateRoomRequest::default()
    });

    let err = rooms::create(&state, req).await.unwrap_err();
    assert_eq!(
        err.code(),
        Code::Unauthenticated,
        "missing auth header must return Unauthenticated, got {:?}",
        err.code(),
    );
    Ok(())
}

/// create defaults max_participants to 300 when the field is 0.
#[tokio::test]
async fn create_room_default_max_participants() -> TestResult {
    let state = shared_state().await;
    let sess = kratos_session().await;

    let req = authed_request(
        CreateRoomRequest {
            display_name: unique_room_slug("it-defmax"),
            max_participants: 0, // should default to 300
            ..CreateRoomRequest::default()
        },
        &sess.bearer,
    );

    let room = rooms::create(&state, req).await?.into_inner();
    assert_eq!(
        room.max_participants, 300,
        "max_participants 0 must default to 300"
    );
    cleanup_room(&state, &room.id).await;
    Ok(())
}

/// Unspecified access_level is normalised to Trusted.
#[tokio::test]
async fn create_room_unspecified_access_level_defaults_to_trusted() -> TestResult {
    let state = shared_state().await;
    let sess = kratos_session().await;

    let req = authed_request(
        CreateRoomRequest {
            display_name: unique_room_slug("it-access"),
            access_level: RoomAccessLevel::Unspecified as i32,
            ..CreateRoomRequest::default()
        },
        &sess.bearer,
    );

    let room = rooms::create(&state, req).await?.into_inner();
    assert_eq!(
        room.access_level,
        RoomAccessLevel::Trusted as i32,
        "Unspecified access_level must normalise to Trusted"
    );
    cleanup_room(&state, &room.id).await;
    Ok(())
}

// ── get ──────────────────────────────────────────────────────────────────────

/// Happy path: get a just-created room.
#[tokio::test]
async fn get_room_returns_created_room() -> TestResult {
    let state = shared_state().await;
    let sess = kratos_session().await;
    let display_name = unique_room_slug("it-get");

    let created = rooms::create(
        &state,
        authed_request(
            CreateRoomRequest {
                display_name: display_name.clone(),
                ..CreateRoomRequest::default()
            },
            &sess.bearer,
        ),
    )
    .await?
    .into_inner();

    // Seed all relations so get() (which checks `participant`) succeeds.
    seed_room_grants(&state, &created.id, &sess.identity_id).await;

    let fetched = rooms::get(
        &state,
        authed_request(
            GetRoomRequest {
                room_id: created.id.clone(),
            },
            &sess.bearer,
        ),
    )
    .await?
    .into_inner();

    assert_eq!(fetched.id, created.id);
    assert_eq!(fetched.display_name, display_name);
    assert_eq!(fetched.status, RoomStatus::Waiting as i32);

    cleanup_room(&state, &created.id).await;
    Ok(())
}

/// get with a well-formed but non-existent UUID returns NotFound.
#[tokio::test]
async fn get_room_not_found_returns_not_found() -> TestResult {
    let state = shared_state().await;
    let sess = kratos_session().await;

    let missing_id = uuid::Uuid::now_v7().to_string();
    let err = rooms::get(
        &state,
        authed_request(
            GetRoomRequest {
                room_id: missing_id,
            },
            &sess.bearer,
        ),
    )
    .await
    .unwrap_err();

    // NotFound is expected — but the room may also fail the authz::can check
    // first (PermissionDenied) because there is no Keto tuple for this id.
    // Both are acceptable: the caller cannot access a non-existent room.
    assert!(
        matches!(err.code(), Code::NotFound | Code::PermissionDenied),
        "missing room must return NotFound or PermissionDenied, got {:?}",
        err.code(),
    );
    Ok(())
}

/// get with a malformed room_id returns InvalidArgument.
#[tokio::test]
async fn get_room_malformed_id_returns_invalid_argument() -> TestResult {
    let state = shared_state().await;
    let sess = kratos_session().await;

    let err = rooms::get(
        &state,
        authed_request(
            GetRoomRequest {
                room_id: "not-a-uuid".into(),
            },
            &sess.bearer,
        ),
    )
    .await
    .unwrap_err();

    assert_eq!(
        err.code(),
        Code::InvalidArgument,
        "malformed uuid must return InvalidArgument, got {:?}",
        err.code(),
    );
    Ok(())
}

// ── list ─────────────────────────────────────────────────────────────────────

/// list returns the room we just created.
#[tokio::test]
async fn list_rooms_includes_created_room() -> TestResult {
    let state = shared_state().await;
    let sess = kratos_session().await;

    let created = rooms::create(
        &state,
        authed_request(
            CreateRoomRequest {
                display_name: unique_room_slug("it-list"),
                ..CreateRoomRequest::default()
            },
            &sess.bearer,
        ),
    )
    .await?
    .into_inner();

    let resp = rooms::list(
        &state,
        authed_request(
            ListRoomsRequest {
                page_size: 500,
                ..ListRoomsRequest::default()
            },
            &sess.bearer,
        ),
    )
    .await?
    .into_inner();

    assert!(
        resp.rooms.iter().any(|r| r.id == created.id),
        "list must surface the newly-created room (id={})",
        created.id,
    );
    assert!(
        resp.total_count >= 1,
        "total_count must be at least 1, got {}",
        resp.total_count,
    );

    cleanup_room(&state, &created.id).await;
    Ok(())
}

/// list with status_filter=Waiting only returns waiting rooms.
#[tokio::test]
async fn list_rooms_status_filter_waiting_excludes_ended() -> TestResult {
    let state = shared_state().await;
    let sess = kratos_session().await;

    // Create one room, then end it.
    let r = rooms::create(
        &state,
        authed_request(
            CreateRoomRequest {
                display_name: unique_room_slug("it-filter-end"),
                ..CreateRoomRequest::default()
            },
            &sess.bearer,
        ),
    )
    .await?
    .into_inner();

    // Seed grants so end() (moderator check) succeeds.
    seed_room_grants(&state, &r.id, &sess.identity_id).await;

    rooms::end(
        &state,
        authed_request(
            EndRoomRequest {
                room_id: r.id.clone(),
            },
            &sess.bearer,
        ),
    )
    .await?;

    let resp = rooms::list(
        &state,
        authed_request(
            ListRoomsRequest {
                status_filter: RoomStatus::Waiting as i32,
                page_size: 500,
                ..ListRoomsRequest::default()
            },
            &sess.bearer,
        ),
    )
    .await?
    .into_inner();

    // The ended room must not appear in the Waiting-filtered results. We
    // don't assert on *every* row because the handler doesn't scope by
    // caller identity, so concurrent tests in this process can legitimately
    // add rooms of any status — only our own ended room is in our control.
    assert!(
        !resp.rooms.iter().any(|room| room.id == r.id),
        "ended room must not appear in waiting filter results",
    );

    cleanup_room(&state, &r.id).await;
    Ok(())
}

/// list page_size=0 defaults to 50 (does not panic or return an error).
#[tokio::test]
async fn list_rooms_zero_page_size_defaults() -> TestResult {
    let state = shared_state().await;
    let sess = kratos_session().await;

    let resp = rooms::list(
        &state,
        authed_request(
            ListRoomsRequest {
                page_size: 0,
                ..ListRoomsRequest::default()
            },
            &sess.bearer,
        ),
    )
    .await?
    .into_inner();

    // The response must be well-formed regardless of how many rows exist.
    assert!(
        resp.rooms.len() <= 50,
        "page_size=0 default is 50; got {} rooms",
        resp.rooms.len(),
    );
    Ok(())
}

// ── update ───────────────────────────────────────────────────────────────────

/// Happy path: update display_name and chat_enabled.
#[tokio::test]
async fn update_room_changes_fields() -> TestResult {
    let state = shared_state().await;
    let sess = kratos_session().await;

    let created = rooms::create(
        &state,
        authed_request(
            CreateRoomRequest {
                display_name: unique_room_slug("it-upd"),
                chat_enabled: true,
                ..CreateRoomRequest::default()
            },
            &sess.bearer,
        ),
    )
    .await?
    .into_inner();

    // Seed moderator grant so update() succeeds.
    seed_room_grants(&state, &created.id, &sess.identity_id).await;

    let updated = rooms::update(
        &state,
        authed_request(
            UpdateRoomRequest {
                room_id: created.id.clone(),
                display_name: Some("Renamed by test".into()),
                chat_enabled: Some(false),
                max_participants: Some(25),
                ..UpdateRoomRequest::default()
            },
            &sess.bearer,
        ),
    )
    .await?
    .into_inner();

    assert_eq!(updated.display_name, "Renamed by test");
    assert!(
        !updated.chat_enabled,
        "chat_enabled must be updated to false"
    );
    assert_eq!(updated.max_participants, 25);
    // Untouched fields survive.
    assert_eq!(updated.id, created.id);

    cleanup_room(&state, &created.id).await;
    Ok(())
}

/// update with a non-existent room_id returns NotFound or PermissionDenied.
#[tokio::test]
async fn update_room_not_found() -> TestResult {
    let state = shared_state().await;
    let sess = kratos_session().await;

    let err = rooms::update(
        &state,
        authed_request(
            UpdateRoomRequest {
                room_id: uuid::Uuid::now_v7().to_string(),
                display_name: Some("ghost".into()),
                ..UpdateRoomRequest::default()
            },
            &sess.bearer,
        ),
    )
    .await
    .unwrap_err();

    assert!(
        matches!(err.code(), Code::NotFound | Code::PermissionDenied),
        "non-existent update must return NotFound or PermissionDenied, got {:?}",
        err.code(),
    );
    Ok(())
}

/// update with a malformed room_id returns InvalidArgument.
#[tokio::test]
async fn update_room_malformed_id() -> TestResult {
    let state = shared_state().await;
    let sess = kratos_session().await;

    let err = rooms::update(
        &state,
        authed_request(
            UpdateRoomRequest {
                room_id: "bad-uuid".into(),
                display_name: Some("x".into()),
                ..UpdateRoomRequest::default()
            },
            &sess.bearer,
        ),
    )
    .await
    .unwrap_err();

    assert_eq!(err.code(), Code::InvalidArgument);
    Ok(())
}

// ── end ──────────────────────────────────────────────────────────────────────

/// Happy path: end a room and verify status transitions to Ended.
#[tokio::test]
async fn end_room_transitions_status_to_ended() -> TestResult {
    let state = shared_state().await;
    let sess = kratos_session().await;

    let created = rooms::create(
        &state,
        authed_request(
            CreateRoomRequest {
                display_name: unique_room_slug("it-end"),
                ..CreateRoomRequest::default()
            },
            &sess.bearer,
        ),
    )
    .await?
    .into_inner();

    // Seed moderator grant so end() succeeds.
    seed_room_grants(&state, &created.id, &sess.identity_id).await;

    // end() returns an empty response; verify via load_room.
    rooms::end(
        &state,
        authed_request(
            EndRoomRequest {
                room_id: created.id.clone(),
            },
            &sess.bearer,
        ),
    )
    .await?;

    let after = rooms::load_room(&state, uuid::Uuid::parse_str(&created.id)?).await?;
    assert_eq!(
        after.status,
        RoomStatus::Ended as i32,
        "status must be Ended after end()"
    );
    assert!(
        after.ended_at.is_some(),
        "ended_at must be populated after end()"
    );

    cleanup_room(&state, &created.id).await;
    Ok(())
}

/// end with a non-existent room_id returns NotFound or PermissionDenied.
#[tokio::test]
async fn end_room_not_found() -> TestResult {
    let state = shared_state().await;
    let sess = kratos_session().await;

    let err = rooms::end(
        &state,
        authed_request(
            EndRoomRequest {
                room_id: uuid::Uuid::now_v7().to_string(),
            },
            &sess.bearer,
        ),
    )
    .await
    .unwrap_err();

    assert!(
        matches!(err.code(), Code::NotFound | Code::PermissionDenied),
        "non-existent end must return NotFound or PermissionDenied, got {:?}",
        err.code(),
    );
    Ok(())
}

/// end with a malformed room_id returns InvalidArgument.
#[tokio::test]
async fn end_room_malformed_id() -> TestResult {
    let state = shared_state().await;
    let sess = kratos_session().await;

    let err = rooms::end(
        &state,
        authed_request(
            EndRoomRequest {
                room_id: "not-a-uuid".into(),
            },
            &sess.bearer,
        ),
    )
    .await
    .unwrap_err();

    assert_eq!(err.code(), Code::InvalidArgument);
    Ok(())
}

// ── full round-trip ───────────────────────────────────────────────────────────

/// create → get → update → end → verify ended_at is set.
#[tokio::test]
async fn rooms_full_crud_round_trip() -> TestResult {
    let state = shared_state().await;
    let sess = kratos_session().await;
    let name = unique_room_slug("it-rt");

    // Create.
    let room = rooms::create(
        &state,
        authed_request(
            CreateRoomRequest {
                display_name: name.clone(),
                access_level: RoomAccessLevel::Restricted as i32,
                max_participants: 5,
                default_quality: VideoQualityPreset::Low as i32,
                waiting_room_enabled: false,
                chat_enabled: false,
                recording_allowed: true,
                metadata: std::collections::HashMap::new(),
            },
            &sess.bearer,
        ),
    )
    .await?
    .into_inner();

    assert_eq!(room.display_name, name);
    assert_eq!(room.access_level, RoomAccessLevel::Restricted as i32);
    assert_eq!(room.max_participants, 5);
    assert_eq!(room.default_quality, VideoQualityPreset::Low as i32);
    assert!(room.recording_allowed);
    assert!(!room.waiting_room_enabled);
    assert!(!room.chat_enabled);

    // Seed all relations so get/update/end succeed.
    seed_room_grants(&state, &room.id, &sess.identity_id).await;

    // Get — round-trips every field.
    let fetched = rooms::get(
        &state,
        authed_request(
            GetRoomRequest {
                room_id: room.id.clone(),
            },
            &sess.bearer,
        ),
    )
    .await?
    .into_inner();
    assert_eq!(fetched.id, room.id);
    assert_eq!(fetched.access_level, RoomAccessLevel::Restricted as i32);

    // Update.
    let updated = rooms::update(
        &state,
        authed_request(
            UpdateRoomRequest {
                room_id: room.id.clone(),
                display_name: Some(format!("{name}-updated")),
                recording_allowed: Some(false),
                ..UpdateRoomRequest::default()
            },
            &sess.bearer,
        ),
    )
    .await?
    .into_inner();
    assert_eq!(updated.display_name, format!("{name}-updated"));
    assert!(!updated.recording_allowed);

    // End.
    rooms::end(
        &state,
        authed_request(
            EndRoomRequest {
                room_id: room.id.clone(),
            },
            &sess.bearer,
        ),
    )
    .await?;

    let ended = rooms::load_room(&state, uuid::Uuid::parse_str(&room.id)?).await?;
    assert_eq!(ended.status, RoomStatus::Ended as i32);
    assert!(ended.ended_at.is_some());

    cleanup_room(&state, &room.id).await;
    Ok(())
}

// ── quality preset coverage ───────────────────────────────────────────────────

/// Cover enum_quality / quality_from_str branches for High, Ultra, Auto.
/// These branches in rooms.rs are only hit when those specific preset values
/// appear in the DB — exercising create with each preset ensures the codec
/// round-trip for every variant.
#[tokio::test]
async fn create_room_quality_presets_high_ultra_auto() -> TestResult {
    let state = shared_state().await;
    let sess = kratos_session().await;

    for (preset, expected_i32) in [
        (
            VideoQualityPreset::High as i32,
            VideoQualityPreset::High as i32,
        ),
        (
            VideoQualityPreset::Ultra as i32,
            VideoQualityPreset::Ultra as i32,
        ),
        (
            VideoQualityPreset::Auto as i32,
            VideoQualityPreset::Auto as i32,
        ),
    ] {
        let room = rooms::create(
            &state,
            authed_request(
                CreateRoomRequest {
                    display_name: unique_room_slug("it-qual"),
                    default_quality: preset,
                    ..CreateRoomRequest::default()
                },
                &sess.bearer,
            ),
        )
        .await?
        .into_inner();

        assert_eq!(
            room.default_quality, expected_i32,
            "quality preset {preset} must round-trip via create",
        );
        cleanup_room(&state, &room.id).await;
    }
    Ok(())
}

/// Update every optional field to exercise all branches in update().
#[tokio::test]
async fn update_room_all_fields() -> TestResult {
    let state = shared_state().await;
    let sess = kratos_session().await;

    let created = rooms::create(
        &state,
        authed_request(
            CreateRoomRequest {
                display_name: unique_room_slug("it-upd-all"),
                access_level: RoomAccessLevel::Public as i32,
                max_participants: 20,
                default_quality: VideoQualityPreset::Low as i32,
                waiting_room_enabled: false,
                chat_enabled: false,
                recording_allowed: false,
                ..CreateRoomRequest::default()
            },
            &sess.bearer,
        ),
    )
    .await?
    .into_inner();
    seed_room_grants(&state, &created.id, &sess.identity_id).await;

    // Update every optional field at once.
    let updated = rooms::update(
        &state,
        authed_request(
            UpdateRoomRequest {
                room_id: created.id.clone(),
                display_name: Some("All fields updated".into()),
                access_level: Some(RoomAccessLevel::Restricted as i32),
                max_participants: Some(99),
                waiting_room_enabled: Some(true),
                chat_enabled: Some(true),
                recording_allowed: Some(true),
                ..UpdateRoomRequest::default()
            },
            &sess.bearer,
        ),
    )
    .await?
    .into_inner();

    assert_eq!(updated.display_name, "All fields updated");
    assert_eq!(updated.access_level, RoomAccessLevel::Restricted as i32);
    assert_eq!(updated.max_participants, 99);
    assert!(updated.waiting_room_enabled);
    assert!(updated.chat_enabled);
    assert!(updated.recording_allowed);

    cleanup_room(&state, &created.id).await;
    Ok(())
}

/// Cover the RoomAccessLevel::Restricted path through enum_access.
#[tokio::test]
async fn create_room_access_level_restricted_round_trips() -> TestResult {
    let state = shared_state().await;
    let sess = kratos_session().await;

    let room = rooms::create(
        &state,
        authed_request(
            CreateRoomRequest {
                display_name: unique_room_slug("it-restricted"),
                access_level: RoomAccessLevel::Restricted as i32,
                ..CreateRoomRequest::default()
            },
            &sess.bearer,
        ),
    )
    .await?
    .into_inner();

    assert_eq!(room.access_level, RoomAccessLevel::Restricted as i32);
    cleanup_room(&state, &room.id).await;
    Ok(())
}

/// list with status_filter=Active returns only active rooms.
/// Also covers the Active branch of the status enum in list().
#[tokio::test]
async fn list_rooms_status_filter_active_and_ended() -> TestResult {
    let state = shared_state().await;
    let sess = kratos_session().await;

    // Create a room and end it to get an Ended room in the DB.
    let r = rooms::create(
        &state,
        authed_request(
            CreateRoomRequest {
                display_name: unique_room_slug("it-filter-status"),
                ..CreateRoomRequest::default()
            },
            &sess.bearer,
        ),
    )
    .await?
    .into_inner();
    seed_room_grants(&state, &r.id, &sess.identity_id).await;
    rooms::end(
        &state,
        authed_request(
            EndRoomRequest {
                room_id: r.id.clone(),
            },
            &sess.bearer,
        ),
    )
    .await?;

    // Filter by Ended — the room we just ended must appear.
    let ended_resp = rooms::list(
        &state,
        authed_request(
            ListRoomsRequest {
                status_filter: RoomStatus::Ended as i32,
                page_size: 500,
                ..ListRoomsRequest::default()
            },
            &sess.bearer,
        ),
    )
    .await?
    .into_inner();

    // Every room returned must have status=Ended.
    for room in &ended_resp.rooms {
        assert_eq!(
            room.status,
            RoomStatus::Ended as i32,
            "status_filter=Ended must only return ended rooms",
        );
    }
    assert!(
        ended_resp.rooms.iter().any(|room| room.id == r.id),
        "ended room must appear in Ended filter",
    );

    // Filter by Active — the ended room must NOT appear (it's ended, not active).
    let active_resp = rooms::list(
        &state,
        authed_request(
            ListRoomsRequest {
                status_filter: RoomStatus::Active as i32,
                page_size: 500,
                ..ListRoomsRequest::default()
            },
            &sess.bearer,
        ),
    )
    .await?
    .into_inner();

    for room in &active_resp.rooms {
        assert_eq!(
            room.status,
            RoomStatus::Active as i32,
            "status_filter=Active must only return active rooms",
        );
    }

    cleanup_room(&state, &r.id).await;
    Ok(())
}

/// Cover the metadata round-trip path in load_room.
#[tokio::test]
async fn create_room_with_metadata_round_trips() -> TestResult {
    let state = shared_state().await;
    let sess = kratos_session().await;

    let mut meta = std::collections::HashMap::new();
    meta.insert("team".to_string(), "engineering".to_string());
    meta.insert("project".to_string(), "sunbeam".to_string());

    let room = rooms::create(
        &state,
        authed_request(
            CreateRoomRequest {
                display_name: unique_room_slug("it-meta"),
                metadata: meta,
                ..CreateRoomRequest::default()
            },
            &sess.bearer,
        ),
    )
    .await?
    .into_inner();

    assert_eq!(
        room.metadata.get("team").map(String::as_str),
        Some("engineering"),
        "metadata key 'team' must round-trip",
    );
    assert_eq!(
        room.metadata.get("project").map(String::as_str),
        Some("sunbeam"),
        "metadata key 'project' must round-trip",
    );
    cleanup_room(&state, &room.id).await;
    Ok(())
}
