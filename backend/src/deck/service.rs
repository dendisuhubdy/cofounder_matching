use std::collections::{HashMap, HashSet};

use uuid::Uuid;

use crate::app::AppState;
use crate::deck::repo::{self, Candidate};
use crate::error::ApiResult;
use crate::scoring::profile::Reason;
use crate::scoring::score;

pub const DECK_SIZE: usize = 20;
/// How far a candidate resembling the viewer's recent passes can be pushed
/// down. Deliberately small: it nudges, it does not filter.
pub const MAX_PASS_PENALTY: f64 = 5.0;
/// Capped well below the smallest scoring component so popularity can never
/// outweigh genuine fit.
pub const MAX_POPULARITY_BOOST: f64 = 3.0;
pub const POPULARITY_WINDOW_DAYS: i32 = 30;
pub const RECENT_PASSES: i64 = 20;

#[derive(Debug, Clone, serde::Serialize)]
pub struct DeckCard {
    pub user_id: Uuid,
    pub display_name: String,
    pub headline: String,
    pub bio: String,
    pub city: String,
    pub country: String,
    pub roles: Vec<String>,
    pub seeking_roles: Vec<String>,
    pub interests: Vec<String>,
    pub score: u16,
    pub reasons: Vec<Reason>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DeckView {
    pub cards: Vec<DeckCard>,
    /// False when the viewer's own profile is not complete. They get an
    /// empty deck and a reason rather than an error, because the frontend
    /// wants to show a prompt, not a failure.
    pub profile_complete: bool,
}

/// Candidates resembling the viewer's recent passes lose ground, in
/// proportion to how much of their own tag set the viewer has been rejecting.
fn pass_penalty(candidate: &Candidate, passed: &HashSet<String>) -> f64 {
    if passed.is_empty() {
        return 0.0;
    }

    let own: Vec<&String> = candidate
        .profile
        .roles
        .iter()
        .chain(candidate.profile.interests.iter())
        .collect();

    if own.is_empty() {
        return 0.0;
    }

    let shared = own.iter().filter(|tag| passed.contains(**tag)).count();

    (shared as f64 / own.len() as f64) * MAX_PASS_PENALTY
}

pub async fn build(state: &AppState, viewer_id: Uuid) -> ApiResult<DeckView> {
    let Some(viewer) = repo::load_profile(&state.db, viewer_id).await? else {
        return Ok(DeckView {
            cards: Vec::new(),
            profile_complete: false,
        });
    };

    let candidates = repo::candidates_for(&state.db, viewer_id).await?;

    let passed: HashSet<String> = repo::recent_pass_tags(&state.db, viewer_id, RECENT_PASSES)
        .await?
        .into_iter()
        .collect();

    let rates: HashMap<Uuid, f64> = repo::right_swipe_rates(&state.db, POPULARITY_WINDOW_DAYS)
        .await?
        .into_iter()
        .collect();

    let mut cards: Vec<DeckCard> = candidates
        .into_iter()
        .map(|candidate| {
            let scored = score::score(&viewer.profile, &candidate.profile);

            let penalty = pass_penalty(&candidate, &passed);
            let boost = rates
                .get(&candidate.profile.user_id)
                .copied()
                .unwrap_or(0.0)
                * MAX_POPULARITY_BOOST;

            let adjusted = (f64::from(scored.total) - penalty + boost)
                .clamp(0.0, f64::from(score::MAX_TOTAL));

            DeckCard {
                user_id: candidate.profile.user_id,
                display_name: candidate.profile.display_name,
                headline: candidate.headline,
                bio: candidate.bio,
                city: candidate.profile.city,
                country: candidate.profile.country,
                roles: candidate.profile.roles,
                seeking_roles: candidate.profile.seeking_roles,
                interests: candidate.profile.interests,
                score: adjusted.round() as u16,
                reasons: scored.reasons,
            }
        })
        .collect();

    // Descending by score, then by id so that equal scores keep a stable
    // order across requests rather than shuffling between page loads.
    cards.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.user_id.cmp(&b.user_id)));
    cards.truncate(DECK_SIZE);

    Ok(DeckView {
        cards,
        profile_complete: true,
    })
}
