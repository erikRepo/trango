//! Voice-activity-detection segmentation of a continuous audio stream
//! (`TODO.md` Vaihe 27) — chops 16kHz mono PCM samples into speech
//! segments at pauses, so `TODO.md` Vaihe 28's per-segment `whisper-cli`
//! transcription gets sentence-sized chunks instead of a fixed sliding
//! window (which would cut sentences mid-word at arbitrary boundaries).
//! Uses `webrtc_vad` (WebRTC's speech/non-speech classifier) frame-by-
//! frame — chosen over whisper-cli's own `--vad`/`--vad-model` support
//! because that only runs *inside* a single transcription call, with no
//! way to pull segment boundaries out ahead of time to slice separate
//! per-segment audio for Vaihe 28's one-`whisper-cli`-call-per-segment
//! architecture (decided with the user; see
//! `docs/src/developer/technology/webrtc-vad.md`).

use webrtc_vad::{SampleRate, Vad, VadMode};

/// Samples per VAD frame at 16kHz — 30ms, the largest frame length
/// `webrtc_vad::Vad::is_voice_segment` supports (only 10/20/30ms), chosen
/// to minimize the number of FFI calls per second of audio.
const FRAME_LEN: usize = 480;

/// Sample rate this module and [`crate::AudioCapture`]'s captured WAV
/// files both use.
const SAMPLE_RATE_HZ: u64 = 16_000;

/// A contiguous run of speech detected in a captured audio stream, with
/// its position (relative to the start of the recording) and raw 16kHz
/// mono PCM samples — the "audio chunk + timestamp" `TODO.md` Vaihe 27
/// hands to the next step (Vaihe 28 turns each into its own
/// `whisper-cli` call and cue).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpeechSegment {
    /// Start of the segment in milliseconds since recording began.
    pub start_ms: u64,
    /// End of the segment in milliseconds since recording began.
    pub end_ms: u64,
    /// The segment's 16kHz mono PCM samples.
    pub samples: Vec<i16>,
}

/// [`VadSegmenter`]'s internal state: either idle in silence, or
/// accumulating an in-progress candidate speech segment.
enum State {
    /// No speech currently accumulating.
    Silence,
    /// Buffering a candidate speech segment. `trailing_silence_samples`
    /// tracks how many samples at the *end* of `samples` were classified
    /// as silence, so [`VadSegmenter`] knows both when a pause has lasted
    /// long enough to end the segment, and how much of it to trim off
    /// the finished segment's audio.
    Speech {
        start_sample: u64,
        samples: Vec<i16>,
        trailing_silence_samples: u64,
    },
}

/// Chops a continuous stream of 16kHz mono PCM samples into
/// [`SpeechSegment`]s at pauses. Samples are pushed incrementally via
/// [`VadSegmenter::push_samples`] as `AudioCapture`'s WAV file grows;
/// [`VadSegmenter::flush`] finalizes whatever's left in progress once the
/// recording stops.
pub struct VadSegmenter {
    vad: Vad,
    frame_buffer: Vec<i16>,
    samples_processed: u64,
    state: State,
    min_speech_duration_ms: u64,
    min_silence_duration_ms: u64,
}

impl Default for VadSegmenter {
    fn default() -> Self {
        Self::new()
    }
}

impl VadSegmenter {
    /// Creates a segmenter with default thresholds: 250ms minimum speech
    /// duration (shorter blips are discarded as noise, mirroring
    /// whisper.cpp's own `--vad-min-speech-duration-ms` default) and
    /// 500ms minimum silence duration to end a segment — long enough that
    /// a natural mid-sentence breath doesn't split one sentence into two
    /// segments, short enough to still land close to sentence boundaries.
    pub fn new() -> Self {
        Self::with_thresholds(250, 500)
    }

