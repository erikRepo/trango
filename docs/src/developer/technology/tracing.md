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
fn init_logging(debug: bool) {
    let filter = if debug {
        tracing_subscriber::EnvFilter::new("info,trango=debug,word_analysis=debug")
    } else {
        tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"))
    };
    tracing_subscriber::fmt().with_env_filter(filter).init();
}
```

`debug` comes from the `--debug` CLI flag (`extract_debug_flag`), not an
environment variable — `CLAUDE.md`'s Rust conventions prefer a flag or
`config.toml` over environment variables for anything that's a genuine
per-run/user setting rather than a rarely-changed system path (like
`TRANGO_WHISPER_CLI_PATH`). `RUST_LOG` still works underneath as a
lower-level escape hatch when `--debug` isn't passed, for filtering finer
than the flag's fixed `trango=debug,word_analysis=debug`.

`tracing::debug!`/`warn!`/`error!` calls are used throughout the app at
the relevant points — see e.g. `TODO.md` Vaihe 10's state-wiring log, or
`crates/word-analysis/src/ollama.rs`'s `analyze_sentence`, which logs the
full prompt sent to Ollama and the raw text it returned at `debug` level
(`TODO.md` Vaihe 24) — useful for diagnosing a model that returns
something `parse_analysis_response` can't make sense of, without needing
to reproduce the call outside trango. Run with `--debug` to see these
(`cargo run -p trango -- --debug video.mp4`).

## Pitfalls

- `init_logging()` must be called once, early in `main`, before any
  `tracing::*!` calls — otherwise events are dropped silently.
- `tracing-subscriber`'s `env-filter` feature (needed for both `--debug`'s
  target-scoped filter and `RUST_LOG` support) isn't part of its default
  feature set — enabled explicitly in `crates/app/Cargo.toml`.
- `RUST_LOG=debug` (as opposed to trango's own `--debug` flag) enables
  `debug`-level logs from every dependency too (`winit` in particular is
  very chatty) — if reaching for `RUST_LOG` directly, scope it the same
  way `--debug` does, e.g. `RUST_LOG=trango=debug,word_analysis=debug`.
