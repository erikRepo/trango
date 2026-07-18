//! Reconciles Ollama's word-by-word Hebrew analysis onto niqud's word
//! boundaries — see [`merge_by_niqud_boundaries`]'s doc comment for why
//! niqud's tokenization is trusted as ground truth instead of Ollama's own.

use niqud::NiqudWord;
use word_analysis::{WordEntry, WordPart};

/// Maximum number of consecutive Ollama entries considered as one niqud
/// word. Hebrew prefix particles stack at most two deep onto a base word
/// (e.g. "כשכולם" = כ+ש+כולם, 3 morphemes total), so 4 leaves one entry of
/// margin without letting an unrelated run of entries merge together.
const MAX_MERGE_WINDOW: usize = 4;

/// Characters stripped from both ends of a word before comparing it across
/// Ollama's and niqud's word lists, purely for matching (and for a merged
/// entry's displayed `word`) — real Ollama output sometimes drops
/// punctuation that niqud's raw whitespace split still carries (e.g. a
/// sentence-final "לעצמו.": niqud's token keeps the period, Ollama's
/// doesn't), which would otherwise block an exact-text match.
const PUNCTUATION_TO_STRIP: [char; 14] = [
    '.', ',', '!', '?', ';', ':', '"', '\'', '״', '׳', '(', ')', '[', ']',
];

/// Strips [`PUNCTUATION_TO_STRIP`] from both ends of `word`.
fn normalize(word: &str) -> &str {
    word.trim_matches(|c: char| PUNCTUATION_TO_STRIP.contains(&c))
}

/// Reconciles `ollama_words` (Ollama's own, sometimes-inconsistent word
/// split) onto `niqud_words`'s word boundaries. Niqud always splits purely
/// on whitespace (`crates/niqud/src/onnx_client.rs`), so it never breaks a
/// fused Hebrew word apart — real Ollama output, in contrast, has been
/// observed to keep an attached prefix particle (ו/ה/ב/כ/ל/מ/ש) fused into
/// one entry for one word in a sentence, then split it into separate
/// top-level entries for another word in that SAME sentence, despite being
/// asked not to. An exact text-equality match (this module's predecessor,
/// `align_pronunciations`) can never fix the split case, since a split
/// fragment's text never equals the fused whole it's part of.
///
/// This instead walks both lists, growing a window of consecutive Ollama
/// entries (smallest first, up to [`MAX_MERGE_WINDOW`]) until their
/// concatenated text matches the current niqud word, then merges whichever
/// entries were consumed into one [`WordEntry`] carrying niqud's
/// pronunciation (see [`merge_window`]). A niqud word with no matching
/// window — an Ollama entry that doesn't correspond to niqud at all, or a
/// disagreement wider than the window covers — is resynced against by
/// checking one word of lookahead on the niqud side (a word Ollama dropped
/// entirely); failing that, the Ollama entry is passed through unchanged
/// so the rest of the sentence can still resync on the next word.
///
/// Known, accepted gaps (not handled, since they haven't been observed in
/// practice and speculatively handling them would be premature): maqaf
/// (־)-hyphenated compounds split by Ollama won't concatenate back to
/// niqud's maqaf-including token; and there's no symmetric check for
/// Ollama merging two real niqud words into one (only the reverse,
/// Ollama-over-splits, is handled).
pub fn merge_by_niqud_boundaries(
    ollama_words: Vec<WordEntry>,
    niqud_words: &[NiqudWord],
) -> Vec<WordEntry> {
    let mut merged = Vec::with_capacity(niqud_words.len());
    let (mut i, mut j) = (0, 0);

    while i < ollama_words.len() && j < niqud_words.len() {
        let target = normalize(&niqud_words[j].word);
        let max_window = MAX_MERGE_WINDOW.min(ollama_words.len() - i);
        let matched_window =
            (1..=max_window).find(|&k| concat_normalized(&ollama_words[i..i + k]) == target);

        if let Some(k) = matched_window {
            merged.push(merge_window(&ollama_words[i..i + k], &niqud_words[j]));
            i += k;
            j += 1;
        } else if j + 1 < niqud_words.len()
            && normalize(&ollama_words[i].word) == normalize(&niqud_words[j + 1].word)
        {
            // niqud has a word Ollama dropped entirely — resync on niqud's
            // side rather than letting the rest of the sentence drift.
            j += 1;
        } else {
            merged.push(ollama_words[i].clone());
            i += 1;
        }
    }

    merged.extend_from_slice(&ollama_words[i..]);
    merged
}

