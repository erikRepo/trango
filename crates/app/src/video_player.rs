//! Embeds libmpv video playback into the Slint window as an OpenGL
//! "underlay" (see `docs/src/developer/architecture/video-playback.md` for the full
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
//!
//! Separately, a repeating `slint::Timer` polls mpv's `time-pos`/`duration`
//! properties (see `poll_scrub_bar`) to drive the scrub bar. This is a
//! second, independent way of talking to mpv alongside the rendering
//! notifier above — plain property reads, not tied to the render/GL loop —
//! kept simple rather than wiring up `Mpv`'s event-context/`observe_property`
//! API for just two properties.

mod gl_proc_address_bridge;
mod gl_video_surface;

use std::cell::RefCell;
use std::ffi::CString;
use std::path::Path;
use std::rc::Rc;
use std::time::Duration;

use libmpv2::render::{OpenGLInitParams, RenderContext, RenderParam, RenderParamApiType};
use libmpv2::Mpv;
use playback_state::{format_time, PlaySpanCommand, PlaybackMode, PlayerState, SeekCommand};
use slint::{ComponentHandle, GraphicsAPI, RenderingState, Timer, TimerMode, Weak};

use gl_proc_address_bridge::{
    bridged_get_proc_address, with_bridged_get_proc_address, SlintGlContext,
};
use gl_video_surface::{GlFns, VideoSurface};

use crate::AppWindow;

/// How often the scrub bar's `Timer` re-reads mpv's `time-pos`/`duration`
/// properties. `Mpv::get_property` is an in-process read off mpv's own
/// core state (no IPC, no disk I/O), so polling at roughly display refresh
/// rate is cheap; 200ms made the thumb visibly step/jump forward instead of
/// gliding, especially on short, sentence-length clips where each tick
/// covers a larger fraction of the total duration.
const SCRUB_BAR_POLL_INTERVAL: Duration = Duration::from_millis(33);

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
    /// Resolved framebuffer/texture/blit GL functions, used by
    /// [`render_frame`] to confine mpv's frame to the video frame box
    /// instead of the whole window — see [`gl_video_surface`]'s doc
    /// comment. `None` if resolving any of them failed, in which case
    /// `render_frame` falls back to filling the whole window as before.
    gl_fns: Option<GlFns>,
    /// The offscreen surface mpv currently renders into, sized to the video
    /// frame box's last-seen physical pixel size — recreated in
    /// [`render_frame`] whenever that size changes (window resize, DPI
    /// change, or sentence panel layout change).
    video_surface: Option<VideoSurface>,
    /// Timestamp at which the scrub bar poll tick (see [`apply_pending_pause`])
    /// should pause mpv, armed by [`VideoPlayer::toggle_play_span`] when it
    /// starts playing a [`PlaySpanCommand`]'s span. Cleared once reached, or
    /// immediately if `toggle_play_span` is called again while still armed
    /// (Space pausing early).
    pause_at: Option<Duration>,
    /// Timestamp the next poll tick (see [`apply_pending_start_seek`]) should
    /// seek mpv to once a file is actually loaded, armed by
    /// [`pause_and_arm_start_seek`] right after `loadfile`.
    /// Deferred rather than seeking immediately: mpv's `seek` command errors
    /// if issued before the core has finished loading anything to seek
    /// within, which `time-pos` becoming readable signals. Cleared once
    /// applied.
    pending_start_seek: Option<Duration>,
}

/// Owns an mpv core registered as a rendering underlay on an [`AppWindow`],
/// plus the scrub bar's polling timer. The rendering notifier and timer
/// closures hold their own `Rc` clone of `inner`; [`VideoPlayer::seek_and_pause`]/
/// [`VideoPlayer::toggle_play_span`] use this handle's own clone to drive
/// mpv from `main.rs`'s cue navigation callbacks. `scrub_bar_timer` must be
/// kept alive too: dropping a `slint::Timer` stops it.
pub struct VideoPlayer {
    inner: Rc<RefCell<VideoPlayerInner>>,
    #[allow(dead_code)]
    scrub_bar_timer: Timer,
}

