# Release Notes

Format based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versions correspond to completed development steps, not per-commit semver bumps.

## [Unreleased]

### Added
### Changed
### Fixed
### Removed

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
