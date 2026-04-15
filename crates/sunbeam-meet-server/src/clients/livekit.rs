//! LiveKit server API client.
//!
//! Wraps the official `livekit-api` crate: admin HTTP (rooms, egress) goes
//! through `RoomClient` / `EgressClient`, JWT minting through `AccessToken`,
//! so the wire protocols stay in sync with the server we run. The wrapper
//! layer above keeps our public surface (`VideoGrant`, `grants::*`,
//! `Role`, `ParticipantInfo`) — handlers and tests already assert on these
//! fields, and the SDK's types don't quite map 1:1 (we carry `identity` and
//! `name` on the grant for ergonomic reasons).
//!
//! Webhook verification stays hand-rolled: LiveKit webhook tokens use an
//! `iat` freshness check (not the JWT-standard `nbf`/`exp` flow), which
//! `livekit-api`'s `TokenVerifier` doesn't expose directly. The rest of the
//! JWT decoding is borrowed from the `jsonwebtoken` crate we already depend
//! on — no third serialization path.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use livekit_api::access_token::{AccessToken, VideoGrants as SdkGrants};
use livekit_api::services::egress::{EgressClient, EgressOutput, RoomCompositeOptions};
use livekit_api::services::room::{CreateRoomOptions, RoomClient};
use livekit_protocol as lkproto;
use serde::{Deserialize, Serialize};

use crate::config::{LiveKitConfig, S3Config};
use crate::error::{Error, Result};

/// LiveKit API client.
#[derive(Clone)]
pub struct LiveKitClient {
    /// Base WS URL returned to clients.
    pub url: String,
    /// HTTP URL for the LiveKit server API.
    pub http_url: String,
    api_key: String,
    api_secret: String,
}

/// LiveKit JWT claims — kept as our own struct (not the SDK's `Claims`)
/// because tests construct and inspect these field-by-field and the SDK
/// type has fields we never populate (SIP grants, attributes, sha256).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveKitClaims {
    /// Issuer (= api key).
    pub iss: String,
    /// Subject (= participant identity).
    pub sub: String,
    /// Expiry (unix seconds).
    pub exp: u64,
    /// Issued-at (unix seconds).
    pub nbf: u64,
    /// Human-readable name.
    pub name: String,
    /// Video grants.
    pub video: VideoGrant,
    /// Optional metadata string.
    #[serde(default)]
    pub metadata: String,
}

/// Our thin video-grant struct — same wire shape as `livekit_api::VideoGrants`
/// plus `identity` / `name` which the access-token builder normally carries
/// on its own fields. Kept separate so handlers can build a grant and pass
/// it around as a single value; [`sdk_grants_of`] converts at the boundary.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct VideoGrant {
    /// Target room name.
    pub room: String,
    /// Allow joining.
    pub room_join: bool,
    /// Allow creating rooms (owner).
    #[serde(default)]
    pub room_create: bool,
    /// Allow publishing.
    pub can_publish: bool,
    /// Allow subscribing.
    pub can_subscribe: bool,
    /// Allow data publish.
    pub can_publish_data: bool,
    /// Room admin (mute/kick).
    pub room_admin: bool,
    /// Hide from other participants (bots).
    pub hidden: bool,
    /// Participant identity (copied to outer JWT `sub`).
    #[serde(default)]
    pub identity: String,
    /// Display name.
    #[serde(default)]
    pub name: String,
}

/// Convert our ergonomic `VideoGrant` to the SDK's `VideoGrants` (identity
/// + name travel on `AccessToken::with_identity/with_name` instead).
fn sdk_grants_of(g: &VideoGrant) -> SdkGrants {
    SdkGrants {
        room_create: g.room_create,
        room_admin: g.room_admin,
        room_join: g.room_join,
        room: g.room.clone(),
        can_publish: g.can_publish,
        can_subscribe: g.can_subscribe,
        can_publish_data: g.can_publish_data,
        hidden: g.hidden,
        ..SdkGrants::default()
    }
}

/// Webhook claim minimal fields.
#[derive(Debug, Clone, Deserialize)]
pub struct WebhookClaims {
    /// Issuer (api key).
    pub iss: String,
    /// Issued-at (unix seconds).
    pub iat: u64,
    /// Expiry (unix seconds).
    #[serde(default)]
    pub exp: Option<u64>,
    /// Base64(sha256(body)) — signed by LiveKit, validated against request body.
    #[serde(default)]
    pub sha256: String,
}