impl VideoPlayer {
    /// Creates an mpv core configured for render-API embedding, registers it
    /// as `window`'s rendering underlay, and — if `video_path` is given —
    /// starts loading it.
    ///
    /// Always called exactly once per process, right after `AppWindow` is
    /// created, **regardless of whether a video path is given yet**: Slint's
    /// `RenderingState::RenderingSetup` notification — the only place the
    /// OpenGL loader mpv needs is exposed — fires *once* for the whole
    /// window's lifetime, on its very first rendered frame, not once per
    /// `set_rendering_notifier` call. Deferring `attach` until the user
    /// actually picks a video via the Open Video dialog (`TODO.md` Vaihe 18)
    /// — i.e. calling it after the window has already rendered other frames
    /// — would mean `RenderingSetup` never fires for it, so the render
    /// context (and the initial `loadfile`) would never happen and every
    /// later seek would fail against mpv's permanently idle core. Loading a
    /// video after `attach`, whether the first one or a later Open Video
    /// dialog pick, goes through [`VideoPlayer::load_video`] instead, which
    /// only needs the render context to already exist — true as soon as the
    /// first frame has rendered, well before any video is likely to have
    /// been picked.
    ///
    /// `player_state` is shared with the rest of the app (see `main.rs`) —
    /// used here only for the initial `loadfile`'s own start-of-playback
    /// pause/seek (see [`pause_and_arm_start_seek`]). `current_cue_index`
    /// is otherwise only ever moved by explicit navigation/Space actions,
    /// not by anything time-pos-driven — see `docs/src/developer/specs.md`, "No mode
    /// autoplays", for why polling `time-pos` to live-track the current
    /// cue was removed rather than patched again.
    pub fn attach(
        window: &AppWindow,
        video_path: Option<&Path>,
        player_state: Rc<RefCell<PlayerState>>,
    ) -> anyhow::Result<Self> {
        let mpv = Mpv::with_initializer(|init| {
            init.set_property("vo", "libmpv")?;
            // Without this, mpv's core goes idle the moment playback
            // reaches EOF — unloading the file outright rather than
            // pausing on its last frame — and every subsequent seek
            // (arrow-key/sentence-list navigation, Space's repeat-cue, even
            // the scrub bar) fails with mpv error Raw(-12) until the video
            // is reloaded from scratch. That failure mode is already
            // documented and specifically worked around for one trigger
            // (generating subtitles mid-playback — see
            // `docs/src/developer/specs.md`'s "Generating subtitles for an
            // already-open video reloads it"), but any video that simply
            // plays to its own end in Normal mode hit the same wall with
            // no recovery. `keep-open=yes` keeps the core loaded and
            // paused at the last frame instead, so it stays seekable.
            init.set_property("keep-open", "yes")?;
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
            gl_fns: None,
            video_surface: None,
            pause_at: None,
            pending_start_seek: None,
        }));

