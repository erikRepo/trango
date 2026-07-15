# Crate structure

trango is a Cargo workspace (`[workspace]` in the root `Cargo.toml`) with
three members:

## `crates/subtitle` (library, package `subtitle`)

Holds the `Cue` data model (`index`, `start`, `end`, `text`,
`translation`) and `SubtitleError` (see
`docs/src/technology/thiserror.md`). `Cue::new` validates that `start <
end` and leaves `translation` as `None`. `parse_srt(&str) ->
Result<Vec<Cue>, SubtitleError>` parses `.srt` file contents into cues:
it strips a leading UTF-8 BOM, normalizes `\n`/`\r\n` line endings, and
returns `SubtitleError::InvalidFormat` for malformed blocks (bad index,
missing timing line, unparseable timestamp). Tested against fixture
files in `crates/subtitle/tests/fixtures/`.

`merge_translation(original: Vec<Cue>, translation: Vec<Cue>) ->
Vec<Cue>` attaches a second (translation) subtitle track's text onto
`original`'s cues. Matching is done by timing overlap, not by index: for
each original cue, the translation cue whose `[start, end)` range
overlaps it the most supplies the text, and a cue with no overlapping
translation cue keeps `translation: None`. Overlap-based matching was
chosen over index-based matching because the two tracks may not have the
same number of cues — e.g. a hand-timed original paired with an
STT-generated translation — so pairing by position would silently drift
out of sync.

No dependency on Slint or libmpv, so it can be tested with fast, isolated
unit tests, later against real `.srt` fixtures.

## `crates/playback-state` (library, package `playback-state`)

Depends on `subtitle` for the `Cue` type. Holds `PlaybackMode` (`Normal` |
`SentenceBySentence`, defaulting to `SentenceBySentence` — the primary
language-learning use case) and `PlayerState { mode,
cues: Vec<Cue>, current_cue_index: Option<usize>, show_translation: bool
}`.

`PlayerState::toggle_mode()` flips between `Normal` and
`SentenceBySentence`. `set_cues(cues)` replaces the loaded cues and resets
`current_cue_index` to `Some(0)`, or `None` if `cues` is empty.
`toggle_translation()` flips `show_translation`.

Cue navigation implements the README's Right/Left/Space rules as pure
logic returning a `SeekCommand { start, end, then_pause }` — "what the
player should do" — instead of driving mpv directly:

- `next_cue()` / `previous_cue()` move `current_cue_index` and return the
  command to play the newly-focused cue's span. At the last/first cue (or
  on an empty cue list) they return `None` and leave the cursor where it
  is — there's nothing further to navigate to.
- `repeat_current_cue()` never moves the cursor; calling it any number of
  times for the same cue returns the identical command, matching the
  README's requirement that Space always replays the same span.
- `jump_to_cue(index: usize)` moves the cursor directly to `index` and
  returns the same command shape, reusing the same private `seek_command_for`
  helper as the other three — `None`, cursor untouched, if `index` is out of
  range. This is what backs the sentence list's row clicks (see below),
  which the README requires to behave exactly like arrow navigation.

`format_time(seconds: f64) -> String` formats a playback time as `MM:SS`,
or `H:MM:SS` once it reaches an hour; used for the scrub bar's time labels
(see `docs/src/architecture/video-playback.md`). It clamps negative or
non-finite input (e.g. mpv's `time-pos`/`duration` before a video has
started reporting them) to `00:00` instead of panicking or underflowing.

`sync_cue_to_time(time: Duration)` sets `current_cue_index` to the cue whose
`start` is the latest one at or before `time` — the sentence currently
playing, or the most recently started one if `time` falls in a gap between
cues — and `None` if `time` is before the first cue's start or no cues are
loaded. This is what drives the current-sentence card from mpv's `time-pos`
while in `SentenceBySentence` mode (see
`docs/src/architecture/video-playback.md`).

No I/O and no UI yet, so this state machine (and `format_time`) is TDD'd
without a Slint window or a video file.

## `crates/app` (binary, package `trango`)

The binary that ties the Slint UI, libmpv, and the two library crates
together. The package name is `trango` (`[package] name = "trango"`), so
the compiled binary is `trango`; the directory is named `crates/app` to
describe its role. The product name shown in the UI is **TrangoPlayer**.

`crates/app/src/main.rs` initializes `tracing` logging, prints the crate
version, and opens the Slint main window defined in
`crates/app/ui/app-window.slint` (see `docs/src/technology/slint.md`) —
window background and a full top bar (wordmark, segmented control, ghost
buttons). If a video path is given as a CLI argument
(`trango path/to/video.mp4`), `video_player::VideoPlayer::attach` embeds
libmpv playback into the window (see
`docs/src/architecture/video-playback.md` and
`docs/src/technology/libmpv2.md`) and starts a repeating timer that polls
mpv's `time-pos`/`duration` properties to drive the scrub bar below the
video frame; without a video path, the video area just shows the window
background as a placeholder and the scrub bar stays at `00:00`. Picking a
file from an in-app dialog is a later `TODO.md` step.

If a second CLI argument is given (`trango video.mp4 subs.srt`),
`load_subtitles` reads and parses it with `subtitle::parse_srt`, loads the
resulting cues into `PlayerState` via `set_cues`, and mirrors the first cue
into the current-sentence card (`crates/app/src/sentence_card.rs`,
`update_sentence_card`) and the sentence list (`crates/app/src/sentence_list.rs`,
`update_sentence_list`) — see
`docs/src/architecture/video-playback.md` for how both keep updating
from mpv's `time-pos` afterward. A file that can't be read or doesn't parse
is logged and otherwise ignored — a bad subtitle path shouldn't stop the
video from playing.

Depends on `playback-state` for `PlayerState`. `wire_player_state(&AppWindow)`
creates a `PlayerState` (behind `Rc<RefCell<_>>` — Slint callbacks run on the
UI thread, so no `Send`/`Sync` is needed) and registers a handler for the
window's `toggle-mode` callback: it calls `PlayerState::toggle_mode()`, logs
the new mode with `tracing::debug!`, and mirrors it into the `sentence-mode-
active` Slint property so the segmented control's active pill stays in sync.
The top bar's `SegmentButton`s invoke `toggle-mode()` from their `clicked`
handler (guarded so clicking the already-active segment is a no-op) instead
of assigning `sentence-mode-active` directly, so the click always goes
through the real state machine.

## Why three crates instead of one

Splitting `subtitle` and `playback-state` out of the binary means most of
the business logic (subtitle parsing, cue navigation) is testable without
pulling in the heavier Slint/libmpv dependencies, and keeps individual
files small (see `CLAUDE.md`: aim for ~200 lines per file).

## Shared workspace metadata

All three crates inherit `version`, `edition`, and `rust-version` from
`[workspace.package]` in the root `Cargo.toml` (`version.workspace = true`,
etc.), so the version only needs to be bumped in one place.
