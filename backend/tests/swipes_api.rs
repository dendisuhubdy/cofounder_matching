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

fn swipe(cookie: &str, target: Uuid, direction: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/swipes")
        .header("cookie", cookie)
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({ "target_id": target, "direction": direction }).to_string(),
        ))
        .unwrap()
}

#[sqlx::test]
async fn swiping_requires_a_session(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let app = router(state_with(pool, mailer));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/swipes")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "target_id": Uuid::new_v4(),
                        "direction": "right"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn a_swipe_is_recorded(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    let grace = complete_profile(
        &pool,
        "grace@example.com",
        "Grace",
        &["engineering"],
        &["gtm"],
    )
    .await;

    let response = router(state)
        .oneshot(swipe(&cookie, grace, "right"))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = json_body(response).await;
    assert_eq!(body["matched"], false);
}

#[sqlx::test]
async fn a_mutual_right_swipe_reports_a_match(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());

    let ada_cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    let ada = complete_profile(&pool, "ada@example.com", "Ada", &["engineering"], &["gtm"]).await;
    let grace_cookie = sign_in(state.clone(), &mailer, "grace@example.com").await;
    let grace = complete_profile(
        &pool,
        "grace@example.com",
        "Grace",
        &["engineering"],
        &["gtm"],
    )
    .await;

    let first = router(state.clone())
        .oneshot(swipe(&ada_cookie, grace, "right"))
        .await
        .unwrap();
    assert_eq!(json_body(first).await["matched"], false);

    let second = router(state)
        .oneshot(swipe(&grace_cookie, ada, "right"))
        .await
        .unwrap();
    assert_eq!(json_body(second).await["matched"], true);
}

#[sqlx::test]
async fn swiping_the_same_person_twice_is_a_conflict(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    let grace = complete_profile(
        &pool,
        "grace@example.com",
        "Grace",
        &["engineering"],
        &["gtm"],
    )
    .await;

    router(state.clone())
        .oneshot(swipe(&cookie, grace, "right"))
        .await
        .unwrap();

    let repeat = router(state)
        .oneshot(swipe(&cookie, grace, "left"))
        .await
        .unwrap();

    assert_eq!(repeat.status(), StatusCode::CONFLICT);
    let body = json_body(repeat).await;
    assert_eq!(body["type"], "conflict");
}

#[sqlx::test]
async fn swiping_on_yourself_is_rejected(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    let ada = complete_profile(&pool, "ada@example.com", "Ada", &["engineering"], &["gtm"]).await;

    let response = router(state)
        .oneshot(swipe(&cookie, ada, "right"))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test]
async fn swiping_on_a_stranger_who_does_not_exist_is_a_404(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let response = router(state)
        .oneshot(swipe(&cookie, Uuid::new_v4(), "right"))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn an_unknown_direction_is_rejected(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    let grace = complete_profile(
        &pool,
        "grace@example.com",
        "Grace",
        &["engineering"],
        &["gtm"],
    )
    .await;

    let response = router(state)
        .oneshot(swipe(&cookie, grace, "sideways"))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test]
async fn matches_are_listed_for_both_sides(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());

    let ada_cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;
    let ada = complete_profile(&pool, "ada@example.com", "Ada", &["engineering"], &["gtm"]).await;
    let grace_cookie = sign_in(state.clone(), &mailer, "grace@example.com").await;
    let grace = complete_profile(
        &pool,
        "grace@example.com",
        "Grace",
        &["engineering"],
        &["gtm"],
    )
    .await;

    router(state.clone())
        .oneshot(swipe(&ada_cookie, grace, "right"))
        .await
        .unwrap();
    router(state.clone())
        .oneshot(swipe(&grace_cookie, ada, "right"))
        .await
        .unwrap();

    let for_ada = json_body(
        router(state.clone())
            .oneshot(get("/matches", &ada_cookie))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(for_ada["matches"][0]["display_name"], "Grace");

    let for_grace = json_body(
        router(state)
            .oneshot(get("/matches", &grace_cookie))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(for_grace["matches"][0]["display_name"], "Ada");
}

#[sqlx::test]
async fn matches_start_empty(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let body = json_body(
        router(state)
            .oneshot(get("/matches", &cookie))
            .await
            .unwrap(),
    )
    .await;

    assert_eq!(body["matches"].as_array().unwrap().len(), 0);
}

#[sqlx::test]
async fn matches_require_a_session(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let app = router(state_with(pool, mailer));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/matches")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
