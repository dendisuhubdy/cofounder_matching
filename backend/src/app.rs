use std::sync::Arc;

use axum::extract::State;
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
}

pub fn router(state: AppState) -> Router {
    let mut app = Router::new()
        .route("/health", get(health))
        .merge(crate::auth::routes::router())
        .merge(crate::assessment::routes::router())
        .merge(crate::profiles::routes::router());

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

async fn last_login_link(State(state): State<AppState>) -> Json<serde_json::Value> {
    let link = state.test_mailer.as_ref().and_then(|mailer| mailer.last());

    Json(json!({ "link": link }))
}
