use std::sync::Arc;

use axum::extract::{Query, State};
use axum::{routing::get, Json, Router};
use serde_json::json;
use sqlx::PgPool;

use crate::email::console::LastLinkMailer;
use crate::email::Mailer;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub mailer: Arc<dyn Mailer>,
    pub base_url: String,
    pub secure_cookies: bool,
    /// Present only when APP_ENV=test. Its presence is what mounts the
    /// test-only route below.
    pub test_mailer: Option<Arc<LastLinkMailer>>,
    /// In-process fan-out for SSE. See `messaging::events::EventBus` for why
    /// this constrains the backend to a single process.
    pub events: crate::messaging::events::EventBus,
}

pub fn router(state: AppState) -> Router {
    let mut app = Router::new()
        .route("/health", get(health))
        .merge(crate::auth::routes::router())
        .merge(crate::assessment::routes::router())
        .merge(crate::profiles::routes::router())
        .merge(crate::deck::routes::router())
        .merge(crate::swipes::routes::router())
        .merge(crate::messaging::routes::router());

    if state.test_mailer.is_some() {
        app = app.merge(test_router());
    }

    app.with_state(state)
}

async fn health() -> &'static str {
    "ok"
}

/// Mounted only in the test environment. Exposing this anywhere else would
/// hand every account to anyone who could reach the endpoint.
pub fn test_router() -> Router<AppState> {
    Router::new().route("/test/last-login-link", get(last_login_link))
}

#[derive(serde::Deserialize)]
pub struct LastLinkQuery {
    /// Required. Asking for "the last link" without naming a recipient is
    /// only safe when one sign-in is ever in flight, which stopped being true
    /// as soon as the e2e suite ran spec files in parallel.
    pub email: String,
}

async fn last_login_link(
    State(state): State<AppState>,
    Query(query): Query<LastLinkQuery>,
) -> Json<serde_json::Value> {
    let link = state
        .test_mailer
        .as_ref()
        .and_then(|mailer| mailer.last_for(&query.email));

    Json(json!({ "link": link }))
}
