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

Currently used only in `crates/app/src/main.rs` (package `trango`):

```rust
tracing_subscriber::fmt::init();
tracing::info!("trango starting");
```

`tracing_subscriber::fmt::init()` installs a default subscriber that
formats events to stdout. As more of the app is built out (libmpv
integration, Slint event handlers), `tracing::debug!`/`warn!`/`error!`
calls will be added at the relevant points — see e.g. `TODO.md` Vaihe 10,
which uses a `tracing::debug!` log to verify state wiring.

## Pitfalls

- `tracing_subscriber::fmt::init()` must be called once, early in `main`,
  before any `tracing::*!` calls — otherwise events are dropped silently.
- Log level filtering follows the `RUST_LOG` environment variable
  convention (e.g. `RUST_LOG=debug cargo run -p trango`); no level filter
  is configured explicitly yet, so the subscriber's default applies.
