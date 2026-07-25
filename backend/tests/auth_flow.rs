use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use cofounder_api::app::{router, AppState};
use cofounder_api::email::console::RecordingMailer;
use sqlx::PgPool;
use tower::ServiceExt;

fn state_with(pool: PgPool, mailer: Arc<RecordingMailer>) -> AppState {
    AppState {
        db: pool,
        mailer,
        base_url: "http://localhost:3000".into(),
        secure_cookies: false,
    }
}

fn post_json(uri: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

#[sqlx::test]
async fn magic_link_request_returns_202_and_sends_a_link(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let app = router(state_with(pool, mailer.clone()));

    let response = app
        .oneshot(post_json(
            "/auth/magic-link",
            serde_json::json!({ "email": "ada@example.com" }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let sent = mailer.sent();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0, "ada@example.com");
    assert!(sent[0].1.starts_with("http://localhost:3000/verify?token="));
}

#[sqlx::test]
async fn magic_link_request_creates_the_user(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let app = router(state_with(pool.clone(), mailer));

    app.oneshot(post_json(
        "/auth/magic-link",
        serde_json::json!({ "email": "ada@example.com" }),
    ))
    .await
    .unwrap();

    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM users WHERE email = $1")
        .bind("ada@example.com")
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(count, 1);
}

#[sqlx::test]
async fn magic_link_stores_only_the_hash_never_the_token(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let app = router(state_with(pool.clone(), mailer.clone()));

    app.oneshot(post_json(
        "/auth/magic-link",
        serde_json::json!({ "email": "ada@example.com" }),
    ))
    .await
    .unwrap();

    let link = mailer.sent()[0].1.clone();
    let token = link.split("token=").nth(1).unwrap().to_string();

    let stored: Vec<u8> = sqlx::query_scalar("SELECT token_hash FROM magic_link_tokens")
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_ne!(stored, token.as_bytes().to_vec());
    assert_eq!(stored, cofounder_api::auth::tokens::hash_token(&token));
}

#[sqlx::test]
async fn magic_link_rejects_a_malformed_email(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let app = router(state_with(pool, mailer.clone()));

    let response = app
        .oneshot(post_json(
            "/auth/magic-link",
            serde_json::json!({ "email": "not-an-email" }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert!(mailer.sent().is_empty());
}
