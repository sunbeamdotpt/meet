//! Typed CRUD for the `rooms` table.
//!
//! Callers pass strongly-typed enums (`RoomAccessLevel`, `RoomStatus`,
//! `VideoQualityPreset`) rather than raw strings; we translate at the SQL
//! boundary.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use sunbeam_meet_proto::meet::v1::{RoomAccessLevel, RoomStatus, VideoQualityPreset};
use thiserror::Error;
use uuid::Uuid;

/// Typed room row.
#[derive(Debug, Clone)]
pub struct Room {
    /// UUID v7 primary key.
    pub id: Uuid,
    /// URL-safe slug.
    pub slug: String,
    /// Display name.
    pub display_name: String,
    /// Access level.
    pub access_level: RoomAccessLevel,
    /// Status.
    pub status: RoomStatus,
    /// Max participants.
    pub max_participants: i32,
    /// Default video quality preset.
    pub default_quality: VideoQualityPreset,
    /// Waiting-room flag.
    pub waiting_room_enabled: bool,
    /// Chat flag.
    pub chat_enabled: bool,
    /// Recording-allowed flag.
    pub recording_allowed: bool,
    /// Creator identity id.
    pub created_by: String,
    /// LiveKit internal room name.
    pub livekit_room_name: String,
    /// Free-form metadata.
    pub metadata: BTreeMap<String, String>,
    /// Created-at.
    pub created_at: DateTime<Utc>,
    /// Started-at (first participant).
    pub started_at: Option<DateTime<Utc>>,
    /// Ended-at.
    pub ended_at: Option<DateTime<Utc>>,
}

/// Input to [`create`].
#[derive(Debug, Clone)]
pub struct NewRoom {
    /// Slug (unique).
    pub slug: String,
    /// Display name.
    pub display_name: String,
    /// Access level.
    pub access_level: RoomAccessLevel,
    /// Max participants.
    pub max_participants: i32,
    /// Video quality default.
    pub default_quality: VideoQualityPreset,
    /// Waiting room on/off.
    pub waiting_room_enabled: bool,
    /// Chat on/off.
    pub chat_enabled: bool,
    /// Recording allowed.
    pub recording_allowed: bool,
    /// Creator identity id.
    pub created_by: String,
    /// Free-form metadata.
    pub metadata: BTreeMap<String, String>,
}

/// Partial update (fields set to `Some` are written; `None` leaves as-is).
#[derive(Debug, Clone, Default)]
pub struct RoomUpdate {
    /// New display name.
    pub display_name: Option<String>,
    /// New access level.
    pub access_level: Option<RoomAccessLevel>,
    /// New max participants.
    pub max_participants: Option<i32>,
    /// New default quality.
    pub default_quality: Option<VideoQualityPreset>,
    /// New waiting-room flag.
    pub waiting_room_enabled: Option<bool>,
    /// New chat flag.
    pub chat_enabled: Option<bool>,
    /// New recording-allowed flag.
    pub recording_allowed: Option<bool>,
}

/// Filter for [`list`].
#[derive(Debug, Clone, Default)]
pub struct ListFilter {
    /// If set, only return rooms with this status.
    pub status: Option<RoomStatus>,
    /// Page size (defaults to 50).
    pub page_size: Option<u32>,
    /// Page token (opaque, currently a UUID cursor).
    pub page_token: Option<String>,
}

/// One page of rooms.
#[derive(Debug, Clone)]
pub struct RoomPage {
    /// Rooms on this page.
    pub rooms: Vec<Room>,
    /// Next page token, if another page exists.
    pub next_page_token: Option<String>,
}

/// Errors returned by this module.
#[derive(Debug, Error)]
pub enum StoreError {
    /// Unique constraint violation (e.g. slug already taken).
    #[error("conflict: {reason}")]
    Conflict {
        /// Human-readable reason.
        reason: String,
    },
    /// Raw database error.
    #[error("database: {0}")]
    Database(#[from] sqlx::Error),
}

fn access_to_str(a: RoomAccessLevel) -> &'static str {
    match a {
        RoomAccessLevel::Unspecified | RoomAccessLevel::Public => "public",
        RoomAccessLevel::Trusted => "trusted",
        RoomAccessLevel::Restricted => "restricted",
    }
}