/// Concatenates `entries`' own (normalized) `word` text, with no
/// separator — Hebrew prefix particles attach directly with no space.
fn concat_normalized(entries: &[WordEntry]) -> String {
    entries.iter().map(|entry| normalize(&entry.word)).collect()
}

/// Builds one merged [`WordEntry`] from `window`, a run of consecutive
/// Ollama entries whose concatenated text matches `niqud_word`'s fused
/// token. A single-entry window (the word was already fused correctly)
/// keeps that entry's own `word`/`translation`/`parts` untouched, only
/// replacing `pronunciation`. A multi-entry window (Ollama split the word)
/// rebuilds `word`/`translation`/`parts` from the window itself — one flat
/// [`WordPart`] per consumed entry, discarding any `parts` an individual
/// consumed entry carried on its own (they're treated as atomic morphemes
/// here; real Ollama output has been observed to attach a nonsensical
/// self-referential `parts` to a lone split-off prefix particle).
fn merge_window(window: &[WordEntry], niqud_word: &NiqudWord) -> WordEntry {
    if let [only] = window {
        return WordEntry {
            pronunciation: niqud_word.pronunciation.clone(),
            ..only.clone()
        };
    }

    WordEntry {
        word: normalize(&niqud_word.word).to_string(),
        translation: join_translations(window),
        pronunciation: niqud_word.pronunciation.clone(),
        parts: window
            .iter()
            .map(|entry| WordPart {
                word: entry.word.clone(),
                translation: entry.translation.clone(),
            })
            .collect(),
    }
}

