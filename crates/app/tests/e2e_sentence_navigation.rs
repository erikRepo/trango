//! End-to-end test: parses the real sample subtitle fixture and drives
//! `playback-state`'s cue navigation over it, tying subtitle parsing and
//! navigation logic together against real files instead of synthetic cues.
//! See `docs/src/architecture/testing.md` for what this suite covers and
//! what it deliberately leaves out (e.g. libmpv rendering — see
//! `docs/src/architecture/video-playback.md`).

use std::path::PathBuf;
use std::time::Duration;

use playback_state::PlayerState;
use subtitle::{parse_srt, Cue};

/// Path to `test-media/sample/`, shared by both fixture files this test uses.
fn sample_media_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-media/sample")
}

/// Reads and parses the real sample subtitle track (`sample.srt`), the same
/// file `test-media/README.md` documents as this crate's E2E fixture.
fn sample_cues() -> Vec<Cue> {
    let srt_path = sample_media_dir().join("sample.srt");
    let contents = std::fs::read_to_string(&srt_path)
        .unwrap_or_else(|err| panic!("failed to read fixture {srt_path:?}: {err}"));
    parse_srt(&contents).expect("sample.srt should parse")
}

#[test]
fn test_sample_video_fixture_exists() {
    // Given: the sample video referenced by test-media/README.md
    // When:  checking it on disk
    // Then:  it exists and is non-empty — ties it to the subtitle fixture
    //        below without decoding it, since libmpv rendering itself isn't
    //        unit-testable (see docs/src/architecture/video-playback.md)
    let video_path = sample_media_dir().join("sample.mp4");
    let metadata = std::fs::metadata(&video_path)
        .unwrap_or_else(|err| panic!("failed to stat fixture {video_path:?}: {err}"));
    assert!(metadata.len() > 0);
}

#[test]
fn test_parses_sample_srt_into_five_cues() {
    // Given: the real sample.srt fixture
    // When:  parsing it
    // Then:  all five sentences come back with correct indices and timing
    let cues = sample_cues();

    assert_eq!(cues.len(), 5);
    assert_eq!(cues[0].index, 1);
    assert_eq!(cues[0].text, "Welcome to Trango Player.");
    assert_eq!(cues[4].index, 5);
    assert_eq!(cues[4].end, Duration::from_millis(16_345));
}

#[test]
fn test_cue_navigation_walks_all_sample_cues_forward_and_back() {
    // Given: PlayerState loaded with the real, parsed sample cues
    // When:  walking next_cue() to the end, then previous_cue() back to the
    //        start, then repeat_current_cue() on the first cue
    // Then:  the cursor and every returned command match the fixture's
    //        real timings at each step. next_cue/previous_cue only ever
    //        carry a seek target (no mode autoplays on navigation, see
    //        docs/src/specs/); repeat_current_cue (Space) is the one that
    //        carries a full start/end span to play.
    let cues = sample_cues();
    let mut state = PlayerState::new();
    state.set_cues(cues.clone());
    assert_eq!(state.current_cue_index, Some(0));

    for (expected_index, cue) in cues.iter().enumerate().skip(1) {
        let command = state
            .next_cue()
            .unwrap_or_else(|| panic!("expected a seek command advancing to cue {expected_index}"));
        assert_eq!(state.current_cue_index, Some(expected_index));
        assert_eq!(command.start, cue.start);
    }

    // Cursor is on the last cue now; next_cue() has nowhere further to go.
    assert_eq!(state.next_cue(), None);
    assert_eq!(state.current_cue_index, Some(cues.len() - 1));

    for (expected_index, cue) in cues.iter().enumerate().rev().skip(1) {
        let command = state.previous_cue().unwrap_or_else(|| {
            panic!("expected a seek command going back to cue {expected_index}")
        });
        assert_eq!(state.current_cue_index, Some(expected_index));
        assert_eq!(command.start, cue.start);
    }

    assert_eq!(state.previous_cue(), None);
    assert_eq!(state.current_cue_index, Some(0));

    let repeat_first = state.repeat_current_cue().expect("a cue is in focus");
    let repeat_second = state.repeat_current_cue().expect("a cue is in focus");
    assert_eq!(repeat_first, repeat_second);
    assert_eq!(repeat_first.start, cues[0].start);
    assert_eq!(repeat_first.end, cues[0].end);
}
