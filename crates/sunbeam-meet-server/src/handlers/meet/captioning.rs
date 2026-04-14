//! Captioning RPCs — dispatch to whisper agent workers.
//
// Proto request/response types are imported wholesale; naming each one is noise.
#![allow(clippy::wildcard_imports)]

use sunbeam_meet_proto::agent::v1::{AgentConfig, AgentKind, StartJobRequest};
use sunbeam_meet_proto::meet::v1::*;
use tonic::{Request, Response, Status};

use crate::handlers::meet::{identity, parse_uuid};
use crate::state::SharedState;

/// Start captioning.
pub async fn start(
    state: &SharedState,
    req: Request<StartCaptioningRequest>,
) -> Result<Response<CaptioningStatus>, Status> {
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

    // Mint a hidden bot token.
    let (token, _) = state
        .livekit
        .mint_token_for_role(
            "bot-whisper",
            "Captions",
            &lk,
            3600,
            crate::clients::livekit::Role::Member,
            true,
        )
        .map_err(Status::from)?;

    let job_id = uuid::Uuid::new_v4().to_string();
    let start_req = StartJobRequest {
        job_id: job_id.clone(),
        room_name: lk,
        token,
        livekit_url: state.livekit.url.clone(),
        kind: AgentKind::WhisperStt as i32,
        config: Some(AgentConfig {
            language: r.language.clone(),
            interim_results: true,
            mistral_model: String::new(),
            max_summary_tokens: 0,
            summary_prompts: Vec::new(),
            callback_url: state.config.bind_addr.clone(),
        }),
    };

    match state
        .agents
        .start_job(AgentKind::WhisperStt, start_req)
        .await
    {
        Ok((worker, true)) => {
            sqlx::query(
                "INSERT INTO captioning_state (room_id, state, language, job_id, worker_name, updated_at)
                 VALUES ($1, 'starting', $2, $3, $4, NOW())
                 ON CONFLICT (room_id) DO UPDATE SET
                   state='starting', language=$2, job_id=$3, worker_name=$4, updated_at=NOW()",
            )
            .bind(room_id)
            .bind(&r.language)
            .bind(&job_id)
            .bind(&worker)
            .execute(&state.db)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

            Ok(Response::new(CaptioningStatus {
                room_id: r.room_id,
                state: CaptioningState::Starting as i32,
                language: r.language,
                error_message: String::new(),
            }))
        }
        _ => Err(Status::unimplemented("pending live integration")),
    }
}

/// Stop captioning.
pub async fn stop(
    state: &SharedState,
    req: Request<StopCaptioningRequest>,
) -> Result<Response<CaptioningStatus>, Status> {
    let id = identity(state, &req).await?;
    let r = req.into_inner();
    let room_id = parse_uuid(&r.room_id, "room_id")?;
    crate::authz::can(state, &id, "room", &room_id.to_string(), "moderator").await?;

    let job_id: Option<String> =
        sqlx::query_scalar("SELECT job_id FROM captioning_state WHERE room_id = $1")
            .bind(room_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| Status::internal(e.to_string()))?
            .flatten_option();

    if let Some(jid) = job_id.filter(|j| !j.is_empty()) {
        let _ = state.agents.stop_job(&jid, true).await;
    }
    sqlx::query("UPDATE captioning_state SET state='stopping', updated_at=NOW() WHERE room_id=$1")
        .bind(room_id)
        .execute(&state.db)
        .await
        .map_err(|e| Status::internal(e.to_string()))?;

    Ok(Response::new(CaptioningStatus {
        room_id: r.room_id,
        state: CaptioningState::Stopping as i32,
        language: String::new(),
        error_message: String::new(),
    }))
}

trait FlattenOption<T> {
    fn flatten_option(self) -> Option<T>;
}
impl<T> FlattenOption<T> for Option<T> {
    fn flatten_option(self) -> Option<T> {
        self
    }
}
