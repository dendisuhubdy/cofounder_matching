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

fn open_with(cookie: &str, target: Uuid) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/conversations")
        .header("cookie", cookie)
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({ "user_id": target }).to_string(),
        ))
        .unwrap()
}

#[sqlx::test]
async fn conversations_require_a_session(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let app = router(state_with(pool, mailer));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/conversations")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn a_conversation_can_be_opened_without_a_match(pool: PgPool) {
    // The central decision in the design: messaging is not gated on matching.
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    complete_profile(&pool, "ada@example.com", "Ada", &["engineering"], &["gtm"]).await;
    let grace = complete_profile(&pool, "grace@example.com", "Grace", &["gtm"], &["engineering"]).await;

    let response = router(state)
        .oneshot(open_with(&cookie, grace))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = json_body(response).await;
    assert!(body["id"].is_string());
    assert_eq!(body["created"], true);
}

#[sqlx::test]
async fn opening_the_same_conversation_twice_returns_the_same_one(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    complete_profile(&pool, "ada@example.com", "Ada", &["engineering"], &["gtm"]).await;
    let grace = complete_profile(&pool, "grace@example.com", "Grace", &["gtm"], &["engineering"]).await;

    let first = json_body(
        router(state.clone())
            .oneshot(open_with(&cookie, grace))
            .await
            .unwrap(),
    )
    .await;

    let response = router(state).oneshot(open_with(&cookie, grace)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let second = json_body(response).await;

    assert_eq!(first["id"], second["id"]);
    assert_eq!(second["created"], false);
}

#[sqlx::test]
async fn an_incomplete_profile_cannot_open_a_conversation(pool: PgPool) {
    // The completeness requirement is the primary spam filter.
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    let grace = complete_profile(&pool, "grace@example.com", "Grace", &["gtm"], &["engineering"]).await;

    let response = router(state)
        .oneshot(open_with(&cookie, grace))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = json_body(response).await;
    assert_eq!(body["type"], "profile_incomplete");
}

#[sqlx::test]
async fn you_cannot_open_a_conversation_with_yourself(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    let ada = complete_profile(&pool, "ada@example.com", "Ada", &["engineering"], &["gtm"]).await;

    let response = router(state).oneshot(open_with(&cookie, ada)).await.unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test]
async fn a_block_prevents_opening_in_either_direction(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    let ada = complete_profile(&pool, "ada@example.com", "Ada", &["engineering"], &["gtm"]).await;
    let grace = complete_profile(&pool, "grace@example.com", "Grace", &["gtm"], &["engineering"]).await;

    sqlx::query("INSERT INTO blocks (blocker_id, blocked_id) VALUES ($1, $2)")
        .bind(grace)
        .bind(ada)
        .execute(&pool)
        .await
        .unwrap();

    let response = router(state)
        .oneshot(open_with(&cookie, grace))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test]
async fn new_conversations_are_capped_per_day(pool: PgPool) {
    use cofounder_api::messaging::service::MAX_NEW_CONVERSATIONS_PER_DAY;

    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    complete_profile(&pool, "ada@example.com", "Ada", &["engineering"], &["gtm"]).await;

    for index in 0..MAX_NEW_CONVERSATIONS_PER_DAY {
        let target =
            complete_profile(&pool, &format!("other{index}@example.com"), "Other", &["gtm"], &["engineering"]).await;
        let response = router(state.clone())
            .oneshot(open_with(&cookie, target))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED, "at {index}");
    }

    let one_too_many =
        complete_profile(&pool, "onetoomany@example.com", "One Too Many", &["gtm"], &["engineering"]).await;
    let response = router(state)
        .oneshot(open_with(&cookie, one_too_many))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(response.headers().contains_key("retry-after"));
}

#[sqlx::test]
async fn being_messaged_does_not_consume_your_own_allowance(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());

    let ada_cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    complete_profile(&pool, "ada@example.com", "Ada", &["engineering"], &["gtm"]).await;
    let grace_cookie = sign_in(state.clone(), &mailer, "grace@example.com").await;
    let grace = complete_profile(&pool, "grace@example.com", "Grace", &["gtm"], &["engineering"]).await;

    router(state.clone())
        .oneshot(open_with(&ada_cookie, grace))
        .await
        .unwrap();

    // Grace has been messaged once but has started nothing.
    let hopper = complete_profile(&pool, "hopper@example.com", "Hopper", &["product"], &["design"]).await;
    let response = router(state)
        .oneshot(open_with(&grace_cookie, hopper))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
}

#[sqlx::test]
async fn reopening_an_existing_thread_is_not_a_new_conversation(pool: PgPool) {
    use cofounder_api::messaging::service::MAX_NEW_CONVERSATIONS_PER_DAY;

    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    complete_profile(&pool, "ada@example.com", "Ada", &["engineering"], &["gtm"]).await;

    let first = complete_profile(&pool, "first@example.com", "First", &["gtm"], &["engineering"]).await;

    for _ in 0..(MAX_NEW_CONVERSATIONS_PER_DAY * 2) {
        let response = router(state.clone())
            .oneshot(open_with(&cookie, first))
            .await
            .unwrap();
        assert_ne!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    }
}

#[sqlx::test]
async fn the_conversation_list_shows_the_other_person(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    complete_profile(&pool, "ada@example.com", "Ada", &["engineering"], &["gtm"]).await;
    let grace = complete_profile(&pool, "grace@example.com", "Grace", &["gtm"], &["engineering"]).await;

    router(state.clone())
        .oneshot(open_with(&cookie, grace))
        .await
        .unwrap();

    let body = json_body(
        router(state)
            .oneshot(get("/conversations", &cookie))
            .await
            .unwrap(),
    )
    .await;

    let conversations = body["conversations"].as_array().unwrap();
    assert_eq!(conversations.len(), 1);
    assert_eq!(conversations[0]["other_display_name"], "Grace");
    assert_eq!(conversations[0]["unread"], 0);
}

#[sqlx::test]
async fn the_conversation_list_starts_empty(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let body = json_body(
        router(state)
            .oneshot(get("/conversations", &cookie))
            .await
            .unwrap(),
    )
    .await;

    assert_eq!(body["conversations"].as_array().unwrap().len(), 0);
}
