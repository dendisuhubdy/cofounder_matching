use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use uuid::Uuid;

use crate::app::AppState;
use crate::auth::extractor::CurrentUser;
use crate::error::{ApiError, ApiResult, FieldError};
use crate::swipes::repo::{Direction, MatchedUser, SwipeOutcome};
use crate::swipes::service;

/// The direction arrives as a plain string and is mapped here rather than
/// deserialized into `Direction` directly: a bad value should render as the
/// same 422 problem shape as any other validation failure, not as axum's
/// own JSON rejection.
#[derive(serde::Deserialize)]
pub struct SwipeRequest {
    pub target_id: Uuid,
    pub direction: String,
}

#[derive(serde::Serialize)]
pub struct MatchesView {
    pub matches: Vec<MatchedUser>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/swipes", post(create_swipe))
        .route("/matches", get(list_matches))
}

async fn create_swipe(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(payload): Json<SwipeRequest>,
) -> ApiResult<(StatusCode, Json<SwipeOutcome>)> {
    let direction = match payload.direction.as_str() {
        "left" => Direction::Left,
        "right" => Direction::Right,
        _ => {
            return Err(ApiError::Validation(vec![FieldError {
                field: "direction".into(),
                message: "must be left or right".into(),
            }]))
        }
    };

    let outcome = service::record(&state, user.id, payload.target_id, direction).await?;

    Ok((StatusCode::CREATED, Json(outcome)))
}

async fn list_matches(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> ApiResult<Json<MatchesView>> {
    Ok(Json(MatchesView {
        matches: service::list_matches(&state, user.id).await?,
    }))
}
