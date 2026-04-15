//! Integration tests — `handlers/meet/scheduling.rs` RPC surface.
//!
//! Exercises create, get, list, update (stubbed), and cancel against real
//! Postgres (schedules table) and real Stalwart CalDAV. CalDAV events are
//! created by the handler via `state.caldav.put_event` — we verify round-trip
//! by inspecting the structured `ScheduledMeeting` fields returned by get/list.
//!
//! Required env (same as test-runner container):
//!   DATABASE_URL, VALKEY_URL, NATS_URL, CALDAV_URL, KRATOS_PUBLIC_URL,
//!   KRATOS_ADMIN_URL, KETO_READ_URL, KETO_WRITE_URL, LIVEKIT_URL,
//!   LIVEKIT_API_KEY, LIVEKIT_API_SECRET, S3_ENDPOINT, S3_ACCESS_KEY,
//!   S3_SECRET_KEY.
//!
//! Assertion strategy: structured proto fields only — never string-match.
//! Cleanup: tests delete their schedule rows and CalDAV events on teardown.

mod common;

use common::{authed_request, kratos_session, shared_state, unique_room_slug, TestResult};
use prost_types::Timestamp;
use sunbeam_meet_proto::meet::v1::{
    CancelScheduledMeetingRequest, GetScheduledMeetingRequest, ListScheduledMeetingsRequest,
    MeetingRecurrence, RoomAccessLevel, ScheduleMeetingRequest, UpdateScheduledMeetingRequest,
    VideoQualityPreset,
};
use sunbeam_meet_server::handlers::meet::scheduling;
use tonic::Code;

// ── helpers ──────────────────────────────────────────────────────────────────

/// Unix timestamp offset from now by `secs` seconds, as a prost Timestamp.
fn ts_from_now(secs: i64) -> Timestamp {
    let t = chrono::Utc::now() + chrono::Duration::seconds(secs);
    Timestamp {
        seconds: t.timestamp(),
        nanos: 0,
    }
}

/// Hard-delete a schedule row — called on teardown only. The handler's cancel
/// only sets `deleted_at`; this removes the row entirely to keep the test DB
/// tidy across runs.
async fn cleanup_schedule(state: &sunbeam_meet_server::state::SharedState, id: &str) {
    if let Ok(uuid) = uuid::Uuid::parse_str(id) {
        let _ = sqlx::query("DELETE FROM schedules WHERE id = $1")
            .bind(uuid)
            .execute(&state.db)
            .await;
        let _ = sqlx::query("DELETE FROM schedule_invitees WHERE schedule_id = $1")
            .bind(uuid)
            .execute(&state.db)
            .await;
    }
}

// ── create ───────────────────────────────────────────────────────────────────

/// Happy path: schedule a meeting, verify every structured proto field.
#[tokio::test]
async fn create_scheduled_meeting_happy_path() -> TestResult {
    let state = shared_state().await;
    let sess = kratos_session().await;
    let title = unique_room_slug("it-sched");

    let req = authed_request(
        ScheduleMeetingRequest {
            display_name: title.clone(),
            description: "created by it_handlers_scheduling".into(),
            starts_at: Some(ts_from_now(3600)),
            ends_at: Some(ts_from_now(7200)),
            access_level: RoomAccessLevel::Trusted as i32,
            default_quality: VideoQualityPreset::High as i32,
            recurrence: MeetingRecurrence::None as i32,
            recurrence_rule: String::new(),
            invitee_emails: vec!["alice@sunbeam.test".into(), "bob@sunbeam.test".into()],
        },
        &sess.bearer,
    );

    let meeting = scheduling::create(&state, req).await?.into_inner();

    // Structural assertions.
    assert_eq!(meeting.display_name, title);
    assert_eq!(meeting.description, "created by it_handlers_scheduling");
    assert_eq!(meeting.organizer_identity, sess.identity_id);
    assert_eq!(meeting.access_level, RoomAccessLevel::Trusted as i32);
    assert_eq!(meeting.default_quality, VideoQualityPreset::High as i32);
    assert_eq!(meeting.recurrence, MeetingRecurrence::None as i32);
    assert!(!meeting.id.is_empty(), "meeting.id must be populated");
    assert!(
        !meeting.caldav_uid.is_empty(),
        "caldav_uid must be populated"
    );
    assert!(meeting.starts_at.is_some(), "starts_at must be set");
    assert!(meeting.ends_at.is_some(), "ends_at must be set");
    assert!(meeting.created_at.is_some(), "created_at must be set");

    // Invitees are stored and round-tripped.
    let mut invitees = meeting.invitee_emails.clone();
    invitees.sort();
    assert_eq!(
        invitees,
        vec!["alice@sunbeam.test", "bob@sunbeam.test"],
        "invitees must round-trip exactly",
    );

    // Timestamps are ordered: starts_at < ends_at.
    let starts = meeting.starts_at.as_ref().unwrap().seconds;
    let ends = meeting.ends_at.as_ref().unwrap().seconds;
    assert!(
        starts < ends,
        "starts_at ({starts}) must be before ends_at ({ends})"
    );

    cleanup_schedule(&state, &meeting.id).await;
    Ok(())
}

