use cofounder_api::assessment::repo;
use cofounder_api::assessment::scoring::TraitScores;
use cofounder_api::users;
use sqlx::PgPool;
use uuid::Uuid;

async fn a_user(pool: &PgPool, email: &str) -> Uuid {
    users::repo::find_or_create_by_email(pool, email)
        .await
        .unwrap()
        .id
}

fn response(question_id: &str, value: i16) -> repo::Response {
    repo::Response {
        question_id: question_id.to_string(),
        value,
    }
}

#[sqlx::test]
async fn responses_start_empty(pool: PgPool) {
    let user_id = a_user(&pool, "ada@example.com").await;

    let responses = repo::responses_for(&pool, user_id).await.unwrap();
    assert!(responses.is_empty());
    assert_eq!(repo::answered_count(&pool, user_id).await.unwrap(), 0);
}

#[sqlx::test]
async fn responses_are_saved_and_read_back(pool: PgPool) {
    let user_id = a_user(&pool, "ada@example.com").await;

    repo::upsert_responses(
        &pool,
        user_id,
        &[response("risk_1", 4), response("pace_1", 2)],
    )
    .await
    .unwrap();

    let answers = repo::answers_map(&pool, user_id).await.unwrap();
    assert_eq!(answers.get("risk_1"), Some(&4));
    assert_eq!(answers.get("pace_1"), Some(&2));
    assert_eq!(repo::answered_count(&pool, user_id).await.unwrap(), 2);
}

#[sqlx::test]
async fn answering_the_same_question_again_overwrites(pool: PgPool) {
    let user_id = a_user(&pool, "ada@example.com").await;

    repo::upsert_responses(&pool, user_id, &[response("risk_1", 1)])
        .await
        .unwrap();
    repo::upsert_responses(&pool, user_id, &[response("risk_1", 5)])
        .await
        .unwrap();

    let answers = repo::answers_map(&pool, user_id).await.unwrap();
    assert_eq!(answers.get("risk_1"), Some(&5));
    assert_eq!(repo::answered_count(&pool, user_id).await.unwrap(), 1);
}

#[sqlx::test]
async fn responses_are_scoped_to_one_user(pool: PgPool) {
    let ada = a_user(&pool, "ada@example.com").await;
    let grace = a_user(&pool, "grace@example.com").await;

    repo::upsert_responses(&pool, ada, &[response("risk_1", 4)])
        .await
        .unwrap();

    assert_eq!(repo::answered_count(&pool, grace).await.unwrap(), 0);
}

#[sqlx::test]
async fn trait_scores_are_saved_and_read_back(pool: PgPool) {
    let user_id = a_user(&pool, "ada@example.com").await;
    let scores = TraitScores {
        risk_tolerance: 80,
        pace_vs_rigor: 60,
        conflict_style: 40,
        decision_basis: 20,
        work_mode: 0,
        orientation: 100,
    };

    repo::save_trait_scores(&pool, user_id, &scores)
        .await
        .unwrap();

    let loaded = repo::trait_scores_for(&pool, user_id).await.unwrap();
    assert_eq!(loaded, Some(scores));
}

#[sqlx::test]
async fn saving_trait_scores_twice_updates_in_place(pool: PgPool) {
    let user_id = a_user(&pool, "ada@example.com").await;
    let first = TraitScores {
        risk_tolerance: 10,
        pace_vs_rigor: 10,
        conflict_style: 10,
        decision_basis: 10,
        work_mode: 10,
        orientation: 10,
    };
    let second = TraitScores {
        risk_tolerance: 90,
        ..first
    };

    repo::save_trait_scores(&pool, user_id, &first)
        .await
        .unwrap();
    repo::save_trait_scores(&pool, user_id, &second)
        .await
        .unwrap();

    assert_eq!(
        repo::trait_scores_for(&pool, user_id).await.unwrap(),
        Some(second)
    );

    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM trait_scores")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 1);
}

#[sqlx::test]
async fn trait_scores_can_be_removed(pool: PgPool) {
    let user_id = a_user(&pool, "ada@example.com").await;
    let scores = TraitScores {
        risk_tolerance: 50,
        pace_vs_rigor: 50,
        conflict_style: 50,
        decision_basis: 50,
        work_mode: 50,
        orientation: 50,
    };

    repo::save_trait_scores(&pool, user_id, &scores)
        .await
        .unwrap();
    repo::delete_trait_scores(&pool, user_id).await.unwrap();

    assert_eq!(repo::trait_scores_for(&pool, user_id).await.unwrap(), None);
}

#[sqlx::test]
async fn removing_absent_trait_scores_is_not_an_error(pool: PgPool) {
    let user_id = a_user(&pool, "ada@example.com").await;

    repo::delete_trait_scores(&pool, user_id).await.unwrap();
}
