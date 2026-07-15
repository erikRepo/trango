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
//!
//! Separately, a repeating `slint::Timer` polls mpv's `time-pos`/`duration`
//! properties (see `poll_scrub_bar`) to drive the scrub bar. This is a
//! second, independent way of talking to mpv alongside the rendering
//! notifier above — plain property reads, not tied to the render/GL loop —
//! kept simple rather than wiring up `Mpv`'s event-context/`observe_property`
//! API for just two properties.

mod gl_proc_address_bridge;

use std::cell::RefCell;
use std::ffi::CString;
use std::path::Path;
use std::rc::Rc;
use std::time::Duration;

use libmpv2::render::{OpenGLInitParams, RenderContext, RenderParam, RenderParamApiType};
use libmpv2::Mpv;
use playback_state::{format_time, PlaybackMode, PlayerState, SeekCommand};
use slint::{ComponentHandle, GraphicsAPI, RenderingState, Timer, TimerMode, Weak};

use gl_proc_address_bridge::{
    bridged_get_proc_address, with_bridged_get_proc_address, SlintGlContext,
};

use crate::sentence_card::update_sentence_card;
use crate::sentence_list::update_sentence_list;
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
    /// Timestamp at which the scrub bar poll tick (see [`apply_pending_pause`])
    /// should pause mpv, armed by [`VideoPlayer::apply_seek_command`] for a
    /// [`SeekCommand`] with `then_pause` set. Cleared once reached.
    pause_at: Option<Duration>,
    /// Timestamp the next poll tick (see [`apply_pending_start_seek`]) should
    /// seek mpv to once a file is actually loaded, armed by
    /// [`pause_and_arm_start_seek_if_sentence_mode`] right after `loadfile`.
    /// Deferred rather than seeking immediately: mpv's `seek` command errors
    /// if issued before the core has finished loading anything to seek
    /// within, which `time-pos` becoming readable signals. Cleared once
    /// applied.
    pending_start_seek: Option<Duration>,
}

/// Owns an mpv core registered as a rendering underlay on an [`AppWindow`],
/// plus the scrub bar's polling timer. The rendering notifier and timer
/// closures hold their own `Rc` clone of `inner`; [`VideoPlayer::apply_seek_command`]
/// uses this handle's own clone to drive mpv from `main.rs`'s cue navigation
/// callbacks. `scrub_bar_timer` must be kept alive too: dropping a
/// `slint::Timer` stops it.
pub struct VideoPlayer {
    inner: Rc<RefCell<VideoPlayerInner>>,
    #[allow(dead_code)]
    scrub_bar_timer: Timer,
}

impl VideoPlayer {
    /// Creates an mpv core configured for render-API embedding, registers it
    /// as `window`'s rendering underlay, and starts loading `video_path`.
    ///
    /// The render context itself is created lazily on the first
    /// `RenderingSetup` notification (only that callback exposes the OpenGL
    /// loader mpv needs), so actual playback start is deferred until Slint
    /// delivers it — normally on the very first rendered frame.
    ///
    /// `player_state` is shared with the rest of the app (see `main.rs`); in
    /// `SentenceBySentence` mode, the scrub bar's polling timer also syncs
    /// its `current_cue_index` to mpv's `time-pos` and mirrors the result
    /// into the window's current-sentence card (see [`sync_current_sentence`]).
    pub fn attach(
        window: &AppWindow,
        video_path: &Path,
        player_state: Rc<RefCell<PlayerState>>,
    ) -> anyhow::Result<Self> {
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
            pause_at: None,
            pending_start_seek: None,
        }));

        let video_path = video_path.to_owned();
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
                    &video_path,
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
            sync_current_sentence(&poll_inner, &player_state, &poll_window_weak);
            apply_pending_pause(&poll_inner);
            apply_pending_start_seek(&poll_inner);
        });

        Ok(Self {
            inner,
            scrub_bar_timer,
        })
    }

    /// Applies a `playback_state` navigation `SeekCommand`: seeks mpv to
    /// `command.start`, resumes playback, and — if `command.then_pause` is
    /// set — arms `pause_at` so the next scrub bar poll tick pauses once
    /// `command.end` is reached (see [`apply_pending_pause`]). Called from
    /// `main.rs`'s `next-cue`/`previous-cue`/`repeat-cue` callback handlers.
    pub fn apply_seek_command(&self, command: SeekCommand) {
        let mut inner = self.inner.borrow_mut();
        let mpv = inner.mpv;
        if let Err(err) = mpv.command(
            "seek",
            &[&command.start.as_secs_f64().to_string(), "absolute"],
        ) {
            tracing::error!(%err, "failed to seek mpv");
        }
        if let Err(err) = mpv.set_property("pause", false) {
            tracing::error!(%err, "failed to resume mpv playback after seek");
        }
        inner.pause_at = command.then_pause.then_some(command.end);
    }
}

