//! Chat RPCs — fully wired to Postgres and fan-out.
//
// Proto request/response types are imported wholesale; naming each one is noise.
#![allow(clippy::wildcard_imports)]

use sunbeam_meet_proto::meet::v1::*;
use tonic::{Request, Response, Status};
use uuid::Uuid;

use crate::handlers::meet::{identity, parse_uuid, rooms};
use crate::state::SharedState;
use crate::storage::pg::new_id;

/// Send a chat message.
pub async fn send(
    state: &SharedState,
    req: Request<SendChatRequest>,
) -> Result<Response<ChatMessage>, Status> {
    let id = identity(state, &req).await?;
    let r = req.into_inner();
    if r.content.len() > crate::domain::chat::MAX_BODY_BYTES {
        return Err(Status::invalid_argument("body too large"));
    }
    let room_id = parse_uuid(&r.room_id, "room_id")?;
    crate::authz::can(state, &id, "room", &room_id.to_string(), "participant").await?;

    let reply: Option<Uuid> = if r.reply_to.is_empty() {
        None
    } else {
        Some(parse_uuid(&r.reply_to, "reply_to")?)
    };
    let msg_id = new_id();
    let display_name = id.display_name.clone().unwrap_or_default();

    sqlx::query(
        "INSERT INTO chat_messages (id, room_id, sender_identity, sender_display_name, body, reply_to)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(msg_id)
    .bind(room_id)
    .bind(&id.id)
    .bind(&display_name)
    .bind(&r.content)
    .bind(reply)
    .execute(&state.db)
    .await
    .map_err(|e| Status::internal(e.to_string()))?;

    let chat = load(state, msg_id).await?;
    let broadcast = MeetServerMessage {
        payload: Some(meet_server_message::Payload::ChatMessage(
            ChatMessageBroadcast {
                message: Some(chat.clone()),
            },
        )),
    };
    state.hub.publish(&room_id.to_string(), broadcast);

    Ok(Response::new(chat))
}

/// Fetch chat history.
pub async fn history(
    state: &SharedState,
    req: Request<GetChatHistoryRequest>,
) -> Result<Response<GetChatHistoryResponse>, Status> {
    let id = identity(state, &req).await?;
    let r = req.into_inner();
    let room_id = parse_uuid(&r.room_id, "room_id")?;
    crate::authz::can(state, &id, "room", &room_id.to_string(), "participant").await?;
    let page = i64::from(if r.page_size == 0 {
        100
    } else {
        r.page_size.min(500)
    });

    // `before = None` means "from now" — use Postgres `NOW()` rather than the
    // server-process clock so a clock skew between the app host and the
    // database host can't hide rows that were just inserted via the same DB.
    let before: Option<chrono::DateTime<chrono::Utc>> = r
        .before
        .and_then(|t| chrono::DateTime::<chrono::Utc>::from_timestamp(t.seconds, t.nanos as u32));

    let ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM chat_messages
         WHERE room_id = $1 AND deleted_at IS NULL
               AND created_at < COALESCE($2, NOW())
         ORDER BY created_at DESC LIMIT $3",
    )
    .bind(room_id)
    .bind(before)
    .bind(page)
    .fetch_all(&state.db)
    .await
    .map_err(|e| Status::internal(e.to_string()))?;

    let mut out = Vec::new();
    for i in ids {
        if let Ok(m) = load(state, i).await {
            out.push(m);
        }
    }
    Ok(Response::new(GetChatHistoryResponse {
        messages: out,
        next_page_token: String::new(),
    }))
}

/// Soft-delete a chat message.
pub async fn delete(
    state: &SharedState,
    req: Request<DeleteChatMessageRequest>,
) -> Result<Response<DeleteChatMessageResponse>, Status> {
    let id = identity(state, &req).await?;
    let r = req.into_inner();
    let msg_id = parse_uuid(&r.message_id, "message_id")?;

    let row: Option<(Uuid, String)> =
        sqlx::query_as("SELECT room_id, sender_identity FROM chat_messages WHERE id = $1")
            .bind(msg_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
    let (room_id, sender) = row.ok_or_else(|| Status::not_found("message"))?;
    if sender != id.id {
        crate::authz::can(state, &id, "room", &room_id.to_string(), "moderator").await?;
    }

    sqlx::query("UPDATE chat_messages SET deleted_at = NOW() WHERE id = $1")
        .bind(msg_id)
        .execute(&state.db)
        .await
        .map_err(|e| Status::internal(e.to_string()))?;
    Ok(Response::new(DeleteChatMessageResponse {}))
}

async fn load(state: &SharedState, id: Uuid) -> Result<ChatMessage, Status> {
    let row = sqlx::query_as::<_, Row>(
        "SELECT id, room_id, sender_identity, sender_display_name, body, reply_to, edited_at, deleted_at, created_at
         FROM chat_messages WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| Status::internal(e.to_string()))?
    .ok_or_else(|| Status::not_found("message"))?;

    Ok(ChatMessage {
        id: row.id.to_string(),
        room_id: row.room_id.to_string(),
        sender_identity: row.sender_identity,
        sender_display_name: row.sender_display_name,
        content: row.body,
        reply_to: row.reply_to.map(|u| u.to_string()).unwrap_or_default(),
        edited: row.edited_at.is_some(),
        deleted: row.deleted_at.is_some(),
        sent_at: Some(rooms::ts(row.created_at)),
    })
}

#[derive(sqlx::FromRow)]
struct Row {
    id: Uuid,
    room_id: Uuid,
    sender_identity: String,
    sender_display_name: String,
    body: String,
    reply_to: Option<Uuid>,
    edited_at: Option<chrono::DateTime<chrono::Utc>>,
    deleted_at: Option<chrono::DateTime<chrono::Utc>>,
    created_at: chrono::DateTime<chrono::Utc>,
}
