# Word-level audio timing via whisper-cli DTW

## Context

Longer-term goal (user's own framing): build an automatic per-sentence
pronunciation-practice track — for each word: translate it, TTS it, play
the TTS, then play the real word audio at 50%, then 75%, then normal
speed a couple of times; after all words, play the whole sentence twice
at normal speed. Each subtitle cue becomes its own practice recording.

None of that is possible without first knowing **where each word starts
and ends** within a cue's `[start, end]` span. That timing does not exist
anywhere today — `Cue` (`crates/subtitle/src/cue.rs:9-21`) only has
whole-sentence `start`/`end`; word boundaries are currently *text*-only
(`str::split_whitespace`, niqud's whitespace pairing in
`crates/niqud/src/onnx_client.rs:118-119`), never *time*.

This plan covers **only** that missing piece: given a cue's known
`start`/`end` and the source video/audio file, get real per-word
start/end timestamps out of whisper.cpp's `--dtw` (forced-alignment-style
token timing), re-running `whisper-cli` — the same external tool the app
already depends on for subtitle generation — against just that cue's
audio slice. Translation, TTS, speed variants, and practice-audio
assembly are explicitly **out of scope** for this step (see "Not in this
plan" below) — one small independent feature at a time, per CLAUDE.md's
TDD rule.

Confirmed via `whisper-cli --help` and `strings` on the installed binary
(`~/.local/bin/whisper-cli`): `-dtw MODEL` accepts exactly these preset
tokens: `tiny`, `tiny.en`, `base`, `base.en`, `small`, `small.en`,
`medium`, `medium.en`, `large.v1`, `large.v2`, `large.v3`,
`large.v3.turbo` (unknown presets are a hard CLI error). Combined with
`-ml 1 -sow` (max segment length 1, split on word rather than token),
`whisper-cli`'s existing `-osrt` output already comes out as **one SRT
cue per word** — no JSON/token parsing needed, `subtitle::parse_srt`
(already in the crate) reads it directly.

## Not in this plan (later steps)

- Any UI trigger, keybinding, or persistence of word timings.
- Translation-per-word (already exists: `word-analysis` crate/`Ctrl+A`),
  TTS, speed-variant audio (`atempo`), and per-sentence practice-audio
  export/concatenation.
- Optional local-LLM speech-language detection the user mentioned — only
  relevant if/when this needs to run on audio whose language isn't
  already known (today `Cue`/config already carry a language via
  `model_picker::language_flag` for the subtitle-generation model, which
  this step reuses as-is).

These are flagged so scope doesn't creep, not because they're rejected —
they're the natural next phases once word timing exists.

## Implementation

### 1. `crates/subtitle/src/word_timing.rs` (new file)

- `pub struct WordTiming { pub word: String, pub start: Duration, pub end: Duration }`
  — timestamps are absolute within the *original* source file (already
  offset by the cue's own `start`), so callers don't have to do that math.
- `pub struct WhisperCliWordSegmenter { binary_path, ffmpeg_path, model_path: Option<PathBuf>, language: Option<String>, dtw_preset: Option<String> }`
  with `Default` — mirrors `WhisperCliGenerator`'s fields/doc-comment
  style (`crates/subtitle/src/generate.rs:79-109`) plus the new
  `dtw_preset`.
- `pub fn segment_words(&self, source_path: &Path, start: Duration, end: Duration) -> Result<Vec<WordTiming>, SubtitleError>`:
  1. `extract_clip` — new private fn, same shape as `extract_audio`
     (`generate.rs:116-152`) but adds `-ss <start> -to <end>` as **output**
     options (placed after `-i`, so `-to` is an absolute source timestamp
     and seeking is sample-accurate rather than keyframe-snapped) —
     16kHz mono `pcm_s16le`, same as today.
  2. Runs `whisper-cli -f <clip> [-m model] [-l language] -ml 1 -sow [-dtw preset] -of <stem> -osrt`
     — `-dtw` omitted entirely when `dtw_preset` is `None` (graceful
     degradation to whisper.cpp's non-DTW word timestamps rather than a
     hard failure).
  3. `crate::parse_srt` the resulting `.srt`, map each `Cue` →
     `WordTiming { word: cue.text.trim().to_string(), start: cue.start + start, end: cue.end + start }`.
  4. Always deletes the temp clip `.wav` and `.srt` (success or error),
     same pattern as `transcribe_segment` (`generate.rs:223-248`).
  - Reuses `run_command`/`last_stderr_line` from `generate.rs` — change
    their visibility from private to `pub(crate)` there rather than
    duplicating them.
  - New temp-path helper mirroring `temp_segment_audio_path`
    (`generate.rs:288-295`) — process-unique via the same
    `AtomicU64` counter pattern (a fresh `static COUNTER`, since it's a
    different call site).

### 2. `crates/subtitle/src/lib.rs`

Add `mod word_timing;` and `pub use word_timing::{WhisperCliWordSegmenter, WordTiming};`, update the module doc-comment.

### 3. `crates/app/src/model_picker.rs`

- `const DTW_PRESETS: &[&str]` — the 12 tokens above, **ordered
  longest/most-specific first** (`large.v3.turbo` before `large.v3`
  before `large`-anything, `medium.en` before `medium`, etc.) so a
  substring scan picks the more specific match first.
- `pub fn dtw_preset_for_model(model_path: &Path) -> Option<&'static str>`
  — mirrors `is_english_only`/`language_flag`'s
  filename-convention-sniffing style (lines 118-136): take the file
  stem, lowercase it, normalize `-`/`_` to `.` (whisper.cpp model
  filenames commonly use `ggml-large-v3.bin`, a dash, while the `--dtw`
  preset token is `large.v3`, a dot), then return the first
  `DTW_PRESETS` entry that appears as a substring, or `None` if nothing
  matches (e.g. a custom fine-tune with an unrecognized name) — `None`
  means the caller omits `-dtw` rather than guessing wrong and hitting
  whisper-cli's hard "unknown DTW preset" error.
  - Verified this normalize-then-substring approach also resolves a real
    local fine-tune correctly: `ggml-large-v3-turbo-ivrit.bin` → normalized
    `ggml.large.v3.turbo.ivrit.bin` → matches `large.v3.turbo`.

### 4. Tests (TDD: written first)

- `word_timing.rs` unit tests, following `generate.rs`'s existing fake-binary
  pattern (`write_fake_binary`, `FAKE_WHISPER_CLI_SCRIPT` at
  `generate.rs:481-510`) — no real `ffmpeg`/`whisper-cli` involved:
  - `extract_clip` passes `-ss`/`-to` with the expected values (mirrors
    `test_extract_audio_runs_ffmpeg_with_expected_flags`).
  - `segment_words` passes `-ml 1 -sow -dtw <preset>` when a preset is
    given, and omits `-dtw` entirely when it's `None`.
  - A fake `whisper-cli` writing a fixed multi-cue `.srt` proves
    `segment_words` maps cues → `WordTiming` with `start`/`end` correctly
    offset by the clip's own `start`.
  - Temp clip `.wav`/`.srt` are cleaned up on both success and
    `whisper-cli` failure (mirrors the `transcribe_segment` cleanup
    tests, `generate.rs:748-824`).
- `model_picker.rs`: `dtw_preset_for_model` unit tests (pure function, no
  fake binaries needed) — covers `ggml-base.en.bin` → `Some("base.en")`,
  `ggml-large-v3.bin` → `Some("large.v3")`, the ivrit fine-tune case
  above, and an unrecognized filename → `None`.

### 5. Docs

- `docs/src/developer/specs.md`: append one decision entry (~10-15
  lines, after the existing Hebrew-word-analysis entries) — what:
  re-running `whisper-cli` per-cue with `-ml 1 -sow [-dtw preset]` on a
  freshly `ffmpeg`-cut clip, reusing `parse_srt` instead of JSON
  token parsing; why: DTW alignment quality depends on a short, focused
  clip rather than the whole file, and this reuses the exact
  `Command`/`ffmpeg` plumbing `WhisperCliGenerator` already has; the one
  pitfall worth recording: preset-name filename normalization (`-`/`_` →
  `.`) and the graceful `None`-preset fallback for unrecognized models.
- No `docs/src/usage/` page yet — nothing user-facing exists until a
  later step wires this into the UI (matches CLAUDE.md's "current state,
  not the goal" rule for docs).

### 6. `TODO.md`

Append `## Vaihe 31 — Sana-tason audion haarukointi (whisper-cli DTW)`,
matching the existing phase format (`Tavoite:` / bullets /
`Voit ajaa/testata:`), describing exactly this step's scope and
explicitly noting translation/TTS/practice-audio as later phases (not
this one) — same role the "Ei tässä listassa" section and phases like
Vaihe 29 already play for scoping.

### 7. Version + release notes (once, at the end — see §8)

- `Cargo.toml` workspace version `0.1.54` → `0.1.55` (UI version display
  already reads `CARGO_PKG_VERSION` automatically, `main.rs:1609` — no
  separate UI edit needed).
- `releasenotes.md`: fill in the (currently empty) `[Unreleased]` →
  `0.1.55` section, `### Added`: word-level audio timing via
  `whisper-cli --dtw`.
- Done exactly once for this whole branch/plan, not per intermediate
  commit — see §8's version/changelog discipline note.

### 8. Git workflow

Confirmed via `git fetch` + `merge-base --is-ancestor`: `hebrew-niqud-pronunciation`
is already merged into `origin/master` (PR #8, tagged `v0.1.54`) — so
this new work branches cleanly off fresh `master`, e.g.
`word-level-audio-timing-dtw`.

**Version/changelog discipline for this plan:** CLAUDE.md normally bumps
`Cargo.toml` + `releasenotes.md` per work step/commit, but per the user's
explicit instruction this plan produces **exactly one** version bump
(`0.1.54` → `0.1.55`) and **one** `releasenotes.md` entry, in the final
commit of this branch — not one per intermediate step (module added,
tests added, docs added, etc.), even if the work is committed in more
than one commit along the way. Re-check this instruction before bumping
anything mid-branch.

Before the final commit: `scripts/check.sh` and `scripts/test.sh` both
`OK`. Push, `gh pr create` against `master`.

## Verification

- `scripts/test.sh` → `OK` (new unit tests above, all using fake
  binaries — no real `whisper-cli`/`ffmpeg`/model file needed in CI).
- `scripts/check.sh` → `OK` (fmt + clippy).
- Manual sanity check (not part of CI): run `segment_words` against a
  real short clip with a real installed model (e.g. the local
  `ggml-large-v3-turbo-ivrit.bin` or `ggml-large-v3.bin` already on this
  machine) and eyeball that returned words/timestamps land inside
  `[start, end]` and roughly match the audio.
