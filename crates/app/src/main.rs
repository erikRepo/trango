//! TrangoPlayer entry point.
//!
//! Initializes logging, then opens the Slint main window (see
//! `ui/app-window.slint`). libmpv integration and the rest of the UI are
//! wired in later development steps (see `TODO.md`).

slint::include_modules!();

/// Prints the current crate version to stdout.
fn print_version() {
    println!("trango {}", env!("CARGO_PKG_VERSION"));
}

fn main() -> Result<(), slint::PlatformError> {
    tracing_subscriber::fmt::init();
    tracing::info!("trango starting");
    print_version();

    let window = AppWindow::new()?;
    window.set_version(env!("CARGO_PKG_VERSION").into());
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
        // Then:  it defaults to false, i.e. the "Normal" segment is active
        assert!(!window.get_sentence_mode_active());

        // When:  sentence_mode_active is set, as the "Sentence by sentence"
        //        segment's TouchArea click handler does
        // Then:  reading it back reflects the new local Slint state
        window.set_sentence_mode_active(true);
        assert!(window.get_sentence_mode_active());
    }
}
