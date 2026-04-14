//! `sunbeam.meet.v1.MeetService` handler tree.
//!
//! Each submodule owns a slice of the RPC surface (rooms, auth, recording…).
//! The `MeetHandler` struct implements the trait by delegating to free
//! functions in those submodules.

// MeetService has ~50 request/response types; importing them by name is noise.
#![allow(clippy::wildcard_imports)]

pub mod auth;
pub mod breakout;
pub mod captioning;
pub mod chat;
pub mod join_room;
pub mod participants;
pub mod reactions;
pub mod recording;
pub mod rooms;
pub mod scheduling;
pub mod summaries;

use std::pin::Pin;

use futures::Stream;
use sunbeam_meet_proto::meet::v1::meet_service_server::MeetService;
use sunbeam_meet_proto::meet::v1::*;
use tonic::{Request, Response, Status, Streaming};

use crate::state::SharedState;

/// Handler implementing `MeetService`.
pub struct MeetHandler {
    /// Shared app state.
    pub state: SharedState,
}

impl MeetHandler {
    /// New handler.
    #[must_use]
    pub fn new(state: SharedState) -> Self {
        Self { state }
    }
}

/// Stream type returned by `JoinRoom`.
pub type JoinRoomStream =
    Pin<Box<dyn Stream<Item = Result<MeetServerMessage, Status>> + Send + 'static>>;

#[tonic::async_trait]
impl MeetService for MeetHandler {
    // Rooms
    async fn create_room(&self, req: Request<CreateRoomRequest>) -> Result<Response<Room>, Status> {
        rooms::create(&self.state, req).await
    }
    async fn get_room(&self, req: Request<GetRoomRequest>) -> Result<Response<Room>, Status> {
        rooms::get(&self.state, req).await
    }
    async fn list_rooms(
        &self,
        req: Request<ListRoomsRequest>,
    ) -> Result<Response<ListRoomsResponse>, Status> {
        rooms::list(&self.state, req).await
    }
    async fn update_room(&self, req: Request<UpdateRoomRequest>) -> Result<Response<Room>, Status> {
        rooms::update(&self.state, req).await
    }
    async fn end_room(
        &self,
        req: Request<EndRoomRequest>,
    ) -> Result<Response<EndRoomResponse>, Status> {
        rooms::end(&self.state, req).await
    }

    // JoinRoom bidi
    type JoinRoomStream = JoinRoomStream;
    async fn join_room(
        &self,
        req: Request<Streaming<MeetClientMessage>>,
    ) -> Result<Response<Self::JoinRoomStream>, Status> {
        join_room::join(&self.state, req).await
    }

    // Auth
    async fn generate_token(
        &self,
        req: Request<GenerateTokenRequest>,
    ) -> Result<Response<GenerateTokenResponse>, Status> {
        auth::generate_token(&self.state, req).await
    }

    // Participants
    async fn invite_participant(
        &self,
        req: Request<InviteRequest>,
    ) -> Result<Response<InviteResponse>, Status> {
        participants::invite(&self.state, req).await
    }
    async fn kick_participant(
        &self,
        req: Request<KickRequest>,
    ) -> Result<Response<KickResponse>, Status> {
        participants::kick(&self.state, req).await
    }
    async fn update_participant_role(
        &self,
        req: Request<UpdateRoleRequest>,
    ) -> Result<Response<Participant>, Status> {
        participants::update_role(&self.state, req).await
    }
    async fn mute_participant(
        &self,
        req: Request<MuteParticipantRequest>,
    ) -> Result<Response<MuteParticipantResponse>, Status> {
        participants::mute(&self.state, req).await
    }

    // Recording
    async fn start_recording(
        &self,
        req: Request<StartRecordingRequest>,
    ) -> Result<Response<Recording>, Status> {
        recording::start(&self.state, req).await
    }
    async fn stop_recording(
        &self,
        req: Request<StopRecordingRequest>,
    ) -> Result<Response<Recording>, Status> {
        recording::stop(&self.state, req).await
    }
    async fn list_recordings(
        &self,
        req: Request<ListRecordingsRequest>,
    ) -> Result<Response<ListRecordingsResponse>, Status> {
        recording::list(&self.state, req).await
    }
    async fn get_recording(
        &self,
        req: Request<GetRecordingRequest>,
    ) -> Result<Response<Recording>, Status> {
        recording::get(&self.state, req).await
    }
    async fn delete_recording(
        &self,
        req: Request<DeleteRecordingRequest>,
    ) -> Result<Response<DeleteRecordingResponse>, Status> {
        recording::delete(&self.state, req).await
    }

