use std::collections::HashMap;

use cofounder_api::assessment::questions::QUESTIONS;
use cofounder_api::assessment::scoring::compute;

/// Every question answered with the same value.
fn uniform(value: i16) -> HashMap<String, i16> {
    QUESTIONS
        .iter()
        .map(|q| (q.id.to_string(), value))
        .collect()
}

#[test]
fn the_midpoint_answer_scores_fifty_on_every_axis() {
    let scores = compute(&uniform(3)).expect("all 18 answered");

    assert_eq!(scores.risk_tolerance, 50);
    assert_eq!(scores.pace_vs_rigor, 50);
    assert_eq!(scores.conflict_style, 50);
    assert_eq!(scores.decision_basis, 50);
    assert_eq!(scores.work_mode, 50);
    assert_eq!(scores.orientation, 50);
}

#[test]
fn answering_uniformly_does_not_produce_an_extreme_profile() {
    // This is the whole point of reverse items. Agreeing with everything must
    // not read as "maximum on every axis".
    let high = compute(&uniform(5)).expect("all 18 answered");

    assert_ne!(high.risk_tolerance, 100);
    assert_ne!(high.pace_vs_rigor, 100);
    assert_ne!(high.work_mode, 100);
}

#[test]
fn a_reverse_item_is_flipped_before_averaging() {
    // risk_2 is the reverse item on the risk axis. Answering 5 to the two
    // forward items and 1 to the reverse one is maximal risk tolerance.
    let mut answers = uniform(3);
    answers.insert("risk_1".into(), 5);
    answers.insert("risk_2".into(), 1);
    answers.insert("risk_3".into(), 5);

    let scores = compute(&answers).expect("all 18 answered");
    assert_eq!(scores.risk_tolerance, 100);
}

#[test]
fn the_opposite_answers_score_zero() {
    let mut answers = uniform(3);
    answers.insert("risk_1".into(), 1);
    answers.insert("risk_2".into(), 5);
    answers.insert("risk_3".into(), 1);

    let scores = compute(&answers).expect("all 18 answered");
    assert_eq!(scores.risk_tolerance, 0);
}

#[test]
fn an_axis_mean_is_mapped_onto_zero_to_one_hundred() {
    // work_1 is the reverse item. 4, 2 (-> 4), 5 gives a mean of 13/3 = 4.333,
    // which maps to (4.333 - 1) / 4 * 100 = 83.33, rounded to 83.
    let mut answers = uniform(3);
    answers.insert("work_1".into(), 2);
    answers.insert("work_2".into(), 4);
    answers.insert("work_3".into(), 5);

    let scores = compute(&answers).expect("all 18 answered");
    assert_eq!(scores.work_mode, 83);
}

#[test]
fn scoring_returns_none_when_any_answer_is_missing() {
    let mut answers = uniform(3);
    answers.remove("orientation_3");

    assert!(compute(&answers).is_none());
}

#[test]
fn scoring_returns_none_for_no_answers_at_all() {
    assert!(compute(&HashMap::new()).is_none());
}

#[test]
fn unknown_answer_keys_are_ignored() {
    let mut answers = uniform(3);
    answers.insert("a_question_we_deleted".into(), 5);

    let scores = compute(&answers).expect("all 18 real questions answered");
    assert_eq!(scores.risk_tolerance, 50);
}
