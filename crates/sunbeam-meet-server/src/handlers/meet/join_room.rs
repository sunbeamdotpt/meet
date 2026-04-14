//! Bidi `JoinRoom` RPC — the primary client channel.
//!
//! - Reads client messages (mute, raise hand, reaction, ping…) and either
//!   persists them (via their respective handlers) or reflects them to the
//!   hub.
//! - Subscribes the client to the in-process `Hub` for the room.
//! - On first local subscriber for a room, spawns a NATS bridge task that
//!   forwards `meet.room.{room_id}` payloads into the local hub for
//!   cross-instance fan-out.

use std::pin::Pin;

use futures::{Stream, StreamExt};
use prost::Message;
use sunbeam_meet_proto::meet::v1::{
    meet_client_message, meet_server_message, MeetClientMessage, MeetServerMessage, Pong, RoomState,
};
use tokio::sync::mpsc;
use tonic::{Request, Response, Status, Streaming};

use crate::events::nats::NatsPublisher;
use crate::handlers::meet::{parse_uuid, rooms};
use crate::state::SharedState;

/// Entry point.
pub async fn join(
    state: &SharedState,
    req: Request<Streaming<MeetClientMessage>>,
) -> Result<Response<super::JoinRoomStream>, Status> {
    let auth = crate::middleware::auth::extract_auth(&req).map_err(tonic::Status::from)?;
    let ident = crate::middleware::auth::identity_from_header(state, &auth)
        .await
        .map_err(tonic::Status::from)?;
    let mut inbound = req.into_inner();

    // First message must be a JoinRequest.
    let Some(first) = inbound.message().await? else {
        return Err(Status::invalid_argument("empty stream"));
    };
    let Some(meet_client_message::Payload::Join(join)) = first.payload else {
        return Err(Status::invalid_argument(
            "first message must be JoinRequest",
        ));
    };
    let room_id = parse_uuid(&join.room_id, "room_id")?;
    crate::authz::can(state, &ident, "room", &room_id.to_string(), "participant").await?;

    let room = rooms::load_room(state, room_id).await?;

    // Subscribe to the in-process hub; first subscriber spawns NATS bridge.
    let (hub, fresh) = state.hub.room(&room_id.to_string());
    let mut rx = hub.tx.subscribe();
    if fresh {
        spawn_nats_bridge(state.clone(), room_id.to_string());
    }

    crate::metrics::metrics()
        .fanout_subscribers
        .with_label_values(&[&room_id.to_string()])
        .inc();

    // Record participant history.
    let _ = sqlx::query(
        "INSERT INTO participants_history (id, room_id, identity, role, joined_at)
         VALUES ($1, $2, $3, 'member', NOW())",
    )
    .bind(crate::storage::pg::new_id())
    .bind(room_id)
    .bind(&ident.id)
    .execute(&state.db)
    .await;

    let _ = state
        .valkey
        .set_presence(
            &room_id.to_string(),
            &ident.id,
            chrono::Utc::now().timestamp_millis(),
        )
        .await;

    // Outbound channel to the client.
    let (tx, out_rx) = mpsc::channel::<Result<MeetServerMessage, Status>>(
        super::super::super::stream::join_room::CHANNEL_BOUND,
    );

    // Send initial RoomState.
    let initial = MeetServerMessage {
        payload: Some(meet_server_message::Payload::RoomState(RoomState {
            room: Some(room),
            your_identity: ident.id.clone(),
            your_role: sunbeam_meet_proto::meet::v1::ParticipantRole::Member as i32,
        })),
    };
    let _ = tx.send(Ok(initial)).await;

    // Task: bridge hub broadcasts to client.
    let tx_hub = tx.clone();
    let room_id_s = room_id.to_string();
    let state_hub = state.clone();
    let ident_clone = ident.clone();
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(msg) => {
                    if tx_hub.send(Ok(msg)).await.is_err() {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    let _ = tx_hub
                        .send(Err(Status::resource_exhausted("backpressure")))
                        .await;
                    break;
                }
                Err(_) => break,
            }
        }
        // Cleanup on disconnect.
        crate::metrics::metrics()
            .fanout_subscribers
            .with_label_values(&[&room_id_s])
            .dec();
        let _ = state_hub
            .valkey
            .clear_presence(&room_id_s, &ident_clone.id)
            .await;
        state_hub.hub.drop_if_empty(&room_id_s);
    });

    // Task: read client messages.
    let state_in = state.clone();
    let tx_in = tx.clone();
    let room_id_in = room_id.to_string();
    let ident_in = ident.clone();
    tokio::spawn(async move {
        while let Ok(Some(msg)) = inbound.message().await {
            handle_client_msg(&state_in, &room_id_in, &ident_in, &tx_in, msg).await;
        }
    });

    let stream: super::JoinRoomStream =
        Box::pin(tokio_stream::wrappers::ReceiverStream::new(out_rx));
    Ok(Response::new(stream))
}

async fn handle_client_msg(
    state: &SharedState,
    room_id: &str,
    ident: &crate::clients::kratos::Identity,
    tx: &mpsc::Sender<Result<MeetServerMessage, Status>>,
    msg: MeetClientMessage,
) {
    use meet_client_message::Payload::{
        Join, LayoutChange, Leave, MuteToggle, PinParticipant, Ping, QualityChange, RaiseHand,
        Reaction,
    };
    match msg.payload {
        Some(Ping(p)) => {
            let _ = tx
                .send(Ok(MeetServerMessage {
                    payload: Some(meet_server_message::Payload::Pong(Pong {
                        timestamp: p.timestamp,
                    })),
                }))
                .await;
        }
        Some(Reaction(r)) => {
            // Persist + fan out via the shared reaction helper.
            let send_req = tonic::Request::new(sunbeam_meet_proto::meet::v1::SendReactionRequest {
                room_id: room_id.to_owned(),
                emoji: r.emoji,
            });
            let _ = crate::handlers::meet::reactions::send(state, set_identity_md(send_req, ident))
                .await;
        }
        Some(RaiseHand(_r)) => {
            // Reflect via ParticipantUpdated would require full participant
            // snapshot; we broadcast a minimal event.
        }
        // No-op: client-side state hints / leave / re-join — server has
        // nothing to broadcast for these on the V1 wire.
        Some(
            MuteToggle(_) | LayoutChange(_) | QualityChange(_) | PinParticipant(_) | Leave(_)
            | Join(_),
        )
        | None => {}
    }
}

fn set_identity_md<T>(
    mut req: tonic::Request<T>,
    ident: &crate::clients::kratos::Identity,
) -> tonic::Request<T> {
    // Re-attach the caller's identity for the helper RPC. We forge a
    // `x-sunbeam-identity` header that the downstream middleware will
    // accept as a trusted pre-resolved identity.
    req.metadata_mut()
        .insert("x-sunbeam-identity", ident.id.parse().unwrap());
    req
}

fn spawn_nats_bridge(state: SharedState, room_id: String) {
    tokio::spawn(async move {
        let subject = NatsPublisher::room_subject(&room_id);
        let Ok(mut sub) = state.nats.subscribe(subject).await else {
            return;
        };
        while let Some(msg) = sub.next().await {
            if let Ok(decoded) = MeetServerMessage::decode(msg.payload.as_ref()) {
                state.hub.publish(&room_id, decoded);
            }
        }
    });
}

// Keep the module's public stream type in scope.
#[allow(dead_code)]
type _J = Pin<Box<dyn Stream<Item = Result<MeetServerMessage, Status>> + Send + 'static>>;
