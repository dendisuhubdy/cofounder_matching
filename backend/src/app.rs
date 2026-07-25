use std::sync::Arc;

use axum::{routing::get, Router};
use sqlx::PgPool;

use crate::email::Mailer;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub mailer: Arc<dyn Mailer>,
    pub base_url: String,
    pub secure_cookies: bool,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .merge(crate::auth::routes::router())
        .with_state(state)
}

async fn health() -> &'static str {
    "ok"
}
