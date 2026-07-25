#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Axis {
    RiskTolerance,
    PaceVsRigor,
    ConflictStyle,
    DecisionBasis,
    WorkMode,
    Orientation,
}

impl Axis {
    pub const ALL: [Axis; 6] = [
        Axis::RiskTolerance,
        Axis::PaceVsRigor,
        Axis::ConflictStyle,
        Axis::DecisionBasis,
        Axis::WorkMode,
        Axis::Orientation,
    ];

    pub fn slug(self) -> &'static str {
        match self {
            Axis::RiskTolerance => "risk_tolerance",
            Axis::PaceVsRigor => "pace_vs_rigor",
            Axis::ConflictStyle => "conflict_style",
            Axis::DecisionBasis => "decision_basis",
            Axis::WorkMode => "work_mode",
            Axis::Orientation => "orientation",
        }
    }

    /// The wording used on cards. Taken from the design document's axis
    /// table so an explanation cannot describe an axis differently from the
    /// way it is scored.
    pub fn low_label(self) -> &'static str {
        match self {
            Axis::RiskTolerance => "de-risk before committing",
            Axis::PaceVsRigor => "build it right",
            Axis::ConflictStyle => "seek harmony",
            Axis::DecisionBasis => "trust intuition",
            Axis::WorkMode => "deep solo work",
            Axis::Orientation => "near-term execution",
        }
    }

    pub fn high_label(self) -> &'static str {
        match self {
            Axis::RiskTolerance => "bet big early",
            Axis::PaceVsRigor => "ship it now",
            Axis::ConflictStyle => "address directly",
            Axis::DecisionBasis => "require data",
            Axis::WorkMode => "constant collaboration",
            Axis::Orientation => "long-range vision",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Question {
    pub id: &'static str,
    pub text: &'static str,
    pub axis: Axis,
    /// A high agreement on this item means a *low* axis score. Never sent to
    /// the client: knowing which items are flipped is enough to fake a
    /// coherent profile.
    pub reverse: bool,
}

/// The instrument itself. Versioned with the code rather than stored in the
/// database, so a change to the wording is a reviewable diff and the scoring
/// tests move with it.
pub const QUESTIONS: [Question; 18] = [
    // risk_tolerance — low: de-risk before committing, high: bet big early
    Question {
        id: "risk_1",
        text: "I would rather launch an unproven idea than spend months validating it.",
        axis: Axis::RiskTolerance,
        reverse: false,
    },
    Question {
        id: "risk_2",
        text: "I need strong evidence that a market exists before I commit to building for it.",
        axis: Axis::RiskTolerance,
        reverse: true,
    },
    Question {
        id: "risk_3",
        text: "A big, uncertain outcome appeals to me more than a safe, modest one.",
        axis: Axis::RiskTolerance,
        reverse: false,
    },
    // pace_vs_rigor — low: build it right, high: ship it now
    Question {
        id: "pace_1",
        text: "Shipping something rough this week beats shipping something polished next month.",
        axis: Axis::PaceVsRigor,
        reverse: false,
    },
    Question {
        id: "pace_2",
        text: "I would hold a release back to fix problems most users would never notice.",
        axis: Axis::PaceVsRigor,
        reverse: true,
    },
    Question {
        id: "pace_3",
        text: "I am comfortable taking on technical debt to hit a deadline.",
        axis: Axis::PaceVsRigor,
        reverse: false,
    },
    // conflict_style — low: seek harmony, high: address directly
    Question {
        id: "conflict_1",
        text: "When a teammate's work falls short, I tell them plainly and quickly.",
        axis: Axis::ConflictStyle,
        reverse: false,
    },
    Question {
        id: "conflict_2",
        text: "I would rather let a small disagreement pass than risk souring the mood.",
        axis: Axis::ConflictStyle,
        reverse: true,
    },
    Question {
        id: "conflict_3",
        text: "I raise disagreements in the room rather than working around them afterwards.",
        axis: Axis::ConflictStyle,
        reverse: false,
    },
    // decision_basis — low: trust intuition, high: require data
    Question {
        id: "decision_1",
        text: "I want to see the numbers before I make a call.",
        axis: Axis::DecisionBasis,
        reverse: false,
    },
    Question {
        id: "decision_2",
        text: "When the evidence is ambiguous, I trust my instinct and move.",
        axis: Axis::DecisionBasis,
        reverse: true,
    },
    Question {
        id: "decision_3",
        text: "A decision with no measurable evidence behind it makes me uncomfortable.",
        axis: Axis::DecisionBasis,
        reverse: false,
    },
    // work_mode — low: deep solo work, high: constant collaboration
    Question {
        id: "work_1",
        text: "I do my best work in long stretches of uninterrupted solo time.",
        axis: Axis::WorkMode,
        reverse: true,
    },
    Question {
        id: "work_2",
        text: "I would rather think a problem through out loud with someone than alone.",
        axis: Axis::WorkMode,
        reverse: false,
    },
    Question {
        id: "work_3",
        text: "A day full of conversations energizes me more than it drains me.",
        axis: Axis::WorkMode,
        reverse: false,
    },
    // orientation — low: near-term execution, high: long-range vision
    Question {
        id: "orientation_1",
        text: "I spend more time on where we will be in five years than on what ships this week.",
        axis: Axis::Orientation,
        reverse: false,
    },
    Question {
        id: "orientation_2",
        text: "I would rather hit this quarter's targets than debate the ten-year plan.",
        axis: Axis::Orientation,
        reverse: true,
    },
    Question {
        id: "orientation_3",
        text: "Painting a compelling long-term picture is where I add the most value.",
        axis: Axis::Orientation,
        reverse: false,
    },
];

pub fn find(id: &str) -> Option<&'static Question> {
    QUESTIONS.iter().find(|question| question.id == id)
}
