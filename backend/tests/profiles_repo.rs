use cofounder_api::profiles::repo::{self, ProfileInput};
use cofounder_api::users;
use sqlx::PgPool;
use uuid::Uuid;

async fn a_user(pool: &PgPool, email: &str) -> Uuid {
    users::repo::find_or_create_by_email(pool, email)
        .await
        .unwrap()
        .id
}

fn an_input() -> ProfileInput {
    ProfileInput {
        display_name: "Ada Lovelace".into(),
        headline: "Building tools for analytical engines".into(),
        bio: "Twenty years in numerical computing.".into(),
        city: "London".into(),
        country: "United Kingdom".into(),
        timezone: "Europe/London".into(),
        utc_offset_minutes: Some(0),
        linkedin_url: Some("https://linkedin.com/in/ada".into()),
        github_url: None,
        website_url: None,
        roles: vec!["engineering".into(), "research".into()],
        seeking_roles: vec!["gtm".into()],
        idea_status: Some("committed_idea".into()),
        stage: Some("prototype".into()),
        commitment: Some("full_time_now".into()),
        interests: vec!["developer_tools".into(), "ai_ml".into()],
    }
}

#[sqlx::test]
async fn a_user_without_a_profile_has_none(pool: PgPool) {
    let user_id = a_user(&pool, "ada@example.com").await;

    assert!(repo::find_by_user_id(&pool, user_id)
        .await
        .unwrap()
        .is_none());
    assert!(repo::interests_for(&pool, user_id).await.unwrap().is_empty());
}

#[sqlx::test]
async fn saving_creates_the_profile(pool: PgPool) {
    let user_id = a_user(&pool, "ada@example.com").await;

    let (row, interests) = repo::save(&pool, user_id, &an_input()).await.unwrap();

    assert_eq!(row.display_name, "Ada Lovelace");
    assert_eq!(row.roles, vec!["engineering", "research"]);
    assert_eq!(row.seeking_roles, vec!["gtm"]);
    assert_eq!(row.commitment.as_deref(), Some("full_time_now"));
    assert_eq!(row.github_url, None);
    assert_eq!(interests, vec!["ai_ml", "developer_tools"]);
}

#[sqlx::test]
async fn saving_returns_what_reading_returns(pool: PgPool) {
    let user_id = a_user(&pool, "ada@example.com").await;
    let (saved, saved_interests) = repo::save(&pool, user_id, &an_input()).await.unwrap();

    let loaded = repo::find_by_user_id(&pool, user_id).await.unwrap().unwrap();
    let loaded_interests = repo::interests_for(&pool, user_id).await.unwrap();

    assert_eq!(loaded, saved);
    assert_eq!(loaded_interests, saved_interests);
}

#[sqlx::test]
async fn saving_twice_updates_rather_than_duplicating(pool: PgPool) {
    let user_id = a_user(&pool, "ada@example.com").await;
    repo::save(&pool, user_id, &an_input()).await.unwrap();

    let mut changed = an_input();
    changed.headline = "Now working on compilers".into();
    repo::save(&pool, user_id, &changed).await.unwrap();

    let row = repo::find_by_user_id(&pool, user_id).await.unwrap().unwrap();
    assert_eq!(row.headline, "Now working on compilers");

    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM profiles")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 1);
}

#[sqlx::test]
async fn interests_are_replaced_not_appended(pool: PgPool) {
    let user_id = a_user(&pool, "ada@example.com").await;
    repo::save(&pool, user_id, &an_input()).await.unwrap();

    let mut changed = an_input();
    changed.interests = vec!["climate".into()];
    let (_, interests) = repo::save(&pool, user_id, &changed).await.unwrap();

    assert_eq!(interests, vec!["climate"]);
}

#[sqlx::test]
async fn interests_can_be_cleared(pool: PgPool) {
    let user_id = a_user(&pool, "ada@example.com").await;
    repo::save(&pool, user_id, &an_input()).await.unwrap();

    let mut changed = an_input();
    changed.interests = vec![];
    let (_, interests) = repo::save(&pool, user_id, &changed).await.unwrap();

    assert!(interests.is_empty());
}

#[sqlx::test]
async fn profiles_are_scoped_to_one_user(pool: PgPool) {
    let ada = a_user(&pool, "ada@example.com").await;
    let grace = a_user(&pool, "grace@example.com").await;

    repo::save(&pool, ada, &an_input()).await.unwrap();

    assert!(repo::find_by_user_id(&pool, grace).await.unwrap().is_none());
}

#[sqlx::test]
async fn the_database_refuses_an_unknown_role(pool: PgPool) {
    // The service layer validates too; this proves the constraint is really
    // there, so a future code path cannot write a role the scorer cannot read.
    let user_id = a_user(&pool, "ada@example.com").await;

    let mut input = an_input();
    input.roles = vec!["astrology".into()];

    assert!(repo::save(&pool, user_id, &input).await.is_err());
}
