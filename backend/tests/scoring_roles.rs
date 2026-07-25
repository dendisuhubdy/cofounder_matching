use cofounder_api::assessment::scoring::TraitScores;
use cofounder_api::scoring::profile::{Component, ScoredProfile};
use cofounder_api::scoring::roles::score_roles;
use uuid::Uuid;

pub fn neutral_traits() -> TraitScores {
    TraitScores {
        risk_tolerance: 50,
        pace_vs_rigor: 50,
        conflict_style: 50,
        decision_basis: 50,
        work_mode: 50,
        orientation: 50,
    }
}

fn profile(roles: &[&str], seeking: &[&str]) -> ScoredProfile {
    ScoredProfile {
        user_id: Uuid::new_v4(),
        display_name: "Someone".into(),
        roles: roles.iter().map(|r| r.to_string()).collect(),
        seeking_roles: seeking.iter().map(|r| r.to_string()).collect(),
        interests: Vec::new(),
        idea_status: None,
        stage: None,
        commitment: None,
        city: String::new(),
        country: String::new(),
        utc_offset_minutes: None,
        traits: neutral_traits(),
    }
}

#[test]
fn a_mutual_fit_scores_the_whole_budget() {
    // The archetypal good result: a builder who wants a seller, matched with
    // a seller who wants a builder.
    let viewer = profile(&["engineering"], &["gtm"]);
    let candidate = profile(&["gtm"], &["engineering"]);

    let result = score_roles(&viewer, &candidate);

    assert_eq!(result.component, Component::Roles);
    assert!((result.points - 30.0).abs() < 0.001, "got {}", result.points);
}

#[test]
fn a_one_sided_fit_scores_half() {
    // They have what the viewer wants, but do not want what the viewer has.
    let viewer = profile(&["engineering"], &["gtm"]);
    let candidate = profile(&["gtm"], &["design"]);

    let result = score_roles(&viewer, &candidate);

    assert!((result.points - 15.0).abs() < 0.001, "got {}", result.points);
}

#[test]
fn no_overlap_scores_nothing() {
    let viewer = profile(&["engineering"], &["gtm"]);
    let candidate = profile(&["research"], &["design"]);

    let result = score_roles(&viewer, &candidate);

    assert_eq!(result.points, 0.0);
    assert!(result.reason.is_none());
}

#[test]
fn covering_more_of_what_is_wanted_scores_higher() {
    let viewer = profile(&["engineering"], &["gtm", "design"]);
    let covers_one = profile(&["gtm"], &[]);
    let covers_both = profile(&["gtm", "design"], &[]);

    let partial = score_roles(&viewer, &covers_one).points;
    let full = score_roles(&viewer, &covers_both).points;

    assert!((partial - 7.5).abs() < 0.001, "got {partial}");
    assert!((full - 15.0).abs() < 0.001, "got {full}");
}

#[test]
fn seeking_nothing_earns_nothing_from_that_direction() {
    // An empty wishlist cannot be satisfied; it must not divide by zero.
    let viewer = profile(&["engineering"], &[]);
    let candidate = profile(&["gtm"], &["engineering"]);

    let result = score_roles(&viewer, &candidate);

    assert!((result.points - 15.0).abs() < 0.001, "got {}", result.points);
}

#[test]
fn a_mutual_fit_explains_both_directions() {
    let viewer = profile(&["engineering"], &["gtm"]);
    let candidate = profile(&["gtm"], &["engineering"]);

    let reason = score_roles(&viewer, &candidate).reason.expect("a reason");

    assert!(reason.contains("GTM / Sales"), "got {reason}");
    assert!(reason.contains("Engineering"), "got {reason}");
}

#[test]
fn a_one_sided_fit_explains_only_that_direction() {
    let viewer = profile(&["engineering"], &["gtm"]);
    let candidate = profile(&["gtm"], &["design"]);

    let reason = score_roles(&viewer, &candidate).reason.expect("a reason");

    assert!(reason.contains("GTM / Sales"), "got {reason}");
}
