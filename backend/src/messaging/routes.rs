use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use uuid::Uuid;

use crate::app::AppState;
use crate::auth::extractor::CurrentUser;
use crate::error::ApiResult;
use crate::messaging::repo::ConversationSummary;
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
    Router::new().route("/conversations", get(list).post(open))
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
