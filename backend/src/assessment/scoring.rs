use std::collections::HashMap;

use crate::assessment::questions::{Axis, QUESTIONS};

/// One 0–100 score per axis. Derives `FromRow` so `trait_scores` can be read
/// straight back into it; that is a column mapping, not I/O, and `compute`
/// below stays a pure function.
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::FromRow, serde::Serialize)]
pub struct TraitScores {
    pub risk_tolerance: i16,
    pub pace_vs_rigor: i16,
    pub conflict_style: i16,
    pub decision_basis: i16,
    pub work_mode: i16,
    pub orientation: i16,
}

fn axis_score(answers: &HashMap<String, i16>, axis: Axis) -> Option<i16> {
    let mut sum = 0.0_f64;
    let mut count = 0.0_f64;

    for question in QUESTIONS.iter().filter(|q| q.axis == axis) {
        let raw = *answers.get(question.id)?;
        // A 1–5 Likert value flips around its midpoint: 1 becomes 5, 5 becomes 1.
        let oriented = if question.reverse { 6 - raw } else { raw };
        sum += f64::from(oriented);
        count += 1.0;
    }

    let mean = sum / count;
    Some((((mean - 1.0) / 4.0) * 100.0).round() as i16)
}

/// `None` unless every one of the 18 questions has an answer. An axis mean
/// over one item is not comparable with one over three, and a partial score
/// would quietly skew every match the deck produces.
pub fn compute(answers: &HashMap<String, i16>) -> Option<TraitScores> {
    Some(TraitScores {
        risk_tolerance: axis_score(answers, Axis::RiskTolerance)?,
        pace_vs_rigor: axis_score(answers, Axis::PaceVsRigor)?,
        conflict_style: axis_score(answers, Axis::ConflictStyle)?,
        decision_basis: axis_score(answers, Axis::DecisionBasis)?,
        work_mode: axis_score(answers, Axis::WorkMode)?,
        orientation: axis_score(answers, Axis::Orientation)?,
    })
}
