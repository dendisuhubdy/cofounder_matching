use cofounder_api::assessment::scoring::TraitScores;
use cofounder_api::scoring::geography::{score_geography, MAX_PARTIAL_OFFSET_MINUTES, MAX_POINTS};
use cofounder_api::scoring::profile::ScoredProfile;
use uuid::Uuid;

fn profile(city: &str, country: &str, offset: Option<i16>) -> ScoredProfile {
    ScoredProfile {
        user_id: Uuid::new_v4(),
        display_name: "Someone".into(),
        roles: Vec::new(),
        seeking_roles: Vec::new(),
        interests: Vec::new(),
        idea_status: None,
        stage: None,
        commitment: None,
        city: city.into(),
        country: country.into(),
        utc_offset_minutes: offset,
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
fn the_same_metro_scores_the_whole_budget() {
    let viewer = profile("Jakarta", "Indonesia", Some(420));
    let candidate = profile("Jakarta", "Indonesia", Some(420));

    let result = score_geography(&viewer, &candidate);

    assert!(
        (result.points - MAX_POINTS).abs() < 0.001,
        "got {}",
        result.points
    );
}

#[test]
fn the_same_metro_is_matched_case_insensitively() {
    let viewer = profile("Jakarta", "Indonesia", Some(420));
    let candidate = profile("  jakarta ", "INDONESIA", Some(420));

    let result = score_geography(&viewer, &candidate);

    assert!((result.points - MAX_POINTS).abs() < 0.001);
}

#[test]
fn the_same_city_name_in_another_country_is_not_the_same_metro() {
    let viewer = profile("Cambridge", "United Kingdom", Some(0));
    let candidate = profile("Cambridge", "United States", Some(-300));

    let result = score_geography(&viewer, &candidate);

    assert!(result.points < MAX_POINTS, "got {}", result.points);
}

#[test]
fn nearby_timezones_score_partially() {
    let viewer = profile("Jakarta", "Indonesia", Some(420));
    let candidate = profile("Singapore", "Singapore", Some(480));

    let result = score_geography(&viewer, &candidate);

    assert!(result.points > 0.0, "got {}", result.points);
    assert!(result.points < MAX_POINTS, "got {}", result.points);
}

#[test]
fn the_edge_of_the_band_still_counts() {
    let viewer = profile("London", "United Kingdom", Some(0));
    let candidate = profile("Dubai", "UAE", Some(MAX_PARTIAL_OFFSET_MINUTES));

    assert!(score_geography(&viewer, &candidate).points > 0.0);
}

#[test]
fn beyond_the_band_scores_nothing() {
    let viewer = profile("London", "United Kingdom", Some(0));
    let candidate = profile(
        "San Francisco",
        "United States",
        Some(MAX_PARTIAL_OFFSET_MINUTES + 1),
    );

    let result = score_geography(&viewer, &candidate);

    assert_eq!(result.points, 0.0);
    assert!(result.reason.is_none());
}

#[test]
fn an_unknown_offset_scores_nothing_rather_than_guessing() {
    let viewer = profile("London", "United Kingdom", None);
    let candidate = profile("Paris", "France", Some(60));

    assert_eq!(score_geography(&viewer, &candidate).points, 0.0);
}

#[test]
fn two_blank_locations_are_not_the_same_metro() {
    // Every profile that skipped the location fields would otherwise be in
    // the same city as every other.
    let viewer = profile("", "", Some(0));
    let candidate = profile("", "", Some(0));

    let result = score_geography(&viewer, &candidate);

    assert!(result.points < MAX_POINTS, "got {}", result.points);
}

#[test]
fn the_shared_city_is_named() {
    let viewer = profile("Jakarta", "Indonesia", Some(420));
    let candidate = profile("Jakarta", "Indonesia", Some(420));

    let reason = score_geography(&viewer, &candidate).reason.expect("a reason");

    assert!(reason.contains("Jakarta"), "got {reason}");
}
