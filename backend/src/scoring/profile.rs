use uuid::Uuid;

use crate::assessment::scoring::TraitScores;

/// Everything the scorer is allowed to see. Assembled by `deck::repo` from
/// `profiles`, `profile_interests` and `trait_scores`; the scorer itself
/// never touches the database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoredProfile {
    pub user_id: Uuid,
    pub display_name: String,
    pub roles: Vec<String>,
    pub seeking_roles: Vec<String>,
    pub interests: Vec<String>,
    pub idea_status: Option<String>,
    pub stage: Option<String>,
    pub commitment: Option<String>,
    pub city: String,
    pub country: String,
    /// Derived from the IANA name when the profile is saved. `None` for a
    /// profile written before the column existed, or one with no timezone.
    pub utc_offset_minutes: Option<i16>,
    pub traits: TraitScores,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Component {
    Roles,
    Traits,
    Situation,
    Interests,
    Geography,
}

/// One component's contribution, with the sentence that explains it. The
/// reason is produced here rather than reconstructed later, so what a card
/// says can never drift from what actually scored.
#[derive(Debug, Clone, PartialEq)]
pub struct ComponentScore {
    pub component: Component,
    pub points: f64,
    pub reason: Option<String>,
}

impl ComponentScore {
    pub fn new(component: Component, points: f64, reason: Option<String>) -> Self {
        Self {
            component,
            points,
            reason,
        }
    }

    pub fn empty(component: Component) -> Self {
        Self {
            component,
            points: 0.0,
            reason: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Reason {
    pub component: Component,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct MatchScore {
    pub total: u16,
    pub reasons: Vec<Reason>,
}
