use cofounder_api::assessment::scoring::TraitScores;
use cofounder_api::scoring::profile::{Component, ScoredProfile};
use cofounder_api::scoring::score::{score, MAX_REASONS, MAX_TOTAL};
use uuid::Uuid;

fn blank() -> ScoredProfile {
    ScoredProfile {
        user_id: Uuid::new_v4(),
        display_name: "Someone".into(),
        roles: Vec::new(),
        seeking_roles: Vec::new(),
        interests: Vec::new(),
        idea_status: None,
        stage: None,
        commitment: None,
        city: String::new(),
        country: String::new(),
        utc_offset_minutes: None,
        traits: TraitScores {
            risk_tolerance: 50,
            pace_vs_rigor: 50,
            conflict_style: 50,
            decision_basis: 50,
            work_mode: 50,
            orientation: 50,
        },
    }
}

/// A pair sitting on the ideal of every single component.
fn perfect_pair() -> (ScoredProfile, ScoredProfile) {
    let viewer = ScoredProfile {
        roles: vec!["engineering".into()],
        seeking_roles: vec!["gtm".into()],
        interests: vec!["ai_ml".into()],
        stage: Some("prototype".into()),
        commitment: Some("full_time_now".into()),
        city: "Jakarta".into(),
        country: "Indonesia".into(),
        utc_offset_minutes: Some(420),
        traits: TraitScores {
            risk_tolerance: 50,
            pace_vs_rigor: 50,
            conflict_style: 50,
            decision_basis: 40,
            work_mode: 20,
            orientation: 20,
        },
        ..blank()
    };

    let candidate = ScoredProfile {
        roles: vec!["gtm".into()],
        seeking_roles: vec!["engineering".into()],
        interests: vec!["ai_ml".into()],
        stage: Some("prototype".into()),
        commitment: Some("full_time_now".into()),
        city: "Jakarta".into(),
        country: "Indonesia".into(),
        utc_offset_minutes: Some(420),
        traits: TraitScores {
            risk_tolerance: 50,
            pace_vs_rigor: 50,
            conflict_style: 50,
            decision_basis: 65,
            work_mode: 80,
            orientation: 80,
        },
        ..blank()
    };

    (viewer, candidate)
}

#[test]
fn the_components_add_up_to_exactly_one_hundred() {
    let (viewer, candidate) = perfect_pair();

    let result = score(&viewer, &candidate);

    assert_eq!(result.total, MAX_TOTAL);
    assert_eq!(MAX_TOTAL, 100);
}

#[test]
fn two_strangers_with_nothing_in_common_score_low() {
    let viewer = blank();
    let candidate = blank();

    let result = score(&viewer, &candidate);

    assert!(result.total < 30, "got {}", result.total);
}

#[test]
fn the_total_never_exceeds_the_budget() {
    let (viewer, candidate) = perfect_pair();

    assert!(score(&viewer, &candidate).total <= MAX_TOTAL);
}

#[test]
fn at_most_three_reasons_are_surfaced() {
    let (viewer, candidate) = perfect_pair();

    let result = score(&viewer, &candidate);

    assert!(
        result.reasons.len() <= MAX_REASONS,
        "got {}",
        result.reasons.len()
    );
    assert_eq!(MAX_REASONS, 3);
}

#[test]
fn reasons_come_from_the_highest_contributing_components() {
    // Roles (30), traits (25) and situation (20) outrank interests (15) and
    // geography (10), so those three are what a card should say.
    let (viewer, candidate) = perfect_pair();

    let components: Vec<Component> = score(&viewer, &candidate)
        .reasons
        .iter()
        .map(|reason| reason.component)
        .collect();

    assert_eq!(
        components,
        vec![Component::Roles, Component::Traits, Component::Situation]
    );
}

#[test]
fn a_component_that_scored_nothing_never_explains_itself() {
    // Nothing in common except a shared city: geography is the only
    // component with anything to say.
    let viewer = ScoredProfile {
        city: "Jakarta".into(),
        country: "Indonesia".into(),
        ..blank()
    };
    let candidate = ScoredProfile {
        city: "Jakarta".into(),
        country: "Indonesia".into(),
        ..blank()
    };

    let result = score(&viewer, &candidate);

    assert!(result
        .reasons
        .iter()
        .all(|reason| reason.component != Component::Interests));
    assert!(result
        .reasons
        .iter()
        .any(|reason| reason.component == Component::Geography));
}

#[test]
fn every_surfaced_reason_has_text() {
    let (viewer, candidate) = perfect_pair();

    for reason in score(&viewer, &candidate).reasons {
        assert!(!reason.text.trim().is_empty(), "{reason:?}");
    }
}

#[test]
fn scoring_is_deterministic() {
    let (viewer, candidate) = perfect_pair();

    assert_eq!(score(&viewer, &candidate), score(&viewer, &candidate));
}
