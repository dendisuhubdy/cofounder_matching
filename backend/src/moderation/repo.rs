use sqlx::PgPool;
use uuid::Uuid;

/// A block hides both people from each other. Checking one direction only
/// would let the blocker keep messaging the person they blocked.
pub async fn is_blocked_either_way(
    pool: &PgPool,
    one: Uuid,
    other: Uuid,
) -> sqlx::Result<bool> {
    sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1 FROM blocks
            WHERE (blocker_id = $1 AND blocked_id = $2)
               OR (blocker_id = $2 AND blocked_id = $1)
        )
        "#,
    )
    .bind(one)
    .bind(other)
    .fetch_one(pool)
    .await
}

/// Idempotent: pressing block twice is not a failure.
pub async fn block(pool: &PgPool, blocker_id: Uuid, blocked_id: Uuid) -> sqlx::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO blocks (blocker_id, blocked_id)
        VALUES ($1, $2)
        ON CONFLICT (blocker_id, blocked_id) DO NOTHING
        "#,
    )
    .bind(blocker_id)
    .bind(blocked_id)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn report(
    pool: &PgPool,
    reporter_id: Uuid,
    reported_id: Uuid,
    reason: &str,
    body: &str,
) -> sqlx::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO reports (reporter_id, reported_id, reason, body)
        VALUES ($1, $2, $3, $4)
        "#,
    )
    .bind(reporter_id)
    .bind(reported_id)
    .bind(reason)
    .bind(body)
    .execute(pool)
    .await?;

    Ok(())
}
