//! TrangoPlayer entry point.
//!
//! Currently just initializes logging and prints the crate version — the
//! Slint UI and libmpv integration are wired in later development steps
//! (see `TODO.md`).

/// Prints the current crate version to stdout.
fn print_version() {
    println!("trango {}", env!("CARGO_PKG_VERSION"));
}

fn main() {
    tracing_subscriber::fmt::init();
    tracing::info!("trango starting");
    print_version();
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_version_is_set() {
        // Given: the crate's compiled version metadata
        // When:  reading CARGO_PKG_VERSION
        // Then:  it is non-empty, proving the version is wired up for display
        assert!(!env!("CARGO_PKG_VERSION").is_empty());
    }
}
