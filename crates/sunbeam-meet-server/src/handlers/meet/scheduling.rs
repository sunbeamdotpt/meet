//! Scheduling RPCs — CalDAV-backed.
//
// Proto request/response types are imported wholesale; naming each one is noise.
#![allow(clippy::wildcard_imports)]

use sunbeam_meet_proto::meet::v1::*;
use tonic::{Request, Response, Status};
use uuid::Uuid;

use crate::handlers::meet::{identity, parse_uuid, rooms};
use crate::state::SharedState;
use crate::storage::pg::new_id;
use crate::storage::pg::rooms::PgRoomAccessLevel;

/// Create a scheduled meeting.
pub async fn create(
    state: &SharedState,
    req: Request<ScheduleMeetingRequest>,
) -> Result<Response<ScheduledMeeting>, Status> {
    let id = identity(state, &req).await?;
    let r = req.into_inner();
    let starts = r
        .starts_at
        .as_ref()
        .ok_or_else(|| Status::invalid_argument("starts_at"))?;
    let ends = r
        .ends_at
        .as_ref()
        .ok_or_else(|| Status::invalid_argument("ends_at"))?;
    let starts_dt =
        chrono::DateTime::<chrono::Utc>::from_timestamp(starts.seconds, starts.nanos as u32)
            .ok_or_else(|| Status::invalid_argument("starts_at"))?;
    let ends_dt = chrono::DateTime::<chrono::Utc>::from_timestamp(ends.seconds, ends.nanos as u32)
        .ok_or_else(|| Status::invalid_argument("ends_at"))?;

    let sched_id = new_id();
    let uid = format!("meet-{sched_id}@sunbeam.pt");
    let ics = render_ics(
        &uid,
        &r.display_name,
        &r.description,
        starts_dt,
        ends_dt,
        &r.recurrence_rule,
    );
    let etag = state
        .caldav
        .put_event(&uid, &ics)
        .await
        .map_err(Status::from)?;

    sqlx::query(
        "INSERT INTO schedules (id, owner_identity, title, description, access_level, default_quality,
                               starts_at, ends_at, recurrence, recurrence_rule, caldav_uid, caldav_etag)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
    )
    .bind(sched_id)
    .bind(&id.id)
    .bind(&r.display_name)
    .bind(&r.description)
    .bind(access_pg(r.access_level))
    .bind(quality_to_str(r.default_quality))
    .bind(starts_dt)
    .bind(ends_dt)
    .bind(recurrence_to_str(r.recurrence))
    .bind(&r.recurrence_rule)
    .bind(&uid)
    .bind(&etag)
    .execute(&state.db)
    .await
    .map_err(|e| Status::internal(e.to_string()))?;

    for email in &r.invitee_emails {
        sqlx::query(
            "INSERT INTO schedule_invitees (schedule_id, email) VALUES ($1, $2) ON CONFLICT DO NOTHING",
        )
        .bind(sched_id)
        .bind(email)
        .execute(&state.db)
        .await
        .map_err(|e| Status::internal(e.to_string()))?;
    }

    Ok(Response::new(load(state, sched_id).await?))
}

/// Get a scheduled meeting.
pub async fn get(
    state: &SharedState,
    req: Request<GetScheduledMeetingRequest>,
) -> Result<Response<ScheduledMeeting>, Status> {
    let _ = identity(state, &req).await?;
    let sched_id = parse_uuid(&req.into_inner().meeting_id, "meeting_id")?;
    Ok(Response::new(load(state, sched_id).await?))
}

/// List scheduled meetings.
pub async fn list(
    state: &SharedState,
    req: Request<ListScheduledMeetingsRequest>,
) -> Result<Response<ListScheduledMeetingsResponse>, Status> {
    let id = identity(state, &req).await?;
    let r = req.into_inner();
    let page = i64::from(if r.page_size == 0 {
        50
    } else {
        r.page_size.min(500)
    });
    let ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM schedules
         WHERE owner_identity = $1 AND deleted_at IS NULL
         ORDER BY starts_at ASC LIMIT $2",
    )
    .bind(&id.id)
    .bind(page)
    .fetch_all(&state.db)
    .await
    .map_err(|e| Status::internal(e.to_string()))?;
    let mut meetings = Vec::new();
    for i in ids {
        if let Ok(m) = load(state, i).await {
            meetings.push(m);
        }
    }
    Ok(Response::new(ListScheduledMeetingsResponse {
        meetings,
        next_page_token: String::new(),
    }))
}

/// Update a scheduled meeting — stubbed (pending).
pub async fn update(
    state: &SharedState,
    req: Request<UpdateScheduledMeetingRequest>,
) -> Result<Response<ScheduledMeeting>, Status> {
    let _ = identity(state, &req).await?;
    Err(Status::unimplemented("pending live integration"))
}

/// Cancel a scheduled meeting.
pub async fn cancel(
    state: &SharedState,
    req: Request<CancelScheduledMeetingRequest>,
) -> Result<Response<CancelScheduledMeetingResponse>, Status> {
    let id = identity(state, &req).await?;
    let r = req.into_inner();
    let sched_id = parse_uuid(&r.meeting_id, "meeting_id")?;

    let (owner, uid): (String, String) =
        sqlx::query_as("SELECT owner_identity, caldav_uid FROM schedules WHERE id = $1")
            .bind(sched_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("meeting"))?;
    if owner != id.id {
        return Err(Status::permission_denied("not owner"));
    }
    let _ = state.caldav.delete_event(&uid, "").await;
    sqlx::query("UPDATE schedules SET deleted_at = NOW() WHERE id = $1")
        .bind(sched_id)
        .execute(&state.db)
        .await
        .map_err(|e| Status::internal(e.to_string()))?;
    Ok(Response::new(CancelScheduledMeetingResponse {}))
}

