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

/// A complete profile, inserted directly so the test does not have to drive
/// eighteen questionnaire answers through the API.
async fn complete_profile(pool: &PgPool, email: &str, name: &str) -> Uuid {
    let id = cofounder_api::users::repo::find_or_create_by_email(pool, email)
        .await
        .unwrap()
        .id;

    sqlx::query(
        "INSERT INTO profiles (user_id, display_name, headline, bio, roles, seeking_roles, commitment)
         VALUES ($1, $2, 'Building things', 'A real bio.', ARRAY['engineering'], ARRAY['gtm'], 'full_time_now')
         ON CONFLICT (user_id) DO UPDATE SET display_name = EXCLUDED.display_name",
    )
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
    let grace = complete_profile(&pool, "grace@example.com", "Grace").await;

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
    let ada = complete_profile(&pool, "ada@example.com", "Ada").await;
    let grace_cookie = sign_in(state.clone(), &mailer, "grace@example.com").await;
    let grace = complete_profile(&pool, "grace@example.com", "Grace").await;

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
    let grace = complete_profile(&pool, "grace@example.com", "Grace").await;

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
    let ada = complete_profile(&pool, "ada@example.com", "Ada").await;

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
    let grace = complete_profile(&pool, "grace@example.com", "Grace").await;

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
    let ada = complete_profile(&pool, "ada@example.com", "Ada").await;
    let grace_cookie = sign_in(state.clone(), &mailer, "grace@example.com").await;
    let grace = complete_profile(&pool, "grace@example.com", "Grace").await;

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

    let body = json_body(router(state).oneshot(get("/matches", &cookie)).await.unwrap()).await;

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
