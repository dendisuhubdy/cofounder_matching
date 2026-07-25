use crate::app::AppState;
use crate::auth::{repo, tokens};
use crate::error::{ApiError, ApiResult, FieldError};
use crate::users;
use crate::users::repo::User;

pub const MAGIC_LINK_TTL_MINUTES: i64 = 15;
pub const SESSION_TTL_DAYS: i64 = 30;
pub const MAX_LINKS_PER_HOUR: i64 = 5;

/// Deliberately minimal: a full RFC 5322 parser would reject valid addresses
/// and accept unusable ones. Deliverability is proven by the link itself.
fn is_plausible_email(email: &str) -> bool {
    let trimmed = email.trim();
    match trimmed.split_once('@') {
        Some((local, domain)) => {
            !local.is_empty()
                && domain.contains('.')
                && !domain.starts_with('.')
                && !domain.ends_with('.')
                && !trimmed.contains(char::is_whitespace)
        }
        None => false,
    }
}

pub async fn request_login_link(state: &AppState, email: &str) -> ApiResult<()> {
    if !is_plausible_email(email) {
        return Err(ApiError::Validation(vec![FieldError {
            field: "email".into(),
            message: "must be a valid email address".into(),
        }]));
    }

    let user = users::repo::find_or_create_by_email(&state.db, email).await?;

    // Without this, the endpoint is an open relay for mailing anyone who has
    // an account here, or can be made to have one.
    let recent = repo::count_recent_magic_links(&state.db, user.id, 60).await?;
    if recent >= MAX_LINKS_PER_HOUR {
        return Err(ApiError::RateLimited {
            retry_after_seconds: 3600,
        });
    }

    let token = tokens::generate_token();
    let hash = tokens::hash_token(&token);
    repo::issue_magic_link(&state.db, user.id, &hash, MAGIC_LINK_TTL_MINUTES).await?;

    let link = format!("{}/verify?token={}", state.base_url, token);
    state
        .mailer
        .send_login_link(&user.email, &link)
        .await
        .map_err(ApiError::Internal)?;

    Ok(())
}

pub async fn verify_login_token(state: &AppState, token: &str) -> ApiResult<(User, String)> {
    let token_hash = tokens::hash_token(token);

    let user_id = repo::consume_magic_link(&state.db, &token_hash)
        .await?
        .ok_or(ApiError::InvalidToken)?;

    let user = users::repo::find_by_id(&state.db, user_id)
        .await?
        .ok_or(ApiError::InvalidToken)?;

    if user.status != "active" {
        return Err(ApiError::Forbidden);
    }

    let session_token = tokens::generate_token();
    let session_hash = tokens::hash_token(&session_token);
    repo::create_session(&state.db, user.id, &session_hash, SESSION_TTL_DAYS).await?;

    Ok((user, session_token))
}
