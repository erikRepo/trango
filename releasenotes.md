# Release Notes

Format based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versions correspond to completed development steps, not per-commit semver bumps.

## [Unreleased]

### Added
### Changed
### Fixed
### Removed

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
