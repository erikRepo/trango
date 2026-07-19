# Design decisions

Implementation decisions for app behavior beyond SPEC.md's handoff spec —
either left open there, or found through real usage/testing.

## Open Video dialog: folder navigation

Opens on a default folder (CLI video's parent, else `config.toml`'s
remembered `video_folder`, else cwd) but isn't limited to it — an "‥ Up"
row and subfolder clicks navigate in place
(`open_media_dialog::list_folder_entries`). Chosen over a native OS
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

`TODO.md` Vaihe 29 reuses this same `generate` for the Audio source's
"Generate subtitles" (same button/dialog as the Video source — no new call
site was needed, since Vaihe 28 already generalized `CurrentMedia` to hold
either a video or a recorded/opened `.wav` path). `generate` skips
`extract_audio` when its input is already a `.wav` — the only extension the
Audio source ever loads — and hands it to `whisper-cli` directly.

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

The original-language `Text` had no bounded height, so
`CurrentSentenceCard`'s `vertical-stretch: 0` asked its `VerticalLayout`
parent for exactly the wrapped text's natural height — whenever the
sentence panel column ran short on room, the layout could squeeze the
card below that, clipping the bottom line(s) instead of showing or
scrolling to them. Fixed the same way `translation-height` already fixes
the same class of bug for the translation line below it: a fixed-height
`ScrollView` (`sentence-height`, 150px ≈ 4 lines) so long sentences
scroll instead of clipping. This does **not** fix mixed-script bidi
rendering glitches — see "Known limitation: bidi text wrapping" below,
which turned out to be the actual cause of the originally reported bug
report this investigation started from.

## Known limitation: bidi text wrapping (Slint/femtovg)

