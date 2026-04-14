//! Token minting for LiveKit.

use sunbeam_meet_proto::meet::v1::{GenerateTokenRequest, GenerateTokenResponse, ParticipantRole};
use tonic::{Request, Response, Status};

use crate::clients::livekit::Role;
use crate::handlers::meet::{identity, parse_uuid, rooms};
use crate::state::SharedState;

/// Mint a LiveKit JWT after an authz check.
pub async fn generate_token(
    state: &SharedState,
    req: Request<GenerateTokenRequest>,
) -> Result<Response<GenerateTokenResponse>, Status> {
    let id = identity(state, &req).await?;
    let r = req.into_inner();
    let room_id = parse_uuid(&r.room_id, "room_id")?;

    // Participant identity must match the authenticated identity unless
    // the caller has `moderator` on the room (e.g. minting a bot token).
    let target_identity = if r.participant_identity.is_empty() {
        id.id.clone()
    } else if r.participant_identity != id.id {
        crate::authz::can(state, &id, "room", &room_id.to_string(), "moderator").await?;
        r.participant_identity.clone()
    } else {
        r.participant_identity.clone()
    };

    crate::authz::can(state, &id, "room", &room_id.to_string(), "participant").await?;

    let lk_name: String = sqlx::query_scalar("SELECT livekit_room_name FROM rooms WHERE id = $1")
        .bind(room_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| Status::internal(e.to_string()))?
        .ok_or_else(|| Status::not_found("room"))?;

    let role = match ParticipantRole::try_from(r.role).unwrap_or(ParticipantRole::Unspecified) {
        ParticipantRole::Viewer => Role::Viewer,
        ParticipantRole::Admin => Role::Admin,
        ParticipantRole::Owner => Role::Owner,
        // Member or Unspecified both default to Member.
        ParticipantRole::Member | ParticipantRole::Unspecified => Role::Member,
    };

    let ttl_secs = r
        .ttl
        .map(|d| d.seconds as u64)
        .filter(|&s| s > 0)
        .unwrap_or(24 * 3600);

    let name = if r.participant_name.is_empty() {
        id.display_name.clone().unwrap_or_default()
    } else {
        r.participant_name
    };

    let (token, exp) = state
        .livekit
        .mint_token_for_role(&target_identity, &name, &lk_name, ttl_secs, role, false)
        .map_err(Status::from)?;

    Ok(Response::new(GenerateTokenResponse {
        token,
        livekit_url: state.livekit.url.clone(),
        expires_at: Some(rooms::ts(
            chrono::DateTime::from_timestamp(exp as i64, 0).unwrap_or_default(),
        )),
    }))
}
