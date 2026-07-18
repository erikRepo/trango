//! Runs a small external CLI wrapper (`tools/niqud-cli/`) around
//! [Phonikud](https://github.com/thewh1teagle/phonikud) as a subprocess —
//! not a Cargo dependency, mirroring how `subtitle::WhisperCliGenerator`
//! drives `whisper-cli`. Phonikud itself has no CLI, hence the wrapper;
//! see `docs/src/developer/specs.md`'s "Hebrew pronunciation" entry.

use std::io;
use std::path::PathBuf;
use std::process::Command;

use crate::cli_output::parse_cli_output;
use crate::client::NiqudClient;
use crate::entry::NiqudResult;
use crate::error::NiqudError;

/// A [`NiqudClient`] that runs the niqud CLI wrapper as a short-lived
/// subprocess per sentence: `<binary> "<sentence>"`, expecting the
/// `{"words": [{"word", "niqud"}]}` JSON shape on stdout and a non-zero
/// exit status with an explanatory stderr message on failure.
pub struct PhonikudCliClient {
    /// Path or bare name of the niqud CLI wrapper binary/script.
    /// [`Default::default`] uses `"trango-niqud-cli"`, resolved via `PATH`.
    pub binary_path: PathBuf,
}

impl Default for PhonikudCliClient {
    fn default() -> Self {
        Self {
            binary_path: PathBuf::from("trango-niqud-cli"),
        }
    }
}

impl NiqudClient for PhonikudCliClient {
    fn transliterate_sentence(&self, sentence: &str) -> Result<NiqudResult, NiqudError> {
        tracing::debug!(binary = ?self.binary_path, %sentence, "running niqud CLI");
        let output = run_command(Command::new(&self.binary_path).arg(sentence)).map_err(|err| {
            if err.kind() == io::ErrorKind::NotFound {
                NiqudError::ProcessFailed(format!(
                    "niqud CLI not found (looked for \"{}\"). Install it and make sure it's \
                     on PATH, or set TRANGO_NIQUD_CLI_PATH to its location — see \
                     tools/niqud-cli/README.md.",
                    self.binary_path.display()
                ))
            } else {
                NiqudError::ProcessFailed(format!("failed to run niqud CLI: {err}"))
            }
        })?;

        if !output.status.success() {
            return Err(NiqudError::ProcessFailed(format!(
                "niqud CLI exited with {}: {}",
                output.status,
                last_stderr_line(&output.stderr)
            )));
        }

        parse_cli_output(&String::from_utf8_lossy(&output.stdout))
    }
}

/// Runs `command`, retrying briefly (up to 4 times, 20ms apart) if the OS
/// reports `ExecutableFileBusy` (errno `ETXTBSY`) — the same transient
/// race `subtitle::generate`'s `run_command` guards against, relevant
/// here too since this crate's own tests write a fresh fake binary
/// immediately before running it.
fn run_command(command: &mut Command) -> io::Result<std::process::Output> {
    for attempt in 0..5 {
        match command.output() {
            Err(err) if attempt < 4 && err.kind() == io::ErrorKind::ExecutableFileBusy => {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            result => return result,
        }
    }
    unreachable!()
}

/// The last non-empty line of `stderr` — mirrors
/// `subtitle::generate::last_stderr_line`.
fn last_stderr_line(stderr: &[u8]) -> String {
    let stderr = String::from_utf8_lossy(stderr);
    stderr
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("no error output")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Writes an executable POSIX shell script standing in for the niqud
    /// CLI wrapper at `dir.join(name)` and returns its path — mirrors
    /// `subtitle::generate`'s `write_fake_binary`.
    #[cfg(unix)]
    fn write_fake_binary(dir: &std::path::Path, name: &str, script: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let script_path = dir.join(name);
        std::fs::write(&script_path, script).expect("failed to write fake binary script");
        std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755))
            .expect("failed to make fake binary script executable");
        script_path
    }

    fn temp_test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("trango-test-niqud-process-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("failed to create temp test dir");
        dir
    }

    #[test]
    #[cfg(unix)]
    fn test_transliterate_sentence_parses_stdout_with_expected_argv() {
        // Given: a fake niqud CLI that logs its argv and writes a fixed
        //        JSON response to stdout
        // When:  transliterating a sentence
        // Then:  the sentence was passed as the sole argument, and the
        //        parsed result carries the derived pronunciation
        let dir = temp_test_dir("parses-stdout");
        let binary_path = write_fake_binary(
            &dir,
            "fake-niqud-cli.sh",
            r#"#!/bin/sh
echo "$@" > "$(dirname "$0")/argv.log"
printf '{"words":[{"word":"שכב","niqud":"שָׁכַב"}]}'
"#,
        );
        let client = PhonikudCliClient { binary_path };

        let result = client.transliterate_sentence("שכב").unwrap();

        assert_eq!(result.words.len(), 1);
        assert_eq!(result.words[0].pronunciation, "sha-khav");
        let logged_argv =
            std::fs::read_to_string(dir.join("argv.log")).expect("fake CLI should log its argv");
        assert_eq!(logged_argv.trim(), "שכב");

        std::fs::remove_dir_all(dir).expect("failed to clean up temp test dir");
    }

    #[test]
    #[cfg(unix)]
    fn test_transliterate_sentence_errors_clearly_when_binary_is_missing() {
        // Given: a binary_path naming a niqud CLI that isn't installed
        // When:  transliterating a sentence
        // Then:  ProcessFailed explains the binary is missing, not a
        //        generic I/O error
        let dir = temp_test_dir("missing-binary");
        let client = PhonikudCliClient {
            binary_path: dir.join("no-such-niqud-cli"),
        };

        let result = client.transliterate_sentence("שכב");

        let Err(NiqudError::ProcessFailed(message)) = result else {
            panic!("expected ProcessFailed, got {result:?}");
        };
        assert!(message.contains("niqud CLI not found"), "{message}");

        std::fs::remove_dir_all(dir).expect("failed to clean up temp test dir");
    }

    #[test]
    #[cfg(unix)]
    fn test_transliterate_sentence_reports_stderr_when_process_fails() {
        // Given: a fake niqud CLI that exits non-zero with a stderr message
        // When:  transliterating a sentence
        // Then:  ProcessFailed carries that stderr message
        let dir = temp_test_dir("process-fails");
        let binary_path = write_fake_binary(
            &dir,
            "fake-niqud-cli.sh",
            "#!/bin/sh\necho 'model file not found' >&2\nexit 1\n",
        );
        let client = PhonikudCliClient { binary_path };

        let result = client.transliterate_sentence("שכב");

        let Err(NiqudError::ProcessFailed(message)) = result else {
            panic!("expected ProcessFailed, got {result:?}");
        };
        assert!(message.contains("model file not found"), "{message}");

        std::fs::remove_dir_all(dir).expect("failed to clean up temp test dir");
    }

    #[test]
    #[cfg(unix)]
    fn test_transliterate_sentence_errors_when_stdout_is_not_valid_json() {
        // Given: a fake niqud CLI that exits 0 but writes garbage to stdout
        // When:  transliterating a sentence
        // Then:  an InvalidResponse error comes back, not a panic
        let dir = temp_test_dir("invalid-json");
        let binary_path = write_fake_binary(
            &dir,
            "fake-niqud-cli.sh",
            "#!/bin/sh\necho 'not json at all'\n",
        );
        let client = PhonikudCliClient { binary_path };

        let result = client.transliterate_sentence("שכב");

        assert!(matches!(result, Err(NiqudError::InvalidResponse(_))));

        std::fs::remove_dir_all(dir).expect("failed to clean up temp test dir");
    }
}