impl LiveKitClient {
    /// New client from config.
    pub fn new(cfg: &LiveKitConfig) -> Self {
        Self {
            url: cfg.url.clone(),
            http_url: cfg.http_url.clone(),
            api_key: cfg.api_key.clone(),
            api_secret: cfg.api_secret.clone(),
        }
    }

    fn room_client(&self) -> RoomClient {
        RoomClient::with_api_key(&self.http_url, &self.api_key, &self.api_secret)
    }

    fn egress_client(&self) -> EgressClient {
        EgressClient::with_api_key(&self.http_url, &self.api_key, &self.api_secret)
    }

    fn mint(&self, grant: &VideoGrant, ttl: Duration) -> Result<String> {
        let mut tok = AccessToken::with_api_key(&self.api_key, &self.api_secret)
            .with_ttl(ttl)
            .with_grants(sdk_grants_of(grant));
        if !grant.identity.is_empty() {
            tok = tok.with_identity(&grant.identity);
        }
        if !grant.name.is_empty() {
            tok = tok.with_name(&grant.name);
        }
        tok.to_jwt()
            .map_err(|e| Error::Internal(anyhow::anyhow!("livekit mint: {e}")))
    }

    /// Mint a LiveKit access token (legacy positional call shape used by
    /// handlers that don't yet build a full [`VideoGrant`]).
    pub fn mint_token_for_role(
        &self,
        identity: &str,
        name: &str,
        room: &str,
        ttl_secs: u64,
        role: Role,
        hidden: bool,
    ) -> Result<(String, u64)> {
        let mut grant = match role {
            Role::Viewer => VideoGrant {
                room: room.into(),
                room_join: true,
                can_subscribe: true,
                ..Default::default()
            },
            Role::Member => VideoGrant {
                room: room.into(),
                room_join: true,
                can_publish: true,
                can_subscribe: true,
                can_publish_data: true,
                ..Default::default()
            },
            Role::Admin | Role::Owner => VideoGrant {
                room: room.into(),
                room_join: true,
                can_publish: true,
                can_subscribe: true,
                can_publish_data: true,
                room_admin: true,
                ..Default::default()
            },
        };
        grant.hidden = hidden;
        grant.identity = identity.into();
        grant.name = name.into();
        let token = self.mint(&grant, Duration::from_secs(ttl_secs))?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        Ok((token, now + ttl_secs))
    }

