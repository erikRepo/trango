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

    #[test]
    fn test_window_version_property_reflects_cargo_version() {
        // Given: a freshly constructed AppWindow
        // When:  the version property is set to CARGO_PKG_VERSION
        // Then:  reading it back returns the same value
        let window = AppWindow::new().expect("failed to create AppWindow");
        window.set_version(env!("CARGO_PKG_VERSION").into());
        assert_eq!(window.get_version(), env!("CARGO_PKG_VERSION"));
    }
}
