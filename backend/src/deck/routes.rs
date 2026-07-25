use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};

use crate::app::AppState;
use crate::auth::extractor::CurrentUser;
use crate::deck::service::{self, DeckView};
use crate::error::ApiResult;

pub fn router() -> Router<AppState> {
    Router::new().route("/deck", get(deck))
}

/// Computed on demand. There is no precomputed match table: that is a cache
/// for a load problem this product does not have yet.
async fn deck(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> ApiResult<Json<DeckView>> {
    Ok(Json(service::build(&state, user.id).await?))
}
