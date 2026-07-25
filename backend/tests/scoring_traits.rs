use cofounder_api::assessment::questions::Axis;
use cofounder_api::assessment::scoring::TraitScores;
use cofounder_api::scoring::profile::ScoredProfile;
use cofounder_api::scoring::traits::{ideal_for, score_traits, Ideal, MAX_POINTS};
use uuid::Uuid;

fn with_traits(traits: TraitScores) -> ScoredProfile {
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
        traits,
    }
}

fn uniform(value: i16) -> TraitScores {
    TraitScores {
        risk_tolerance: value,
        pace_vs_rigor: value,
        conflict_style: value,
        decision_basis: value,
        work_mode: value,
        orientation: value,
    }
}

// The table straight out of the design document. If an axis is ever flipped,
// this is the test that says so.
#[test]
fn each_axis_wants_what_the_design_says_it_wants() {
    assert_eq!(ideal_for(Axis::RiskTolerance), Ideal::Similar);
    assert_eq!(ideal_for(Axis::PaceVsRigor), Ideal::Similar);
    assert_eq!(ideal_for(Axis::ConflictStyle), Ideal::Similar);
    assert_eq!(ideal_for(Axis::DecisionBasis), Ideal::MildDifference);
    assert_eq!(ideal_for(Axis::WorkMode), Ideal::Complementary);
    assert_eq!(ideal_for(Axis::Orientation), Ideal::Complementary);
}

#[test]
fn the_ideal_distances_are_ordered() {
    assert!(Ideal::Similar.target_distance() < Ideal::MildDifference.target_distance());
    assert!(Ideal::MildDifference.target_distance() < Ideal::Complementary.target_distance());
}

#[test]
fn a_pair_sitting_exactly_on_every_ideal_scores_the_whole_budget() {
    // Similar axes identical; decision_basis 25 apart; the two complementary
    // axes 60 apart.
    let viewer = with_traits(TraitScores {
        risk_tolerance: 50,
        pace_vs_rigor: 50,
        conflict_style: 50,
        decision_basis: 40,
        work_mode: 20,
        orientation: 20,
    });
    let candidate = with_traits(TraitScores {
        risk_tolerance: 50,
        pace_vs_rigor: 50,
        conflict_style: 50,
        decision_basis: 65,
        work_mode: 80,
        orientation: 80,
    });

    let result = score_traits(&viewer, &candidate);

    assert!(
        (result.points - MAX_POINTS).abs() < 0.001,
        "got {}",
        result.points
    );
}

#[test]
fn identical_people_do_not_score_full_marks() {
    // Two identical profiles are perfect on the similar axes and poor on the
    // complementary ones. If this ever reaches the maximum, the
    // complementary axes have been treated as similarity axes.
    let viewer = with_traits(uniform(50));
    let candidate = with_traits(uniform(50));

    let result = score_traits(&viewer, &candidate);

    assert!(result.points < MAX_POINTS, "got {}", result.points);
    assert!(result.points > 0.0, "got {}", result.points);
}

#[test]
fn opposites_beat_twins_on_a_complementary_axis() {
    let twin_a = with_traits(TraitScores {
        work_mode: 50,
        ..uniform(50)
    });
    let twin_b = with_traits(TraitScores {
        work_mode: 50,
        ..uniform(50)
    });
    let deep = with_traits(TraitScores {
        work_mode: 20,
        ..uniform(50)
    });
    let social = with_traits(TraitScores {
        work_mode: 80,
        ..uniform(50)
    });

    let twins = score_traits(&twin_a, &twin_b).points;
    let opposites = score_traits(&deep, &social).points;

    assert!(
        opposites > twins,
        "complementary axis: opposites {opposites} should beat twins {twins}"
    );
}

#[test]
fn twins_beat_opposites_on_a_similarity_axis() {
    let twin_a = with_traits(uniform(50));
    let twin_b = with_traits(uniform(50));
    let cautious = with_traits(TraitScores {
        risk_tolerance: 10,
        ..uniform(50)
    });
    let bold = with_traits(TraitScores {
        risk_tolerance: 90,
        ..uniform(50)
    });

    let twins = score_traits(&twin_a, &twin_b).points;
    let opposites = score_traits(&cautious, &bold).points;

    assert!(
        twins > opposites,
        "similarity axis: twins {twins} should beat opposites {opposites}"
    );
}

#[test]
fn scoring_is_symmetric() {
    let a = with_traits(TraitScores {
        risk_tolerance: 30,
        pace_vs_rigor: 70,
        conflict_style: 10,
        decision_basis: 90,
        work_mode: 25,
        orientation: 80,
    });
    let b = with_traits(TraitScores {
        risk_tolerance: 45,
        pace_vs_rigor: 55,
        conflict_style: 60,
        decision_basis: 20,
        work_mode: 75,
        orientation: 15,
    });

    let forwards = score_traits(&a, &b).points;
    let backwards = score_traits(&b, &a).points;

    assert!((forwards - backwards).abs() < 0.001);
}

#[test]
fn the_worst_possible_pair_scores_nothing_on_the_similar_axes() {
    let cautious = with_traits(uniform(0));
    let bold = with_traits(uniform(100));

    let result = score_traits(&cautious, &bold);

    // Every similar axis is 100 apart, so those three contribute zero; the
    // complementary axes are overshot but not by the full range.
    assert!(result.points < MAX_POINTS / 2.0, "got {}", result.points);
}

#[test]
fn a_similar_axis_is_explained_by_what_the_pair_shares() {
    // The card explains whichever axis fits best, so every other axis is
    // deliberately made a poor fit to leave pace_vs_rigor the clear winner.
    let ship_fast_a = with_traits(TraitScores {
        risk_tolerance: 0,
        pace_vs_rigor: 90,
        conflict_style: 0,
        decision_basis: 0,
        work_mode: 50,
        orientation: 50,
    });
    let ship_fast_b = with_traits(TraitScores {
        risk_tolerance: 100,
        pace_vs_rigor: 88,
        conflict_style: 100,
        decision_basis: 100,
        work_mode: 50,
        orientation: 50,
    });

    let reason = score_traits(&ship_fast_a, &ship_fast_b)
        .reason
        .expect("a reason");

    assert!(reason.starts_with("Both "), "got {reason}");
    assert!(reason.contains("ship it now"), "got {reason}");
}

#[test]
fn a_complementary_axis_is_explained_as_a_pairing() {
    // As above: work_mode has to be the best-fitting axis for the card to
    // choose it.
    let deep = with_traits(TraitScores {
        risk_tolerance: 0,
        pace_vs_rigor: 0,
        conflict_style: 0,
        decision_basis: 0,
        work_mode: 15,
        orientation: 50,
    });
    let social = with_traits(TraitScores {
        risk_tolerance: 100,
        pace_vs_rigor: 100,
        conflict_style: 100,
        decision_basis: 100,
        work_mode: 85,
        orientation: 50,
    });

    let reason = score_traits(&deep, &social).reason.expect("a reason");

    assert!(reason.contains("deep solo work"), "got {reason}");
    assert!(reason.contains("constant collaboration"), "got {reason}");
}

#[test]
fn every_axis_has_labels_for_both_ends() {
    for axis in Axis::ALL {
        assert!(!axis.low_label().is_empty(), "{}", axis.slug());
        assert!(!axis.high_label().is_empty(), "{}", axis.slug());
    }
}
