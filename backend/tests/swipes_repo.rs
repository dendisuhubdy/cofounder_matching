use cofounder_api::swipes::repo::{self, Direction};
use cofounder_api::users;
use sqlx::PgPool;
use uuid::Uuid;

async fn a_user(pool: &PgPool, email: &str) -> Uuid {
    users::repo::find_or_create_by_email(pool, email)
        .await
        .unwrap()
        .id
}

/// The deck requires a complete profile, and `matches_for` reads the display
/// name, so matched users need a profile row.
async fn with_profile(pool: &PgPool, email: &str, name: &str) -> Uuid {
    let id = a_user(pool, email).await;

    sqlx::query(
        "INSERT INTO profiles (user_id, display_name, headline, bio, roles, seeking_roles, commitment)
         VALUES ($1, $2, 'Building things', 'A bio.', ARRAY['engineering'], ARRAY['gtm'], 'full_time_now')",
    )
    .bind(id)
    .bind(name)
    .execute(pool)
    .await
    .unwrap();

    id
}

#[sqlx::test]
async fn a_left_swipe_is_recorded_without_matching(pool: PgPool) {
    let ada = a_user(&pool, "ada@example.com").await;
    let grace = a_user(&pool, "grace@example.com").await;

    let outcome = repo::record_swipe(&pool, ada, grace, Direction::Left)
        .await
        .unwrap()
        .expect("a fresh swipe");

    assert!(!outcome.matched);
}

#[sqlx::test]
async fn a_one_sided_right_swipe_does_not_match(pool: PgPool) {
    let ada = a_user(&pool, "ada@example.com").await;
    let grace = a_user(&pool, "grace@example.com").await;

    let outcome = repo::record_swipe(&pool, ada, grace, Direction::Right)
        .await
        .unwrap()
        .expect("a fresh swipe");

    assert!(!outcome.matched);

    let matches: i64 = sqlx::query_scalar("SELECT count(*) FROM matches")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(matches, 0);
}

#[sqlx::test]
async fn a_mutual_right_swipe_creates_exactly_one_match(pool: PgPool) {
    let ada = with_profile(&pool, "ada@example.com", "Ada").await;
    let grace = with_profile(&pool, "grace@example.com", "Grace").await;

    repo::record_swipe(&pool, ada, grace, Direction::Right)
        .await
        .unwrap();
    let second = repo::record_swipe(&pool, grace, ada, Direction::Right)
        .await
        .unwrap()
        .expect("a fresh swipe");

    assert!(second.matched);

    let matches: i64 = sqlx::query_scalar("SELECT count(*) FROM matches")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(matches, 1, "one row, not one per direction");
}

#[sqlx::test]
async fn a_right_swipe_onto_a_left_swipe_does_not_match(pool: PgPool) {
    let ada = a_user(&pool, "ada@example.com").await;
    let grace = a_user(&pool, "grace@example.com").await;

    repo::record_swipe(&pool, ada, grace, Direction::Left)
        .await
        .unwrap();
    let outcome = repo::record_swipe(&pool, grace, ada, Direction::Right)
        .await
        .unwrap()
        .expect("a fresh swipe");

    assert!(!outcome.matched);
}

#[sqlx::test]
async fn swiping_the_same_person_twice_is_refused(pool: PgPool) {
    let ada = a_user(&pool, "ada@example.com").await;
    let grace = a_user(&pool, "grace@example.com").await;

    repo::record_swipe(&pool, ada, grace, Direction::Right)
        .await
        .unwrap()
        .expect("a fresh swipe");

    let repeat = repo::record_swipe(&pool, ada, grace, Direction::Left)
        .await
        .unwrap();

    assert!(repeat.is_none(), "a swipe is permanent");

    let direction: String =
        sqlx::query_scalar("SELECT direction FROM swipes WHERE swiper_id = $1")
            .bind(ada)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(direction, "right", "the original swipe stands");
}

#[sqlx::test]
async fn the_match_pair_is_stored_in_a_fixed_order(pool: PgPool) {
    let ada = with_profile(&pool, "ada@example.com", "Ada").await;
    let grace = with_profile(&pool, "grace@example.com", "Grace").await;

    repo::record_swipe(&pool, ada, grace, Direction::Right)
        .await
        .unwrap();
    repo::record_swipe(&pool, grace, ada, Direction::Right)
        .await
        .unwrap();

    let (a, b): (Uuid, Uuid) = sqlx::query_as("SELECT user_a_id, user_b_id FROM matches")
        .fetch_one(&pool)
        .await
        .unwrap();

    assert!(a < b);
}

#[sqlx::test]
async fn both_sides_see_the_match(pool: PgPool) {
    let ada = with_profile(&pool, "ada@example.com", "Ada").await;
    let grace = with_profile(&pool, "grace@example.com", "Grace").await;

    repo::record_swipe(&pool, ada, grace, Direction::Right)
        .await
        .unwrap();
    repo::record_swipe(&pool, grace, ada, Direction::Right)
        .await
        .unwrap();

    let for_ada = repo::matches_for(&pool, ada).await.unwrap();
    let for_grace = repo::matches_for(&pool, grace).await.unwrap();

    assert_eq!(for_ada.len(), 1);
    assert_eq!(for_ada[0].display_name, "Grace");
    assert_eq!(for_grace.len(), 1);
    assert_eq!(for_grace[0].display_name, "Ada");
}

#[sqlx::test]
async fn someone_with_no_matches_sees_none(pool: PgPool) {
    let ada = with_profile(&pool, "ada@example.com", "Ada").await;

    assert!(repo::matches_for(&pool, ada).await.unwrap().is_empty());
}

#[sqlx::test]
async fn recent_left_swipes_are_returned_newest_first_and_capped(pool: PgPool) {
    let ada = a_user(&pool, "ada@example.com").await;

    let mut passed = Vec::new();
    for index in 0..5 {
        let target = a_user(&pool, &format!("passed{index}@example.com")).await;
        repo::record_swipe(&pool, ada, target, Direction::Left)
            .await
            .unwrap();
        passed.push(target);
    }

    let liked = a_user(&pool, "liked@example.com").await;
    repo::record_swipe(&pool, ada, liked, Direction::Right)
        .await
        .unwrap();

    let recent = repo::recent_left_swipe_targets(&pool, ada, 3).await.unwrap();

    assert_eq!(recent.len(), 3, "the limit is respected");
    assert!(!recent.contains(&liked), "right swipes are not passes");
    for target in &recent {
        assert!(passed.contains(target));
    }
}
