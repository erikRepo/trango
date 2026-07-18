# Release Notes

Format based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versions correspond to completed development steps, not per-commit semver bumps.

## [Unreleased]

### Added

### Changed
- Word analysis for Hebrew sentences now runs niqud's whitespace-based word split *before* asking Ollama anything, and hands Ollama that fixed word list to fill in translations for, instead of letting Ollama split the sentence itself and reconciling the mismatch afterward. Ollama's own free-text word splitting kept drifting from niqud's boundaries in real use (e.g. a 31-word Ollama split against niqud's 30, logged as a `tracing::warn` every time) even after several rounds of prompt wording fixes — asking a token-based LLM to fill in blanks for a list it's already given is far more reliable than asking it to reproduce an exact word count/order on its own. The niqud-boundary reconciliation added previously (`hebrew_word_merge::merge_by_niqud_boundaries`) still runs as a safety net for the rarer case where Ollama's response doesn't match the given list either

### Fixed

### Removed

## [0.1.54] - 2026-07-18

### Added
- Hebrew sentences' word-analysis pronunciation is now derived from a real niqud (vowel-point) diacritization model instead of Ollama's own unreliable guess (e.g. שכב "shkach" -> "sha-khav") — automatic (detected from script, no setting for *which* sentences), with graceful fallback to Ollama's own guess for whichever words can't be reconciled with niqud's (e.g. no model configured, or it fails to load). New `crates/niqud`: `contains_hebrew` gates the pipeline, `niqud_to_pronunciation` deterministically converts niqud text into a hyphenated Latin guide, `tokenizer.rs`/`decode.rs` reimplement the niqud model's tokenizer and output reconstruction directly (no `tokenizers` crate needed — confirmed to be character-level despite its WordPiece format), and `OnnxNiqudClient` runs the model via `ort` (ONNX Runtime bindings) with no subprocess/Python involved. Configured via Settings' new "Hebrew niqud model (.onnx)" field; see `docs/src/usage/word-analysis.md` for installing the model and `docs/src/developer/technology/ort.md`. The required ONNX Runtime library needs no manual setup either: the `.deb` package now depends on Ubuntu/Debian's `libonnxruntime1.23`, and trango finds it in the usual system library locations on its own — no `ORT_DYLIB_PATH` or other environment variable needed. Model loading also runs with a bounded timeout at startup, so an incompatible/broken ONNX Runtime install can no longer hang the whole app
- Settings screen: a gear icon in the top bar opens a dialog showing and editing every `config.toml` setting in one place — video folder, audio monitor source, and audio recording folder are plain text fields that save immediately; whisper model, Ollama model, target language, and the Hebrew niqud model reopen the same pickers/field already used elsewhere in the app. `audio_monitor_source` previously had no UI at all and required hand-editing `config.toml`. The Hebrew niqud model row started as a plain text field but was switched to an in-app folder picker (same chrome as the whisper/Ollama model rows) after a relative path silently failed to resolve depending on trango's working directory at launch — the picker always saves an absolute one. Picking a new niqud model shows "Restart TrangoPlayer to use this model", since the pick only takes effect on the next launch, not live
- Word analysis now breaks Hebrew's single-letter prefix particles (ו/ה/ב/כ/ל/מ/ש, e.g. לסרטים = ל "to" + סרטים "movies", written attached with no space) into a "parts" translation breakdown shown as a small second line in the Ctrl+A popup (e.g. "ל = to · סרטים = movies") — while `word`/`pronunciation` stay as the whole combined form, matching how it's actually pronounced together in speech (an earlier version split such words into separate top-level entries, which got both of those wrong). Since small local Ollama models don't reliably follow this even when asked, a word Ollama still splits despite it is merged back onto niqud's own whitespace-delimited word boundaries (niqud never splits a fused word) before it's ever shown or cached, so the popup always shows one row per actually-spoken word regardless of how Ollama split it internally
- Audio source's placeholder panel now always shows which folder a new recording will be saved to ("Saving to: …"), kept in sync with the Settings screen's audio-recording-folder field
- Starting a recording (Ctrl+Space/Rec) into a folder that doesn't exist now surfaces "Recording folder does not exist: …" in the Audio panel instead of silently failing — `ffmpeg`'s own error was previously discarded (`Stdio::null()`), so a missing folder looked like the shortcut/button did nothing

### Changed
- "Analyze all sentences" now retries a cue up to 3 times before giving up on it, instead of giving up after a single failed Ollama call — covers a transient hiccup (e.g. a model occasionally dropping a field from its JSON reply) without needing to rerun an otherwise-long batch. A cue that still fails after all retries is saved with an empty analysis rather than left out of the cache entirely, so the run moves on and isn't retried again on every future run

### Fixed
- Switching the top bar's Video/Audio source no longer leaves whatever was playing in the panel being hidden running silently behind the other one: clicking either segment now pauses mpv first. The visible panel's ScrubBar/SpeedSlider/mpv picture also only show once the actually-loaded file matches that panel (`AppWindow::media-ready`) — previously a loaded video's scrub bar and picture could bleed into the Audio panel just because *some* file was loaded, since both sources always shared one mpv instance
- Switching to the Audio source now also blanks the current-sentence card and sentence list until a matching file is loaded there, instead of leaving the Video source's sentence stuck on screen — Ctrl+A reports "No sentence is currently in focus" rather than analyzing that stale sentence. Cue navigation itself still never depends on which source is visible; only what's shown/analyzed as "current" does now

## [0.1.52] - 2026-07-17

### Added
- Independent Video/Audio source toggle in the top bar (`playback_state::MediaSource`), alongside the existing Normal/Sentence-by-sentence toggle — any combination of source and mode now works
- Audio source: Ctrl+Space starts/stops capturing the system's own audio output (e.g. a video playing in the browser) to a single WAV file via an `ffmpeg -f pulse -i <monitor-source>` subprocess. The PulseAudio/PipeWire monitor source is autodetected via `pactl get-default-sink`, overridable through `config.toml`'s `audio_monitor_source`. Linux/PulseAudio-PipeWire only. A failed start/stop (e.g. missing `pactl`/`ffmpeg`) surfaces an explanatory message in the Audio source's placeholder panel instead of only logging it
- Audio source's placeholder panel shows a Rec/Stop button (same command as Ctrl+Space) and the current recording's filename: a default `<date>_<time>.wav` name locked for the duration of the recording, editable afterwards (Enter commits a rename on disk). `config.rs`'s `audio_recording_folder` remembers the last folder a recording was written to, same principle as `video_folder`
- Audio source can open and play back an existing `.wav` file: the top bar's "Open…" button is now shared by both sources, listing video files in the Video source and `.wav` recordings in the Audio source. A picked or freshly recorded audio file loads through the same `video_player::VideoPlayer` path as a video, so the scrub bar/speed slider/play-pause and same-stem `.srt` auto-linking all work identically once one is loaded
- "Generate subtitles" now also works for the Audio source's recorded/opened `.wav` files, via the same "Subtitles…" button/dialog and `WhisperCliGenerator` the Video source uses — `WhisperCliGenerator::generate` skips its `ffmpeg` audio-extraction step for `.wav` input, since it's already audio
- Validated that sentence list, Ctrl+A word analysis, and the translation toggle work identically in the Audio source as in the Video source, since they never depended on a video being loaded — locked in with new tests that switch to the Audio source mid-run

