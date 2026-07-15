# Release Notes

Format based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versions correspond to completed development steps, not per-commit semver bumps.

## [Unreleased]

### Added
### Changed
### Fixed
### Removed

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
