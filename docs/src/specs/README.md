# Specs

Not written yet. This section will hold functional specifications for app
behavior that go beyond what's already covered by the repository root
`README.md` handoff spec — for example, implementation decisions the
handoff spec leaves open (see e.g. `TODO.md` Vaihe 21, Normal mode's
sentence-panel behavior).

## Open Video dialog: folder navigation

The Open Video dialog (`TODO.md` Vaihe 18) opens on a default folder
(`main.rs`'s `default_video_folder`: the CLI video path's parent directory
if one was given, otherwise `config.toml`'s `video_folder` — the folder the
last successfully opened video lived in, kept up to date by
`open_selected_video` on every video open — otherwise the current working
directory), but isn't limited to it: an "‥ Up" row and clicking a listed
subfolder navigate the dialog in place, re-listing that folder's contents
(`open_video_dialog::list_folder_entries`). This was chosen over a
native OS folder picker to stay consistent with README's "no OS-native
file picker — mockin oma UI" direction for the dialog as a whole, and
needs no new dependency. `TODO.md`'s "Ei tässä listassa" section originally
deferred folder switching with a *native* picker specifically; this in-app
navigation isn't that, so it's covered here instead.

## Open Subtitles dialog: no OS drag-and-drop for the translation link

README specs the translation section's `.srt` linking as an OS-level
drag-and-drop target ("drop a translated .srt file here"). That isn't
implemented as literal drag-and-drop: Slint 1.17.1's winit backend doesn't
relay external file drops to `DropArea` at all (only in-app `DragArea`
sources, of which this dialog has none) — see
`docs/src/technology/slint.md`'s "Pitfalls" section for how that was
confirmed. `TODO.md` Vaihe 19 instead links a translation subtitle through
a small in-app file picker (`open_subtitles_dialog::list_srt_files` +
`crates/app/main.rs`'s `wire_open_subtitles_dialog`'s
`link-translation-requested` handler), reusing the Open Video dialog's
file-list chrome — generalized into `app-window.slint`'s `FileListDialog`
component for that purpose — scoped to the video's own folder's `.srt`
files (no subfolder navigation, unlike the Open Video dialog: a
translation file is expected right next to the video). Picking one there
re-merges cues immediately (not deferred to the Open Subtitles dialog's
"Done" button, which just closes the modal). If Slint gains real OS file
drop support later, this picker can stay as a fallback/alternate entry
point rather than being removed outright.

README's mock also labels the two subtitle sections "(DE)"/"(EN)" as
language-code examples for that specific demo video; since trango doesn't
track subtitle language, the dialog instead uses the generic labels
"Original subtitle" / "Translation".

## Subtitle generation: stub interface, no STT dependency yet

`TODO.md` Vaihe 20 asks for the `subtitleGenerationStatus`
(`Idle | Generating | Done | Error`) flow to be wired end-to-end before any
speech-to-text library is added — adding one (e.g. a local Whisper binding)
is a significant new dependency and needs a separate go-ahead. The
`subtitle` crate's `SubtitleGenerator` trait (`crates/subtitle/src/generate.rs`)
captures the shape a real backend will fill in later:
`fn generate(&self, video_path: &Path) -> Result<PathBuf, SubtitleError>`.
For now, `StubSubtitleGenerator` is the only implementation — it writes a
single fixed placeholder cue to a same-stem `.srt` next to the video (the
same naming convention `open_video_dialog::matching_subtitle_path` looks
for), so a "generated" file is picked up as the video's linked original
subtitle immediately, without a separate refresh step.

At this stage, `crates/app/src/subtitle_generation.rs`'s `generate` ran the
generator synchronously on the UI thread and mirrored the result into
`AppWindow::subtitle-generation-status` plus (on success) the Open
Subtitles dialog's original row. That was fine for a stub that returns
instantly; see "Subtitle generation: whisper-cli as an external process"
below for the real backend that replaced it and moved this off the UI
thread.

## Subtitle generation: whisper-cli as an external process

`TODO.md` Vaihe 21.5 adds the real speech-to-text backend the previous
section deferred. Two implementation options were discussed: a Rust
binding to whisper.cpp (e.g. the `whisper-rs` crate) versus driving
whisper.cpp's own `whisper-cli` binary as an external process via
`std::process::Command`. The external-process route won, for two reasons:
it needs **no new Cargo dependency** (`whisper-cli` isn't linked into
trango at all — the user installs it separately, see
`docs/src/usage/`), and whisper.cpp already ships an `-osrt` flag that
writes a ready-made `.srt` file directly, so trango doesn't need to parse
raw transcript text or timestamps itself. The tradeoff is that generation
now depends on an external tool being installed and discoverable, which
`WhisperCliGenerator`'s `Error` status message (see below) is written to
make legible rather than a generic failure.

