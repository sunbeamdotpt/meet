//! Room entity.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

/// Maximum slug length (bytes / ASCII chars).
pub const SLUG_MAX_LEN: usize = 255;

/// Slug validation errors.
#[derive(Debug, Error)]
pub enum SlugError {
    /// Empty slug.
    #[error("slug must not be empty")]
    Empty,
    /// Slug contains an unsupported character.
    #[error("slug contains invalid character: {0:?}")]
    InvalidCharacter(char),
    /// Slug begins or ends with a hyphen.
    #[error("slug must not begin or end with a hyphen")]
    LeadingOrTrailingHyphen,
    /// Slug contains two or more consecutive hyphens.
    #[error("slug must not contain consecutive hyphens")]
    ConsecutiveHyphens,
    /// Slug exceeds the maximum length.
    #[error("slug is too long: {0} bytes")]
    TooLong(usize),
}

/// Validate a room slug per DESIGN §5:
/// lowercase ASCII letters, digits, and hyphens; no leading/trailing hyphen;
/// no consecutive hyphens; length 1..=[`SLUG_MAX_LEN`].
///
/// Returns `Err(SlugError)` on any violation; `Ok(())` otherwise.
pub fn validate_slug(slug: &str) -> Result<(), SlugError> {
    if slug.is_empty() {
        return Err(SlugError::Empty);
    }
    if slug.len() > SLUG_MAX_LEN {
        return Err(SlugError::TooLong(slug.len()));
    }
    let bytes = slug.as_bytes();
    if bytes[0] == b'-' || bytes[bytes.len() - 1] == b'-' {
        return Err(SlugError::LeadingOrTrailingHyphen);
    }
    let mut prev_hyphen = false;
    for &b in bytes {
        let c = b as char;
        let ok = c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-';
        if !ok {
            return Err(SlugError::InvalidCharacter(c));
        }
        if c == '-' {
            if prev_hyphen {
                return Err(SlugError::ConsecutiveHyphens);
            }
            prev_hyphen = true;
        } else {
            prev_hyphen = false;
        }
    }
    Ok(())
}

/// Persisted room row.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Room {
    /// UUID v7 primary key.
    pub id: Uuid,
    /// URL-safe slug.
    pub slug: String,
    /// Human display name.
    pub display_name: String,
    /// Access level ('public'|'trusted'|'restricted').
    pub access_level: String,
    /// Status ('waiting'|'active'|'ended').
    pub status: String,
    /// Max participants.
    pub max_participants: i32,
    /// Default video quality preset.
    pub default_quality: String,
    /// Waiting room flag.
    pub waiting_room_enabled: bool,
    /// Chat flag.
    pub chat_enabled: bool,
    /// Recording allowed flag.
    pub recording_allowed: bool,
    /// Creator identity.
    pub created_by: String,
    /// LiveKit internal room name.
    pub livekit_room_name: String,
    /// JSONB metadata bag.
    pub metadata: sqlx::types::Json<Metadata>,
    /// Created-at.
    pub created_at: DateTime<Utc>,
    /// Started-at (first participant).
    pub started_at: Option<DateTime<Utc>>,
    /// Ended-at.
    pub ended_at: Option<DateTime<Utc>>,
}

/// Free-form metadata map.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Metadata(pub std::collections::BTreeMap<String, String>);
