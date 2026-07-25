use uuid::Uuid;

use crate::app::AppState;
use crate::error::{ApiError, ApiResult, FieldError};
use crate::swipes::repo::{self, Direction, MatchedUser, SwipeOutcome};
use crate::users;

pub async fn record(
    state: &AppState,
    swiper_id: Uuid,
    target_id: Uuid,
    direction: Direction,
) -> ApiResult<SwipeOutcome> {
    if swiper_id == target_id {
        return Err(ApiError::Validation(vec![FieldError {
            field: "target_id".into(),
            message: "you cannot swipe on yourself".into(),
        }]));
    }

    if users::repo::find_by_id(&state.db, target_id).await?.is_none() {
        return Err(ApiError::NotFound);
    }

    repo::record_swipe(&state.db, swiper_id, target_id, direction)
        .await?
        .ok_or(ApiError::Conflict)
}

pub async fn list_matches(state: &AppState, user_id: Uuid) -> ApiResult<Vec<MatchedUser>> {
    Ok(repo::matches_for(&state.db, user_id).await?)
}
