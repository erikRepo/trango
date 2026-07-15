# toml

## What it is

[`toml`](https://docs.rs/toml) parses and produces TOML — a simple,
human-editable config file format — into/from `serde`-derived Rust types.

## Why it's needed

`TODO.md` Vaihe 21.6 persists trango's picked whisper.cpp model across
restarts, so it needs a config file format and a way to (de)serialize it.

## Why this one

TOML is a common choice for small, hand-editable app config files (it's
what `Cargo.toml` itself uses) — easy for a user to open and edit
directly if something needs fixing, unlike JSON's stricter syntax or a
binary format. The `toml` crate is the standard implementation, and
integrates directly with `serde` (see `docs/src/technology/serde.md`),
which trango already needed for this feature anyway.

## Usage in this project

Used in `crates/app/src/config.rs` (package `trango`) to read/write
`TrangoConfig`:

```rust
let config: TrangoConfig = toml::from_str(&contents)?;
let contents = toml::to_string_pretty(&config)?;
```

The file lives at `$XDG_CONFIG_HOME/trango/config.toml` (falling back to
`$HOME/.config/trango/config.toml`) — see `config.rs`'s `config_path`.

## Pitfalls

- A missing or corrupt config file is handled as "start with defaults",
  not a hard error (`config::load`/`load_from`) — a persisted setting is
  a convenience, not something trango should refuse to start over.
