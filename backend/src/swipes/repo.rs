use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Left,
    Right,
}

impl Direction {
    pub fn as_str(self) -> &'static str {
        match self {
            Direction::Left => "left",
            Direction::Right => "right",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct SwipeOutcome {
    pub matched: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow, serde::Serialize)]
pub struct MatchedUser {
    pub user_id: Uuid,
    pub display_name: String,
    pub headline: String,
    pub matched_at: DateTime<Utc>,
}

/// A match is one row for the pair, so the two ids are stored in a fixed
/// order. Uuid ordering is arbitrary but stable, which is all that is needed.
fn ordered(a: Uuid, b: Uuid) -> (Uuid, Uuid) {
    if a < b {
        (a, b)
    } else {
        (b, a)
    }
}

/// Records a swipe and, when it completes a mutual right swipe, the match it
/// creates — in one transaction, so a match can never exist without the
/// swipe that caused it. Returns `None` if this pair was already swiped:
/// swipes are permanent, so a second one is a conflict rather than an update.
pub async fn record_swipe(
    pool: &PgPool,
    swiper_id: Uuid,
    target_id: Uuid,
    direction: Direction,
) -> sqlx::Result<Option<SwipeOutcome>> {
    let mut tx = pool.begin().await?;

    let inserted: Option<Uuid> = sqlx::query_scalar(
        r#"
        INSERT INTO swipes (swiper_id, target_id, direction)
        VALUES ($1, $2, $3)
        ON CONFLICT (swiper_id, target_id) DO NOTHING
        RETURNING swiper_id
        "#,
    )
    .bind(swiper_id)
    .bind(target_id)
    .bind(direction.as_str())
    .fetch_optional(&mut *tx)
    .await?;

    if inserted.is_none() {
        tx.rollback().await?;
        return Ok(None);
    }

    let mut matched = false;

    if direction == Direction::Right {
        let reciprocated: Option<Uuid> = sqlx::query_scalar(
            r#"
            SELECT swiper_id FROM swipes
            WHERE swiper_id = $1 AND target_id = $2 AND direction = 'right'
            "#,
        )
        .bind(target_id)
        .bind(swiper_id)
        .fetch_optional(&mut *tx)
        .await?;

        if reciprocated.is_some() {
            let (a, b) = ordered(swiper_id, target_id);

            sqlx::query(
                r#"
                INSERT INTO matches (user_a_id, user_b_id)
                VALUES ($1, $2)
                ON CONFLICT (user_a_id, user_b_id) DO NOTHING
                "#,
            )
            .bind(a)
            .bind(b)
            .execute(&mut *tx)
            .await?;

            matched = true;
        }
    }

    tx.commit().await?;

    Ok(Some(SwipeOutcome { matched }))
}

/// Both sides of a match see the other person, so the query looks at each
/// column in turn and selects whichever id is not the caller's.
pub async fn matches_for(pool: &PgPool, user_id: Uuid) -> sqlx::Result<Vec<MatchedUser>> {
    sqlx::query_as::<_, MatchedUser>(
        r#"
        SELECT
            other.id            AS user_id,
            p.display_name      AS display_name,
            p.headline          AS headline,
            m.created_at        AS matched_at
        FROM matches m
        JOIN users other
          ON other.id = CASE WHEN m.user_a_id = $1 THEN m.user_b_id ELSE m.user_a_id END
        JOIN profiles p ON p.user_id = other.id
        WHERE $1 IN (m.user_a_id, m.user_b_id)
        ORDER BY m.created_at DESC
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}

pub async fn recent_left_swipe_targets(
    pool: &PgPool,
    user_id: Uuid,
    limit: i64,
) -> sqlx::Result<Vec<Uuid>> {
    sqlx::query_scalar(
        r#"
        SELECT target_id FROM swipes
        WHERE swiper_id = $1 AND direction = 'left'
        ORDER BY created_at DESC
        LIMIT $2
        "#,
    )
    .bind(user_id)
    .bind(limit)
    .fetch_all(pool)
    .await
}
