use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};

use crate::app::AppState;
use crate::auth::service;
use crate::error::ApiResult;

pub const SESSION_COOKIE: &str = "session";

#[derive(serde::Deserialize)]
pub struct MagicLinkRequest {
    pub email: String,
}

#[derive(serde::Deserialize)]
pub struct VerifyRequest {
    pub token: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/magic-link", post(request_magic_link))
        .route("/auth/verify", post(verify))
}

pub fn session_cookie(token: String, secure: bool, max_age_days: i64) -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE, token))
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(secure)
        .path("/")
        .max_age(time::Duration::days(max_age_days))
        .build()
}

async fn request_magic_link(
    State(state): State<AppState>,
    Json(payload): Json<MagicLinkRequest>,
) -> ApiResult<StatusCode> {
    service::request_login_link(&state, &payload.email).await?;

    // 202 regardless of whether the address was already registered, so the
    // response cannot be used to enumerate accounts.
    Ok(StatusCode::ACCEPTED)
}

async fn verify(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(payload): Json<VerifyRequest>,
) -> ApiResult<(CookieJar, Json<crate::users::repo::User>)> {
    let (user, session_token) = service::verify_login_token(&state, &payload.token).await?;

    let jar = jar.add(session_cookie(
        session_token,
        state.secure_cookies,
        service::SESSION_TTL_DAYS,
    ));

    Ok((jar, Json(user)))
}