async fn load(state: &SharedState, id: Uuid) -> Result<ScheduledMeeting, Status> {
    let row = sqlx::query_as::<_, Row>(
        "SELECT id, owner_identity, title, description, access_level, default_quality,
                starts_at, ends_at, recurrence, recurrence_rule, caldav_uid, created_at, updated_at
         FROM schedules WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| Status::internal(e.to_string()))?
    .ok_or_else(|| Status::not_found("meeting"))?;

    let invitees: Vec<String> =
        sqlx::query_scalar("SELECT email FROM schedule_invitees WHERE schedule_id = $1")
            .bind(id)
            .fetch_all(&state.db)
            .await
            .unwrap_or_default();

    Ok(ScheduledMeeting {
        id: row.id.to_string(),
        room_name: String::new(),
        display_name: row.title,
        description: row.description,
        organizer_identity: row.owner_identity,
        access_level: RoomAccessLevel::from(row.access_level) as i32,
        default_quality: match row.default_quality.as_str() {
            "low" => VideoQualityPreset::Low as i32,
            "medium" => VideoQualityPreset::Medium as i32,
            "high" => VideoQualityPreset::High as i32,
            "ultra" => VideoQualityPreset::Ultra as i32,
            _ => VideoQualityPreset::Auto as i32,
        },
        invitee_emails: invitees,
        recurrence: match row.recurrence.as_str() {
            "daily" => MeetingRecurrence::Daily as i32,
            "weekly" => MeetingRecurrence::Weekly as i32,
            "biweekly" => MeetingRecurrence::Biweekly as i32,
            "monthly" => MeetingRecurrence::Monthly as i32,
            "custom" => MeetingRecurrence::Custom as i32,
            _ => MeetingRecurrence::None as i32,
        },
        recurrence_rule: row.recurrence_rule,
        caldav_uid: row.caldav_uid,
        starts_at: Some(rooms::ts(row.starts_at)),
        ends_at: Some(rooms::ts(row.ends_at)),
        created_at: Some(rooms::ts(row.created_at)),
        updated_at: Some(rooms::ts(row.updated_at)),
    })
}

#[derive(sqlx::FromRow)]
struct Row {
    id: Uuid,
    owner_identity: String,
    title: String,
    description: String,
    access_level: PgRoomAccessLevel,
    default_quality: String,
    starts_at: chrono::DateTime<chrono::Utc>,
    ends_at: chrono::DateTime<chrono::Utc>,
    recurrence: String,
    recurrence_rule: String,
    caldav_uid: String,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

fn access_pg(v: i32) -> PgRoomAccessLevel {
    match RoomAccessLevel::try_from(v).unwrap_or(RoomAccessLevel::Unspecified) {
        RoomAccessLevel::Public => PgRoomAccessLevel::Public,
        RoomAccessLevel::Restricted => PgRoomAccessLevel::Restricted,
        RoomAccessLevel::Trusted | RoomAccessLevel::Unspecified => PgRoomAccessLevel::Trusted,
    }
}

fn quality_to_str(v: i32) -> &'static str {
    match VideoQualityPreset::try_from(v).unwrap_or(VideoQualityPreset::Unspecified) {
        VideoQualityPreset::Low => "low",
        VideoQualityPreset::Medium => "medium",
        VideoQualityPreset::High => "high",
        VideoQualityPreset::Ultra => "ultra",
        _ => "auto",
    }
}

fn recurrence_to_str(v: i32) -> &'static str {
    match MeetingRecurrence::try_from(v).unwrap_or(MeetingRecurrence::Unspecified) {
        MeetingRecurrence::Daily => "daily",
        MeetingRecurrence::Weekly => "weekly",
        MeetingRecurrence::Biweekly => "biweekly",
        MeetingRecurrence::Monthly => "monthly",
        MeetingRecurrence::Custom => "custom",
        _ => "none",
    }
}

fn render_ics(
    uid: &str,
    title: &str,
    desc: &str,
    starts: chrono::DateTime<chrono::Utc>,
    ends: chrono::DateTime<chrono::Utc>,
    rrule: &str,
) -> String {
    use std::fmt::Write as _;
    let fmt = |d: chrono::DateTime<chrono::Utc>| d.format("%Y%m%dT%H%M%SZ").to_string();
    let mut s = String::new();
    s.push_str("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//sunbeam-meet//EN\r\n");
    s.push_str("BEGIN:VEVENT\r\n");
    let _ = writeln!(s, "UID:{uid}\r");
    let _ = writeln!(s, "DTSTAMP:{}\r", fmt(chrono::Utc::now()));
    let _ = writeln!(s, "DTSTART:{}\r", fmt(starts));
    let _ = writeln!(s, "DTEND:{}\r", fmt(ends));
    let _ = writeln!(s, "SUMMARY:{}\r", title.replace(['\r', '\n'], " "));
    if !desc.is_empty() {
        let _ = writeln!(s, "DESCRIPTION:{}\r", desc.replace('\n', "\\n"));
    }
    if !rrule.is_empty() {
        let _ = writeln!(s, "RRULE:{rrule}\r");
    }
    s.push_str("END:VEVENT\r\nEND:VCALENDAR\r\n");
    s
}