A cue mixing Hebrew (RTL) with an embedded Latin word (e.g. "co-working")
renders garbled characters at the line-wrap boundary when the Latin word
falls across a wrap point — not a height/clipping issue (ruled out above),
but character-level bidi reordering going wrong in Slint's text shaping.
`video_player.rs` requires Slint's OpenGL (femtovg) renderer for the mpv
render context, so switching renderer isn't an available workaround.
Slint's RTL/bidi support is itself incomplete upstream — see
[slint-ui/slint#2294](https://github.com/slint-ui/slint/issues/2294) and
[#7267](https://github.com/slint-ui/slint/issues/7267). Accepted as a
known limitation for now; no in-repo workaround attempted. Revisit if
Slint's bidi support improves, or if this affects enough real subtitle
content to justify manually inserting Unicode directional-isolate marks
(U+2066/U+2069) around embedded Latin runs before handing cue text to
`sentence_card.rs`.

## Audio source: system-audio capture, not YouTube download/caption scraping

Live subtitle recording without a video (`TODO.md` Vaihe 25–31) needs some
source of audio/text to transcribe. Two alternatives were considered and
rejected for copyright reasons: playing/downloading the source video
directly (e.g. via `yt-dlp` + mpv's `ytdl_hook`), and scraping a site's
already-generated captions (e.g. `yt-dlp --write-auto-sub --skip-download`).
Both would have trango fetch copyrighted content from a third party.
Instead, Vaihe 26 onward capture the system's own audio *output* — whatever
is already playing locally, from any source — and never persist more than
the resulting `.srt`; no video/audio file trango didn't already have is
ever downloaded or saved.

## System audio capture: `pactl`'s default-sink monitor, graceful `ffmpeg` stop

`TODO.md` Vaihe 26 needed a monitor source to feed `ffmpeg -f pulse -i`.
Rather than parsing `pactl list sources` for whichever ones end in
`.monitor` (several, if multiple outputs exist — ambiguous to pick
between), `AudioCapture::default_monitor_source` asks `pactl
get-default-sink` and appends `.monitor` itself, since PulseAudio/
PipeWire guarantee that naming convention. `config.rs`'s
`audio_monitor_source` overrides this for setups where the default sink
isn't the one to capture.

Killing `ffmpeg` outright (`SIGKILL`) leaves the WAV header's size field
wrong, since `ffmpeg` only finalizes it on a clean exit. `AudioCapture::stop`
instead writes `q` to `ffmpeg`'s stdin — the same key it reads
interactively to quit gracefully — and only falls back to `kill()` after
`graceful_stop_timeout` (a test-injectable field; production uses 5s).

A missing `pactl`/`ffmpeg` install only showed up in the log (usually
invisible to a user running the packaged app), making Ctrl+Space look
like it silently did nothing. `system_audio_capture::wire_audio_capture`
now also mirrors every start/stop outcome into `audio-capture-error-message`
(`AppWindow` property, shown in the Audio source's placeholder), cleared on
success — a small, targeted piece of Vaihe 29's UI pulled forward, without
building the full rec/stop control it also adds.

## `MediaSource`, split out from `PlaybackMode`

Which source is active (video file vs. audio) and how navigation behaves
(Normal vs. Sentence by sentence) are independent choices, so a single
`PlaybackMode` enum can't express both — a three-way mode would have no way
to select "audio source" and "Sentence by sentence" together.
`playback_state::MediaSource` (Video/Audio) exists alongside the original
two-variant `PlaybackMode`; `PlayerState` holds both fields independently.
The top bar mirrors this with two separate segmented-control groups —
Video/Audio and Normal/Sentence-by-sentence — rather than one combined
control (not in the mock, `sketch/design_reference.dc.html#1c`, which only
showed the mode pair). The video area's `Rectangle` stays unconditionally
instantiated in the Audio source too (so `video-frame-x/-y/-width/-height`,
read every frame by `video_player.rs`, keep resolving) — the Audio
placeholder is an overlay child inside it, not a swapped-out sibling.

## System audio capture reverted to a single WAV file, not live segmentation

An earlier version of Vaihe 26 had `ffmpeg` stream raw PCM to its stdout
so a `VadSegmenter` could chop it into speech segments for per-segment
`whisper-cli` transcription, growing the sentence list live. That
approach (`webrtc-vad`, `vad.rs`, `live_transcription.rs`) was removed:
it added real complexity (FFI, a non-`Send` VAD instance, a channel
draining onto the UI thread) for transcription quality no better than
running `whisper-cli` once over the finished recording. `AudioCapture`
now just has `ffmpeg` write directly to a WAV file; `TODO.md` Vaihe 29
runs "Generate subtitles" over that file as a whole, the same
`WhisperCliGenerator` path video files use.

## Recording filename: `chrono` dependency, rename only after stop

`TODO.md` Vaihe 27's default filename needs a local (not UTC) date+time —
the std library has no timezone-aware formatting for `SystemTime`, so
`chrono` was added to `crates/app` rather than hand-rolling civil-date
math. The filename is locked while a recording is in progress and only
renamable afterwards, matching a normal recorder's behavior; the rename
handler rejects any value that isn't a single plain path component so a
pasted value containing `/` or `..` can't move the file outside its
recording folder.

## Validated: cue-based features never depended on video

`TODO.md` Vaihe 30 asked whether sentence list, Ctrl+A word analysis, and
the translation toggle secretly assumed a video was loaded. They don't:
`sentence_card.rs`/`sentence_list.rs`/`word_analysis.rs` and
`playback_state::PlayerState`'s cue navigation only ever read
`cues`/`current_cue_index`/`show_translation`, never `MediaSource` or
anything video-specific — confirmed by grepping for `MediaSource` outside
`main.rs`'s own source-selection code and `video_player.rs`. No code
changed; `crates/app/tests/e2e_sentence_navigation.rs` and
`main.rs`'s `test_app_window_properties` gained tests that switch to the
Audio source mid-run and repeat the same navigation/Ctrl+A/sentence-list
assertions, locking the guarantee in against regressions.

## Source switch pauses playback and gates controls by the loaded file's kind

Video and Audio share one `video_player::VideoPlayer`/mpv instance — the
top bar's source buttons only ever swapped which panel was visible, never
touched playback. That meant switching sources left whatever was playing
running audibly behind the hidden panel, and a loaded video's ScrubBar
could appear in the Audio panel (or its picture show through) just because
*some* file happened to be loaded, regardless of kind. Fixed two ways: the
Video/Audio segment buttons now call a new `pause-playback` callback
(`video_player::VideoPlayer::pause()`) before `select-media-source`, and
`AppWindow::media-ready` (`video-loaded && loaded-media-source ==
media-source`) gates the mpv underlay/ScrubBar/SpeedSlider/Audio
placeholder so they only activate once the actually-loaded file's kind
matches the visible panel. `loaded-media-source` is set in
`open_selected_media`, the single choke point both the Open dialog and the
post-recording auto-load go through. Two independent player "slots" (each
source remembering its own loaded file/position) was considered and
rejected as unnecessarily large for the actual complaint — the one shared
mpv instance never caused problems the previous UI just failed to gate
against.

## Sentence card/list and Ctrl+A also gated by panel_content_ready

Once playback controls were gated by `media-ready` above, the same
complaint showed up one layer up: switching to a not-yet-loaded Audio
panel left the Video source's current-sentence card and sentence list
sitting on screen untouched, and Ctrl+A would still analyze that stale
sentence — the "Validated: cue-based features never depended on video"
decision above had made this a deliberate, tested guarantee (switching
source never touches cues), which now reads as the bug rather than the
fix. Revised: `main.rs`'s `panel_content_ready` (same `media-source !=
Audio || media-ready` condition as the Slint gate) decides what the
sentence card/list *display*, blanking it to an empty `PlayerState`'s
placeholder in `on_select_media_source` when the newly-selected panel
isn't ready, and restoring the real one when switching back to a source
that is. The Ctrl+A handler checks the same condition before reading
`current_cue_index`, so it reports "No sentence is currently in focus"
rather than reusing the other source's cache entry. `PlayerState.cues`
itself is never touched — only what's displayed/analyzed — so cue
navigation's source-independence (the `e2e_sentence_navigation.rs`
guarantee) still holds unchanged; only the two Slint-facing consumers
that read the *current* cue for on-screen display now also check which
panel is showing it.

## CI: PR checks and .deb release automation

Pull requests against `master` run `.github/workflows/ci.yml`: fmt +
clippy (`scripts/check.sh`), the test suite (`scripts/test.sh`), and a
release build, as three separate jobs so failures are easy to tell apart.

`.github/workflows/release-deb.yml` builds and publishes a `.deb` as a
GitHub Release whenever `master`'s workspace `Cargo.toml` changes — in
practice, every merged PR, since versioning bumps on every commit. A
`check-version` job guards against re-publishing a version that already
has a release (e.g. a `Cargo.toml` change that touched something other
than the version field). Packaging uses `cargo-deb`, configured via
`[package.metadata.deb]` in `crates/app/Cargo.toml`; runtime `Depends`
are left at the default `$auto` so `dpkg-shlibdeps` derives them from the
built binary's actual shared-library links instead of being hand-maintained.

## Hebrew pronunciation: native `ort` inference, not the Ollama prompt

Ollama's own `pronunciation` field is unreliable for Hebrew even with a
Hebrew-capable model — small LLMs mistransliterate niqud/dagesh
distinctions (e.g. שכב → "shkach" instead of "sha-khav"), and re-feeding
niqud text through the LLM wouldn't fix this: BPE tokenization splits
Hebrew combining diacritics unpredictably, so the same unreliability just
moves one step later. Instead `crates/niqud`'s `OnnxNiqudClient` runs
[Phonikud](https://github.com/thewh1teagle/phonikud)'s niqud model
directly via `ort` (ONNX Runtime bindings), and a deterministic Rust
table (`transliterate.rs`) converts the resulting niqud text to a
hyphenated Latin guide — no further LLM call. Gated automatically by
`contains_hebrew` (Unicode block U+0590–U+05FF); other languages are
untouched. Ollama still handles translation (a real semantic task) and
its `pronunciation` guess is kept as a fallback if no niqud model is
configured, loading fails, or the word counts don't align
(`tracing::warn`, never a hard failure).

**Native Rust, not a Python subprocess.** An earlier version shelled out
to a Python/`phonikud-onnx` CLI wrapper; it worked but accumulated real
operational hackiness (a venv whose activation state depends on whatever
shell happens to launch trango, on top of CPU-pinning/offline-mode
workarounds). Reimplementing natively turned out tractable because the
model's I/O contract and the `dicta-il/dictabert-large-char-menaked`
tokenizer were both fully reverse-engineered during that first
implementation: despite being stored in HuggingFace's "WordPiece"
format, the tokenizer is actually **character-level** (its
`pre_tokenizer` splits into individual characters first, so no subword
merging ever happens) — a flat char→id vocab parsed straight from
`tokenizer.json` is enough, no `tokenizers` crate dependency needed.
`decode.rs` ports `phonikud_onnx`'s Python reconstruction loop
(argmax over `nikud_logits`/`shin_logits`, threshold over
`additional_logits`'s stress/vocal-shva/prefix classifiers) directly.

**Pitfalls found in the model's output that both the decode loop and the
transliteration table depend on:** beyond standard nikud/dagesh/shin-dot
marks, the model also emits a `|` (U+007C) after prefix letters
(ו/ב/כ/ל/מ/ש) marking a morpheme boundary, and a meteg (U+05BD) combined
with shva distinguishes vocal shva ("e", pronounced) from silent shva —
both undocumented in Phonikud's own API but load-bearing for correct
syllabification.

**Build vs. runtime linking.** `ort`'s default `download-binaries`
feature fetches a prebuilt ONNX Runtime binary *at compile time* over the
network — unacceptable for offline/CI builds. `crates/niqud/Cargo.toml`
instead uses `load-dynamic` (loads `libonnxruntime.so` at *runtime*) plus
`api-23`: the crate's *default* feature set requests API 24, which hangs
indefinitely (not a clean error) against Ubuntu's apt-packaged
`libonnxruntime1.23` — api-23 works correctly against that same package,
confirmed by comparing its output against Python `onnxruntime`'s for
identical input (matching to a few significant digits; a newer runtime
like pip's onnxruntime 1.27 matches exactly).

No `ORT_DYLIB_PATH` needed for a normal install: `crates/app/Cargo.toml`
depends on `libonnxruntime1.23` directly (`$auto`/`dpkg-shlibdeps` can't
detect it, since `load-dynamic` means no link-time ELF reference exists
for it to find), and `crates/niqud/src/dylib.rs` scans the usual
Debian/Ubuntu library directories at runtime for a match — see
[ort](technology/ort.md) for the hang-avoidance details this needed.
Model/tokenizer files are still a manual download (accepted tradeoff,
not automatable the way the library dependency is — too large to bundle
in the `.deb`) — see `docs/src/usage/word-analysis.md`.

**GPU checked, CPU kept deliberately.** `onnxruntime`'s CUDA provider
silently falls back to CPU if system cuDNN is missing (no hard error —
easy to miss). Measured explicitly on real hardware (RTX 5070 Ti):
inference is already ~16ms on plain CPU for this int8 model, so GPU
wouldn't help. `OnnxNiqudClient` requests `CPUExecutionProvider`
explicitly (not a silent default).

## Hebrew prefix particles: a `parts` breakdown, not split top-level entries

Real use surfaced two problems with Hebrew's single-letter prefix
particles (ו/ה/ב/כ/ל/מ/ש, written attached to the following word with no
space, e.g. לסרטים = ל + סרטים). A first attempt asked Ollama to always
split such a word into two separate top-level word entries (own
`"word"`/`"translation"`/`"pronunciation"` each). That was wrong on two
counts:

- **Not how it sounds.** A prefixed word is pronounced as one fused unit
  in speech (e.g. "לסרטים" as "le-sratim"), not two separate sounds — but
  the user wants exactly that fused pronunciation, matching what's
  actually heard, not a per-morpheme guess.
- **Broke niqud alignment.** `apply_niqud_pronunciation`'s word-count
  check compares Ollama's word list against niqud's own, which only ever
  splits on whitespace — so splitting a prefixed word into two Ollama
  entries mismatched niqud's one, and the mismatch fallback (keep
  Ollama's own pronunciation guess) applied far more often than it
  should have.

Fixed by keeping `"word"`/`"pronunciation"` as the whole combined form
(restoring niqud's 1:1 alignment as a side effect — no changes needed to
the niqud pipeline itself) and moving the morpheme breakdown into a new
optional `"parts"` array on each `WordEntry` (`word-analysis/src/
entry.rs`'s `WordPart`, `#[serde(default, skip_serializing_if =
"Vec::is_empty")]` so it's absent from JSON for the overwhelming
majority of words that have nothing to break down). `ollama.rs`'s
`HEBREW_PREFIX_GUIDANCE` (gated on a `contains_hebrew` check duplicated
from `niqud` rather than adding a crate dependency for one predicate)
asks for this with a concrete worked example. The Ctrl+A popup
(`WordAnalysisRow`'s `parts-label`) shows the breakdown as a small
second line under the translation, e.g. "ל = to · סרטים = movies", only
when non-empty.

## Hebrew prefix particles: merging by niqud's boundaries, not by exact text

The prompt guidance above isn't followed consistently — real captured
output for one sentence correctly fused one prefixed word but still
split two others into separate top-level entries in the same response.
An earlier fix compared Ollama's and niqud's word lists via an LCS
match on exact text equality, correcting pronunciation wherever both
sides matched verbatim. That can't fix a split word: a split
fragment's text ("ו", "אמר") never equals the fused token niqud
returns ("ואמר"), so it stayed both visually split and mispronounced.

`hebrew_word_merge::merge_by_niqud_boundaries` (`crates/app/src/
hebrew_word_merge.rs`) fixes this instead by trusting niqud's
whitespace-only tokenization as the word boundary, and growing a
window of consecutive Ollama entries (smallest first) until their
concatenated text matches niqud's current word — merging whichever
entries were consumed into one `WordEntry`, joining their translations
with a space and rebuilding `parts` from them. Runs once, inside
`apply_niqud_pronunciation`, before the analysis is cached — the Ctrl+A
popup and cache file only ever see the already-reconciled result.

## Hebrew word analysis: niqud's word list feeds the Ollama prompt, not the other way around

Even with the merge above, Ollama's own word count kept drifting from
niqud's in real use (e.g. logged as `ollama_words=31 niqud_words=30` on
a real subtitle line) — asking a token-based LLM to reproduce an exact
word count/order for a sentence it segments itself is inherently
unreliable, no matter how the prompt wording is tuned.

`word_analysis::analyze_sentence` (`crates/app/src/word_analysis.rs`)
now calls niqud *before* Ollama for a Hebrew sentence, and passes its
whitespace-split words to `OllamaClient::analyze_words` (`crates/
word-analysis/src/ollama.rs`) as a fixed JSON array the model fills in
`translation`/`pronunciation`/`parts` for — never asking it to decide
word boundaries itself. `merge_by_niqud_boundaries` still runs
afterward as a safety net for the rarer case where Ollama's response
doesn't match the given list either. Non-Hebrew sentences are
unaffected — they still use `analyze_sentence`'s free-text prompt,
since there's no niqud tokenization to pre-split them with.

## Word-level audio timing: a per-cue whisper-cli re-run, not JSON tokens

Building automatic pronunciation-practice audio needs to know where
each word starts/ends within a cue's known `[start, end)` span — data
nothing in the app produced before. `WhisperCliWordSegmenter::segment_words`
(`crates/subtitle/src/word_timing.rs`) gets it by cutting just that
span out of the source file with `ffmpeg` and re-running `whisper-cli`
on the clip with `-ml 1 -sow` (one word per output cue) plus, when the
loaded model maps to a known preset, `-dtw <preset>` for cross-
attention-based accurate word timing. A short, focused clip rather
than the whole file, since DTW alignment quality depends on it — and
`-osrt` output is reused via the existing `parse_srt`, since one word
per SRT cue already gives word/start/end without needing whisper.cpp's
separate JSON token-timestamp output.

`dtw_preset_for_model` (same file) infers the `-dtw` preset from the
model's filename: whisper.cpp model names use a dash before the
version (`ggml-large-v3.bin`) while its own preset tokens use a dot
(`"large.v3"`), so the stem is normalized before matching. An
unrecognized filename (e.g. a custom fine-tune) returns `None` rather
than guessing — `whisper-cli` hard-errors on an unknown `--dtw` value,
and the non-DTW word timestamps whisper.cpp falls back to are still
usable, just less precisely aligned.
