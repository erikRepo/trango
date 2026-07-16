//! Capturing the system's own audio output as a WAV file.

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::error::AudioCaptureError;

/// Records the system's outgoing audio (e.g. a video playing in a
/// browser) to a WAV file via an `ffmpeg -f pulse` subprocess (`TODO.md`
/// Vaihe 26) — the same external-process pattern as
/// `subtitle::WhisperCliGenerator::extract_audio`, so no new Cargo
/// dependency was needed. Linux/PulseAudio-PipeWire only for now (see
/// `docs/src/developer/architecture/system-audio-capture.md`): `pactl` and
/// `ffmpeg -f pulse` have no equivalent wired up on Windows/macOS.
pub struct AudioCapture {
    /// Path or bare name of the `ffmpeg` binary used to capture audio.
    /// [`Default::default`] uses `"ffmpeg"`, resolved via `PATH`.
    pub ffmpeg_path: PathBuf,
    /// Path or bare name of the `pactl` binary used by
    /// [`AudioCapture::default_monitor_source`]. [`Default::default`] uses
    /// `"pactl"`, resolved via `PATH`.
    pub pactl_path: PathBuf,
    /// How long [`AudioCapture::stop`] waits for `ffmpeg` to exit on its
    /// own after asking it to quit gracefully, before falling back to
    /// killing it outright. [`Default::default`] uses 5 seconds; tests use
    /// a much shorter value so a stuck fake `ffmpeg` doesn't slow the
    /// suite down.
    pub graceful_stop_timeout: Duration,
    /// The running `ffmpeg` child process, if a capture is in progress.
    child: Option<Child>,
}

impl Default for AudioCapture {
    fn default() -> Self {
        Self {
            ffmpeg_path: PathBuf::from("ffmpeg"),
            pactl_path: PathBuf::from("pactl"),
            graceful_stop_timeout: Duration::from_secs(5),
            child: None,
        }
    }
}

impl AudioCapture {
    /// Whether a capture is currently running.
    pub fn is_recording(&self) -> bool {
        self.child.is_some()
    }

    /// Asks `pactl` for the system's default sink and returns its matching
    /// monitor source, `<sink>.monitor` — the PulseAudio/PipeWire
    /// convention for "whatever that sink is currently outputting", which
    /// is what needs to be fed to `ffmpeg -f pulse -i` to capture played-back
    /// audio rather than a microphone. Callers that find autodetection
    /// unreliable for their setup can bypass this and pass a
    /// user-configured source name straight to [`AudioCapture::start`]
    /// instead (`crates/app/src/config.rs`'s `audio_monitor_source`).
    pub fn default_monitor_source(&self) -> Result<String, AudioCaptureError> {
        tracing::debug!(pactl = ?self.pactl_path, "detecting default monitor source");
        let output =
            run_output(Command::new(&self.pactl_path).arg("get-default-sink")).map_err(|err| {
                if err.kind() == io::ErrorKind::NotFound {
                    AudioCaptureError::CaptureFailed(format!(
                        "pactl not found (looked for \"{}\"). Install PulseAudio/PipeWire's \
                        pactl utility, or set audio_monitor_source in config.toml instead of \
                        relying on autodetection — see docs/src/usage.",
                        self.pactl_path.display()
                    ))
                } else {
                    AudioCaptureError::CaptureFailed(format!("failed to run pactl: {err}"))
                }
            })?;

        if !output.status.success() {
            return Err(AudioCaptureError::CaptureFailed(format!(
                "pactl exited with {}: {}",
                output.status,
                last_stderr_line(&output.stderr)
            )));
        }

        let sink = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if sink.is_empty() {
            return Err(AudioCaptureError::CaptureFailed(
                "pactl get-default-sink reported no default sink".to_string(),
            ));
        }
        Ok(format!("{sink}.monitor"))
    }

