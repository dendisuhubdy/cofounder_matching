use cofounder_api::deck::repo;
use cofounder_api::swipes::repo as swipes;
use cofounder_api::swipes::repo::Direction;
use cofounder_api::users;
use sqlx::PgPool;
use uuid::Uuid;

/// A user who satisfies every deck filter: active, complete profile, and a
/// trait_scores row (which exists only when all eighteen answers do).
async fn complete_user(pool: &PgPool, email: &str, name: &str) -> Uuid {
    let id = users::repo::find_or_create_by_email(pool, email)
        .await
        .unwrap()
        .id;

    sqlx::query(
        "INSERT INTO profiles (user_id, display_name, headline, bio, city, country,
                               timezone, utc_offset_minutes, roles, seeking_roles,
                               idea_status, stage, commitment)
         VALUES ($1, $2, 'Building things', 'A real bio.', 'Jakarta', 'Indonesia',
                 'Asia/Jakarta', 420, ARRAY['engineering'], ARRAY['gtm'],
                 'committed_idea', 'prototype', 'full_time_now')",
    )
    .bind(id)
    .bind(name)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO trait_scores (user_id, risk_tolerance, pace_vs_rigor, conflict_style,
                                   decision_basis, work_mode, orientation)
         VALUES ($1, 50, 50, 50, 50, 50, 50)",
    )
    .bind(id)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query("INSERT INTO profile_interests (user_id, tag) VALUES ($1, 'ai_ml')")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();

    id
}

#[sqlx::test]
async fn a_complete_stranger_is_a_candidate(pool: PgPool) {
    let ada = complete_user(&pool, "ada@example.com", "Ada").await;
    complete_user(&pool, "grace@example.com", "Grace").await;

    let candidates = repo::candidates_for(&pool, ada).await.unwrap();

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].profile.display_name, "Grace");
    assert_eq!(candidates[0].bio, "A real bio.");
    assert_eq!(candidates[0].profile.roles, vec!["engineering"]);
    assert_eq!(candidates[0].profile.interests, vec!["ai_ml"]);
    assert_eq!(candidates[0].profile.utc_offset_minutes, Some(420));
    assert_eq!(candidates[0].profile.traits.risk_tolerance, 50);
}

#[sqlx::test]
async fn you_are_never_your_own_candidate(pool: PgPool) {
    let ada = complete_user(&pool, "ada@example.com", "Ada").await;

    assert!(repo::candidates_for(&pool, ada).await.unwrap().is_empty());
}

#[sqlx::test]
async fn someone_already_swiped_on_is_excluded(pool: PgPool) {
    let ada = complete_user(&pool, "ada@example.com", "Ada").await;
    let grace = complete_user(&pool, "grace@example.com", "Grace").await;

    swipes::record_swipe(&pool, ada, grace, Direction::Left)
        .await
        .unwrap();

    assert!(repo::candidates_for(&pool, ada).await.unwrap().is_empty());
}

#[sqlx::test]
async fn a_right_swipe_also_removes_them_from_the_deck(pool: PgPool) {
    let ada = complete_user(&pool, "ada@example.com", "Ada").await;
    let grace = complete_user(&pool, "grace@example.com", "Grace").await;

    swipes::record_swipe(&pool, ada, grace, Direction::Right)
        .await
        .unwrap();

    assert!(repo::candidates_for(&pool, ada).await.unwrap().is_empty());
}

#[sqlx::test]
async fn a_profile_without_trait_scores_is_incomplete(pool: PgPool) {
    let ada = complete_user(&pool, "ada@example.com", "Ada").await;
    let grace = complete_user(&pool, "grace@example.com", "Grace").await;

    sqlx::query("DELETE FROM trait_scores WHERE user_id = $1")
        .bind(grace)
        .execute(&pool)
        .await
        .unwrap();

    assert!(repo::candidates_for(&pool, ada).await.unwrap().is_empty());
}

#[sqlx::test]
async fn a_profile_without_a_bio_is_incomplete(pool: PgPool) {
    let ada = complete_user(&pool, "ada@example.com", "Ada").await;
    let grace = complete_user(&pool, "grace@example.com", "Grace").await;

    sqlx::query("UPDATE profiles SET bio = '' WHERE user_id = $1")
        .bind(grace)
        .execute(&pool)
        .await
        .unwrap();

    assert!(repo::candidates_for(&pool, ada).await.unwrap().is_empty());
}

#[sqlx::test]
async fn a_profile_with_no_roles_is_incomplete(pool: PgPool) {
    let ada = complete_user(&pool, "ada@example.com", "Ada").await;
    let grace = complete_user(&pool, "grace@example.com", "Grace").await;

    sqlx::query("UPDATE profiles SET roles = '{}' WHERE user_id = $1")
        .bind(grace)
        .execute(&pool)
        .await
        .unwrap();

    assert!(repo::candidates_for(&pool, ada).await.unwrap().is_empty());
}

#[sqlx::test]
async fn a_profile_with_no_commitment_is_incomplete(pool: PgPool) {
    let ada = complete_user(&pool, "ada@example.com", "Ada").await;
    let grace = complete_user(&pool, "grace@example.com", "Grace").await;

    sqlx::query("UPDATE profiles SET commitment = NULL WHERE user_id = $1")
        .bind(grace)
        .execute(&pool)
        .await
        .unwrap();

    assert!(repo::candidates_for(&pool, ada).await.unwrap().is_empty());
}

