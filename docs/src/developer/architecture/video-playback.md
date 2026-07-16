# Video playback (libmpv render-API embedding)

`crates/app/src/video_player.rs` (+ its `gl_proc_address_bridge` and
`gl_video_surface` submodules) embeds libmpv video playback inside the
Slint window, without mpv creating a window of its own. See
`docs/src/developer/technology/libmpv2.md` for the crate choice and its pitfalls;
this page covers the mechanism.

## Mechanism

1. `Mpv::with_initializer` creates an mpv core with `vo=libmpv` (no native
   window of its own — output only happens through the render API we
   drive ourselves) and `keep-open=yes` (stays loaded and paused on the
   last frame at EOF instead of unloading the file — see "EOF leaves the
   core idle unless `keep-open` is set" below).
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
   tells mpv to draw the current video frame into an offscreen surface
   sized to the video frame box, then blits that surface into the right
   place in the window's own framebuffer (see "Confining mpv to the video
   frame box" below). Slint then paints its scene on top of that.

## Why the video shows through at all

Slint's scene painting is not a "clear to black, draw everything" pass —
whatever was already in the backbuffer before Slint's own draw calls stays
visible wherever Slint doesn't paint something opaque over it. Concretely,
in `app-window.slint`:

- The root `Window` has **no** `background` set (transparent).
- `video-frame`, the named `Rectangle` inside `body-row`'s video column, is
  filled with `Palette.window-bg` *only* while `video-loaded` is `false`;
  once a video is loaded it's left fully transparent.
- Every other element that shares the window with it (top bar, scrub bar,
  sentence panel cards, and — since Vaihe 22 — the padding/spacing around
  them) keeps its own opaque `background`, so it's unaffected either way.

So with a video loaded, mpv's `BeforeRendering` draw is the only thing
that ever paints color into `video-frame`'s box — Slint's own pass leaves
it untouched there, while covering every other pixel of the window.

## Confining mpv to the video frame box

mpv's render API always draws starting at `(0, 0)` of whatever framebuffer
it's given, scaled to fill the `width`/`height` passed to `render()` —
there is no parameter to offset it into an arbitrary sub-rectangle of an
already-bound, larger framebuffer. Before Vaihe 22, `render_frame` passed
the *whole window's* physical size, so mpv's frame filled the entire
window edge-to-edge; only the fact that every other element happened to
paint an opaque background over it (see above) kept it from showing
through gaps. That assumption broke as soon as any part of the window
besides `video-frame` was left without a background — e.g. the
`HorizontalLayout` padding/spacing around the sentence panel, which
until Vaihe 22 painted nothing, letting the full-window mpv frame bleed
through at the window's right edge.

The fix (`crates/app/src/video_player/gl_video_surface.rs`) renders mpv
into its own offscreen texture-backed framebuffer (`VideoSurface`) sized
to exactly `video-frame`'s current on-screen box, then copies that into
the real framebuffer at the right position with `glBlitFramebuffer` —
which, unlike mpv's own render call, *does* take independent source and
destination rectangles. Concretely, `render_frame`:

1. Reads `video-frame`'s resolved box off `AppWindow`'s `video-frame-x`/
   `-y`/`-width`/`-height` properties (`app-window.slint`) — these sum the
   named ancestor elements' own `x`/`y` (`body-row`, `video-column`,
   `video-frame`), since Slint has no built-in "position relative to the
   window" accessor, and multiplies by `Window::scale_factor()` to convert
   from logical to physical pixels.
2. Recreates its cached `VideoSurface` if that box's physical size changed
   since the last frame (window resize, DPI change, or a sentence panel
   layout change) — cheap to skip when it hasn't.
3. Calls `RenderContext::render` with the surface's own framebuffer, so
   mpv fills it edge-to-edge same as before, just at the smaller size.
4. Calls `VideoSurface::blit_into`, which flips the Y coordinate (Slint's
   `video-frame-y` is measured from the window's top; `glBlitFramebuffer`'s
   destination rectangle uses OpenGL's bottom-left-origin convention) and
   blits into the window's currently-bound framebuffer (queried via
   `glGetIntegerv(GL_DRAW_FRAMEBUFFER_BINDING)`, same as before).

`gl_video_surface::GlFns` resolves the handful of FBO/texture/blit GL
functions this needs (`glGenFramebuffers`, `glBlitFramebuffer`, etc.) the
same way `render_frame`'s existing `glGetIntegerv` lookup already did:
once, during `RenderingSetup`, via Slint's `get_proc_address` closure. If
resolving any of them fails (in practice, only on GL implementations
without framebuffer-object support — not expected on any of trango's
supported desktop targets), `render_frame` falls back to the pre-Vaihe-22
behavior of filling the whole window, rather than failing to render video
at all.

Rounding `video-frame`'s corners to match the design mock's inset card
frame is a separate, still-open cosmetic step — clipping the *content* of
a GL-blitted rectangle to rounded corners needs its own stencil/scissor
work beyond this box-confinement fix.

