use axum::body::Body;
use axum::http::{Request, StatusCode};
use cofounder_api::app::{router, AppState};
use sqlx::PgPool;
use tower::ServiceExt;

#[sqlx::test]
async fn health_returns_ok(pool: PgPool) {
    let app = router(AppState { db: pool });

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
