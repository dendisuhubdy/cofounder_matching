use std::collections::HashSet;

use crate::profiles::vocab::ROLES;
use crate::scoring::profile::{Component, ComponentScore, ScoredProfile};

pub const MAX_POINTS: f64 = 30.0;
const PER_DIRECTION: f64 = MAX_POINTS / 2.0;

/// What fraction of `wanted` is covered by `offered`, and the first covered
/// role in the vocabulary's own order so the explanation is deterministic.
fn coverage(offered: &[String], wanted: &[String]) -> (f64, Option<&'static str>) {
    if wanted.is_empty() {
        return (0.0, None);
    }

    let offered: HashSet<&str> = offered.iter().map(String::as_str).collect();
    let matched: Vec<&str> = wanted
        .iter()
        .map(String::as_str)
        .filter(|role| offered.contains(role))
        .collect();

    if matched.is_empty() {
        return (0.0, None);
    }

    let fraction = matched.len() as f64 / wanted.len() as f64;

    // Ordered by the vocabulary rather than by the user's array so that the
    // same pair always produces the same sentence.
    let first = ROLES
        .iter()
        .find(|choice| matched.contains(&choice.id))
        .map(|choice| choice.label);

    (fraction.min(1.0), first)
}

/// The strongest single signal in the model: a technical founder seeking GTM
/// matched with a GTM founder seeking technical.
pub fn score_roles(viewer: &ScoredProfile, candidate: &ScoredProfile) -> ComponentScore {
    let (they_offer, they_bring) = coverage(&candidate.roles, &viewer.seeking_roles);
    let (you_offer, you_bring) = coverage(&viewer.roles, &candidate.seeking_roles);

    let points = they_offer * PER_DIRECTION + you_offer * PER_DIRECTION;

    let reason = match (they_bring, you_bring) {
        (Some(theirs), Some(yours)) => Some(format!("They bring {theirs}, you bring {yours}")),
        (Some(theirs), None) => Some(format!("Brings {theirs}, which you're after")),
        (None, Some(yours)) => Some(format!("Looking for {yours}, which you bring")),
        (None, None) => None,
    };

    ComponentScore::new(Component::Roles, points, reason)
}
