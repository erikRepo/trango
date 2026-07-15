# Release Notes

Format based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versions correspond to completed development steps, not per-commit semver bumps.

## [Unreleased]

### Added
### Changed
### Fixed
### Removed

## [0.1.2] - 2026-07-15

### Added
- `subtitle` crate: `Cue` data model (`index`, `start`, `end`, `text`) with `Cue::new` validating `start < end`
- `subtitle` crate: `SubtitleError` (`thiserror`) with `InvalidFormat`, `IoError`, `InvalidTiming` variants

## [0.1.1] - 2026-07-15

### Added
- `docs/` mdbook scaffold (`usage/`, `architecture/`, `specs/`, `technology/`)
- `docs/src/architecture/crates.md` describing the Vaihe 1 crate split
- `docs/src/technology/tracing.md`

## [0.1.0] - 2026-07-15

### Added
- Cargo workspace with `crates/subtitle`, `crates/playback-state` (empty libs) and `crates/app` (binary `trango`)
- `tracing`-based logging initialized in `trango`'s `main.rs`; version printed on startup
