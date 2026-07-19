//! Parsing of the SubRip (`.srt`) subtitle format into `Cue`s.

use std::time::Duration;

use crate::cue::Cue;
use crate::error::SubtitleError;

/// Parses the contents of an `.srt` file into a sequence of `Cue`s.
///
/// Strips a leading UTF-8 BOM and normalizes both `\n` and `\r\n` line
/// endings. Returns `SubtitleError::InvalidFormat` if a block does not
/// match the expected `index` / `start --> end` / `text` structure, and
/// `SubtitleError::InvalidTiming` if a cue's end time is not after its
/// start time.
pub fn parse_srt(input: &str) -> Result<Vec<Cue>, SubtitleError> {
    parse_srt_blocks(input)?
        .into_iter()
        .map(|(index, start, end, text)| Cue::new(index, start, end, text))
        .collect()
}

/// Splits `input` into raw `(index, start, end, text)` blocks — the same
/// index/timing-line/text-lines structure [`parse_srt`] builds `Cue`s
/// from, minus [`Cue::new`]'s `start < end` validation. Used directly by
/// [`parse_srt`] and by `word_timing::WhisperCliWordSegmenter::segment_words`,
/// which needs to tolerate (by dropping, not erroring the whole batch) an
/// occasional zero/negative-duration word `whisper-cli`'s DTW timestamps
/// produce at a clip boundary — a real subtitle file failing this
/// strictly is a problem worth surfacing, but one degenerate word out of
/// a sentence's worth of `whisper-cli` output isn't worth losing the
/// rest of that sentence's timing over.
pub(crate) fn parse_srt_blocks(
    input: &str,
) -> Result<Vec<(u32, Duration, Duration, String)>, SubtitleError> {
    let without_bom = input.strip_prefix('\u{FEFF}').unwrap_or(input);
    let normalized = without_bom.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = normalized.lines().collect();

    let mut blocks = Vec::new();
    let mut position = 0;

    while position < lines.len() {
        if lines[position].trim().is_empty() {
            position += 1;
            continue;
        }

        let index_line = lines[position];
        position += 1;
        let index: u32 = index_line.trim().parse().map_err(|_| {
            SubtitleError::InvalidFormat(format!("expected a cue index, got {index_line:?}"))
        })?;

        let timing_line: &str = lines.get(position).copied().ok_or_else(|| {
            SubtitleError::InvalidFormat(format!("cue {index}: missing timing line"))
        })?;
        position += 1;
        let (start, end) = parse_timing_line(timing_line)?;

        let mut text_lines = Vec::new();
        while position < lines.len() && !lines[position].trim().is_empty() {
            text_lines.push(lines[position]);
            position += 1;
        }

        blocks.push((index, start, end, text_lines.join("\n")));
    }

    Ok(blocks)
}

/// Parses a `start --> end` timing line into a pair of `Duration`s.
fn parse_timing_line(line: &str) -> Result<(Duration, Duration), SubtitleError> {
    let (start_str, rest) = line.split_once("-->").ok_or_else(|| {
        SubtitleError::InvalidFormat(format!("expected a timing line, got {line:?}"))
    })?;
    let end_str = rest.split_whitespace().next().unwrap_or("");
    Ok((
        parse_timestamp(start_str.trim())?,
        parse_timestamp(end_str)?,
    ))
}

/// Parses a single `HH:MM:SS,mmm` timestamp into a `Duration`.
fn parse_timestamp(text: &str) -> Result<Duration, SubtitleError> {
    let invalid = || SubtitleError::InvalidFormat(format!("invalid timestamp: {text:?}"));

    let (hms, millis_str) = text.split_once(',').ok_or_else(invalid)?;
    let mut parts = hms.split(':');
    let hours: u64 = parts
        .next()
        .ok_or_else(invalid)?
        .parse()
        .map_err(|_| invalid())?;
    let minutes: u64 = parts
        .next()
        .ok_or_else(invalid)?
        .parse()
        .map_err(|_| invalid())?;
    let seconds: u64 = parts
        .next()
        .ok_or_else(invalid)?
        .parse()
        .map_err(|_| invalid())?;
    if parts.next().is_some() {
        return Err(invalid());
    }
    let millis: u64 = millis_str.parse().map_err(|_| invalid())?;

    Ok(Duration::from_millis(
        ((hours * 60 + minutes) * 60 + seconds) * 1000 + millis,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_srt_returns_empty_vec_for_empty_input() {
        // Given: an empty string
        // When:  parsing it
        // Then:  no cues are returned and no error occurs
        assert_eq!(parse_srt("").unwrap(), Vec::new());
    }

    #[test]
    fn test_parse_srt_handles_crlf_line_endings() {
        // Given: a single cue using CRLF line endings
        // When:  parsing it
        // Then:  it parses identically to the LF version
        let input = "1\r\n00:00:01,000 --> 00:00:02,000\r\nHi\r\n\r\n";
        let cues = parse_srt(input).unwrap();
        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].text, "Hi");
    }

    #[test]
    fn test_parse_timestamp_converts_hours_minutes_seconds_millis() {
        // Given: a well-formed timestamp
        // When:  parsing it
        // Then:  it converts to the correct total Duration
        assert_eq!(
            parse_timestamp("01:02:03,456").unwrap(),
            Duration::from_millis(3_723_456)
        );
    }

    #[test]
    fn test_parse_srt_rejects_non_numeric_index() {
        // Given: a cue block whose first line is not a number
        // When:  parsing it
        // Then:  it returns SubtitleError::InvalidFormat
        let input = "one\n00:00:01,000 --> 00:00:02,000\nHi\n";
        assert!(matches!(
            parse_srt(input),
            Err(SubtitleError::InvalidFormat(_))
        ));
    }

    #[test]
    fn test_parse_srt_propagates_invalid_timing_from_cue() {
        // Given: a cue block whose end time is not after its start time
        // When:  parsing it
        // Then:  it returns SubtitleError::InvalidTiming
        let input = "1\n00:00:05,000 --> 00:00:01,000\nHi\n";
        assert!(matches!(
            parse_srt(input),
            Err(SubtitleError::InvalidTiming { .. })
        ));
    }

    #[test]
    fn test_parse_srt_blocks_does_not_validate_timing() {
        // Given: a block whose end time equals its start time (degenerate
        //        zero-duration, e.g. what whisper-cli's DTW timestamps can
        //        occasionally produce for a token right at a clip's edge)
        // When:  parsing via parse_srt_blocks (not parse_srt)
        // Then:  it's returned as-is rather than erroring — leaving the
        //        start<end check to Cue::new/parse_srt's callers, not
        //        this lower-level primitive
        let input = "1\n00:00:00,000 --> 00:00:00,000\nHi\n";
        let blocks = parse_srt_blocks(input).unwrap();
        assert_eq!(
            blocks,
            vec![(1, Duration::ZERO, Duration::ZERO, "Hi".to_string())]
        );
    }
}
