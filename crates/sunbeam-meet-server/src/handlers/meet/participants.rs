//! Participant management RPCs.
//
// Proto request/response types are imported wholesale; naming each one is noise.
#![allow(clippy::wildcard_imports)]

use sunbeam_meet_proto::meet::v1::*;
use tonic::{Request, Response, Status};

use crate::handlers::meet::{identity, parse_uuid};
use crate::state::SharedState;

/// Invite a participant — fully wired for LINK method, stubbed for others.
pub async fn invite(
    state: &SharedState,
    req: Request<InviteRequest>,
) -> Result<Response<InviteResponse>, Status> {
    let id = identity(state, &req).await?;
    let r = req.into_inner();
    let room_id = parse_uuid(&r.room_id, "room_id")?;
    crate::authz::can(state, &id, "room", &room_id.to_string(), "moderator").await?;

    let invite_id = uuid::Uuid::new_v4().to_string();
    let invite_url = format!("https://meet.sunbeam.pt/room/{room_id}/join?invite={invite_id}");

    // Grant invited relation.
    let _ = crate::authz::grant(state, "room", &room_id.to_string(), "invited", &r.target).await;

    match InviteMethod::try_from(r.method).unwrap_or(InviteMethod::Unspecified) {
        InviteMethod::Link => {}
        _ => return Err(Status::unimplemented("pending live integration")),
    }

    Ok(Response::new(InviteResponse {
        invite_id,
        invite_url,
    }))
}

/// Kick a participant.
pub async fn kick(
    state: &SharedState,
    req: Request<KickRequest>,
) -> Result<Response<KickResponse>, Status> {
    let id = identity(state, &req).await?;
    let r = req.into_inner();
    let room_id = parse_uuid(&r.room_id, "room_id")?;
    crate::authz::can(state, &id, "room", &room_id.to_string(), "moderator").await?;

    let lk: String = sqlx::query_scalar("SELECT livekit_room_name FROM rooms WHERE id = $1")
        .bind(room_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| Status::internal(e.to_string()))?
        .ok_or_else(|| Status::not_found("room"))?;
    state
        .livekit
        .remove_participant(&lk, &r.participant_identity)
        .await
        .map_err(Status::from)?;
    Ok(Response::new(KickResponse {}))
}

/// Update a participant's role.
pub async fn update_role(
    state: &SharedState,
    req: Request<UpdateRoleRequest>,
) -> Result<Response<Participant>, Status> {
    let id = identity(state, &req).await?;
    let r = req.into_inner();
    let room_id = parse_uuid(&r.room_id, "room_id")?;
    crate::authz::can(state, &id, "room", &room_id.to_string(), "moderator").await?;
    let relation =
        match ParticipantRole::try_from(r.new_role).unwrap_or(ParticipantRole::Unspecified) {
            ParticipantRole::Admin => "moderator",
            ParticipantRole::Owner => "owner",
            _ => "participant",
        };
    let _ = crate::authz::grant(
        state,
        "room",
        &room_id.to_string(),
        relation,
        &r.participant_identity,
    )
    .await;
    Err(Status::unimplemented("pending live integration"))
}

/// Mute a participant.
pub async fn mute(
    state: &SharedState,
    req: Request<MuteParticipantRequest>,
) -> Result<Response<MuteParticipantResponse>, Status> {
    let id = identity(state, &req).await?;
    let r = req.into_inner();
    let room_id = parse_uuid(&r.room_id, "room_id")?;
    crate::authz::can(state, &id, "room", &room_id.to_string(), "moderator").await?;
    let lk: String = sqlx::query_scalar("SELECT livekit_room_name FROM rooms WHERE id = $1")
        .bind(room_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| Status::internal(e.to_string()))?
        .ok_or_else(|| Status::not_found("room"))?;
    if r.mute_audio || r.mute_video {
        state
            .livekit
            .mute_track(&lk, &r.participant_identity, true)
            .await
            .map_err(Status::from)?;
    }
    Ok(Response::new(MuteParticipantResponse {}))
}