`subtitle::WhisperCliGenerator` (`crates/subtitle/src/generate.rs`) runs
`whisper-cli -f <video> [-m <model>] -of <video-stem> -osrt`. The `-of`
flag matters: it must be given the output path *without* an extension,
because `whisper-cli` appends `.srt` itself when `-osrt` is set. Passing
`video_path` with its extension stripped there makes the final output land
at `video_path.with_extension("srt")` — the exact same same-stem
convention `StubSubtitleGenerator` and
`open_video_dialog::matching_subtitle_path` already use — without trango
needing to rename or move whisper-cli's output afterward.

Since `whisper-cli` isn't installed on the machine this was implemented
on (noted in `TODO.md` Vaihe 21.5 itself), `WhisperCliGenerator`'s
automated tests (`crates/subtitle/src/generate.rs`) don't exercise the
real binary. Instead they write small POSIX shell scripts that mimic its
`-of`/`-osrt` contract (writing `<-of value>.srt`, or exiting non-zero
with a stderr message) and point `binary_path` at those — real `Command`
plumbing (argument passing, exit status, stdout/stderr capture) is still
exercised end-to-end, just against a stand-in binary rather than a real
transcription. These tests are `#[cfg(unix)]`-only (the fake scripts need
a POSIX shell and `chmod +x`); the "binary not found" and "video file
missing" cases are platform-independent and run everywhere.

### Audio extraction via ffmpeg — whisper-cli can't read most video containers

Not caught by the fake-binary tests above (they never exercise real
audio decoding) and only found once a real `whisper-cli` and a real
video were tried together: `whisper-cli` only reads a handful of raw
audio formats (its own `--help` says `flac, mp3, ogg, wav`) — not
`.mp4`/`.mkv`/other video containers at all. Worse, when given an
unsupported file it prints an error to stderr but still **exits 0**, so
the original implementation's `output.status.success()` check reported
success while `-osrt` never wrote anything — surfacing only as trango's
generic "no subtitle file was found" `Error` message, with no indication
of the real cause.

The fix: `WhisperCliGenerator::generate` now always extracts the video's
audio to a temporary 16kHz mono PCM WAV file with `ffmpeg` first
(`extract_audio`, matching whisper.cpp's own examples' recommended
format), then runs `whisper-cli` against that WAV file instead of the
original video (`run_whisper_cli`, taking an `audio_path` parameter
distinct from `video_path`). This is a second external process, `ffmpeg`
— also not a Cargo dependency, same reasoning as `whisper-cli` itself,
and about as close to universally preinstalled as an external tool gets.
The temp WAV lives in `std::env::temp_dir()` under a process-and-call
-unique name (`temp_audio_path`, a monotonic counter rather than
wall-clock time, so back-to-back calls never collide) and is deleted
after `generate` returns, success or failure.

Splitting `generate` into `extract_audio` and `run_whisper_cli` (both
private methods) also solved a test-design problem: the temp audio
path's exact name can't be predicted from outside (by design, so
concurrent generations never collide), so a test driving the public
`generate` entry point can't assert on it directly. Most tests instead
call `run_whisper_cli` directly with an arbitrary, test-chosen "audio"
fixture path (standing in for whatever `extract_audio` would have
produced) to check whisper-cli's own argument handling in isolation, and
`extract_audio` gets its own tests the same way with a fake `ffmpeg`
script. One further test (`test_generate_extracts_audio_before_running_whisper_cli`)
proves the two are actually wired together correctly *without* needing
to predict the temp path: the fake `ffmpeg` writes a fixed marker string
as its "audio" output, and the fake `whisper-cli` refuses to proceed
unless the file it receives via `-f` contains exactly that marker.

One more thing found while testing this for real: writing a fresh fake
binary and executing it milliseconds later occasionally raced with
ETXTBSY ("text file busy") in the sandboxed environment these tests were
developed in — the write's file handle not always visibly closed to a
following `exec` yet. `run_command` (wrapping every `Command::output()`
call in both `extract_audio` and `run_whisper_cli`) retries briefly on
that specific error rather than the test suite intermittently failing;
it's a generically reasonable thing for any external-process call to do
regardless of environment, not just a test-only workaround.

### Background thread, not the UI thread

Real transcription can take seconds to minutes, unlike the stub, so
running it synchronously (the earlier stub-era `generate` function) would
freeze the whole app. `crates/app/src/subtitle_generation.rs::spawn_generate`
runs the generator on a `std::thread::spawn`-ed background thread instead,
reporting its result to a caller-supplied `on_done` callback.

The tricky part: `on_done` runs on the background thread, but updating
`AppWindow` properties and the app's `Rc<RefCell<PlayerState>>`/
`Rc<RefCell<CurrentMedia>>` state must happen on the UI thread, and
`Rc`/`RefCell` aren't `Send` — they can't cross the thread boundary at
all, even transiently through a closure capture. `main.rs`'s
`wire_open_subtitles_dialog` handles this in two hops:

1. `on_done` (background thread) calls `slint::invoke_from_event_loop`
   with a closure that captures only `Send` data — a `Weak<AppWindow>`
   and the owned `Result<PathBuf, SubtitleError>` — mirroring
   `video_player.rs`'s `load_file` pattern for the same reason. That
   closure runs `subtitle_generation::apply_result`, which needs only
   `&AppWindow` to update `subtitle-generation-status`/`-error-message`
   and (on success) the dialog's original row.
