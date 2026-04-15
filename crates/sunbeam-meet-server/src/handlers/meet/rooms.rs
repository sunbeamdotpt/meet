//! Room CRUD RPCs.
//
// Proto request/response types are imported wholesale; naming each one is noise.
#![allow(clippy::wildcard_imports)]

use prost_types::Timestamp;
use sunbeam_meet_proto::meet::v1::*;
use tonic::{Request, Response, Status};
use uuid::Uuid;

use crate::handlers::meet::{identity, parse_uuid};
use crate::state::SharedState;
use crate::storage::pg::new_id;
use crate::storage::pg::rooms::{PgRoomAccessLevel, PgRoomStatus};

/// Create a new room.
pub async fn create(
    state: &SharedState,
    req: Request<CreateRoomRequest>,
) -> Result<Response<Room>, Status> {
    let identity = identity(state, &req).await?;
    let r = req.into_inner();
    if r.display_name.trim().is_empty() {
        return Err(Status::invalid_argument("display_name required"));
    }
    let id = new_id();
    let slug = slugify(&r.display_name);
    let livekit_room_name = format!("meet-{}", id.simple());
    let max = if r.max_participants == 0 {
        300
    } else {
        r.max_participants as i32
    };
    let access = enum_access(r.access_level);
    let quality = enum_quality(r.default_quality);
    let metadata_json = serde_json::to_value(&r.metadata).unwrap_or_default();

    // Create in LiveKit first; if that fails we never commit DB row.
    state
        .livekit
        .create_room(&livekit_room_name, max as u32)
        .await
        .map_err(Status::from)?;

    sqlx::query(
        "INSERT INTO rooms (id, slug, display_name, access_level, status, max_participants,
                            default_quality, waiting_room_enabled, chat_enabled, recording_allowed,
                            created_by, livekit_room_name, metadata)
         VALUES ($1, $2, $3, $4, 'waiting', $5, $6, $7, $8, $9, $10, $11, $12::jsonb)",
    )
    .bind(id)
    .bind(&slug)
    .bind(&r.display_name)
    .bind(access)
    .bind(max)
    .bind(quality)
    .bind(r.waiting_room_enabled)
    .bind(r.chat_enabled)
    .bind(r.recording_allowed)
    .bind(&identity.id)
    .bind(&livekit_room_name)
    .bind(&metadata_json)
    .execute(&state.db)
    .await
    .map_err(|e| Status::internal(e.to_string()))?;

    // Grant creator as owner in Keto.
    let _ = crate::authz::grant(state, "room", &id.to_string(), "owner", &identity.id).await;

    Ok(Response::new(load_room(state, id).await?))
}

/// Get a room.
pub async fn get(
    state: &SharedState,
    req: Request<GetRoomRequest>,
) -> Result<Response<Room>, Status> {
    let identity = identity(state, &req).await?;
    let id = parse_uuid(&req.into_inner().room_id, "room_id")?;
    crate::authz::can(state, &identity, "room", &id.to_string(), "participant").await?;
    Ok(Response::new(load_room(state, id).await?))
}