        let video_path = video_path.map(Path::to_owned);
        let window_weak = window.as_weak();
        let notifier_inner = Rc::clone(&inner);
        let notifier_player_state = Rc::clone(&player_state);
        window
            .window()
            .set_rendering_notifier(move |state, graphics_api| match state {
                RenderingState::RenderingSetup => setup_render_context(
                    &notifier_inner,
                    graphics_api,
                    &window_weak,
                    video_path.as_deref(),
                    &notifier_player_state,
                ),
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

        let scrub_bar_timer = Timer::default();
        let poll_inner = Rc::clone(&inner);
        let poll_window_weak = window.as_weak();
        scrub_bar_timer.start(TimerMode::Repeated, SCRUB_BAR_POLL_INTERVAL, move || {
            poll_scrub_bar(&poll_inner, &poll_window_weak);
            apply_pending_pause(&poll_inner);
            apply_pending_start_seek(&poll_inner);
        });

        Ok(Self {
            inner,
            scrub_bar_timer,
        })
    }

    /// Applies a `playback_state` navigation `SeekCommand`: seeks mpv to
    /// `command.start` and pauses it there. No mode autoplays on navigation
    /// (see `docs/src/developer/specs.md`) — only [`toggle_play_span`](Self::toggle_play_span)
    /// (Space) starts playback. Called from `main.rs`'s `next-cue`/
    /// `previous-cue`/`jump-to-cue` callback handlers.
    pub fn seek_and_pause(&self, command: SeekCommand) {
        let mut inner = self.inner.borrow_mut();
        let mpv = inner.mpv;
        if let Err(err) = mpv.command(
            "seek",
            &[&command.start.as_secs_f64().to_string(), "absolute"],
        ) {
            tracing::error!(%err, "failed to seek mpv");
        }
        if let Err(err) = mpv.set_property("pause", true) {
            tracing::error!(%err, "failed to pause mpv after seek");
        }
        inner.pause_at = None;
    }

    /// Applies a `playback_state` `PlaySpanCommand` (Space's "play/replay
    /// the current cue" directive) as a toggle: if mpv is currently playing
    /// (presumably this same span, mid-play toward its own `pause_at`),
    /// pauses immediately rather than waiting out the rest of it. Otherwise
    /// seeks to `command.start`, resumes playback, and arms `pause_at` so
    /// the next scrub bar poll tick pauses once `command.end` is reached
    /// (see [`apply_pending_pause`]). Called from `main.rs`'s `repeat-cue`
    /// callback handler (wired to Space).
    pub fn toggle_play_span(&self, command: PlaySpanCommand) {
        let mut inner = self.inner.borrow_mut();
        let mpv = inner.mpv;
        let is_playing = mpv.get_property::<bool>("pause").map(|paused| !paused);
        if is_playing.unwrap_or(false) {
            if let Err(err) = mpv.set_property("pause", true) {
                tracing::error!(%err, "failed to pause mpv early");
            }
            inner.pause_at = None;
            return;
        }
        if let Err(err) = mpv.command(
            "seek",
            &[&command.start.as_secs_f64().to_string(), "absolute"],
        ) {
            tracing::error!(%err, "failed to seek mpv");
        }
        if let Err(err) = mpv.set_property("pause", false) {
            tracing::error!(%err, "failed to resume mpv playback after seek");
        }
        inner.pause_at = Some(command.end);
    }

    /// Plain play/pause toggle, unbounded — no seek, no `pause_at` armed to
    /// auto-stop at any particular point. Used for Space when there's no
    /// current cue to bound playback to: `Normal` mode (no per-sentence
    /// span makes sense there), or `SentenceBySentence` mode before any
    /// subtitle is linked. Called from `main.rs`'s `repeat-cue` callback
    /// handler alongside [`toggle_play_span`](Self::toggle_play_span) — see
    /// its doc comment for which of the two a given Space press uses.
    pub fn toggle_playback(&self) {
        let mut inner = self.inner.borrow_mut();
        let mpv = inner.mpv;
        let is_playing = mpv
            .get_property::<bool>("pause")
            .map(|paused| !paused)
            .unwrap_or(false);
        if let Err(err) = mpv.set_property("pause", is_playing) {
            tracing::error!(%err, "failed to toggle mpv playback");
        }
        if is_playing {
            inner.pause_at = None;
        }
    }

    /// Loads `video_path` into this already-attached `VideoPlayer` — used
    /// for every video load, including the one `attach`'s `video_path` names
    /// (if any) as well as later Open Video dialog picks (`TODO.md` Vaihe
    /// 18); see `attach`'s doc comment for why loading is split out from
    /// attaching in the first place. `player_state` should already reflect
    /// the new file's cues (or be cleared) by the time this is called, since
    /// the sentence-by-sentence start-pause/seek this arms reads them.
    pub fn load_video(&self, window: &AppWindow, video_path: &Path, player_state: &PlayerState) {
        let mpv = self.inner.borrow().mpv;
        load_file(
            &self.inner,
            mpv,
            video_path,
            player_state,
            &window.as_weak(),
        );
    }
}

/// Issues mpv's `loadfile` for `video_path`, marks `window`'s `video-loaded`
/// property `true` (via `slint::invoke_from_event_loop`, since this may run
/// from the rendering notifier rather than the plain UI-thread path), and
/// always pauses mpv there — in `SentenceBySentence` mode with cues loaded,
/// also arms `inner`'s `pending_start_seek` (see [`pause_and_arm_start_seek`]).
/// Shared by `setup_render_context`'s initial load (if `attach` was given a
/// path) and `VideoPlayer::load_video`'s later ones.
fn load_file(
    inner: &Rc<RefCell<VideoPlayerInner>>,
    mpv: &Mpv,
    video_path: &Path,
    player_state: &PlayerState,
    window_weak: &Weak<AppWindow>,
) {
    let Some(path_str) = video_path.to_str() else {
        tracing::error!(?video_path, "video path is not valid UTF-8");
        return;
    };
    if let Err(err) = mpv.command("loadfile", &[path_str, "replace"]) {
        tracing::error!(%err, ?video_path, "failed to load video file");
        return;
    }
    inner.borrow_mut().pending_start_seek = pause_and_arm_start_seek(mpv, player_state);

    let loaded_window_weak = window_weak.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(window) = loaded_window_weak.upgrade() {
            window.set_video_loaded(true);
        }
    });
}

