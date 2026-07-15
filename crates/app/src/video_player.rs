//! Embeds libmpv video playback into the Slint window as an OpenGL
//! "underlay" (see `docs/src/architecture/video-playback.md` for the full
//! picture and why this can't be meaningfully unit-tested).
//!
//! Mechanism: `slint::Window::set_rendering_notifier` lets us hook into
//! Slint's own render loop. During `RenderingSetup` we get one-time access to
//! Slint's OpenGL function loader and use it to create an mpv render context
//! sharing that same GL context. During `BeforeRendering` — i.e. just before
//! Slint paints its own scene into the window's backbuffer — we tell mpv to
//! draw the current video frame directly into that backbuffer. Slint then
//! paints its scene on top; wherever that scene is transparent (the video
//! area in `app-window.slint`, while `video-loaded` is true), the frame mpv
//! just drew remains visible.

mod gl_proc_address_bridge;

use std::cell::RefCell;
use std::ffi::CString;
use std::path::Path;
use std::rc::Rc;

use libmpv2::render::{OpenGLInitParams, RenderContext, RenderParam, RenderParamApiType};
use libmpv2::Mpv;
use slint::{ComponentHandle, GraphicsAPI, RenderingState, Weak};

use gl_proc_address_bridge::{
    bridged_get_proc_address, with_bridged_get_proc_address, SlintGlContext,
};

use crate::AppWindow;

/// `GL_DRAW_FRAMEBUFFER_BINDING` (== `GL_FRAMEBUFFER_BINDING`, same value in
/// desktop GL and GLES) — queried each frame in [`render_frame`] rather than
/// assumed to be `0`, since Slint's renderer isn't guaranteed to have the
/// default framebuffer bound (e.g. with multisampling, the real target can
/// be an intermediate FBO resolved to `0` only at the very end of a frame).
const GL_DRAW_FRAMEBUFFER_BINDING: u32 = 0x8CA6;

/// The mpv core and, once available, the render context used to draw video
/// frames into the Slint window's OpenGL context.
struct VideoPlayerInner {
    mpv: &'static Mpv,
    render_context: Option<RenderContext<'static>>,
    /// Resolved `glGetIntegerv`, used to look up the currently bound
    /// framebuffer in [`render_frame`].
    gl_get_integerv: Option<unsafe extern "C" fn(u32, *mut i32)>,
}

/// Owns an mpv core registered as a rendering underlay on an [`AppWindow`].
/// The rendering notifier closure holds its own `Rc` clone of the shared
/// state and keeps playback going independent of this handle, so `inner`
/// isn't read yet — it's kept here for Vaihe 12 (scrub bar), which will read
/// `time-pos`/`duration` off `inner.borrow().mpv` the same way.
pub struct VideoPlayer {
    #[allow(dead_code)]
    inner: Rc<RefCell<VideoPlayerInner>>,
}

impl VideoPlayer {
    /// Creates an mpv core configured for render-API embedding, registers it
    /// as `window`'s rendering underlay, and starts loading `video_path`.
    ///
    /// The render context itself is created lazily on the first
    /// `RenderingSetup` notification (only that callback exposes the OpenGL
    /// loader mpv needs), so actual playback start is deferred until Slint
    /// delivers it — normally on the very first rendered frame.
    pub fn attach(window: &AppWindow, video_path: &Path) -> anyhow::Result<Self> {
        let mpv = Mpv::with_initializer(|init| {
            init.set_property("vo", "libmpv")?;
            Ok(())
        })
        .map_err(|err| anyhow::anyhow!("failed to create mpv core: {err}"))?;
        // Leaked deliberately: trango has exactly one player per (process-
        // lifetime) window, and libmpv2's `RenderContext<'a>` borrows from
        // `Mpv`, which would make storing both in one struct self-
        // referential without a stable, never-moved address for the `Mpv`.
        let mpv: &'static Mpv = Box::leak(Box::new(mpv));

        let inner = Rc::new(RefCell::new(VideoPlayerInner {
            mpv,
            render_context: None,
            gl_get_integerv: None,
        }));

        let video_path = video_path.to_owned();
        let window_weak = window.as_weak();
        let notifier_inner = Rc::clone(&inner);
        window
            .window()
            .set_rendering_notifier(move |state, graphics_api| match state {
                RenderingState::RenderingSetup => {
                    setup_render_context(&notifier_inner, graphics_api, &window_weak, &video_path)
                }
                RenderingState::BeforeRendering => render_frame(&notifier_inner, &window_weak),
                RenderingState::AfterRendering => {
                    if let Some(render_context) = &notifier_inner.borrow().render_context {
                        render_context.report_swap();
                    }
                }
                RenderingState::RenderingTeardown => {
                    notifier_inner.borrow_mut().render_context = None;
                }
                _ => {}
            })
            .map_err(|err| anyhow::anyhow!("failed to register mpv rendering notifier: {err}"))?;

        Ok(Self { inner })
    }
}

