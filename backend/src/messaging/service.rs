use uuid::Uuid;

use crate::app::AppState;
use crate::error::{ApiError, ApiResult, FieldError};
use crate::messaging::repo::{self, Conversation, ConversationSummary};
use crate::profiles;
use crate::users;

/// Ten new conversations per rolling twenty-four hours. Replies within an
/// existing conversation are unlimited: the limit exists to stop bulk
/// outreach, not to ration talking to someone who answered.
pub const MAX_NEW_CONVERSATIONS_PER_DAY: i64 = 10;
pub const NEW_CONVERSATION_WINDOW_MINUTES: i64 = 24 * 60;

/// Every precondition for one person messaging another, in one place. Gating
/// chat on a mutual match later — the design's most likely revision — is a
/// change here and nowhere else.
pub async fn ensure_can_message(
    state: &AppState,
    sender_id: Uuid,
    other_id: Uuid,
) -> ApiResult<()> {
    if sender_id == other_id {
        return Err(ApiError::Validation(vec![FieldError {
            field: "user_id".into(),
            message: "you cannot message yourself".into(),
        }]));
    }

    if users::repo::find_by_id(&state.db, other_id).await?.is_none() {
        return Err(ApiError::NotFound);
    }

    if !profiles::repo::is_complete(&state.db, sender_id).await? {
        return Err(ApiError::ProfileIncomplete);
    }

    // Someone who cannot appear in a deck cannot be written to either.
    if !profiles::repo::is_complete(&state.db, other_id).await? {
        return Err(ApiError::Forbidden);
    }

    if crate::moderation::repo::is_blocked_either_way(&state.db, sender_id, other_id).await? {
        return Err(ApiError::Forbidden);
    }

    Ok(())
}

pub async fn open_conversation(
    state: &AppState,
    initiator: Uuid,
    other_id: Uuid,
) -> ApiResult<(Conversation, bool)> {
    ensure_can_message(state, initiator, other_id).await?;

    // An existing thread is never a new conversation, so reopening one must
    // not cost anything against the daily allowance.
    if let Some(conversation) = repo::find_between(&state.db, initiator, other_id).await? {
        return Ok((conversation, false));
    }

    let started =
        repo::count_started_since(&state.db, initiator, NEW_CONVERSATION_WINDOW_MINUTES).await?;

    if started >= MAX_NEW_CONVERSATIONS_PER_DAY {
        return Err(ApiError::RateLimited {
            retry_after_seconds: (NEW_CONVERSATION_WINDOW_MINUTES * 60) as u64,
        });
    }

    Ok(repo::open(&state.db, initiator, other_id).await?)
}

pub async fn list(state: &AppState, user_id: Uuid) -> ApiResult<Vec<ConversationSummary>> {
    Ok(repo::for_user(&state.db, user_id).await?)
}