    /// Creates a segmenter with explicit thresholds — used by tests that
    /// need segment boundaries to land within a shorter synthesized
    /// fixture than the real-world defaults in [`VadSegmenter::new`]
    /// would produce.
    pub fn with_thresholds(min_speech_duration_ms: u64, min_silence_duration_ms: u64) -> Self {
        Self {
            // Aggressive mode leans toward reporting non-speech on
            // borderline frames, which matters here: this segmenter's
            // main use case is a language-learning video playing in the
            // background, where music/ambient audio should be less
            // likely to be misclassified as speech than with the
            // library's default `Quality` mode.
            vad: Vad::new_with_rate_and_mode(SampleRate::Rate16kHz, VadMode::Aggressive),
            frame_buffer: Vec::new(),
            samples_processed: 0,
            state: State::Silence,
            min_speech_duration_ms,
            min_silence_duration_ms,
        }
    }

    /// Feeds newly captured samples into the segmenter, returning any
    /// [`SpeechSegment`]s completed as a result (i.e. followed by enough
    /// silence). Samples that don't fill a whole 30ms frame yet are
    /// buffered until the next call, so callers can push samples in
    /// whatever chunk sizes are convenient.
    pub fn push_samples(&mut self, samples: &[i16]) -> Vec<SpeechSegment> {
        self.frame_buffer.extend_from_slice(samples);
        let mut completed = Vec::new();
        while self.frame_buffer.len() >= FRAME_LEN {
            let frame: Vec<i16> = self.frame_buffer.drain(..FRAME_LEN).collect();
            let is_voice = self.vad.is_voice_segment(&frame).unwrap_or(false);
            if let Some(segment) = self.process_frame(frame, is_voice) {
                completed.push(segment);
            }
        }
        completed
    }

    /// Ends the stream: if a speech segment is in progress (e.g. the
    /// recording stopped mid-sentence, before enough trailing silence
    /// accumulated to close it), it's finalized and returned regardless
    /// of `min_silence_duration_ms`. Leftover samples shorter than one
    /// 30ms frame are dropped, since `webrtc_vad` can't classify them.
    pub fn flush(&mut self) -> Option<SpeechSegment> {
        match std::mem::replace(&mut self.state, State::Silence) {
            State::Silence => None,
            State::Speech {
                start_sample,
                samples,
                trailing_silence_samples,
            } => self.finalize_segment(start_sample, samples, trailing_silence_samples),
        }
    }

    /// Advances the state machine by one classified frame, returning a
    /// completed [`SpeechSegment`] if this frame's trailing silence just
    /// crossed `min_silence_duration_ms` inside an in-progress segment.
    fn process_frame(&mut self, frame: Vec<i16>, is_voice: bool) -> Option<SpeechSegment> {
        let frame_len = frame.len() as u64;
        let result = match std::mem::replace(&mut self.state, State::Silence) {
            State::Silence if is_voice => {
                self.state = State::Speech {
                    start_sample: self.samples_processed,
                    samples: frame,
                    trailing_silence_samples: 0,
                };
                None
            }
            State::Silence => None,
            State::Speech {
                start_sample,
                mut samples,
                trailing_silence_samples,
            } => {
                samples.extend_from_slice(&frame);
                let trailing_silence_samples = if is_voice {
                    0
                } else {
                    trailing_silence_samples + frame_len
                };
                if ms(trailing_silence_samples) >= self.min_silence_duration_ms {
                    self.finalize_segment(start_sample, samples, trailing_silence_samples)
                } else {
                    self.state = State::Speech {
                        start_sample,
                        samples,
                        trailing_silence_samples,
                    };
                    None
                }
            }
        };
        self.samples_processed += frame_len;
        result
    }

    /// Trims the trailing silence off a candidate segment's buffered
    /// samples and, if what's left is at least `min_speech_duration_ms`
    /// long, returns it as a [`SpeechSegment`]; otherwise discards it as
    /// a noise blip too short to be real speech.
    fn finalize_segment(
        &self,
        start_sample: u64,
        mut samples: Vec<i16>,
        trailing_silence_samples: u64,
    ) -> Option<SpeechSegment> {
        let trim = trailing_silence_samples.min(samples.len() as u64) as usize;
        samples.truncate(samples.len() - trim);
        if ms(samples.len() as u64) < self.min_speech_duration_ms {
            return None;
        }
        let end_sample = start_sample + samples.len() as u64;
        Some(SpeechSegment {
            start_ms: ms(start_sample),
            end_ms: ms(end_sample),
            samples,
        })
    }
}

