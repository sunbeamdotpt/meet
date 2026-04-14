//! Breakout room RPCs.
//
// Proto request/response types are imported wholesale; naming each one is noise.
#![allow(clippy::wildcard_imports)]

use sunbeam_meet_proto::meet::v1::*;
use tonic::{Request, Response, Status};

use crate::handlers::meet::{identity, parse_uuid};
use crate::state::SharedState;
use crate::storage::pg::new_id;

/// Create breakout rooms.
pub async fn create(
    state: &SharedState,
    req: Request<CreateBreakoutRequest>,
) -> Result<Response<BreakoutRoomsResponse>, Status> {
    let id = identity(state, &req).await?;
    let r = req.into_inner();
    let parent = parse_uuid(&r.room_id, "room_id")?;
    crate::authz::can(state, &id, "room", &parent.to_string(), "moderator").await?;

    let mut rooms = Vec::new();
    for i in 0..r.count {
        let br_id = new_id();
        let name = format!("Breakout {}", i + 1);
        let lk_name = format!("breakout-{}", br_id.simple());
        state
            .livekit
            .create_room(&lk_name, 50)
            .await
            .map_err(Status::from)?;
        sqlx::query(
            "INSERT INTO breakout_rooms (id, parent_room_id, name, livekit_room_name)
             VALUES ($1, $2, $3, $4)",
        )
        .bind(br_id)
        .bind(parent)
        .bind(&name)
        .bind(&lk_name)
        .execute(&state.db)
        .await
        .map_err(|e| Status::internal(e.to_string()))?;

        let idents: Vec<String> = r
            .assignments
            .iter()
            .filter(|a| a.breakout_index == i)
            .map(|a| a.participant_identity.clone())
            .collect();
        for ident in &idents {
            sqlx::query(
                "INSERT INTO breakout_assignments (breakout_room_id, participant_identity) VALUES ($1, $2)
                 ON CONFLICT DO NOTHING",
            )
            .bind(br_id)
            .bind(ident)
            .execute(&state.db)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        }
        rooms.push(BreakoutRoom {
            id: br_id.to_string(),
            name,
            livekit_room_name: lk_name,
            participant_identities: idents,
        });
    }
    Ok(Response::new(BreakoutRoomsResponse { rooms }))
}

/// Merge breakouts back into the parent room.
pub async fn merge(
    state: &SharedState,
    req: Request<MergeBreakoutRequest>,
) -> Result<Response<MergeBreakoutResponse>, Status> {
    let id = identity(state, &req).await?;
    let r = req.into_inner();
    let parent = parse_uuid(&r.room_id, "room_id")?;
    crate::authz::can(state, &id, "room", &parent.to_string(), "moderator").await?;

    let lk_names: Vec<String> = sqlx::query_scalar(
        "SELECT livekit_room_name FROM breakout_rooms WHERE parent_room_id = $1 AND ended_at IS NULL",
    )
    .bind(parent)
    .fetch_all(&state.db)
    .await
    .map_err(|e| Status::internal(e.to_string()))?;
    let mut returned: u32 = 0;
    for name in lk_names {
        if state.livekit.delete_room(&name).await.is_ok() {
            returned += 1;
        }
    }
    sqlx::query(
        "UPDATE breakout_rooms SET ended_at = NOW() WHERE parent_room_id = $1 AND ended_at IS NULL",
    )
    .bind(parent)
    .execute(&state.db)
    .await
    .map_err(|e| Status::internal(e.to_string()))?;

    Ok(Response::new(MergeBreakoutResponse {
        participants_returned: returned,
    }))
}

/// Move a participant to a specific breakout.
pub async fn move_participant(
    state: &SharedState,
    req: Request<MoveToBreakoutRequest>,
) -> Result<Response<MoveToBreakoutResponse>, Status> {
    let id = identity(state, &req).await?;
    let r = req.into_inner();
    let parent = parse_uuid(&r.room_id, "room_id")?;
    let breakout = parse_uuid(&r.breakout_room_id, "breakout_room_id")?;
    crate::authz::can(state, &id, "room", &parent.to_string(), "moderator").await?;
    sqlx::query(
        "INSERT INTO breakout_assignments (breakout_room_id, participant_identity) VALUES ($1, $2)
         ON CONFLICT DO NOTHING",
    )
    .bind(breakout)
    .bind(&r.participant_identity)
    .execute(&state.db)
    .await
    .map_err(|e| Status::internal(e.to_string()))?;
    Ok(Response::new(MoveToBreakoutResponse {}))
}
