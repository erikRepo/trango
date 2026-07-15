//! Formats a playback time in seconds as a clock string for the scrub bar's
//! time labels (`MM:SS`, or `H:MM:SS` once the hour mark is reached).

/// Formats `seconds` as `MM:SS`, or `H:MM:SS` if `seconds` reaches one hour.
/// Negative or non-finite input (e.g. mpv's `time-pos`/`duration` before a
/// video has started reporting them) is clamped to `00:00`.
pub fn format_time(seconds: f64) -> String {
    let total_seconds = if seconds.is_finite() && seconds > 0.0 {
        seconds.floor() as u64
    } else {
        0
    };

    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let secs = total_seconds % 60;

    if hours > 0 {
        format!("{hours}:{minutes:02}:{secs:02}")
    } else {
        format!("{minutes:02}:{secs:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_time_zero() {
        // Given: zero seconds
        // When:  formatting
        // Then:  "00:00"
        assert_eq!(format_time(0.0), "00:00");
    }

    #[test]
    fn test_format_time_matches_mock_values() {
        // Given: the two clock values shown in sketch/design_reference.dc.html's scrub bar
        // When:  formatting 134s and 492s
        // Then:  they match the mock's "02:14" and "08:12"
        assert_eq!(format_time(134.0), "02:14");
        assert_eq!(format_time(492.0), "08:12");
    }

    #[test]
    fn test_format_time_truncates_fractional_seconds() {
        // Given: a sub-second time-pos value, as mpv reports
        // When:  formatting
        // Then:  it truncates rather than rounds
        assert_eq!(format_time(59.9), "00:59");
    }

    #[test]
    fn test_format_time_rolls_over_to_hours() {
        // Given: a duration past one hour
        // When:  formatting
        // Then:  an "H:MM:SS" form is used instead of overflowing minutes
        assert_eq!(format_time(3_661.0), "1:01:01");
    }

    #[test]
    fn test_format_time_clamps_negative_and_non_finite() {
        // Given: values mpv can report before it knows real playback time
        // When:  formatting -1.0 and NaN
        // Then:  both clamp to "00:00" instead of panicking or underflowing
        assert_eq!(format_time(-1.0), "00:00");
        assert_eq!(format_time(f64::NAN), "00:00");
    }
}
