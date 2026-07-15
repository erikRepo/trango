//! The `Cue` data model: a single subtitle entry with timing and text.

use std::time::Duration;

use crate::error::SubtitleError;

/// A single subtitle cue: an index, a visible time range, and its text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cue {
    /// 1-based position of this cue within its subtitle track.
    pub index: u32,
    /// Time at which the cue becomes visible.
    pub start: Duration,
    /// Time at which the cue disappears.
    pub end: Duration,
    /// The subtitle text shown during `[start, end)`.
    pub text: String,
}

impl Cue {
    /// Builds a new `Cue`, validating that `start` is strictly before `end`.
    pub fn new(
        index: u32,
        start: Duration,
        end: Duration,
        text: impl Into<String>,
    ) -> Result<Self, SubtitleError> {
        if start >= end {
            return Err(SubtitleError::InvalidTiming { index, start, end });
        }
        Ok(Cue {
            index,
            start,
            end,
            text: text.into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_accepts_valid_timing() {
        // Given: a start time strictly before the end time
        // When:  constructing a Cue
        // Then:  it succeeds and stores the given fields
        let cue = Cue::new(1, Duration::from_secs(1), Duration::from_secs(2), "hello").unwrap();
        assert_eq!(cue.index, 1);
        assert_eq!(cue.start, Duration::from_secs(1));
        assert_eq!(cue.end, Duration::from_secs(2));
        assert_eq!(cue.text, "hello");
    }

    #[test]
    fn test_new_rejects_equal_start_and_end() {
        // Given: start == end
        // When:  constructing a Cue
        // Then:  it returns SubtitleError::InvalidTiming
        let result = Cue::new(1, Duration::from_secs(1), Duration::from_secs(1), "x");
        assert!(matches!(result, Err(SubtitleError::InvalidTiming { .. })));
    }

    #[test]
    fn test_new_rejects_end_before_start() {
        // Given: end strictly before start
        // When:  constructing a Cue
        // Then:  it returns SubtitleError::InvalidTiming
        let result = Cue::new(1, Duration::from_secs(5), Duration::from_secs(1), "x");
        assert!(matches!(result, Err(SubtitleError::InvalidTiming { .. })));
    }
}
