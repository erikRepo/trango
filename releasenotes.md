# Release Notes

Format based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versions correspond to completed development steps, not per-commit semver bumps.

## [Unreleased]

### Added
### Changed
### Fixed
### Removed

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