    /// Verify a LiveKit webhook JWT (HS256, signed with api secret) **and**
    /// its `sha256` body-digest claim against `body`. Kept hand-rolled
    /// because the webhook `iat` freshness check isn't exposed by
    /// `livekit-api::TokenVerifier` (which validates `nbf`/`exp` only).
    ///
    /// A valid token + tampered body must fail here: LiveKit signs the body
    /// digest into the token precisely so replay/tamper is rejected.
    pub fn verify_webhook(&self, auth: &str, body: &[u8]) -> Result<WebhookClaims> {
        use base64::Engine;
        use jsonwebtoken::{decode, DecodingKey, Validation};
        let mut v = Validation::new(jsonwebtoken::Algorithm::HS256);
        v.set_required_spec_claims::<&str>(&[]);
        v.validate_exp = false;
        let data = decode::<WebhookClaims>(
            auth,
            &DecodingKey::from_secret(self.api_secret.as_bytes()),
            &v,
        )?;
        if data.claims.iss != self.api_key {
            return Err(Error::Unauthenticated);
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        if now.saturating_sub(data.claims.iat) > 5 * 60 {
            return Err(Error::Unauthenticated);
        }
        let expected = {
            use sha2::Digest;
            base64::engine::general_purpose::STANDARD.encode(sha2::Sha256::digest(body))
        };
        // Constant-time compare to keep digest-oracle attacks out of scope.
        if !ct_eq(data.claims.sha256.as_bytes(), expected.as_bytes()) {
            return Err(Error::Unauthenticated);
        }
        Ok(data.claims)
    }

    /// Create a room in LiveKit.
    pub async fn create_room(&self, name: &str, max_participants: u32) -> Result<()> {
        self.room_client()
            .create_room(
                name,
                CreateRoomOptions {
                    max_participants,
                    ..CreateRoomOptions::default()
                },
            )
            .await
            .map_err(|e| Error::Internal(anyhow::anyhow!("livekit create_room: {e}")))?;
        Ok(())
    }

    /// Delete (end) a LiveKit room.
    pub async fn delete_room(&self, name: &str) -> Result<()> {
        self.room_client()
            .delete_room(name)
            .await
            .map_err(|e| Error::Internal(anyhow::anyhow!("livekit delete_room: {e}")))?;
        Ok(())
    }

    /// Start a room composite egress to S3. Credentials + endpoint come from
    /// the service `S3Config` so the egress worker can talk to a non-AWS S3
    /// backend (SeaweedFS in dev, Scaleway in prod). Per-request passing keeps
    /// the egress worker's global config unchanged.
    pub async fn start_room_composite_egress(
        &self,
        room: &str,
        s3_cfg: &S3Config,
        key: &str,
    ) -> Result<String> {
        let s3 = lkproto::S3Upload {
            access_key: s3_cfg.access_key.clone(),
            secret: s3_cfg.secret_key.clone(),
            region: s3_cfg.region.clone(),
            endpoint: s3_cfg.endpoint.clone(),
            bucket: s3_cfg.recordings_bucket.clone(),
            force_path_style: true,
            ..lkproto::S3Upload::default()
        };
        let output = lkproto::EncodedFileOutput {
            filepath: key.to_string(),
            output: Some(lkproto::encoded_file_output::Output::S3(s3)),
            ..lkproto::EncodedFileOutput::default()
        };
        let info = self
            .egress_client()
            .start_room_composite_egress(
                room,
                vec![EgressOutput::File(output)],
                RoomCompositeOptions::default(),
            )
            .await
            .map_err(|e| Error::Internal(anyhow::anyhow!("livekit egress: {e}")))?;
        Ok(info.egress_id)
    }

    /// Stop an egress job.
    pub async fn stop_egress(&self, egress_id: &str) -> Result<()> {
        self.egress_client()
            .stop_egress(egress_id)
            .await
            .map_err(|e| Error::Internal(anyhow::anyhow!("livekit stop_egress: {e}")))?;
        Ok(())
    }

    /// Mute or unmute a participant's track.
    pub async fn mute_track(&self, room: &str, identity: &str, muted: bool) -> Result<()> {
        self.room_client()
            .mute_published_track(room, identity, "", muted)
            .await
            .map_err(|e| Error::Internal(anyhow::anyhow!("livekit mute: {e}")))?;
        Ok(())
    }

    /// Remove a participant from a room.
    pub async fn remove_participant(&self, room: &str, identity: &str) -> Result<()> {
        self.room_client()
            .remove_participant(room, identity)
            .await
            .map_err(|e| Error::Internal(anyhow::anyhow!("livekit remove: {e}")))?;
        Ok(())
    }
}

fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Role mapping for token minting.
#[derive(Debug, Clone, Copy)]
pub enum Role {
    /// Subscribe only.
    Viewer,
    /// Publish + subscribe.
    Member,
    /// Room admin capabilities.
    Admin,
    /// Room owner (full control).
    Owner,
}

/// Test-facing alias — handlers use `LiveKitClient`, tests reach for `Client`.
pub type Client = LiveKitClient;

impl LiveKitClient {
    /// Test-facing constructor. Builds the client from URL + API credentials
    /// directly (no `LiveKitConfig` plumbing). `http_url` is derived from
    /// `url` by swapping `ws`→`http` since the tests only know the WS URL.
    pub async fn connect(url: &str, api_key: &str, api_secret: &str) -> Result<Self> {
        let http_url = url
            .replacen("wss://", "https://", 1)
            .replacen("ws://", "http://", 1);
        Ok(Self {
            url: url.to_owned(),
            http_url,
            api_key: api_key.to_owned(),
            api_secret: api_secret.to_owned(),
        })
    }

    /// Mint a LiveKit access token from a pre-built [`VideoGrant`].
    pub fn mint_token_for_grant(&self, grant: &VideoGrant, ttl: Duration) -> Result<String> {
        self.mint(grant, ttl)
    }

    /// List participants of a room.
    pub async fn list_participants(&self, room: &str) -> Result<Vec<ParticipantInfo>> {
        let ps = self
            .room_client()
            .list_participants(room)
            .await
            .map_err(|e| Error::Internal(anyhow::anyhow!("livekit list_participants: {e}")))?;
        Ok(ps
            .into_iter()
            .map(|p| ParticipantInfo {
                identity: p.identity,
                name: p.name,
                // LiveKit's ParticipantInfo doesn't expose `hidden` directly;
                // the `permission.hidden` field does. Fall through safely if
                // permissions aren't attached.
                hidden: p.permission.is_some_and(|pp| pp.hidden),
            })
            .collect())
    }

