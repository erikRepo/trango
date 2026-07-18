//! Reconstructs phonikud-onnx's diacritized-text output format from a
//! model run's raw logits — a direct port of `phonikud_onnx`'s
//! `OnnxModel.predict` reconstruction loop (read in full during
//! development), minus the parts that don't apply to our simplified
//! one-character-per-token tokenization (see `tokenizer.rs`'s module
//! docs).

use crate::tokenizer::Token;

/// `phonikud_onnx`'s `NIKUD_CLASSES`: the 29 possible nikud-point
/// combinations `nikud_logits`' argmax selects between. Index 1
/// (`<MAT_LECT>`) is the mater-lectionis marker, handled specially below
/// rather than emitted literally.
const NIKUD_CLASSES: [&str; 29] = [
    "",
    "<MAT_LECT>",
    "\u{05BC}",
    "\u{05B0}",
    "\u{05B1}",
    "\u{05B2}",
    "\u{05B3}",
    "\u{05B4}",
    "\u{05B5}",
    "\u{05B6}",
    "\u{05B7}",
    "\u{05B8}",
    "\u{05B9}",
    "\u{05BA}",
    "\u{05BB}",
    "\u{05BC}\u{05B0}",
    "\u{05BC}\u{05B1}",
    "\u{05BC}\u{05B2}",
    "\u{05BC}\u{05B3}",
    "\u{05BC}\u{05B4}",
    "\u{05BC}\u{05B5}",
    "\u{05BC}\u{05B6}",
    "\u{05BC}\u{05B7}",
    "\u{05BC}\u{05B8}",
    "\u{05BC}\u{05B9}",
    "\u{05BC}\u{05BA}",
    "\u{05BC}\u{05BB}",
    "\u{05C7}",
    "\u{05BC}\u{05C7}",
];
const MAT_LECT_CLASS: usize = 1;
const SHIN_CLASSES: [&str; 2] = ["\u{05C1}", "\u{05C2}"];
const MATRES_LETTERS: [char; 3] = ['\u{05D0}', '\u{05D5}', '\u{05D9}']; // alef, vav, yod
const STRESS_CHAR: &str = "\u{05AB}";
const VOCAL_SHVA_CHAR: &str = "\u{05BD}";
const PREFIX_CHAR: &str = "|";

fn is_hebrew_letter(c: char) -> bool {
    ('\u{05D0}'..='\u{05EA}').contains(&c)
}

/// The index of the largest value in `logits`, matching NumPy's
/// `argmax` (the *first* index on ties, via strict `>`).
fn argmax(logits: &[f32]) -> usize {
    let mut best_index = 0;
    let mut best_value = logits[0];
    for (index, &value) in logits.iter().enumerate().skip(1) {
        if value > best_value {
            best_value = value;
            best_index = index;
        }
    }
    best_index
}

