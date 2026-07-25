use crate::profiles::vocab::{self, COMMITMENTS, STAGES};
use crate::scoring::profile::{Component, ComponentScore, ScoredProfile};

pub const MAX_POINTS: f64 = 20.0;

/// Commitment carries most of the weight; stage proximity contributes a
/// smaller share, as the design document specifies.
const COMMITMENT_POINTS: f64 = 14.0;
const STAGE_POINTS: f64 = 6.0;

/// Both vocabularies are ladders, so a position in the list is a meaningful
/// distance rather than an arbitrary index.
fn rung(choices: &[vocab::Choice], id: &Option<String>) -> Option<usize> {
    let id = id.as_deref()?;
    choices.iter().position(|choice| choice.id == id)
}

fn ladder_fraction(steps: usize, falloff: &[f64]) -> f64 {
    falloff.get(steps).copied().unwrap_or(0.0)
}

pub fn score_situation(viewer: &ScoredProfile, candidate: &ScoredProfile) -> ComponentScore {
    // Identical scores full, adjacent scores most, distant scores near zero.
    const COMMITMENT_FALLOFF: [f64; 4] = [1.0, 0.7, 0.3, 0.0];
    const STAGE_FALLOFF: [f64; 4] = [1.0, 0.6, 0.3, 0.0];

    let mut points = 0.0;
    let mut reason = None;

    if let (Some(a), Some(b)) = (
        rung(&COMMITMENTS, &viewer.commitment),
        rung(&COMMITMENTS, &candidate.commitment),
    ) {
        let steps = a.abs_diff(b);
        points += ladder_fraction(steps, &COMMITMENT_FALLOFF) * COMMITMENT_POINTS;

        if steps == 0 {
            if let Some(label) = vocab::label(&COMMITMENTS, COMMITMENTS[a].id) {
                reason = Some(format!("Both {label}"));
            }
        }
    }

    if let (Some(a), Some(b)) = (
        rung(&STAGES, &viewer.stage),
        rung(&STAGES, &candidate.stage),
    ) {
        points += ladder_fraction(a.abs_diff(b), &STAGE_FALLOFF) * STAGE_POINTS;
    }

    ComponentScore::new(Component::Situation, points, reason)
}
