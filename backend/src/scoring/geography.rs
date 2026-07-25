use crate::scoring::profile::{Component, ComponentScore, ScoredProfile};

pub const MAX_POINTS: f64 = 10.0;

/// Three hours either side, in minutes. Beyond this a working day does not
/// meaningfully overlap.
pub const MAX_PARTIAL_OFFSET_MINUTES: i16 = 180;

const PARTIAL_POINTS: f64 = 5.0;

fn normalize(value: &str) -> String {
    value.trim().to_lowercase()
}

/// Blank counts as unknown, never as a match: otherwise every profile that
/// skipped the location fields shares a city with every other one.
fn same_metro(viewer: &ScoredProfile, candidate: &ScoredProfile) -> bool {
    let city = normalize(&viewer.city);
    let country = normalize(&viewer.country);

    !city.is_empty()
        && !country.is_empty()
        && city == normalize(&candidate.city)
        && country == normalize(&candidate.country)
}

pub fn score_geography(viewer: &ScoredProfile, candidate: &ScoredProfile) -> ComponentScore {
    if same_metro(viewer, candidate) {
        let city = viewer.city.trim().to_string();
        return ComponentScore::new(
            Component::Geography,
            MAX_POINTS,
            Some(format!("Both in {city}")),
        );
    }

    let (Some(mine), Some(theirs)) = (viewer.utc_offset_minutes, candidate.utc_offset_minutes)
    else {
        return ComponentScore::empty(Component::Geography);
    };

    let apart = (i32::from(mine) - i32::from(theirs)).abs();
    if apart > i32::from(MAX_PARTIAL_OFFSET_MINUTES) {
        return ComponentScore::empty(Component::Geography);
    }

    let hours = (apart as f64 / 60.0).round() as i32;
    let reason = if hours == 0 {
        Some("In the same timezone as you".to_string())
    } else {
        Some(format!(
            "Within {hours} hour{} of you",
            if hours == 1 { "" } else { "s" }
        ))
    };

    ComponentScore::new(Component::Geography, PARTIAL_POINTS, reason)
}