#[sqlx::test]
async fn a_suspended_account_is_excluded(pool: PgPool) {
    let ada = complete_user(&pool, "ada@example.com", "Ada").await;
    let grace = complete_user(&pool, "grace@example.com", "Grace").await;

    sqlx::query("UPDATE users SET status = 'suspended' WHERE id = $1")
        .bind(grace)
        .execute(&pool)
        .await
        .unwrap();

    assert!(repo::candidates_for(&pool, ada).await.unwrap().is_empty());
}

#[sqlx::test]
async fn someone_the_viewer_blocked_is_excluded(pool: PgPool) {
    let ada = complete_user(&pool, "ada@example.com", "Ada").await;
    let grace = complete_user(&pool, "grace@example.com", "Grace").await;

    sqlx::query("INSERT INTO blocks (blocker_id, blocked_id) VALUES ($1, $2)")
        .bind(ada)
        .bind(grace)
        .execute(&pool)
        .await
        .unwrap();

    assert!(repo::candidates_for(&pool, ada).await.unwrap().is_empty());
}

#[sqlx::test]
async fn someone_who_blocked_the_viewer_is_excluded(pool: PgPool) {
    // The block has to bite in both directions, or blocking someone merely
    // hides you from them while they stay visible to you.
    let ada = complete_user(&pool, "ada@example.com", "Ada").await;
    let grace = complete_user(&pool, "grace@example.com", "Grace").await;

    sqlx::query("INSERT INTO blocks (blocker_id, blocked_id) VALUES ($1, $2)")
        .bind(grace)
        .bind(ada)
        .execute(&pool)
        .await
        .unwrap();

    assert!(repo::candidates_for(&pool, ada).await.unwrap().is_empty());
}

#[sqlx::test]
async fn a_candidate_appears_once_however_many_interests_they_have(pool: PgPool) {
    // A naive join against profile_interests returns one row per tag.
    let ada = complete_user(&pool, "ada@example.com", "Ada").await;
    let grace = complete_user(&pool, "grace@example.com", "Grace").await;

    sqlx::query("INSERT INTO profile_interests (user_id, tag) VALUES ($1, 'saas'), ($1, 'fintech')")
        .bind(grace)
        .execute(&pool)
        .await
        .unwrap();

    let candidates = repo::candidates_for(&pool, ada).await.unwrap();

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].profile.interests.len(), 3);
}

#[sqlx::test]
async fn a_candidate_with_no_interests_still_appears(pool: PgPool) {
    let ada = complete_user(&pool, "ada@example.com", "Ada").await;
    let grace = complete_user(&pool, "grace@example.com", "Grace").await;

    sqlx::query("DELETE FROM profile_interests WHERE user_id = $1")
        .bind(grace)
        .execute(&pool)
        .await
        .unwrap();

    let candidates = repo::candidates_for(&pool, ada).await.unwrap();

    assert_eq!(candidates.len(), 1);
    assert!(candidates[0].profile.interests.is_empty());
}

#[sqlx::test]
async fn the_viewer_can_be_loaded_directly(pool: PgPool) {
    let ada = complete_user(&pool, "ada@example.com", "Ada").await;

    let viewer = repo::load_profile(&pool, ada).await.unwrap().expect("loaded");

    assert_eq!(viewer.profile.user_id, ada);
    assert_eq!(viewer.profile.display_name, "Ada");
}

#[sqlx::test]
async fn loading_an_incomplete_viewer_returns_nothing(pool: PgPool) {
    let ada = users::repo::find_or_create_by_email(&pool, "ada@example.com")
        .await
        .unwrap()
        .id;

    assert!(repo::load_profile(&pool, ada).await.unwrap().is_none());
}

#[sqlx::test]
async fn recent_pass_tags_gather_roles_and_interests(pool: PgPool) {
    let ada = complete_user(&pool, "ada@example.com", "Ada").await;
    let passed = complete_user(&pool, "passed@example.com", "Passed").await;

    swipes::record_swipe(&pool, ada, passed, Direction::Left)
        .await
        .unwrap();

    let tags = repo::recent_pass_tags(&pool, ada, 20).await.unwrap();

    assert!(tags.contains(&"engineering".to_string()));
    assert!(tags.contains(&"ai_ml".to_string()));
}

#[sqlx::test]
async fn right_swipes_do_not_become_pass_tags(pool: PgPool) {
    let ada = complete_user(&pool, "ada@example.com", "Ada").await;
    let liked = complete_user(&pool, "liked@example.com", "Liked").await;

    swipes::record_swipe(&pool, ada, liked, Direction::Right)
        .await
        .unwrap();

    assert!(repo::recent_pass_tags(&pool, ada, 20)
        .await
        .unwrap()
        .is_empty());
}

#[sqlx::test]
async fn right_swipe_rates_are_reported_per_target(pool: PgPool) {
    let popular = complete_user(&pool, "popular@example.com", "Popular").await;
    let ignored = complete_user(&pool, "ignored@example.com", "Ignored").await;

    for index in 0..3 {
        let admirer = complete_user(&pool, &format!("fan{index}@example.com"), "Fan").await;
        swipes::record_swipe(&pool, admirer, popular, Direction::Right)
            .await
            .unwrap();
        swipes::record_swipe(&pool, admirer, ignored, Direction::Left)
            .await
            .unwrap();
    }

    let rates = repo::right_swipe_rates(&pool, 30).await.unwrap();

    let popular_rate = rates.iter().find(|(id, _)| *id == popular).unwrap().1;
    let ignored_rate = rates.iter().find(|(id, _)| *id == ignored).unwrap().1;

    assert!((popular_rate - 1.0).abs() < 0.001, "got {popular_rate}");
    assert!((ignored_rate - 0.0).abs() < 0.001, "got {ignored_rate}");
}
