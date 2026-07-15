//! Integration tests: parsing real `.srt` fixture files from disk.

use std::time::Duration;

use subtitle::{parse_srt, SubtitleError};

/// Reads a fixture file from `tests/fixtures/` into a `String`.
fn read_fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(path).expect("fixture file should be readable")
}

#[test]
fn test_parses_valid_srt_file_into_cues() {
    // Given: a well-formed .srt fixture with two cues, one spanning two lines
    // When:  parsing it
    // Then:  both cues are returned with correct indices, timings, and text
    let cues = parse_srt(&read_fixture("valid.srt")).unwrap();

    assert_eq!(cues.len(), 2);

    assert_eq!(cues[0].index, 1);
    assert_eq!(cues[0].start, Duration::from_millis(1_000));
    assert_eq!(cues[0].end, Duration::from_millis(4_000));
    assert_eq!(cues[0].text, "First line of dialogue.");

    assert_eq!(cues[1].index, 2);
    assert_eq!(cues[1].start, Duration::from_millis(5_500));
    assert_eq!(cues[1].end, Duration::from_millis(8_000));
    assert_eq!(cues[1].text, "Second cue,\nspanning two lines.");
}

#[test]
fn test_parses_srt_file_with_leading_bom() {
    // Given: a valid .srt fixture prefixed with a UTF-8 BOM
    // When:  parsing it
    // Then:  the BOM is stripped and the cue is parsed normally
    let cues = parse_srt(&read_fixture("bom.srt")).unwrap();

    assert_eq!(cues.len(), 1);
    assert_eq!(cues[0].text, "BOM should not break parsing.");
}

#[test]
fn test_rejects_srt_file_with_missing_newline() {
    // Given: a fixture where the index and timing line are joined on one line
    // When:  parsing it
    // Then:  parsing fails with SubtitleError::InvalidFormat
    let result = parse_srt(&read_fixture("missing_newline.srt"));

    assert!(matches!(result, Err(SubtitleError::InvalidFormat(_))));
}

#[test]
fn test_rejects_srt_file_with_invalid_timestamp() {
    // Given: a fixture using a dot instead of a comma as the millisecond separator
    // When:  parsing it
    // Then:  parsing fails with SubtitleError::InvalidFormat
    let result = parse_srt(&read_fixture("invalid_timestamp.srt"));

    assert!(matches!(result, Err(SubtitleError::InvalidFormat(_))));
}