/// Converts a sample count at [`SAMPLE_RATE_HZ`] to milliseconds.
fn ms(samples: u64) -> u64 {
    samples * 1000 / SAMPLE_RATE_HZ
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `duration_ms` of silence (all-zero samples) — `webrtc_vad`
    /// reliably classifies these as non-voice.
    fn synth_silence(duration_ms: u64) -> Vec<i16> {
        vec![0i16; samples_for(duration_ms)]
    }

    /// `duration_ms` of a synthesized multi-harmonic tone (fundamental
    /// ~150Hz plus overtones, loosely mimicking voiced speech's
    /// spectral shape) — no real speech fixture exists in this repo's
    /// test suite, but `webrtc_vad::Vad` reliably classifies this signal
    /// as voice, including its very first frame, which is what these
    /// tests rely on for predictable segment boundaries.
    fn synth_speech(duration_ms: u64) -> Vec<i16> {
        let n = samples_for(duration_ms);
        (0..n)
            .map(|i| {
                let t = i as f32 / SAMPLE_RATE_HZ as f32;
                let s = 0.5 * (2.0 * std::f32::consts::PI * 150.0 * t).sin()
                    + 0.3 * (2.0 * std::f32::consts::PI * 300.0 * t).sin()
                    + 0.2 * (2.0 * std::f32::consts::PI * 450.0 * t).sin()
                    + 0.1 * (2.0 * std::f32::consts::PI * 900.0 * t).sin();
                (s * 8000.0) as i16
            })
            .collect()
    }

    /// `duration_ms` at [`SAMPLE_RATE_HZ`], rounded to a whole number of
    /// 30ms VAD frames so tests get exact, predictable segment
    /// boundaries instead of off-by-one-frame rounding.
    fn samples_for(duration_ms: u64) -> usize {
        let frames = duration_ms / 30;
        frames as usize * FRAME_LEN
    }

    #[test]
    fn test_pure_silence_produces_no_segments() {
        // Given: a segmenter fed nothing but silence
        // When:  pushing it and then flushing
        // Then:  no segments are produced at all
        let mut segmenter = VadSegmenter::new();

        let segments = segmenter.push_samples(&synth_silence(1000));

        assert!(segments.is_empty());
        assert_eq!(segmenter.flush(), None);
    }

    /// How far past a speech burst's actual end [`SpeechSegment::end_ms`]
    /// may land, in these tests. `webrtc_vad::Vad` keeps reporting a few
    /// extra frames of voice after loud audio genuinely stops — its
    /// internal noise-floor estimate needs a moment to settle back down
    /// — empirically ~90-120ms; this constant gives that generous
    /// headroom rather than asserting an exact millisecond tied to one
    /// synthesized waveform's amplitude.
    const END_MS_LAG_TOLERANCE: u64 = 300;

    /// Silence long enough to close a segment even after
    /// [`END_MS_LAG_TOLERANCE`]'s worth of lingering "voice" frames, with
    /// clear room to spare above `min_silence_duration_ms` in these
    /// tests' segmenters.
    const CLOSING_SILENCE_MS: u64 = 900;

    #[test]
    fn test_speech_surrounded_by_silence_produces_one_segment_with_expected_timing() {
        // Given: 300ms silence, 600ms speech, then enough closing silence
        // When:  pushed through a segmenter with low thresholds
        // Then:  exactly one segment is returned, starting exactly at
        //        300ms (onset is detected without delay) and ending at
        //        or shortly after 900ms (trailing silence trimmed off,
        //        modulo the VAD's own settle lag)
        let mut segmenter = VadSegmenter::with_thresholds(100, 300);
        let mut audio = synth_silence(300);
        audio.extend(synth_speech(600));
        audio.extend(synth_silence(CLOSING_SILENCE_MS));

        let segments = segmenter.push_samples(&audio);

        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].start_ms, 300);
        assert!(
            (900..=900 + END_MS_LAG_TOLERANCE).contains(&segments[0].end_ms),
            "end_ms {} not within tolerance of 900",
            segments[0].end_ms
        );
        assert_eq!(
            ms(segments[0].samples.len() as u64),
            segments[0].end_ms - segments[0].start_ms
        );
        assert_eq!(segmenter.flush(), None);
    }

    #[test]
    fn test_two_speech_bursts_produce_two_segments_in_order() {
        // Given: silence-speech-silence-speech-silence
        // When:  pushed through the segmenter
        // Then:  two segments come back, in order, each starting exactly
        //        at its expected onset
        let mut segmenter = VadSegmenter::with_thresholds(100, 300);
        let mut audio = synth_silence(300);
        audio.extend(synth_speech(300)); // segment 1 starts at 300ms
        audio.extend(synth_silence(CLOSING_SILENCE_MS)); // closes segment 1
        let second_start = audio.len() as u64 * 1000 / SAMPLE_RATE_HZ;
        audio.extend(synth_speech(300)); // segment 2 starts here
        audio.extend(synth_silence(CLOSING_SILENCE_MS)); // closes segment 2

        let segments = segmenter.push_samples(&audio);

        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].start_ms, 300);
        assert!(segments[0].end_ms < second_start);
        assert_eq!(segments[1].start_ms, second_start);
    }

    #[test]
    fn test_short_blip_shorter_than_min_speech_duration_is_discarded() {
        // Given: a 60ms speech-like blip surrounded by silence, with a
        //        250ms minimum speech duration
        // When:  pushed through the segmenter
        // Then:  no segment is produced — it's too short to be real
        //        speech rather than noise
        let mut segmenter = VadSegmenter::with_thresholds(250, 300);
        let mut audio = synth_silence(300);
        audio.extend(synth_speech(60));
        audio.extend(synth_silence(CLOSING_SILENCE_MS));

        let segments = segmenter.push_samples(&audio);

        assert!(segments.is_empty());
    }

    #[test]
    fn test_push_samples_across_many_small_calls_matches_a_single_call() {
        // Given: the same silence-speech-silence audio, once pushed in
        //        one call and once in small (37-sample), frame-unaligned
        //        chunks
        // When:  segmenting both
        // Then:  they produce the same segment — proving the internal
        //        frame buffer correctly carries leftover samples across
        //        calls
        let mut audio = synth_silence(300);
        audio.extend(synth_speech(600));
        audio.extend(synth_silence(300));

        let mut whole = VadSegmenter::with_thresholds(100, 300);
        let expected = whole.push_samples(&audio);

        let mut chunked = VadSegmenter::with_thresholds(100, 300);
        let mut got = Vec::new();
        for chunk in audio.chunks(37) {
            got.extend(chunked.push_samples(chunk));
        }

        assert_eq!(got, expected);
    }

    #[test]
    fn test_flush_finalizes_in_progress_segment_without_closing_silence() {
        // Given: speech that never gets enough trailing silence to close
        //        the segment on its own (recording just stops)
        // When:  pushing it and then flushing
        // Then:  push_samples returns nothing yet, but flush() returns
        //        the in-progress segment starting at 300ms; since there's
        //        no trailing silence to trim, its end is exact (no VAD
        //        settle-lag involved here)
        let mut segmenter = VadSegmenter::with_thresholds(100, 300);
        let mut audio = synth_silence(300);
        audio.extend(synth_speech(600));

        let segments = segmenter.push_samples(&audio);
        assert!(segments.is_empty());

        let flushed = segmenter.flush().expect("in-progress segment expected");
        assert_eq!(flushed.start_ms, 300);
        assert_eq!(flushed.end_ms, 900);
        assert_eq!(segmenter.flush(), None);
    }
}
