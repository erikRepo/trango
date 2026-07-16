# toml

Parses/produces TOML, a simple hand-editable config format, into/from
`serde` types. Used in `crates/app/src/config.rs` to read/write
`TrangoConfig` at `$XDG_CONFIG_HOME/trango/config.toml`. Chosen for the
same reason `Cargo.toml` itself uses it: easy for a user to open and fix
by hand, unlike JSON or a binary format; integrates directly with
`serde`, which trango already needed for this feature.

## Pitfall

A missing or corrupt config file falls back to defaults rather than
erroring — a persisted setting is a convenience, not something trango
should refuse to start over.
