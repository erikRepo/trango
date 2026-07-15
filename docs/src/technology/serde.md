# serde

## What it is

[`serde`](https://docs.rs/serde) is Rust's standard serialization/
deserialization framework — `#[derive(Serialize, Deserialize)]` generates
the conversion code for a struct/enum against whatever format-specific
crate (here, `toml`) actually reads/writes bytes.

## Why it's needed

`TODO.md` Vaihe 21.6 adds trango's first persisted settings (the picked
whisper.cpp model), which needs (de)serializing to/from a config file.

## Why this one

It's the de facto standard for this in the Rust ecosystem — every
format-specific crate (`toml`, `serde_json`, ...) targets `serde`'s
`Serialize`/`Deserialize` traits, so picking it keeps the config format
swappable later without changing `TrangoConfig` itself.

## Usage in this project

Used in `crates/app/src/config.rs` (package `trango`, `derive` feature
enabled) to make `TrangoConfig` (de)serializable — `toml::from_str`/
`toml::to_string_pretty` do the actual TOML conversion against the trait
impls this derives:

```rust
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrangoConfig {
    pub whisper_model_path: Option<PathBuf>,
    pub whisper_model_folder: Option<PathBuf>,
}
```

## Pitfalls

- None encountered yet — this is a small, single-struct config with no
  versioning/migration concerns so far.
