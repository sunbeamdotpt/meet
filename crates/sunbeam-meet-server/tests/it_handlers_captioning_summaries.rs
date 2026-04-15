//! Integration tests — captioning handlers (start, stop) and summaries
//! handlers (get, list).
//!
//! Captioning requires an agent worker (`WhisperStt`). In the integration
//! compose stack no live agent worker is configured, so `start` will always
//! return `Unimplemented` (the `agents.start_job` call returns an error,
//! triggering the `_ => Err(Status::unimplemented(...))` arm). We test:
//!   - that the auth/authz gate fires before the agent attempt
//!   - that start returns `Unimplemented` when there are no workers
//!   - that stop soft-transitions any existing captioning_state row
//!
//! Summaries are tested by seeding rows directly (no AI summarizer needed).
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
    CaptioningState, GetMeetingSummaryRequest, ListMeetingSummariesRequest, RoomAccessLevel,
    StartCaptioningRequest, StopCaptioningRequest, VideoQualityPreset,
};
use sunbeam_meet_server::handlers::meet::{captioning, summaries};
use sunbeam_meet_server::storage::pg::{new_id, rooms as room_store};
use uuid::Uuid;

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
    let slug = unique_room_slug("it-cap-sum");

    let room = room_store::create(
        &pool,
        room_store::NewRoom {
            slug,
            display_name: "Captioning/Summaries IT".into(),
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
    .expect("create room for captioning/summaries test");

    // Create the LiveKit room so the captioning token mint finds a room name.
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

/// Seed a summary row for testing.
async fn seed_summary(pool: &sqlx::PgPool, room_id: Uuid, identity_id: &str) -> Uuid {
    let sum_id = new_id();
    sqlx::query(
        "INSERT INTO summaries
         (id, room_id, transcript_ref, summary_md, action_items, meeting_duration_ms, attendees, generated_at)
         VALUES ($1, $2, 'ref-transcript', 'Summary text.', '[]'::jsonb, 60000, ARRAY[$3::text], NOW())",
    )
    .bind(sum_id)
    .bind(room_id)
    .bind(identity_id)
    .execute(pool)
    .await
    .expect("seed summary row");
    sum_id
}

// ── Captioning ────────────────────────────────────────────────────────────

/// start_captioning without configured agent workers returns `Unimplemented`.
/// This also exercises the LiveKit token-mint path and the Postgres room lookup.
#[tokio::test]
async fn start_captioning_no_workers_returns_unimplemented() -> TestResult {
    let sess = kratos_session().await;
    let (room, pool, state) = setup_room_with_moderator(&sess.identity_id).await;

    let req = authed_request(
        StartCaptioningRequest {
            room_id: room.id.to_string(),
            language: "en".into(),
        },
        &sess.bearer,
    );
    let err = captioning::start(&state, req)
        .await
        .expect_err("must be Unimplemented without workers");
    assert_eq!(err.code(), tonic::Code::Unimplemented);

    room_store::hard_delete(&pool, &room.id).await?;
    state
        .livekit
        .delete_room(&room.livekit_room_name)
        .await
        .ok();
    Ok(())
}

/// Non-moderator is denied before the agent attempt.
#[tokio::test]
async fn start_captioning_denied_without_moderator_role() -> TestResult {
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
        StartCaptioningRequest {
            room_id: room.id.to_string(),
            language: "en".into(),
        },
        &member_sess.bearer,
    );
    let err = captioning::start(&state, req)
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

/// Captioning room not found returns NotFound.
#[tokio::test]
async fn start_captioning_room_not_found() -> TestResult {
    let sess = kratos_session().await;
    let state = shared_state().await;
    let room_id = uuid::Uuid::new_v4().to_string();

    state
        .keto
        .grant("room", &room_id, "moderator", &sess.identity_id)
        .await?;

    let req = authed_request(
        StartCaptioningRequest {
            room_id: room_id.clone(),
            language: "en".into(),
        },
        &sess.bearer,
    );
    let err = captioning::start(&state, req)
        .await
        .expect_err("must be NotFound");
    assert_eq!(err.code(), tonic::Code::NotFound);
    Ok(())
}

/// stop_captioning transitions existing captioning_state row to `stopping`.
/// The response carries `CaptioningState::Stopping`.
#[tokio::test]
async fn stop_captioning_returns_stopping_state() -> TestResult {
    let sess = kratos_session().await;
    let (room, pool, state) = setup_room_with_moderator(&sess.identity_id).await;

    // Seed a captioning_state row so stop has something to transition.
    sqlx::query(
        "INSERT INTO captioning_state (room_id, state, language, job_id, worker_name, updated_at)
         VALUES ($1, 'active', 'en', 'job-test', 'worker-0', NOW())
         ON CONFLICT (room_id) DO UPDATE SET state='active', updated_at=NOW()",
    )
    .bind(room.id)
    .execute(&pool)
    .await
    .expect("seed captioning_state");

    let req = authed_request(
        StopCaptioningRequest {
            room_id: room.id.to_string(),
        },
        &sess.bearer,
    );
    let resp = captioning::stop(&state, req).await?.into_inner();

    assert_eq!(
        resp.state,
        CaptioningState::Stopping as i32,
        "stop must return CaptioningState::Stopping"
    );
    assert_eq!(
        resp.room_id,
        room.id.to_string(),
        "room_id must echo the request"
    );

    room_store::hard_delete(&pool, &room.id).await?;
    state
        .livekit
        .delete_room(&room.livekit_room_name)
        .await
        .ok();
    Ok(())
}

/// stop_captioning when no captioning_state row exists still succeeds
/// (UPDATE on empty table is a no-op).
#[tokio::test]
async fn stop_captioning_no_state_row_succeeds() -> TestResult {
    let sess = kratos_session().await;
    let (room, pool, state) = setup_room_with_moderator(&sess.identity_id).await;

    let req = authed_request(
        StopCaptioningRequest {
            room_id: room.id.to_string(),
        },
        &sess.bearer,
    );
    let resp = captioning::stop(&state, req).await?.into_inner();
    assert_eq!(resp.state, CaptioningState::Stopping as i32);

    room_store::hard_delete(&pool, &room.id).await?;
    state
        .livekit
        .delete_room(&room.livekit_room_name)
        .await
        .ok();
    Ok(())
}

/// Non-moderator cannot stop captioning.
#[tokio::test]
async fn stop_captioning_denied_without_moderator_role() -> TestResult {
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
        StopCaptioningRequest {
            room_id: room.id.to_string(),
        },
        &member_sess.bearer,
    );
    let err = captioning::stop(&state, req)
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

/// Invalid room_id returns InvalidArgument.
#[tokio::test]
async fn start_captioning_invalid_room_id() -> TestResult {
    let sess = kratos_session().await;
    let state = shared_state().await;

    let req = authed_request(
        StartCaptioningRequest {
            room_id: "bad-uuid".into(),
            language: "en".into(),
        },
        &sess.bearer,
    );
    let err = captioning::start(&state, req).await.expect_err("must fail");
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    Ok(())
}

// ── Summaries ─────────────────────────────────────────────────────────────

/// get_meeting_summary returns the latest summary for a room with structured
/// fields populated: id, room_id, summary_markdown, generated_at.
#[tokio::test]
async fn get_meeting_summary_happy_path() -> TestResult {
    let sess = kratos_session().await;
    let (room, pool, state) = setup_room_with_moderator(&sess.identity_id).await;

    let sum_id = seed_summary(&pool, room.id, &sess.identity_id).await;

    let req = authed_request(
        GetMeetingSummaryRequest {
            room_id: room.id.to_string(),
        },
        &sess.bearer,
    );
    let summary = summaries::get(&state, req).await?.into_inner();

    assert_eq!(summary.id, sum_id.to_string());
    assert_eq!(summary.room_id, room.id.to_string());
    assert_eq!(summary.summary_markdown, "Summary text.");
    assert!(
        summary.generated_at.is_some(),
        "generated_at must be populated"
    );
    // Full transcript is included in get (not list).
    assert_eq!(summary.full_transcript, "ref-transcript");

    room_store::hard_delete(&pool, &room.id).await?;
    state
        .livekit
        .delete_room(&room.livekit_room_name)
        .await
        .ok();
    Ok(())
}

/// get_meeting_summary returns NotFound when no summary row exists.
#[tokio::test]
async fn get_meeting_summary_not_found() -> TestResult {
    let sess = kratos_session().await;
    let (room, pool, state) = setup_room_with_moderator(&sess.identity_id).await;

    let req = authed_request(
        GetMeetingSummaryRequest {
            room_id: room.id.to_string(),
        },
        &sess.bearer,
    );
    let err = summaries::get(&state, req)
        .await
        .expect_err("must be NotFound");
    assert_eq!(err.code(), tonic::Code::NotFound);

    room_store::hard_delete(&pool, &room.id).await?;
    state
        .livekit
        .delete_room(&room.livekit_room_name)
        .await
        .ok();
    Ok(())
}

/// Non-participant is denied get_meeting_summary.
#[tokio::test]
async fn get_meeting_summary_denied_without_participant_role() -> TestResult {
    let owner_sess = kratos_session().await;
    let intruder_sess = kratos_session().await;
    let (room, pool, state) = setup_room_with_moderator(&owner_sess.identity_id).await;

    seed_summary(&pool, room.id, &owner_sess.identity_id).await;

    let req = authed_request(
        GetMeetingSummaryRequest {
            room_id: room.id.to_string(),
        },
        &intruder_sess.bearer,
    );
    let err = summaries::get(&state, req)
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

/// Invalid room_id returns InvalidArgument for get_meeting_summary.
#[tokio::test]
async fn get_meeting_summary_invalid_room_id() -> TestResult {
    let sess = kratos_session().await;
    let state = shared_state().await;

    let req = authed_request(
        GetMeetingSummaryRequest {
            room_id: "not-a-uuid".into(),
        },
        &sess.bearer,
    );
    let err = summaries::get(&state, req).await.expect_err("must fail");
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    Ok(())
}

/// list_meeting_summaries returns rows with transcript omitted (empty string).
#[tokio::test]
async fn list_meeting_summaries_omits_transcript() -> TestResult {
    let sess = kratos_session().await;
    let (room, pool, state) = setup_room_with_moderator(&sess.identity_id).await;

    let sum_id = seed_summary(&pool, room.id, &sess.identity_id).await;

    let req = authed_request(
        ListMeetingSummariesRequest {
            page_size: 50,
            page_token: String::new(),
        },
        &sess.bearer,
    );
    let resp = summaries::list(&state, req).await?.into_inner();

    let found = resp
        .summaries
        .iter()
        .find(|s| s.id == sum_id.to_string())
        .expect("seeded summary must be in list");

    // list omits full_transcript.
    assert!(
        found.full_transcript.is_empty(),
        "list must not include the full transcript"
    );
    assert_eq!(found.summary_markdown, "Summary text.");
    assert!(found.generated_at.is_some(), "generated_at must be set");

    room_store::hard_delete(&pool, &room.id).await?;
    state
        .livekit
        .delete_room(&room.livekit_room_name)
        .await
        .ok();
    Ok(())
}

/// list_meeting_summaries with page_size=0 uses the default (50) and does not error.
#[tokio::test]
async fn list_meeting_summaries_zero_page_size_defaults() -> TestResult {
    let sess = kratos_session().await;
    let state = shared_state().await;

    let req = authed_request(
        ListMeetingSummariesRequest {
            page_size: 0,
            page_token: String::new(),
        },
        &sess.bearer,
    );
    summaries::list(&state, req).await?;
    Ok(())
}

/// Unauthenticated list_meeting_summaries is rejected.
#[tokio::test]
async fn list_meeting_summaries_unauthenticated() -> TestResult {
    let state = shared_state().await;
    let req = tonic::Request::new(ListMeetingSummariesRequest {
        page_size: 10,
        page_token: String::new(),
    });
    let err = summaries::list(&state, req).await.expect_err("must fail");
    assert_eq!(err.code(), tonic::Code::Unauthenticated);
    Ok(())
}
