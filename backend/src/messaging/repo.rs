use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow, serde::Serialize)]
pub struct Conversation {
    pub id: Uuid,
    pub user_a_id: Uuid,
    pub user_b_id: Uuid,
    pub started_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub last_message_at: Option<DateTime<Utc>>,
}

impl Conversation {
    pub fn includes(&self, user_id: Uuid) -> bool {
        self.user_a_id == user_id || self.user_b_id == user_id
    }

    /// The participant who is not `user_id`. Callers have already checked
    /// membership, so a conversation the user is not in returns their own id
    /// rather than panicking.
    pub fn other_than(&self, user_id: Uuid) -> Uuid {
        if self.user_a_id == user_id {
            self.user_b_id
        } else {
            self.user_a_id
        }
    }
}

/// A row of the conversation list: who it is with, what was said last, and
/// how much of it the caller has not read.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow, serde::Serialize)]
pub struct ConversationSummary {
    pub id: Uuid,
    pub other_user_id: Uuid,
    pub other_display_name: String,
    pub other_headline: String,
    pub last_message: Option<String>,
    pub last_message_at: Option<DateTime<Utc>>,
    pub unread: i64,
    pub matched: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow, serde::Serialize)]
pub struct Message {
    pub id: Uuid,
    pub conversation_id: Uuid,
    pub sender_id: Uuid,
    pub body: String,
    pub created_at: DateTime<Utc>,
    pub read_at: Option<DateTime<Utc>>,
}

const CONVERSATION_COLUMNS: &str =
    "id, user_a_id, user_b_id, started_by, created_at, last_message_at";

fn ordered(a: Uuid, b: Uuid) -> (Uuid, Uuid) {
    if a < b {
        (a, b)
    } else {
        (b, a)
    }
}

/// Finds or creates the conversation between two people. The second element
/// is true when this call created it, which is what the new-conversation
/// rate limit counts — reopening an existing thread is not a new one.
///
/// Written as look-then-insert rather than a single upsert returning
/// Postgres's `xmax` trick: that trick works, but it is an internal detail a
/// reader has to go and look up, and this runs on a path that is not hot.
pub async fn open(
    pool: &PgPool,
    initiator: Uuid,
    other: Uuid,
) -> sqlx::Result<(Conversation, bool)> {
    let (a, b) = ordered(initiator, other);

    if let Some(conversation) = find_between(pool, a, b).await? {
        return Ok((conversation, false));
    }

    // `started_by` is only ever set on insert, so whoever opened the
    // conversation first keeps it and reopening cannot move the limit's cost
    // onto the other person.
    let inserted = sqlx::query_as::<_, Conversation>(&format!(
        r#"
        INSERT INTO conversations (user_a_id, user_b_id, started_by)
        VALUES ($1, $2, $3)
        ON CONFLICT (user_a_id, user_b_id) DO NOTHING
        RETURNING {CONVERSATION_COLUMNS}
        "#
    ))
    .bind(a)
    .bind(b)
    .bind(initiator)
    .fetch_optional(pool)
    .await?;

    match inserted {
        Some(conversation) => Ok((conversation, true)),
        // A concurrent opener won the race. Their row is the conversation.
        None => {
            let conversation = find_between(pool, a, b)
                .await?
                .expect("the conflicting row exists");
            Ok((conversation, false))
        }
    }
}

/// The conversation between two people, whichever order they are given in.
pub async fn find_between(
    pool: &PgPool,
    one: Uuid,
    other: Uuid,
) -> sqlx::Result<Option<Conversation>> {
    let (a, b) = ordered(one, other);

    sqlx::query_as::<_, Conversation>(&format!(
        "SELECT {CONVERSATION_COLUMNS} FROM conversations
         WHERE user_a_id = $1 AND user_b_id = $2"
    ))
    .bind(a)
    .bind(b)
    .fetch_optional(pool)
    .await
}

