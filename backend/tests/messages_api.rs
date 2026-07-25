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

async fn a_conversation(state: cofounder_api::app::AppState, cookie: &str, target: Uuid) -> String {
    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/conversations")
                .header("cookie", cookie)
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "user_id": target }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    json_body(response).await["id"].as_str().unwrap().to_string()
}

fn say(cookie: &str, conversation: &str, body: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(format!("/conversations/{conversation}/messages"))
        .header("cookie", cookie)
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({ "body": body }).to_string(),
        ))
        .unwrap()
}

#[sqlx::test]
async fn a_message_is_sent_and_read_back(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    complete_profile(&pool, "ada@example.com", "Ada", &["engineering"], &["gtm"]).await;
    let grace = complete_profile(&pool, "grace@example.com", "Grace", &["gtm"], &["engineering"]).await;
    let conversation = a_conversation(state.clone(), &cookie, grace).await;

    let response = router(state.clone())
        .oneshot(say(&cookie, &conversation, "hello there"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let body = json_body(
        router(state)
            .oneshot(get(
                &format!("/conversations/{conversation}/messages"),
                &cookie,
            ))
            .await
            .unwrap(),
    )
    .await;

    let messages = body["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0]["body"], "hello there");
}

#[sqlx::test]
async fn an_empty_message_is_rejected(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    complete_profile(&pool, "ada@example.com", "Ada", &["engineering"], &["gtm"]).await;
    let grace = complete_profile(&pool, "grace@example.com", "Grace", &["gtm"], &["engineering"]).await;
    let conversation = a_conversation(state.clone(), &cookie, grace).await;

    let response = router(state)
        .oneshot(say(&cookie, &conversation, "   "))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = json_body(response).await;
    assert_eq!(body["errors"][0]["field"], "body");
}

#[sqlx::test]
async fn an_overlong_message_is_rejected(pool: PgPool) {
    use cofounder_api::messaging::service::MAX_MESSAGE_LENGTH;

    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    complete_profile(&pool, "ada@example.com", "Ada", &["engineering"], &["gtm"]).await;
    let grace = complete_profile(&pool, "grace@example.com", "Grace", &["gtm"], &["engineering"]).await;
    let conversation = a_conversation(state.clone(), &cookie, grace).await;

    let response = router(state)
        .oneshot(say(&cookie, &conversation, &"x".repeat(MAX_MESSAGE_LENGTH + 1)))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test]
async fn a_stranger_cannot_read_or_write_someone_elses_conversation(pool: PgPool) {
    // 404 rather than 403: whether a conversation exists is itself private.
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());

    let ada_cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    complete_profile(&pool, "ada@example.com", "Ada", &["engineering"], &["gtm"]).await;
    let grace = complete_profile(&pool, "grace@example.com", "Grace", &["gtm"], &["engineering"]).await;
    let conversation = a_conversation(state.clone(), &ada_cookie, grace).await;

    let stranger = sign_in(state.clone(), &mailer, "stranger@example.com").await;
    complete_profile(&pool, "stranger@example.com", "Stranger", &["design"], &["product"]).await;

    let read = router(state.clone())
        .oneshot(get(
            &format!("/conversations/{conversation}/messages"),
            &stranger,
        ))
        .await
        .unwrap();
    assert_eq!(read.status(), StatusCode::NOT_FOUND);

    let write = router(state)
        .oneshot(say(&stranger, &conversation, "let me in"))
        .await
        .unwrap();
    assert_eq!(write.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn a_block_stops_further_messages_in_an_existing_thread(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    let ada = complete_profile(&pool, "ada@example.com", "Ada", &["engineering"], &["gtm"]).await;
    let grace = complete_profile(&pool, "grace@example.com", "Grace", &["gtm"], &["engineering"]).await;
    let conversation = a_conversation(state.clone(), &cookie, grace).await;

    router(state.clone())
        .oneshot(say(&cookie, &conversation, "hello"))
        .await
        .unwrap();

    sqlx::query("INSERT INTO blocks (blocker_id, blocked_id) VALUES ($1, $2)")
        .bind(grace)
        .bind(ada)
        .execute(&pool)
        .await
        .unwrap();

    let response = router(state)
        .oneshot(say(&cookie, &conversation, "still here"))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test]
async fn messages_are_capped_per_minute(pool: PgPool) {
    use cofounder_api::messaging::service::MAX_MESSAGES_PER_MINUTE;

    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    complete_profile(&pool, "ada@example.com", "Ada", &["engineering"], &["gtm"]).await;
    let grace = complete_profile(&pool, "grace@example.com", "Grace", &["gtm"], &["engineering"]).await;
    let conversation = a_conversation(state.clone(), &cookie, grace).await;

    for index in 0..MAX_MESSAGES_PER_MINUTE {
        let response = router(state.clone())
            .oneshot(say(&cookie, &conversation, "spam"))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED, "at {index}");
    }

    let response = router(state)
        .oneshot(say(&cookie, &conversation, "one more"))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(response.headers().contains_key("retry-after"));
}

#[sqlx::test]
async fn replies_within_a_thread_are_not_limited_as_new_conversations(pool: PgPool) {
    // The daily cap counts conversations started, not messages sent.
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    complete_profile(&pool, "ada@example.com", "Ada", &["engineering"], &["gtm"]).await;
    let grace = complete_profile(&pool, "grace@example.com", "Grace", &["gtm"], &["engineering"]).await;
    let conversation = a_conversation(state.clone(), &cookie, grace).await;

    for _ in 0..15 {
        let response = router(state.clone())
            .oneshot(say(&cookie, &conversation, "chatting"))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
    }
}

#[sqlx::test]
async fn opening_a_thread_marks_the_other_persons_messages_read(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());

    let ada_cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    complete_profile(&pool, "ada@example.com", "Ada", &["engineering"], &["gtm"]).await;
    let grace_cookie = sign_in(state.clone(), &mailer, "grace@example.com").await;
    let grace = complete_profile(&pool, "grace@example.com", "Grace", &["gtm"], &["engineering"]).await;
    let conversation = a_conversation(state.clone(), &ada_cookie, grace).await;

    router(state.clone())
        .oneshot(say(&grace_cookie, &conversation, "hello"))
        .await
        .unwrap();

    let before = json_body(
        router(state.clone())
            .oneshot(get("/conversations", &ada_cookie))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(before["conversations"][0]["unread"], 1);

    router(state.clone())
        .oneshot(get(
            &format!("/conversations/{conversation}/messages"),
            &ada_cookie,
        ))
        .await
        .unwrap();

    let after = json_body(
        router(state)
            .oneshot(get("/conversations", &ada_cookie))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(after["conversations"][0]["unread"], 0);
}

#[sqlx::test]
async fn messaging_requires_a_session(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let app = router(state_with(pool, mailer));

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/conversations/{}/messages", Uuid::new_v4()))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
