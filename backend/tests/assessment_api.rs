use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use cofounder_api::app::{router, AppState};
use cofounder_api::assessment::questions::QUESTIONS;
use cofounder_api::email::console::RecordingMailer;
use sqlx::PgPool;
use tower::ServiceExt;

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

/// Signs a user in and returns the `session=...` cookie pair.
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

    let cookie = response
        .headers()
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap();

    cookie.split(';').next().unwrap().to_string()
}

fn get(uri: &str, cookie: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .header("cookie", cookie)
        .body(Body::empty())
        .unwrap()
}

fn put_json(uri: &str, cookie: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method("PUT")
        .uri(uri)
        .header("cookie", cookie)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

async fn json_body(response: axum::response::Response) -> serde_json::Value {
    let bytes = http_body_util::BodyExt::collect(response.into_body())
        .await
        .unwrap()
        .to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

/// Every question answered with the same value.
fn all_answers(value: i16) -> serde_json::Value {
    let responses: Vec<serde_json::Value> = QUESTIONS
        .iter()
        .map(|q| serde_json::json!({ "question_id": q.id, "value": value }))
        .collect();
    serde_json::json!({ "responses": responses })
}

#[sqlx::test]
async fn questions_are_listed(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let response = router(state)
        .oneshot(get("/questions", &cookie))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = json_body(response).await;
    assert_eq!(body["questions"].as_array().unwrap().len(), 18);
    assert_eq!(body["scale"].as_array().unwrap().len(), 5);
    assert!(body["questions"][0]["text"].is_string());
    assert!(body["questions"][0]["axis"].is_string());
}

#[sqlx::test]
async fn the_reverse_flag_is_never_exposed(pool: PgPool) {
    // Knowing which items are flipped is enough to fake a coherent profile.
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let response = router(state)
        .oneshot(get("/questions", &cookie))
        .await
        .unwrap();
    let body = json_body(response).await;

    for question in body["questions"].as_array().unwrap() {
        assert!(
            question.get("reverse").is_none(),
            "reverse leaked: {question}"
        );
    }
}

#[sqlx::test]
async fn questions_require_a_session(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let app = router(state_with(pool, mailer));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/questions")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn responses_start_empty(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let response = router(state)
        .oneshot(get("/me/responses", &cookie))
        .await
        .unwrap();

    let body = json_body(response).await;
    assert_eq!(body["answered"], 0);
    assert_eq!(body["total"], 18);
    assert_eq!(body["complete"], false);
}

#[sqlx::test]
async fn a_partial_submission_is_accepted(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let response = router(state)
        .oneshot(put_json(
            "/me/responses",
            &cookie,
            serde_json::json!({ "responses": [{ "question_id": "risk_1", "value": 4 }] }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["answered"], 1);
    assert_eq!(body["complete"], false);
}

#[sqlx::test]
async fn answering_everything_completes_the_assessment(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let response = router(state)
        .oneshot(put_json("/me/responses", &cookie, all_answers(3)))
        .await
        .unwrap();

    let body = json_body(response).await;
    assert_eq!(body["answered"], 18);
    assert_eq!(body["complete"], true);

    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM trait_scores")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 1, "trait scores should be materialized once complete");
}

#[sqlx::test]
async fn trait_scores_are_not_written_while_incomplete(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    router(state)
        .oneshot(put_json(
            "/me/responses",
            &cookie,
            serde_json::json!({ "responses": [{ "question_id": "risk_1", "value": 4 }] }),
        ))
        .await
        .unwrap();

    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM trait_scores")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 0);
}

#[sqlx::test]
async fn changing_an_answer_recomputes_the_scores(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    router(state.clone())
        .oneshot(put_json("/me/responses", &cookie, all_answers(3)))
        .await
        .unwrap();

    let before: i16 = sqlx::query_scalar("SELECT risk_tolerance FROM trait_scores")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(before, 50);

    // risk_2 is the reverse item; 5 / 1 / 5 is maximal risk tolerance.
    router(state)
        .oneshot(put_json(
            "/me/responses",
            &cookie,
            serde_json::json!({ "responses": [
                { "question_id": "risk_1", "value": 5 },
                { "question_id": "risk_2", "value": 1 },
                { "question_id": "risk_3", "value": 5 }
            ]}),
        ))
        .await
        .unwrap();

    let after: i16 = sqlx::query_scalar("SELECT risk_tolerance FROM trait_scores")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(after, 100);
}

#[sqlx::test]
async fn an_unknown_question_is_rejected(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let response = router(state)
        .oneshot(put_json(
            "/me/responses",
            &cookie,
            serde_json::json!({ "responses": [{ "question_id": "made_up", "value": 3 }] }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = json_body(response).await;
    assert_eq!(body["errors"][0]["field"], "made_up");
}

#[sqlx::test]
async fn a_value_outside_one_to_five_is_rejected(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let response = router(state)
        .oneshot(put_json(
            "/me/responses",
            &cookie,
            serde_json::json!({ "responses": [{ "question_id": "risk_1", "value": 9 }] }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

    // Nothing in the batch is written when any part of it is invalid.
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM question_responses")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 0);
}

#[sqlx::test]
async fn one_users_answers_are_invisible_to_another(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());

    let ada = sign_in(state.clone(), &mailer, "ada@example.com").await;
    router(state.clone())
        .oneshot(put_json("/me/responses", &ada, all_answers(5)))
        .await
        .unwrap();

    let grace = sign_in(state.clone(), &mailer, "grace@example.com").await;
    let response = router(state)
        .oneshot(get("/me/responses", &grace))
        .await
        .unwrap();

    let body = json_body(response).await;
    assert_eq!(body["answered"], 0);
}

#[sqlx::test]
async fn submitting_responses_requires_a_session(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let app = router(state_with(pool, mailer));

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/me/responses")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "responses": [] }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
