use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use cofounder_api::app::{router, AppState};
use cofounder_api::email::console::RecordingMailer;
use sqlx::PgPool;
use tower::ServiceExt;

#[sqlx::test]
async fn health_returns_ok(pool: PgPool) {
    let app = router(AppState {
        db: pool,
        mailer: Arc::new(RecordingMailer::default()),
        base_url: "http://localhost:3000".into(),
        secure_cookies: false,
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}