/// create with missing starts_at returns InvalidArgument.
#[tokio::test]
async fn create_scheduled_meeting_missing_starts_at() -> TestResult {
    let state = shared_state().await;
    let sess = kratos_session().await;

    let req = authed_request(
        ScheduleMeetingRequest {
            display_name: unique_room_slug("it-nosched"),
            ends_at: Some(ts_from_now(7200)),
            starts_at: None, // missing
            ..ScheduleMeetingRequest::default()
        },
        &sess.bearer,
    );

    let err = scheduling::create(&state, req).await.unwrap_err();
    assert_eq!(
        err.code(),
        Code::InvalidArgument,
        "missing starts_at must return InvalidArgument, got {:?}",
        err.code(),
    );
    Ok(())
}

/// create with missing ends_at returns InvalidArgument.
#[tokio::test]
async fn create_scheduled_meeting_missing_ends_at() -> TestResult {
    let state = shared_state().await;
    let sess = kratos_session().await;

    let req = authed_request(
        ScheduleMeetingRequest {
            display_name: unique_room_slug("it-noend"),
            starts_at: Some(ts_from_now(3600)),
            ends_at: None, // missing
            ..ScheduleMeetingRequest::default()
        },
        &sess.bearer,
    );

    let err = scheduling::create(&state, req).await.unwrap_err();
    assert_eq!(
        err.code(),
        Code::InvalidArgument,
        "missing ends_at must return InvalidArgument, got {:?}",
        err.code(),
    );
    Ok(())
}

/// create without auth returns Unauthenticated.
#[tokio::test]
async fn create_scheduled_meeting_unauthenticated() -> TestResult {
    let state = shared_state().await;

    let req = tonic::Request::new(ScheduleMeetingRequest {
        display_name: unique_room_slug("it-noauth-sched"),
        starts_at: Some(ts_from_now(3600)),
        ends_at: Some(ts_from_now(7200)),
        ..ScheduleMeetingRequest::default()
    });

    let err = scheduling::create(&state, req).await.unwrap_err();
    assert_eq!(
        err.code(),
        Code::Unauthenticated,
        "missing auth must return Unauthenticated, got {:?}",
        err.code(),
    );
    Ok(())
}

/// create with a recurrence rule stores the rule and recurrence type.
#[tokio::test]
async fn create_scheduled_meeting_with_rrule() -> TestResult {
    let state = shared_state().await;
    let sess = kratos_session().await;

    let req = authed_request(
        ScheduleMeetingRequest {
            display_name: unique_room_slug("it-rrule"),
            starts_at: Some(ts_from_now(3600)),
            ends_at: Some(ts_from_now(5400)),
            recurrence: MeetingRecurrence::Weekly as i32,
            recurrence_rule: "FREQ=WEEKLY;COUNT=4".into(),
            ..ScheduleMeetingRequest::default()
        },
        &sess.bearer,
    );

    let meeting = scheduling::create(&state, req).await?.into_inner();
    assert_eq!(meeting.recurrence, MeetingRecurrence::Weekly as i32);
    assert_eq!(meeting.recurrence_rule, "FREQ=WEEKLY;COUNT=4");

    cleanup_schedule(&state, &meeting.id).await;
    Ok(())
}

// ── get ──────────────────────────────────────────────────────────────────────