/// List rooms.
pub async fn list(
    state: &SharedState,
    req: Request<ListRoomsRequest>,
) -> Result<Response<ListRoomsResponse>, Status> {
    let _identity = identity(state, &req).await?;
    let r = req.into_inner();
    let page_size = i64::from(if r.page_size == 0 {
        50
    } else {
        r.page_size.min(500)
    });

    let status_filter =
        match RoomStatus::try_from(r.status_filter).unwrap_or(RoomStatus::Unspecified) {
            RoomStatus::Waiting => Some(PgRoomStatus::Waiting),
            RoomStatus::Active => Some(PgRoomStatus::Active),
            RoomStatus::Ended => Some(PgRoomStatus::Ended),
            // Unspecified means "no filter".
            RoomStatus::Unspecified => None,
        };

    let ids: Vec<Uuid> = if let Some(f) = status_filter {
        sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM rooms WHERE deleted_at IS NULL AND status = $1
             ORDER BY created_at DESC LIMIT $2",
        )
        .bind(f)
        .bind(page_size)
        .fetch_all(&state.db)
        .await
    } else {
        sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM rooms WHERE deleted_at IS NULL ORDER BY created_at DESC LIMIT $1",
        )
        .bind(page_size)
        .fetch_all(&state.db)
        .await
    }
    .map_err(|e| Status::internal(e.to_string()))?;

    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rooms WHERE deleted_at IS NULL")
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);

    let mut rooms = Vec::with_capacity(ids.len());
    for id in ids {
        if let Ok(r) = load_room(state, id).await {
            rooms.push(r);
        }
    }

    Ok(Response::new(ListRoomsResponse {
        rooms,
        next_page_token: String::new(),
        total_count: u32::try_from(total).unwrap_or(0),
    }))
}

/// Update a room.
pub async fn update(
    state: &SharedState,
    req: Request<UpdateRoomRequest>,
) -> Result<Response<Room>, Status> {
    let identity = identity(state, &req).await?;
    let r = req.into_inner();
    let id = parse_uuid(&r.room_id, "room_id")?;
    crate::authz::can(state, &identity, "room", &id.to_string(), "moderator").await?;

    if let Some(dn) = &r.display_name {
        sqlx::query("UPDATE rooms SET display_name = $1 WHERE id = $2")
            .bind(dn)
            .bind(id)
            .execute(&state.db)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
    }
    if let Some(al) = r.access_level {
        sqlx::query("UPDATE rooms SET access_level = $1 WHERE id = $2")
            .bind(enum_access(al))
            .bind(id)
            .execute(&state.db)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
    }
    if let Some(mx) = r.max_participants {
        sqlx::query("UPDATE rooms SET max_participants = $1 WHERE id = $2")
            .bind(mx as i32)
            .bind(id)
            .execute(&state.db)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
    }
    if let Some(w) = r.waiting_room_enabled {
        sqlx::query("UPDATE rooms SET waiting_room_enabled = $1 WHERE id = $2")
            .bind(w)
            .bind(id)
            .execute(&state.db)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
    }
    if let Some(c) = r.chat_enabled {
        sqlx::query("UPDATE rooms SET chat_enabled = $1 WHERE id = $2")
            .bind(c)
            .bind(id)
            .execute(&state.db)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
    }
    if let Some(rec) = r.recording_allowed {
        sqlx::query("UPDATE rooms SET recording_allowed = $1 WHERE id = $2")
            .bind(rec)
            .bind(id)
            .execute(&state.db)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
    }

    Ok(Response::new(load_room(state, id).await?))
}

