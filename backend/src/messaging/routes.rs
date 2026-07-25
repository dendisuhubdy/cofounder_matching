use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
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