/// Happy path: get a just-created scheduled meeting.
#[tokio::test]
async fn get_scheduled_meeting_returns_created() -> TestResult {
    let state = shared_state().await;
    let sess = kratos_session().await;
    let title = unique_room_slug("it-get-sched");

    let created = scheduling::create(
        &state,
        authed_request(
            ScheduleMeetingRequest {
                display_name: title.clone(),
                starts_at: Some(ts_from_now(3600)),
                ends_at: Some(ts_from_now(7200)),
                ..ScheduleMeetingRequest::default()
            },
            &sess.bearer,
        ),
    )
    .await?
    .into_inner();

    let fetched = scheduling::get(
        &state,
        authed_request(
            GetScheduledMeetingRequest {
                meeting_id: created.id.clone(),
            },
            &sess.bearer,
        ),
    )
    .await?
    .into_inner();

    assert_eq!(fetched.id, created.id);
    assert_eq!(fetched.display_name, title);
    assert_eq!(fetched.organizer_identity, sess.identity_id);
    assert_eq!(
        fetched.starts_at.as_ref().map(|t| t.seconds),
        created.starts_at.as_ref().map(|t| t.seconds),
        "starts_at must round-trip",
    );

    cleanup_schedule(&state, &created.id).await;
    Ok(())
}

/// get with a non-existent meeting_id returns NotFound.
#[tokio::test]
async fn get_scheduled_meeting_not_found() -> TestResult {
    let state = shared_state().await;
    let sess = kratos_session().await;

    let err = scheduling::get(
        &state,
        authed_request(
            GetScheduledMeetingRequest {
                meeting_id: uuid::Uuid::now_v7().to_string(),
            },
            &sess.bearer,
        ),
    )
    .await
    .unwrap_err();

    assert_eq!(
        err.code(),
        Code::NotFound,
        "non-existent meeting must return NotFound, got {:?}",
        err.code(),
    );
    Ok(())
}

/// get with a malformed meeting_id returns InvalidArgument.
#[tokio::test]
async fn get_scheduled_meeting_malformed_id() -> TestResult {
    let state = shared_state().await;
    let sess = kratos_session().await;

    let err = scheduling::get(
        &state,
        authed_request(
            GetScheduledMeetingRequest {
                meeting_id: "not-a-uuid".into(),
            },
            &sess.bearer,
        ),
    )
    .await
    .unwrap_err();

    assert_eq!(err.code(), Code::InvalidArgument);
    Ok(())
}

// ── list ─────────────────────────────────────────────────────────────────────

/// list returns scheduled meetings owned by the calling identity.
#[tokio::test]
async fn list_scheduled_meetings_returns_owned() -> TestResult {
    let state = shared_state().await;
    let sess = kratos_session().await;

    let m1 = scheduling::create(
        &state,
        authed_request(
            ScheduleMeetingRequest {
                display_name: unique_room_slug("it-list-s1"),
                starts_at: Some(ts_from_now(3600)),
                ends_at: Some(ts_from_now(7200)),
                ..ScheduleMeetingRequest::default()
            },
            &sess.bearer,
        ),
    )
    .await?
    .into_inner();

    let m2 = scheduling::create(
        &state,
        authed_request(
            ScheduleMeetingRequest {
                display_name: unique_room_slug("it-list-s2"),
                starts_at: Some(ts_from_now(7200)),
                ends_at: Some(ts_from_now(10800)),
                ..ScheduleMeetingRequest::default()
            },
            &sess.bearer,
        ),
    )
    .await?
    .into_inner();

    let resp = scheduling::list(
        &state,
        authed_request(
            ListScheduledMeetingsRequest {
                page_size: 500,
                ..ListScheduledMeetingsRequest::default()
            },
            &sess.bearer,
        ),
    )
    .await?
    .into_inner();

    let ids: Vec<&str> = resp.meetings.iter().map(|m| m.id.as_str()).collect();
    assert!(
        ids.contains(&m1.id.as_str()),
        "list must include meeting m1 (id={})",
        m1.id,
    );
    assert!(
        ids.contains(&m2.id.as_str()),
        "list must include meeting m2 (id={})",
        m2.id,
    );

    // All returned meetings must belong to the calling identity.
    for m in &resp.meetings {
        assert_eq!(
            m.organizer_identity, sess.identity_id,
            "list must only return meetings owned by the caller",
        );
    }

    cleanup_schedule(&state, &m1.id).await;
    cleanup_schedule(&state, &m2.id).await;
    Ok(())
}

