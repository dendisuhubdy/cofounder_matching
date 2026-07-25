use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow, serde::Serialize)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub last_active_at: DateTime<Utc>,
}

fn normalize(email: &str) -> String {
    email.trim().to_lowercase()
}

pub async fn find_or_create_by_email(pool: &PgPool, email: &str) -> sqlx::Result<User> {
    // ON CONFLICT DO UPDATE (rather than DO NOTHING) so RETURNING yields a row
    // whether the user already existed or not.
    sqlx::query_as::<_, User>(
        r#"
        INSERT INTO users (email)
        VALUES ($1)
        ON CONFLICT (email) DO UPDATE SET email = EXCLUDED.email
        RETURNING id, email, status, created_at, last_active_at
        "#,
    )
    .bind(normalize(email))
    .fetch_one(pool)
    .await
}

pub async fn find_by_id(pool: &PgPool, id: Uuid) -> sqlx::Result<Option<User>> {
    sqlx::query_as::<_, User>(
        r#"
        SELECT id, email, status, created_at, last_active_at
        FROM users
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}
