use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};

use crate::app::AppState;
use crate::auth::service;
use crate::error::ApiResult;

#[derive(serde::Deserialize)]
pub struct MagicLinkRequest {
    pub email: String,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/auth/magic-link", post(request_magic_link))
}

async fn request_magic_link(
    State(state): State<AppState>,
    Json(payload): Json<MagicLinkRequest>,
) -> ApiResult<StatusCode> {
    service::request_login_link(&state, &payload.email).await?;

    // 202 regardless of whether the address was already registered, so the
    // response cannot be used to enumerate accounts.
    Ok(StatusCode::ACCEPTED)
}
