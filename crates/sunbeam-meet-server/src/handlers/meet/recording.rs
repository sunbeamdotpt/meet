//! Recording (Egress) RPCs — fully wired start/stop.
//
// Proto request/response types are imported wholesale; naming each one is noise.
#![allow(clippy::wildcard_imports)]

use sunbeam_meet_proto::meet::v1::*;
use tonic::{Request, Response, Status};
use uuid::Uuid;

use crate::handlers::meet::{identity, parse_uuid, rooms};
use crate::state::SharedState;
use crate::storage::pg::new_id;

/// Start a recording via LiveKit Egress.
pub async fn start(
    state: &SharedState,
    req: Request<StartRecordingRequest>,
) -> Result<Response<Recording>, Status> {
    let id = identity(state, &req).await?;
    let r = req.into_inner();
    let room_id = parse_uuid(&r.room_id, "room_id")?;
    crate::authz::can(state, &id, "room", &room_id.to_string(), "moderator").await?;

    let lk_name: String = sqlx::query_scalar("SELECT livekit_room_name FROM rooms WHERE id = $1")
        .bind(room_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| Status::internal(e.to_string()))?
        .ok_or_else(|| Status::not_found("room"))?;

    let mode = match RecordingMode::try_from(r.mode).unwrap_or(RecordingMode::Unspecified) {
        RecordingMode::Speaker => "speaker",
        RecordingMode::AudioOnly => "audio_only",
        // Composite is the default for unspecified.
        RecordingMode::Composite | RecordingMode::Unspecified => "composite",
    };
    let output = match RecordingOutput::try_from(r.output).unwrap_or(RecordingOutput::Unspecified) {
        RecordingOutput::Hls => "hls",
        RecordingOutput::Rtmp => "rtmp",
        // File is the default for unspecified.
        RecordingOutput::File | RecordingOutput::Unspecified => "file",
    };

    let rec_id = new_id();
    let key = format!("recordings/{room_id}/{rec_id}.mp4");
    let egress_id = state
        .livekit
        .start_room_composite_egress(&lk_name, &state.config.s3.recordings_bucket, &key)
        .await
        .map_err(|e| {
            crate::metrics::metrics()
                .egress_total
                .with_label_values(&["fail"])
                .inc();
            Status::from(e)
        })?;
    crate::metrics::metrics()
        .egress_total
        .with_label_values(&["start"])
        .inc();

    sqlx::query(
        "INSERT INTO recordings (id, room_id, egress_id, mode, output, status, started_by, storage_url, rtmp_url)
         VALUES ($1, $2, $3, $4, $5, 'starting', $6, $7, $8)",
    )
    .bind(rec_id)
    .bind(room_id)
    .bind(&egress_id)
    .bind(mode)
    .bind(output)
    .bind(&id.id)
    .bind(&key)
    .bind(&r.rtmp_url)
    .execute(&state.db)
    .await
    .map_err(|e| Status::internal(e.to_string()))?;

    let rec = load(state, rec_id).await?;
    // Broadcast state.
    let msg = MeetServerMessage {
        payload: Some(meet_server_message::Payload::RecordingState(
            RecordingStateChanged {
                recording: Some(rec.clone()),
            },
        )),
    };
    state.hub.publish(&room_id.to_string(), msg);
    Ok(Response::new(rec))
}

/// Stop a recording.
pub async fn stop(
    state: &SharedState,
    req: Request<StopRecordingRequest>,
) -> Result<Response<Recording>, Status> {
    let id = identity(state, &req).await?;
    let r = req.into_inner();
    let rec_id = parse_uuid(&r.recording_id, "recording_id")?;
    let (room_id, egress_id): (Uuid, String) =
        sqlx::query_as("SELECT room_id, egress_id FROM recordings WHERE id = $1")
            .bind(rec_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("recording"))?;
    crate::authz::can(state, &id, "room", &room_id.to_string(), "moderator").await?;

    state
        .livekit
        .stop_egress(&egress_id)
        .await
        .map_err(Status::from)?;
    sqlx::query("UPDATE recordings SET status='stopping' WHERE id = $1")
        .bind(rec_id)
        .execute(&state.db)
        .await
        .map_err(|e| Status::internal(e.to_string()))?;
    Ok(Response::new(load(state, rec_id).await?))
}

