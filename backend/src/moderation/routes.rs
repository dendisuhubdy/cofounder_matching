use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use uuid::Uuid;

use crate::app::AppState;
use crate::auth::extractor::CurrentUser;
use crate::error::{ApiError, ApiResult, FieldError};
use crate::moderation::repo;
use crate::moderation::vocab::REPORT_REASONS;
use crate::profiles::vocab;
use crate::users;

#[derive(serde::Deserialize)]
pub struct BlockRequest {
    pub user_id: Uuid,
}

#[derive(serde::Deserialize)]
pub struct ReportRequest {
    pub user_id: Uuid,
    pub reason: String,
    #[serde(default)]
    pub body: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/blocks", post(create_block))
        .route("/reports", post(create_report))
}

/// Shared by both endpoints: you cannot act on yourself, and the person has
/// to exist.
async fn target(state: &AppState, actor: Uuid, subject: Uuid) -> ApiResult<()> {
    if actor == subject {
        return Err(ApiError::Validation(vec![FieldError {
            field: "user_id".into(),
            message: "you cannot do that to yourself".into(),
        }]));
    }

    if users::repo::find_by_id(&state.db, subject).await?.is_none() {
        return Err(ApiError::NotFound);
    }

    Ok(())
}

async fn create_block(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(payload): Json<BlockRequest>,
) -> ApiResult<StatusCode> {
    target(&state, user.id, payload.user_id).await?;

    repo::block(&state.db, user.id, payload.user_id).await?;

    Ok(StatusCode::NO_CONTENT)
}

async fn create_report(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(payload): Json<ReportRequest>,
) -> ApiResult<StatusCode> {
    target(&state, user.id, payload.user_id).await?;

    if !vocab::contains(&REPORT_REASONS, &payload.reason) {
        return Err(ApiError::Validation(vec![FieldError {
            field: "reason".into(),
            message: "is not one of the available reasons".into(),
        }]));
    }

    let body = payload.body.trim();
    if body.chars().count() > 2000 {
        return Err(ApiError::Validation(vec![FieldError {
            field: "body".into(),
            message: "must be 2000 characters or fewer".into(),
        }]));
    }

    repo::report(&state.db, user.id, payload.user_id, &payload.reason, body).await?;

    Ok(StatusCode::NO_CONTENT)
}
