use std::convert::Infallible;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::routing::get;
use axum::{Json, Router};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::{Stream, StreamExt};
use uuid::Uuid;

use crate::app::AppState;
use crate::auth::extractor::CurrentUser;
use crate::error::ApiResult;
use crate::messaging::repo::{ConversationSummary, Message};
use crate::messaging::service;

#[derive(serde::Deserialize)]
pub struct OpenConversationRequest {
    pub user_id: Uuid,
}

#[derive(serde::Serialize)]
pub struct OpenedConversation {
    pub id: Uuid,
    /// False when the thread already existed, which the frontend uses to
    /// decide between "started a conversation" and simply navigating to it.
    pub created: bool,
}

#[derive(serde::Serialize)]
pub struct ConversationsView {
    pub conversations: Vec<ConversationSummary>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/conversations", get(list).post(open))
        .route(
            "/conversations/{id}/messages",
            get(read_messages).post(send_message),
        )
        .route("/events", get(events))
}

async fn list(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> ApiResult<Json<ConversationsView>> {
    Ok(Json(ConversationsView {
        conversations: service::list(&state, user.id).await?,
    }))
}

async fn open(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(payload): Json<OpenConversationRequest>,
) -> ApiResult<(StatusCode, Json<OpenedConversation>)> {
    let (conversation, created) =
        service::open_conversation(&state, user.id, payload.user_id).await?;

    let status = if created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };

    Ok((
        status,
        Json(OpenedConversation {
            id: conversation.id,
            created,
        }),
    ))
}

#[derive(serde::Deserialize)]
pub struct SendMessageRequest {
    pub body: String,
}

#[derive(serde::Serialize)]
pub struct MessagesView {
    pub messages: Vec<Message>,
}

async fn read_messages(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<MessagesView>> {
    Ok(Json(MessagesView {
        messages: service::read_thread(&state, id, user.id).await?,
    }))
}

async fn send_message(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
    Json(payload): Json<SendMessageRequest>,
) -> ApiResult<(StatusCode, Json<Message>)> {
    let message = service::send_message(&state, id, user.id, &payload.body).await?;

    Ok((StatusCode::CREATED, Json(message)))
}

/// A stream of this user's events. Subscribing before filtering means every
/// connected client sees every envelope, so the `recipient_id` check here is
/// what keeps one person's messages out of another's stream.
async fn events(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>>> {
    let user_id = user.id;

    let stream = BroadcastStream::new(state.events.subscribe()).filter_map(move |result| {
        // A lagged subscriber yields an error rather than an envelope. Drop
        // it: the client refetches on reconnect, and killing the stream
        // would be worse than missing one notification.
        let envelope = result.ok()?;
        if envelope.recipient_id != user_id {
            return None;
        }
        Some(Ok(SseEvent::default().json_data(envelope.event).ok()?))
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}
