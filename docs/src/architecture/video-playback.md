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