    /// Starts capturing `monitor_source` to `output_path` as a 16kHz mono
    /// 16-bit PCM WAV file (the same format `subtitle::WhisperCliGenerator`
    /// extracts for `whisper-cli`, since this capture is meant to feed the
    /// same pipeline later). Returns [`AudioCaptureError::AlreadyRunning`]
    /// if a capture is already in progress — call [`AudioCapture::stop`]
    /// first.
    pub fn start(
        &mut self,
        monitor_source: &str,
        output_path: &Path,
    ) -> Result<(), AudioCaptureError> {
        if self.child.is_some() {
            return Err(AudioCaptureError::AlreadyRunning);
        }

        tracing::info!(
            monitor_source,
            ?output_path,
            ffmpeg = ?self.ffmpeg_path,
            "starting system audio capture"
        );
        let mut command = Command::new(&self.ffmpeg_path);
        command
            .arg("-y")
            .arg("-f")
            .arg("pulse")
            .arg("-i")
            .arg(monitor_source)
            .arg("-ar")
            .arg("16000")
            .arg("-ac")
            .arg("1")
            .arg("-c:a")
            .arg("pcm_s16le")
            .arg(output_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        let child = run_spawn(&mut command).map_err(|err| {
            if err.kind() == io::ErrorKind::NotFound {
                AudioCaptureError::CaptureFailed(format!(
                    "ffmpeg not found (looked for \"{}\"). Install ffmpeg and make sure \
                        it's on PATH, or set TRANGO_FFMPEG_PATH to its location — see \
                        docs/src/usage.",
                    self.ffmpeg_path.display()
                ))
            } else {
                AudioCaptureError::CaptureFailed(format!("failed to start ffmpeg: {err}"))
            }
        })?;

        self.child = Some(child);
        Ok(())
    }

    /// Stops the running capture, returning [`AudioCaptureError::NotRunning`]
    /// if none is in progress. Asks `ffmpeg` to quit gracefully by writing
    /// `q` to its stdin — the same key it reads interactively — so it
    /// finalizes the WAV file's header correctly instead of leaving it
    /// truncated; if it hasn't exited by [`Self::graceful_stop_timeout`],
    /// it's killed outright.
    pub fn stop(&mut self) -> Result<(), AudioCaptureError> {
        let mut child = self.child.take().ok_or(AudioCaptureError::NotRunning)?;
        tracing::info!("stopping system audio capture");

        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(b"q");
        }

        let deadline = Instant::now() + self.graceful_stop_timeout;
        loop {
            match child.try_wait() {
                Ok(Some(_status)) => return Ok(()),
                Ok(None) if Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(20));
                }
                Ok(None) => {
                    tracing::warn!("ffmpeg did not exit after quit signal, killing it");
                    let _ = child.kill();
                    return child.wait().map(|_| ()).map_err(|err| {
                        AudioCaptureError::CaptureFailed(format!(
                            "failed to wait for ffmpeg after killing it: {err}"
                        ))
                    });
                }
                Err(err) => {
                    return Err(AudioCaptureError::CaptureFailed(format!(
                        "failed to wait for ffmpeg: {err}"
                    )))
                }
            }
        }
    }
}

/// Runs `command.output()`, retrying briefly (up to 4 times, 20ms apart) if
/// the OS reports `ExecutableFileBusy` (errno `ETXTBSY`) — a transient race
/// that can happen if the target binary was written to disk moments
/// earlier (its write handle not fully released yet when exec is
/// attempted; this crate's own tests hit it occasionally, writing a fresh
/// fake binary immediately before running it), mirroring
/// `subtitle::generate`'s `run_command`.
fn run_output(command: &mut Command) -> io::Result<std::process::Output> {
    for attempt in 0..5 {
        match command.output() {
            Err(err) if attempt < 4 && err.kind() == io::ErrorKind::ExecutableFileBusy => {
                thread::sleep(Duration::from_millis(20));
            }
            result => return result,
        }
    }
    unreachable!()
}

/// Same retry as [`run_output`], for `command.spawn()` — used by
/// [`AudioCapture::start`], which needs the long-running `Child` rather
/// than a completed `Output`.
fn run_spawn(command: &mut Command) -> io::Result<Child> {
    for attempt in 0..5 {
        match command.spawn() {
            Err(err) if attempt < 4 && err.kind() == io::ErrorKind::ExecutableFileBusy => {
                thread::sleep(Duration::from_millis(20));
            }
            result => return result,
        }
    }
    unreachable!()
}

