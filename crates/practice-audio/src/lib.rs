//! Assembles per-sentence pronunciation-practice audio (`TODO.md`
//! Vaihe 34): for each word in a sentence, a TTS translation followed by
//! that word's real audio at 50%/75%/100% speed (twice each), then the
//! whole sentence's real audio three times at normal speed — all
//! separated by dynamic pauses, concatenated into one `.mp3`.
//!
//! Generic — no knowledge of `subtitle`/`word-analysis` types, so this
//! crate doesn't depend on either; the `app` crate composes them.

mod error;
mod pieces;
mod process;
mod sentence;
mod tts;

pub use error::PracticeAudioError;
pub use sentence::{PracticeAudioBuilder, SentencePracticeAudioRequest, WordPracticeSpec};
pub use tts::{espeak_voice_for_language, EspeakTtsSynthesizer};
