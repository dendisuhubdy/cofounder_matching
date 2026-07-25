use chrono::{Offset, TimeZone, Utc};
use chrono_tz::Tz;

/// Offsets are resolved against the first of January rather than against
/// today. An IANA zone's offset moves with daylight saving, so resolving at
/// score time would make the same pair score differently in March than in
/// July. The cost is that southern-hemisphere zones are recorded at their
/// summer-time offset; that is a consistent hour, not a drifting one.
pub const REFERENCE_YEAR: i32 = 2024;

pub fn utc_offset_minutes(name: &str) -> Option<i16> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return None;
    }

    let zone: Tz = trimmed.parse().ok()?;
    let reference = Utc
        .with_ymd_and_hms(REFERENCE_YEAR, 1, 1, 0, 0, 0)
        .single()?;

    let seconds = zone
        .offset_from_utc_datetime(&reference.naive_utc())
        .fix()
        .local_minus_utc();

    i16::try_from(seconds / 60).ok()
}
