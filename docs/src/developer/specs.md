# Design decisions

Implementation decisions for app behavior beyond SPEC.md's handoff spec —
either left open there, or found through real usage/testing.

## Open Video dialog: folder navigation

Opens on a default folder (CLI video's parent, else `config.toml`'s
remembered `video_folder`, else cwd) but isn't limited to it — an "‥ Up"
row and subfolder clicks navigate in place
(`open_video_dialog::list_folder_entries`). Chosen over a native OS
picker to stay consistent with SPEC.md's "no OS-native file picker"
direction.

## Open Subtitles dialog: no OS drag-and-drop

SPEC.md specs the translation link as an OS drag-and-drop target, but
Slint 1.17.1's winit backend doesn't relay external file drops to
`DropArea` at all (see [slint](technology/slint.md#pitfalls)). Instead, a
small in-app file picker (`FileListDialog`, shared with the Open Video
dialog) scoped to the video's own folder's `.srt` files links a
translation, re-merging cues immediately. SPEC.md's "(DE)"/"(EN)"
language-code labels are generic "Original subtitle"/"Translation"
instead, since trango doesn't track subtitle language.

## Subtitle generation: stub, then whisper-cli

The `subtitle` crate's `SubtitleGenerator` trait (`fn generate(&self,
video_path) -> Result<PathBuf, SubtitleError>`) captured the shape before
any STT dependency was added (`StubSubtitleGenerator` wrote a fixed
placeholder cue). `WhisperCliGenerator` (`crates/subtitle/src/generate.rs`)
now runs whisper.cpp's `whisper-cli` binary as an external process
(`std::process::Command`) rather than a Rust binding crate like
`whisper-rs` — no new Cargo dependency, and `-osrt` already writes a
ready-made `.srt`. Tests fake the binary with small POSIX shell scripts
standing in for its `-of`/`-osrt` contract.

**Audio extraction via ffmpeg.** `whisper-cli` only reads raw audio
(flac/mp3/ogg/wav), not video containers — and silently exits 0 while
writing nothing when given one. `WhisperCliGenerator::generate` now
extracts the video's audio to a temp 16kHz mono WAV with `ffmpeg` first,
then runs `whisper-cli` against that. `extract_audio` and
`run_whisper_cli` are separately testable against fake binaries;
`run_command` retries briefly on `ETXTBSY` (freshly written test binaries
occasionally racing `exec`).

**Background thread, not the UI thread.** Real transcription can take
minutes; `spawn_generate` runs it on `std::thread::spawn`, reporting back
via `slint::invoke_from_event_loop` (state behind `Rc`/`RefCell` isn't
`Send`, so only a `Weak<AppWindow>` + owned `Result` cross the thread
boundary — mirroring `video_player.rs`'s `load_file`).

## Model selection: UI + autodiscovery, persisted to TOML

Replaced an env var (`TRANGO_WHISPER_MODEL_PATH`) with an in-app picker,
since models are switched more often than the CLI binary path is — the
Open Subtitles dialog's model row opens a `FileListDialog` scoped to
`.bin`/`.gguf` files (`model_picker.rs`). `default_start_folder` tries the
config's remembered folder, then a few well-known whisper.cpp model
locations, then cwd — no OS-specific magic. The pick persists to
`config.rs`'s `$XDG_CONFIG_HOME/trango/config.toml` (trango's first
persistent settings file, added with user approval per CLAUDE.md).
`model_picker::language_flag` infers `-l en` vs `-l auto` from
whisper.cpp's `.en` filename convention. Smaller models transcribe
non-English audio much worse than English — usage docs recommend
`medium`/`large-v3` for anything else.

## Generating subtitles for an open video reloads it

(Superseded as the *sole* fix by `keep-open=yes` — see
[Video playback](architecture/video-playback.md) — but still done, since
it also re-arms the sentence-by-sentence start seek.) Generating
subtitles for an already-playing video can let it reach EOF
mid-generation, leaving mpv's core idle and every subsequent seek failing
(`Raw(-12)`). Fix: after linking the generated subtitle,
`wire_open_subtitles_dialog` also reloads the video via
`VideoPlayer::load_video`. Since a real `VideoPlayer` can't be constructed
in `main.rs`'s tests, the handler takes a `reload_video` closure instead
of the player directly, so tests can assert the reload without a real
mpv instance.

## No mode autoplays — only Space starts/stops playback

Initially every navigation action (`next_cue`/`previous_cue`/
`jump_to_cue`/`repeat_current_cue`) auto-played through to the cue's end
and paused. This broke replay: real STT output commonly produces
contiguous cues (cue N's end == cue N+1's start), so the moment mpv
auto-paused at N's end, `sync_current_sentence` immediately reclassified
the cursor onto N+1, and Space then replayed the wrong sentence.

Fix: **navigation only seeks and leaves mpv paused; Space is the only
thing that starts/stops playback**, as a toggle. This needed a type split
in `playback-state`: `SeekCommand { start }` (navigation) vs.
`PlaySpanCommand { start, end }` (repeat). Whether a span should start or
interrupt playback needs live mpv state, so that decision moved to
`video_player.rs`'s `toggle_play_span`. `sync_current_sentence` was
further restricted to only re-derive the cursor while `pause_at` is
actually armed, so a paused cursor never gets silently reclassified — and
`pause_and_arm_start_seek` now unconditionally pauses on load, only
conditionally arming a start-of-playback seek, so a video with no
subtitle also opens paused.

## `sync_current_sentence` removed entirely

The `pause_at`-gated fix above still had a same-tick race:
`sync_current_sentence` and `apply_pending_pause` run in the same poll
tick, sync first — so on the tick `time-pos` reaches a cue's end,
`pause_at` hasn't cleared yet and the cursor still gets reclassified onto
the contiguous next cue before the pause lands. Since every play action
is now a bounded, already-known-cue span (`toggle_play_span`), there was
no remaining case needing live rediscovery of the cursor from `time-pos`
— so the function, its poll call, and `PlayerState::sync_cue_to_time`
were deleted outright rather than patched again. (Both `Normal` mode
continuous playback and scrub-bar dragging later needed the same kind of
live tracking — see "Normal mode: live time-pos syncing" below — but were
designed fresh against their own seek model, not by reviving this.)

## Space works in every mode

`Right`/`Left`/`Space` were all gated behind `sentence-mode-active`, a
leftover from before autoplay-on-open was removed. `Right`/`Left` stay
gated (no `Normal`-mode cue navigation exists), but Space now works
unconditionally: `repeat_current_cue` returning `Some` (a cue in focus,
`SentenceBySentence`) plays that cue's bounded span via
`toggle_play_span`; returning `None` (`Normal` mode, or no subtitle)
instead calls the new unbounded `VideoPlayer::toggle_playback`. First
pass missed that `current_cue_index` is set regardless of mode, so a
subtitle linked while in `Normal` mode wrongly routed Space to the bounded
path — fixed by adding a mode check directly to `repeat_current_cue`
itself.

## Word analysis: local Ollama, not a cloud API

Word-by-word translation + pronunciation for the on-screen sentence uses
[Ollama](https://ollama.com) (`localhost:11434`) for the same reason
whisper-cli was chosen for subtitle generation: no upload, no per-call
cost, on-device. Ollama is an external program, not a Cargo dependency.

- **Crate split:** HTTP/JSON/cache logic lives in `crates/word-analysis`,
  free of Slint/libmpv, mirroring `subtitle`/`playback-state`. The
  app-local wiring module `crates/app/src/word_analysis.rs` shadows the
  extern crate `word_analysis` at `main.rs`'s crate root; call sites
  needing the crate use a leading `::word_analysis::...`.
- **HTTP client: `ureq`, not `reqwest`** — no async runtime elsewhere in
  trango; see [ureq](technology/ureq.md).
- **Prompt/response:** `build_prompt` asks for `{"words": [{"word",
  "translation", "pronunciation"}]}` with Ollama's `format: "json"` and
  `stream: false`; `parse_analysis_response` strips a defensive
  ` ```json ` fence some models still add.
- **Cache:** one JSON sidecar per subtitle (`subs.srt` →
  `subs.wordanalysis.json`), keyed by `Cue::index` (`AnalysisCache {
  model, entries }`) so a shifted line doesn't reuse a stale entry. A
  missing/corrupt cache becomes empty rather than an error. Ctrl+A
  (single sentence) and "Analyze all sentences" (batch, saving
  incrementally after every cue so a stopped run loses no progress) share
  this same file.
- **Model + target language:** an "Ollama model" row reuses the
  `FileListDialog` chrome, backed by a network call (`GET /api/tags`) run
  on a background thread. Target language is free text (not a fixed
  list, per user preference — trango's first `LineEdit`), saved to config
  on every keystroke, defaulting to `"English"` only in code, not in
  `TrangoConfig::default()`.

## Word analysis: `"think": false`, and debug logging

Reasoning models (e.g. the `qwen3` family) can spend their whole
generation budget on internal "thinking" and return an empty `response`,
which `serde_json` fails to parse with a confusing zero-length-input
error. `GenerateRequest` now sets `"think": false`; `analyze_sentence`
also checks for an empty response explicitly, returning a clear
`OllamaError::InvalidResponse` instead of forwarding the parse error.

Diagnosing this needed the raw prompt/response logged at
`tracing::debug!`, which surfaced that `tracing-subscriber`'s
`env-filter` feature had never actually been enabled — `RUST_LOG`
filtering had silently never worked. Rather than just enabling it and
relying on `RUST_LOG`, the user asked for a CLI flag instead (per
CLAUDE.md's env-var-vs-flag convention): `--debug`
(`extract_debug_flag`) now builds a fixed
`"info,trango=debug,word_analysis=debug"` filter; `RUST_LOG` still works
underneath as a finer-grained escape hatch.

## Normal mode's hint bar content

The bottom `HintBar` used to be gated behind `sentence-mode-active`
entirely, so Normal mode showed no shortcut reminders even though
Space/Ctrl+T/Ctrl+A already worked there. `HintBar` now takes
`sentence-mode-active` as an input and shows a mode-dependent subset of
the same five entries (Right/Left only in `SentenceBySentence`; Space's
label switches between "repeat sentence"/"play-pause"), always
instantiated. The five near-identical labels were factored into a
`HintLabel` sub-component.

## Scrub bar drag-to-seek

A `TouchArea` (24px tall, taller than the 4px visible track) inside
`ScrubBar` computes the pointer's fraction across the track on
`clicked`/`moved`-while-`pressed`, firing `seek-requested(float)` →
`video_player::VideoPlayer::seek_to_fraction`. Unlike cue-navigation
seeks, this **never touches `pause`** (a drag shouldn't start or stop
playback) and clamps the fraction (which can overshoot `0.0..1.0` on an
out-of-bounds drag) in a small pure, unit-tested `seek_target_secs`
helper. It clears any armed `pause_at`, same as other seeks.

## Normal mode: live time-pos syncing

The last open item: Ctrl+A in `Normal` mode showed a stale sentence once
playback moved past whatever cue was current when the mode was entered,
since nothing re-derives `current_cue_index` from live `time-pos` outside
`SentenceBySentence`. Designed fresh rather than reviving the removed
`sync_current_sentence` (see above): `PlayerState::sync_cue_to_time`
returns whether the cursor changed; `video_player.rs`'s new
`sync_current_sentence_normal_mode` (a no-op outside `Normal` mode) runs
on every poll tick and mirrors a changed cue into the sentence card/list.
`Normal` mode never arms `pause_at`, so the same-tick race that killed the
original mechanism structurally can't happen here. Scope: only live
*tracking* — whether the sentence panel should even show in `Normal` mode
is still open.

## Current-sentence card: bounded, scrollable sentence text

Reported as long cues getting visually cut off mid-word in the
current-sentence card. Cause: the original-language `Text` had no bounded
height, so `CurrentSentenceCard`'s `vertical-stretch: 0` asked its
`VerticalLayout` parent for exactly the wrapped text's natural height —
whenever the sentence panel column ran short on room, the layout squeezed
the card below that, silently clipping the bottom line(s) instead of
showing or scrolling to them. Fixed the same way `translation-height`
already fixes the same class of bug for the translation line below it: a
fixed-height `ScrollView` (`sentence-height`, 150px ≈ 4 lines) so long
sentences scroll instead of clipping, and the card's height stays
predictable regardless of cue length.
