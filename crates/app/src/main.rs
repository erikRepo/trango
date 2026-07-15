//! TrangoPlayer entry point.
//!
//! Initializes logging, then opens the Slint main window (see
//! `ui/app-window.slint`). libmpv integration and the rest of the UI are
//! wired in later development steps (see `TODO.md`).

use std::cell::RefCell;
use std::rc::Rc;

use playback_state::{PlaybackMode, PlayerState};

slint::include_modules!();

/// Prints the current crate version to stdout.
fn print_version() {
    println!("trango {}", env!("CARGO_PKG_VERSION"));
}

/// Owns a fresh `PlayerState` and wires the window's `toggle-mode` callback
/// (invoked by the top bar's segmented control) to
/// `PlayerState::toggle_mode()`, mirroring the resulting mode back into the
/// `sentence-mode-active` Slint property. Returns the shared state so
/// callers can inspect it (used by tests; later steps will read it too).
fn wire_player_state(window: &AppWindow) -> Rc<RefCell<PlayerState>> {
    let state = Rc::new(RefCell::new(PlayerState::new()));

    let state_for_callback = Rc::clone(&state);
    let window_weak = window.as_weak();
    window.on_toggle_mode(move || {
        let mode = {
            let mut state = state_for_callback.borrow_mut();
            state.toggle_mode();
            state.mode
        };
        tracing::debug!(?mode, "playback mode toggled");
        if let Some(window) = window_weak.upgrade() {
            window.set_sentence_mode_active(mode == PlaybackMode::SentenceBySentence);
        }
    });

    state
}

fn main() -> Result<(), slint::PlatformError> {
    tracing_subscriber::fmt::init();
    tracing::info!("trango starting");
    print_version();

    let window = AppWindow::new()?;
    window.set_version(env!("CARGO_PKG_VERSION").into());
    let _player_state = wire_player_state(&window);
    window.run()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_is_set() {
        // Given: the crate's compiled version metadata
        // When:  reading CARGO_PKG_VERSION
        // Then:  it is non-empty, proving the version is wired up for display
        assert!(!env!("CARGO_PKG_VERSION").is_empty());
    }

    // Slint's winit backend can only be initialized once per process (and
    // stays bound to the thread that created it), so every assertion that
    // needs a real `AppWindow` lives in this single test instead of one
    // `AppWindow::new()` call per test — a second call from cargo test's
    // per-test thread fails with "platform was initialized in another
    // thread" / "EventLoop can't be recreated".
    #[test]
    fn test_app_window_properties() {
        // Given: a freshly constructed AppWindow
        let window = AppWindow::new().expect("failed to create AppWindow");

        // When:  the version property is set to CARGO_PKG_VERSION
        // Then:  reading it back returns the same value
        window.set_version(env!("CARGO_PKG_VERSION").into());
        assert_eq!(window.get_version(), env!("CARGO_PKG_VERSION"));

        // When:  reading sentence_mode_active before any interaction
        // Then:  it defaults to false, i.e. the "Normal" segment is active,
        //        matching the freshly wired PlayerState's Normal mode
        assert!(!window.get_sentence_mode_active());
        let player_state = wire_player_state(&window);
        assert_eq!(player_state.borrow().mode, PlaybackMode::Normal);

        // When:  invoking toggle-mode, as a segmented control click does
        // Then:  both the Rust-owned PlayerState and the mirrored Slint
        //        property switch to SentenceBySentence
        window.invoke_toggle_mode();
        assert_eq!(player_state.borrow().mode, PlaybackMode::SentenceBySentence);
        assert!(window.get_sentence_mode_active());

        // When:  invoking toggle-mode again
        // Then:  both flip back to Normal
        window.invoke_toggle_mode();
        assert_eq!(player_state.borrow().mode, PlaybackMode::Normal);
        assert!(!window.get_sentence_mode_active());
    }
}