Confining mpv to `video-frame`'s box only makes the video *area* track
window resizes correctly if `video-frame`'s own box actually grows with
the window in the first place — see `app-window.slint`'s content column,
which needs `width: 100%; height: 100%;` for exactly that reason (a plain
`FocusScope` child, unlike a `Layout`'s child, doesn't stretch to fill its
parent on its own).

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
keyboard focus. Its `key-pressed` handler checks for Ctrl+T first — that one
calls `toggle-translation()` unconditionally, since translation visibility
is purely visual and independent of playback mode (see README). Everything
else only acts while `sentence-mode-active` is `true`; it checks
`event.text` against `Key.RightArrow`, `Key.LeftArrow`, and the literal
`" "` (Space), and calls `next-cue()`, `previous-cue()`, or `repeat-cue()`
respectively — otherwise it `reject`s the event, leaving Normal-mode key
handling for a later step.

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

## EOF leaves the core idle unless `keep-open` is set

Without `keep-open=yes` (see "Mechanism" above), mpv's default behavior at
end-of-file is to unload the file entirely and return the core to the same
kind of idle state it starts in before any `loadfile` — not just pause on
the last frame. An idle core rejects every `seek` command outright with
mpv error `Raw(-12)` (`MPV_ERROR_COMMAND`), the same failure mode
documented above for a `seek` issued right after `loadfile` before
anything has actually loaded. Concretely, playing a video to its own end
in `Normal` mode used to permanently break Space (`repeat-cue`/`toggle_playback`
in `video_player.rs`), arrow-key/sentence-list navigation, and the scrub
bar for the rest of the session — no code path recovered from it.

This was independently discovered and worked around once already, for one
specific trigger: generating subtitles for an already-playing video can
take long enough for it to reach EOF mid-generation, and the fix there
was to reload the video via `VideoPlayer::load_video` once generation
finishes (see `docs/src/developer/specs.md`'s "Generating subtitles for an
already-open video reloads it"). That workaround only ever covered its
one trigger — a video reaching EOF on its own in normal use had no
equivalent recovery. `keep-open=yes` fixes it at the source instead: mpv
now stays loaded and paused on the last frame at EOF, so it remains
seekable, and the subtitle-generation reload above is no longer covering
for a gap but is still worth keeping (it also re-arms the
sentence-by-sentence start-of-playback seek onto the newly-generated
subtitle's first cue).

## `attach` always runs at startup — `RenderingSetup` only ever fires once

`VideoPlayer::attach` is called exactly once, unconditionally, right after
`AppWindow` is created in `main` — *even when trango is started without a
CLI video argument*, in which case it's given `video_path: None` and mpv
stays idle (no `loadfile`) until a video is actually picked. This looks
redundant (why attach at all with nothing to play?) but it isn't: Slint's
`RenderingState::RenderingSetup` notification — the only place the OpenGL
loader `setup_render_context` needs is exposed — fires *once per window*,
on its very first rendered frame, not once per `set_rendering_notifier`
call.

An earlier version of this code called `VideoPlayer::attach` lazily, only
once the Open Video dialog (`TODO.md` Vaihe 18) had a file to load — which
worked fine when a CLI video argument was given (that path attaches before
`window.run()`, i.e. before any frame has rendered) but silently broke
video loaded via the dialog with no CLI argument: by the time the user had
clicked through the dialog, the window had already rendered several frames
(the dialog's own UI), so `RenderingSetup` had already fired-and-gone for
good. The render context (and the `loadfile` it would have issued) never
got created; mpv's core stayed permanently idle; every subsequent seek
(arrow keys, Space, sentence list clicks) failed with mpv error `Raw(-12)`
forever, and the failure was silent to the user beyond an unresponsive
sentence-by-sentence UI — the fix (attaching unconditionally at startup)
landed in the same release as the folder-navigation feature that made the
no-CLI-argument path actually reachable in practice, see `releasenotes.md`.

## Loading a video: `attach`'s own path, or later via the Open Video dialog

`VideoPlayer::attach`'s `loadfile` + sentence-by-sentence start-seek arming
lives in a private `load_file` helper (also responsible for setting
`video-loaded`, since that now depends on whether a load was ever actually
requested rather than always following `attach`), reused by two call sites:

- `setup_render_context`, once, on `RenderingSetup` — only if `attach` was
  given a `Some(video_path)` (a CLI argument).
- The public `VideoPlayer::load_video(window, video_path, player_state)`,
  called by `main.rs`'s `open_selected_video` whenever the Open Video
  dialog (`TODO.md` Vaihe 18) picks a file — the session's first video (if
  trango started with no CLI argument) or a later one (switching files
  mid-session). Either way the render context and polling timer from
  `attach` already exist, so this is just the `loadfile` call itself.

Either way, `open_selected_video` resolves (and loads) a same-stem `.srt`
subtitle match — or clears any previously loaded cues if none is found —
*before* calling `load_video`, not after: `load_file`'s sentence-by-sentence
start-pause reads `player_state.cues` to find the first cue to pause at, so
loading the new video first would arm the pause against the *previous*
video's (now stale) cues.

## Why this needs manual/visual testing

None of this is meaningfully unit-testable:

- The whole mechanism only exists once Slint has a real OpenGL context,
  which requires a real windowing backend and display connection (see
  `docs/src/developer/technology/slint.md`'s pitfalls) — not guaranteed in CI.
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
