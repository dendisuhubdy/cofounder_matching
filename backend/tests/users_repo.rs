use cofounder_api::users::repo;
use sqlx::PgPool;

#[sqlx::test]
async fn creates_user_on_first_lookup(pool: PgPool) {
    let user = repo::find_or_create_by_email(&pool, "ada@example.com")
        .await
        .unwrap();

    assert_eq!(user.email, "ada@example.com");
    assert_eq!(user.status, "active");
}

#[sqlx::test]
async fn returns_same_user_on_second_lookup(pool: PgPool) {
    let first = repo::find_or_create_by_email(&pool, "ada@example.com")
        .await
        .unwrap();
    let second = repo::find_or_create_by_email(&pool, "ada@example.com")
        .await
        .unwrap();

    assert_eq!(first.id, second.id);
}

#[sqlx::test]
async fn normalizes_case_and_whitespace(pool: PgPool) {
    let first = repo::find_or_create_by_email(&pool, "Ada@Example.com")
        .await
        .unwrap();
    let second = repo::find_or_create_by_email(&pool, "  ada@example.com  ")
        .await
        .unwrap();

    assert_eq!(first.id, second.id);
    assert_eq!(first.email, "ada@example.com");
}

#[sqlx::test]
async fn find_by_id_returns_none_for_unknown_id(pool: PgPool) {
    let result = repo::find_by_id(&pool, uuid::Uuid::new_v4()).await.unwrap();
    assert!(result.is_none());
}

#[sqlx::test]
async fn find_by_id_round_trips_a_created_user(pool: PgPool) {
    let created = repo::find_or_create_by_email(&pool, "ada@example.com")
        .await
        .unwrap();

    let found = repo::find_by_id(&pool, created.id).await.unwrap().unwrap();

    assert_eq!(found, created);
}