/// Pauses mpv once its `time-pos` reaches `inner`'s armed `pause_at` (set by
/// [`VideoPlayer::apply_seek_command`]), then clears it. A no-op if nothing
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

/// If `player_state` is in `SentenceBySentence` mode and has cues loaded,
/// pauses mpv immediately — setting the `pause` property is safe before the
/// file has actually loaded, unlike the `seek` command — and returns the
/// first cue's start for the caller to arm as `pending_start_seek`, applied
/// once mpv has something loaded to seek within (see
/// [`apply_pending_start_seek`]). Pausing (rather than leaving playback
/// running until the seek lands) is what makes the learner press
/// Right/Space to begin the first sentence instead of playback starting
/// immediately. Returns `None` (no pause, nothing to arm) in `Normal` mode
/// or with no cues loaded. Called once, right after `setup_render_context`
/// issues `loadfile`.
fn pause_and_arm_start_seek_if_sentence_mode(
    mpv: &Mpv,
    player_state: &PlayerState,
) -> Option<Duration> {
    if player_state.mode != PlaybackMode::SentenceBySentence {
        return None;
    }
    let first_cue = player_state.cues.first()?;
    if let Err(err) = mpv.set_property("pause", true) {
        tracing::error!(%err, "failed to pause mpv at start");
    }
    Some(first_cue.start)
}

/// Seeks mpv to `inner`'s armed `pending_start_seek` (see
/// [`pause_and_arm_start_seek_if_sentence_mode`]) once mpv's `time-pos`
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

/// While `player_state` is in `SentenceBySentence` mode, syncs its
/// `current_cue_index` to mpv's `time-pos` (see
/// `PlayerState::sync_cue_to_time`) and mirrors the resulting cue into the
/// window's current-sentence card. The sentence list is only rebuilt when
/// the cursor's cue actually changed, since this runs on every
/// `SCRUB_BAR_POLL_INTERVAL` tick and rebuilding its model is otherwise
/// pointless churn. A no-op in `Normal` mode, and while mpv hasn't started
/// decoding a file yet (`time-pos` unavailable). Called on
/// `SCRUB_BAR_POLL_INTERVAL` by the timer started in [`VideoPlayer::attach`].
fn sync_current_sentence(
    inner: &Rc<RefCell<VideoPlayerInner>>,
    player_state: &Rc<RefCell<PlayerState>>,
    window_weak: &Weak<AppWindow>,
) {
    let Some(window) = window_weak.upgrade() else {
        return;
    };
    let mut state = player_state.borrow_mut();
    if state.mode != PlaybackMode::SentenceBySentence {
        return;
    }
    let Ok(time_pos) = inner.borrow().mpv.get_property::<f64>("time-pos") else {
        return;
    };
    let previous_cue_index = state.current_cue_index;
    state.sync_cue_to_time(Duration::from_secs_f64(time_pos.max(0.0)));
    update_sentence_card(&window, &state);
    if state.current_cue_index != previous_cue_index {
        update_sentence_list(&window, &state);
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
            } else {
                inner.borrow_mut().pending_start_seek =
                    pause_and_arm_start_seek_if_sentence_mode(mpv, &player_state.borrow());
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