### Fixed
- Pressing Space to replay a file that had already played to its end (Normal mode's/Audio's unbounded `VideoPlayer::toggle_playback`) looked like a no-op — mpv's `keep-open=yes` pauses at EOF rather than unloading, but unpausing there without seeking just re-hits the same EOF. `toggle_playback` now checks mpv's `eof-reached` property and seeks back to `0` first, so Space restarts playback from the beginning instead

## [0.1.51] - 2026-07-16

### Added
- CI on every pull request against `master`: `scripts/check.sh` (fmt + clippy), `scripts/test.sh`, and a release-profile build, each as a separate GitHub Actions job (`.github/workflows/ci.yml`)
- GitHub Actions workflow (`.github/workflows/release-deb.yml`) publishes a `.deb` package as a GitHub Release whenever the workspace version in `Cargo.toml` changes on `master` — skips if a release for that version already exists. Packaging metadata lives in `crates/app/Cargo.toml`'s `[package.metadata.deb]` (`cargo-deb`)

### Fixed
- CI's `test` job failed on GitHub's headless runners: constructing `AppWindow` needs a working windowing backend even without ever showing the window, contrary to what `docs/src/developer/technology/slint.md` previously claimed. `scripts/test.sh` now runs under `xvfb-run` in CI, and the docs are corrected

## [0.1.50] - 2026-07-16

### Fixed
- Current-sentence card's original-language text had no bounded height to wrap against, so long cues could get squeezed shorter than their wrapped line count (whenever the sentence panel ran low on vertical room) and lose their bottom line(s) without any indication. It now scrolls inside a fixed-height box, same pattern already used for the translation line below it, so long sentences are always fully reachable instead of silently clipped. Note: this does not fix the separate bidi text-rendering glitch with mixed Hebrew/Latin cues — see `docs/src/developer/specs.md`, accepted as a known upstream Slint limitation for now

## [0.1.49] - 2026-07-16

### Added
- Always-visible playback-speed slider below the scrub bar, in both Normal and Sentence by sentence mode. Maximum is normal speed (1.0x); dragging left only slows the video down, snapping to 0.05 increments between 0.5x and 1.0x, with "0.5x"/"0.75x"/"1.0x" markers under the track — useful for language learners who want to slow a line down without losing per-sentence navigation

## [0.1.48] - 2026-07-16

### Fixed
- In Normal mode, the current-sentence card and Ctrl+A word analysis never updated as the video played past the sentence that was current when Normal mode was entered (or last navigated to) — `current_cue_index` only ever moved on explicit navigation, which Normal mode doesn't use. Both now follow along live as continuous playback (or a scrub-bar seek) moves through the subtitle

## [0.1.47] - 2026-07-16

### Added
- Scrub bar (Normal mode) can now be clicked or dragged to seek to any point in the video, instead of only showing progress. Never changes play/pause state — it just relocates the playhead
- Bottom hint bar now shows in Normal mode too, listing whichever shortcuts actually work there (Space · play/pause, Ctrl+T, Ctrl+A) — previously it only appeared in Sentence by sentence mode

## [0.1.46] - 2026-07-16

### Added
- The Open Video dialog now remembers the last folder a video was successfully opened from (`config.toml`'s new `video_folder` field) and defaults to it on the next run, instead of always resetting to the CLI argument's folder or the current working directory. Still overridden by an explicit `trango <path/to/video>` CLI argument for that run; updates every time a different folder's video is opened, so it always reflects the most recent one

## [0.1.45] - 2026-07-16

### Fixed
- Space (and arrow-key/sentence-list navigation) could stop responding entirely after using the Open Subtitles dialog's "Target language" field (added in 0.1.41, the app's first editable text input) — that `LineEdit` grabs keyboard focus while the dialog is open, and since the dialog is destroyed on close, nothing reclaimed focus for `nav-focus` afterward, so all keyboard shortcuts silently went nowhere. `app-window.slint` now returns focus to `nav-focus` whenever the Open Subtitles dialog closes
- Playing a video to its end in Normal mode left mpv's core idle (unloaded), breaking every subsequent seek — Space, arrow-key/sentence-list navigation, even the scrub bar — until the video was reloaded from scratch. This exact failure mode was already known and worked around for one specific trigger (generating subtitles mid-playback, see `docs/src/specs/README.md`), but a video simply reaching its own natural end had no such recovery. mpv is now initialized with `keep-open=yes`, so it stays loaded and seekable after EOF instead of going idle

## [0.1.44] - 2026-07-16

### Changed
- Renamed stale/ambiguous UI labels: top bar's "Open subtitles…" button (which opens a whole subtitle-management modal, not just a file picker) is now "Subtitles…"; the current-sentence card's "Translation" toggle label and the Open Subtitles dialog's "Translation" section header are now "Secondary subtitle" (more descriptive of what the toggle/section actually control); bottom hint bar's "ctrl+t · toggle translation" is now "ctrl+t · toggle secondary subtitle". Purely cosmetic — underlying `show-translation`/`toggle-translation`/`translation-linked` properties and callbacks are unchanged

## [0.1.43] - 2026-07-16

### Added
- `--debug` CLI flag (`cargo run -p trango -- --debug video.mp4`, works anywhere among the other arguments) — turns on `debug`-level logging scoped to trango's own crates (including the Ollama prompt/response logging added in 0.1.42), without needing to export `RUST_LOG`. `CLAUDE.md`'s Rust conventions now say to prefer a CLI flag or `config.toml` over environment variables for this kind of setting
- Bottom hint bar: "ctrl+a · word analysis" hint, which TODO.md Vaihe 24 (Ctrl+A word analysis) had left off the hint bar

### Changed
- `RUST_LOG` still works as a lower-level fallback when `--debug` isn't passed, but is no longer the primary documented way to enable debug logging

## [0.1.42] - 2026-07-16

### Added
- `RUST_LOG` environment variable now actually filters log output (`crates/app/Cargo.toml` enables `tracing-subscriber`'s `env-filter` feature, `main.rs`'s new `init_logging`) — previously silently ignored despite `docs/src/technology/tracing.md` claiming it worked. `crates/word-analysis/src/ollama.rs`'s `analyze_sentence` logs the full prompt sent to Ollama and the raw response text at `debug` level, e.g. `RUST_LOG=word_analysis=debug cargo run -p trango -- video.mp4`

