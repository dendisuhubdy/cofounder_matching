use std::collections::HashSet;

use cofounder_api::assessment::questions::{find, Axis, QUESTIONS};

#[test]
fn there_are_eighteen_questions() {
    assert_eq!(QUESTIONS.len(), 18);
}

#[test]
fn every_axis_has_exactly_three_questions() {
    for axis in Axis::ALL {
        let count = QUESTIONS.iter().filter(|q| q.axis == axis).count();
        assert_eq!(count, 3, "axis {} has {} questions", axis.slug(), count);
    }
}

#[test]
fn every_axis_has_at_least_one_reverse_item() {
    // Without a reverse item, answering straight down the page produces a
    // coherent-looking profile that means nothing.
    for axis in Axis::ALL {
        let reversed = QUESTIONS
            .iter()
            .filter(|q| q.axis == axis && q.reverse)
            .count();
        assert!(reversed >= 1, "axis {} has no reverse item", axis.slug());
    }
}

#[test]
fn question_ids_are_unique() {
    let unique: HashSet<&str> = QUESTIONS.iter().map(|q| q.id).collect();
    assert_eq!(unique.len(), QUESTIONS.len());
}

#[test]
fn every_question_has_text() {
    for question in QUESTIONS.iter() {
        assert!(
            !question.text.trim().is_empty(),
            "{} has no text",
            question.id
        );
    }
}

#[test]
fn axis_slugs_are_unique_and_snake_case() {
    let unique: HashSet<&str> = Axis::ALL.iter().map(|a| a.slug()).collect();
    assert_eq!(unique.len(), 6);
    for axis in Axis::ALL {
        assert!(axis
            .slug()
            .chars()
            .all(|c| c.is_ascii_lowercase() || c == '_'));
    }
}

#[test]
fn find_locates_a_question_by_id() {
    let question = find("risk_1").expect("risk_1 should exist");
    assert_eq!(question.axis, Axis::RiskTolerance);
}

#[test]
fn find_rejects_an_unknown_id() {
    assert!(find("not_a_question").is_none());
}
