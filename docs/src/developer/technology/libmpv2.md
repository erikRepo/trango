# libmpv2

Rust binding to libmpv's OpenGL render API, letting mpv draw video frames
into our own GL context instead of opening its own window — needed to
embed video inside the Slint window (SPEC.md: "Rust + Slint + libmpv").

The original `libmpv` crate is unmaintained and lacks the render API;
`libmpv2` is an actively maintained fork with a complete `render` module.
`libmpv2-sys` links against the system's `libmpv.so` directly using
pre-generated bindings (no `bindgen`) — `libmpv-dev`/`mpv-libs-devel` must
be installed to build.

`crates/app/src/video_player.rs` owns the `Mpv` core and `RenderContext`,
tied to Slint's window via `set_rendering_notifier` — see
[Video playback](../architecture/video-playback.md) for the mechanism.

## Pitfalls

- `RenderContext<'a>` borrows from `Mpv`; `video_player.rs` sidesteps the
  resulting self-reference with a deliberate `Box::leak` (fine — one
  player per process).
- `OpenGLInitParams::get_proc_address` wants a plain `fn` pointer, not a
  closure — `gl_proc_address_bridge.rs` bridges Slint's borrowed loader
  closure through a thread-local.
- `set_update_callback`'s closure runs on an mpv-internal thread and must
  not call mpv/GL APIs — it only signals via
  `slint::invoke_from_event_loop`.
- mpv's render call always draws at `(0, 0)` of the given framebuffer with
  no offset parameter — confining it to a sub-rectangle needs an offscreen
  FBO (see video-playback.md).
