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
