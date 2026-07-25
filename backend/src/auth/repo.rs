use sqlx::PgPool;
use uuid::Uuid;

use crate::users::repo::User;

pub async fn issue_magic_link(
    pool: &PgPool,
    user_id: Uuid,
    token_hash: &[u8],
    ttl_minutes: i64,
) -> sqlx::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO magic_link_tokens (user_id, token_hash, expires_at)
        VALUES ($1, $2, now() + make_interval(mins => $3))
        "#,
    )
    .bind(user_id)
    .bind(token_hash)
    .bind(ttl_minutes as i32)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn count_recent_magic_links(
    pool: &PgPool,
    user_id: Uuid,
    within_minutes: i64,
) -> sqlx::Result<i64> {
    sqlx::query_scalar(
        r#"
        SELECT count(*)
        FROM magic_link_tokens
        WHERE user_id = $1
          AND created_at > now() - make_interval(mins => $2)
        "#,
    )
    .bind(user_id)
    .bind(within_minutes as i32)
    .fetch_one(pool)
    .await
}

/// Marks the token consumed and returns its user, in a single statement so
/// two concurrent verifications cannot both succeed. Returns `None` when the
/// token is unknown, already consumed, or expired — the caller must not be
/// able to distinguish these cases.
pub async fn consume_magic_link(pool: &PgPool, token_hash: &[u8]) -> sqlx::Result<Option<Uuid>> {
    sqlx::query_scalar(
        r#"
        UPDATE magic_link_tokens
        SET consumed_at = now()
        WHERE token_hash = $1
          AND consumed_at IS NULL
          AND expires_at > now()
        RETURNING user_id
        "#,
    )
    .bind(token_hash)
    .fetch_optional(pool)
    .await
}

pub async fn create_session(
    pool: &PgPool,
    user_id: Uuid,
    token_hash: &[u8],
    ttl_days: i64,
) -> sqlx::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO sessions (token_hash, user_id, expires_at)
        VALUES ($1, $2, now() + make_interval(days => $3))
        "#,
    )
    .bind(token_hash)
    .bind(user_id)
    .bind(ttl_days as i32)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn find_user_by_session(pool: &PgPool, token_hash: &[u8]) -> sqlx::Result<Option<User>> {
    sqlx::query_as::<_, User>(
        r#"
        SELECT u.id, u.email, u.status, u.created_at, u.last_active_at
        FROM sessions s
        JOIN users u ON u.id = s.user_id
        WHERE s.token_hash = $1
          AND s.expires_at > now()
          AND u.status = 'active'
        "#,
    )
    .bind(token_hash)
    .fetch_optional(pool)
    .await
}

pub async fn delete_session(pool: &PgPool, token_hash: &[u8]) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM sessions WHERE token_hash = $1")
        .bind(token_hash)
        .execute(pool)
        .await?;

    Ok(())
}
