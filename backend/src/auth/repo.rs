use sqlx::PgPool;
use uuid::Uuid;

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