/// Creates the mpv render context using Slint's OpenGL loader, wires mpv's
/// "a new frame is ready" callback to Slint's redraw scheduling, and kicks
/// off loading `video_path`. Runs once, on the first `RenderingSetup`.
fn setup_render_context(
    inner: &Rc<RefCell<VideoPlayerInner>>,
    graphics_api: &GraphicsAPI,
    window_weak: &Weak<AppWindow>,
    video_path: &Path,
) {
    let GraphicsAPI::NativeOpenGL { get_proc_address } = graphics_api else {
        tracing::error!("mpv render context requires Slint's OpenGL renderer");
        return;
    };

    let mpv = inner.borrow().mpv;

    // SAFETY: `get_proc_address` resolves a genuine, process-lifetime-valid
    // C function address (unlike the closure itself, which is only valid
    // during this callback) — transmuting *that* to the known signature of
    // `glGetIntegerv` is the standard way to call a GL function resolved
    // through a loader.
    let gl_get_integerv = {
        let name = CString::new("glGetIntegerv").expect("static string has no NUL bytes");
        let ptr = get_proc_address(&name);
        (!ptr.is_null())
            .then(|| unsafe { std::mem::transmute::<_, unsafe extern "C" fn(u32, *mut i32)>(ptr) })
    };
    inner.borrow_mut().gl_get_integerv = gl_get_integerv;

    let render_context = with_bridged_get_proc_address(get_proc_address, || {
        mpv.create_render_context(vec![
            RenderParam::ApiType(RenderParamApiType::OpenGl),
            RenderParam::InitParams(OpenGLInitParams {
                get_proc_address: bridged_get_proc_address,
                ctx: SlintGlContext,
            }),
        ])
    });

    let mut render_context = match render_context {
        Ok(render_context) => render_context,
        Err(err) => {
            tracing::error!(%err, "failed to create mpv render context");
            return;
        }
    };

    let update_window_weak = window_weak.clone();
    render_context.set_update_callback(move || {
        let window_weak = update_window_weak.clone();
        // Runs on an mpv-internal thread; must not touch mpv or GL state
        // here (see `RenderContext::set_update_callback`'s docs), only
        // schedule work back on the Slint event loop thread.
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(window) = window_weak.upgrade() {
                window.window().request_redraw();
            }
        });
    });

    inner.borrow_mut().render_context = Some(render_context);

    match video_path.to_str() {
        Some(video_path) => {
            if let Err(err) = mpv.command("loadfile", &[video_path, "replace"]) {
                tracing::error!(%err, "failed to load video file");
            }
        }
        None => tracing::error!(?video_path, "video path is not valid UTF-8"),
    }

    let loaded_window_weak = window_weak.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(window) = loaded_window_weak.upgrade() {
            window.set_video_loaded(true);
        }
    });
}

/// Draws the current video frame into whichever framebuffer Slint currently
/// has bound, scaled to the window's current physical size. Called on
/// `RenderingState::BeforeRendering`, i.e. immediately before Slint paints
/// its own (partly transparent) scene on top.
fn render_frame(inner: &Rc<RefCell<VideoPlayerInner>>, window_weak: &Weak<AppWindow>) {
    let Some(window) = window_weak.upgrade() else {
        return;
    };
    let inner = inner.borrow();
    let Some(render_context) = &inner.render_context else {
        return;
    };

    let mut fbo = 0i32;
    if let Some(get_integerv) = inner.gl_get_integerv {
        unsafe { get_integerv(GL_DRAW_FRAMEBUFFER_BINDING, &mut fbo) };
    }

    let size = window.window().size();
    if let Err(err) = render_context.render::<()>(fbo, size.width as i32, size.height as i32, true)
    {
        tracing::error!(%err, "mpv render call failed");
    }
}
