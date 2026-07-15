//! Merging a translation subtitle track into an original track's cues.

use std::time::Duration;

use crate::cue::Cue;

/// Merges `translation`'s text into `original`'s cues by timing overlap.
///
/// Matching is done by the overlap between each original cue's `[start,
/// end)` range and each translation cue's range, not by index: each
/// original cue receives the text of whichever translation cue overlaps it
/// the most. This is deliberate rather than matching by position, because
/// the original and translation tracks may not have the same number of
/// cues (e.g. a hand-timed original paired with an STT-generated
/// translation, or vice versa) — index-based matching would silently pair
/// up unrelated lines once the two tracks drift out of sync.
///
/// An original cue with no overlapping translation cue keeps
/// `translation: None`.
pub fn merge_translation(original: Vec<Cue>, translation: Vec<Cue>) -> Vec<Cue> {
    original
        .into_iter()
        .map(|cue| {
            let matched_text = best_overlapping_text(&cue, &translation);
            Cue {
                translation: matched_text,
                ..cue
            }
        })
        .collect()
}

/// Finds the text of the cue in `candidates` with the largest overlap
/// against `cue`'s time range, if any candidate overlaps at all.
fn best_overlapping_text(cue: &Cue, candidates: &[Cue]) -> Option<String> {
    candidates
        .iter()
        .map(|candidate| (overlap(cue, candidate), candidate))
        .filter(|(overlap, _)| *overlap > Duration::ZERO)
        .max_by_key(|(overlap, _)| *overlap)
        .map(|(_, candidate)| candidate.text.clone())
}

/// Computes the duration of overlap between two cues' `[start, end)` ranges,
/// or `Duration::ZERO` if they don't overlap.
fn overlap(a: &Cue, b: &Cue) -> Duration {
    let start = a.start.max(b.start);
    let end = a.end.min(b.end);
    end.saturating_sub(start)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cue(index: u32, start_ms: u64, end_ms: u64, text: &str) -> Cue {
        Cue::new(
            index,
            Duration::from_millis(start_ms),
            Duration::from_millis(end_ms),
            text,
        )
        .unwrap()
    }

    #[test]
    fn test_merge_translation_matches_fully_compatible_pair() {
        // Given: original and translation tracks with identical timings and cue counts
        // When:  merging the translation in
        // Then:  each original cue receives the translation text at the same timing
        let original = vec![cue(1, 0, 2_000, "Hello"), cue(2, 2_000, 4_000, "World")];
        let translation = vec![cue(1, 0, 2_000, "Hei"), cue(2, 2_000, 4_000, "Maailma")];

        let merged = merge_translation(original, translation);

        assert_eq!(merged[0].translation.as_deref(), Some("Hei"));
        assert_eq!(merged[1].translation.as_deref(), Some("Maailma"));
    }

    #[test]
    fn test_merge_translation_leaves_none_when_no_translation_cue_overlaps() {
        // Given: a translation track that only covers the timing of one of two original cues
        // When:  merging the translation in
        // Then:  the uncovered original cue's translation stays None
        let original = vec![cue(1, 0, 2_000, "Hello"), cue(2, 5_000, 7_000, "Goodbye")];
        let translation = vec![cue(1, 0, 2_000, "Hei")];

        let merged = merge_translation(original, translation);

        assert_eq!(merged[0].translation.as_deref(), Some("Hei"));
        assert_eq!(merged[1].translation, None);
    }

    #[test]
    fn test_merge_translation_matches_by_overlap_when_cue_counts_differ() {
        // Given: an original track with three cues and a translation track with only two,
        //        each translation cue spanning the timing of roughly 1.5 original cues
        // When:  merging the translation in
        // Then:  each original cue is matched to the translation cue it overlaps most
        let original = vec![
            cue(1, 0, 1_000, "A"),
            cue(2, 1_000, 2_400, "B"),
            cue(3, 2_400, 3_000, "C"),
        ];
        let translation = vec![
            cue(1, 0, 1_500, "Ensimmäinen"),
            cue(2, 1_500, 3_000, "Toinen"),
        ];

        let merged = merge_translation(original, translation);

        assert_eq!(merged[0].translation.as_deref(), Some("Ensimmäinen"));
        assert_eq!(merged[1].translation.as_deref(), Some("Toinen"));
        assert_eq!(merged[2].translation.as_deref(), Some("Toinen"));
    }
}
