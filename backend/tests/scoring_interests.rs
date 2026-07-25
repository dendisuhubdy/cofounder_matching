use cofounder_api::assessment::scoring::TraitScores;
use cofounder_api::scoring::interests::{score_interests, MAX_POINTS};
use cofounder_api::scoring::profile::ScoredProfile;
use uuid::Uuid;

fn profile(interests: &[&str]) -> ScoredProfile {
    ScoredProfile {
        user_id: Uuid::new_v4(),
        display_name: "Someone".into(),
        roles: Vec::new(),
        seeking_roles: Vec::new(),
        interests: interests.iter().map(|i| i.to_string()).collect(),
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

#[test]
fn identical_interests_score_the_whole_budget() {
    let viewer = profile(&["ai_ml", "saas"]);
    let candidate = profile(&["ai_ml", "saas"]);

    let result = score_interests(&viewer, &candidate);

    assert!(
        (result.points - MAX_POINTS).abs() < 0.001,
        "got {}",
        result.points
    );
}

#[test]
fn jaccard_similarity_is_used() {
    // One shared out of two distinct: 1/2 of the budget.
    let viewer = profile(&["ai_ml", "saas"]);
    let candidate = profile(&["ai_ml"]);

    let result = score_interests(&viewer, &candidate);

    assert!(
        (result.points - MAX_POINTS / 2.0).abs() < 0.001,
        "got {}",
        result.points
    );
}

#[test]
fn disjoint_interests_score_nothing() {
    let viewer = profile(&["ai_ml"]);
    let candidate = profile(&["climate"]);

    let result = score_interests(&viewer, &candidate);

    assert_eq!(result.points, 0.0);
    assert!(result.reason.is_none());
}

#[test]
fn two_people_with_no_interests_score_nothing_rather_than_everything() {
    // An empty union must not be read as a perfect overlap.
    let viewer = profile(&[]);
    let candidate = profile(&[]);

    let result = score_interests(&viewer, &candidate);

    assert_eq!(result.points, 0.0);
    assert!(result.reason.is_none());
}

#[test]
fn scoring_is_symmetric() {
    let a = profile(&["ai_ml", "saas", "fintech"]);
    let b = profile(&["saas"]);

    let forwards = score_interests(&a, &b).points;
    let backwards = score_interests(&b, &a).points;

    assert!((forwards - backwards).abs() < 0.001);
}

#[test]
fn a_shared_interest_is_named_by_its_label() {
    let viewer = profile(&["ai_ml"]);
    let candidate = profile(&["ai_ml"]);

    let reason = score_interests(&viewer, &candidate).reason.expect("a reason");

    assert!(reason.contains("AI / ML"), "got {reason}");
}
