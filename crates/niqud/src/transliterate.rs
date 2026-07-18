//! Converts phonikud-onnx's niqud (diacritized) output into a
//! deterministic, hyphenated Latin pronunciation guide — replacing what
//! Ollama previously guessed unreliably for Hebrew. See
//! `docs/src/developer/specs.md`'s "Hebrew pronunciation" entry for why
//! this is computed here rather than by another LLM call.
//!
//! One syllable is emitted per vowel found in the niqud text; a bare
//! (vowelless) consonant becomes the onset of the *next* vowel-bearing
//! syllable if one follows in the word, or the coda of the *previous*
//! syllable if it's word-final — matching how Hebrew pronunciation guides
//! are conventionally hyphenated (e.g. שָׁכַב -> "sha-khav").

const DAGESH: char = '\u{05BC}';
const SHIN_DOT: char = '\u{05C1}';
const SIN_DOT: char = '\u{05C2}';
const VOCAL_SHVA: char = '\u{05BD}'; // meteg, marks a pronounced (not silent) shva
const STRESS: char = '\u{05AB}'; // ole, stress mark — not used in the guide yet
const PREFIX_BOUNDARY: char = '|'; // morpheme boundary after a prefix letter — syllable breaks already fall out of vowel placement, so this needs no special handling

/// One base Hebrew letter plus the diacritics phonikud-onnx attached
/// directly after it (dagesh, a vowel point, shin/sin-dot, vocal-shva).
struct Grapheme {
    letter: char,
    has_dagesh: bool,
    vowel_point: Option<char>,
    is_sin: bool,
    vocal_shva: bool,
}

fn is_hebrew_letter(c: char) -> bool {
    ('\u{05D0}'..='\u{05EA}').contains(&c)
}

/// Splits niqud text into base-letter graphemes, discarding stress marks
/// and prefix-boundary markers (both insignificant for hyphenation — see
/// the module doc) and any non-Hebrew character (spaces, punctuation).
fn split_graphemes(niqud: &str) -> Vec<Grapheme> {
    let filtered: Vec<char> = niqud.chars().filter(|&c| c != STRESS).collect();
    let mut graphemes = Vec::new();
    let mut chars = filtered.into_iter().peekable();
    while let Some(c) = chars.next() {
        if !is_hebrew_letter(c) {
            continue;
        }
        let mut grapheme = Grapheme {
            letter: c,
            has_dagesh: false,
            vowel_point: None,
            is_sin: false,
            vocal_shva: false,
        };
        while let Some(&next) = chars.peek() {
            match next {
                DAGESH => grapheme.has_dagesh = true,
                SHIN_DOT => grapheme.is_sin = false,
                SIN_DOT => grapheme.is_sin = true,
                VOCAL_SHVA => grapheme.vocal_shva = true,
                PREFIX_BOUNDARY => {}
                '\u{05B0}'..='\u{05BB}' | '\u{05C7}' => grapheme.vowel_point = Some(next),
                _ => break,
            }
            chars.next();
        }
        graphemes.push(grapheme);
    }
    graphemes
}

/// Maps a vowel-point codepoint to its Latin sound. Sheva (`\u{05B0}`) is
/// handled by the caller instead, since its sound depends on
/// `vocal_shva`, not the point alone.
fn vowel_sound(point: char) -> &'static str {
    match point {
        '\u{05B1}' => "e", // hataf segol
        '\u{05B2}' => "a", // hataf patah
        '\u{05B3}' => "o", // hataf qamats
        '\u{05B4}' => "i", // hiriq
        '\u{05B5}' => "e", // tsere
        '\u{05B6}' => "e", // segol
        '\u{05B7}' => "a", // patah
        '\u{05B8}' => "a", // qamats
        '\u{05B9}' => "o", // holam
        '\u{05BA}' => "o", // holam haser for vav
        '\u{05BB}' => "u", // qubuts
        '\u{05C7}' => "o", // qamats qatan
        _ => "",
    }
}

/// Resolves a grapheme's vowel point (if any) to its Latin sound.
fn resolve_vowel(grapheme: &Grapheme) -> Option<String> {
    match grapheme.vowel_point {
        None => None,
        Some('\u{05B0}') => grapheme.vocal_shva.then(|| "e".to_string()),
        Some(point) => Some(vowel_sound(point).to_string()),
    }
}

