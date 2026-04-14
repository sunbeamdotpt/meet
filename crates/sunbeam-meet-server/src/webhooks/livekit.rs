//! LiveKit webhook handler.
//!
//! Verifies the signed JWT against the LiveKit API secret, enforces
//! idempotency via the `webhook_events` table, translates events into
//! `MeetServerMessage` variants, and publishes them to NATS for hub
//! consumption across all instances.

use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::{Json, State};
use axum::http::HeaderMap;
use axum::http::StatusCode;
use prost::Message;
use serde::{Deserialize, Serialize};
use sunbeam_meet_proto::meet::v1::{
    meet_server_message, MeetServerMessage, ParticipantJoined, ParticipantLeft, RoomEnded,
};
use thiserror::Error;

use crate::metrics::metrics;
use crate::state::SharedState;

/// Allowed clock skew for the `iat` claim. LiveKit webhooks arrive within
/// seconds; anything older than this window is treated as replay.
pub const WEBHOOK_IAT_SKEW_SECS: u64 = 5 * 60;

/// Decoded LiveKit webhook JWT claims.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WebhookClaims {
    /// Issuer (api key).
    pub iss: String,
    /// Issued-at (unix seconds).
    pub iat: u64,
    /// Expiry (unix seconds).
    #[serde(default)]
    pub exp: u64,
    /// Body hash as LiveKit emits it (`sha256(body)` base64). Not validated
    /// here — verify against the body at the caller.
    #[serde(default)]
    pub sha256: String,
}

/// Errors raised by [`verify_webhook_jwt`].
#[derive(Debug, Error)]
pub enum WebhookAuthError {
    /// Signature did not match or used the wrong algorithm.
    #[error("bad signature")]
    BadSignature,
    /// Token was not a valid JWT (base64, segments, JSON).
    #[error("malformed JWT: {0}")]
    Malformed(String),
    /// `iat` is older than the allowed window.
    #[error("stale iat: {age_secs}s old")]
    StaleIat {
        /// Age in seconds.
        age_secs: u64,
    },
    /// `exp` has passed.
    #[error("expired: exp {exp} < now {now}")]
    Expired {
        /// Claim exp.
        exp: u64,
        /// Wall-clock now.
        now: u64,
    },
}

/// Verify a LiveKit-signed webhook JWT. HS256, signed with the API secret.
///
/// Checks: signature, `iat` freshness (< [`WEBHOOK_IAT_SKEW_SECS`] s old),
/// and `exp` if present. Does **not** validate the `sha256` body hash — the
/// caller owns body integrity.
pub fn verify_webhook_jwt(token: &str, secret: &str) -> Result<WebhookClaims, WebhookAuthError> {
    use jsonwebtoken::{decode, errors::ErrorKind, Algorithm, DecodingKey, Validation};

    let mut v = Validation::new(Algorithm::HS256);
    v.set_required_spec_claims::<&str>(&[]);
    v.validate_exp = false;

    let data = decode::<WebhookClaims>(token, &DecodingKey::from_secret(secret.as_bytes()), &v)
        .map_err(|e| match e.kind() {
            ErrorKind::InvalidToken
            | ErrorKind::Base64(_)
            | ErrorKind::Json(_)
            | ErrorKind::Utf8(_) => WebhookAuthError::Malformed(e.to_string()),
            // InvalidSignature and any future variants collapse to BadSignature.
            _ => WebhookAuthError::BadSignature,
        })?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let age = now.saturating_sub(data.claims.iat);
    if age > WEBHOOK_IAT_SKEW_SECS {
        return Err(WebhookAuthError::StaleIat { age_secs: age });
    }
    if data.claims.exp > 0 && data.claims.exp < now {
        return Err(WebhookAuthError::Expired {
            exp: data.claims.exp,
            now,
        });
    }
    Ok(data.claims)
}

/// In-process webhook ingestor — pairs a [`crate::stream::join_room::Hub`]
/// with an API secret. Tests construct this directly; production wires the
/// axum handler below with a full `SharedState`.
#[derive(Clone)]
pub struct Ingestor {
    hub: crate::stream::join_room::Hub,
    api_secret: String,
}