pub async fn find_by_id(pool: &PgPool, id: Uuid) -> sqlx::Result<Option<Conversation>> {
    sqlx::query_as::<_, Conversation>(&format!(
        "SELECT {CONVERSATION_COLUMNS} FROM conversations WHERE id = $1"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await
}

/// Every conversation the user is in, excluding anyone blocked in either
/// direction. Matches sort to the top, then the most recently active.
pub async fn for_user(pool: &PgPool, user_id: Uuid) -> sqlx::Result<Vec<ConversationSummary>> {
    sqlx::query_as::<_, ConversationSummary>(
        r#"
        SELECT
            c.id                AS id,
            other.id            AS other_user_id,
            p.display_name      AS other_display_name,
            p.headline          AS other_headline,
            (
                SELECT m.body FROM messages m
                WHERE m.conversation_id = c.id
                ORDER BY m.created_at DESC LIMIT 1
            )                   AS last_message,
            c.last_message_at   AS last_message_at,
            (
                SELECT count(*) FROM messages m
                WHERE m.conversation_id = c.id
                  AND m.sender_id <> $1
                  AND m.read_at IS NULL
            )                   AS unread,
            EXISTS (
                SELECT 1 FROM matches mt
                WHERE (mt.user_a_id = c.user_a_id AND mt.user_b_id = c.user_b_id)
            )                   AS matched
        FROM conversations c
        JOIN users other
          ON other.id = CASE WHEN c.user_a_id = $1 THEN c.user_b_id ELSE c.user_a_id END
        JOIN profiles p ON p.user_id = other.id
        WHERE $1 IN (c.user_a_id, c.user_b_id)
          AND NOT EXISTS (
              SELECT 1 FROM blocks b
              WHERE (b.blocker_id = $1 AND b.blocked_id = other.id)
                 OR (b.blocker_id = other.id AND b.blocked_id = $1)
          )
        ORDER BY matched DESC, c.last_message_at DESC NULLS LAST, c.created_at DESC
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}

pub async fn messages_in(pool: &PgPool, conversation_id: Uuid) -> sqlx::Result<Vec<Message>> {
    sqlx::query_as::<_, Message>(
        r#"
        SELECT id, conversation_id, sender_id, body, created_at, read_at
        FROM messages
        WHERE conversation_id = $1
        ORDER BY created_at
        "#,
    )
    .bind(conversation_id)
    .fetch_all(pool)
    .await
}

/// Writes the message and stamps the conversation in one transaction, so the
/// list can never show a thread whose last activity predates its own last
/// message.
pub async fn send(
    pool: &PgPool,
    conversation_id: Uuid,
    sender_id: Uuid,
    body: &str,
) -> sqlx::Result<Message> {
    let mut tx = pool.begin().await?;

    let message = sqlx::query_as::<_, Message>(
        r#"
        INSERT INTO messages (conversation_id, sender_id, body)
        VALUES ($1, $2, $3)
        RETURNING id, conversation_id, sender_id, body, created_at, read_at
        "#,
    )
    .bind(conversation_id)
    .bind(sender_id)
    .bind(body)
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query("UPDATE conversations SET last_message_at = $2 WHERE id = $1")
        .bind(conversation_id)
        .bind(message.created_at)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    Ok(message)
}

/// Marks the *other* participant's messages as read. Returns how many were
/// cleared, which is what the caller reports as the new unread delta.
pub async fn mark_read(
    pool: &PgPool,
    conversation_id: Uuid,
    reader_id: Uuid,
) -> sqlx::Result<u64> {
    let result = sqlx::query(
        r#"
        UPDATE messages SET read_at = now()
        WHERE conversation_id = $1 AND sender_id <> $2 AND read_at IS NULL
        "#,
    )
    .bind(conversation_id)
    .bind(reader_id)
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
}

pub async fn count_started_since(
    pool: &PgPool,
    user_id: Uuid,
    minutes: i64,
) -> sqlx::Result<i64> {
    sqlx::query_scalar(
        r#"
        SELECT count(*) FROM conversations
        WHERE started_by = $1 AND created_at > now() - make_interval(mins => $2)
        "#,
    )
    .bind(user_id)
    .bind(minutes as i32)
    .fetch_one(pool)
    .await
}

pub async fn count_messages_since(
    pool: &PgPool,
    user_id: Uuid,
    minutes: i64,
) -> sqlx::Result<i64> {
    sqlx::query_scalar(
        r#"
        SELECT count(*) FROM messages
        WHERE sender_id = $1 AND created_at > now() - make_interval(mins => $2)
        "#,
    )
    .bind(user_id)
    .bind(minutes as i32)
    .fetch_one(pool)
    .await
}
