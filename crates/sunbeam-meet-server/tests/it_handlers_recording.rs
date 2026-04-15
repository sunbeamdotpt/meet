//! Integration tests — recording handler: list, get, delete (soft).
//!
//! `start` and `stop` are tested in `it_recording_egress_s3.rs` against the
//! LiveKit Egress worker. Here we cover the DB-facing read/write RPCs: list,
//! get, and delete, seeding rows directly via SQL to avoid spinning up Egress.
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
    DeleteRecordingRequest, GetRecordingRequest, ListRecordingsRequest, RecordingMode,
    RecordingOutput, RecordingStatus, RoomAccessLevel, VideoQualityPreset,
};
use sunbeam_meet_server::handlers::meet::recording;
use sunbeam_meet_server::storage::pg::{new_id, rooms as room_store};
use uuid::Uuid;

// ── test setup ────────────────────────────────────────────────────────────

async fn setup_room_with_moderator(
    identity_id: &str,
) -> (
    room_store::Room,
    sqlx::PgPool,
    sunbeam_meet_server::state::SharedState,
) {
    let pool = pg_pool().await;
    let state = shared_state().await;
    let slug = unique_room_slug("it-rec-h");

    let room = room_store::create(
        &pool,
        room_store::NewRoom {
            slug,
            display_name: "Recording Handler IT".into(),
            access_level: RoomAccessLevel::Public,
            max_participants: 20,
            default_quality: VideoQualityPreset::Auto,
            waiting_room_enabled: false,
            chat_enabled: true,
            recording_allowed: true,
            created_by: identity_id.to_owned(),
            metadata: BTreeMap::new(),
        },
    )
    .await
    .expect("create room for recording handler test");

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

/// Seed a `recordings` row directly so we don't need a running Egress worker.
async fn seed_recording(pool: &sqlx::PgPool, room_id: Uuid, started_by: &str) -> Uuid {
    let rec_id = new_id();
    let key = format!("recordings/{room_id}/{rec_id}.mp4");
    sqlx::query(
        "INSERT INTO recordings
         (id, room_id, egress_id, mode, output, status, started_by, storage_url, rtmp_url)
         VALUES ($1, $2, $3, 'composite', 'file', 'stopped', $4, $5, '')",
    )
    .bind(rec_id)
    .bind(room_id)
    .bind(format!("egress-{rec_id}"))
    .bind(started_by)
    .bind(&key)
    .execute(pool)
    .await
    .expect("seed recording row");
    rec_id
}

// ── list_recordings ───────────────────────────────────────────────────────

/// list_recordings returns the seeded row and fields are populated correctly.
#[tokio::test]
async fn list_recordings_returns_seeded_rows() -> TestResult {
    let sess = kratos_session().await;
    let (room, pool, state) = setup_room_with_moderator(&sess.identity_id).await;

    let rec_id = seed_recording(&pool, room.id, &sess.identity_id).await;

    let req = authed_request(
        ListRecordingsRequest {
            room_id: room.id.to_string(),
            page_size: 10,
            page_token: String::new(),
        },
        &sess.bearer,
    );
    let resp = recording::list(&state, req).await?.into_inner();

    let found = resp
        .recordings
        .iter()
        .find(|r| r.id == rec_id.to_string())
        .expect("seeded recording must appear in list");

    assert_eq!(found.room_id, room.id.to_string());
    assert_eq!(found.started_by, sess.identity_id);
    assert_eq!(found.mode, RecordingMode::Composite as i32);
    assert_eq!(found.output, RecordingOutput::File as i32);
    assert_eq!(found.status, RecordingStatus::Stopped as i32);
    assert!(!found.storage_path.is_empty(), "storage_path must be set");

    room_store::hard_delete(&pool, &room.id).await?;
    Ok(())
}

/// list_recordings with page_size=0 uses the default (50) and does not error.
#[tokio::test]
async fn list_recordings_zero_page_size_defaults() -> TestResult {
    let sess = kratos_session().await;
    let (room, pool, state) = setup_room_with_moderator(&sess.identity_id).await;

    let req = authed_request(
        ListRecordingsRequest {
            room_id: room.id.to_string(),
            page_size: 0,
            page_token: String::new(),
        },
        &sess.bearer,
    );
    recording::list(&state, req).await?;

    room_store::hard_delete(&pool, &room.id).await?;
    Ok(())
}

/// Non-participant cannot list recordings.
#[tokio::test]
async fn list_recordings_denied_without_participant_role() -> TestResult {
    let owner_sess = kratos_session().await;
    let intruder_sess = kratos_session().await;
    let (room, pool, state) = setup_room_with_moderator(&owner_sess.identity_id).await;

    let req = authed_request(
        ListRecordingsRequest {
            room_id: room.id.to_string(),
            page_size: 10,
            page_token: String::new(),
        },
        &intruder_sess.bearer,
    );
    let err = recording::list(&state, req)
        .await
        .expect_err("must be denied");
    assert_eq!(err.code(), tonic::Code::PermissionDenied);

    room_store::hard_delete(&pool, &room.id).await?;
    Ok(())
}

/// Invalid room_id returns InvalidArgument.
#[tokio::test]
async fn list_recordings_invalid_room_id() -> TestResult {
    let sess = kratos_session().await;
    let state = shared_state().await;

    let req = authed_request(
        ListRecordingsRequest {
            room_id: "bad-id".into(),
            page_size: 10,
            page_token: String::new(),
        },
        &sess.bearer,
    );
    let err = recording::list(&state, req).await.expect_err("must fail");
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    Ok(())
}

// ── get_recording ─────────────────────────────────────────────────────────

/// get_recording returns the correct row for a known recording_id.
#[tokio::test]
async fn get_recording_happy_path() -> TestResult {
    let sess = kratos_session().await;
    let (room, pool, state) = setup_room_with_moderator(&sess.identity_id).await;

    let rec_id = seed_recording(&pool, room.id, &sess.identity_id).await;

    let req = authed_request(
        GetRecordingRequest {
            recording_id: rec_id.to_string(),
        },
        &sess.bearer,
    );
    let rec = recording::get(&state, req).await?.into_inner();

    assert_eq!(rec.id, rec_id.to_string());
    assert_eq!(rec.room_id, room.id.to_string());
    assert_eq!(rec.status, RecordingStatus::Stopped as i32);

    room_store::hard_delete(&pool, &room.id).await?;
    Ok(())
}

/// get_recording with a nonexistent recording_id returns NotFound.
#[tokio::test]
async fn get_recording_not_found() -> TestResult {
    let sess = kratos_session().await;
    let state = shared_state().await;
    let room_id = uuid::Uuid::new_v4().to_string();
    // Grant participant so auth passes.
    state
        .keto
        .grant("room", &room_id, "participant", &sess.identity_id)
        .await?;

    let req = authed_request(
        GetRecordingRequest {
            recording_id: uuid::Uuid::new_v4().to_string(),
        },
        &sess.bearer,
    );
    let err = recording::get(&state, req)
        .await
        .expect_err("must be NotFound");
    assert_eq!(err.code(), tonic::Code::NotFound);
    Ok(())
}

/// get_recording with an invalid UUID returns InvalidArgument.
#[tokio::test]
async fn get_recording_invalid_id() -> TestResult {
    let sess = kratos_session().await;
    let state = shared_state().await;

    let req = authed_request(
        GetRecordingRequest {
            recording_id: "not-uuid".into(),
        },
        &sess.bearer,
    );
    let err = recording::get(&state, req).await.expect_err("must fail");
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    Ok(())
}

// ── delete_recording ──────────────────────────────────────────────────────

/// A moderator can soft-delete a recording. After deletion the recording no
/// longer appears in list_recordings (deleted_at filter).
#[tokio::test]
async fn delete_recording_soft_deletes() -> TestResult {
    let sess = kratos_session().await;
    let (room, pool, state) = setup_room_with_moderator(&sess.identity_id).await;

    let rec_id = seed_recording(&pool, room.id, &sess.identity_id).await;

    // Delete.
    let del_req = authed_request(
        DeleteRecordingRequest {
            recording_id: rec_id.to_string(),
        },
        &sess.bearer,
    );
    recording::delete(&state, del_req).await?;

    // Must no longer appear in list.
    let list_req = authed_request(
        ListRecordingsRequest {
            room_id: room.id.to_string(),
            page_size: 100,
            page_token: String::new(),
        },
        &sess.bearer,
    );
    let resp = recording::list(&state, list_req).await?.into_inner();
    assert!(
        !resp.recordings.iter().any(|r| r.id == rec_id.to_string()),
        "deleted recording must not appear in list"
    );

    room_store::hard_delete(&pool, &room.id).await?;
    Ok(())
}

/// Non-moderator cannot delete a recording.
#[tokio::test]
async fn delete_recording_denied_without_moderator() -> TestResult {
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

    let rec_id = seed_recording(&pool, room.id, &owner_sess.identity_id).await;

    let req = authed_request(
        DeleteRecordingRequest {
            recording_id: rec_id.to_string(),
        },
        &member_sess.bearer,
    );
    let err = recording::delete(&state, req)
        .await
        .expect_err("must be denied");
    assert_eq!(err.code(), tonic::Code::PermissionDenied);

    room_store::hard_delete(&pool, &room.id).await?;
    Ok(())
}

/// Deleting a nonexistent recording_id returns NotFound.
#[tokio::test]
async fn delete_recording_not_found() -> TestResult {
    let sess = kratos_session().await;
    let state = shared_state().await;

    let req = authed_request(
        DeleteRecordingRequest {
            recording_id: uuid::Uuid::new_v4().to_string(),
        },
        &sess.bearer,
    );
    let err = recording::delete(&state, req)
        .await
        .expect_err("must be NotFound");
    assert_eq!(err.code(), tonic::Code::NotFound);
    Ok(())
}