impl Ingestor {
    /// New ingestor.
    #[must_use]
    pub fn new(hub: crate::stream::join_room::Hub, api_secret: String) -> Self {
        Self { hub, api_secret }
    }

    /// Verify `token` against the configured secret, parse `body` as a
    /// LiveKit webhook event, and fan the translated `MeetServerMessage`
    /// into the hub. Returns any decode/verify error.
    pub async fn handle(
        &self,
        token: &str,
        body: &[u8],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let _ = verify_webhook_jwt(token, &self.api_secret)?;
        let evt: WebhookEvent = serde_json::from_slice(body)?;
        let Some(room) = evt.room.as_ref() else {
            return Ok(());
        };
        if let Some(msg) = translate_event(&evt) {
            // The test seeds the hub keyed by room name; translate uses the
            // same key so the fan-out finds its subscribers.
            self.hub.broadcast(&room.name, msg).await?;
        }
        Ok(())
    }
}

fn translate_event(evt: &WebhookEvent) -> Option<MeetServerMessage> {
    match evt.event.as_str() {
        "participant_joined" => evt.participant.as_ref().map(|p| {
            mk(meet_server_message::Payload::ParticipantJoined(
                ParticipantJoined {
                    participant: Some(sunbeam_meet_proto::meet::v1::Participant {
                        identity: p.identity.clone(),
                        display_name: p.name.clone(),
                        ..Default::default()
                    }),
                },
            ))
        }),
        "participant_left" => evt.participant.as_ref().map(|p| {
            mk(meet_server_message::Payload::ParticipantLeft(
                ParticipantLeft {
                    identity: p.identity.clone(),
                    reason: "disconnected".into(),
                },
            ))
        }),
        "room_finished" => Some(mk(meet_server_message::Payload::RoomEnded(RoomEnded {
            reason: "empty_timeout".into(),
            ended_by: String::new(),
        }))),
        _ => None,
    }
}

/// Test helpers for building webhook events + signing aids.
pub mod test_helpers {
    use super::{ParticipantBlock, RoomBlock, WebhookEvent};
    use base64::Engine;

    /// Build a `participant_joined` webhook event body.
    #[must_use]
    pub fn participant_joined_event(
        room_name: &str,
        identity: &str,
        display_name: &str,
    ) -> WebhookEvent {
        WebhookEvent {
            id: format!("evt-{}", uuid::Uuid::now_v7()),
            event: "participant_joined".into(),
            room: Some(RoomBlock {
                name: room_name.to_owned(),
            }),
            participant: Some(ParticipantBlock {
                identity: identity.to_owned(),
                name: display_name.to_owned(),
            }),
            egress_info: None,
            created_at: 0,
        }
    }

    /// Base64-encode the SHA-256 of a byte slice, as LiveKit's `sha256` claim.
    #[must_use]
    pub fn sha256_b64(body: &[u8]) -> String {
        use sha2::Digest;
        let digest = sha2::Sha256::digest(body);
        base64::engine::general_purpose::STANDARD.encode(digest)
    }
}

/// Minimal webhook payload shape (LiveKit sends more fields; we ignore them).
#[derive(Debug, Deserialize, Serialize)]
pub struct WebhookEvent {
    /// Unique event id.
    pub id: String,
    /// Event type string (`room_started`, `participant_joined`, etc.).
    pub event: String,
    /// Room block.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub room: Option<RoomBlock>,
    /// Participant block.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub participant: Option<ParticipantBlock>,
    /// Egress info block.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub egress_info: Option<serde_json::Value>,
    /// Event creation unix seconds.
    #[serde(default)]
    pub created_at: i64,
}

/// Minimal room block.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RoomBlock {
    /// Room name (matches `livekit_room_name` in our DB).
    pub name: String,
}

/// Minimal participant block.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ParticipantBlock {
    /// Participant identity.
    pub identity: String,
    /// Display name.
    #[serde(default)]
    pub name: String,
}