/// Pauses mpv once its `time-pos` reaches `inner`'s armed `pause_at` (set by
/// [`VideoPlayer::toggle_play_span`]), then clears it. A no-op if nothing
/// is armed. Called on every `SCRUB_BAR_POLL_INTERVAL` tick.
fn apply_pending_pause(inner: &Rc<RefCell<VideoPlayerInner>>) {
    let mut inner = inner.borrow_mut();
    let Some(pause_at) = inner.pause_at else {
        return;
    };
    let Ok(time_pos) = inner.mpv.get_property::<f64>("time-pos") else {
        return;
    };
    if time_pos >= pause_at.as_secs_f64() {
        if let Err(err) = inner.mpv.set_property("pause", true) {
            tracing::error!(%err, "failed to pause mpv at cue end");
        }
        inner.pause_at = None;
    }
}

/// Pauses mpv immediately after `loadfile` — setting the `pause` property
/// is safe before the file has actually loaded, unlike the `seek` command —
/// unconditionally, regardless of mode or whether any subtitle is loaded:
/// no mode autoplays on its own (see `docs/src/developer/specs.md`, "No mode
/// autoplays"), a video with no subtitle included, so it always lands
/// paused rather than running until the user explicitly starts it (Space).
///
/// In `SentenceBySentence` mode with cues loaded, also returns the first
/// cue's start for the caller to arm as `pending_start_seek`, applied once
/// mpv has something loaded to seek within (see
/// [`apply_pending_start_seek`]) — so sentence-by-sentence playback starts
/// exactly at the first line rather than wherever `loadfile` happened to
/// land (e.g. `0:00`, which may be lead-in silence/titles before the first
/// cue). Returns `None` (nothing to arm) in `Normal` mode or with no cues
/// loaded — the video stays paused at `0:00` there instead. Called once,
/// right after `setup_render_context`/`load_file` issues `loadfile`.
fn pause_and_arm_start_seek(mpv: &Mpv, player_state: &PlayerState) -> Option<Duration> {
    if let Err(err) = mpv.set_property("pause", true) {
        tracing::error!(%err, "failed to pause mpv at start");
    }
    if player_state.mode != PlaybackMode::SentenceBySentence {
        return None;
    }
    player_state.cues.first().map(|cue| cue.start)
}

/// Seeks mpv to `inner`'s armed `pending_start_seek` (see
/// [`pause_and_arm_start_seek`]) once mpv's `time-pos`
/// property becomes readable — the signal that `loadfile` has actually
/// finished loading something to seek within, since issuing `seek`
/// immediately after `loadfile` fails (mpv error `Raw(-12)`, the core is
/// still idle). A no-op if nothing is armed. Called on every
/// `SCRUB_BAR_POLL_INTERVAL` tick.
fn apply_pending_start_seek(inner: &Rc<RefCell<VideoPlayerInner>>) {
    let mut inner = inner.borrow_mut();
    let Some(start) = inner.pending_start_seek else {
        return;
    };
    if inner.mpv.get_property::<f64>("time-pos").is_err() {
        return;
    }
    if let Err(err) = inner
        .mpv
        .command("seek", &[&start.as_secs_f64().to_string(), "absolute"])
    {
        tracing::error!(%err, "failed to seek mpv to first cue's start");
    }
    inner.pending_start_seek = None;
}

/// Reads mpv's `time-pos`/`duration` properties and mirrors them into the
/// scrub bar's Slint properties: formatted `MM:SS` (or `H:MM:SS`) time
/// labels and a `0.0`–`1.0` progress fraction. Called on
/// `SCRUB_BAR_POLL_INTERVAL` by the timer started in [`VideoPlayer::attach`].
/// Both properties are unavailable (an `Err`) before mpv has loaded and
/// started decoding a file, in which case this reports `00:00` / `0.0`
/// rather than propagating the error — there's nothing wrong to report, mpv
/// just hasn't got there yet.
fn poll_scrub_bar(inner: &Rc<RefCell<VideoPlayerInner>>, window_weak: &Weak<AppWindow>) {
    let Some(window) = window_weak.upgrade() else {
        return;
    };
    let mpv = inner.borrow().mpv;

    let time_pos = mpv.get_property::<f64>("time-pos").unwrap_or(0.0);
    let duration = mpv.get_property::<f64>("duration").unwrap_or(0.0);

    window.set_current_time_label(format_time(time_pos).into());
    window.set_duration_label(format_time(duration).into());
    window.set_scrub_progress(if duration > 0.0 {
        (time_pos / duration).clamp(0.0, 1.0) as f32
    } else {
        0.0
    });
}