/// Resolves a grapheme to its consonant sound (`""` if silent) and vowel
/// sound. Alef/ayin are always silent; he is silent only word-finally
/// with no vowel of its own (the common mater-lectionis pattern); vav/yod
/// are silent vowel-carriers (mater lectionis) when they carry no
/// consonantal vowel of their own — shuruk (vav+dagesh, no other point)
/// is "u", vav+holam alone is "o", a bare yod next to a preceding vowel
/// is silent.
fn resolve_consonant_and_vowel(grapheme: &Grapheme, is_last: bool) -> (String, Option<String>) {
    match grapheme.letter {
        '\u{05D0}' | '\u{05E2}' => (String::new(), resolve_vowel(grapheme)), // alef, ayin
        '\u{05D4}' => {
            // he
            if is_last && grapheme.vowel_point.is_none() {
                (String::new(), None)
            } else {
                ("h".to_string(), resolve_vowel(grapheme))
            }
        }
        '\u{05D5}' => {
            // vav
            if grapheme.has_dagesh && grapheme.vowel_point.is_none() {
                (String::new(), Some("u".to_string())) // shuruk
            } else if !grapheme.has_dagesh && grapheme.vowel_point == Some('\u{05B9}') {
                (String::new(), Some("o".to_string())) // holam male
            } else {
                ("v".to_string(), resolve_vowel(grapheme))
            }
        }
        '\u{05D9}' => {
            // yod
            match resolve_vowel(grapheme) {
                Some(vowel) => ("y".to_string(), Some(vowel)),
                None => (String::new(), None), // silent mater lectionis
            }
        }
        '\u{05D1}' => (dagesh_pick(grapheme, "b", "v"), resolve_vowel(grapheme)), // bet
        '\u{05D2}' => ("g".to_string(), resolve_vowel(grapheme)),
        '\u{05D3}' => ("d".to_string(), resolve_vowel(grapheme)),
        '\u{05D6}' => ("z".to_string(), resolve_vowel(grapheme)),
        '\u{05D7}' => ("kh".to_string(), resolve_vowel(grapheme)),
        '\u{05D8}' => ("t".to_string(), resolve_vowel(grapheme)),
        '\u{05DA}' | '\u{05DB}' => (dagesh_pick(grapheme, "k", "kh"), resolve_vowel(grapheme)), // final kaf, kaf
        '\u{05DC}' => ("l".to_string(), resolve_vowel(grapheme)),
        '\u{05DD}' | '\u{05DE}' => ("m".to_string(), resolve_vowel(grapheme)), // final mem, mem
        '\u{05DF}' | '\u{05E0}' => ("n".to_string(), resolve_vowel(grapheme)), // final nun, nun
        '\u{05E1}' => ("s".to_string(), resolve_vowel(grapheme)),
        '\u{05E3}' | '\u{05E4}' => (dagesh_pick(grapheme, "p", "f"), resolve_vowel(grapheme)), // final pe, pe
        '\u{05E5}' | '\u{05E6}' => ("tz".to_string(), resolve_vowel(grapheme)), // final tsadi, tsadi
        '\u{05E7}' => ("k".to_string(), resolve_vowel(grapheme)),
        '\u{05E8}' => ("r".to_string(), resolve_vowel(grapheme)),
        '\u{05E9}' => (
            if grapheme.is_sin { "s" } else { "sh" }.to_string(),
            resolve_vowel(grapheme),
        ),
        '\u{05EA}' => ("t".to_string(), resolve_vowel(grapheme)),
        _ => (String::new(), None),
    }
}

/// Picks a letter's dagesh/no-dagesh sound (begadkefat letters ב/כ/פ).
fn dagesh_pick(grapheme: &Grapheme, with_dagesh: &str, without: &str) -> String {
    if grapheme.has_dagesh {
        with_dagesh
    } else {
        without
    }
    .to_string()
}

