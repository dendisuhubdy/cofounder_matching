use cofounder_api::assessment::scoring::TraitScores;
use cofounder_api::scoring::profile::ScoredProfile;
use cofounder_api::scoring::situation::{score_situation, MAX_POINTS};
use uuid::Uuid;

fn profile(commitment: Option<&str>, stage: Option<&str>) -> ScoredProfile {
    ScoredProfile {
        user_id: Uuid::new_v4(),
        display_name: "Someone".into(),
        roles: Vec::new(),
        seeking_roles: Vec::new(),
        interests: Vec::new(),
        idea_status: None,
        stage: stage.map(str::to_string),
        commitment: commitment.map(str::to_string),
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
fn an_identical_situation_scores_the_whole_budget() {
    let viewer = profile(Some("full_time_now"), Some("prototype"));
    let candidate = profile(Some("full_time_now"), Some("prototype"));

    let result = score_situation(&viewer, &candidate);

    assert!(
        (result.points - MAX_POINTS).abs() < 0.001,
        "got {}",
        result.points
    );
}

#[test]
fn adjacent_commitment_levels_score_most_of_it() {
    let now = profile(Some("full_time_now"), Some("prototype"));
    let when_funded = profile(Some("full_time_when_funded"), Some("prototype"));

    let adjacent = score_situation(&now, &when_funded).points;
    let identical = score_situation(&now, &now).points;

    assert!(adjacent < identical, "{adjacent} should be under {identical}");
    assert!(adjacent > identical * 0.7, "{adjacent} is too harsh");
}

#[test]
fn distant_commitment_levels_score_near_zero_on_that_part() {
    // Full-time now against exploring is the classic doomed pairing.
    let now = profile(Some("full_time_now"), Some("prototype"));
    let exploring = profile(Some("exploring"), Some("prototype"));

    let result = score_situation(&now, &exploring);

    // The stage half still scores; the commitment half does not.
    assert!(result.points < MAX_POINTS / 2.0, "got {}", result.points);
    assert!(result.points > 0.0, "got {}", result.points);
}

#[test]
fn a_mismatch_is_penalised_never_filtered() {
    // Even the worst situation returns a score rather than an absence, so a
    // strong fit elsewhere can still outweigh it.
    let now = profile(Some("full_time_now"), Some("idea"));
    let exploring = profile(Some("exploring"), Some("revenue"));

    let result = score_situation(&now, &exploring);

    assert!(result.points >= 0.0);
    assert!(result.points < MAX_POINTS);
}

#[test]
fn commitment_counts_for_more_than_stage() {
    let base = profile(Some("full_time_now"), Some("idea"));
    let stage_differs = profile(Some("full_time_now"), Some("revenue"));
    let commitment_differs = profile(Some("exploring"), Some("idea"));

    let losing_stage = score_situation(&base, &stage_differs).points;
    let losing_commitment = score_situation(&base, &commitment_differs).points;

    assert!(
        losing_commitment < losing_stage,
        "commitment {losing_commitment} should cost more than stage {losing_stage}"
    );
}

#[test]
fn an_unset_commitment_scores_nothing_for_that_half() {
    let viewer = profile(None, Some("prototype"));
    let candidate = profile(Some("full_time_now"), Some("prototype"));

    let result = score_situation(&viewer, &candidate);

    assert!(result.points < MAX_POINTS, "got {}", result.points);
}

#[test]
fn an_identical_commitment_is_explained() {
    let viewer = profile(Some("full_time_now"), Some("prototype"));
    let candidate = profile(Some("full_time_now"), Some("prototype"));

    let reason = score_situation(&viewer, &candidate).reason.expect("a reason");

    assert!(reason.contains("Full-time now"), "got {reason}");
}

#[test]
fn a_distant_pair_is_not_explained() {
    let viewer = profile(Some("full_time_now"), Some("idea"));
    let candidate = profile(Some("exploring"), Some("revenue"));

    assert!(score_situation(&viewer, &candidate).reason.is_none());
}
