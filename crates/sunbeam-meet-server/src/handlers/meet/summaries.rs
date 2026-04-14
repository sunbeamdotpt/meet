//! Summary fetch RPCs.
//
// Proto request/response types are imported wholesale; naming each one is noise.
#![allow(clippy::wildcard_imports)]

use sunbeam_meet_proto::meet::v1::*;
use tonic::{Request, Response, Status};
use uuid::Uuid;

use crate::handlers::meet::{identity, parse_uuid, rooms};
use crate::state::SharedState;

/// Get the latest summary for a room.
pub async fn get(
    state: &SharedState,
    req: Request<GetMeetingSummaryRequest>,
) -> Result<Response<MeetingSummary>, Status> {
    let id = identity(state, &req).await?;
    let r = req.into_inner();
    let room_id = parse_uuid(&r.room_id, "room_id")?;
    crate::authz::can(state, &id, "room", &room_id.to_string(), "participant").await?;

    let row = sqlx::query_as::<_, Row>(
        "SELECT id, room_id, transcript_ref, summary_md, action_items, meeting_duration_ms,
                attendees, generated_at
         FROM summaries WHERE room_id = $1 ORDER BY generated_at DESC LIMIT 1",
    )
    .bind(room_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| Status::internal(e.to_string()))?
    .ok_or_else(|| Status::not_found("summary"))?;

    let display_name: String = sqlx::query_scalar("SELECT display_name FROM rooms WHERE id = $1")
        .bind(room_id)
        .fetch_optional(&state.db)
        .await
        .unwrap_or_default()
        .unwrap_or_default();

    Ok(Response::new(to_proto(row, display_name, true)))
}

/// List summaries (transcript omitted).
pub async fn list(
    state: &SharedState,
    req: Request<ListMeetingSummariesRequest>,
) -> Result<Response<ListMeetingSummariesResponse>, Status> {
    let _id = identity(state, &req).await?;
    let r = req.into_inner();
    let page = i64::from(if r.page_size == 0 {
        50
    } else {
        r.page_size.min(500)
    });
    let rows = sqlx::query_as::<_, Row>(
        "SELECT id, room_id, transcript_ref, summary_md, action_items, meeting_duration_ms,
                attendees, generated_at
         FROM summaries ORDER BY generated_at DESC LIMIT $1",
    )
    .bind(page)
    .fetch_all(&state.db)
    .await
    .map_err(|e| Status::internal(e.to_string()))?;

    let mut summaries = Vec::new();
    for row in rows {
        let dn: String = sqlx::query_scalar("SELECT display_name FROM rooms WHERE id = $1")
            .bind(row.room_id)
            .fetch_optional(&state.db)
            .await
            .unwrap_or_default()
            .unwrap_or_default();
        summaries.push(to_proto(row, dn, false));
    }
    Ok(Response::new(ListMeetingSummariesResponse {
        summaries,
        next_page_token: String::new(),
    }))
}

fn to_proto(row: Row, display_name: String, include_transcript: bool) -> MeetingSummary {
    let action_items: Vec<ActionItem> = row
        .action_items
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|v| ActionItem {
                    description: v
                        .get("description")
                        .and_then(|s| s.as_str())
                        .unwrap_or_default()
                        .to_owned(),
                    assignee_identity: v
                        .get("assignee_identity")
                        .and_then(|s| s.as_str())
                        .unwrap_or_default()
                        .to_owned(),
                    assignee_name: v
                        .get("assignee_name")
                        .and_then(|s| s.as_str())
                        .unwrap_or_default()
                        .to_owned(),
                    completed: v
                        .get("completed")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false),
                })
                .collect()
        })
        .unwrap_or_default();
    MeetingSummary {
        id: row.id.to_string(),
        room_id: row.room_id.to_string(),
        room_display_name: display_name,
        summary_markdown: row.summary_md,
        action_items,
        attendee_identities: row.attendees,
        meeting_duration: Some(prost_types::Duration {
            seconds: row.meeting_duration_ms / 1000,
            nanos: ((row.meeting_duration_ms % 1000) * 1_000_000) as i32,
        }),
        full_transcript: if include_transcript {
            row.transcript_ref.unwrap_or_default()
        } else {
            String::new()
        },
        generated_at: Some(rooms::ts(row.generated_at)),
    }
}

#[derive(sqlx::FromRow)]
struct Row {
    id: Uuid,
    room_id: Uuid,
    transcript_ref: Option<String>,
    summary_md: String,
    action_items: serde_json::Value,
    meeting_duration_ms: i64,
    attendees: Vec<String>,
    generated_at: chrono::DateTime<chrono::Utc>,
}
