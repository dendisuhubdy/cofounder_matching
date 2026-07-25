use crate::scoring::profile::{MatchScore, Reason, ScoredProfile};
use crate::scoring::{geography, interests, roles, situation, traits};

pub const MAX_TOTAL: u16 = 100;
pub const MAX_REASONS: usize = 3;

/// A pure function: no database, no clock, no I/O. Swipe-history adjustments
/// live in `deck::service` precisely so that this stays true and the whole
/// point table can be tested exhaustively.
pub fn score(viewer: &ScoredProfile, candidate: &ScoredProfile) -> MatchScore {
    let components = [
        roles::score_roles(viewer, candidate),
        traits::score_traits(viewer, candidate),
        situation::score_situation(viewer, candidate),
        interests::score_interests(viewer, candidate),
        geography::score_geography(viewer, candidate),
    ];

    let raw: f64 = components.iter().map(|component| component.points).sum();
    let total = raw.round().clamp(0.0, f64::from(MAX_TOTAL)) as u16;

    let mut ranked: Vec<_> = components
        .iter()
        .filter(|component| component.points > 0.0 && component.reason.is_some())
        .collect();

    // Descending by contribution. `total_cmp` rather than `partial_cmp`
    // because these are plain finite floats and this avoids an unwrap.
    ranked.sort_by(|a, b| b.points.total_cmp(&a.points));

    let reasons = ranked
        .into_iter()
        .take(MAX_REASONS)
        .map(|component| Reason {
            component: component.component,
            text: component.reason.clone().unwrap_or_default(),
        })
        .collect();

    MatchScore { total, reasons }
}
