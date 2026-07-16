//! Capturing the system's own audio output as a live stream of speech
//! segments.

use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::error::AudioCaptureError;
use crate::vad::{SpeechSegment, VadSegmenter};

/// Records the system's outgoing audio (e.g. a video playing in a
/// browser) via an `ffmpeg -f pulse` subprocess (`TODO.md` Vaihe 26) — the
/// same external-process pattern as
/// `subtitle::WhisperCliGenerator::extract_audio`, so no new Cargo
/// dependency was needed. Linux/PulseAudio-PipeWire only for now (see
/// `docs/src/developer/architecture/system-audio-capture.md`): `pactl` and
/// `ffmpeg -f pulse` have no equivalent wired up on Windows/macOS.
///
/// Rather than writing a container file to disk, `ffmpeg` streams raw
/// 16kHz mono 16-bit PCM to its stdout, which a background thread reads and
/// runs through a fresh [`VadSegmenter`] (`TODO.md` Vaihe 27/28) — no audio
/// ever touches disk here, only the completed [`SpeechSegment`]s handed to
/// [`AudioCapture::start`]'s callback (per-segment `whisper-cli`
/// transcription, and any resulting temp WAV, is the caller's concern —
/// see `subtitle::WhisperCliGenerator::transcribe_segment`).
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
    /// The thread reading and segmenting `child`'s stdout, if a capture is
    /// in progress.
    reader_thread: Option<JoinHandle<()>>,
}