/// List recordings for a room.
pub async fn list(
    state: &SharedState,
    req: Request<ListRecordingsRequest>,
) -> Result<Response<ListRecordingsResponse>, Status> {
    let id = identity(state, &req).await?;
    let r = req.into_inner();
    let room_id = parse_uuid(&r.room_id, "room_id")?;
    crate::authz::can(state, &id, "room", &room_id.to_string(), "participant").await?;

    let page = i64::from(if r.page_size == 0 {
        50
    } else {
        r.page_size.min(500)
    });
    let ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM recordings WHERE room_id = $1 AND deleted_at IS NULL
         ORDER BY started_at DESC LIMIT $2",
    )
    .bind(room_id)
    .bind(page)
    .fetch_all(&state.db)
    .await
    .map_err(|e| Status::internal(e.to_string()))?;
    let mut out = Vec::new();
    for i in ids {
        if let Ok(rec) = load(state, i).await {
            out.push(rec);
        }
    }
    Ok(Response::new(ListRecordingsResponse {
        recordings: out,
        next_page_token: String::new(),
    }))
}

/// Get a single recording.
pub async fn get(
    state: &SharedState,
    req: Request<GetRecordingRequest>,
) -> Result<Response<Recording>, Status> {
    let id = identity(state, &req).await?;
    let r = req.into_inner();
    let rec_id = parse_uuid(&r.recording_id, "recording_id")?;
    let room_id: Uuid = sqlx::query_scalar("SELECT room_id FROM recordings WHERE id = $1")
        .bind(rec_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| Status::internal(e.to_string()))?
        .ok_or_else(|| Status::not_found("recording"))?;
    crate::authz::can(state, &id, "room", &room_id.to_string(), "participant").await?;
    Ok(Response::new(load(state, rec_id).await?))
}

/// Delete (soft) a recording.
pub async fn delete(
    state: &SharedState,
    req: Request<DeleteRecordingRequest>,
) -> Result<Response<DeleteRecordingResponse>, Status> {
    let id = identity(state, &req).await?;
    let r = req.into_inner();
    let rec_id = parse_uuid(&r.recording_id, "recording_id")?;
    let room_id: Uuid = sqlx::query_scalar("SELECT room_id FROM recordings WHERE id = $1")
        .bind(rec_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| Status::internal(e.to_string()))?
        .ok_or_else(|| Status::not_found("recording"))?;
    crate::authz::can(state, &id, "room", &room_id.to_string(), "moderator").await?;

    sqlx::query("UPDATE recordings SET deleted_at = NOW() WHERE id = $1")
        .bind(rec_id)
        .execute(&state.db)
        .await
        .map_err(|e| Status::internal(e.to_string()))?;
    Ok(Response::new(DeleteRecordingResponse {}))
}

async fn load(state: &SharedState, id: Uuid) -> Result<Recording, Status> {
    let row = sqlx::query_as::<_, Row>(
        "SELECT id, room_id, egress_id, mode, output, status, started_by, storage_url, rtmp_url,
                duration_ms, size_bytes, started_at, ended_at
         FROM recordings WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| Status::internal(e.to_string()))?
    .ok_or_else(|| Status::not_found("recording"))?;

    Ok(Recording {
        id: row.id.to_string(),
        room_id: row.room_id.to_string(),
        status: match row.status.as_str() {
            "starting" => RecordingStatus::Starting as i32,
            "active" => RecordingStatus::Active as i32,
            "stopping" => RecordingStatus::Stopping as i32,
            "stopped" => RecordingStatus::Stopped as i32,
            "saved" => RecordingStatus::Saved as i32,
            "failed" => RecordingStatus::Failed as i32,
            _ => RecordingStatus::Unspecified as i32,
        },
        mode: match row.mode.as_str() {
            "composite" => RecordingMode::Composite as i32,
            "speaker" => RecordingMode::Speaker as i32,
            "audio_only" => RecordingMode::AudioOnly as i32,
            _ => RecordingMode::Unspecified as i32,
        },
        output: match row.output.as_str() {
            "file" => RecordingOutput::File as i32,
            "hls" => RecordingOutput::Hls as i32,
            "rtmp" => RecordingOutput::Rtmp as i32,
            _ => RecordingOutput::Unspecified as i32,
        },
        started_by: row.started_by,
        duration: Some(prost_types::Duration {
            seconds: row.duration_ms / 1000,
            nanos: ((row.duration_ms % 1000) * 1_000_000) as i32,
        }),
        size_bytes: row.size_bytes as u64,
        storage_path: row.storage_url.unwrap_or_default(),
        download_url: String::new(),
        rtmp_url: row.rtmp_url.unwrap_or_default(),
        egress_id: row.egress_id,
        started_at: Some(rooms::ts(row.started_at)),
        ended_at: row.ended_at.map(rooms::ts),
    })
}

#[derive(sqlx::FromRow)]
struct Row {
    id: Uuid,
    room_id: Uuid,
    egress_id: String,
    mode: String,
    output: String,
    status: String,
    started_by: String,
    storage_url: Option<String>,
    rtmp_url: Option<String>,
    duration_ms: i64,
    size_bytes: i64,
    started_at: chrono::DateTime<chrono::Utc>,
    ended_at: Option<chrono::DateTime<chrono::Utc>>,
}
