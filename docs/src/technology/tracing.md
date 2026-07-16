# tracing

## What it is

[`tracing`](https://docs.rs/tracing) is a structured logging/diagnostics
framework for Rust, paired here with `tracing-subscriber` for formatting
and emitting log output to the terminal.

## Why it's needed

`CLAUDE.md` requires structured logging via `tracing` instead of
`println!` in production code (levels: `trace`/`debug`/`info`/`warn`/
`error`).

## Why this one

It's the de facto standard structured-logging crate in the Rust ecosystem,
integrates cleanly with async and sync code alike, and `tracing-subscriber`
gives a ready-made formatter so no custom logging setup is needed.

## Usage in this project

`crates/app/src/main.rs`'s `init_logging` (called once, first thing in
`main`) installs the subscriber:

```rust
fn init_logging() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}
```

`tracing::debug!`/`warn!`/`error!` calls are used throughout the app at
the relevant points — see e.g. `TODO.md` Vaihe 10's state-wiring log, or
`crates/word-analysis/src/ollama.rs`'s `analyze_sentence`, which logs the
full prompt sent to Ollama and the raw text it returned at `debug` level
(`TODO.md` Vaihe 24) — useful for diagnosing a model that returns
something `parse_analysis_response` can't make sense of, without needing
to reproduce the call outside trango.

## Pitfalls

- `init_logging()` must be called once, early in `main`, before any
  `tracing::*!` calls — otherwise events are dropped silently.
- Log level filtering follows the `RUST_LOG` environment variable
  convention (e.g. `RUST_LOG=debug cargo run -p trango -- video.mp4`), via
  `tracing-subscriber`'s `env-filter` feature (enabled explicitly in
  `crates/app/Cargo.toml` — it's not part of `tracing-subscriber`'s
  default feature set). Without `RUST_LOG` set, `info`-level logging
  applies, matching the original un-filtered default. `RUST_LOG=debug`
  also enables `debug`-level logs from dependencies (`winit`, etc.), which
  can be noisy — scope it to just this crate's own logging with e.g.
  `RUST_LOG=trango=debug,word_analysis=debug`.
