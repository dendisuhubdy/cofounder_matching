use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow, serde::Serialize)]
pub struct ProfileRow {
    pub display_name: String,
    pub headline: String,
    pub bio: String,
    pub city: String,
    pub country: String,
    pub timezone: String,
    pub linkedin_url: Option<String>,
    pub github_url: Option<String>,
    pub website_url: Option<String>,
    pub roles: Vec<String>,
    pub seeking_roles: Vec<String>,
    pub idea_status: Option<String>,
    pub stage: Option<String>,
    pub commitment: Option<String>,
}

/// The write shape. Every field is replaced on save: the profile is edited as
/// one document, so a partial update would silently discard whatever the form
/// did not send.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ProfileInput {
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub headline: String,
    #[serde(default)]
    pub bio: String,
    #[serde(default)]
    pub city: String,
    #[serde(default)]
    pub country: String,
    #[serde(default)]
    pub timezone: String,
    #[serde(default)]
    pub linkedin_url: Option<String>,
    #[serde(default)]
    pub github_url: Option<String>,
    #[serde(default)]
    pub website_url: Option<String>,
    #[serde(default)]
    pub roles: Vec<String>,
    #[serde(default)]
    pub seeking_roles: Vec<String>,
    #[serde(default)]
    pub idea_status: Option<String>,
    #[serde(default)]
    pub stage: Option<String>,
    #[serde(default)]
    pub commitment: Option<String>,
    #[serde(default)]
    pub interests: Vec<String>,
}

const COLUMNS: &str = "display_name, headline, bio, city, country, timezone, \
     linkedin_url, github_url, website_url, roles, seeking_roles, \
     idea_status, stage, commitment";

pub async fn find_by_user_id(pool: &PgPool, user_id: Uuid) -> sqlx::Result<Option<ProfileRow>> {
    sqlx::query_as::<_, ProfileRow>(&format!(
        "SELECT {COLUMNS} FROM profiles WHERE user_id = $1"
    ))
    .bind(user_id)
    .fetch_optional(pool)
    .await
}

pub async fn interests_for(pool: &PgPool, user_id: Uuid) -> sqlx::Result<Vec<String>> {
    sqlx::query_scalar("SELECT tag FROM profile_interests WHERE user_id = $1 ORDER BY tag")
        .bind(user_id)
        .fetch_all(pool)
        .await
}

/// Writes the profile and its interests together. A transaction because a
/// half-applied save would leave the interests of the previous version
/// attached to the new one.
pub async fn save(
    pool: &PgPool,
    user_id: Uuid,
    input: &ProfileInput,
) -> sqlx::Result<(ProfileRow, Vec<String>)> {
    let mut tx = pool.begin().await?;

    let row = sqlx::query_as::<_, ProfileRow>(&format!(
        r#"
        INSERT INTO profiles (
            user_id, display_name, headline, bio, city, country, timezone,
            linkedin_url, github_url, website_url, roles, seeking_roles,
            idea_status, stage, commitment, updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, now())
        ON CONFLICT (user_id) DO UPDATE SET
            display_name  = EXCLUDED.display_name,
            headline      = EXCLUDED.headline,
            bio           = EXCLUDED.bio,
            city          = EXCLUDED.city,
            country       = EXCLUDED.country,
            timezone      = EXCLUDED.timezone,
            linkedin_url  = EXCLUDED.linkedin_url,
            github_url    = EXCLUDED.github_url,
            website_url   = EXCLUDED.website_url,
            roles         = EXCLUDED.roles,
            seeking_roles = EXCLUDED.seeking_roles,
            idea_status   = EXCLUDED.idea_status,
            stage         = EXCLUDED.stage,
            commitment    = EXCLUDED.commitment,
            updated_at    = now()
        RETURNING {COLUMNS}
        "#
    ))
    .bind(user_id)
    .bind(&input.display_name)
    .bind(&input.headline)
    .bind(&input.bio)
    .bind(&input.city)
    .bind(&input.country)
    .bind(&input.timezone)
    .bind(&input.linkedin_url)
    .bind(&input.github_url)
    .bind(&input.website_url)
    .bind(&input.roles)
    .bind(&input.seeking_roles)
    .bind(&input.idea_status)
    .bind(&input.stage)
    .bind(&input.commitment)
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query("DELETE FROM profile_interests WHERE user_id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

    if !input.interests.is_empty() {
        sqlx::query(
            r#"
            INSERT INTO profile_interests (user_id, tag)
            SELECT $1, tag FROM UNNEST($2::TEXT[]) AS tag
            "#,
        )
        .bind(user_id)
        .bind(&input.interests)
        .execute(&mut *tx)
        .await?;
    }

    let interests: Vec<String> =
        sqlx::query_scalar("SELECT tag FROM profile_interests WHERE user_id = $1 ORDER BY tag")
            .bind(user_id)
            .fetch_all(&mut *tx)
            .await?;

    tx.commit().await?;

    Ok((row, interests))
}