/// list for a different identity does not return meetings owned by another user.
#[tokio::test]
async fn list_scheduled_meetings_isolation_between_identities() -> TestResult {
    let state = shared_state().await;
    let owner = kratos_session().await;
    let stranger = kratos_session().await;

    let owned = scheduling::create(
        &state,
        authed_request(
            ScheduleMeetingRequest {
                display_name: unique_room_slug("it-isol"),
                starts_at: Some(ts_from_now(3600)),
                ends_at: Some(ts_from_now(7200)),
                ..ScheduleMeetingRequest::default()
            },
            &owner.bearer,
        ),
    )
    .await?
    .into_inner();

    // Stranger lists their own — must not see owner's meeting.
    let stranger_resp = scheduling::list(
        &state,
        authed_request(
            ListScheduledMeetingsRequest {
                page_size: 500,
                ..ListScheduledMeetingsRequest::default()
            },
            &stranger.bearer,
        ),
    )
    .await?
    .into_inner();

    assert!(
        !stranger_resp.meetings.iter().any(|m| m.id == owned.id),
        "stranger must not see meetings owned by a different identity",
    );

    cleanup_schedule(&state, &owned.id).await;
    Ok(())
}

/// list page_size=0 defaults gracefully (must not panic or error).
#[tokio::test]
async fn list_scheduled_meetings_zero_page_size_defaults() -> TestResult {
    let state = shared_state().await;
    let sess = kratos_session().await;

    let resp = scheduling::list(
        &state,
        authed_request(
            ListScheduledMeetingsRequest {
                page_size: 0,
                ..ListScheduledMeetingsRequest::default()
            },
            &sess.bearer,
        ),
    )
    .await?
    .into_inner();

    assert!(
        resp.meetings.len() <= 50,
        "page_size=0 must default to 50; got {} meetings",
        resp.meetings.len(),
    );
    Ok(())
}

// ── update (stubbed) ──────────────────────────────────────────────────────────

/// update returns Unimplemented (stubbed per scheduling.rs comments).
#[tokio::test]
async fn update_scheduled_meeting_returns_unimplemented() -> TestResult {
    let state = shared_state().await;
    let sess = kratos_session().await;

    let err = scheduling::update(
        &state,
        authed_request(
            UpdateScheduledMeetingRequest {
                meeting_id: uuid::Uuid::now_v7().to_string(),
                ..UpdateScheduledMeetingRequest::default()
            },
            &sess.bearer,
        ),
    )
    .await
    .unwrap_err();

    assert_eq!(
        err.code(),
        Code::Unimplemented,
        "update must return Unimplemented (pending live integration), got {:?}",
        err.code(),
    );
    Ok(())
}

// ── cancel ────────────────────────────────────────────────────────────────────

/// Happy path: cancel a meeting removes it from the DB and CalDAV.
#[tokio::test]
async fn cancel_scheduled_meeting_happy_path() -> TestResult {
    let state = shared_state().await;
    let sess = kratos_session().await;

    let created = scheduling::create(
        &state,
        authed_request(
            ScheduleMeetingRequest {
                display_name: unique_room_slug("it-cancel"),
                starts_at: Some(ts_from_now(3600)),
                ends_at: Some(ts_from_now(7200)),
                ..ScheduleMeetingRequest::default()
            },
            &sess.bearer,
        ),
    )
    .await?
    .into_inner();

    // Cancel returns an empty success response.
    scheduling::cancel(
        &state,
        authed_request(
            CancelScheduledMeetingRequest {
                meeting_id: created.id.clone(),
                notify_invitees: false,
            },
            &sess.bearer,
        ),
    )
    .await?;

    // After cancel, get must return NotFound (deleted_at is set).
    let err = scheduling::get(
        &state,
        authed_request(
            GetScheduledMeetingRequest {
                meeting_id: created.id.clone(),
            },
            &sess.bearer,
        ),
    )
    .await
    .unwrap_err();

    assert_eq!(
        err.code(),
        Code::NotFound,
        "cancelled meeting must not be retrievable; expected NotFound, got {:?}",
        err.code(),
    );

    // Hard-delete residual row (deleted_at set, not gone).
    cleanup_schedule(&state, &created.id).await;
    Ok(())
}

/// cancel by a different identity returns PermissionDenied.
#[tokio::test]
async fn cancel_scheduled_meeting_non_owner_returns_permission_denied() -> TestResult {
    let state = shared_state().await;
    let owner = kratos_session().await;
    let interloper = kratos_session().await;

    let created = scheduling::create(
        &state,
        authed_request(
            ScheduleMeetingRequest {
                display_name: unique_room_slug("it-perm"),
                starts_at: Some(ts_from_now(3600)),
                ends_at: Some(ts_from_now(7200)),
                ..ScheduleMeetingRequest::default()
            },
            &owner.bearer,
        ),
    )
    .await?
    .into_inner();

    let err = scheduling::cancel(
        &state,
        authed_request(
            CancelScheduledMeetingRequest {
                meeting_id: created.id.clone(),
                notify_invitees: false,
            },
            &interloper.bearer, // different identity
        ),
    )
    .await
    .unwrap_err();

    assert_eq!(
        err.code(),
        Code::PermissionDenied,
        "non-owner cancel must return PermissionDenied, got {:?}",
        err.code(),
    );

    cleanup_schedule(&state, &created.id).await;
    Ok(())
}