/// The last non-empty line of `stderr` — external tools' real error tends
/// to be the final line after loader/setup chatter, so showing just that
/// keeps `CaptureFailed`'s message readable instead of dumping the whole
/// log.
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

    /// A fresh temp dir for a test, named after `name` — used as a place
    /// to write fake `pactl`/`ffmpeg` binaries and their output files.
    fn test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("trango-test-audio-capture-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("failed to create temp test dir");
        dir
    }

    /// Writes an executable POSIX shell script standing in for an external
    /// tool at `dir.join(name)` and returns its path — real `pactl`/
    /// `ffmpeg` behavior isn't something CI/dev machines can rely on
    /// having installed (or having a PulseAudio session to talk to), so
    /// these tests exercise `AudioCapture`'s actual `Command` plumbing
    /// against fake binaries instead, the same approach
    /// `subtitle::generate`'s tests use for `whisper-cli`/`ffmpeg`.
    #[cfg(unix)]
    fn write_fake_binary(dir: &Path, name: &str, script: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let script_path = dir.join(name);
        std::fs::write(&script_path, script).expect("failed to write fake binary script");
        std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755))
            .expect("failed to make fake binary script executable");
        script_path
    }

    #[test]
    #[cfg(unix)]
    fn test_default_monitor_source_appends_monitor_suffix_to_default_sink() {
        // Given: a fake pactl that prints a sink name
        // When:  detecting the default monitor source
        // Then:  it's that sink name with ".monitor" appended
        let dir = test_dir("default-monitor-source");
        let pactl_path = write_fake_binary(
            &dir,
            "fake-pactl.sh",
            "#!/bin/sh\necho 'alsa_output.pci-0000_00_1f.3.analog-stereo'\n",
        );
        let capture = AudioCapture {
            pactl_path,
            ..AudioCapture::default()
        };

        let source = capture.default_monitor_source().unwrap();

        assert_eq!(source, "alsa_output.pci-0000_00_1f.3.analog-stereo.monitor");

        std::fs::remove_dir_all(dir).expect("failed to clean up temp test dir");
    }

    #[test]
    #[cfg(unix)]
    fn test_default_monitor_source_errors_clearly_when_pactl_is_missing() {
        // Given: a pactl_path naming a binary that isn't installed
        // When:  detecting the default monitor source
        // Then:  CaptureFailed explains pactl is missing
        let dir = test_dir("missing-pactl");
        let capture = AudioCapture {
            pactl_path: dir.join("no-such-pactl-binary"),
            ..AudioCapture::default()
        };

        let result = capture.default_monitor_source();

        let Err(AudioCaptureError::CaptureFailed(message)) = result else {
            panic!("expected CaptureFailed, got {result:?}");
        };
        assert!(message.contains("pactl not found"), "{message}");

        std::fs::remove_dir_all(dir).expect("failed to clean up temp test dir");
    }

    #[test]
    #[cfg(unix)]
    fn test_default_monitor_source_errors_when_pactl_reports_no_sink() {
        // Given: a fake pactl that succeeds but prints nothing
        // When:  detecting the default monitor source
        // Then:  CaptureFailed explains no default sink was reported
        let dir = test_dir("no-default-sink");
        let pactl_path = write_fake_binary(&dir, "fake-pactl.sh", "#!/bin/sh\nexit 0\n");
        let capture = AudioCapture {
            pactl_path,
            ..AudioCapture::default()
        };

        let result = capture.default_monitor_source();

        let Err(AudioCaptureError::CaptureFailed(message)) = result else {
            panic!("expected CaptureFailed, got {result:?}");
        };
        assert!(message.contains("no default sink"), "{message}");

        std::fs::remove_dir_all(dir).expect("failed to clean up temp test dir");
    }

    /// A fake `ffmpeg` that logs its argv to `"<output path>.args"`, writes
    /// a marker to the output path, then blocks reading a line from stdin
    /// (mirroring real `ffmpeg`'s graceful-quit-on-`q` behavior) before
    /// exiting.
    #[cfg(unix)]
    const FAKE_FFMPEG_CAPTURE_SCRIPT: &str = r#"#!/bin/sh
last=""
for arg in "$@"; do
    last="$arg"