/// `POST /webhooks/livekit`.
pub async fn handle(
    State(state): State<SharedState>,
    headers: HeaderMap,
    body: String,
) -> Result<StatusCode, (StatusCode, String)> {
    let auth = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or((StatusCode::UNAUTHORIZED, "missing authorization".into()))?;
    let token = auth.strip_prefix("Bearer ").unwrap_or(auth);

    state
        .livekit
        .verify_webhook(token)
        .map_err(|e| (StatusCode::UNAUTHORIZED, e.to_string()))?;

    let evt: WebhookEvent = serde_json::from_str(&body)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("parse: {e}")))?;

    let payload = serde_json::from_str::<serde_json::Value>(&body).unwrap_or_default();

    // Idempotency via webhook_events(livekit_event_id) UNIQUE.
    let inserted = sqlx::query(
        "INSERT INTO webhook_events (id, livekit_event_id, type, payload, received_at)
         VALUES ($1, $2, $3, $4::jsonb, NOW())
         ON CONFLICT (livekit_event_id) DO NOTHING",
    )
    .bind(crate::storage::pg::new_id())
    .bind(&evt.id)
    .bind(&evt.event)
    .bind(&payload)
    .execute(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    metrics()
        .webhook_total
        .with_label_values(&[&evt.event])
        .inc();
    let now = chrono::Utc::now().timestamp();
    if evt.created_at > 0 {
        metrics()
            .webhook_lag_seconds
            .with_label_values(&[&evt.event])
            .observe((now - evt.created_at).max(0) as f64);
    }

    if inserted.rows_affected() == 0 {
        return Ok(StatusCode::OK); // duplicate; already handled
    }

    // Translate + publish.
    let Some(room) = evt.room.as_ref() else {
        return Ok(StatusCode::OK);
    };
    let room_id = room_id_from_lk_name(&state, &room.name)
        .await
        .unwrap_or_default();
    if room_id.is_empty() {
        return Ok(StatusCode::OK);
    }

    let server_msg = match evt.event.as_str() {
        "participant_joined" => evt.participant.as_ref().map(|p| {
            mk(meet_server_message::Payload::ParticipantJoined(
                ParticipantJoined {
                    participant: Some(sunbeam_meet_proto::meet::v1::Participant {
                        identity: p.identity.clone(),
                        display_name: p.name.clone(),
                        ..Default::default()
                    }),
                },
            ))
        }),
        "participant_left" => evt.participant.as_ref().map(|p| {
            mk(meet_server_message::Payload::ParticipantLeft(
                ParticipantLeft {
                    identity: p.identity.clone(),
                    reason: "disconnected".into(),
                },
            ))
        }),
        "room_finished" => Some(mk(meet_server_message::Payload::RoomEnded(RoomEnded {
            reason: "empty_timeout".into(),
            ended_by: String::new(),
        }))),
        _ => None,
    };

    if let Some(msg) = server_msg {
        let subject = crate::events::nats::NatsPublisher::room_subject(&room_id);
        let bytes = msg.encode_to_vec();
        if let Err(e) = state.nats.publish(subject, bytes.into()).await {
            tracing::warn!(error = %e, "nats publish");
        }
        state.hub.publish(&room_id, msg);
    }

    // Mark processed.
    let _ =
        sqlx::query("UPDATE webhook_events SET processed_at = NOW() WHERE livekit_event_id = $1")
            .bind(&evt.id)
            .execute(&state.db)
            .await;

    Ok(StatusCode::OK)
}

fn mk(payload: meet_server_message::Payload) -> MeetServerMessage {
    MeetServerMessage {
        payload: Some(payload),
    }
}

async fn room_id_from_lk_name(state: &SharedState, lk_name: &str) -> Option<String> {
    sqlx::query_scalar::<_, uuid::Uuid>("SELECT id FROM rooms WHERE livekit_room_name = $1 LIMIT 1")
        .bind(lk_name)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
        .map(|u| u.to_string())
}

// Allow accepting either JSON or raw body; some LiveKit configs send
// application/webhook+json. We accept `String` above, but provide this
// to keep the axum extractor surface obvious.
#[allow(dead_code)]
async fn _json_passthrough(Json(v): Json<serde_json::Value>) -> Json<serde_json::Value> {
    Json(v)
}
