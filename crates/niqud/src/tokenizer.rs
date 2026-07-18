//! A minimal reimplementation of the `dicta-il/dictabert-large-char-menaked`
//! tokenizer's relevant behavior — confirmed during development (by reading
//! its real `tokenizer.json`) to be a **character-level** tokenizer despite
//! being stored in HuggingFace's "WordPiece" format: its `pre_tokenizer`
//! splits into individual characters before the (never-actually-merging)
//! WordPiece model runs, so no subword logic is needed — a flat char->id
//! vocab lookup is enough. This means the `tokenizers` crate isn't a
//! dependency here at all, just `serde_json` for the vocab.
//!
//! **Known simplification vs. the real tokenizer's normalizer:** NFKC and
//! lowercasing are replicated per-character; `StripAccents` is not. This
//! only affects rare accented-Latin-loanword characters inside an
//! otherwise-Hebrew sentence (a stray accented character becomes `[UNK]`
//! instead of having its accent stripped) — real Hebrew letters/niqud are
//! never affected. Also assumes one input character maps to exactly one
//! token (true for the real tokenizer whenever every character is
//! individually allowed, which covers all real Hebrew subtitle content;
//! only differs if the normalizer's disallowed-character regex would
//! collapse a *run* of consecutive disallowed characters into a single
//! `[UNK]`, an edge case outside Hebrew subtitle text).

use std::collections::HashMap;

use unicode_normalization::UnicodeNormalization;

use crate::error::NiqudError;

const UNK_ID: u32 = 0;
const CLS_ID: u32 = 1;
const SEP_ID: u32 = 2;

/// The tokenizer's char->id vocabulary, loaded from a `tokenizer.json`
/// file's `model.vocab` object.
pub struct Vocab {
    char_to_id: HashMap<char, u32>,
}

impl Vocab {
    /// Parses `json` (a `tokenizer.json` file's contents) into a [`Vocab`],
    /// keeping only single-character vocab entries — the special tokens
    /// (`[UNK]`/`[CLS]`/`[SEP]`/...) are handled separately by
    /// [`tokenize`]/[`strip_niqud`], not looked up through this map.
    pub fn from_tokenizer_json(json: &str) -> Result<Self, NiqudError> {
        let parsed: serde_json::Value = serde_json::from_str(json)
            .map_err(|err| NiqudError::ModelLoadFailed(format!("invalid tokenizer.json: {err}")))?;
        let vocab_obj = parsed
            .get("model")
            .and_then(|model| model.get("vocab"))
            .and_then(|vocab| vocab.as_object())
            .ok_or_else(|| {
                NiqudError::ModelLoadFailed(
                    "tokenizer.json is missing a model.vocab object".to_string(),
                )
            })?;

        let mut char_to_id = HashMap::new();
        for (key, value) in vocab_obj {
            let mut chars = key.chars();
            let (Some(c), None) = (chars.next(), chars.next()) else {
                continue; // multi-char entry (a special token like "[CLS]") — not a lookup target
            };
            if let Some(id) = value.as_u64() {
                char_to_id.insert(c, id as u32);
            }
        }
        Ok(Self { char_to_id })
    }
}

/// Removes niqud diacritics/cantillation and the prefix-boundary marker
/// from `text` (Unicode block U+0590-U+05C7, plus `|`) — mirrors
/// phonikud_onnx's `remove_nikkud`. Base Hebrew letters (U+05D0 onward)
/// are untouched. The model is always run on already-undiacritized text,
/// even if `text` happens to already contain niqud.
pub fn strip_niqud(text: &str) -> String {
    text.chars()
        .filter(|&c| !(('\u{0590}'..='\u{05C7}').contains(&c) || c == '|'))
        .collect()
}

/// Looks up `c`'s vocab id after normalizing it the way the tokenizer's
/// own normalizer would (NFKC + lowercase — see module docs for what's
/// intentionally not replicated). Falls back to `[UNK]` if `c` isn't in
/// the vocab, or its normalized form isn't a single character.
fn normalized_id(vocab: &Vocab, c: char) -> u32 {
    let normalized: String = c.nfkc().collect::<String>().to_lowercase();
    let mut chars = normalized.chars();
    match (chars.next(), chars.next()) {
        (Some(normalized_char), None) => vocab
            .char_to_id
            .get(&normalized_char)
            .copied()
            .unwrap_or(UNK_ID),
        _ => UNK_ID,
    }
}

/// One token: its vocab id, and the index (into `text`'s `chars()`) of the
/// source character it came from — `None` for `[CLS]`/`[SEP]`.
pub struct Token {
    pub id: u32,
    pub char_index: Option<usize>,
}

/// Tokenizes already niqud-stripped `text` (see [`strip_niqud`]) into
/// `[CLS, <one token per character>, SEP]`.
pub fn tokenize(vocab: &Vocab, text: &str) -> Vec<Token> {
    let mut tokens = vec![Token {
        id: CLS_ID,
        char_index: None,
    }];
    for (index, c) in text.chars().enumerate() {
        tokens.push(Token {
            id: normalized_id(vocab, c),
            char_index: Some(index),
        });
    }
    tokens.push(Token {
        id: SEP_ID,
        char_index: None,
    });
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_vocab() -> Vocab {
        let json = include_str!("../tests/fixtures/tokenizer.json");
        Vocab::from_tokenizer_json(json).expect("fixture tokenizer.json should parse")
    }

    #[test]
    fn test_strip_niqud_removes_diacritics_but_keeps_letters() {
        // Given: niqud-annotated text with a prefix-boundary marker
        // When:  stripping niqud
        // Then:  only the base letters remain
        assert_eq!(strip_niqud("בַּ|מִּיטָּה"), "במיטה");
    }

    #[test]
    fn test_strip_niqud_is_a_no_op_on_plain_text() {
        // Given: text with no niqud at all
        // When:  stripping niqud
        // Then:  it comes back unchanged
        assert_eq!(strip_niqud("שכב"), "שכב");
    }

    #[test]
    fn test_tokenize_shakhav_matches_the_real_tokenizers_ids() {
        // Given: the fixture vocab and the word "שכב"
        // When:  tokenizing it
        // Then:  the ids match exactly what the real HuggingFace tokenizer
        //        produced for this word during development ([CLS]=1,
        //        ש=234, כ=220, ב=210, [SEP]=2)
        let vocab = fixture_vocab();

        let tokens = tokenize(&vocab, "שכב");

        let ids: Vec<u32> = tokens.iter().map(|t| t.id).collect();
        assert_eq!(ids, vec![1, 234, 220, 210, 2]);
    }

    #[test]
    fn test_tokenize_tracks_source_char_index_and_special_tokens() {
        // Given: the fixture vocab and a short word
        // When:  tokenizing it
        // Then:  CLS/SEP carry no char_index, and each letter token's
        //        char_index matches its position in the source text
        let vocab = fixture_vocab();

        let tokens = tokenize(&vocab, "אב");

        assert_eq!(tokens[0].char_index, None); // CLS
        assert_eq!(tokens[1].char_index, Some(0)); // א
        assert_eq!(tokens[2].char_index, Some(1)); // ב
        assert_eq!(tokens[3].char_index, None); // SEP
    }

    #[test]
    fn test_tokenize_unknown_character_falls_back_to_unk() {
        // Given: the fixture vocab (which doesn't include Latin letters)
        //        and text containing one
        // When:  tokenizing it
        // Then:  the unknown character becomes [UNK] rather than panicking
        let vocab = fixture_vocab();

        let tokens = tokenize(&vocab, "x");

        assert_eq!(tokens[1].id, 0);
    }
}
