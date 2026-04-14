//! `sunbeam.agent.v1.AgentCallback` server implementation.

use prost::Message;
use sunbeam_meet_proto::agent::v1::agent_callback_server::AgentCallback;
use sunbeam_meet_proto::agent::v1::{
    ReportFailureRequest, ReportFailureResponse, SubmitSummaryRequest, SubmitSummaryResponse,
};
use sunbeam_meet_proto::meet::v1::{meet_server_message, Error as MeetError, MeetServerMessage};
use tonic::{Request, Response, Status};

use crate::state::SharedState;
use crate::storage::pg::new_id;

/// Handler struct.
pub struct AgentCallbackHandler {
    state: SharedState,
}

impl AgentCallbackHandler {
    /// Construct with shared state.
    #[must_use]
    pub fn new(state: SharedState) -> Self {
        Self { state }
    }
}

#[tonic::async_trait]
impl AgentCallback for AgentCallbackHandler {
    async fn submit_summary(
        &self,
        req: Request<SubmitSummaryRequest>,
    ) -> Result<Response<SubmitSummaryResponse>, Status> {
        let r = req.into_inner();
        let room_uuid = parse_uuid(&r.room_id)?;
        let id = new_id();
        let action_items_json = serde_json::Value::Array(
            r.action_items
                .iter()
                .map(|a| {
                    serde_json::json!({
                        "description": a.description,
                        "assignee_identity": a.assignee_identity,
                        "assignee_name": a.assignee_name,
                    })
                })
                .collect(),
        );
        let duration_ms = r
            .meeting_duration
            .as_ref()
            .map_or(0, |d| d.seconds * 1000 + i64::from(d.nanos) / 1_000_000);

        sqlx::query(
            "INSERT INTO summaries (id, room_id, transcript_ref, summary_md, action_items, meeting_duration_ms, attendees, generated_at, model)
             VALUES ($1, $2, NULL, $3, $4::jsonb, $5, $6, NOW(), $7)",
        )
        .bind(id)
        .bind(room_uuid)
        .bind(&r.summary_markdown)
        .bind(&action_items_json)
        .bind(duration_ms)
        .bind(&r.attendee_identities)
        .bind(crate::domain::summary::DEFAULT_MODEL)
        .execute(&self.state.db)
        .await
        .map_err(|e| Status::internal(e.to_string()))?;

        // Optionally store transcript as a blob ref; inline for now.
        if !r.full_transcript.is_empty() {
            let _ = sqlx::query("UPDATE summaries SET transcript_ref = $2 WHERE id = $1")
                .bind(id)
                .bind(&r.full_transcript)
                .execute(&self.state.db)
                .await;
        }

        // Broadcast to anyone still on JoinRoom for this room.
        let msg = MeetServerMessage {
            payload: Some(meet_server_message::Payload::Error(MeetError {
                message: format!("summary ready: {id}"),
                fatal: false,
            })),
        };
        let subject = crate::events::nats::NatsPublisher::room_subject(&r.room_id);
        let _ = self
            .state
            .nats
            .publish(subject, msg.encode_to_vec().into())
            .await;
        self.state.hub.publish(&r.room_id, msg);

        Ok(Response::new(SubmitSummaryResponse {
            summary_id: id.to_string(),
        }))
    }

    async fn report_failure(
        &self,
        req: Request<ReportFailureRequest>,
    ) -> Result<Response<ReportFailureResponse>, Status> {
        let r = req.into_inner();
        tracing::warn!(job_id = %r.job_id, room_id = %r.room_id, error = %r.error, fatal = r.is_fatal, "agent reported failure");

        let msg = MeetServerMessage {
            payload: Some(meet_server_message::Payload::Error(MeetError {
                message: r.error.clone(),
                fatal: r.is_fatal,
            })),
        };
        let subject = crate::events::nats::NatsPublisher::room_subject(&r.room_id);
        let _ = self
            .state
            .nats
            .publish(subject, msg.encode_to_vec().into())
            .await;
        self.state.hub.publish(&r.room_id, msg);

        Ok(Response::new(ReportFailureResponse {}))
    }
}

fn parse_uuid(s: &str) -> Result<uuid::Uuid, Status> {
    uuid::Uuid::parse_str(s).map_err(|_| Status::invalid_argument("invalid uuid"))
}
