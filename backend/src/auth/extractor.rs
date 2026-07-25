use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum_extra::extract::cookie::CookieJar;

use crate::app::AppState;
use crate::auth::routes::SESSION_COOKIE;
use crate::auth::{repo, tokens};
use crate::error::ApiError;
use crate::users::repo::User;

/// Extracting this on a handler makes the route require authentication.
pub struct CurrentUser(pub User);

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_headers(&parts.headers);

        let token = jar
            .get(SESSION_COOKIE)
            .map(|cookie| cookie.value().to_string())
            .ok_or(ApiError::Unauthorized)?;

        let user = repo::find_user_by_session(&state.db, &tokens::hash_token(&token))
            .await?
            .ok_or(ApiError::Unauthorized)?;

        Ok(CurrentUser(user))
    }
}