/// Converts one word's niqud (diacritized) text into a hyphenated Latin
/// pronunciation guide, e.g. "שָׁכַב" -> "sha-khav".
///
/// Known limitation: this covers standard nikud/dagesh/shin-dot/vocal-shva
/// patterns validated against real phonikud-onnx output, not full Hebrew
/// grammar — rare or ambiguous forms may hyphenate imperfectly.
pub fn niqud_to_pronunciation(niqud: &str) -> String {
    let graphemes = split_graphemes(niqud);
    let mut syllables: Vec<String> = Vec::new();
    let mut pending: Vec<String> = Vec::new();

    let last_index = graphemes.len().saturating_sub(1);
    for (index, grapheme) in graphemes.iter().enumerate() {
        let (consonant, vowel) = resolve_consonant_and_vowel(grapheme, index == last_index);
        if !consonant.is_empty() {
            pending.push(consonant);
        }
        if let Some(vowel) = vowel {
            let mut onset = pending.pop().unwrap_or_default();
            if !pending.is_empty() {
                let coda: String = pending.concat();
                match syllables.last_mut() {
                    Some(previous) => previous.push_str(&coda),
                    None => onset = format!("{coda}{onset}"),
                }
                pending.clear();
            }
            syllables.push(format!("{onset}{vowel}"));
        }
    }

    if !pending.is_empty() {
        let coda: String = pending.concat();
        match syllables.last_mut() {
            Some(last) => last.push_str(&coda),
            None => syllables.push(coda),
        }
    }

    syllables.join("-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shakhav_lay_down() {
        // Given: niqud for שכב ("lay down"), a case the LLM previously
        //        got wrong ("shkach")
        // When:  converting to a pronunciation guide
        // Then:  it reads "sha-khav" — kaf without dagesh is "kh", final
        //        bet without dagesh is "v" and attaches to the same syllable
        assert_eq!(niqud_to_pronunciation("שָׁכַב"), "sha-khav");
    }

    #[test]
    fn test_leatzmo_to_himself() {
        // Given: niqud for לעצמו ("to himself"), another case the LLM
        //        previously got wrong ("la-atz-mu")
        // When:  converting to a pronunciation guide
        // Then:  it reads "le-atz-mo" — vocal shva on the lamed prefix
        //        gives "le", silent shva on tsadi makes it the coda of the
        //        ayin's syllable, and the final vav+holam mater gives "o"
        assert_eq!(niqud_to_pronunciation("לְֽ|עַצְמוֹ"), "le-atz-mo");
    }

    #[test]
    fn test_hu_he_pronoun() {
        // Given: niqud for הוא ("he"), where shuruk (vav+dagesh, no other
        //        vowel point) is the mater lectionis for "u"
        // When:  converting to a pronunciation guide
        // Then:  it reads "hu" — the final alef is silent
        assert_eq!(niqud_to_pronunciation("הוּא"), "hu");
    }

    #[test]
    fn test_bamita_in_the_bed() {
        // Given: niqud for במיטה ("in the bed"), with a prefix boundary
        //        marker and a bare medial yod (mater lectionis for hiriq)
        // When:  converting to a pronunciation guide
        // Then:  it reads "ba-mi-ta" — the prefix bet has dagesh ("b"),
        //        the medial yod contributes nothing (already voiced by
        //        the preceding hiriq), and the final he is silent
        assert_eq!(niqud_to_pronunciation("בַּ|מִּיטָּה"), "ba-mi-ta");
    }

    #[test]
    fn test_veamar_and_said() {
        // Given: niqud for ואמר ("and said/he said")
        // When:  converting to a pronunciation guide
        // Then:  it reads "ve-a-mar" — vocal shva on the prefix vav gives
        //        "ve", the silent alef still carries its own qamats
        //        syllable "a", and the final resh has no vowel of its own
        //        so it becomes the coda of "ma"
        assert_eq!(niqud_to_pronunciation("וְֽ|אָמַר"), "ve-a-mar");
    }

    #[test]
    fn test_shalom_greeting() {
        // Given: niqud for שלום ("hello"/"peace"), with a bare lamed and
        //        a vav+holam mater (no dagesh) for the "o" sound
        // When:  converting to a pronunciation guide
        // Then:  it reads "sha-lom"
        assert_eq!(niqud_to_pronunciation("שָׁלוֹם"), "sha-lom");
    }

    #[test]
    fn test_shlomkha_how_are_you() {
        // Given: niqud for שלומך ("your peace", as in "מה שלומך"), with
        //        two silent shvas (no vocal-shva marker) and a final kaf
        // When:  converting to a pronunciation guide
        // Then:  it reads "shlom-kha" — matching real phonikud-onnx output
        //        validated during implementation
        assert_eq!(niqud_to_pronunciation("שְׁלוֹמְךָ"), "shlom-kha");
    }

    #[test]
    fn test_empty_string_produces_empty_pronunciation() {
        // Given: an empty niqud string
        // When:  converting to a pronunciation guide
        // Then:  the result is empty rather than panicking
        assert_eq!(niqud_to_pronunciation(""), "");
    }
}
