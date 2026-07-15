# libmpv2

## What it is

[`libmpv2`](https://crates.io/crates/libmpv2) is a Rust binding to
[libmpv](https://github.com/mpv-player/mpv/tree/master/libmpv), the mpv
media player's embeddable client library. `crates/app`'s `render` feature
enables its `render` module, a binding for libmpv's OpenGL render API
(`render.h`/`render_gl.h`), which lets the host application draw video
frames into its own OpenGL context instead of mpv creating its own window.

## Why it's needed

The product spec (`README.md`) names `libmpv` for video playback and
subtitle timing. `TODO.md` Vaihe 11 needs to embed video *inside* the Slint
window (a bounded area, not a separate window), which requires the render
API rather than mpv's default "give me a window handle" embedding mode.

## Why this one

The original `libmpv` crate (`ParadoxSpiral/libmpv-rs`) is unmaintained
(last published 2020, max version 2.0.1) and doesn't expose the render
API. `libmpv2` (a maintained fork, `kohsine/libmpv2-rs`) is actively
published, targets client API major version 2 (matching this system's
libmpv 2.5.0), and exposes a complete `render` module
(`RenderContext`, `OpenGLInitParams`, `RenderParam`) — asked and approved
per `CLAUDE.md` before adding.

## Usage in this project

`crates/app/src/video_player.rs` owns an `Mpv` core (created with
`vo=libmpv`, telling mpv not to open its own window) and, once Slint's
window has an OpenGL context, an `mpv::render::RenderContext` created from
it. The two are tied together via `slint::Window::set_rendering_notifier`
— see `docs/src/architecture/video-playback.md` for the full mechanism.

`crates/app/src/video_player/gl_proc_address_bridge.rs` exists purely to
adapt Slint's `get_proc_address` closure (borrowed, valid only inside one
`RenderingSetup` callback) to the `'static` plain `fn` pointer
`OpenGLInitParams` requires.

`libmpv2-sys` (the low-level FFI crate `libmpv2` depends on) links against
the system's `libmpv.so` directly (`cargo:rustc-link-lib=mpv`) using
pre-generated bindings — no `bindgen`/`libclang` needed at build time, but
`libmpv`'s development headers/shared library must be installed
(`libmpv-dev` / `mpv-libs-devel`, see `TODO.md`'s prerequisites).

## Pitfalls

- `RenderContext<'a>` borrows from the `Mpv` it was created from, which
  makes storing both together in a struct self-referential unless the
  `Mpv`'s address is stable. `video_player.rs` sidesteps this with a
  deliberate `Box::leak` — acceptable since trango has exactly one player
  per (process-lifetime) window.
- `OpenGLInitParams::get_proc_address` is a plain `fn` pointer, not a
  closure, so it can't directly capture Slint's borrowed loader closure —
  see `gl_proc_address_bridge.rs`'s thread-local bridge and its safety
  comments for why that's sound only within a single synchronous call.
- `RenderContext::set_update_callback`'s closure fires on an
  mpv-internal thread and must not call any mpv or GL API — it may only
  signal elsewhere (here: `slint::invoke_from_event_loop` posting a
  `request_redraw()` back onto the UI thread).
- mpv's OpenGL render API always draws starting at `(0, 0)` of whatever
  framebuffer id you pass it — there's no x/y offset parameter to draw
  into an arbitrary sub-rectangle of a larger already-bound framebuffer.
  `video_player.rs` currently renders across the whole window (see
  `docs/src/architecture/video-playback.md`); a real inset video area with
  surrounding chrome will need its own offscreen FBO+texture in a later
  step.
