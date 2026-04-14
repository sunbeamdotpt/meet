//! Reactions RPC.
//
// Proto request/response types are imported wholesale; naming each one is noise.
#![allow(clippy::wildcard_imports)]

use sunbeam_meet_proto::meet::v1::*;
use tonic::{Request, Response, Status};

use crate::handlers::meet::{identity, parse_uuid, rooms};
use crate::state::SharedState;
use crate::storage::pg::new_id;

/// Send a reaction; appended to the log and fanned out.
pub async fn send(
    state: &SharedState,
    req: Request<SendReactionRequest>,
) -> Result<Response<SendReactionResponse>, Status> {
    let id = identity(state, &req).await?;
    let r = req.into_inner();
    let room_id = parse_uuid(&r.room_id, "room_id")?;
    crate::authz::can(state, &id, "room", &room_id.to_string(), "participant").await?;
    if r.emoji.is_empty() || r.emoji.chars().count() > 8 {
        return Err(Status::invalid_argument("emoji"));
    }

    sqlx::query(
        "INSERT INTO reactions (id, room_id, sender_identity, emoji) VALUES ($1, $2, $3, $4)",
    )
    .bind(new_id())
    .bind(room_id)
    .bind(&id.id)
    .bind(&r.emoji)
    .execute(&state.db)
    .await
    .map_err(|e| Status::internal(e.to_string()))?;

    let now = chrono::Utc::now();
    let broadcast = MeetServerMessage {
        payload: Some(meet_server_message::Payload::Reaction(ReactionBroadcast {
            reaction: Some(Reaction {
                participant_identity: id.id.clone(),
                participant_name: id.display_name.clone().unwrap_or_default(),
                emoji: r.emoji,
                timestamp: Some(rooms::ts(now)),
            }),
        })),
    };
    state.hub.publish(&room_id.to_string(), broadcast);

    Ok(Response::new(SendReactionResponse {}))
}
