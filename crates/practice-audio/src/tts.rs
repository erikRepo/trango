//! Text-to-speech for a word's translation, via the external `espeak-ng`
//! binary (`TODO.md` Vaihe 34) — not a Cargo dependency, same external-
//! process pattern as `whisper-cli`/`ffmpeg` elsewhere in this project.

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::PracticeAudioError;
use crate::process::{last_stderr_line, run_command};

/// Runs `espeak-ng` to synthesize spoken text to a WAV file.
pub struct EspeakTtsSynthesizer {
    /// Path or bare name of the `espeak-ng` binary. [`Default::default`]
    /// uses `"espeak-ng"`, resolved via `PATH`.
    pub binary_path: PathBuf,
}

impl Default for EspeakTtsSynthesizer {
    fn default() -> Self {
        Self {
            binary_path: PathBuf::from("espeak-ng"),
        }
    }
}

impl EspeakTtsSynthesizer {
    /// Synthesizes `text` as `voice` (an `espeak-ng` voice code, e.g.
    /// `"en"` — see [`espeak_voice_for_language`]) to a WAV file at
    /// `out_path` via `espeak-ng -v <voice> -w <out_path> "<text>"`.
    pub fn synthesize(
        &self,
        text: &str,
        voice: &str,
        out_path: &Path,
    ) -> Result<(), PracticeAudioError> {
        tracing::debug!(%text, %voice, ?out_path, binary = ?self.binary_path, "running espeak-ng");
        let output = run_command(
            Command::new(&self.binary_path)
                .arg("-v")
                .arg(voice)
                .arg("-w")
                .arg(out_path)
                .arg(text),
        )
        .map_err(|err| {
            if err.kind() == io::ErrorKind::NotFound {
                PracticeAudioError::GenerationFailed(format!(
                    "espeak-ng not found (looked for \"{}\"). Install espeak-ng and make sure \
                    it's on PATH, or set TRANGO_ESPEAK_PATH to its location — see \
                    docs/src/usage.",
                    self.binary_path.display()
                ))
            } else {
                PracticeAudioError::GenerationFailed(format!("failed to run espeak-ng: {err}"))
            }
        })?;

        if !output.status.success() {
            return Err(PracticeAudioError::GenerationFailed(format!(
                "espeak-ng exited with {}: {}",
                output.status,
                last_stderr_line(&output.stderr)
            )));
        }

        if !out_path.is_file() {
            return Err(PracticeAudioError::GenerationFailed(format!(
                "espeak-ng finished but no WAV file was found at {}",
                out_path.display()
            )));
        }

        Ok(())
    }
}

/// Known display-name → `espeak-ng` voice-code mappings, covering common
/// target languages. Not exhaustive — `word_analysis::DEFAULT_TARGET_LANGUAGE`
/// and the Open Subtitles dialog's language field are free-typed text,
/// not a controlled vocabulary, so [`espeak_voice_for_language`] falls
/// back to `"en"` for anything not listed here.
const LANGUAGE_VOICES: &[(&str, &str)] = &[
    ("english", "en"),
    ("finnish", "fi"),
    ("suomi", "fi"),
    ("swedish", "sv"),
    ("hebrew", "he"),
    ("german", "de"),
    ("french", "fr"),
    ("spanish", "es"),
    ("russian", "ru"),
    ("italian", "it"),
    ("portuguese", "pt"),
    ("dutch", "nl"),
    ("polish", "pl"),
    ("arabic", "ar"),
    ("japanese", "ja"),
    ("chinese", "zh"),
    ("korean", "ko"),
];

/// The `espeak-ng` `-v` voice code for `display_name` (case-insensitive
/// match against [`LANGUAGE_VOICES`]), or `"en"` with a `tracing::warn!`
/// if `display_name` isn't recognized — `espeak-ng` errors on an unknown
/// voice code, so an unrecognized language falls back to a working
/// default rather than failing the whole batch over one language name.
pub fn espeak_voice_for_language(display_name: &str) -> &'static str {
    let normalized = display_name.trim().to_lowercase();
    match LANGUAGE_VOICES.iter().find(|(name, _)| *name == normalized) {
        Some((_, voice)) => voice,
        None => {
            tracing::warn!(
                language = %display_name,
                "no known espeak-ng voice for this target language, falling back to \"en\""
            );
            "en"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_espeak_voice_for_language_matches_known_languages_case_insensitively() {
        // Given/When/Then: known language names map to their voice code
        //                   regardless of case
        assert_eq!(espeak_voice_for_language("English"), "en");
        assert_eq!(espeak_voice_for_language("FINNISH"), "fi");
        assert_eq!(espeak_voice_for_language("Hebrew"), "he");
    }

    #[test]
    fn test_espeak_voice_for_language_falls_back_to_en_for_unknown_language() {
        // Given: a language name not in the known list
        // When:  resolving its voice code
        // Then:  falls back to "en" rather than erroring
        assert_eq!(espeak_voice_for_language("Klingon"), "en");
    }

    /// Writes an executable POSIX shell script standing in for an
    /// external tool at `dir.join(name)` and returns its path — mirrors
    /// `subtitle::generate`'s `write_fake_binary`.
    #[cfg(unix)]
    fn write_fake_binary(dir: &Path, name: &str, script: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let script_path = dir.join(name);
        std::fs::write(&script_path, script).expect("failed to write fake binary script");
        std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755))
            .expect("failed to make fake binary script executable");
        script_path
    }

    fn test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("trango-test-tts-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("failed to create temp test dir");
        dir
    }

    #[test]
    #[cfg(unix)]
    fn test_synthesize_runs_espeak_ng_with_expected_flags() {
        // Given: a fake espeak-ng that logs its argv and writes the
        //        requested WAV file
        // When:  synthesizing text
        // Then:  -v <voice> -w <out_path> "<text>" were all passed, and
        //        the call succeeds
        let dir = test_dir("synthesize-flags");
        let binary_path = write_fake_binary(
            &dir,
            "fake-espeak-ng.sh",
            r#"#!/bin/sh
echo "$@" > "$0.args"
out=""
prev=""
for arg in "$@"; do
    if [ "$prev" = "-w" ]; then out="$arg"; fi
    prev="$arg"
done
printf 'fake wav content' > "$out"
"#,
        );
        let out_path = dir.join("word.wav");
        let synthesizer = EspeakTtsSynthesizer { binary_path };

        synthesizer.synthesize("hello", "en", &out_path).unwrap();

        assert!(out_path.is_file());

        std::fs::remove_dir_all(&dir).expect("failed to clean up temp test dir");
    }

    #[test]
    #[cfg(unix)]
    fn test_synthesize_errors_clearly_when_binary_is_missing() {
        // Given: a binary_path naming an espeak-ng that isn't installed
        // When:  synthesizing
        // Then:  GenerationFailed explains the binary is missing
        let dir = test_dir("missing-binary");
        let synthesizer = EspeakTtsSynthesizer {
            binary_path: dir.join("no-such-espeak-ng-binary"),
        };

        let result = synthesizer.synthesize("hello", "en", &dir.join("out.wav"));

        let Err(PracticeAudioError::GenerationFailed(message)) = result else {
            panic!("expected GenerationFailed, got {result:?}");
        };
        assert!(message.contains("espeak-ng not found"), "{message}");

        std::fs::remove_dir_all(&dir).expect("failed to clean up temp test dir");
    }
}
