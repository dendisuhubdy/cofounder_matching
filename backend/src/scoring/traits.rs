use crate::assessment::questions::Axis;
use crate::assessment::scoring::TraitScores;
use crate::scoring::profile::{Component, ComponentScore, ScoredProfile};

pub const MAX_POINTS: f64 = 25.0;

/// What a healthy pair looks like on one axis. Distances are on the same
/// 0–100 scale as the axis scores themselves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ideal {
    Similar,
    MildDifference,
    Complementary,
}

impl Ideal {
    pub fn target_distance(self) -> f64 {
        match self {
            Ideal::Similar => 0.0,
            Ideal::MildDifference => 25.0,
            Ideal::Complementary => 60.0,
        }
    }
}

/// The design document's table, verbatim:
///
/// | axis           | ideal           | why                                       |
/// |----------------|-----------------|-------------------------------------------|
/// | risk_tolerance | similar         | divergent risk appetite breaks cofounders |
/// | pace_vs_rigor  | similar         | ship-fast and build-right partners fight  |
/// | conflict_style | similar         | mismatched styles breed resentment        |
/// | decision_basis | mild difference | instinct against evidence is productive   |
/// | work_mode      | complementary   | one goes deep, one runs relationships     |
/// | orientation    | complementary   | vision and execution is the classic pair  |
pub fn ideal_for(axis: Axis) -> Ideal {
    match axis {
        Axis::RiskTolerance => Ideal::Similar,
        Axis::PaceVsRigor => Ideal::Similar,
        Axis::ConflictStyle => Ideal::Similar,
        Axis::DecisionBasis => Ideal::MildDifference,
        Axis::WorkMode => Ideal::Complementary,
        Axis::Orientation => Ideal::Complementary,
    }
}

fn value_of(traits: &TraitScores, axis: Axis) -> f64 {
    let raw = match axis {
        Axis::RiskTolerance => traits.risk_tolerance,
        Axis::PaceVsRigor => traits.pace_vs_rigor,
        Axis::ConflictStyle => traits.conflict_style,
        Axis::DecisionBasis => traits.decision_basis,
        Axis::WorkMode => traits.work_mode,
        Axis::Orientation => traits.orientation,
    };
    f64::from(raw)
}

/// How well one axis sits against its ideal: 1.0 exactly on target, falling
/// linearly to 0.0 when it is the full range away from it.
fn axis_fraction(viewer: &TraitScores, candidate: &TraitScores, axis: Axis) -> f64 {
    let distance = (value_of(viewer, axis) - value_of(candidate, axis)).abs();
    let miss = (distance - ideal_for(axis).target_distance()).abs();
    (1.0 - miss / 100.0).clamp(0.0, 1.0)
}

fn explain(viewer: &TraitScores, candidate: &TraitScores, axis: Axis) -> String {
    let a = value_of(viewer, axis);
    let b = value_of(candidate, axis);

    match ideal_for(axis) {
        Ideal::Complementary => {
            format!("{} meets {}", axis.low_label(), axis.high_label())
        }
        Ideal::Similar | Ideal::MildDifference => {
            let midpoint = (a + b) / 2.0;
            if midpoint >= 50.0 {
                format!("Both {}", axis.high_label())
            } else {
                format!("Both {}", axis.low_label())
            }
        }
    }
}

/// Axes are equally weighted within the budget. Each contributes in
/// proportion to how close the pair's actual distance sits to that axis's
/// ideal distance — which is not the same thing as being alike.
pub fn score_traits(viewer: &ScoredProfile, candidate: &ScoredProfile) -> ComponentScore {
    let per_axis = MAX_POINTS / Axis::ALL.len() as f64;

    let mut points = 0.0;
    let mut best: Option<(f64, Axis)> = None;

    for axis in Axis::ALL {
        let fraction = axis_fraction(&viewer.traits, &candidate.traits, axis);
        points += fraction * per_axis;

        if best.is_none_or(|(top, _)| fraction > top) {
            best = Some((fraction, axis));
        }
    }

    // Only explain an axis that actually sits well; a card should not boast
    // about the least bad of six poor fits.
    let reason = best
        .filter(|(fraction, _)| *fraction >= 0.75)
        .map(|(_, axis)| explain(&viewer.traits, &candidate.traits, axis));

    ComponentScore::new(Component::Traits, points, reason)
}