/// Reconstructs the diacritized string for `niqud_stripped` (the text
/// `tokens` was tokenized from — see `tokenizer::tokenize`) from a model
/// run's three flat output tensors. `nikud_logits`/`shin_logits`/
/// `additional_logits` are the flattened `[1, seq_len, C]` tensors (`C` =
/// 29/2/3 respectively) `tokens.len() == seq_len` rows tall.
///
/// Mirrors `phonikud_onnx.OnnxModel.predict`'s per-token loop with
/// `mark_matres_lectionis=None` (matching what the removed Python CLI
/// wrapper always passed): a mater-lectionis letter predicted with no
/// other vowel gets no diacritic mark at all, rather than the optional
/// `|`-style marker phonikud-onnx supports and this project never used.
pub fn decode(
    niqud_stripped: &str,
    tokens: &[Token],
    nikud_logits: &[f32],
    shin_logits: &[f32],
    additional_logits: &[f32],
) -> String {
    let chars: Vec<char> = niqud_stripped.chars().collect();
    let mut output = String::new();

    for (row, token) in tokens.iter().enumerate() {
        let Some(char_index) = token.char_index else {
            continue; // [CLS]/[SEP] contribute nothing to the output
        };
        let Some(&letter) = chars.get(char_index) else {
            continue;
        };
        if !is_hebrew_letter(letter) {
            output.push(letter);
            continue;
        }

        let nikud_class = argmax(&nikud_logits[row * 29..row * 29 + 29]);
        let mut nikud = NIKUD_CLASSES[nikud_class];
        let shin = if letter == 'ש' {
            SHIN_CLASSES[argmax(&shin_logits[row * 2..row * 2 + 2])]
        } else {
            ""
        };

        if nikud_class == MAT_LECT_CLASS {
            if !MATRES_LETTERS.contains(&letter) {
                nikud = "";
            } else {
                output.push(letter);
                continue;
            }
        }

        let stress = if additional_logits[row * 3] > 0.0 {
            STRESS_CHAR
        } else {
            ""
        };
        let vocal_shva = if additional_logits[row * 3 + 1] > 0.0 {
            VOCAL_SHVA_CHAR
        } else {
            ""
        };
        let prefix = if additional_logits[row * 3 + 2] > 0.0 {
            PREFIX_CHAR
        } else {
            ""
        };

        output.push(letter);
        output.push_str(shin);
        output.push_str(nikud);
        output.push_str(stress);
        output.push_str(vocal_shva);
        output.push_str(prefix);
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokenizer::tokenize;

    #[test]
    fn test_argmax_returns_the_first_index_on_ties() {
        // Given: a slice with a tied maximum at indices 1 and 3
        // When:  taking the argmax
        // Then:  the first tied index wins, matching NumPy's argmax
        assert_eq!(argmax(&[1.0, 5.0, 2.0, 5.0]), 1);
    }

    /// The real `nikud_logits`/`shin_logits`/`additional_logits` (dumped
    /// from a real phonikud-onnx run against "שכב" during development)
    /// plus the token ids the real HuggingFace tokenizer produced for it,
    /// as JSON — a genuine regression fixture needing no model file at
    /// `cargo test` time.
    struct ShakhavFixture {
        nikud_logits: Vec<f32>,
        shin_logits: Vec<f32>,
        additional_logits: Vec<f32>,
    }

    fn load_shakhav_fixture() -> ShakhavFixture {
        let raw = include_str!("../tests/fixtures/shakhav_logits.json");
        let json: serde_json::Value = serde_json::from_str(raw).expect("fixture should parse");
        let floats = |key: &str| -> Vec<f32> {
            json[key]
                .as_array()
                .unwrap_or_else(|| panic!("fixture missing {key}"))
                .iter()
                .map(|v| v.as_f64().expect("fixture value should be a number") as f32)
                .collect()
        };
        ShakhavFixture {
            nikud_logits: floats("nikud_logits"),
            shin_logits: floats("shin_logits"),
            additional_logits: floats("additional_logits"),
        }
    }

    #[test]
    fn test_decode_shakhav_matches_the_real_phonikud_onnx_output() {
        // Given: real captured logits for "שכב" and the matching tokens
        //        (using the crate's own tokenizer, since the fixture's
        //        vocab ids were confirmed identical to the real
        //        tokenizer's in tokenizer.rs's own test)
        // When:  decoding them
        // Then:  the exact diacritized string phonikud-onnx produced
        //        ("שָׁכַב") comes back — the same string
        //        niqud_to_pronunciation already turns into "sha-khav"
        let fixture = load_shakhav_fixture();
        let niqud_stripped = "שכב";
        let vocab_json = include_str!("../tests/fixtures/tokenizer.json");
        let vocab =
            crate::tokenizer::Vocab::from_tokenizer_json(vocab_json).expect("fixture should load");
        let tokens = tokenize(&vocab, niqud_stripped);

        let result = decode(
            niqud_stripped,
            &tokens,
            &fixture.nikud_logits,
            &fixture.shin_logits,
            &fixture.additional_logits,
        );

        // Built from explicit codepoints (rather than a pasted literal)
        // since shin-dot and the vowel point visually render the same
        // regardless of insertion order, but decode's actual output
        // order (shin before nikud, matching phonikud_onnx's own
        // `char + shin + nikud + ...` construction) matters for equality.
        let expected = "\u{05E9}\u{05C1}\u{05B8}\u{05DB}\u{05B7}\u{05D1}";
        assert_eq!(result, expected);
        assert_eq!(
            crate::transliterate::niqud_to_pronunciation(&result),
            "sha-khav"
        );
    }
}
