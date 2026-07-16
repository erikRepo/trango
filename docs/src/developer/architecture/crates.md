# Crate structure

trango is a Cargo workspace (`[workspace]` in the root `Cargo.toml`) with
four members:

## `crates/subtitle` (library, package `subtitle`)

Holds the `Cue` data model (`index`, `start`, `end`, `text`,
`translation`) and `SubtitleError` (see
`docs/src/developer/technology/thiserror.md`). `Cue::new` validates that `start <
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
(see `docs/src/developer/architecture/video-playback.md`). It clamps negative or
non-finite input (e.g. mpv's `time-pos`/`duration` before a video has
started reporting them) to `00:00` instead of panicking or underflowing.

`sync_cue_to_time(time: Duration)` sets `current_cue_index` to the cue whose
`start` is the latest one at or before `time` — the sentence currently
playing, or the most recently started one if `time` falls in a gap between
cues — and `None` if `time` is before the first cue's start or no cues are
loaded. This is what drives the current-sentence card from mpv's `time-pos`
while in `SentenceBySentence` mode (see
`docs/src/developer/architecture/video-playback.md`).

No I/O and no UI yet, so this state machine (and `format_time`) is TDD'd
without a Slint window or a video file.

## `crates/word-analysis` (library, package `word-analysis`)

`TODO.md` Vaihe 24's word-by-word sentence analysis, split out the same
way `subtitle` and `playback-state` are: the HTTP/JSON/file-I/O logic is
plain Rust with no Slint or libmpv dependency, so it's unit-testable
(including against a hand-rolled local mock HTTP server, see
`crates/word-analysis/src/ollama.rs`'s tests) without a UI or a real
Ollama installation. `crates/app` wires it to the Ctrl+A popup and the
Open Subtitles dialog's "Analyze all sentences" batch loop.

`WordEntry { word, translation, pronunciation }` and `WordAnalysis {
words: Vec<WordEntry> }` (`entry.rs`) are the data model for one
sentence's analysis.

`cache.rs` persists analyses to a JSON sidecar file next to the subtitle
they belong to, so re-opening the same video/subtitle reuses
already-computed translations instead of re-calling Ollama:
`cache_path_for(subtitle_path)` swaps the subtitle's extension for
`.wordanalysis.json` (e.g. `subs.srt` -> `subs.wordanalysis.json`);
`AnalysisCache { model, entries: HashMap<u32, WordAnalysis> }` is keyed by
`Cue::index`. `load_cache`/`save_cache` follow the same robustness
convention as `crates/app/src/config.rs`: a missing or corrupt cache file
becomes an empty `AnalysisCache::default()` (logged via `tracing::warn!`),
not an error — a lost cache means re-analyzing, not a failure to start.

`ollama.rs` talks to a local Ollama instance (see
`docs/src/developer/technology/ureq.md`). The `OllamaClient` trait (`list_models`,
`analyze_sentence`) lets callers swap in a fake instead of a real server
in tests, mirroring `subtitle::SubtitleGenerator`'s role for whisper-cli.
`HttpOllamaClient` implements it over HTTP, defaulting to
`http://localhost:11434`: `list_models` reads `GET /api/tags`;
`analyze_sentence` posts to `/api/generate` with `stream: false` and
`format: "json"` so the whole answer comes back as one JSON object rather
than a streamed sequence, then parses its `response` field (itself JSON
text, per `build_prompt`'s instructions to the model) into a
`WordAnalysis` — defensively stripping a ` ```json ` code fence first,
since some local models still wrap their answer in one despite `format:
"json"`. `build_prompt(sentence, target_language)` and the response
parser are both plain functions with no I/O, tested directly with canned
strings; `HttpOllamaClient` itself is tested against a small mock HTTP
server started on a random local port (`std::net::TcpListener` in its own
thread), so the test suite doesn't depend on Ollama being installed.

## `crates/app` (binary, package `trango`)

The binary that ties the Slint UI, libmpv, and the two library crates
together. The package name is `trango` (`[package] name = "trango"`), so
the compiled binary is `trango`; the directory is named `crates/app` to
describe its role. The product name shown in the UI is **TrangoPlayer**.

`crates/app/src/main.rs` initializes `tracing` logging, prints the crate
version, and opens the Slint main window defined in
`crates/app/ui/app-window.slint` (see `docs/src/developer/technology/slint.md`) —
window background and a full top bar (wordmark, segmented control, ghost
buttons). `video_player::VideoPlayer::attach` always runs once, right
after the window is created, embedding libmpv playback into it (see
`docs/src/developer/architecture/video-playback.md` and
`docs/src/developer/technology/libmpv2.md`) and starting a repeating timer that
polls mpv's `time-pos`/`duration` properties to drive the scrub bar below
the video frame — *even without a video path yet*, for reasons the linked
architecture page explains in detail (a Slint API subtlety around when its
rendering-setup notification fires). If a video path is given as a CLI
argument (`trango path/to/video.mp4`), `attach` also starts loading it
immediately; without one, the video area just shows the window background
as a placeholder and the scrub bar stays at `00:00` until a video is
picked another way.

A video can also be picked in-app via the top bar's "Open video…" button
(`TODO.md` Vaihe 18): `open_video_dialog::list_folder_entries` lists a
folder's subfolders and video files (by extension, with a
`std::fs::metadata`-based size label for videos — duration is deferred,
since it would need decoding the file) as `FolderEntry` rows, and
`open_video_dialog::matching_subtitle_path` looks for a same-stem `.srt`
next to the chosen video. Clicking an `Up`/`Folder` row navigates the
dialog to that folder in place (see `docs/src/developer/specs.md`'s "Open
Video dialog: folder navigation") instead of selecting it — only `Video`
rows are selectable. `wire_open_video_dialog` in `main.rs` wires the
button, row navigation/selection, and the "Open" button's
`open_selected_video`, which loads any auto-matched subtitle first (or
clears stale cues if none match), then calls the already-attached
`VideoPlayer`'s `load_video` — the session's first video load if trango
started with no CLI argument, or a later one if switching files
mid-session; either way the same `VideoPlayer` from startup — see
`docs/src/developer/architecture/video-playback.md`.

If a second CLI argument is given (`trango video.mp4 subs.srt`),
`load_subtitles` reads and parses it (via the shared `parse_subtitle_file`
helper, which wraps `subtitle::parse_srt`), loads the resulting cues into
`PlayerState` via `set_cues`, and mirrors the first cue into the
current-sentence card (`crates/app/src/sentence_card.rs`,
`update_sentence_card`) and the sentence list (`crates/app/src/sentence_list.rs`,
`update_sentence_list`) — see
`docs/src/developer/architecture/video-playback.md` for how both keep updating
from mpv's `time-pos` afterward. A file that can't be read or doesn't parse
is logged and otherwise ignored — a bad subtitle path shouldn't stop the
video from playing.

If a third CLI argument is given (`trango video.mp4 subs.srt subs.en.srt`),
`load_subtitles` also parses it with `parse_subtitle_file` and merges it into
the original cues via `subtitle::merge_translation` before loading them into
`PlayerState`, populating each cue's `translation` field. A translation file
that can't be read or parsed is logged and skipped — the original cues still
load, just without translations. `update_sentence_card` mirrors the current
cue's translation into the window's `translation-text` property regardless
of the toggle state below; only its Slint-side visibility depends on
`show-translation`.

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

`wire_player_state` wires the window's `toggle-translation` callback the
same way: it calls `PlayerState::toggle_translation()` and mirrors the
resulting `show_translation` into the window's `show-translation` property,
which `CurrentSentenceCard`'s `ToggleSwitch` and translation `Text` (in
`app-window.slint`) read directly — purely visual, no effect on playback,
per the README's translation toggle spec.

## Why four crates instead of one

Splitting `subtitle`, `playback-state`, and `word-analysis` out of the
binary means most of the business logic (subtitle parsing, cue
navigation, Ollama's HTTP/JSON handling) is testable without pulling in
the heavier Slint/libmpv dependencies, and keeps individual files small
(see `CLAUDE.md`: aim for ~200 lines per file).

## Shared workspace metadata

All four crates inherit `version`, `edition`, and `rust-version` from
`[workspace.package]` in the root `Cargo.toml` (`version.workspace = true`,
etc.), so the version only needs to be bumped in one place.