/// Creates the mpv render context using Slint's OpenGL loader, wires mpv's
/// "a new frame is ready" callback to Slint's redraw scheduling, and — if
/// `video_path` is given (i.e. `attach` was called with one, see its doc
/// comment) — kicks off loading it. Runs once, on the window's first-ever
/// `RenderingSetup`.
fn setup_render_context(
    inner: &Rc<RefCell<VideoPlayerInner>>,
    graphics_api: &GraphicsAPI,
    window_weak: &Weak<AppWindow>,
    video_path: Option<&Path>,
    player_state: &Rc<RefCell<PlayerState>>,
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
    inner.borrow_mut().gl_fns = GlFns::resolve(get_proc_address);

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

    if let Some(video_path) = video_path {
        load_file(inner, mpv, video_path, &player_state.borrow(), window_weak);
    }
}

/// Draws the current video frame into the video frame box's own on-screen
/// position, scaled to fit that box rather than the whole window. Called on
/// `RenderingState::BeforeRendering`, i.e. immediately before Slint paints
/// its own (partly transparent) scene on top.
///
/// mpv's render API has no way to target an arbitrary sub-rectangle of an
/// already-bound framebuffer — it always draws starting at `(0, 0)`, scaled
/// to fill whatever `width`/`height` it's given (see `gl_video_surface`'s
/// doc comment). So this renders mpv into its own offscreen
/// [`VideoSurface`] sized to the video frame box (in physical pixels, read
/// off `AppWindow`'s `video-frame-*` properties — see `app-window.slint`'s
/// doc comment on them), then blits that into place. Falls back to filling
/// the whole window, as before `TODO.md` Vaihe 22, if `gl_fns` failed to
/// resolve during setup.
fn render_frame(inner: &Rc<RefCell<VideoPlayerInner>>, window_weak: &Weak<AppWindow>) {
    let Some(window) = window_weak.upgrade() else {
        return;
    };
    let mut inner = inner.borrow_mut();
    let VideoPlayerInner {
        render_context,
        gl_get_integerv,
        gl_fns,
        video_surface,
        ..
    } = &mut *inner;
    let Some(render_context) = render_context else {
        return;
    };

    let mut fbo = 0i32;
    if let Some(get_integerv) = gl_get_integerv {
        unsafe { get_integerv(GL_DRAW_FRAMEBUFFER_BINDING, &mut fbo) };
    }

    let physical_size = window.window().size();
    let Some(gl_fns) = gl_fns else {
        if let Err(err) = render_context.render::<()>(
            fbo,
            physical_size.width as i32,
            physical_size.height as i32,
            true,
        ) {
            tracing::error!(%err, "mpv render call failed");
        }
        return;
    };

    let scale = window.window().scale_factor();
    let to_physical = |logical: f32| (logical * scale).round() as i32;
    let dst_x = to_physical(window.get_video_frame_x());
    let dst_y = to_physical(window.get_video_frame_y());
    let dst_width = to_physical(window.get_video_frame_width()).max(1);
    let dst_height = to_physical(window.get_video_frame_height()).max(1);

    if video_surface
        .as_ref()
        .is_none_or(|surface| surface.width() != dst_width || surface.height() != dst_height)
    {
        if let Some(old_surface) = video_surface.take() {
            old_surface.destroy(gl_fns);
        }
        *video_surface = VideoSurface::new(gl_fns, dst_width, dst_height);
    }
    let Some(surface) = video_surface else {
        return;
    };

    if let Err(err) = render_context.render::<()>(
        surface.framebuffer_id(),
        surface.width(),
        surface.height(),
        true,
    ) {
        tracing::error!(%err, "mpv render call failed");
        return;
    }

    surface.blit_into(
        gl_fns,
        fbo,
        physical_size.height as i32,
        dst_x,
        dst_y,
        dst_width,
        dst_height,
    );
}
