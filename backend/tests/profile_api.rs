use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use cofounder_api::app::{router, AppState};
use cofounder_api::assessment::questions::QUESTIONS;
use cofounder_api::email::console::RecordingMailer;
use sqlx::PgPool;
use tower::ServiceExt;

mod common;
use common::*;

fn a_complete_profile() -> serde_json::Value {
    serde_json::json!({
        "display_name": "Ada Lovelace",
        "headline": "Building tools for analytical engines",
        "bio": "Twenty years in numerical computing.",
        "city": "London",
        "country": "United Kingdom",
        "timezone": "Europe/London",
        "linkedin_url": "https://linkedin.com/in/ada",
        "github_url": null,
        "website_url": null,
        "roles": ["engineering", "research"],
        "seeking_roles": ["gtm"],
        "idea_status": "committed_idea",
        "stage": "prototype",
        "commitment": "full_time_now",
        "interests": ["developer_tools", "ai_ml"]
    })
}

async fn answer_everything(state: AppState, cookie: &str) {
    let responses: Vec<serde_json::Value> = QUESTIONS
        .iter()
        .map(|q| serde_json::json!({ "question_id": q.id, "value": 3 }))
        .collect();

    router(state)
        .oneshot(put_json(
            "/me/responses",
            cookie,
            serde_json::json!({ "responses": responses }),
        ))
        .await
        .unwrap();
}

#[sqlx::test]
async fn options_lists_every_vocabulary(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let response = router(state)
        .oneshot(get("/options", &cookie))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = json_body(response).await;
    assert_eq!(body["roles"].as_array().unwrap().len(), 6);
    assert_eq!(body["idea_statuses"].as_array().unwrap().len(), 3);
    assert_eq!(body["stages"].as_array().unwrap().len(), 4);
    assert_eq!(body["commitments"].as_array().unwrap().len(), 4);
    assert!(!body["interests"].as_array().unwrap().is_empty());
    assert!(body["roles"][0]["id"].is_string());
    assert!(body["roles"][0]["label"].is_string());
}

#[sqlx::test]
async fn a_new_user_gets_an_empty_profile_rather_than_a_404(pool: PgPool) {
    // The form needs something to render. 404 would make an ordinary first
    // visit look like an error.
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let response = router(state)
        .oneshot(get("/me/profile", &cookie))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["profile"]["bio"], "");
    assert_eq!(body["profile"]["roles"].as_array().unwrap().len(), 0);
    assert_eq!(body["complete"], false);
}

#[sqlx::test]
async fn a_profile_is_saved_and_read_back(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let saved = router(state.clone())
        .oneshot(put_json("/me/profile", &cookie, a_complete_profile()))
        .await
        .unwrap();
    assert_eq!(saved.status(), StatusCode::OK);

    let response = router(state)
        .oneshot(get("/me/profile", &cookie))
        .await
        .unwrap();
    let body = json_body(response).await;

    assert_eq!(body["profile"]["display_name"], "Ada Lovelace");
    assert_eq!(body["profile"]["commitment"], "full_time_now");
    assert_eq!(body["profile"]["interests"].as_array().unwrap().len(), 2);
}

#[sqlx::test]
async fn a_profile_is_incomplete_until_the_assessment_is_answered(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let response = router(state)
        .oneshot(put_json("/me/profile", &cookie, a_complete_profile()))
        .await
        .unwrap();

    let body = json_body(response).await;
    assert_eq!(body["complete"], false);
    assert_eq!(body["missing"], serde_json::json!(["responses"]));
}

#[sqlx::test]
async fn a_profile_becomes_complete_once_everything_is_filled(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    router(state.clone())
        .oneshot(put_json("/me/profile", &cookie, a_complete_profile()))
        .await
        .unwrap();
    answer_everything(state.clone(), &cookie).await;

    let response = router(state)
        .oneshot(get("/me/profile", &cookie))
        .await
        .unwrap();
    let body = json_body(response).await;

    assert_eq!(body["complete"], true);
    assert_eq!(body["missing"].as_array().unwrap().len(), 0);
}

#[sqlx::test]
async fn every_missing_requirement_is_named(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let response = router(state)
        .oneshot(put_json(
            "/me/profile",
            &cookie,
            serde_json::json!({ "display_name": "Ada" }),
        ))
        .await
        .unwrap();

    let body = json_body(response).await;
    assert_eq!(
        body["missing"],
        serde_json::json!(["bio", "roles", "seeking_roles", "commitment", "responses"])
    );
}

#[sqlx::test]
async fn an_unknown_role_is_rejected(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let mut payload = a_complete_profile();
    payload["roles"] = serde_json::json!(["astrology"]);

    let response = router(state)
        .oneshot(put_json("/me/profile", &cookie, payload))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = json_body(response).await;
    assert_eq!(body["errors"][0]["field"], "roles");
}

#[sqlx::test]
async fn an_unknown_interest_tag_is_rejected(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let mut payload = a_complete_profile();
    payload["interests"] = serde_json::json!(["underwater_basket_weaving"]);

    let response = router(state)
        .oneshot(put_json("/me/profile", &cookie, payload))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = json_body(response).await;
    assert_eq!(body["errors"][0]["field"], "interests");
}