/// Joins `window`'s own per-entry translations with a single space, in
/// order, skipping any that are empty — Ollama occasionally omits a
/// translation for one entry, and a naive join would otherwise leave a
/// stray leading/double space in the merged phrase. This is a best-effort
/// gloss for the merged word as a whole; `parts` remains the authoritative
/// per-morpheme breakdown.
fn join_translations(window: &[WordEntry]) -> String {
    window
        .iter()
        .map(|entry| entry.translation.as_str())
        .filter(|translation| !translation.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(word: &str, translation: &str, pronunciation: &str) -> WordEntry {
        WordEntry {
            word: word.to_string(),
            translation: translation.to_string(),
            pronunciation: pronunciation.to_string(),
            parts: Vec::new(),
        }
    }

    fn word_with_parts(
        word_text: &str,
        translation: &str,
        pronunciation: &str,
        parts: Vec<WordPart>,
    ) -> WordEntry {
        WordEntry {
            parts,
            ..word(word_text, translation, pronunciation)
        }
    }

    fn part(word: &str, translation: &str) -> WordPart {
        WordPart {
            word: word.to_string(),
            translation: translation.to_string(),
        }
    }

    fn niqud_word(word: &str, pronunciation: &str) -> NiqudWord {
        NiqudWord {
            word: word.to_string(),
            niqud: word.to_string(),
            pronunciation: pronunciation.to_string(),
        }
    }

    #[test]
    fn test_single_entry_match_keeps_its_own_parts_and_overwrites_pronunciation() {
        // Given: Ollama already fused "במיטה" correctly, with its own
        //        correct parts breakdown
        // When:  merging onto niqud's boundaries
        // Then:  word/translation/parts are untouched, only pronunciation
        //        is replaced with niqud's
        let ollama_words = vec![word_with_parts(
            "במיטה",
            "in the bed",
            "baMITA",
            vec![part("ב", "in"), part("מיטה", "bed")],
        )];
        let niqud_words = vec![niqud_word("במיטה", "ba-mi-ta")];

        let result = merge_by_niqud_boundaries(ollama_words, &niqud_words);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].word, "במיטה");
        assert_eq!(result[0].translation, "in the bed");
        assert_eq!(result[0].pronunciation, "ba-mi-ta");
        assert_eq!(result[0].parts, vec![part("ב", "in"), part("מיטה", "bed")]);
    }

    #[test]
    fn test_two_entry_split_merges_into_one_fused_word() {
        // Given: Ollama split "ואמר" (ו+אמר) into two top-level entries
        // When:  merging onto niqud's single fused boundary
        // Then:  one entry comes back with the fused word/pronunciation, a
        //        space-joined translation, and a two-entry parts breakdown
        let ollama_words = vec![word("ו", "and", "vav"), word("אמר", "said", "amar")];
        let niqud_words = vec![niqud_word("ואמר", "ve-a-mar")];

        let result = merge_by_niqud_boundaries(ollama_words, &niqud_words);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].word, "ואמר");
        assert_eq!(result[0].translation, "and said");
        assert_eq!(result[0].pronunciation, "ve-a-mar");
        assert_eq!(result[0].parts, vec![part("ו", "and"), part("אמר", "said")]);
    }

    #[test]
    fn test_merge_discards_a_consumed_entrys_own_malformed_parts() {
        // Given: the split-off prefix entry carries a nonsensical
        //        self-referential parts array of its own — a real observed
        //        Ollama artifact
        // When:  merging it with the following entry
        // Then:  the merged entry's parts reflect the two consumed
        //        entries themselves, not the malformed nested one
        let ollama_words = vec![
            word_with_parts("ו", "and", "vav", vec![part("ו", "and")]),
            word("אמר", "said", "amar"),
        ];
        let niqud_words = vec![niqud_word("ואמר", "ve-a-mar")];

        let result = merge_by_niqud_boundaries(ollama_words, &niqud_words);

        assert_eq!(result[0].parts, vec![part("ו", "and"), part("אמר", "said")]);
    }

    #[test]
    fn test_three_entry_stack_merges_double_prefix() {
        // Given: Ollama split a double-stacked prefix ("מהספרים" = מ+ה+ספרים)
        //        into three top-level entries
        // When:  merging onto niqud's single fused boundary
        // Then:  one entry comes back with all three parts, in order
        let ollama_words = vec![
            word("מ", "from", "me"),
            word("ה", "the", "ha"),
            word("ספרים", "books", "sfarim"),
        ];
        let niqud_words = vec![niqud_word("מהספרים", "me-has-fa-rim")];

        let result = merge_by_niqud_boundaries(ollama_words, &niqud_words);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].word, "מהספרים");
        assert_eq!(result[0].translation, "from the books");
        assert_eq!(result[0].pronunciation, "me-has-fa-rim");
        assert_eq!(
            result[0].parts,
            vec![part("מ", "from"), part("ה", "the"), part("ספרים", "books")]
        );
    }

    #[test]
    fn test_translation_join_skips_empty_translations() {
        // Given: one of the consumed entries has no translation at all
        // When:  merging
        // Then:  the joined translation has no stray leading/double space
        let ollama_words = vec![word("ו", "", "vav"), word("אמר", "said", "amar")];
        let niqud_words = vec![niqud_word("ואמר", "ve-a-mar")];

        let result = merge_by_niqud_boundaries(ollama_words, &niqud_words);

        assert_eq!(result[0].translation, "said");
    }

    #[test]
    fn test_trailing_punctuation_on_niqud_word_does_not_block_a_match() {
        // Given: niqud's whitespace-split token kept a sentence-final
        //        period that Ollama's own "word" field dropped
        // When:  merging
        // Then:  the match still succeeds and pronunciation is corrected
        let ollama_words = vec![word("לעצמו", "himself", "atzmo")];
        let niqud_words = vec![niqud_word("לעצמו.", "le-atz-mo")];

        let result = merge_by_niqud_boundaries(ollama_words, &niqud_words);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].pronunciation, "le-atz-mo");
    }

    #[test]
    fn test_no_match_pushes_entry_through_and_niqud_pointer_waits() {
        // Given: an Ollama entry that doesn't correspond to niqud's only
        //        word at all (a hallucinated/unrelated entry), followed by
        //        one that does
        // When:  merging
        // Then:  the unrelated entry is kept unchanged and the real entry
        //        still gets corrected against the same (unconsumed) niqud
        //        word
        let ollama_words = vec![
            word("XXX", "???", "unrelated-pron"),
            word("שכב", "lay", "shkach"),
        ];
        let niqud_words = vec![niqud_word("שכב", "sha-khav")];

        let result = merge_by_niqud_boundaries(ollama_words, &niqud_words);

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].pronunciation, "unrelated-pron");
        assert_eq!(result[1].pronunciation, "sha-khav");
    }

    #[test]
    fn test_dropped_ollama_word_resyncs_via_niqud_lookahead() {
        // Given: Ollama dropped the sentence's first word entirely, so its
        //        one entry ("שכב") would otherwise be wrongly compared
        //        against niqud's first word ("הוא")
        // When:  merging
        // Then:  the lookahead resync skips niqud's dropped word instead of
        //        leaving the real entry uncorrected for the rest of the
        //        sentence
        let ollama_words = vec![word("שכב", "lay", "shkach")];
        let niqud_words = vec![niqud_word("הוא", "hu"), niqud_word("שכב", "sha-khav")];

        let result = merge_by_niqud_boundaries(ollama_words, &niqud_words);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].pronunciation, "sha-khav");
    }

    #[test]
    fn test_repeated_fused_word_aligns_each_occurrence_by_position() {
        // Given: the same fused word appears twice, both in Ollama's and
        //        niqud's lists, each occurrence with its own pronunciation
        // When:  merging
        // Then:  each occurrence is corrected at its own position, not
        //        both collapsing onto the first match
        let ollama_words = vec![
            word("בלילה", "at night", "first-ollama"),
            word("בלילה", "at night", "second-ollama"),
        ];
        let niqud_words = vec![
            niqud_word("בלילה", "first-niqud"),
            niqud_word("בלילה", "second-niqud"),
        ];

        let result = merge_by_niqud_boundaries(ollama_words, &niqud_words);

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].pronunciation, "first-niqud");
        assert_eq!(result[1].pronunciation, "second-niqud");
    }

    #[test]
    fn test_niqud_words_exhausted_appends_remaining_ollama_entries_unchanged() {
        // Given: an Ollama entry with no corresponding niqud word at all
        //        (niqud's list ran out first)
        // When:  merging
        // Then:  it's appended unchanged, without panicking
        let ollama_words = vec![
            word("שכב", "lay", "shkach"),
            word("EXTRA", "extra", "extra-pron"),
        ];
        let niqud_words = vec![niqud_word("שכב", "sha-khav")];

        let result = merge_by_niqud_boundaries(ollama_words, &niqud_words);

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].pronunciation, "sha-khav");
        assert_eq!(result[1].word, "EXTRA");
        assert_eq!(result[1].pronunciation, "extra-pron");
    }

    #[test]
    fn test_ollama_words_exhausted_stops_cleanly() {
        // Given: fewer Ollama entries than niqud words
        // When:  merging
        // Then:  the loop stops cleanly rather than panicking on an
        //        out-of-bounds index
        let ollama_words = vec![word("שכב", "lay", "shkach")];
        let niqud_words = vec![
            niqud_word("שכב", "sha-khav"),
            niqud_word("EXTRA", "extra-niqud"),
        ];

        let result = merge_by_niqud_boundaries(ollama_words, &niqud_words);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].pronunciation, "sha-khav");
    }
}