fn access_from_str(s: &str) -> RoomAccessLevel {
    match s {
        "trusted" => RoomAccessLevel::Trusted,
        "restricted" => RoomAccessLevel::Restricted,
        _ => RoomAccessLevel::Public,
    }
}

fn status_to_str(s: RoomStatus) -> &'static str {
    match s {
        RoomStatus::Unspecified | RoomStatus::Waiting => "waiting",
        RoomStatus::Active => "active",
        RoomStatus::Ended => "ended",
    }
}

fn status_from_str(s: &str) -> RoomStatus {
    match s {
        "active" => RoomStatus::Active,
        "ended" => RoomStatus::Ended,
        _ => RoomStatus::Waiting,
    }
}

fn quality_to_str(q: VideoQualityPreset) -> &'static str {
    match q {
        VideoQualityPreset::Unspecified | VideoQualityPreset::Auto => "auto",
        VideoQualityPreset::Low => "low",
        VideoQualityPreset::Medium => "medium",
        VideoQualityPreset::High => "high",
        VideoQualityPreset::Ultra => "ultra",
    }
}

fn quality_from_str(s: &str) -> VideoQualityPreset {
    match s {
        "low" => VideoQualityPreset::Low,
        "medium" => VideoQualityPreset::Medium,
        "high" => VideoQualityPreset::High,
        "ultra" => VideoQualityPreset::Ultra,
        _ => VideoQualityPreset::Auto,
    }
}

#[derive(sqlx::FromRow)]
struct Row {
    id: Uuid,
    slug: String,
    display_name: String,
    access_level: String,
    status: String,
    max_participants: i32,
    default_quality: String,
    waiting_room_enabled: bool,
    chat_enabled: bool,
    recording_allowed: bool,
    created_by: String,
    livekit_room_name: String,
    metadata: sqlx::types::Json<BTreeMap<String, String>>,
    created_at: DateTime<Utc>,
    started_at: Option<DateTime<Utc>>,
    ended_at: Option<DateTime<Utc>>,
}

impl From<Row> for Room {
    fn from(r: Row) -> Self {
        Room {
            id: r.id,
            slug: r.slug,
            display_name: r.display_name,
            access_level: access_from_str(&r.access_level),
            status: status_from_str(&r.status),
            max_participants: r.max_participants,
            default_quality: quality_from_str(&r.default_quality),
            waiting_room_enabled: r.waiting_room_enabled,
            chat_enabled: r.chat_enabled,
            recording_allowed: r.recording_allowed,
            created_by: r.created_by,
            livekit_room_name: r.livekit_room_name,
            metadata: r.metadata.0,
            created_at: r.created_at,
            started_at: r.started_at,
            ended_at: r.ended_at,
        }
    }
}

const SELECT_COLS: &str = "id, slug, display_name, access_level, status, max_participants,\
                           default_quality, waiting_room_enabled, chat_enabled, recording_allowed,\
                           created_by, livekit_room_name, metadata, created_at, started_at, ended_at";

/// Insert a new room. Returns the created row.
pub async fn create(pool: &PgPool, new: NewRoom) -> Result<Room, StoreError> {
    let id = Uuid::now_v7();
    let lk_name = format!("room-{id}");
    let metadata = sqlx::types::Json(new.metadata);
    let row = sqlx::query_as::<_, Row>(&format!(
        "INSERT INTO rooms (
            id, slug, display_name, access_level, status, max_participants,
            default_quality, waiting_room_enabled, chat_enabled, recording_allowed,
            created_by, livekit_room_name, metadata, created_at
        ) VALUES (
            $1, $2, $3, $4, 'waiting', $5, $6, $7, $8, $9, $10, $11, $12::jsonb, NOW()
        ) RETURNING {SELECT_COLS}"
    ))
    .bind(id)
    .bind(&new.slug)
    .bind(&new.display_name)
    .bind(access_to_str(new.access_level))
    .bind(new.max_participants)
    .bind(quality_to_str(new.default_quality))
    .bind(new.waiting_room_enabled)
    .bind(new.chat_enabled)
    .bind(new.recording_allowed)
    .bind(&new.created_by)
    .bind(&lk_name)
    .bind(metadata)
    .fetch_one(pool)
    .await
    .map_err(map_insert_error)?;
    Ok(row.into())
}