2. On success, that same closure invokes a second, UI-thread-only signal:
   `AppWindow::subtitle-generated(string)`. This callback isn't tied to
   any UI element — it exists purely so a *separate* handler, set up once
   in `wire_open_subtitles_dialog` and holding the `Rc`-based state
   directly (no thread crossing involved, since both the `invoke_*` call
   and the handler run on the UI thread), can load the generated subtitle
   into the player and record it in `CurrentMedia`.

This keeps the background thread's payload to genuinely `Send`-safe data
without reaching for `unsafe impl Send` wrappers or switching
`PlayerState`/`CurrentMedia` to `Arc<Mutex<...>>` throughout the app just
for this one feature.

Because `slint::invoke_from_event_loop` only *queues* its closure — it
runs the next time the event loop actually polls — this whole path can't
be driven end-to-end in an automated test the way most of the app's
`AppWindow`-touching code is: `crates/app/src/main.rs`'s tests construct
a real `AppWindow` but never call `AppWindow::run()` (see that test
module's own comment on why only one such window can exist per test
process), so a queued event loop closure never actually executes there.
The test suite instead covers each layer separately: `spawn_generate`'s
thread-spawn-and-callback plumbing is tested directly with a plain
`mpsc` channel (no `AppWindow` involved,
`crates/app/src/subtitle_generation.rs`), `apply_result`'s window-mirroring
is tested against a real `AppWindow` by calling it directly with an
already-resolved `Result`, and the real button wiring
(`window.invoke_generate_subtitles_requested()`) is asserted to return
immediately with status `Generating` — proving it doesn't block the UI
thread — without asserting the later `Done`/`Error` transition, which is
instead covered by manual testing (`TODO.md` Vaihe 21.5's "Voit
ajaa/testata").

## Model selection: UI + autodiscovery instead of an environment variable, persisted to a small TOML config

`TODO.md` Vaihe 21.5 initially configured `WhisperCliGenerator`'s model
through an environment variable (`TRANGO_WHISPER_MODEL_PATH`), mirroring
the binary path's own `TRANGO_WHISPER_CLI_PATH`. Vaihe 21.6 replaces that
for the model specifically (the binary path env var is unchanged) — a
learner is expected to switch models somewhat often (e.g. one per target
language being studied), and re-exporting an environment variable and
restarting the app for that is more friction than the UI can avoid.

The Open Subtitles dialog gained a model row next to "Generate
subtitles" (disabled until a model is picked). Clicking it opens a
`FileListDialog` — the same in-app folder-browsing chrome already used
for the Open Video dialog and the translation-link picker, scoped to
`.bin`/`.gguf` files this time (`crates/app/src/model_picker.rs`,
mirroring `open_video_dialog.rs`'s `FolderEntry`/`list_folder_entries`
shape closely). Three things this module handles that the Open Video
dialog's equivalent doesn't need to:

- **Autodiscovery of a starting folder.** Rather than a folder derived
  from a currently-open file (as the Open Video dialog does) or a plain
  "always start empty", `default_start_folder` tries, in order: the
  config's last-browsed folder (see below) if it still exists; the first
  of a short list of folders whisper.cpp models commonly end up in
  (`candidate_model_folders` — a cloned+built whisper.cpp repo's own
  `models/`, a couple of XDG-ish cache/data locations, and `./models`,
  matching whisper-cli's own default model lookup path) that both exists
  *and* actually contains model files; the first of those that merely
  exists; finally the current working directory. This is deliberately not
  exhaustive OS-specific magic (no registry lookups, no `dirs`/`directories`
  crate) — just a handful of well-known conventions plus always-available
  manual navigation as the fallback, in keeping with README's "no
  OS-native file picker" direction for every other in-app dialog.
- **Persisting the pick.** `crates/app/src/config.rs` adds trango's first
  persistent settings file — `$XDG_CONFIG_HOME/trango/config.toml`
  (falling back to `$HOME/.config/trango/config.toml`), read at startup
  and written whenever a model is confirmed in the picker. This needed a
  new Cargo dependency (`serde` + `toml`) — asked and approved by the user
  before adding, per `CLAUDE.md`. A missing or corrupt config file loads
  as `TrangoConfig::default()` rather than failing startup — losing a
  remembered path is much less disruptive than trango refusing to open.
- **Language inference.** whisper-cli's own `--language` default is
  always `"en"`, regardless of which model is loaded — passing nothing
  would silently mistranscribe non-English audio even with a multilingual
  model loaded. `model_picker::language_flag` inspects the model's
  filename for whisper.cpp's own `.en` naming convention (e.g.
  `ggml-base.en.bin`) and passes `-l en` for those, `-l auto` (explicit
  language auto-detection) for everything else, so the right thing happens
  without asking the user to also pick a language separately. This is
  filename-convention-based, not a guarantee — a model renamed against
  convention would be inferred incorrectly, but whisper.cpp's own
  distribution and download tooling follows this convention consistently.

One more consequence worth documenting: whisper.cpp's smaller models
(`tiny`/`base`/`small`) are trained on far less non-English data than
English, so transcription quality for lower-resource languages (Hebrew
was the concrete case that prompted this) degrades much more on small
models than it does for English. `docs/src/usage/` recommends `medium` or
`large-v3` for anything other than English as a result — this is
documentation/guidance only, trango doesn't enforce or check it.

## Generating subtitles for an already-open video reloads it

(Superseded as the sole fix for the underlying idle-core problem by
`keep-open=yes` on the mpv core itself — see
`docs/src/architecture/video-playback.md`'s "EOF leaves the core idle
unless `keep-open` is set". The reload described below is still done, but
now as a bonus (it also re-arms the sentence-by-sentence start-of-playback
seek onto the newly-generated subtitle's first cue) rather than the only
thing standing between EOF and a permanently broken player.)

Found through real end-to-end testing: generating subtitles for a video
that's already open and playing (not just freshly opened) can leave
cue-navigation (arrow keys / sentence list) permanently broken afterward
— every seek fails with mpv error `Raw(-12)`. The cause: real
transcription can take anywhere from seconds to minutes, and if the
video is short, it can easily finish playing and reach EOF *during* that
wait — mpv's core goes idle at EOF, and issuing a `seek` command to an
idle core fails outright (this exact failure mode, and the same error
code, is already documented on `video_player.rs`'s
`apply_pending_start_seek`, which exists specifically to avoid it for the
*initial* start-of-playback seek by polling until `time-pos` becomes
readable rather than seeking immediately after `loadfile`).

The `subtitle-generated` handler (`main.rs`'s `wire_open_subtitles_dialog`)
didn't have an equivalent recovery step, since it doesn't call
`load_video`/`loadfile` at all — it only updates `PlayerState`'s cues and
`CurrentMedia`. The fix: after successfully loading the newly generated
subtitle, it now also reloads the video via `video_player::VideoPlayer::load_video`
(the same call `open_selected_video` makes when a video is first opened),
which re-issues `loadfile` and re-arms the sentence-by-sentence
start-of-playback seek through the already-correct, already-tested
`apply_pending_start_seek` machinery — recovering a normal, seekable
mpv core regardless of whether it had drifted to EOF during generation.

This introduced a testability wrinkle: `wire_open_subtitles_dialog` now
needs *some* way to trigger a video reload, but a real
`video_player::VideoPlayer` can't be constructed in `main.rs`'s tests
(`VideoPlayer::attach` needs a real mpv render context, which only comes
alive once `window.run()` is actually driving the event loop, and this
test suite's single shared `AppWindow` never calls it — see that test's
own comment on why). So `wire_open_subtitles_dialog` takes a
`reload_video: impl Fn(&AppWindow, &Path, &PlayerState)` closure instead
of a `Rc<video_player::VideoPlayer>` directly: `main`'s real caller wraps
`VideoPlayer::load_video`, while the test passes a closure that just
records its arguments into a `Vec`, letting the test assert
`wire_open_subtitles_dialog`'s own wiring (that a reload is triggered,
with the correct video path) without needing a working mpv instance at
all.

## No mode autoplays — only Space starts/stops playback

Found through real usage, right after the fix above: pressing Space to
replay the current sentence kept landing on the *next* sentence instead,
even with a healthy, seekable mpv core. README originally left Right
Arrow's exact behavior on landing at a new cue as an implementer's call
("pause or continue per current play state... recommend: play through to
the end of that cue, then pause"); the initial implementation chose
"always autoplay through to the end, then pause" for every navigation
action (`next_cue`/`previous_cue`/`jump_to_cue`/`repeat_current_cue` all
produced the same "seek, resume, arm an end-of-span auto-pause" command).
That choice turned out to directly cause the bug: `sync_current_sentence`
(`video_player.rs`) tracks `current_cue_index` from mpv's live `time-pos`
so the sentence card/list follow whatever's actually playing, and real
speech-to-text output very commonly produces *contiguous* cues — cue N's
`end` exactly equals cue N+1's `start`, no gap — so the instant mpv
auto-paused at cue N's end, that same timestamp *also* matched cue N+1's
start, and the very next poll tick reclassified the cursor onto cue N+1.
Pressing Space to "repeat" then replayed the wrong sentence, because the
cursor had already silently moved.

The chosen fix goes beyond patching that one boundary case: **navigation
(`Right`/`Left`/sentence-list clicks) no longer starts playback at all** —
it only seeks to the target cue's start and leaves mpv paused there.
**Space is the only thing that starts or stops playback**, as a toggle:
pressed while paused, it plays the current cue's span and auto-pauses at
its end (unchanged from before, just no longer triggered by navigation
too); pressed again while that's still playing, it pauses immediately
rather than waiting out the rest of the sentence. This is a deliberate,
uniform "nothing plays until you ask it to" model, not just a targeted
bugfix — it also removes the awkwardness of a sentence starting to play
the instant you glance at the next line via the arrow keys.

This split needed a matching type-level split in `playback-state`:
`SeekCommand { start }` (no `end`/`then_pause` — used by `next_cue`/
`previous_cue`/`jump_to_cue`, which now only ever mean "land here,
paused") versus `PlaySpanCommand { start, end }` (used by
`repeat_current_cue`, which still needs both ends of the span). Deciding
whether a `PlaySpanCommand` should actually start playing, versus pausing
an already-playing one early, needs live mpv state `PlayerState` can't
see — that decision moved into `video_player.rs`'s `toggle_play_span`,
which is genuinely a toggle (checks mpv's own `pause` property) rather
than the pure, mpv-agnostic transform `PlayerState`'s navigation methods
are elsewhere.

The original `sync_current_sentence` boundary bug still needed its own
fix independent of this redesign — `toggle_play_span` still arms the same
kind of end-of-span auto-pause `repeat_current_cue`'s command, so the
identical contiguous-cue reclassification could still happen once Space
starts a span and it auto-pauses. The fix: `sync_current_sentence` now
only re-derives `current_cue_index` from live `time-pos` while
`VideoPlayerInner::pause_at` is actually armed (i.e. a span is genuinely
still playing toward its own scheduled pause) — once paused, for any
reason, it leaves the cursor exactly where the triggering navigation/Space
action already set it, rather than letting a boundary-matching pause
position silently reclassify it.

One gap in the first pass of this fix: `video_player.rs`'s own
`pause_and_arm_start_seek_if_sentence_mode` (the function that pauses mpv
right after `loadfile`) only ever paused in `SentenceBySentence` mode
*with at least one cue already loaded* — `let first_cue =
player_state.cues.first()?;` returned early (skipping the pause
entirely) for `Normal` mode or a video with no subtitle linked yet. So a
video opened without a subtitle file (a common case — that's exactly
when the Open Subtitles dialog's "Generate subtitles" empty state shows)
started playing immediately on its own, contradicting "no mode
autoplays" for that specific case. Renamed to `pause_and_arm_start_seek`
and restructured so the pause always happens unconditionally; only the
*first-cue seek-arming* part stays conditional on `SentenceBySentence`
mode with cues present (Normal mode, or no cues, still pauses — just at
`0:00`, since there's no particular cue start to land on instead).

## `sync_current_sentence` removed entirely — not patched a third time

The `pause_at`-gated fix described above (keeping `sync_current_sentence`
but skipping it once `pause_at` cleared) turned out to still be wrong:
tested against `test-media/sample2/`'s real whisper-cli-generated,
contiguous-cue subtitle, Space still replayed the *next* sentence
instead of the current one. The gating missed the actual failure moment
— `sync_current_sentence` and `apply_pending_pause` run in the same
`SCRUB_BAR_POLL_INTERVAL` tick, `sync_current_sentence` first. On the
very tick `time-pos` first reaches (or, more likely, slightly overshoots)
the playing cue's `end`, `pause_at` is *still* `Some` — `apply_pending_pause`
hasn't cleared it yet, it runs right after in the same tick — so
`sync_current_sentence` still ran, saw a `time-pos` already at/past the
next cue's `start` (contiguous cues again), and reclassified the cursor
before the pause even landed. Gating on "has `pause_at` been cleared"
can't fix a bug that happens *before* clearing, only after.

Stepping back, the actual question was: does `sync_current_sentence`
serve any purpose left to fix, or should it go? Under the post-redesign
model (this file's "No mode autoplays" section above), every play action
is a bounded single-cue span kicked off by `toggle_play_span`, which
already knows exactly which cue it's playing — the cursor doesn't need
to be *rediscovered* from `time-pos` at any point during that span, it's
already correct from the moment the action fired. The only thing that
would ever need live `time-pos`-based cue discovery is a free-running,
un-bounded playback the app doesn't know the cue for in advance —
`Normal` mode's continuous playback (not yet wired to any UI at all,
`TODO.md` Vaihe 21) or a scrub-bar drag-to-seek feature (doesn't exist —
the scrub bar is currently display-only). Neither exists today, so there
was nothing left for `sync_current_sentence` to correctly serve, only a
recurring source of the same class of bug.

It was removed outright — the function, its poll-loop call, and
`PlayerState::sync_cue_to_time` (now with no remaining callers) —  rather
than patched a third time. The sentence card/list still update correctly
on every cursor-moving action, since `next_cue`/`previous_cue`/
`jump_to_cue`'s callers (`apply_navigation_result` in `main.rs`) already
call `sentence_card::update_sentence_card`/`sentence_list::update_sentence_list`
directly, synchronously, independent of the removed poll-based path;
`repeat_current_cue` (Space) never needed to trigger either, since it
never moves the cursor. If `Normal` mode or scrub-bar dragging need
live cue tracking later, that should be designed and tested fresh against
whatever their actual seek/playback model turns out to be, not by
reviving this removed mechanism as-is.

## Space works in every mode, as a plain toggle when there's no cue to bound it to

A direct consequence of "no mode autoplays": once opening a video always
lands paused (see above), `Normal` mode had no way to ever start playback
at all — Right/Left/Space were gated behind `sentence-mode-active` in
`app-window.slint`'s `key-pressed` handler, a leftover from before
autoplay-on-open was removed there too. `Right`/`Left` stay gated (cue
navigation has no meaning without a full `Normal` mode implementation,
still `TODO.md` Vaihe 21), but Space is pulled out from behind that guard
and now works in both modes, unconditionally.

On the Rust side, `main.rs`'s `repeat-cue` handler was already the one
callback Space always invokes; it now branches on whether
`PlayerState::repeat_current_cue` returns a cue at all:
`Some` (`SentenceBySentence` mode with a subtitle linked) hands the
`PlaySpanCommand` to `VideoPlayer::toggle_play_span` exactly as before —
bounded playback of that one cue's span. `None` (`Normal` mode, or
`SentenceBySentence` before any subtitle exists) instead calls the new
`VideoPlayer::toggle_playback` — a plain, unbounded play/pause toggle
with no seek and no `pause_at` armed, since there's no particular cue's
span to stop at. Reusing the same Slint callback for both, rather than
adding a second one, keeps "Space always means play/pause, but the exact
shape depends on whether a sentence is in focus" as a single decision
point instead of two independently-triggered code paths that could drift
out of sync with each other.

**First pass got the `None` case wrong for `Normal` mode with a subtitle
loaded.** `repeat_current_cue`'s original guard was only `self.cues.get(self.current_cue_index?)?`
— it never checked `self.mode` at all. `current_cue_index` is set to
`Some(0)` by `set_cues` regardless of mode, so a video with a subtitle
linked *while in `Normal` mode* still had a "current cue" in the state
sense, and `repeat_current_cue` happily returned `Some`, routing Space to
the bounded `toggle_play_span` instead of the intended unbounded
`toggle_playback` — Normal-mode playback would start, then immediately
auto-pause at the end of whatever cue happened to be in focus, instead of
continuing. The fix adds the missing mode check directly to
`repeat_current_cue` (`if self.mode != PlaybackMode::SentenceBySentence { return None; }`)
rather than in `main.rs`'s handler, since "a bounded per-cue span is a
`SentenceBySentence`-only concept" belongs in `playback_state`'s own
definition of what `repeat_current_cue` means, not in a caller-side
special case.

## Word analysis: local Ollama, not a cloud API

`TODO.md` Vaihe 24 is a step not called out in the original README
handoff spec — added later, for a concrete language-learning need: given
the sentence currently on screen, break it into words and show each
word's translation and a pronunciation guide (the motivating case was
Hebrew → English, but nothing about the design is Hebrew-specific).
[Ollama](https://ollama.com) running locally (`http://localhost:11434` by
default) was chosen over a cloud LLM API for the same reason whisper-cli
was chosen for subtitle generation: no upload, no per-call cost, and
consistent with trango's "everything runs on-device" posture so far.
Ollama itself isn't a new Cargo dependency — like `whisper-cli`, it's an
external program the user installs and runs separately; trango only talks
to its already-running HTTP API.

### Crate split: `word-analysis`

Following the same reasoning as `subtitle`/`playback-state`
(`docs/src/architecture/crates.md`), the HTTP/JSON/cache-file logic lives
in its own crate (`crates/word-analysis`) with no Slint or libmpv
dependency, so it's unit-testable — including against a hand-rolled local
mock HTTP server — without a UI or a real Ollama install. `crates/app`
only adds the thread-spawning and `AppWindow`-property wiring on top,
mirroring `subtitle::SubtitleGenerator` (pure trait) vs.
`subtitle_generation.rs` (UI wiring)'s split.

One naming wrinkle from this: the app-local wiring module is also named
`word_analysis` (`crates/app/src/word_analysis.rs`), which collides with
the external crate `word_analysis` (Cargo's `word-analysis` package name,
hyphens become underscores) at `main.rs`'s crate root, since `mod
word_analysis;` there shadows the extern-prelude entry of the same name.
Call sites in `main.rs` that need the *crate* (not the local module) use
a leading `::` (e.g. `::word_analysis::HttpOllamaClient::default()`,
`::word_analysis::cache_path_for(...)`) to force crate-root resolution;
inside `word_analysis.rs` itself there's no collision, since that module
has no local item of the same name in its own scope.

### HTTP client: `ureq`, not `reqwest`

trango has no async runtime anywhere — background work (whisper-cli,
and now Ollama calls) runs via plain `std::thread::spawn` plus
`slint::invoke_from_event_loop` to report back to the UI thread (see
"Background thread, not the UI thread" above). `reqwest` would have
pulled in `tokio` solely for this one feature; `ureq` is synchronous, so
an Ollama call is just a blocking function call made from inside a
`thread::spawn` closure. Asked and approved by the user before adding,
along with `serde_json` (for both the cache file and parsing Ollama's
JSON envelopes), per `CLAUDE.md`.

### Prompt and response shape

`word_analysis::build_prompt(sentence, target_language)`
(`crates/word-analysis/src/ollama.rs`) asks the model to reply with
*only* this JSON shape:

```json
{"words": [{"word": "...", "translation": "...", "pronunciation": "..."}]}
```

The request also sets Ollama's `format: "json"` (JSON-mode decoding) and
`stream: false` — the whole answer comes back as one JSON object with a
`response` string, rather than needing to reassemble a streamed NDJSON
sequence the way gemhunter's `call_ollama` (the sibling project this was
modeled on) does. Some local models still wrap their answer in a
` ```json ` code fence despite `format: "json"`, so
`parse_analysis_response` strips one defensively before parsing — the
same defensive stripping gemhunter's `call_ollama` does.

The source language needs no separate setting at all, since the model
just reads it directly off the sentence text. The target language is a
free-text field in the Open Subtitles dialog (`TODO.md` Vaihe 24.1, see
"Target language: free text, not a fixed list" below) —
`crates/app/src/word_analysis.rs`'s `DEFAULT_TARGET_LANGUAGE` (`"English"`)
is only the value shown before the user has ever typed something else.

### Cache file: one JSON sidecar per subtitle

`word_analysis::cache_path_for(subtitle_path)` swaps the subtitle's
extension for `.wordanalysis.json` in place — `subs.srt` becomes
`subs.wordanalysis.json`, next to it on disk. `AnalysisCache { model,
entries: HashMap<u32, WordAnalysis> }` is keyed by `Cue::index`, not by
sentence text, so re-analyzing after minor subtitle edits doesn't
silently reuse a stale entry for a shifted line. `load_cache`/`save_cache`
follow the same robustness convention as `crates/app/src/config.rs`: a
missing or corrupt cache file becomes an empty `AnalysisCache::default()`
(logged, not an error) — a lost cache means re-analyzing some sentences,
not trango refusing to start or open a subtitle.

Both the Ctrl+A popup and the "Analyze all sentences" batch loop read and
write the *same* cache file, so whichever ran first benefits the other:
running the batch loop overnight, then pressing Ctrl+A the next day, is
just a cache hit with no Ollama call at all.

### Ctrl+A: analyze one sentence, cache-first

`main.rs`'s `wire_word_analysis_popup` resolves the sentence currently
shown in the current-sentence card
(`PlayerState::current_cue_index`/`cues`) — not mode-gated, for the same
reason Ctrl+T (translation toggle) isn't: it targets whatever's on screen
right now, in either `Normal` or `SentenceBySentence` mode. It re-reads
the cache file from disk on every press rather than keeping an in-memory
copy synced across the popup and the batch loop — cheap for a small JSON
file, and guarantees it always reflects the batch loop's latest writes
without extra shared-state bookkeeping. On a cache hit, the popup opens
immediately (`Done` status, no network call); on a miss, it opens
`Loading` and `word_analysis::spawn_analyze_sentence` runs the single-
sentence Ollama call on a background thread, writing the result into the
same cache file once it reports back so the next lookup — Ctrl+A again,
or a later "Analyze all sentences" run — is a cache hit too.

### "Analyze all sentences": incremental saves, not all-or-nothing

`word_analysis::spawn_batch_analyze` loops every cue in the currently
loaded subtitle on a background thread, skipping any cue index already
present in the cache (`HashMap::entry` — see the `clippy::map_entry` note
in `crates/app/src/word_analysis.rs` for why it's written that way rather
than a `contains_key`/`insert` pair) and saving the cache to disk after
**every** newly analyzed cue, not just once at the end. A real subtitle
can be dozens of sentences and each Ollama call can take real time —
saving incrementally means closing the app, a crash, or just deciding to
stop partway through loses nothing already finished; resuming later
picks up exactly where it left off via the same cache-skip check. A cue
that fails to analyze (e.g. a transient Ollama hiccup) is logged and
skipped rather than aborting the whole run; `on_done` reports the *last*
error seen, if any, so the UI can surface "finished, but N sentences
failed" rather than silently declaring success.

### Model selection: same pattern as the whisper model, adapted for a network listing

The Open Subtitles dialog gained an "Ollama model" row next to the
existing whisper-model row (`TODO.md` Vaihe 21.6), reusing the same
`FileListDialog` chrome. The key difference:
`model_picker::list_folder_entries` (whisper) reads a filesystem folder
synchronously, since that's fast and local; Ollama's model list
(`word_analysis::OllamaClient::list_models`, `GET /api/tags`) is a
network call, so `crates/app/src/ollama_model_picker.rs`'s
`spawn_list_models` runs it on a background thread the same way
`subtitle_generation::spawn_generate` runs whisper-cli, with the picker
showing a "Loading models…" state (reusing the existing `folder-label`
text slot for status, rather than adding a new Slint property just for
that) while it's in flight. The picked model persists to
`config::TrangoConfig::ollama_model`, mirroring `whisper_model_path`.

### Target language: free text, not a fixed list

`TODO.md` Vaihe 24.1 fills in the target-language gap the initial Vaihe
24 left open (hardcoded `"English"`). Two options were considered: a
`FileListDialog`-style picker over a fixed list of common languages (no
new UI element, but limited to whatever's on the list), or a free-text
field (any language name, but the first editable text input anywhere in
trango — every other input so far is a button, toggle, or list row).
Asked and the user picked free text: a target language is exactly the
kind of open-ended value a fixed list would keep needing updates for.

The Open Subtitles dialog's Word-analysis section gained a `LineEdit`
(from Slint's `std-widgets`, newly imported — no other component in
`app-window.slint` used one before) next to the Ollama model row,
bound to a new `ollama-target-language` string property. Its `edited`
callback (fires on every keystroke, not just on Enter/blur) invokes
`AppWindow::set-ollama-target-language`, which `main.rs`'s
`wire_ollama_target_language` uses to update the shared
`Rc<RefCell<String>>` `spawn_batch_analyze`/`spawn_analyze_sentence`
read their `target_language` argument from, and persist immediately to
`config::TrangoConfig::ollama_target_language` — the same
"save-on-every-confirmed-change" pattern the Ollama/whisper model rows
already use, just triggered by typing instead of a picker selection.
Saving a whole small TOML file on every keystroke is more disk I/O than
strictly necessary, but the file is tiny and edits to this field are
infrequent — not worth a debounce mechanism for the complexity it'd add.

`ollama_target_language` is `Option<String>` in the config (`None` means
"never edited"), not a plain `String` defaulting to `"English"` at the
config layer — so the *shown default* (`word_analysis::DEFAULT_TARGET_LANGUAGE`)
stays a single source of truth in the Rust code that uses it, rather than
being duplicated into `TrangoConfig::default()` as well.

## Word analysis: reasoning models need `"think": false`, and debug logging for diagnosing bad responses

Found through real usage: with a real Ollama instance and a reasoning-
capable model (`qwen3.5-32k` was the concrete case — the `qwen3` family
supports an extended "thinking" mode before producing a final answer),
every single "Analyze all sentences" call failed with `failed to parse
Ollama response: EOF while parsing a value at line 1 column 0` — the
exact `serde_json` error `serde_json::from_str::<T>("")` produces for a
zero-length input. The outer `/api/generate` envelope parsed fine; the
*inner* `response` field it carried was an empty string, because the
`GenerateRequest` this crate sends never told Ollama to skip reasoning —
the model spent its whole generation budget on internal "thinking"
tokens instead of producing the JSON answer `build_prompt` asked for.
gemhunter's `call_ollama` (the sibling project the whole `/api/generate`
approach was modeled on) already sets `"think": False` for exactly this
reason; `GenerateRequest` gained the same `think: false` field.

Since an empty `response` is a real (if now much rarer) possibility with
any model — not every failure mode is guaranteed fixed by one flag —
`analyze_sentence` also checks for it explicitly before attempting to
parse it as JSON, returning a clear `OllamaError::InvalidResponse`
("Ollama returned an empty response...") instead of forwarding
`serde_json`'s confusing zero-length-input message to the UI.

**Debug logging**, requested directly after diagnosing the above (seeing
`serde_json`'s raw parse error in the log wasn't enough to tell what
Ollama had actually sent back): `analyze_sentence` now logs the full
prompt and the raw `response` text at `tracing::debug!` level. Getting
these to actually show up needed a second fix — `main.rs`'s
`tracing_subscriber::fmt::init()` predates any real use of `RUST_LOG`
(see `docs/src/technology/tracing.md`'s corrected "Pitfalls" section:
the doc previously *claimed* `RUST_LOG` filtering worked, but
`tracing-subscriber`'s `env-filter` feature — required for that — was
never actually enabled, so it silently had no effect). The first pass
fixed this by wiring `init_logging` to `RUST_LOG` directly; asked right
after, the user preferred a CLI flag over an environment variable for a
setting like this — `CLAUDE.md`'s Rust conventions now say so explicitly
(flag or `config.toml`, not an env var, unless it's a rarely-changed
system path like `TRANGO_WHISPER_CLI_PATH`). `main.rs` gained
`extract_debug_flag`, stripping `--debug` out of the CLI args before the
existing positional video/subtitle/translation-path parsing sees them
(so `trango --debug video.mp4 subs.srt` and `trango video.mp4 --debug
subs.srt` both work) and `init_logging(debug: bool)` uses it, when
present, to build a fixed `"info,trango=debug,word_analysis=debug"`
filter directly — no `RUST_LOG` export needed for the common case.
`RUST_LOG` still works underneath when `--debug` isn't passed, as a
lower-level escape hatch for filtering finer than the flag's fixed set
(e.g. `RUST_LOG=word_analysis=trace`).