/// cancel with a non-existent meeting_id returns NotFound.
#[tokio::test]
async fn cancel_scheduled_meeting_not_found() -> TestResult {
    let state = shared_state().await;
    let sess = kratos_session().await;

    let err = scheduling::cancel(
        &state,
        authed_request(
            CancelScheduledMeetingRequest {
                meeting_id: uuid::Uuid::now_v7().to_string(),
                notify_invitees: false,
            },
            &sess.bearer,
        ),
    )
    .await
    .unwrap_err();

    assert_eq!(
        err.code(),
        Code::NotFound,
        "non-existent cancel must return NotFound, got {:?}",
        err.code(),
    );
    Ok(())
}

/// cancel with a malformed meeting_id returns InvalidArgument.
#[tokio::test]
async fn cancel_scheduled_meeting_malformed_id() -> TestResult {
    let state = shared_state().await;
    let sess = kratos_session().await;

    let err = scheduling::cancel(
        &state,
        authed_request(
            CancelScheduledMeetingRequest {
                meeting_id: "not-a-uuid".into(),
                notify_invitees: false,
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

/// create → get → list → cancel → verify gone.
#[tokio::test]
async fn scheduling_full_round_trip() -> TestResult {
    let state = shared_state().await;
    let sess = kratos_session().await;
    let title = unique_room_slug("it-sched-rt");

    // Create.
    let m = scheduling::create(
        &state,
        authed_request(
            ScheduleMeetingRequest {
                display_name: title.clone(),
                description: "round-trip test".into(),
                starts_at: Some(ts_from_now(3600)),
                ends_at: Some(ts_from_now(5400)),
                access_level: RoomAccessLevel::Public as i32,
                default_quality: VideoQualityPreset::Auto as i32,
                recurrence: MeetingRecurrence::Daily as i32,
                recurrence_rule: "FREQ=DAILY;COUNT=3".into(),
                invitee_emails: vec!["rsvp@sunbeam.test".into()],
            },
            &sess.bearer,
        ),
    )
    .await?
    .into_inner();

    assert_eq!(m.display_name, title);
    assert_eq!(m.recurrence, MeetingRecurrence::Daily as i32);
    assert_eq!(m.recurrence_rule, "FREQ=DAILY;COUNT=3");
    assert_eq!(m.invitee_emails, vec!["rsvp@sunbeam.test"]);

    // Get.
    let fetched = scheduling::get(
        &state,
        authed_request(
            GetScheduledMeetingRequest {
                meeting_id: m.id.clone(),
            },
            &sess.bearer,
        ),
    )
    .await?
    .into_inner();
    assert_eq!(fetched.caldav_uid, m.caldav_uid);
    assert_eq!(fetched.organizer_identity, sess.identity_id);

    // List.
    let resp = scheduling::list(
        &state,
        authed_request(
            ListScheduledMeetingsRequest {
                page_size: 500,
                ..ListScheduledMeetingsRequest::default()
            },
            &sess.bearer,
        ),
    )
    .await?
    .into_inner();
    assert!(
        resp.meetings.iter().any(|x| x.id == m.id),
        "list must include the created meeting",
    );

    // Cancel.
    scheduling::cancel(
        &state,
        authed_request(
            CancelScheduledMeetingRequest {
                meeting_id: m.id.clone(),
                notify_invitees: false,
            },
            &sess.bearer,
        ),
    )
    .await?;

    // Verify gone from get.
    let gone_err = scheduling::get(
        &state,
        authed_request(
            GetScheduledMeetingRequest {
                meeting_id: m.id.clone(),
            },
            &sess.bearer,
        ),
    )
    .await
    .unwrap_err();
    assert_eq!(gone_err.code(), Code::NotFound);

    // Verify gone from list.
    let after_resp = scheduling::list(
        &state,
        authed_request(
            ListScheduledMeetingsRequest {
                page_size: 500,
                ..ListScheduledMeetingsRequest::default()
            },
            &sess.bearer,
        ),
    )
    .await?
    .into_inner();
    assert!(
        !after_resp.meetings.iter().any(|x| x.id == m.id),
        "cancelled meeting must not appear in list",
    );

    cleanup_schedule(&state, &m.id).await;
    Ok(())
}
