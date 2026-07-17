# Video playback (libmpv render-API embedding)

`crates/app/src/video_player.rs` (+ `gl_proc_address_bridge`/
`gl_video_surface` submodules) embeds libmpv video playback inside the
Slint window, without mpv creating a window of its own. See
[libmpv2](../technology/libmpv2.md) for the crate choice.

## Mechanism

1. `Mpv::with_initializer` creates an mpv core with `vo=libmpv` (output
   only via the render API) and `keep-open=yes` (stays loaded and paused
   on the last frame at EOF — see below).
2. `VideoPlayer::attach` registers a closure via
   `slint::Window::set_rendering_notifier`, called at `RenderingSetup`,
   `BeforeRendering`, `AfterRendering`, `RenderingTeardown`.
3. On `RenderingSetup` (fires once, on the window's first rendered
   frame), `setup_render_context` creates an `mpv::render::RenderContext`
   sharing Slint's GL context, wires mpv's "frame ready" callback to
   request a Slint redraw, and issues `loadfile`.
4. On every `BeforeRendering`, `render_frame` draws the current video
   frame into an offscreen surface sized to the video frame box, then
   blits it into the window's own framebuffer — Slint paints its own
   scene on top afterward.

## Why the video shows through

Slint doesn't clear-then-redraw the whole backbuffer each frame —
whatever was already there stays visible wherever Slint paints nothing
opaque. `app-window.slint`'s root `Window` has no background, and
`video-frame` (the video column's `Rectangle`) is only filled before a
video loads; every other element (top bar, scrub bar, sentence panel)
keeps its own opaque background. So with a video loaded, mpv's
`BeforeRendering` draw is the only thing painting `video-frame`'s box.

## Confining mpv to the video frame box

mpv's render call always draws at `(0, 0)` of the given framebuffer,
scaled to fill it — there's no way to offset it into a sub-rectangle of a
larger, already-bound framebuffer. `render_frame` (`gl_video_surface.rs`)
works around this by rendering mpv into its own offscreen texture-backed
`VideoSurface`, sized to `video-frame`'s current on-screen box (read off
`AppWindow`'s `video-frame-x/-y/-width/-height` properties, converted to
physical pixels), then `glBlitFramebuffer`s that into the real
framebuffer at the right position (flipping Y, since Slint measures from
the top and OpenGL's blit destination is bottom-left-origin). The surface
is only recreated when its physical size changes. `GlFns` resolves the
handful of FBO/blit GL functions needed once, at `RenderingSetup`; if
that fails (no FBO support), `render_frame` falls back to filling the
whole window rather than not rendering at all.

Rounding `video-frame`'s corners to match the design mock is a separate,
still-open step (would need stencil/scissor clipping of the blitted
rectangle). This box-confinement only works if `video-frame`'s own layout
box actually grows with the window — see `app-window.slint`'s content
column's `width: 100%; height: 100%;`.

## Scrub bar: polling, not events

`attach` starts a `slint::Timer` (`SCRUB_BAR_POLL_INTERVAL`, 33ms) that
reads mpv's `time-pos`/`duration` via `Mpv::get_property`, formats them
with `playback_state::format_time`, and writes `current-time-label`/
`duration-label`/`scrub-progress` onto the window. Plain polling was
chosen over mpv's `observe_property` API since two properties on a fixed
interval don't need a second event source, and `Timer` callbacks already
run on the UI thread (no `invoke_from_event_loop` handoff needed). Before
mpv starts decoding, both properties return `Err`, treated as
`00:00`/`0.0` rather than an error.

## Current-sentence card: syncing to time-pos

The same timer tick calls `sync_current_sentence`, which — only in
`SentenceBySentence` mode — moves `current_cue_index` via
`PlayerState::sync_cue_to_time` and refreshes the current-sentence card
and sentence list (only rebuilding the list when the index actually
changed). In `Normal` mode it's a no-op; `Normal` mode has its own
separate live-sync mechanism (`sync_current_sentence_normal_mode`, see
[Design decisions](../specs.md)). Cue lookup itself is plain `Duration`
arithmetic, unit-tested in `playback-state` without mpv/Slint.

## Keyboard navigation and the sentence list

`key-pressed` on the root `FocusScope` handles Ctrl+T unconditionally
(purely visual); Right/Left/Space only act while `sentence-mode-active`.
All three, plus sentence-list row clicks (`jump-to-cue`), funnel through
`apply_navigation_result` (`main.rs`): run the `PlayerState` navigation
method, mirror the resulting cue into the sentence card/list regardless
of outcome, then hand any `SeekCommand` to
`VideoPlayer::apply_seek_command` — which issues mpv's `seek ...
absolute`, unpauses, and (if the command has an end) arms `pause_at`,
cleared by the poll timer's `apply_pending_pause` once `time-pos` reaches
it (there's no mpv "play until timestamp" command). Sharing one code path
is what makes row clicks behave identically to arrow-key navigation.
`SentenceListCard` scrolls the current row into view itself, in Slint,
via a `changed current-index` handler.

## Starting paused at the first cue

`PlaybackMode::default()` is `SentenceBySentence`, so a freshly loaded
video needs to start paused at the first cue rather than playing
immediately. Right after `loadfile`, `pause_and_arm_start_seek` pauses
mpv (safe pre-load) and — only with cues loaded in `SentenceBySentence`
mode — records the first cue's start as `pending_start_seek`. The seek
itself can't happen in the same call (`seek` right after `loadfile` fails
with `Raw(-12)`, the core still being idle) — it's deferred to
`apply_pending_start_seek`, run on the poll timer once `time-pos` becomes
readable.

## EOF leaves the core idle unless `keep-open` is set

Without `keep-open=yes`, mpv unloads the file entirely at EOF, returning
to the same idle state that rejects every `seek` with `Raw(-12)` —
permanently breaking Space/navigation/scrub-bar for the rest of the
session once a video played to its end. `keep-open=yes` fixes this at the
source; the subtitle-generation reload workaround (see [Design
decisions](../specs.md)) predates this fix and is now a bonus (re-arming
the start-of-playback seek) rather than the only recovery path.

Staying loaded-but-paused at EOF still means unpausing without seeking
does nothing (`time-pos` immediately re-hits the same EOF). `Normal`
mode's/Audio's unbounded `VideoPlayer::toggle_playback` checks mpv's
`eof-reached` property and seeks back to `0` first when set, so Space
replays from the start instead of looking like a no-op once a file has
played through.

## `attach` always runs at startup

`attach` is called exactly once, unconditionally, right after `AppWindow`
is created — even with no CLI video path — because `RenderingSetup`
fires once per window, on its first rendered frame, not once per
`set_rendering_notifier` call. An earlier, lazy version (attaching only
once a video was picked via the dialog) broke the no-CLI-argument path: by
the time a user picked a file, several frames (the dialog's own UI) had
already rendered, `RenderingSetup` had already fired-and-gone, and the
render context — and its `loadfile` — never got created, permanently
idling the core.

`load_file` (used by both `attach`'s own startup load and the public
`VideoPlayer::load_video`, called from `open_selected_media` when a video
is picked later) handles the `loadfile` call, `video-loaded`, and the
sentence-by-sentence start-seek arming either way. `open_selected_media`
always resolves subtitles *before* calling `load_video`, since the
start-pause needs `player_state.cues` to already reflect the new video,
not the previous one.

## Why this needs manual/visual testing

None of this is meaningfully unit-testable: the render path only exists
with a real OpenGL context (a real windowing backend + display
connection, not guaranteed in CI), and correctness is about pixels
actually appearing on screen — `render()` returning `Ok(())` says nothing
about whether a frame was ever actually visible. This was confirmed the
hard way once (an early screenshot showed a plain dark frame with no
errors logged, indistinguishable from "not rendering" until content — the
video's own subtitles/overlay — was compared against later screenshots).
This step, and the rest of the video/UI integration work, is verified by
compiling, running `trango` against a real video, and looking at the
window.