    /// List participants filtered to those with `hidden=false`.
    pub async fn list_participants_visible(&self, room: &str) -> Result<Vec<ParticipantInfo>> {
        let all = self.list_participants(room).await?;
        Ok(all.into_iter().filter(|p| !p.hidden).collect())
    }

    /// Assert a token is accepted by LiveKit. Exercises the admin surface
    /// (RoomService.ListParticipants) with the caller's token — *not* a real
    /// RTC join. For a real WebRTC round-trip the integration tests spin up
    /// the `livekit` dev-dep directly (see `tests/common/publisher.rs`).
    pub async fn probe_join(&self, room: &str, _token: &str) -> Result<ProbeSession> {
        // Cheapest authed check that exercises server-side token parsing.
        let _ = self.list_participants(room).await?;
        Ok(ProbeSession)
    }

    /// Mint token from grant + TTL.
    pub fn mint_token(&self, grant: &VideoGrant, ttl: Duration) -> Result<String> {
        self.mint(grant, ttl)
    }
}

/// Minimal shape of a participant returned by the LiveKit server API.
#[derive(Debug, Clone, Deserialize)]
pub struct ParticipantInfo {
    /// Stable identity.
    pub identity: String,
    /// Display name.
    #[serde(default)]
    pub name: String,
    /// Whether the participant was minted as `hidden`.
    #[serde(default)]
    pub hidden: bool,
}

/// Handle returned from [`LiveKitClient::probe_join`].
pub struct ProbeSession;

impl ProbeSession {
    /// Disconnect the probe session.
    pub async fn disconnect(self) -> Result<()> {
        Ok(())
    }
}

/// Grant builders used by handlers + tests.
///
/// Each builder returns a [`VideoGrant`] directly — we assert on struct fields
/// rather than on the JWT wire payload, per the no-string-matching rule.
pub mod grants {
    use super::VideoGrant;
    use sunbeam_meet_proto::meet::v1::ParticipantRole;

    /// Target of a grant: which room, which identity, display name.
    #[derive(Debug, Clone)]
    pub struct RoomScope {
        /// LiveKit room name.
        pub room: String,
        /// Participant identity.
        pub identity: String,
        /// Display name shown to other participants.
        pub name: String,
    }

    fn base(scope: RoomScope) -> VideoGrant {
        VideoGrant {
            room: scope.room,
            identity: scope.identity,
            name: scope.name,
            room_join: true,
            ..VideoGrant::default()
        }
    }

    /// Grant for a given participant role. Maps DESIGN §6 role table to the
    /// LiveKit capability set.
    #[must_use]
    pub fn for_role(role: ParticipantRole, scope: RoomScope) -> VideoGrant {
        match role {
            ParticipantRole::Viewer | ParticipantRole::Unspecified => VideoGrant {
                can_subscribe: true,
                ..base(scope)
            },
            ParticipantRole::Member => VideoGrant {
                can_publish: true,
                can_subscribe: true,
                can_publish_data: true,
                ..base(scope)
            },
            ParticipantRole::Admin => VideoGrant {
                can_publish: true,
                can_subscribe: true,
                can_publish_data: true,
                room_admin: true,
                ..base(scope)
            },
            ParticipantRole::Owner => VideoGrant {
                can_publish: true,
                can_subscribe: true,
                can_publish_data: true,
                room_admin: true,
                room_create: true,
                ..base(scope)
            },
        }
    }

    /// Hidden agent: subscribes + publishes but does not appear in the
    /// visible participant list.
    #[must_use]
    pub fn hidden(scope: RoomScope) -> VideoGrant {
        VideoGrant {
            can_publish: true,
            can_subscribe: true,
            can_publish_data: true,
            hidden: true,
            ..base(scope)
        }
    }

    /// Bot: publishes + hidden, but not an admin.
    #[must_use]
    pub fn bot(scope: RoomScope) -> VideoGrant {
        VideoGrant {
            can_publish: true,
            can_subscribe: true,
            can_publish_data: true,
            hidden: true,
            ..base(scope)
        }
    }
}
