use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use cofounder_api::app::{router, AppState};
use cofounder_api::email::console::RecordingMailer;
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

fn state_with(pool: PgPool, mailer: Arc<RecordingMailer>) -> AppState {
    AppState {
        db: pool,
        mailer,
        base_url: "http://localhost:3000".into(),
        secure_cookies: false,
        test_mailer: None,
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

async fn sign_in(state: AppState, mailer: &RecordingMailer, email: &str) -> String {
    router(state.clone())
        .oneshot(post_json(
            "/auth/magic-link",
            serde_json::json!({ "email": email }),
        ))
        .await
        .unwrap();

    let link = mailer.sent().last().unwrap().1.clone();
    let token = link.split("token=").nth(1).unwrap().to_string();

    let response = router(state)
        .oneshot(post_json(
            "/auth/verify",
            serde_json::json!({ "token": token }),
        ))
        .await
        .unwrap();

    response
        .headers()
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string()
}

fn get(uri: &str, cookie: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .header("cookie", cookie)
        .body(Body::empty())
        .unwrap()
}

async fn json_body(response: axum::response::Response) -> serde_json::Value {
    let bytes = http_body_util::BodyExt::collect(response.into_body())
        .await
        .unwrap()
        .to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

/// Inserts a complete profile directly. `roles`/`seeking` are passed so a
/// test can make one candidate a better fit than another.
async fn complete_profile(
    pool: &PgPool,
    email: &str,
    name: &str,
    roles: &str,
    seeking: &str,
) -> Uuid {
    let id = cofounder_api::users::repo::find_or_create_by_email(pool, email)
        .await
        .unwrap()
        .id;

    sqlx::query(&format!(
        "INSERT INTO profiles (user_id, display_name, headline, bio, city, country,
                               timezone, utc_offset_minutes, roles, seeking_roles,
                               idea_status, stage, commitment)
         VALUES ($1, $2, 'Building things', 'A real bio.', 'Jakarta', 'Indonesia',
                 'Asia/Jakarta', 420, ARRAY['{roles}'], ARRAY['{seeking}'],
                 'committed_idea', 'prototype', 'full_time_now')
         ON CONFLICT (user_id) DO UPDATE SET
             display_name = EXCLUDED.display_name,
             roles = EXCLUDED.roles,
             seeking_roles = EXCLUDED.seeking_roles"
    ))
    .bind(id)
    .bind(name)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO trait_scores (user_id, risk_tolerance, pace_vs_rigor, conflict_style,
                                   decision_basis, work_mode, orientation)
         VALUES ($1, 50, 50, 50, 50, 50, 50)
         ON CONFLICT (user_id) DO NOTHING",
    )
    .bind(id)
    .execute(pool)
    .await
    .unwrap();

    id
}

#[sqlx::test]
async fn the_deck_requires_a_session(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let app = router(state_with(pool, mailer));

    let response = app
        .oneshot(Request::builder().uri("/deck").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn an_incomplete_viewer_gets_an_empty_deck_and_is_told_why(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let response = router(state).oneshot(get("/deck", &cookie)).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["profile_complete"], false);
    assert_eq!(body["cards"].as_array().unwrap().len(), 0);
}

#[sqlx::test]
async fn a_complete_viewer_sees_a_scored_card(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    complete_profile(&pool, "ada@example.com", "Ada", "engineering", "gtm").await;
    complete_profile(&pool, "grace@example.com", "Grace", "gtm", "engineering").await;

    let response = router(state).oneshot(get("/deck", &cookie)).await.unwrap();
    let body = json_body(response).await;

    assert_eq!(body["profile_complete"], true);
    let cards = body["cards"].as_array().unwrap();
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0]["display_name"], "Grace");
    assert!(cards[0]["score"].as_u64().unwrap() > 0);
    assert!(!cards[0]["reasons"].as_array().unwrap().is_empty());
    assert!(cards[0]["reasons"][0]["text"].is_string());
}

#[sqlx::test]
async fn a_better_fit_is_ranked_first(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    complete_profile(&pool, "ada@example.com", "Ada", "engineering", "gtm").await;
    // Wants what Ada has, has what Ada wants.
    complete_profile(&pool, "great@example.com", "Great", "gtm", "engineering").await;
    // Neither.
    complete_profile(&pool, "poor@example.com", "Poor", "research", "design").await;

    let response = router(state).oneshot(get("/deck", &cookie)).await.unwrap();
    let body = json_body(response).await;
    let cards = body["cards"].as_array().unwrap();

    assert_eq!(cards.len(), 2);
    assert_eq!(cards[0]["display_name"], "Great");
    assert!(
        cards[0]["score"].as_u64().unwrap() > cards[1]["score"].as_u64().unwrap(),
        "{cards:?}"
    );
}

#[sqlx::test]
async fn the_deck_never_contains_the_viewer(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    complete_profile(&pool, "ada@example.com", "Ada", "engineering", "gtm").await;

    let response = router(state).oneshot(get("/deck", &cookie)).await.unwrap();
    let body = json_body(response).await;

    assert_eq!(body["cards"].as_array().unwrap().len(), 0);
}

#[sqlx::test]
async fn the_deck_is_capped(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    complete_profile(&pool, "ada@example.com", "Ada", "engineering", "gtm").await;
    for index in 0..25 {
        complete_profile(
            &pool,
            &format!("other{index}@example.com"),
            "Other",
            "gtm",
            "engineering",
        )
        .await;
    }

    let response = router(state).oneshot(get("/deck", &cookie)).await.unwrap();
    let body = json_body(response).await;

    assert_eq!(
        body["cards"].as_array().unwrap().len(),
        cofounder_api::deck::service::DECK_SIZE
    );
}

#[sqlx::test]
async fn a_score_never_leaves_the_zero_to_one_hundred_range(pool: PgPool) {
    // The popularity boost is added after scoring, so a perfect pair must
    // still not be able to exceed the budget.
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    complete_profile(&pool, "ada@example.com", "Ada", "engineering", "gtm").await;
    let grace = complete_profile(&pool, "grace@example.com", "Grace", "gtm", "engineering").await;

    for index in 0..5 {
        let fan = complete_profile(
            &pool,
            &format!("fan{index}@example.com"),
            "Fan",
            "gtm",
            "engineering",
        )
        .await;
        cofounder_api::swipes::repo::record_swipe(
            &pool,
            fan,
            grace,
            cofounder_api::swipes::repo::Direction::Right,
        )
        .await
        .unwrap();
    }

    let response = router(state).oneshot(get("/deck", &cookie)).await.unwrap();
    let body = json_body(response).await;

    for card in body["cards"].as_array().unwrap() {
        let score = card["score"].as_u64().unwrap();
        assert!(score <= 100, "got {score}");
    }
}
