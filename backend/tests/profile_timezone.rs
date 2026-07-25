use cofounder_api::profiles::timezone::utc_offset_minutes;

#[test]
fn a_zone_east_of_utc_has_a_positive_offset() {
    assert_eq!(utc_offset_minutes("Asia/Jakarta"), Some(420));
}

#[test]
fn a_zone_west_of_utc_has_a_negative_offset() {
    assert_eq!(utc_offset_minutes("America/New_York"), Some(-300));
}

#[test]
fn utc_itself_is_zero() {
    assert_eq!(utc_offset_minutes("UTC"), Some(0));
}

#[test]
fn a_half_hour_zone_is_handled() {
    // India is UTC+5:30. An hours-only implementation loses the thirty.
    assert_eq!(utc_offset_minutes("Asia/Kolkata"), Some(330));
}

#[test]
fn resolution_is_against_a_fixed_instant_not_today() {
    // London is UTC+0 in January and UTC+1 in July. Resolving against a
    // fixed reference is what keeps the scorer's output stable year-round.
    assert_eq!(utc_offset_minutes("Europe/London"), Some(0));
}

#[test]
fn surrounding_whitespace_is_tolerated() {
    assert_eq!(utc_offset_minutes("  Asia/Jakarta  "), Some(420));
}

#[test]
fn an_unknown_zone_has_no_offset() {
    assert_eq!(utc_offset_minutes("Mars/Olympus_Mons"), None);
}

#[test]
fn a_blank_zone_has_no_offset() {
    assert_eq!(utc_offset_minutes(""), None);
    assert_eq!(utc_offset_minutes("   "), None);
}
