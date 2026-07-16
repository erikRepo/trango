//! Maps the playback-speed slider's drag fraction to an actual mpv `speed`
//! value, and formats that speed for display. Mirrors `time_format.rs`'s
//! shape: pure, testable math with no mpv/UI dependency — the mpv `speed`
//! property itself is set by `crates/app/src/video_player.rs`.

/// Slowest speed the slider allows — half of normal playback.
pub const MIN_SPEED: f64 = 0.5;
/// Fastest speed the slider allows — normal playback. The slider only
/// slows video down, for language-learning use (see SPEC.md); it never
/// speeds it up past 1.0.
pub const MAX_SPEED: f64 = 1.0;
/// Increment `speed_from_fraction` snaps to, giving a small, predictable
/// set of reachable speeds (0.50, 0.55, ..., 1.00) rather than every
/// possible float across the drag range.
pub const SPEED_STEP: f64 = 0.05;

/// Maps a slider drag `fraction` (0.0-1.0 across the track) to an mpv
/// `speed` value between [`MIN_SPEED`] and [`MAX_SPEED`], snapped to the
/// nearest [`SPEED_STEP`] increment. `fraction` isn't assumed pre-clamped —
/// mirroring `video_player.rs`'s `seek_target_secs` — since a drag beyond
/// the track's own edges reports values outside 0.0-1.0.
pub fn speed_from_fraction(fraction: f32) -> f64 {
    let fraction = f64::from(fraction.clamp(0.0, 1.0));
    let raw = MIN_SPEED + fraction * (MAX_SPEED - MIN_SPEED);
    let steps = ((raw - MIN_SPEED) / SPEED_STEP).round();
    (MIN_SPEED + steps * SPEED_STEP).clamp(MIN_SPEED, MAX_SPEED)
}

/// Formats `speed` for display next to the slider, e.g. `"0.75x"`.
pub fn format_speed_label(speed: f64) -> String {
    format!("{speed:.2}x")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_speed_from_fraction_endpoints() {
        // Given: the track's two extremes
        // When:  mapping them to a speed
        // Then:  0.0 is the slowest speed, 1.0 is normal speed
        assert_eq!(speed_from_fraction(0.0), MIN_SPEED);
        assert_eq!(speed_from_fraction(1.0), MAX_SPEED);
    }

    #[test]
    fn test_speed_from_fraction_midpoint_matches_075_marker() {
        // Given: the track's midpoint, where the "0.75x" marker sits
        // When:  mapping it to a speed
        // Then:  it lands exactly on 0.75
        assert_eq!(speed_from_fraction(0.5), 0.75);
    }

    #[test]
    fn test_speed_from_fraction_snaps_to_step() {
        // Given: a fraction that doesn't land on an exact 0.05 increment
        // When:  mapping it to a speed
        // Then:  it snaps to the nearest reachable step
        assert_eq!(speed_from_fraction(0.51), 0.75);
        assert_eq!(speed_from_fraction(0.59), 0.80);
    }

    #[test]
    fn test_speed_from_fraction_clamps_out_of_range() {
        // Given: a drag that overshoots the track's own edges
        // When:  mapping it to a speed
        // Then:  it clamps to MIN_SPEED/MAX_SPEED instead of extrapolating
        //        past them
        assert_eq!(speed_from_fraction(-0.3), MIN_SPEED);
        assert_eq!(speed_from_fraction(1.4), MAX_SPEED);
    }

    #[test]
    fn test_format_speed_label() {
        // Given: a few representative speeds
        // When:  formatting them for display
        // Then:  always two decimal places plus a trailing "x"
        assert_eq!(format_speed_label(1.0), "1.00x");
        assert_eq!(format_speed_label(0.75), "0.75x");
        assert_eq!(format_speed_label(0.5), "0.50x");
    }
}
