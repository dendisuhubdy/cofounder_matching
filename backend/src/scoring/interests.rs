use std::collections::HashSet;

use crate::profiles::vocab::INTERESTS;
use crate::scoring::profile::{Component, ComponentScore, ScoredProfile};

pub const MAX_POINTS: f64 = 15.0;

pub fn score_interests(viewer: &ScoredProfile, candidate: &ScoredProfile) -> ComponentScore {
    let mine: HashSet<&str> = viewer.interests.iter().map(String::as_str).collect();
    let theirs: HashSet<&str> = candidate.interests.iter().map(String::as_str).collect();

    let union = mine.union(&theirs).count();
    if union == 0 {
        // Two empty sets are not a perfect overlap; they are no information.
        return ComponentScore::empty(Component::Interests);
    }

    let shared: Vec<&str> = mine.intersection(&theirs).copied().collect();
    let jaccard = shared.len() as f64 / union as f64;

    // Named in vocabulary order rather than set order, which is arbitrary,
    // so the same pair always produces the same sentence.
    let reason = INTERESTS
        .iter()
        .find(|choice| shared.contains(&choice.id))
        .map(|choice| {
            if shared.len() > 1 {
                format!("Share {} interests including {}", shared.len(), choice.label)
            } else {
                format!("Both into {}", choice.label)
            }
        });

    ComponentScore::new(Component::Interests, jaccard * MAX_POINTS, reason)
}
