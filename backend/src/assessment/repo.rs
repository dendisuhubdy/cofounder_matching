use std::collections::HashMap;

use sqlx::PgPool;
use uuid::Uuid;

use crate::assessment::scoring::TraitScores;

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow, serde::Serialize, serde::Deserialize)]
pub struct Response {
    pub question_id: String,
    pub value: i16,
}

pub async fn responses_for(pool: &PgPool, user_id: Uuid) -> sqlx::Result<Vec<Response>> {
    sqlx::query_as::<_, Response>(
        r#"
        SELECT question_id, value
        FROM question_responses
        WHERE user_id = $1
        ORDER BY question_id
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}

pub async fn answers_map(pool: &PgPool, user_id: Uuid) -> sqlx::Result<HashMap<String, i16>> {
    let responses = responses_for(pool, user_id).await?;

    Ok(responses
        .into_iter()
        .map(|response| (response.question_id, response.value))
        .collect())
}

pub async fn upsert_responses(
    pool: &PgPool,
    user_id: Uuid,
    responses: &[Response],
) -> sqlx::Result<()> {
    if responses.is_empty() {
        return Ok(());
    }

    let ids: Vec<String> = responses.iter().map(|r| r.question_id.clone()).collect();
    let values: Vec<i16> = responses.iter().map(|r| r.value).collect();

    // One statement rather than a loop: the whole batch lands or none of it does.
    sqlx::query(
        r#"
        INSERT INTO question_responses (user_id, question_id, value, updated_at)
        SELECT $1, submitted.id, submitted.value, now()
        FROM UNNEST($2::TEXT[], $3::SMALLINT[]) AS submitted(id, value)
        ON CONFLICT (user_id, question_id)
        DO UPDATE SET value = EXCLUDED.value, updated_at = now()
        "#,
    )
    .bind(user_id)
    .bind(&ids)
    .bind(&values)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn answered_count(pool: &PgPool, user_id: Uuid) -> sqlx::Result<i64> {
    sqlx::query_scalar("SELECT count(*) FROM question_responses WHERE user_id = $1")
        .bind(user_id)
        .fetch_one(pool)
        .await
}

pub async fn save_trait_scores(
    pool: &PgPool,
    user_id: Uuid,
    scores: &TraitScores,
) -> sqlx::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO trait_scores (
            user_id, risk_tolerance, pace_vs_rigor, conflict_style,
            decision_basis, work_mode, orientation, computed_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, now())
        ON CONFLICT (user_id) DO UPDATE SET
            risk_tolerance = EXCLUDED.risk_tolerance,
            pace_vs_rigor  = EXCLUDED.pace_vs_rigor,
            conflict_style = EXCLUDED.conflict_style,
            decision_basis = EXCLUDED.decision_basis,
            work_mode      = EXCLUDED.work_mode,
            orientation    = EXCLUDED.orientation,
            computed_at    = now()
        "#,
    )
    .bind(user_id)
    .bind(scores.risk_tolerance)
    .bind(scores.pace_vs_rigor)
    .bind(scores.conflict_style)
    .bind(scores.decision_basis)
    .bind(scores.work_mode)
    .bind(scores.orientation)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn delete_trait_scores(pool: &PgPool, user_id: Uuid) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM trait_scores WHERE user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await?;

    Ok(())
}

pub async fn trait_scores_for(pool: &PgPool, user_id: Uuid) -> sqlx::Result<Option<TraitScores>> {
    sqlx::query_as::<_, TraitScores>(
        r#"
        SELECT risk_tolerance, pace_vs_rigor, conflict_style,
               decision_basis, work_mode, orientation
        FROM trait_scores
        WHERE user_id = $1
        "#,
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
}
