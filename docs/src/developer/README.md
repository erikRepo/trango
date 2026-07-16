# Developer Guide

This section is for anyone working on TrangoPlayer's own source code,
rather than just using the app. It covers how the codebase is put
together, why specific implementation decisions were made, and the
third-party libraries it depends on.

A few other documents outside this book are worth knowing about:

- The repository root `SPEC.md` is the original product handoff spec —
  views, states, and interactions. `STYLE.md` holds the accompanying
  visual design reference and design tokens.
- `CLAUDE.md` covers the development workflow: TDD, the `scripts/`
  helpers, git workflow, and Rust conventions used throughout the
  codebase.
- `TODO.md` is the step-by-step development roadmap the project was
  built against.

## In this section

- **[Architecture](architecture/crates.md)** — crate structure, the
  state model, and how video playback is embedded in the UI.
- **[Design decisions](specs.md)** — a running log of implementation
  decisions and the bugs/tradeoffs that shaped them, for behavior not
  already covered by the handoff spec.
- **[Technology choices](technology/README.md)** — one page per notable
  dependency: why it was chosen and how it's used here.
