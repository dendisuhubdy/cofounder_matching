//! Shared helpers for the integration tests.
//!
//! Rust builds each file in `tests/` as its own crate, so this is included
//! with `mod common;` rather than imported. Every crate uses a different
//! subset of it, hence the allow.
#![allow(dead_code)]

use std::sync::Arc;

use axum::body::Body;
use axum::http::Request;
use cofounder_api::app::{router, AppState};
use cofounder_api::email::console::RecordingMailer;
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

pub fn state_with(pool: PgPool, mailer: Arc<RecordingMailer>) -> AppState {
    AppState {
        db: pool,
        mailer,
        base_url: "http://localhost:3000".into(),
        secure_cookies: false,
        test_mailer: None,
        events: cofounder_api::messaging::events::EventBus::new(),
    }
}

pub fn post_json(uri: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

pub fn put_json(uri: &str, cookie: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method("PUT")
        .uri(uri)
        .header("cookie", cookie)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

pub fn get(uri: &str, cookie: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .header("cookie", cookie)
        .body(Body::empty())
        .unwrap()
}

pub async fn json_body(response: axum::response::Response) -> serde_json::Value {
    let bytes = http_body_util::BodyExt::collect(response.into_body())
        .await
        .unwrap()
        .to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

pub async fn sign_in(state: AppState, mailer: &RecordingMailer, email: &str) -> String {
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

/// A user who satisfies every completeness rule: active, full profile, and
/// a `trait_scores` row — which exists only when all eighteen answers do.
/// Roles are parameters because scoring tests need pairs that complement
/// each other.
pub async fn complete_profile(
    pool: &PgPool,
    email: &str,
    name: &str,
    roles: &[&str],
    seeking: &[&str],
) -> Uuid {
    let id = cofounder_api::users::repo::find_or_create_by_email(pool, email)
        .await
        .unwrap()
        .id;

    sqlx::query(
        "INSERT INTO profiles (user_id, display_name, headline, bio, city, country,
                               timezone, utc_offset_minutes, roles, seeking_roles,
                               idea_status, stage, commitment)
         VALUES ($1, $2, 'Building things', 'A real bio.', 'Jakarta', 'Indonesia',
                 'Asia/Jakarta', 420, $3, $4, 'committed_idea', 'prototype', 'full_time_now')
         ON CONFLICT (user_id) DO UPDATE SET
             display_name  = EXCLUDED.display_name,
             roles         = EXCLUDED.roles,
             seeking_roles = EXCLUDED.seeking_roles",
    )
    .bind(id)
    .bind(name)
    .bind(roles.iter().map(|r| r.to_string()).collect::<Vec<String>>())
    .bind(
        seeking
            .iter()
            .map(|r| r.to_string())
            .collect::<Vec<String>>(),
    )
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
