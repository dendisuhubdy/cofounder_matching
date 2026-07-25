use sqlx::PgPool;
use uuid::Uuid;

use crate::assessment::scoring::TraitScores;
use crate::scoring::profile::ScoredProfile;

/// A candidate as the deck needs them: everything the scorer reads, plus the
/// display-only fields a card shows. Kept separate from `ScoredProfile` so
/// that prose never leaks into the scoring type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub profile: ScoredProfile,
    pub headline: String,
    pub bio: String,
}

#[derive(sqlx::FromRow)]
struct CandidateRow {
    user_id: Uuid,
    display_name: String,
    headline: String,
    bio: String,
    city: String,
    country: String,
    utc_offset_minutes: Option<i16>,
    roles: Vec<String>,
    seeking_roles: Vec<String>,
    interests: Vec<String>,
    idea_status: Option<String>,
    stage: Option<String>,
    commitment: Option<String>,
    risk_tolerance: i16,
    pace_vs_rigor: i16,
    conflict_style: i16,
    decision_basis: i16,
    work_mode: i16,
    orientation: i16,
}

impl From<CandidateRow> for Candidate {
    fn from(row: CandidateRow) -> Self {
        Candidate {
            headline: row.headline,
            bio: row.bio,
            profile: ScoredProfile {
                user_id: row.user_id,
                display_name: row.display_name,
                roles: row.roles,
                seeking_roles: row.seeking_roles,
                interests: row.interests,
                idea_status: row.idea_status,
                stage: row.stage,
                commitment: row.commitment,
                city: row.city,
                country: row.country,
                utc_offset_minutes: row.utc_offset_minutes,
                traits: TraitScores {
                    risk_tolerance: row.risk_tolerance,
                    pace_vs_rigor: row.pace_vs_rigor,
                    conflict_style: row.conflict_style,
                    decision_basis: row.decision_basis,
                    work_mode: row.work_mode,
                    orientation: row.orientation,
                },
            },
        }
    }
}

/// Selected columns and the joins that define completeness. The join onto
/// `trait_scores` is the completeness check for the assessment: that row
/// exists only when all eighteen answers do, so there is no need to count
/// responses here.
const SELECT: &str = r#"
    SELECT
        u.id                 AS user_id,
        p.display_name       AS display_name,
        p.headline           AS headline,
        p.bio                AS bio,
        p.city               AS city,
        p.country            AS country,
        p.utc_offset_minutes AS utc_offset_minutes,
        p.roles              AS roles,
        p.seeking_roles      AS seeking_roles,
        COALESCE(
            ARRAY_AGG(pi.tag ORDER BY pi.tag) FILTER (WHERE pi.tag IS NOT NULL),
            '{}'
        )                    AS interests,
        p.idea_status        AS idea_status,
        p.stage              AS stage,
        p.commitment         AS commitment,
        t.risk_tolerance     AS risk_tolerance,
        t.pace_vs_rigor      AS pace_vs_rigor,
        t.conflict_style     AS conflict_style,
        t.decision_basis     AS decision_basis,
        t.work_mode          AS work_mode,
        t.orientation        AS orientation
    FROM users u
    JOIN profiles p     ON p.user_id = u.id
    JOIN trait_scores t ON t.user_id = u.id
    LEFT JOIN profile_interests pi ON pi.user_id = u.id
"#;

/// The completeness rule, in SQL. Mirrors `profiles::service::missing_requirements`.
const COMPLETE: &str = r#"
    u.status = 'active'
    AND btrim(p.bio) <> ''
    AND cardinality(p.roles) > 0
    AND cardinality(p.seeking_roles) > 0
    AND p.commitment IS NOT NULL
"#;

const GROUP_BY: &str = " GROUP BY u.id, p.user_id, t.user_id ";

pub async fn load_profile(pool: &PgPool, user_id: Uuid) -> sqlx::Result<Option<Candidate>> {
    let sql = format!("{SELECT} WHERE u.id = $1 AND {COMPLETE} {GROUP_BY}");

    let row = sqlx::query_as::<_, CandidateRow>(&sql)
        .bind(user_id)
        .fetch_optional(pool)
        .await?;

    Ok(row.map(Candidate::from))
}

/// The candidate pool: everyone except the viewer, anyone already swiped on,
/// anyone blocked in either direction, suspended accounts, and incomplete
/// profiles.
pub async fn candidates_for(pool: &PgPool, viewer_id: Uuid) -> sqlx::Result<Vec<Candidate>> {
    let sql = format!(
        r#"
        {SELECT}
        WHERE u.id <> $1
          AND {COMPLETE}
          AND NOT EXISTS (
              SELECT 1 FROM swipes s
              WHERE s.swiper_id = $1 AND s.target_id = u.id
          )
          AND NOT EXISTS (
              SELECT 1 FROM blocks b
              WHERE (b.blocker_id = $1 AND b.blocked_id = u.id)
                 OR (b.blocker_id = u.id AND b.blocked_id = $1)
          )
        {GROUP_BY}
        "#
    );

    let rows = sqlx::query_as::<_, CandidateRow>(&sql)
        .bind(viewer_id)
        .fetch_all(pool)
        .await?;

    Ok(rows.into_iter().map(Candidate::from).collect())
}

/// The role and interest tags of the people this viewer most recently passed
/// on. Feeds pass suppression.
pub async fn recent_pass_tags(
    pool: &PgPool,
    viewer_id: Uuid,
    limit: i64,
) -> sqlx::Result<Vec<String>> {
    sqlx::query_scalar(
        r#"
        WITH recent AS (
            SELECT target_id FROM swipes
            WHERE swiper_id = $1 AND direction = 'left'
            ORDER BY created_at DESC
            LIMIT $2
        )
        SELECT DISTINCT tag FROM (
            SELECT UNNEST(p.roles) AS tag
            FROM profiles p JOIN recent r ON r.target_id = p.user_id
            UNION ALL
            SELECT pi.tag
            FROM profile_interests pi JOIN recent r ON r.target_id = pi.user_id
        ) tags
        "#,
    )
    .bind(viewer_id)
    .bind(limit)
    .fetch_all(pool)
    .await
}

/// Each target's right-swipe rate over a trailing window. Feeds the
/// popularity boost. This reads the clock, which is exactly why it lives
/// here and not in the scorer.
pub async fn right_swipe_rates(pool: &PgPool, days: i32) -> sqlx::Result<Vec<(Uuid, f64)>> {
    sqlx::query_as(
        r#"
        SELECT
            target_id,
            count(*) FILTER (WHERE direction = 'right')::float8 / count(*)::float8 AS rate
        FROM swipes
        WHERE created_at > now() - make_interval(days => $1)
        GROUP BY target_id
        "#,
    )
    .bind(days)
    .fetch_all(pool)
    .await
}