### Fixed
- Word analysis ("Analyze all sentences"/Ctrl+A) failed on every cue against a real Ollama instance running a reasoning-capable model (e.g. `qwen3.5`) — `failed to parse Ollama response: EOF while parsing a value at line 1 column 0`. The model spent its whole generation budget "thinking" instead of answering, leaving the `response` field empty, because `GenerateRequest` never disabled extended reasoning. Now sends `"think": false` (matching gemhunter's `call_ollama`), and an empty `response` is caught explicitly with a clear error message instead of forwarding `serde_json`'s zero-length-input parse error

## [0.1.41] - 2026-07-16

### Added
- Open Subtitles dialog: "Target language" field (TODO.md Vaihe 24.1) — a free-text `LineEdit` (Slint's `std-widgets`, the first editable text input anywhere in trango) next to the Ollama model row, replacing word analysis's previously hardcoded `"English"` target language. Saves on every keystroke to `config.toml`'s new `ollama_target_language` field, the same way picking an Ollama/whisper model persists immediately; both the Ctrl+A popup and "Analyze all sentences" now read the typed language instead of the fixed default

## [0.1.40] - 2026-07-16

### Added
- TODO.md Vaihe 24 documented retroactively (word-by-word Ollama analysis via Ctrl+A and "Analyze all sentences"), part 6/6 — `docs/src/specs/README.md`'s "Word analysis: local Ollama, not a cloud API" section (crate split, HTTP client choice, prompt/cache design, Ctrl+A and batch-loop behavior), `docs/src/usage/README.md`'s "Word analysis with local Ollama" section (Ollama install/model requirements, usage), and TODO.md's own "Vaihe 24" entry

## [0.1.39] - 2026-07-16

### Added
- Ctrl+A word-analysis popup (TODO.md Vaihe 24, part 5/6): analyzes the sentence currently shown in the current-sentence card word-by-word via a local Ollama model, showing each word's translation and pronunciation guide in a new modal (`WordAnalysisPopup` in `app-window.slint`). Checks the subtitle's `.wordanalysis.json` cache file first (read fresh from disk each time, so it always reflects whatever "Analyze all sentences" or an earlier Ctrl+A press already wrote) and only calls Ollama on a cache miss, writing the result back to the same cache file once it completes. Not mode-gated, same as Ctrl+T — works in both Normal and Sentence-by-sentence mode. Requires an Ollama model to be selected and a subtitle to be linked; shows a clear inline error otherwise

## [0.1.38] - 2026-07-16

### Added
- Open Subtitles dialog: "Analyze all sentences" button (TODO.md Vaihe 24, part 4/6) — new `crates/app/src/word_analysis.rs`'s `spawn_batch_analyze` loops every cue in the currently linked subtitle on a background thread, skipping cues already present in that subtitle's `.wordanalysis.json` cache file and saving newly analyzed ones to it as it goes (not just once at the end, so an interrupted run keeps what it already finished). A cue that fails to analyze is logged and skipped rather than aborting the whole run. Progress ("Analyzing N / M…") and errors surface inline on the button; disabled until an Ollama model is selected

## [0.1.37] - 2026-07-16

### Added
- Open Subtitles dialog: "Ollama model" row (TODO.md Vaihe 24, part 3/6) — opens a model picker (new `crates/app/src/ollama_model_picker.rs`, reusing `FileListDialog`'s chrome) listing models a local Ollama instance reports installed, fetched on a background thread since it's a network call (unlike the whisper model picker's synchronous filesystem listing). Picking a model persists it to `config.toml`'s new `ollama_model` field, the same way the whisper model is persisted

## [0.1.36] - 2026-07-16

### Added
- `crates/app/src/config.rs`: `TrangoConfig.ollama_model` (TODO.md Vaihe 24, part 2/6) — persists the Ollama model picked for word-by-word sentence analysis across restarts, the same way `whisper_model_path` already does for whisper.cpp

## [0.1.35] - 2026-07-16

### Added
- New `crates/word-analysis` crate (TODO.md Vaihe 24, part 1/6): pure, Slint/libmpv-free data model and client for word-by-word sentence analysis via a local Ollama instance — `WordAnalysis`/`WordEntry`, `AnalysisCache` with `cache_path_for`/`load_cache`/`save_cache` (a JSON sidecar file next to the subtitle, e.g. `subs.srt` -> `subs.wordanalysis.json`, missing/corrupt file falling back to an empty cache the same way `crates/app/src/config.rs` does), the `OllamaClient` trait plus an `ureq`-backed `HttpOllamaClient` (`list_models` via `GET /api/tags`, `analyze_sentence` via `POST /api/generate` with `format: "json"`), and `build_prompt`/response parsing as pure, independently testable functions. New dependencies `ureq` (synchronous HTTP client — trango has no async runtime, matching the existing `std::thread::spawn` background-work pattern) and `serde_json`, both approved by the user beforehand per CLAUDE.md

## [0.1.34] - 2026-07-16

### Fixed
- `crates/app/src/video_player.rs`, `app-window.slint`: mpv's rendering notifier drew the video frame scaled to fill the *entire window*, relying on every other UI element painting an opaque background over the parts that weren't the video area — an assumption that broke wherever the layout left transparent gaps (the `HorizontalLayout` padding/spacing around the sentence panel), letting the video bleed through at the window's right edge behind/around the sentence cards. mpv now renders into its own offscreen `VideoSurface` (new `crates/app/src/video_player/gl_video_surface.rs` submodule) sized to the video frame's actual on-screen box — exposed from Slint via new `AppWindow` properties `video-frame-x`/`-y`/`-width`/`-height` — then blits that into place with `glBlitFramebuffer`, confining it exactly to its box regardless of window size
- `app-window.slint`: the window didn't actually resize — two compounding bugs. First, `AppWindow` bound the root `Window`'s `width`/`height` directly to `960px`/`660px`; per Slint's `Window` docs that makes the window a *fixed* size at the window-manager level (`min-width`==`max-width`), so maximizing only stretched the OS-level frame/decorations while Slint's own content stayed clamped to `960x660` in the corner, leaving the rest of the (visually bigger) window an unpainted gap — fixed by using `preferred-width`/`-height` instead, which only set the initial size. Second, even with the window genuinely resizable, the main content column (top bar + video/sentence-panel row + hint bar) was a plain child of `nav-focus`, a `FocusScope` — which, unlike a `Layout`, doesn't stretch an arbitrary single child to fill itself, so the column would still sit at its own natural content size; given `width: 100%; height: 100%;` so it now actually fills the window. Together these mean the video area (now correctly confined to its own box by the fix above) grows and shrinks with the window, including maximized/fullscreen-sized ones
- `app-window.slint`: `CurrentSentenceCard`'s translation line had no height cap, so the card — and with it, the boundary above the sentence list below — visibly grew and shrank every time the current cue's translation changed length. The translation line now sits in a fixed-height (`84px`, ~3 lines) `ScrollView`, scrolling internally for longer translations instead of resizing the card

## [0.1.33] - 2026-07-16

### Fixed
- `crates/playback-state/src/navigation.rs`: `PlayerState::repeat_current_cue` never checked `self.mode`, so in `Normal` mode with a subtitle linked (`current_cue_index` is set to `Some(0)` by `set_cues` regardless of mode), Space still routed to the bounded `toggle_play_span` instead of the unbounded `toggle_playback` added in 0.1.32 — playback would start, then immediately auto-pause at the end of whatever cue happened to be in focus, instead of continuing normally. `repeat_current_cue` now returns `None` outright in `Normal` mode

## [0.1.32] - 2026-07-15

### Added
- `crates/app/src/video_player.rs`: `VideoPlayer::toggle_playback` — a plain, unbounded play/pause toggle (no seek, no `pause_at` armed), used when there's no current cue to bound playback to

### Fixed
- Space (play/pause) didn't work in `Normal` mode at all — combined with "no mode autoplays" (0.1.30/0.1.29), this meant a video opened in `Normal` mode, or in `SentenceBySentence` mode before any subtitle was linked, could never be started. Right/Left/Space were all gated behind `sentence-mode-active` in `app-window.slint`'s `key-pressed` handler; Space is now pulled out from behind that guard (Right/Left stay gated — cue navigation still needs `TODO.md` Vaihe 21's `Normal` mode work). `main.rs`'s `repeat-cue` handler (the one callback Space always invokes) now falls back to `toggle_playback` when `PlayerState::repeat_current_cue` returns `None`, instead of doing nothing

## [0.1.31] - 2026-07-15

### Fixed
- `crates/app/src/video_player.rs`: Space could still replay the *next* sentence instead of the current one even after the previous fix — `sync_current_sentence` and `apply_pending_pause` run in the same poll tick, `sync_current_sentence` first, so on the exact tick `time-pos` first reached a contiguous cue's shared end/next-start boundary, `pause_at` was still armed (not cleared until immediately after, same tick) and the cursor got reclassified before the pause even landed. Gating on "has `pause_at` cleared" couldn't fix a bug that happens before clearing

### Removed
- `sync_current_sentence` (`video_player.rs`) and `PlayerState::sync_cue_to_time` (`playback-state`), rather than patching the bug above a third time — under the current model every play action is a bounded single-cue span whose cue is already known from the moment it starts, so there's nothing left for live `time-pos`-based cue rediscovery to correctly do (see `docs/src/specs/`'s "`sync_current_sentence` removed entirely" for the full reasoning, including what would need it back: `Normal` mode continuous playback or scrub-bar dragging, neither of which exists yet)

## [0.1.30] - 2026-07-15

### Fixed
- `crates/app/src/video_player.rs`: opening a video with no subtitle linked yet (or in `Normal` mode) started playing immediately on its own, contradicting the "no mode autoplays" behavior just added — `pause_and_arm_start_seek_if_sentence_mode` only paused mpv when `SentenceBySentence` mode already had cues loaded, returning early otherwise. Renamed to `pause_and_arm_start_seek` and restructured so the initial pause always happens; only arming a seek to the first cue's start stays conditional on `SentenceBySentence` mode with cues present

## [0.1.29] - 2026-07-15

### Fixed
- `crates/app/src/video_player.rs`: pressing Space to repeat the current sentence could instead replay the *next* one — real speech-to-text output commonly produces contiguous cues (cue N's `end` equals cue N+1's `start`), so pausing exactly at a cue's end also matched the next cue's start, and the live `time-pos`-driven sentence tracking (`sync_current_sentence`) silently reclassified the cursor there right after auto-pausing. It now only re-derives the cursor from `time-pos` while a span is actually still playing toward its own scheduled pause (`pause_at` armed), leaving it alone once paused for any reason

### Changed
- **Right/Left/sentence-list navigation no longer starts playback.** They now only seek to the target cue's start and leave mpv paused there (`VideoPlayer::seek_and_pause`, replacing `apply_seek_command`); **Space is now the only thing that starts or stops playback**, toggling between playing the current cue's span (auto-pausing at its end, same as before) and pausing immediately if pressed again while that's still playing (`VideoPlayer::toggle_play_span`) — nothing plays until asked to, in any mode. See `docs/src/specs/`'s "No mode autoplays" for the full reasoning (this was chosen as the fix for the bug above, not just alongside it)
- `playback_state::SeekCommand` dropped `end`/`then_pause` (only `start` now — `next_cue`/`previous_cue`/`jump_to_cue`'s contract is now purely "land here, paused"); new `playback_state::PlaySpanCommand { start, end }` carries what `repeat_current_cue` (Space) needs instead
- README.md: Right/Left/Space's documented behavior updated to match

## [0.1.28] - 2026-07-15

### Fixed
- `crates/app/src/main.rs`: cue-navigation seeking (arrow keys / sentence list) could break after "Generate subtitles" finished for the video already open and playing — generation can take long enough that a short video reaches EOF and idles mpv's core while it runs, and a seek issued to an idle core fails outright (mpv error `Raw(-12)`). `wire_open_subtitles_dialog`'s `subtitle-generated` handler now reloads the video (the same way opening it fresh does) once the newly generated subtitle is loaded, re-arming the sentence-by-sentence start-of-playback seek and recovering a normal, seekable state

### Changed
- `crates/app/src/main.rs`: `wire_open_subtitles_dialog` takes a `reload_video: impl Fn(&AppWindow, &Path, &PlayerState)` closure instead of needing a `Rc<video_player::VideoPlayer>` directly, so it stays testable without a real mpv render context (which `VideoPlayer::attach` needs and this module's tests don't have)

## [0.1.27] - 2026-07-15

### Added
- `subtitle::WhisperCliGenerator`: new `ffmpeg_path` field — `generate` now always extracts the video's audio to a temporary 16kHz mono WAV file with `ffmpeg` before handing it to `whisper-cli`, since whisper-cli only reads raw audio formats (`flac`/`mp3`/`ogg`/`wav`), not video containers, and previously exited 0 even when it silently failed to read one, surfacing as a misleading generic "no subtitle file was found" error (found via real end-to-end testing, see `docs/src/specs/`)
- `crates/subtitle/src/generate.rs`: `run_command` retries briefly on a transient `ExecutableFileBusy` (`ETXTBSY`) — a race hit occasionally when running a binary immediately after writing it (this crate's own tests do exactly that against fake ffmpeg/whisper-cli scripts)
- `tracing` added to the `subtitle` crate (`crates/subtitle/Cargo.toml`) for logging around the extraction/transcription steps; more `info!` logging around model selection and generation start in `crates/app/src/main.rs`
- `docs/src/usage/`: `ffmpeg` install/requirement note and `TRANGO_FFMPEG_PATH`; `docs/src/specs/`: the audio-extraction bug/fix writeup and the test-design split (`extract_audio`/`run_whisper_cli` as separately testable private methods)

### Changed
- `crates/subtitle/src/generate.rs`: `WhisperCliGenerator::generate` is now a thin wrapper delegating to `extract_audio` + `run_whisper_cli`; existing whisper-cli tests moved to call `run_whisper_cli` directly (they were never testing audio extraction) and gained a new end-to-end test proving the two steps are actually wired together

## [0.1.26] - 2026-07-15

### Added
- `crates/app/src/config.rs`: `TrangoConfig`, trango's first persisted settings file (`$XDG_CONFIG_HOME/trango/config.toml`, falling back to `$HOME/.config/trango/config.toml`) — remembers the picked whisper.cpp model and last-browsed folder across restarts; new Cargo dependencies `serde` + `toml` (asked and approved before adding, see `docs/src/technology/`)
- `crates/app/src/model_picker.rs`: whisper.cpp model selection (`TODO.md` Vaihe 21.6) — `list_folder_entries`/`default_start_folder` for an in-app `.bin`/`.gguf` folder browser with best-effort autodiscovery of a starting folder (a few common whisper.cpp install locations, then `./models`), and `language_flag`/`display_name` inferring `-l en`/`-l auto` from whisper.cpp's own `.en` filename convention (its own `--language` default is always `en` regardless of the loaded model)
- `subtitle::WhisperCliGenerator`: new `language` field, passed as `-l` when set
- `app-window.slint`: Open Subtitles dialog gained a model row ("select a whisper model…" / "whisper model: `<name>` (change)") next to "Generate subtitles", which is now disabled until a model is picked; a new `FileListDialog` instance (`is-model-picker-dialog-open` etc., same chrome as the Open Video dialog and translation-link picker) for picking one
- `docs/src/usage/`: UI-driven model selection replaces the old `TRANGO_WHISPER_MODEL_PATH` env var in the docs, plus a note that non-English languages (Hebrew was the concrete case) need a `medium`/`large-v3` multilingual model for good quality, not `base`/`small`; `docs/src/specs/`: the autodiscovery/persistence/language-inference design

### Changed
- `crates/app/src/main.rs`: `generate-subtitles-requested` now reads the model from a shared `Rc<RefCell<Option<PathBuf>>>` (set by the new model picker, loaded from `config::load()` at startup) instead of the `TRANGO_WHISPER_MODEL_PATH` environment variable; `whisper_cli_generator_from_env` renamed `whisper_cli_generator`, taking the model path as a parameter

## [0.1.25] - 2026-07-15

### Added
- `crates/subtitle/src/generate.rs`: `WhisperCliGenerator`, a real `SubtitleGenerator` (`TODO.md` Vaihe 21.5) that runs whisper.cpp's `whisper-cli` binary as an external process (no new Cargo dependency — see `docs/src/specs/`) via `-f`/`-m`/`-of`/`-osrt`, writing the same same-stem `.srt` convention `StubSubtitleGenerator` uses; `binary_path`/`model_path` are configurable, defaulting to a `PATH` lookup and whisper-cli's own default model lookup respectively
- `crates/subtitle/src/error.rs`: `SubtitleError::GenerationFailed(String)` for whisper-cli failures (binary not found, non-zero exit, missing output file), with a message meant to be shown to the user as-is
- `crates/app/src/subtitle_generation.rs`: `spawn_generate` runs a generator on a background thread, since real transcription can take seconds to minutes and would freeze the UI thread if run synchronously; `apply_result` mirrors a finished generation's outcome into the window
- `app-window.slint`: `AppWindow::subtitle-generation-error-message`, shown under the empty-state row in the `Error` status instead of always saying "Generation failed"; `AppWindow::subtitle-generated(string)`, an internal signal (not tied to any UI element) letting the background-thread completion handler — which may only carry `Send` data across the thread boundary — hand a generated subtitle's path to UI-thread code that holds the `Rc`-based player/media state needed to load it
- `docs/src/usage/`: `whisper-cli` install instructions for Linux/Windows, model download, and the `TRANGO_WHISPER_CLI_PATH`/`TRANGO_WHISPER_MODEL_PATH` environment variables; `docs/src/specs/`: the whisper.cpp-as-external-process decision and the background-thread/`Send`-boundary architecture

### Changed
- `crates/app/src/main.rs`: `wire_open_subtitles_dialog`'s `generate-subtitles-requested` handler now runs `subtitle::WhisperCliGenerator` (configured from environment variables via `whisper_cli_generator_from_env`) on a background thread instead of `subtitle::StubSubtitleGenerator` synchronously

## [0.1.24] - 2026-07-15

### Added
- `crates/subtitle/src/generate.rs`: `SubtitleGenerator` trait (`fn generate(&self, video_path: &Path) -> Result<PathBuf, SubtitleError>`) plus `StubSubtitleGenerator`, a placeholder implementation that writes a single fixed-text cue to a same-stem `.srt` next to the video — no speech-to-text library added yet, that's a separate later step needing its own go-ahead (`TODO.md` Vaihe 20)
- `app-window.slint`: `SubtitleGenerationStatus` enum (`Idle | Generating | Done | Error`, README's `subtitleGenerationStatus`) and `AppWindow::subtitle-generation-status` property; `OpenSubtitlesDialog`'s empty-state "Generate subtitles" button and label now reflect it ("Generating…" while running, "Generation failed" / "Try again" on error), and a new `Palette.error-text` token colors the error state
- `crates/app/src/subtitle_generation.rs`: `generate` runs a `SubtitleGenerator` synchronously against the current video, mirroring `Idle -> Generating -> Done`/`Error` into the window and, on success, the dialog's original row (`open_subtitles_dialog::mark_original_linked`)

### Changed
- `crates/app/src/main.rs`: `wire_open_subtitles_dialog`'s `generate-subtitles-requested` handler is no longer a no-op stub — it runs `subtitle::StubSubtitleGenerator` via `subtitle_generation::generate` and, on success, loads the generated subtitle into the player and records it in `CurrentMedia`, the same as picking a translation already did
- `crates/app/src/open_subtitles_dialog.rs`: `open_dialog` now resets `subtitle-generation-status` to `Idle` each time the dialog opens, so a previous video's Done/Error state doesn't leak into a newly scoped dialog

## [0.1.23] - 2026-07-15

### Added
- `crates/app/src/open_subtitles_dialog.rs`: Open Subtitles dialog (`TODO.md` Vaihe 19) — `open_dialog` shows the video's original-language subtitle as a linked row if found (or the empty "No subtitle file found" state + a stub "Generate subtitles" button, real generation lands in Vaihe 20) and its translation the same way; `list_srt_files` lists a folder's `.srt` files for the translation-link file picker. README specs linking the translation via OS drag-and-drop, but Slint 1.17.1's winit backend doesn't relay external file drops to `DropArea` (only in-app `DragArea` sources, of which this dialog has none) — linking instead goes through a small in-app picker reusing the Open Video dialog's file-list chrome
- `app-window.slint`: `OpenVideoDialog` generalized into `FileListDialog` (`title`/`confirm-label` properties), reused by both the Open Video dialog and the new translation-link picker; `OpenVideoRow` renamed `FileListRow` to match. New `OpenSubtitlesDialog` component (pixel reference `sketch/design_reference.dc.html#2a`, right mock) plus `LinkedFileRow`/`EmptyFileRow` sub-components — dashed borders in the mock are approximated with a solid muted border since Slint has no dashed-border support
- `crates/app/src/main.rs`: `CurrentMedia` tracks the currently open video/subtitle/translation paths so the dialog knows what it's scoped to without re-deriving it from disk (a CLI-loaded subtitle may not share the video's filename stem); `wire_open_subtitles_dialog` wires the dialog, the translation-link picker, and the "Generate subtitles" stub. `load_subtitles` now returns whether it actually loaded, used to decide whether to record a path in `CurrentMedia`

### Changed
- `crates/app/src/main.rs`: `open_selected_video`/`wire_open_video_dialog` thread `CurrentMedia` through, resetting it (clearing any previous translation link) whenever a new video is opened

## [0.1.22] - 2026-07-15

### Fixed
- `crates/app/src/video_player.rs`: video opened via the Open Video dialog with no CLI video argument would show but then never respond to Right/Left/Space/sentence-list navigation (every seek logged `failed to seek mpv err=Raw(-12)`) — root cause was `VideoPlayer::attach` being called lazily, only once a video was actually picked, but Slint's `RenderingState::RenderingSetup` notification (needed to create mpv's render context and issue the initial `loadfile`) only ever fires once per window, on its very first rendered frame; by the time the dialog had been used, that frame had long since rendered, so `RenderingSetup` never fired for the newly-registered notifier and mpv's core stayed permanently idle. `VideoPlayer::attach` now always runs once, unconditionally, right after the window is created (with `video_path: Option<&Path>`, `None` when trango starts without a CLI video argument) — see `docs/src/architecture/video-playback.md` for the full explanation

### Changed
- `crates/app/src/video_player.rs`: `VideoPlayer::load_video` is now the only path a video is ever loaded through after `attach` (both the CLI-argument video, if any, and any later Open Video dialog pick) — `main.rs` no longer branches between "attach a fresh `VideoPlayer`" and "load into the existing one"; there's always exactly one, from startup. `load_file` (the shared `loadfile` + start-seek-arming helper) now also sets `video-loaded`, since that no longer always follows `setup_render_context`

## [0.1.21] - 2026-07-15

### Added
- `crates/app/src/open_video_dialog.rs`: the Open Video dialog can now switch folders in-app — `list_folder_entries` replaces `list_video_files`, returning a `FolderEntry` per row (`Up` for the listed folder's parent, `Folder` for subfolders, `Video` for video files, sorted subfolders-then-videos-then-alphabetically), and clicking an `Up`/`Folder` row re-lists into that folder instead of selecting it (only `Video` rows are selectable/openable). `app-window.slint`'s `OpenVideoRow` (renamed from `OpenVideoFileRow`) gained `is-navigable`, rendering navigable rows without a chip/size line and in a heavier weight

### Changed
- `crates/app/src/main.rs`: `wire_open_video_dialog`'s `select-open-video-row` handler now branches on the clicked row's `FolderEntry` kind — navigating (re-listing and re-opening the dialog against the new folder) for `Up`/`Folder`, marking-selected for `Video`, same as before

## [0.1.20] - 2026-07-15

### Added
- `crates/app/src/open_video_dialog.rs`: Open Video dialog (`TODO.md` Vaihe 18) — `list_video_files` lists a folder's video files (`.mp4`/`.mkv`/`.webm`/`.mov`/`.avi`, case-insensitive) sorted by name, with a formatted size label (`format_file_size`, e.g. "340 MB") read via `std::fs::metadata`; duration is deferred to a later iteration since it would need decoding the file with libmpv/ffprobe rather than a cheap metadata read. `matching_subtitle_path` looks for a same-stem `.srt` next to a video, backing the "attempts to auto-match a same-name subtitle file" part of README's Open Video spec
- `app-window.slint`: `OpenVideoFileRow` struct and `OpenVideoDialog` component — modal backdrop + card matching `sketch/design_reference.dc.html#2a` (left mock): header with title + "✕", scrollable file-type-chip/name/size rows (selected row accent-tinted), footer Cancel/Open buttons. `GhostButton` gained a `clicked` callback, wired on the top bar's "Open video…" button (`Palette` gained `modal-bg`/`chip-muted` tokens for it) — "Open subtitles…" stays static until Vaihe 19
- `crates/app/src/main.rs`: `default_video_folder` resolves the dialog's default folder — the CLI video path's parent directory if one was given, otherwise the current working directory. `wire_open_video_dialog` lists that folder and opens the dialog on "Open video…", tracks row selection, and on "Open" calls the new `open_selected_video`, which auto-matches and loads a subtitle file (or clears stale cues if none match) before loading the video — attaching a fresh `VideoPlayer` if trango was started without a CLI video argument, or telling the existing one to load the new file via `VideoPlayer::load_video` otherwise
- `crates/app/src/video_player.rs`: `VideoPlayer::load_video(&self, video_path, player_state)` loads a new file into an already-attached mpv core, re-arming the sentence-by-sentence start pause/seek for it — used by the Open Video dialog when a video was already playing

### Changed
- `crates/app/src/video_player.rs`: the `loadfile` + sentence-by-sentence start-seek arming logic in `setup_render_context`'s tail was extracted into a shared `load_file` helper, now reused by both the very first video load and `VideoPlayer::load_video`
- `crates/app/src/main.rs`: `main` now holds the attached `VideoPlayer` behind `Rc<RefCell<Option<Rc<VideoPlayer>>>>` instead of a plain `Option`, so the Open Video dialog can attach one lazily (no CLI video argument given) or reuse the existing one (switching videos mid-session)

## [0.1.19] - 2026-07-15

### Added
- `app-window.slint`: **Ctrl+T** keyboard shortcut toggles translation visibility, handled first in `nav-focus`'s `key-pressed` (before the `sentence-mode-active` guard) since translation display is purely visual and independent of playback mode; calls the same `toggle-translation` callback as the current-sentence card's toggle switch. `HintBar` gained a fourth hint, "ctrl+t · toggle translation"

### Changed
- `app-window.slint`: `AppWindow`'s default height grew from 600px to 660px — `CurrentSentenceCard`'s Vaihe 17 header row ("Translation" label + toggle switch) and conditional translation line left no slack for the bottom `HintBar` at 600px, clipping it off the bottom of the fixed-height window whenever the sentence panel's content grew tall enough

## [0.1.18] - 2026-07-15

### Added
- `app-window.slint`: `ToggleSwitch` component (pill track, animated knob, accent-filled when on) and a "Translation" label + switch in `CurrentSentenceCard`'s header row; the translation line itself renders below the divider, in `Palette.translation-text` (`#7fa6f0`), only while `show-translation` is on
- `crates/app/src/main.rs`: `translation_path_from_args` reads a third CLI argument (`trango video.mp4 subs.srt subs.en.srt`); `load_subtitles` merges it into the loaded cues via `subtitle::merge_translation` when given. `wire_player_state` now also wires the window's `toggle-translation` callback to `playback_state::PlayerState::toggle_translation()`, mirroring `show_translation` into the window's `show-translation` property, off by default — the same pattern already used for `toggle-mode`/`sentence-mode-active`
- `crates/app/src/sentence_card.rs`: `update_sentence_card` now also sets `translation-text` from the current cue's merged `translation` (empty string if none), independent of the toggle's own visibility state
- `test-media/sample/sample.fi.srt`: Finnish translation fixture for `sample.srt`, same five cue timings, used to exercise `merge_translation` and the translation toggle without a second generated audio track

### Changed
- `crates/app/src/main.rs`: subtitle file reading + `parse_srt` was extracted from `load_subtitles` into a reusable `parse_subtitle_file` helper, now shared between the original and translation subtitle loads

## [0.1.17] - 2026-07-15

### Added
- `playback_state::PlayerState::jump_to_cue(index: usize)`: moves the cursor directly to `index` and returns the same `SeekCommand` shape as `next_cue`/`previous_cue`, reusing the shared `seek_command_for` helper — `None`, cursor untouched, if `index` is out of range
- `app-window.slint`: `SentenceListRow` struct and `SentenceListCard` component — the scrollable "index · text" sentence list underneath `CurrentSentenceCard`, with the current cue highlighted via an accent-tinted pill and clicking a row emitting `jump-to-cue(index)`. Auto-scrolls the clicked/synced row into view via a `bring-into-view` function modeled on Slint's own `StandardListViewBase`
- `crates/app/src/sentence_list.rs`: `update_sentence_list(&AppWindow, &PlayerState)` mirrors the loaded cues into the window's `sentence-list-rows`/`sentence-list-current-index` properties, split from a pure `sentence_list_rows` helper so the mapping is unit-tested without a Slint window
- `crates/app/src/main.rs`: `wire_cue_navigation` now also wires `on_jump_to_cue`, driving `PlayerState::jump_to_cue` from sentence list row clicks — the same post-navigation handling (`apply_navigation_result`) as arrow/space key presses, so both paths behave identically per README's "Sentence list" spec

### Changed
- `crates/app/src/main.rs`: `cue_navigation_handler`'s per-callback body was extracted into a shared `apply_navigation_result` (refreshes the sentence card and sentence list, then applies any produced `SeekCommand`), now reused by both key-driven navigation and the sentence list's row-click handler instead of duplicating the logic
- `video_player.rs`: `sync_current_sentence` only rebuilds the sentence list's model when the synced cue index actually changes (comparing against the previous `current_cue_index`), since it otherwise runs on every `SCRUB_BAR_POLL_INTERVAL` tick

## [0.1.16] - 2026-07-15

### Changed
- `playback_state::PlaybackMode::default()` is now `SentenceBySentence` (was `Normal`) — the primary language-learning use case, so a fresh player starts there; `main.rs`'s `wire_player_state` mirrors this into the window's `sentence-mode-active` property right after wiring, since `app-window.slint` itself still hardcodes `false`
- `video_player.rs`: after `loadfile`, a new `pause_and_arm_start_seek_if_sentence_mode` pauses mpv immediately and arms a deferred seek to the first loaded cue's start when the shared `PlayerState` is in `SentenceBySentence` mode, instead of continuing to autoplay — a no-op in `Normal` mode or with no cues loaded. The seek itself is applied by `apply_pending_start_seek` on the next scrub bar poll tick rather than right after `loadfile`, since mpv's `seek` command errors (`Raw(-12)`) if issued before the core has actually finished loading a file

## [0.1.15] - 2026-07-15

### Added
- `app-window.slint`: `AppWindow` gains a `nav-focus` `FocusScope` (held via `forward-focus`) whose `key-pressed` handler, while `sentence-mode-active`, maps Right/Left/Space to new `next-cue`/`previous-cue`/`repeat-cue` callbacks
- `app-window.slint`: `HintBar` component — "← previous sentence", "space · repeat sentence", "→ next sentence" — instantiated only in `SentenceBySentence` mode via an `if` guard
- `crates/app/src/main.rs`: `wire_cue_navigation`/`cue_navigation_handler` connect `next-cue`/`previous-cue`/`repeat-cue` to `PlayerState::next_cue`/`previous_cue`/`repeat_current_cue`, refresh the sentence card, and hand any produced `SeekCommand` to `VideoPlayer::apply_seek_command`
- `video_player.rs`: `VideoPlayer::apply_seek_command(SeekCommand)` seeks mpv to the command's start and resumes playback; `apply_pending_pause`, run on the existing scrub bar poll timer, pauses mpv once `time-pos` reaches an armed `pause_at` (so Right/Space play through a cue's span and stop at its end, per README)

### Changed
- `video_player::VideoPlayer::attach` now returns a `VideoPlayer` usable from `main.rs` (wrapped in `Rc`) to drive mpv from the cue navigation callbacks, not just to keep its rendering notifier/timer alive

## [0.1.14] - 2026-07-15

### Added
- `playback_state::PlayerState::sync_cue_to_time(time: Duration)`: sets `current_cue_index` to the cue whose start is the latest one at or before `time` (the sentence currently playing, or the most recently started one across a gap between cues), `None` before the first cue's start or with no cues loaded
- `crates/app/src/sentence_card.rs`: `update_sentence_card(&AppWindow, &PlayerState)` mirrors the current cue into the window's "Sentence N / M" label and original-language text (placeholder text when none is in focus), split from a pure `sentence_card_display` helper so the mapping is unit-tested without a Slint window
- `app-window.slint`: `CurrentSentenceCard` component (rounded card, uppercase mono sentence label, 24px/600 original text, divider) in a new sentence-panel column next to the video, per `sketch/design_reference.dc.html#1c`
- `trango video.mp4 subs.srt` CLI usage: a second argument (`subtitle_path_from_args`) is read, parsed with `subtitle::parse_srt`, loaded into `PlayerState` via `set_cues`, and mirrored into the current-sentence card on startup — a bad/missing path is logged and otherwise ignored rather than stopping video playback
- `video_player.rs`: the scrub bar's polling timer also calls the new `sync_current_sentence`, which — only in `SentenceBySentence` mode — syncs `current_cue_index` to mpv's `time-pos` and refreshes the current-sentence card

### Changed
- `trango` depends on `subtitle` (previously only a dev-dependency, used by the E2E test) so `main.rs` can parse subtitle files at runtime
- `video_player::VideoPlayer::attach` takes an additional `Rc<RefCell<PlayerState>>` parameter, shared with the rest of the app, so its polling timer can read and update playback state
- `app-window.slint`: the body row is now a `HorizontalLayout` (video column + 16px margins/gaps + fixed-width sentence panel column) instead of a single video column filling the whole width

## [0.1.13] - 2026-07-15

### Added
- `crates/app/tests/e2e_sentence_navigation.rs`: first E2E test — parses the real `test-media/sample/sample.srt` fixture and drives `playback-state` cue navigation (`next_cue`/`previous_cue`/`repeat_current_cue`) forward and back across all five real cues, plus a sanity check that the paired `sample.mp4` fixture exists on disk and is non-empty
- `docs/src/architecture/testing.md`: documents what the E2E suite covers (subtitle parsing + cue navigation against real fixtures) and what it deliberately doesn't (libmpv rendering/decoding — see `docs/src/architecture/video-playback.md` — and pixel-level UI screenshot testing)

### Changed
- `trango` (`crates/app`) gains a dev-dependency on `subtitle`, used only by the new E2E test

## [0.1.12] - 2026-07-15

### Fixed
- Scrub bar thumb visibly stepped/jumped forward instead of gliding, especially on short clips — `SCRUB_BAR_POLL_INTERVAL` dropped from 200ms to 33ms (mpv's `get_property` is an in-process read, cheap enough to poll near display refresh rate)

## [0.1.11] - 2026-07-15

### Added
- `playback_state::format_time(seconds: f64) -> String`: formats a playback time as `MM:SS` (or `H:MM:SS` past one hour) for the scrub bar's time labels, clamping negative/non-finite input to `00:00`
- `crates/app/ui/app-window.slint`: `ScrubBar` component below the video frame — mono muted time labels either side of a 4px rounded track with an accent-filled progress fill and a white circular thumb, per `sketch/design_reference.dc.html#1c`
- `video_player.rs`: `VideoPlayer::attach` starts a repeating `slint::Timer` (`SCRUB_BAR_POLL_INTERVAL`, 200ms) that polls mpv's `time-pos`/`duration` properties and mirrors them into the new `current-time-label`/`duration-label`/`scrub-progress` `AppWindow` properties
- `docs/src/architecture/video-playback.md`: new "Scrub bar: polling mpv's playback-time properties" section

## [0.1.10] - 2026-07-15

### Added
- `trango` depends on `libmpv2` (`render` feature) for libmpv OpenGL render-API embedding — asked and approved per `CLAUDE.md` before adding, chosen over the unmaintained original `libmpv` crate
- `crates/app/src/video_player.rs` (+ `gl_proc_address_bridge` submodule): embeds libmpv video playback into the Slint window as an OpenGL underlay via `Window::set_rendering_notifier`, with no separate mpv window
- `trango <path/to/video>` CLI argument (`video_path_from_args`) starts playing that video on launch; without one, the video area just shows the placeholder background
- `docs/src/architecture/video-playback.md` and `docs/src/technology/libmpv2.md`

### Changed
- `app-window.slint`: root `Window` no longer has an opaque `background` (needed so the mpv underlay can show through); the video area's background is now `Palette.window-bg` only while no video is loaded (`video-loaded` property, `in`, defaults `false`), and fully transparent once one is

## [0.1.9] - 2026-07-15

### Added
- `trango` depends on `playback-state`; `crates/app/src/main.rs` owns a `PlayerState` (behind `Rc<RefCell<_>>`) and wires it to a new `toggle-mode` Slint callback

### Changed
- Top bar segmented control now drives `PlayerState::toggle_mode()` for real instead of only flipping a local Slint property: each `SegmentButton`'s `clicked` handler invokes `toggle-mode()` (guarded so clicking the already-active segment is a no-op), Rust toggles the mode and logs it with `tracing::debug!`, then mirrors the result back into `sentence-mode-active` — `sentence-mode-active` changed from `in-out` to `in` since only Rust writes it now

## [0.1.8] - 2026-07-15

### Added
- `trango` top bar: accent dot + "TrangoPlayer" wordmark, Normal / Sentence by sentence segmented control (static — toggles a local `sentence-mode-active` Slint property only, not yet wired to `playback-state`), "Open video…" / "Open subtitles…" ghost buttons — pixel reference `sketch/design_reference.dc.html#1c`

### Changed
- App version moved from a top bar label to the window title (`"TrangoPlayer v{version}"`) to make room for the full top bar layout

### Fixed
- Segmented control pills and ghost buttons were stretched to the full 52px top bar height — `HorizontalLayout`'s `cross-axis-alignment` defaults to `stretch`; set it to `center` on the top bar's row so children size to their own preferred height (padding + text) and sit vertically centered, matching `sketch/design_reference.dc.html#1c`

## [0.1.7] - 2026-07-15

### Added
- `trango` (`crates/app`): `slint` dependency + `crates/app/ui/app-window.slint` — the main window shell, background `#1c1d22`, 52px top bar (`#202127`) showing the "TrangoPlayer" wordmark and the current `Cargo.toml` version
- `docs/src/technology/slint.md`

## [0.1.6] - 2026-07-15

### Added
- `playback-state` crate: `SeekCommand { start, end, then_pause }` describing what the player should do without driving mpv itself
- `playback-state` crate: `PlayerState::next_cue()`, `previous_cue()`, `repeat_current_cue()` implementing the README's Right/Left/Space navigation rules — `next_cue`/`previous_cue` return `None` and leave the cursor in place at the last/first cue or on an empty cue list; `repeat_current_cue` never moves the cursor and returns the identical command however many times it is called

## [0.1.5] - 2026-07-15

### Added
- `playback-state` crate: `PlaybackMode` (`Normal` | `SentenceBySentence`, defaults to `Normal`)
- `playback-state` crate: `PlayerState { mode, cues, current_cue_index, show_translation }` with `toggle_mode()`, `set_cues(...)` (resets the cursor to the first cue, or `None` if empty), and `toggle_translation()`

## [0.1.4] - 2026-07-15

### Added
- `subtitle` crate: `merge_translation(original: Vec<Cue>, translation: Vec<Cue>) -> Vec<Cue>` attaching a translation track's text onto an original track's cues by timing overlap (not index), so mismatched cue counts between hand-timed and STT-generated tracks don't silently drift out of sync

### Changed
- `Cue` gains a `translation: Option<String>` field (`None` until `merge_translation` fills it in)

## [0.1.3] - 2026-07-15

### Added
- `subtitle` crate: `parse_srt(&str) -> Result<Vec<Cue>, SubtitleError>` parsing `.srt` files into cues, handling a leading UTF-8 BOM and both `\n`/`\r\n` line endings
- `crates/subtitle/tests/fixtures/*.srt` fixtures (valid, BOM, missing newline, invalid timestamp) with an integration test reading them from disk

## [0.1.2] - 2026-07-15

### Added
- `subtitle` crate: `Cue` data model (`index`, `start`, `end`, `text`) with `Cue::new` validating `start < end`
- `subtitle` crate: `SubtitleError` (`thiserror`) with `InvalidFormat`, `IoError`, `InvalidTiming` variants

## [0.1.1] - 2026-07-15

### Added
- `docs/` mdbook scaffold (`usage/`, `architecture/`, `specs/`, `technology/`)
- `docs/src/architecture/crates.md` describing the Vaihe 1 crate split
- `docs/src/technology/tracing.md`

## [0.1.0] - 2026-07-15

### Added
- Cargo workspace with `crates/subtitle`, `crates/playback-state` (empty libs) and `crates/app` (binary `trango`)
- `tracing`-based logging initialized in `trango`'s `main.rs`; version printed on startup