    // Captioning
    async fn start_captioning(
        &self,
        req: Request<StartCaptioningRequest>,
    ) -> Result<Response<CaptioningStatus>, Status> {
        captioning::start(&self.state, req).await
    }
    async fn stop_captioning(
        &self,
        req: Request<StopCaptioningRequest>,
    ) -> Result<Response<CaptioningStatus>, Status> {
        captioning::stop(&self.state, req).await
    }

    // Chat
    async fn send_chat_message(
        &self,
        req: Request<SendChatRequest>,
    ) -> Result<Response<ChatMessage>, Status> {
        chat::send(&self.state, req).await
    }
    async fn get_chat_history(
        &self,
        req: Request<GetChatHistoryRequest>,
    ) -> Result<Response<GetChatHistoryResponse>, Status> {
        chat::history(&self.state, req).await
    }
    async fn delete_chat_message(
        &self,
        req: Request<DeleteChatMessageRequest>,
    ) -> Result<Response<DeleteChatMessageResponse>, Status> {
        chat::delete(&self.state, req).await
    }

    // Reactions
    async fn send_reaction(
        &self,
        req: Request<SendReactionRequest>,
    ) -> Result<Response<SendReactionResponse>, Status> {
        reactions::send(&self.state, req).await
    }

    // Breakout
    async fn create_breakout_rooms(
        &self,
        req: Request<CreateBreakoutRequest>,
    ) -> Result<Response<BreakoutRoomsResponse>, Status> {
        breakout::create(&self.state, req).await
    }
    async fn merge_breakout_rooms(
        &self,
        req: Request<MergeBreakoutRequest>,
    ) -> Result<Response<MergeBreakoutResponse>, Status> {
        breakout::merge(&self.state, req).await
    }
    async fn move_participant_to_breakout(
        &self,
        req: Request<MoveToBreakoutRequest>,
    ) -> Result<Response<MoveToBreakoutResponse>, Status> {
        breakout::move_participant(&self.state, req).await
    }

    // Scheduling
    async fn schedule_meeting(
        &self,
        req: Request<ScheduleMeetingRequest>,
    ) -> Result<Response<ScheduledMeeting>, Status> {
        scheduling::create(&self.state, req).await
    }
    async fn get_scheduled_meeting(
        &self,
        req: Request<GetScheduledMeetingRequest>,
    ) -> Result<Response<ScheduledMeeting>, Status> {
        scheduling::get(&self.state, req).await
    }
    async fn list_scheduled_meetings(
        &self,
        req: Request<ListScheduledMeetingsRequest>,
    ) -> Result<Response<ListScheduledMeetingsResponse>, Status> {
        scheduling::list(&self.state, req).await
    }
    async fn update_scheduled_meeting(
        &self,
        req: Request<UpdateScheduledMeetingRequest>,
    ) -> Result<Response<ScheduledMeeting>, Status> {
        scheduling::update(&self.state, req).await
    }
    async fn cancel_scheduled_meeting(
        &self,
        req: Request<CancelScheduledMeetingRequest>,
    ) -> Result<Response<CancelScheduledMeetingResponse>, Status> {
        scheduling::cancel(&self.state, req).await
    }

    // Summaries
    async fn get_meeting_summary(
        &self,
        req: Request<GetMeetingSummaryRequest>,
    ) -> Result<Response<MeetingSummary>, Status> {
        summaries::get(&self.state, req).await
    }
    async fn list_meeting_summaries(
        &self,
        req: Request<ListMeetingSummariesRequest>,
    ) -> Result<Response<ListMeetingSummariesResponse>, Status> {
        summaries::list(&self.state, req).await
    }
}

/// Shared helper: extract identity, 401 on failure.
pub(crate) async fn identity<T>(
    state: &SharedState,
    req: &Request<T>,
) -> Result<crate::clients::kratos::Identity, Status> {
    crate::middleware::auth::identity_from_request(state, req)
        .await
        .map_err(Into::into)
}

/// Shared helper: parse a UUID, invalid-arg on failure.
pub(crate) fn parse_uuid(s: &str, field: &str) -> Result<uuid::Uuid, Status> {
    uuid::Uuid::parse_str(s).map_err(|_| Status::invalid_argument(format!("invalid {field}")))
}