done
echo "$@" > "${last}.args"
printf 'fake wav content' > "$last"
read -r _line
exit 0
"#;

    #[test]
    #[cfg(unix)]
    fn test_start_runs_ffmpeg_with_expected_flags_and_stop_waits_for_graceful_exit() {
        // Given: a fake ffmpeg (see FAKE_FFMPEG_CAPTURE_SCRIPT) started
        //        against a monitor source and output path
        // When:  starting, then stopping the capture
        // Then:  ffmpeg was invoked with -f pulse -i <source>, the 16kHz
        //        mono PCM flags, and the output path; stop() returns Ok
        //        once the fake process reads the quit signal and exits
        let dir = test_dir("start-stop-happy-path");
        let output_path = dir.join("captured.wav");
        let ffmpeg_path = write_fake_binary(&dir, "fake-ffmpeg.sh", FAKE_FFMPEG_CAPTURE_SCRIPT);
        let mut capture = AudioCapture {
            ffmpeg_path,
            graceful_stop_timeout: Duration::from_millis(500),
            ..AudioCapture::default()
        };

        capture
            .start("alsa_output.analog-stereo.monitor", &output_path)
            .unwrap();
        assert!(capture.is_recording());

        capture.stop().unwrap();
        assert!(!capture.is_recording());

        let logged_args = std::fs::read_to_string(format!("{}.args", output_path.display()))
            .expect("fake ffmpeg should have logged its args");
        assert!(logged_args.contains("-f pulse"));
        assert!(logged_args.contains("-i alsa_output.analog-stereo.monitor"));
        assert!(logged_args.contains("-ar 16000"));
        assert!(logged_args.contains("-ac 1"));
        assert!(logged_args.contains("pcm_s16le"));
        assert_eq!(
            std::fs::read_to_string(&output_path).unwrap(),
            "fake wav content"
        );

        std::fs::remove_dir_all(dir).expect("failed to clean up temp test dir");
    }

    #[test]
    #[cfg(unix)]
    fn test_start_errors_clearly_when_ffmpeg_is_missing() {
        // Given: an ffmpeg_path naming a binary that isn't installed
        // When:  starting a capture
        // Then:  CaptureFailed explains ffmpeg is missing, and no capture
        //        is left "running"
        let dir = test_dir("missing-ffmpeg");
        let mut capture = AudioCapture {
            ffmpeg_path: dir.join("no-such-ffmpeg-binary"),
            ..AudioCapture::default()
        };

        let result = capture.start("some.monitor", &dir.join("out.wav"));

        let Err(AudioCaptureError::CaptureFailed(message)) = result else {
            panic!("expected CaptureFailed, got {result:?}");
        };
        assert!(message.contains("ffmpeg not found"), "{message}");
        assert!(!capture.is_recording());

        std::fs::remove_dir_all(dir).expect("failed to clean up temp test dir");
    }

    #[test]
    #[cfg(unix)]
    fn test_start_twice_errors_already_running() {
        // Given: a capture already started against a fake ffmpeg that
        //        blocks on stdin
        // When:  starting a second capture without stopping the first
        // Then:  AlreadyRunning is returned, and the first process is
        //        still the one running (cleaned up via stop() after)
        let dir = test_dir("start-twice");
        let ffmpeg_path = write_fake_binary(&dir, "fake-ffmpeg.sh", FAKE_FFMPEG_CAPTURE_SCRIPT);
        let mut capture = AudioCapture {
            ffmpeg_path,
            graceful_stop_timeout: Duration::from_millis(500),
            ..AudioCapture::default()
        };
        capture
            .start("some.monitor", &dir.join("out1.wav"))
            .unwrap();

        let result = capture.start("some.monitor", &dir.join("out2.wav"));

        assert!(matches!(result, Err(AudioCaptureError::AlreadyRunning)));
        capture.stop().unwrap();

        std::fs::remove_dir_all(dir).expect("failed to clean up temp test dir");
    }

    #[test]
    fn test_stop_without_start_errors_not_running() {
        // Given: a fresh AudioCapture that was never started
        // When:  stopping it
        // Then:  NotRunning is returned
        let mut capture = AudioCapture::default();

        let result = capture.stop();

        assert!(matches!(result, Err(AudioCaptureError::NotRunning)));
    }

    #[test]
    #[cfg(unix)]
    fn test_stop_kills_process_that_ignores_the_quit_signal() {
        // Given: a fake ffmpeg that never reads stdin and sleeps well
        //        past the (short, test-only) graceful_stop_timeout
        // When:  stopping the capture
        // Then:  stop() still returns Ok, having killed the process
        //        instead of hanging forever
        let dir = test_dir("stop-kills-stuck-process");
        let ffmpeg_path = write_fake_binary(&dir, "fake-ffmpeg.sh", "#!/bin/sh\nsleep 30\n");
        let mut capture = AudioCapture {
            ffmpeg_path,
            graceful_stop_timeout: Duration::from_millis(100),
            ..AudioCapture::default()
        };
        capture.start("some.monitor", &dir.join("out.wav")).unwrap();

        capture.stop().unwrap();

        assert!(!capture.is_recording());

        std::fs::remove_dir_all(dir).expect("failed to clean up temp test dir");
    }
}
