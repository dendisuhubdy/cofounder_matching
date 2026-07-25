use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use cofounder_api::app::router;
use cofounder_api::email::console::RecordingMailer;
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

mod common;
use common::*;

fn post_to(uri: &str, cookie: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("cookie", cookie)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

#[sqlx::test]
async fn blocking_someone_records_it(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    let ada = complete_profile(&pool, "ada@example.com", "Ada", &["engineering"], &["gtm"]).await;
    let grace = complete_profile(&pool, "grace@example.com", "Grace", &["gtm"], &["engineering"]).await;

    let response = router(state)
        .oneshot(post_to("/blocks", &cookie, serde_json::json!({ "user_id": grace })))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM blocks WHERE blocker_id = $1 AND blocked_id = $2)",
    )
    .bind(ada)
    .bind(grace)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(exists);
}

#[sqlx::test]
async fn blocking_twice_is_not_an_error(pool: PgPool) {
    // The button may be pressed twice; the second press is not a failure.
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    complete_profile(&pool, "ada@example.com", "Ada", &["engineering"], &["gtm"]).await;
    let grace = complete_profile(&pool, "grace@example.com", "Grace", &["gtm"], &["engineering"]).await;

    for _ in 0..2 {
        let response = router(state.clone())
            .oneshot(post_to(
                "/blocks",
                &cookie,
                serde_json::json!({ "user_id": grace }),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM blocks")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 1);
}

#[sqlx::test]
async fn you_cannot_block_yourself(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    let ada = complete_profile(&pool, "ada@example.com", "Ada", &["engineering"], &["gtm"]).await;

    let response = router(state)
        .oneshot(post_to("/blocks", &cookie, serde_json::json!({ "user_id": ada })))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test]
async fn a_block_removes_them_from_the_deck(pool: PgPool) {
    // The end-to-end point of blocking: they stop being shown to each other.
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    complete_profile(&pool, "ada@example.com", "Ada", &["engineering"], &["gtm"]).await;
    let grace = complete_profile(&pool, "grace@example.com", "Grace", &["gtm"], &["engineering"]).await;

    let before = json_body(
        router(state.clone())
            .oneshot(get("/deck", &cookie))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(before["cards"].as_array().unwrap().len(), 1);

    router(state.clone())
        .oneshot(post_to(
            "/blocks",
            &cookie,
            serde_json::json!({ "user_id": grace }),
        ))
        .await
        .unwrap();

    let after = json_body(router(state).oneshot(get("/deck", &cookie)).await.unwrap()).await;
    assert_eq!(after["cards"].as_array().unwrap().len(), 0);
}

#[sqlx::test]
async fn a_report_is_queued_for_review(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    complete_profile(&pool, "ada@example.com", "Ada", &["engineering"], &["gtm"]).await;
    let grace = complete_profile(&pool, "grace@example.com", "Grace", &["gtm"], &["engineering"]).await;

    let response = router(state)
        .oneshot(post_to(
            "/reports",
            &cookie,
            serde_json::json!({
                "user_id": grace,
                "reason": "harassment",
                "body": "Repeated unwanted messages."
            }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let (reason, status): (String, String) =
        sqlx::query_as("SELECT reason, status FROM reports")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(reason, "harassment");
    assert_eq!(status, "pending", "reports never act automatically");
}

#[sqlx::test]
async fn a_report_does_not_suspend_anyone(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    complete_profile(&pool, "ada@example.com", "Ada", &["engineering"], &["gtm"]).await;
    let grace = complete_profile(&pool, "grace@example.com", "Grace", &["gtm"], &["engineering"]).await;

    router(state)
        .oneshot(post_to(
            "/reports",
            &cookie,
            serde_json::json!({ "user_id": grace, "reason": "spam", "body": "" }),
        ))
        .await
        .unwrap();

    let status: String = sqlx::query_scalar("SELECT status FROM users WHERE id = $1")
        .bind(grace)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "active");
}

#[sqlx::test]
async fn an_unknown_report_reason_is_rejected(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    complete_profile(&pool, "ada@example.com", "Ada", &["engineering"], &["gtm"]).await;
    let grace = complete_profile(&pool, "grace@example.com", "Grace", &["gtm"], &["engineering"]).await;

    let response = router(state)
        .oneshot(post_to(
            "/reports",
            &cookie,
            serde_json::json!({ "user_id": grace, "reason": "vibes", "body": "" }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = json_body(response).await;
    assert_eq!(body["errors"][0]["field"], "reason");
}

#[sqlx::test]
async fn options_lists_the_report_reasons(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let body = json_body(router(state).oneshot(get("/options", &cookie)).await.unwrap()).await;

    assert_eq!(body["report_reasons"].as_array().unwrap().len(), 5);
    assert!(body["report_reasons"][0]["label"].is_string());
}

#[sqlx::test]
async fn moderation_requires_a_session(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let app = router(state_with(pool, mailer));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/blocks")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "user_id": Uuid::new_v4() }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
