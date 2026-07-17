//! Drains live-transcribed cues (`TODO.md` Vaihe 28) into `PlayerState`.
//!
//! `system_audio_capture.rs` spawns one background thread per completed
//! `audio_capture::SpeechSegment`, each running
//! `subtitle::WhisperCliGenerator::transcribe_segment` and sending its
//! resulting cues through the channel [`LiveTranscription::start`] sets up.
//! [`LiveTranscription`] itself owns a repeating `slint::Timer` that drains
//! that channel back on the UI thread, appending each batch to `PlayerState`
//! (`PlayerState::push_cues`) and refreshing the sentence list/current-
//! sentence card whenever any arrive — the Audio source's sentence list
//! updates live, a few hundred milliseconds behind however long each
//! segment's `whisper-cli` call took.
//!
//! The channel indirection (rather than transcription threads touching
//! `AppWindow`/`PlayerState` directly) exists because those threads only
//! carry `Send` data — a `Weak<AppWindow>` wouldn't help here since
//! `PlayerState` lives behind a non-`Send` `Rc<RefCell<_>>`; draining
//! happens back on the UI thread instead, the same reasoning
//! `subtitle_generation.rs`'s `slint::invoke_from_event_loop` pattern
//! follows for the single-shot "Generate subtitles" flow.

use std::cell::RefCell;
use std::rc::{Rc, Weak};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use playback_state::PlayerState;
use slint::{ComponentHandle, Timer, TimerMode};
use subtitle::Cue;

use crate::{sentence_card, sentence_list, AppWindow};

/// How often the UI-thread timer drains newly transcribed cues.
const POLL_INTERVAL: Duration = Duration::from_millis(300);

/// Owns the channel and polling `slint::Timer` that carry live-transcribed
/// cues from background transcription threads onto `PlayerState`. Must be
/// kept alive for as long as live transcription should keep updating the
/// UI — dropping it stops the timer.
pub struct LiveTranscription {
    tx: Sender<Vec<Cue>>,
    rx: RefCell<Receiver<Vec<Cue>>>,
    _timer: Timer,
}

impl LiveTranscription {
    /// Starts polling for newly transcribed cues, appending each batch that
    /// arrives to `state` and refreshing `window`'s sentence list/current-
    /// sentence card. Returns the shared handle — clone
    /// [`LiveTranscription::sender`] for each capture session's per-segment
    /// transcription threads to report their results through.
    pub fn start(window: &AppWindow, state: Rc<RefCell<PlayerState>>) -> Rc<Self> {
        let (tx, rx) = mpsc::channel();
        let this = Rc::new(Self {
            tx,
            rx: RefCell::new(rx),
            _timer: Timer::default(),
        });

        let window_weak = window.as_weak();
        let this_weak = Rc::downgrade(&this);
        this._timer
            .start(TimerMode::Repeated, POLL_INTERVAL, move || {
                poll_tick(&window_weak, &this_weak, &state);
            });

        this
    }

    /// A cloneable sender for a capture session's per-segment transcription
    /// threads (see `system_audio_capture.rs`) to report finished cues
    /// through.
    pub fn sender(&self) -> Sender<Vec<Cue>> {
        self.tx.clone()
    }

    /// Drains whatever cues have arrived since the last call, appending
    /// them to `state` and refreshing `window`'s sentence list/current-
    /// sentence card if any did. Called by the polling timer; exposed
    /// separately so tests can drain deterministically without relying on
    /// the timer actually firing (Slint's test harness doesn't run a real
    /// event loop).
    pub fn drain(&self, window: &AppWindow, state: &Rc<RefCell<PlayerState>>) {
        let mut received_any = false;
        while let Ok(cues) = self.rx.borrow_mut().try_recv() {
            state.borrow_mut().push_cues(cues);
            received_any = true;
        }
        if received_any {
            sentence_card::update_sentence_card(window, &state.borrow());
            sentence_list::update_sentence_list(window, &state.borrow());
        }
    }
}

/// One polling timer tick: upgrades both weak handles and drains, doing
/// nothing if either the window or the `LiveTranscription` itself has since
/// been dropped.
fn poll_tick(
    window_weak: &slint::Weak<AppWindow>,
    this_weak: &Weak<LiveTranscription>,
    state: &Rc<RefCell<PlayerState>>,
) {
    let (Some(window), Some(this)) = (window_weak.upgrade(), this_weak.upgrade()) else {
        return;
    };
    this.drain(&window, state);
}
