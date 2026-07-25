use crate::app::AppState;
use crate::auth::{repo, tokens};
use crate::error::{ApiError, ApiResult, FieldError};
use crate::users;

pub const MAGIC_LINK_TTL_MINUTES: i64 = 15;

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
