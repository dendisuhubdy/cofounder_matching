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

/// A per-minute ceiling on messages from one person, in any conversation.
pub const MAX_MESSAGES_PER_MINUTE: i64 = 20;
pub const MAX_MESSAGE_LENGTH: usize = 4000;

/// Loads a conversation the caller is actually in. A conversation someone is
/// not part of reports as missing rather than forbidden: whether two other
/// people are talking is itself private.
async fn participating(
    state: &AppState,
    conversation_id: Uuid,
    user_id: Uuid,
) -> ApiResult<Conversation> {
    let conversation = repo::find_by_id(&state.db, conversation_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    if !conversation.includes(user_id) {
        return Err(ApiError::NotFound);
    }

    Ok(conversation)
}

pub async fn read_thread(
    state: &AppState,
    conversation_id: Uuid,
    reader_id: Uuid,
) -> ApiResult<Vec<repo::Message>> {
    let conversation = participating(state, conversation_id, reader_id).await?;

    repo::mark_read(&state.db, conversation.id, reader_id).await?;

    Ok(repo::messages_in(&state.db, conversation.id).await?)
}

pub async fn send_message(
    state: &AppState,
    conversation_id: Uuid,
    sender_id: Uuid,
    body: &str,
) -> ApiResult<repo::Message> {
    let conversation = participating(state, conversation_id, sender_id).await?;
    let recipient_id = conversation.other_than(sender_id);

    // Re-checked on every send, not just when the thread was opened: a block
    // raised mid-conversation has to take effect immediately.
    ensure_can_message(state, sender_id, recipient_id).await?;

    let trimmed = body.trim();

    if trimmed.is_empty() {
        return Err(ApiError::Validation(vec![FieldError {
            field: "body".into(),
            message: "cannot be empty".into(),
        }]));
    }

    if trimmed.chars().count() > MAX_MESSAGE_LENGTH {
        return Err(ApiError::Validation(vec![FieldError {
            field: "body".into(),
            message: format!("must be {MAX_MESSAGE_LENGTH} characters or fewer"),
        }]));
    }

    let recent = repo::count_messages_since(&state.db, sender_id, 1).await?;
    if recent >= MAX_MESSAGES_PER_MINUTE {
        return Err(ApiError::RateLimited {
            retry_after_seconds: 60,
        });
    }

    let message = repo::send(&state.db, conversation.id, sender_id, trimmed).await?;

    // After the write, so a client is never told about a message that failed
    // to commit.
    state.events.publish(
        recipient_id,
        crate::messaging::events::Event::NewMessage {
            conversation_id: conversation.id,
            sender_id,
            preview: trimmed.chars().take(120).collect(),
        },
    );

    let unread = repo::count_unread(&state.db, recipient_id).await?;
    state
        .events
        .publish(recipient_id, crate::messaging::events::Event::UnreadCount {
            count: unread,
        });

    Ok(message)
}