/// End a room.
pub async fn end(
    state: &SharedState,
    req: Request<EndRoomRequest>,
) -> Result<Response<EndRoomResponse>, Status> {
    let identity = identity(state, &req).await?;
    let r = req.into_inner();
    let id = parse_uuid(&r.room_id, "room_id")?;
    crate::authz::can(state, &identity, "room", &id.to_string(), "moderator").await?;

    let lk_name: String = sqlx::query_scalar("SELECT livekit_room_name FROM rooms WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| Status::internal(e.to_string()))?
        .ok_or_else(|| Status::not_found("room"))?;
    let _ = state.livekit.delete_room(&lk_name).await;

    sqlx::query("UPDATE rooms SET status='ended', ended_at=NOW() WHERE id=$1")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(|e| Status::internal(e.to_string()))?;

    // Broadcast room-ended to any connected participants.
    let msg = MeetServerMessage {
        payload: Some(meet_server_message::Payload::RoomEnded(RoomEnded {
            reason: "ended_by_host".into(),
            ended_by: identity.id.clone(),
        })),
    };
    state.hub.publish(&id.to_string(), msg);

    Ok(Response::new(EndRoomResponse {}))
}

/// Load a full Room proto from its UUID.
pub async fn load_room(state: &SharedState, id: Uuid) -> Result<Room, Status> {
    let row = sqlx::query_as::<_, RoomRow>(
        "SELECT id, slug, display_name, access_level, status, max_participants, default_quality,
                waiting_room_enabled, chat_enabled, recording_allowed, created_by, livekit_room_name,
                metadata, created_at, ended_at
         FROM rooms WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| Status::internal(e.to_string()))?
    .ok_or_else(|| Status::not_found("room"))?;

    let metadata: std::collections::HashMap<String, String> =
        serde_json::from_value(row.metadata).unwrap_or_default();

    Ok(Room {
        id: row.id.to_string(),
        name: row.slug,
        display_name: row.display_name,
        access_level: RoomAccessLevel::from(row.access_level) as i32,
        status: RoomStatus::from(row.status) as i32,
        max_participants: u32::try_from(row.max_participants).unwrap_or(0),
        default_quality: quality_from_str(&row.default_quality),
        waiting_room_enabled: row.waiting_room_enabled,
        chat_enabled: row.chat_enabled,
        recording_allowed: row.recording_allowed,
        captioning_available: true,
        created_by: row.created_by,
        metadata,
        participants: Vec::new(),
        active_recording_status: RecordingStatus::Unspecified as i32,
        captioning_state: CaptioningState::Unspecified as i32,
        created_at: Some(ts(row.created_at)),
        ended_at: row.ended_at.map(ts),
        livekit_room_name: row.livekit_room_name,
    })
}

#[derive(sqlx::FromRow)]
struct RoomRow {
    id: Uuid,
    slug: String,
    display_name: String,
    access_level: PgRoomAccessLevel,
    status: PgRoomStatus,
    max_participants: i32,
    default_quality: String,
    waiting_room_enabled: bool,
    chat_enabled: bool,
    recording_allowed: bool,
    created_by: String,
    livekit_room_name: String,
    metadata: serde_json::Value,
    created_at: chrono::DateTime<chrono::Utc>,
    ended_at: Option<chrono::DateTime<chrono::Utc>>,
}

fn slugify(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .chars()
        .take(48)
        .collect::<String>()
        + "-"
        + &Uuid::new_v4().simple().to_string()[..8]
}

fn enum_access(v: i32) -> PgRoomAccessLevel {
    match RoomAccessLevel::try_from(v).unwrap_or(RoomAccessLevel::Unspecified) {
        RoomAccessLevel::Public => PgRoomAccessLevel::Public,
        RoomAccessLevel::Restricted => PgRoomAccessLevel::Restricted,
        // Trusted is the default for Unspecified per legacy semantics.
        RoomAccessLevel::Trusted | RoomAccessLevel::Unspecified => PgRoomAccessLevel::Trusted,
    }
}

fn enum_quality(v: i32) -> &'static str {
    match VideoQualityPreset::try_from(v).unwrap_or(VideoQualityPreset::Unspecified) {
        VideoQualityPreset::Low => "low",
        VideoQualityPreset::Medium => "medium",
        VideoQualityPreset::High => "high",
        VideoQualityPreset::Ultra => "ultra",
        _ => "auto",
    }
}

fn quality_from_str(s: &str) -> i32 {
    match s {
        "low" => VideoQualityPreset::Low as i32,
        "medium" => VideoQualityPreset::Medium as i32,
        "high" => VideoQualityPreset::High as i32,
        "ultra" => VideoQualityPreset::Ultra as i32,
        "auto" => VideoQualityPreset::Auto as i32,
        _ => VideoQualityPreset::Unspecified as i32,
    }
}

/// Convert chrono -> prost_types::Timestamp.
pub fn ts(dt: chrono::DateTime<chrono::Utc>) -> Timestamp {
    Timestamp {
        seconds: dt.timestamp(),
        nanos: dt.timestamp_subsec_nanos() as i32,
    }
}