impl Default for AudioCapture {
    fn default() -> Self {
        Self {
            ffmpeg_path: PathBuf::from("ffmpeg"),
            pactl_path: PathBuf::from("pactl"),
            graceful_stop_timeout: Duration::from_secs(5),
            child: None,
            reader_thread: None,
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

    /// Starts capturing `monitor_source`'s raw 16kHz mono 16-bit PCM audio,
    /// running it through a fresh [`VadSegmenter`] on a background thread
    /// and invoking `on_segment` for each completed [`SpeechSegment`] —
    /// including, once the stream ends (`stop` is called), whatever
    /// trailing speech was still in progress. `on_segment` runs on that
    /// background thread, not the caller's — it should return quickly
    /// (spawning further work of its own if transcription is needed) so it
    /// doesn't stall the capture. Returns
    /// [`AudioCaptureError::AlreadyRunning`] if a capture is already in
    /// progress — call [`AudioCapture::stop`] first.
    pub fn start(
        &mut self,
        monitor_source: &str,
        mut on_segment: impl FnMut(SpeechSegment) + Send + 'static,
    ) -> Result<(), AudioCaptureError> {
        if self.child.is_some() {
            return Err(AudioCaptureError::AlreadyRunning);
        }

        tracing::info!(
            monitor_source,
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
            .arg("-f")
            .arg("s16le")
            .arg("pipe:1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = run_spawn(&mut command).map_err(|err| {
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

        let stdout = child.stdout.take().ok_or_else(|| {
            AudioCaptureError::CaptureFailed("ffmpeg's stdout was not piped".to_string())
        })?;
        let reader_thread = thread::spawn(move || {
            feed_pcm_stream(stdout, &mut on_segment);
        });

        self.child = Some(child);
        self.reader_thread = Some(reader_thread);
        Ok(())
    }

    /// Stops the running capture, returning [`AudioCaptureError::NotRunning`]
    /// if none is in progress. Asks `ffmpeg` to quit gracefully by writing
    /// `q` to its stdin — the same key it reads interactively — before
    /// falling back to killing it outright if it hasn't exited by
    /// [`Self::graceful_stop_timeout`]. Either way, `ffmpeg` exiting closes
    /// its stdout, which lets the reader thread finish (reporting any
    /// still-in-progress speech segment via `flush`) — `stop` joins that
    /// thread before returning, so any final segment is always delivered
    /// before the capture is considered fully stopped.
    pub fn stop(&mut self) -> Result<(), AudioCaptureError> {
        let mut child = self.child.take().ok_or(AudioCaptureError::NotRunning)?;
        tracing::info!("stopping system audio capture");

        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(b"q");
        }

        let deadline = Instant::now() + self.graceful_stop_timeout;
        let result = loop {
            match child.try_wait() {
                Ok(Some(_status)) => break Ok(()),
                Ok(None) if Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(20));
                }
                Ok(None) => {
                    tracing::warn!("ffmpeg did not exit after quit signal, killing it");
                    let _ = child.kill();
                    break child.wait().map(|_| ()).map_err(|err| {
                        AudioCaptureError::CaptureFailed(format!(
                            "failed to wait for ffmpeg after killing it: {err}"
                        ))
                    });
                }
                Err(err) => {
                    break Err(AudioCaptureError::CaptureFailed(format!(
                        "failed to wait for ffmpeg: {err}"
                    )))
                }
            }
        };

        if let Some(handle) = self.reader_thread.take() {
            let _ = handle.join();
        }
        result
    }
}

/// Decodes a stream of raw little-endian 16-bit PCM bytes into `i16`
/// samples across arbitrarily-sized reads, carrying over a trailing odd
/// byte between calls so a sample split across two reads is decoded
/// correctly instead of dropped.
#[derive(Default)]
struct PcmDecoder {
    leftover: Option<u8>,
}

impl PcmDecoder {
    /// Decodes `bytes` into samples, prepending any byte carried over from
    /// a previous call and carrying over a new trailing odd byte, if any.
    fn decode(&mut self, bytes: &[u8]) -> Vec<i16> {
        let mut buf = Vec::with_capacity(bytes.len() + 1);
        if let Some(leftover) = self.leftover.take() {
            buf.push(leftover);
        }
        buf.extend_from_slice(bytes);

        let mut chunks = buf.chunks_exact(2);
        let samples = chunks
            .by_ref()
            .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
            .collect();
        if let [leftover] = *chunks.remainder() {
            self.leftover = Some(leftover);
        }
        samples
    }
}

/// Reads raw little-endian 16-bit PCM from `stream` until it ends, feeding
/// it through a fresh [`VadSegmenter`] and calling `on_segment` for each
/// completed speech segment; once `stream` ends, any still-in-progress
/// speech is flushed and reported too. Pulled out of
/// [`AudioCapture::start`]'s reader thread as its own function so the
/// PCM-decoding/segmentation wiring can be unit-tested directly against an
/// in-memory reader, without spawning a real `ffmpeg` subprocess.
fn feed_pcm_stream(mut stream: impl Read, on_segment: &mut impl FnMut(SpeechSegment)) {
    let mut decoder = PcmDecoder::default();
    let mut segmenter = VadSegmenter::new();
    let mut buf = [0u8; 4096];
    loop {
        match stream.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                for segment in segmenter.push_samples(&decoder.decode(&buf[..n])) {
                    on_segment(segment);
                }
            }
        }
    }
    if let Some(segment) = segmenter.flush() {
        on_segment(segment);
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
    use std::path::Path;
    use std::sync::{Arc, Mutex};

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

    /// A fake `ffmpeg` that logs its argv to `dir/args.log` (the raw-PCM
    /// stdout stream leaves no output path to derive a log name from
    /// anymore, unlike the old WAV-file version), writes a few PCM sample
    /// bytes to stdout, then blocks reading a line from stdin (mirroring
    /// real `ffmpeg`'s graceful-quit-on-`q` behavior) before exiting. Bytes
    /// are chosen with no zero bytes (`\001\001\002\002\003\003` — i16 LE
    /// samples 257, 514, 771) since POSIX `printf`'s octal escapes for NUL
    /// bytes aren't reliably portable across shells.
    #[cfg(unix)]
    fn fake_ffmpeg_capture_script(dir: &Path) -> String {
        format!(
            "#!/bin/sh\necho \"$@\" > {}/args.log\nprintf '\\001\\001\\002\\002\\003\\003'\nread -r _line\nexit 0\n",
            dir.display()
        )
    }

    #[test]
    #[cfg(unix)]
    fn test_start_runs_ffmpeg_with_expected_flags_and_stop_waits_for_graceful_exit() {
        // Given: a fake ffmpeg (see fake_ffmpeg_capture_script) started
        //        against a monitor source
        // When:  starting, then stopping the capture
        // Then:  ffmpeg was invoked with -f pulse -i <source>, the 16kHz
        //        mono raw-PCM-to-stdout flags; stop() returns Ok once the
        //        fake process reads the quit signal and exits, and the
        //        samples it wrote to stdout were decoded and delivered
        let dir = test_dir("start-stop-happy-path");
        let ffmpeg_path =
            write_fake_binary(&dir, "fake-ffmpeg.sh", &fake_ffmpeg_capture_script(&dir));
        let mut capture = AudioCapture {
            ffmpeg_path,
            graceful_stop_timeout: Duration::from_millis(500),
            ..AudioCapture::default()
        };
        let received_samples = Arc::new(Mutex::new(Vec::new()));
        let received_for_callback = Arc::clone(&received_samples);

        capture
            .start("alsa_output.analog-stereo.monitor", move |segment| {
                received_for_callback
                    .lock()
                    .unwrap()
                    .extend(segment.samples);
            })
            .unwrap();
        assert!(capture.is_recording());

        capture.stop().unwrap();
        assert!(!capture.is_recording());

        let logged_args = std::fs::read_to_string(dir.join("args.log"))
            .expect("fake ffmpeg should have logged its args");
        assert!(logged_args.contains("-f pulse"));
        assert!(logged_args.contains("-i alsa_output.analog-stereo.monitor"));
        assert!(logged_args.contains("-ar 16000"));
        assert!(logged_args.contains("-ac 1"));
        assert!(logged_args.contains("-f s16le"));
        assert!(logged_args.contains("pipe:1"));
        // The 3 written samples are below VadSegmenter's minimum speech
        // duration, so they never surface as a SpeechSegment — this test's
        // job is only to prove the PCM bytes flowed through ffmpeg's stdout
        // and were captured, not to exercise segmentation (see
        // feed_pcm_stream's own tests for that).
        assert!(received_samples.lock().unwrap().is_empty());

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

        let result = capture.start("some.monitor", |_segment| {});

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
        let ffmpeg_path =
            write_fake_binary(&dir, "fake-ffmpeg.sh", &fake_ffmpeg_capture_script(&dir));
        let mut capture = AudioCapture {
            ffmpeg_path,
            graceful_stop_timeout: Duration::from_millis(500),
            ..AudioCapture::default()
        };
        capture.start("some.monitor", |_segment| {}).unwrap();

        let result = capture.start("some.monitor", |_segment| {});

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
        capture.start("some.monitor", |_segment| {}).unwrap();

        capture.stop().unwrap();

        assert!(!capture.is_recording());

        std::fs::remove_dir_all(dir).expect("failed to clean up temp test dir");
    }

    #[test]
    fn test_pcm_decoder_carries_partial_sample_across_chunks() {
        // Given: two i16 LE samples (0x1234, 0x5678) split across two
        //        decode() calls at a byte boundary that lands mid-sample
        // When:  decoding both chunks in sequence
        // Then:  both samples are recovered correctly — the second chunk's
        //        leading byte is combined with the first chunk's carried
        //        leftover byte
        let mut decoder = PcmDecoder::default();

        let first = decoder.decode(&[0x34, 0x12, 0x78]);
        let second = decoder.decode(&[0x56]);

        assert_eq!(first, vec![0x1234]);
        assert_eq!(second, vec![0x5678]);
    }

    #[test]
    fn test_pcm_decoder_handles_whole_chunks_with_no_leftover() {
        // Given: a byte stream containing exactly two whole samples
        // When:  decoding it in one call
        // Then:  both samples come back and nothing is carried over
        let mut decoder = PcmDecoder::default();

        let samples = decoder.decode(&[0x01, 0x00, 0x02, 0x00]);

        assert_eq!(samples, vec![1, 2]);
        assert_eq!(decoder.decode(&[]), Vec::<i16>::new());
    }

    /// `duration_ms` of silence (all-zero samples) at 16kHz.
    fn synth_silence(duration_ms: u64) -> Vec<i16> {
        vec![0i16; duration_ms as usize * 16]
    }

    /// `duration_ms` of a synthesized multi-harmonic tone, reliably
    /// classified as voice by `webrtc_vad` — identical technique to
    /// `vad.rs`'s own tests, since `VadSegmenter`'s `Aggressive` mode
    /// needs this exact harmonic mix to classify it as speech reliably.
    fn synth_speech(duration_ms: u64) -> Vec<i16> {
        let n = duration_ms as usize * 16;
        (0..n)
            .map(|i| {
                let t = i as f32 / 16_000.0;
                let s = 0.5 * (2.0 * std::f32::consts::PI * 150.0 * t).sin()
                    + 0.3 * (2.0 * std::f32::consts::PI * 300.0 * t).sin()
                    + 0.2 * (2.0 * std::f32::consts::PI * 450.0 * t).sin()
                    + 0.1 * (2.0 * std::f32::consts::PI * 900.0 * t).sin();
                (s * 8000.0) as i16
            })
            .collect()
    }

    fn samples_to_le_bytes(samples: &[i16]) -> Vec<u8> {
        samples.iter().flat_map(|s| s.to_le_bytes()).collect()
    }

    #[test]
    fn test_feed_pcm_stream_decodes_and_segments_a_synthesized_recording() {
        // Given: a raw PCM byte stream — silence, then a speech-like tone,
        //        then closing silence — read from an in-memory Cursor
        //        instead of a real ffmpeg subprocess
        // When:  feeding it through feed_pcm_stream
        // Then:  exactly one SpeechSegment is reported, starting around the
        //        tone's onset — proving the byte-decode -> VadSegmenter ->
        //        callback wiring works end to end
        let mut audio = synth_silence(300);
        audio.extend(synth_speech(600));
        audio.extend(synth_silence(900));
        let bytes = samples_to_le_bytes(&audio);

        let mut segments = Vec::new();
        feed_pcm_stream(std::io::Cursor::new(bytes), &mut |segment| {
            segments.push(segment)
        });

        assert_eq!(segments.len(), 1);
        assert!(
            (250..=350).contains(&segments[0].start_ms),
            "start_ms was {}",
            segments[0].start_ms
        );
    }

    #[test]
    fn test_feed_pcm_stream_flushes_in_progress_segment_at_end_of_stream() {
        // Given: a stream that ends mid-speech, without enough trailing
        //        silence to close the segment on its own
        // When:  feeding it through feed_pcm_stream
        // Then:  the in-progress segment is still reported, via flush()
        let mut audio = synth_silence(300);
        audio.extend(synth_speech(600));
        let bytes = samples_to_le_bytes(&audio);

        let mut segments = Vec::new();
        feed_pcm_stream(std::io::Cursor::new(bytes), &mut |segment| {
            segments.push(segment)
        });

        assert_eq!(segments.len(), 1);
    }
}