fn map_insert_error(e: sqlx::Error) -> StoreError {
    if let sqlx::Error::Database(db_err) = &e {
        if db_err.is_unique_violation() {
            return StoreError::Conflict {
                reason: db_err.constraint().unwrap_or("unique").to_owned(),
            };
        }
    }
    StoreError::Database(e)
}

/// Fetch by id.
pub async fn get(pool: &PgPool, id: &Uuid) -> Result<Option<Room>, StoreError> {
    let row = sqlx::query_as::<_, Row>(&format!("SELECT {SELECT_COLS} FROM rooms WHERE id = $1"))
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(Into::into))
}

/// List rooms.
pub async fn list(pool: &PgPool, filter: ListFilter) -> Result<RoomPage, StoreError> {
    let limit = i64::from(filter.page_size.unwrap_or(50).min(500));
    let status_filter = filter.status.map(status_to_str);
    let rows: Vec<Row> = if let Some(st) = status_filter {
        sqlx::query_as::<_, Row>(&format!(
            "SELECT {SELECT_COLS} FROM rooms WHERE status = $1 ORDER BY created_at DESC LIMIT $2"
        ))
        .bind(st)
        .bind(limit)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as::<_, Row>(&format!(
            "SELECT {SELECT_COLS} FROM rooms ORDER BY created_at DESC LIMIT $1"
        ))
        .bind(limit)
        .fetch_all(pool)
        .await?
    };
    Ok(RoomPage {
        rooms: rows.into_iter().map(Into::into).collect(),
        next_page_token: None,
    })
}

/// Patch. Only fields with `Some(_)` are updated.
pub async fn update(pool: &PgPool, id: &Uuid, patch: RoomUpdate) -> Result<Room, StoreError> {
    // Fetch existing, merge, write.
    let current = get(pool, id).await?.ok_or_else(|| StoreError::Conflict {
        reason: format!("room {id} not found"),
    })?;
    let display_name = patch.display_name.unwrap_or(current.display_name);
    let access_level = patch.access_level.unwrap_or(current.access_level);
    let max_participants = patch.max_participants.unwrap_or(current.max_participants);
    let default_quality = patch.default_quality.unwrap_or(current.default_quality);
    let waiting_room_enabled = patch
        .waiting_room_enabled
        .unwrap_or(current.waiting_room_enabled);
    let chat_enabled = patch.chat_enabled.unwrap_or(current.chat_enabled);
    let recording_allowed = patch.recording_allowed.unwrap_or(current.recording_allowed);

    let row = sqlx::query_as::<_, Row>(&format!(
        "UPDATE rooms SET display_name = $2, access_level = $3, max_participants = $4,
            default_quality = $5, waiting_room_enabled = $6, chat_enabled = $7,
            recording_allowed = $8
         WHERE id = $1 RETURNING {SELECT_COLS}"
    ))
    .bind(id)
    .bind(&display_name)
    .bind(access_to_str(access_level))
    .bind(max_participants)
    .bind(quality_to_str(default_quality))
    .bind(waiting_room_enabled)
    .bind(chat_enabled)
    .bind(recording_allowed)
    .fetch_one(pool)
    .await?;
    Ok(row.into())
}

/// Mark a room as ended.
pub async fn end(pool: &PgPool, id: &Uuid, _reason: &str) -> Result<Room, StoreError> {
    let row = sqlx::query_as::<_, Row>(&format!(
        "UPDATE rooms SET status = 'ended', ended_at = NOW() WHERE id = $1 RETURNING {SELECT_COLS}"
    ))
    .bind(id)
    .fetch_one(pool)
    .await?;
    Ok(row.into())
}

/// Hard-delete (tests only). Production code calls [`end`].
pub async fn hard_delete(pool: &PgPool, id: &Uuid) -> Result<(), StoreError> {
    sqlx::query("DELETE FROM rooms WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}
