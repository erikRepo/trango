# Video playback (libmpv render-API embedding)

`crates/app/src/video_player.rs` (+ its `gl_proc_address_bridge` submodule)
embeds libmpv video playback inside the Slint window, without mpv creating
a window of its own. See `docs/src/technology/libmpv2.md` for the crate
choice and its pitfalls; this page covers the mechanism.

## Mechanism

1. `Mpv::with_initializer` creates an mpv core with `vo=libmpv`, telling it
   not to open a native window — output only happens through the render
   API we drive ourselves.
2. `VideoPlayer::attach` registers a closure via
   `slint::Window::set_rendering_notifier`, which Slint calls at four
   points in its own render loop: `RenderingSetup`, `BeforeRendering`,
   `AfterRendering`, `RenderingTeardown`.
3. On `RenderingSetup` (once, when Slint's OpenGL context first exists),
   `setup_render_context` creates an `mpv::render::RenderContext` sharing
   that same GL context, wires mpv's "a new frame is ready" callback to
   request a Slint redraw, and issues `loadfile`.
4. On every `BeforeRendering` — i.e. immediately before Slint paints its
   own scene into the window's backbuffer for that frame — `render_frame`
   tells mpv to draw the current video frame into whichever framebuffer is
   currently bound. Slint then paints its scene on top of that.

## Why the video shows through at all

Slint's scene painting is not a "clear to black, draw everything" pass —
whatever was already in the backbuffer before Slint's own draw calls stays
visible wherever Slint doesn't paint something opaque over it. Concretely,
in `app-window.slint`:

- The root `Window` has **no** `background` set (transparent).
- The body `Rectangle` (the video area) is filled with `Palette.window-bg`
  *only* while `video-loaded` is `false`; once a video is loaded it's left
  fully transparent.
- The top bar keeps its own opaque `background`, so it's unaffected either
  way.

So with a video loaded, mpv's `BeforeRendering` draw is the only thing
that ever paints color into the video area — Slint's own pass leaves it
untouched.

## Scrub bar: polling mpv's playback-time properties

Unlike frame rendering, the scrub bar (current time / total time / progress
fill + thumb) doesn't need to run inside the render loop — it just needs
mpv's `time-pos` and `duration` properties on a steady cadence. `attach`
starts a repeating `slint::Timer` (`SCRUB_BAR_POLL_INTERVAL`, 33ms — roughly
display refresh rate) that calls `poll_scrub_bar`: it reads both properties
with `Mpv::get_property`,
formats them with `playback_state::format_time` (`MM:SS`, or `H:MM:SS` past
the one-hour mark), and writes `current-time-label`, `duration-label`, and
`scrub-progress` (a `0.0`–`1.0` fraction) on the `AppWindow`, which
`ScrubBar` in `app-window.slint` renders as the mock's 4px track + accent
fill + white thumb.

Plain polling was chosen over `Mpv`'s event-context / `observe_property`
API: two properties on a fixed interval doesn't need a second event source
alongside the rendering notifier, and `slint::Timer` callbacks already run
on the Slint UI thread, so no cross-thread handoff (`invoke_from_event_loop`,
as the render context's update callback needs) is required to set Slint
properties. Before mpv has started decoding a file, both properties return
`Err`; `poll_scrub_bar` treats that as `00:00` / `0.0` rather than
propagating an error, since it isn't one — mpv just hasn't got there yet.

The timer is stored on `VideoPlayer` (`scrub_bar_timer`) purely to keep it
alive: dropping a `slint::Timer` stops it, the same reason the mpv core
itself is kept alive via `'static Mpv` for the process lifetime.

## Current-sentence card: syncing to mpv's time-pos

The same timer tick that drives the scrub bar also calls
`sync_current_sentence`, which mirrors the current subtitle cue into the
sentence panel's `CurrentSentenceCard` (`app-window.slint`). It reads mpv's
`time-pos`, and — only while the shared `PlayerState` (passed into
`VideoPlayer::attach`) is in `SentenceBySentence` mode — calls
`PlayerState::sync_cue_to_time` to move `current_cue_index` to whichever
cue most recently started, then `sentence_card::update_sentence_card` to
write the resulting "Sentence N / M" label and cue text onto the window. In
`Normal` mode it's a no-op, so the card simply keeps showing whatever cue
was last focused (e.g. the first one, set by `set_cues` when subtitles are
loaded) instead of chasing playback.

`sync_current_sentence` also refreshes the sentence list
(`sentence_list::update_sentence_list`), but only when `current_cue_index`
actually changed compared to before the `sync_cue_to_time` call — the poll
tick runs at `SCRUB_BAR_POLL_INTERVAL` regardless of whether the focused cue
moved, and rebuilding the list's `VecModel` on every tick would be pointless
churn for a property that usually stays the same between ticks.

Cue-lookup itself (`sync_cue_to_time`) is plain `Duration` arithmetic with
no mpv/Slint dependency, so it's unit-tested directly in
`playback-state` — only the "read `time-pos`, write it into the window"
integration around it needs a real mpv/Slint instance to exercise the same
way frame rendering does (see below).

## Keyboard navigation: Right/Left/Space → seek + play-to-end + pause

`app-window.slint`'s root `Window` sets `forward-focus: nav-focus`, so a
`FocusScope` (`nav-focus`) wrapping the whole window content always holds
keyboard focus. Its `key-pressed` handler only acts while
`sentence-mode-active` is `true`; it checks `event.text` against
`Key.RightArrow`, `Key.LeftArrow`, and the literal `" "` (Space), and calls
`next-cue()`, `previous-cue()`, or `repeat-cue()` respectively — otherwise
it `reject`s the event, leaving Normal-mode key handling for a later step.

`main.rs`'s `wire_cue_navigation` connects those three callbacks (via
`cue_navigation_handler`) to the matching `PlayerState` method
(`next_cue`/`previous_cue`/`repeat_current_cue`, see `crates.md`'s
navigation section for their pure logic), and separately wires
`on_jump_to_cue` — invoked by the sentence list's row clicks, see below — to
`PlayerState::jump_to_cue`. Both paths funnel through the same
`apply_navigation_result` helper:

1. Runs the navigation method, producing an `Option<SeekCommand>`.
2. Mirrors the resulting cue into the sentence card and sentence list via
   `sentence_card::update_sentence_card`/`sentence_list::update_sentence_list`
   regardless of whether a command came back (e.g. `next_cue` at the last
   cue returns `None` but the cursor hasn't moved, so re-rendering is
   harmless).
3. If a `SeekCommand` was produced, hands it to
   `VideoPlayer::apply_seek_command`.

Sharing `apply_navigation_result` is what makes row clicks behave exactly
like arrow-key navigation, per README's "Sentence list" spec, without
duplicating the seek/pause/card/list-refresh logic in two places.

## Sentence list: row clicks and auto-scroll into view

`SentenceListCard` (`app-window.slint`) renders one row per
`sentence-list-rows` entry (set by `sentence_list::update_sentence_list`),
highlighting whichever row has `is-current` set with an accent-tinted pill.
Clicking a row's `TouchArea` emits `row-clicked(index)`, forwarded by
`AppWindow`'s `jump-to-cue(int)` callback straight to
`wire_cue_navigation`'s `on_jump_to_cue` handler above.

`SentenceListCard` also keeps the current row scrolled into view on its own,
entirely in Slint: a `changed current-index` handler calls its
`bring-into-view` function whenever `sentence-list-current-index` changes
(from a row click, arrow-key navigation, or mpv time-pos sync), adjusting
the underlying `ListView`'s `viewport-y` just enough to bring that row's
fixed-height slot back within `visible-height` — the same technique
Slint's built-in `StandardListViewBase` widget uses internally, reimplemented
here since the sentence list's rows are custom-styled rather than
`StandardListViewItem`s.

`apply_seek_command` issues mpv's `seek <start> absolute` command and sets
`pause` to `false`, then — if `then_pause` is set — arms `pause_at =
Some(command.end)` on `VideoPlayerInner`. There's no mpv command for "play
until this timestamp, then pause", so the scrub bar's existing
`SCRUB_BAR_POLL_INTERVAL` timer tick (already polling `time-pos` for the
scrub bar and current-sentence sync) also calls `apply_pending_pause`,
which pauses mpv and clears `pause_at` once `time-pos` reaches it — reusing
the same poll cadence instead of adding a second timer.

These callbacks are only wired when a video is attached (`wire_cue_navigation`
runs from `main`'s `if let Some(video_player) = &video_player` branch), so
pressing the keys with subtitles but no video loaded is a no-op rather than
a panic.

## Starting paused at the first cue in SentenceBySentence mode

`PlaybackMode::default()` is `SentenceBySentence` (the primary
language-learning use case — see `crates.md`), so a freshly opened video
would otherwise start playing immediately with no cue focused yet, which
doesn't fit "step through one sentence at a time". Right after
`setup_render_context` issues `loadfile`, it calls
`pause_and_arm_start_seek_if_sentence_mode(mpv, &player_state.borrow())`:
if the shared `PlayerState` is in `SentenceBySentence` mode and has cues
loaded (via `subtitle_path_from_args`/`load_subtitles`, which runs before
`VideoPlayer::attach` in `main.rs`), it immediately sets mpv's `pause`
property (safe before the file has loaded) and returns the first cue's
start, which is stored as `VideoPlayerInner::pending_start_seek`. It's a
no-op (no pause, nothing armed) in `Normal` mode or with no cues loaded,
since there's nothing to seek to.

The seek to that timestamp can't happen in the same call: issuing mpv's
`seek` command immediately after `loadfile` fails with `Raw(-12)`
(`MPV_ERROR_COMMAND`) because the core is still idle — nothing has loaded
yet for `seek` to act on, unlike a plain property set. So the seek is
deferred to `apply_pending_start_seek`, called on every existing
`SCRUB_BAR_POLL_INTERVAL` tick alongside `apply_pending_pause`: once mpv's
`time-pos` property becomes readable (the signal that `loadfile` actually
finished), it issues the seek and clears `pending_start_seek`. Playback
stays paused throughout — pausing happens up front, the seek only moves
*where* it's paused once possible.

## Current limitation: no inset video area yet

mpv's render API always draws starting at `(0, 0)` of the framebuffer it's
given, scaled to fill the `width`/`height` passed to `render()` — there is
no parameter to offset it into an arbitrary sub-rectangle of an
already-bound, larger framebuffer. `render_frame` currently passes the
*whole window's* physical size, so the video fills the entire area below
the top bar edge-to-edge, not the inset-with-margins, rounded-corner frame
README.md describes. Getting that requires rendering mpv into its own
offscreen FBO+texture sized to the intended video area and compositing
that — deferred to a later step (`TODO.md` Vaihe 22, "Design-tarkennus").

## Why this needs manual/visual testing

None of this is meaningfully unit-testable:

- The whole mechanism only exists once Slint has a real OpenGL context,
  which requires a real windowing backend and display connection (see
  `docs/src/technology/slint.md`'s pitfalls) — not guaranteed in CI.
- Correctness is about *pixels actually appearing on screen*, which
  `cargo test` has no way to observe. A test could assert
  `render_context.render(...)` returns `Ok(())` and it would still pass
  even if the frame were never actually visible — which is close to what
  happened during development here: an early screenshot showed a plain
  dark frame with `render()` returning `Ok(())` on every call and no
  errors logged anywhere, even though nothing was actually visibly wrong
  (mpv was decoding and playing correctly the whole time — later
  screenshots, taken after giving the window more time to actually present
  a composited frame, showed the video rendering correctly). A solid-color
  `Ok(())` return gives no way to distinguish "video not showing" from
  "video showing, but its content happens to be a similar dark color" —
  only actually looking at distinguishing content (here: the video's own
  text overlay and burned-in subtitles) settled it. Verifying this step
  meant compiling, running `trango` against a real video file, and looking
  at the actual window.

Per `TODO.md` Vaihe 11's own note, this is the first development step
where that's unavoidable, and it stays true for the rest of the video/UI
integration steps that follow it.
