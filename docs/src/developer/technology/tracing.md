# tracing

Structured logging framework, paired with `tracing-subscriber` for
terminal output. Used instead of `println!` per CLAUDE.md's conventions
(levels: trace/debug/info/warn/error). Chosen as the ecosystem standard,
with a ready-made formatter.

`main.rs`'s `init_logging(debug: bool)` installs the subscriber once,
first thing in `main`. `debug` comes from the `--debug` CLI flag
(`extract_debug_flag`), not an environment variable — CLAUDE.md prefers a
flag/`config.toml` over env vars for per-run settings. With `--debug`,
the filter is fixed to `"info,trango=debug,word_analysis=debug"`;
otherwise `RUST_LOG` still works as a lower-level escape hatch, defaulting
to `info`.

## Pitfalls

- `init_logging()` must run before any `tracing::*!` calls, or events are
  dropped silently.
- The `env-filter` feature (needed for both `--debug` and `RUST_LOG`)
  isn't in `tracing-subscriber`'s default features — enabled explicitly in
  `crates/app/Cargo.toml`.
- Plain `RUST_LOG=debug` also enables chatty dependency logs (`winit`
  especially) — scope it like `--debug` does, e.g.
  `RUST_LOG=trango=debug,word_analysis=debug`.