#[sqlx::test]
async fn an_unknown_commitment_is_rejected(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let mut payload = a_complete_profile();
    payload["commitment"] = serde_json::json!("whenever");

    let response = router(state)
        .oneshot(put_json("/me/profile", &cookie, payload))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = json_body(response).await;
    assert_eq!(body["errors"][0]["field"], "commitment");
}

#[sqlx::test]
async fn an_overlong_bio_is_rejected(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let mut payload = a_complete_profile();
    payload["bio"] = serde_json::json!("x".repeat(2001));

    let response = router(state)
        .oneshot(put_json("/me/profile", &cookie, payload))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = json_body(response).await;
    assert_eq!(body["errors"][0]["field"], "bio");
}

#[sqlx::test]
async fn a_link_that_is_not_a_url_is_rejected(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let mut payload = a_complete_profile();
    payload["website_url"] = serde_json::json!("javascript:alert(1)");

    let response = router(state)
        .oneshot(put_json("/me/profile", &cookie, payload))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = json_body(response).await;
    assert_eq!(body["errors"][0]["field"], "website_url");
}

#[sqlx::test]
async fn an_empty_link_is_stored_as_null_rather_than_rejected(pool: PgPool) {
    // The form submits "" for a link the user left blank.
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let mut payload = a_complete_profile();
    payload["website_url"] = serde_json::json!("");

    let response = router(state)
        .oneshot(put_json("/me/profile", &cookie, payload))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert!(body["profile"]["website_url"].is_null());
}

#[sqlx::test]
async fn whitespace_around_text_is_trimmed(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let mut payload = a_complete_profile();
    payload["display_name"] = serde_json::json!("  Ada Lovelace  ");

    let response = router(state)
        .oneshot(put_json("/me/profile", &cookie, payload))
        .await
        .unwrap();

    let body = json_body(response).await;
    assert_eq!(body["profile"]["display_name"], "Ada Lovelace");
}

#[sqlx::test]
async fn a_bio_of_only_whitespace_does_not_count_as_filled_in(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let mut payload = a_complete_profile();
    payload["bio"] = serde_json::json!("   ");

    let response = router(state)
        .oneshot(put_json("/me/profile", &cookie, payload))
        .await
        .unwrap();

    let body = json_body(response).await;
    assert!(body["missing"]
        .as_array()
        .unwrap()
        .contains(&serde_json::json!("bio")));
}

#[sqlx::test]
async fn one_users_profile_is_invisible_to_another(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());

    let ada = sign_in(state.clone(), &mailer, "ada@example.com").await;
    router(state.clone())
        .oneshot(put_json("/me/profile", &ada, a_complete_profile()))
        .await
        .unwrap();

    let grace = sign_in(state.clone(), &mailer, "grace@example.com").await;
    let response = router(state)
        .oneshot(get("/me/profile", &grace))
        .await
        .unwrap();

    let body = json_body(response).await;
    assert_eq!(body["profile"]["display_name"], "");
}

#[sqlx::test]
async fn the_profile_endpoints_require_a_session(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer);

    let read = router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/me/profile")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(read.status(), StatusCode::UNAUTHORIZED);

    let write = router(state)
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/me/profile")
                .header("content-type", "application/json")
                .body(Body::from(a_complete_profile().to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(write.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn saving_a_profile_derives_the_timezone_offset(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let mut payload = a_complete_profile();
    payload["timezone"] = serde_json::json!("Asia/Jakarta");

    let response = router(state)
        .oneshot(put_json("/me/profile", &cookie, payload))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let offset: Option<i16> = sqlx::query_scalar("SELECT utc_offset_minutes FROM profiles")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(offset, Some(420));
}

#[sqlx::test]
async fn an_unknown_timezone_is_rejected(pool: PgPool) {
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool, mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let mut payload = a_complete_profile();
    payload["timezone"] = serde_json::json!("Mars/Olympus_Mons");

    let response = router(state)
        .oneshot(put_json("/me/profile", &cookie, payload))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = json_body(response).await;
    assert_eq!(body["errors"][0]["field"], "timezone");
}

#[sqlx::test]
async fn a_client_cannot_supply_its_own_offset(pool: PgPool) {
    // The offset is derived. Accepting it from the body would let a caller
    // claim to be in a timezone their named zone contradicts.
    let mailer = Arc::new(RecordingMailer::default());
    let state = state_with(pool.clone(), mailer.clone());
    let cookie = sign_in(state.clone(), &mailer, "ada@example.com").await;

    let mut payload = a_complete_profile();
    payload["timezone"] = serde_json::json!("Asia/Jakarta");
    payload["utc_offset_minutes"] = serde_json::json!(-600);

    router(state)
        .oneshot(put_json("/me/profile", &cookie, payload))
        .await
        .unwrap();

    let offset: Option<i16> = sqlx::query_scalar("SELECT utc_offset_minutes FROM profiles")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(offset, Some(420));
}
