# slint

## What it is

[`slint`](https://slint.dev) is a declarative GUI toolkit for Rust. UI
layout and styling are written in `.slint` markup files, compiled at build
time (via `slint-build`) into generated Rust types that the application
code instantiates and drives.

## Why it's needed

The UI framework is already decided in the product handoff spec
(`README.md`): "Rust + Slint + libmpv", with `.slint` markup for
layout/styling and Rust for state/logic.

## Why this one

It's the toolkit named explicitly in `README.md`'s handoff spec — no
alternative was considered for this project.

## Usage in this project

`crates/app` (package `trango`) depends on `slint` (runtime) and
`slint-build` (build-dependency, invoked from `build.rs`):

```rust
// build.rs
fn main() {
    slint_build::compile("ui/app-window.slint").expect("failed to compile app-window.slint");
}
```

`crates/app/ui/app-window.slint` defines the `AppWindow` component: the
main window shell (`#1c1d22` background) and a 52px top bar (`#202127`)
in its full Vaihe 9 visual form — accent dot + "TrangoPlayer" wordmark,
a Normal / Sentence by sentence segmented control, and two ghost buttons
("Open video…", "Open subtitles…"). Colors follow README.md's Design
Tokens as a `global Palette` block; the segmented control and ghost
buttons are small local components (`SegmentButton`, `GhostButton`)
reused for both instances. The app version is shown in the window title
(`"TrangoPlayer v{version}"`) rather than in the top bar itself, to match
the pixel reference (`sketch/design_reference.dc.html#1c`), which has no
version text in that area. `main.rs` pulls in the generated bindings with
`slint::include_modules!()`, then:

```rust
let window = AppWindow::new()?;
window.set_version(env!("CARGO_PKG_VERSION").into());
window.run()
```

The segmented control only flips a local `sentence-mode-active` Slint
property so far (`SegmentButton`'s `TouchArea` sets it directly) — wiring
it to `playback-state::PlayerState::toggle_mode()` is `TODO.md` Vaihe 10.
The ghost buttons are static; the video column, sentence panel, and
bottom hint bar are added in later `TODO.md` steps.

## Pitfalls

- `.slint` files are not Rust — `cargo fmt`/`cargo clippy` don't touch
  them; formatting/linting is Slint's own responsibility (no equivalent
  tool wired into `scripts/check.sh` yet).
- `AppWindow::new()` can be constructed and its properties set/read
  without a visible window appearing (no `.show()`/`.run()` call), which
  is what makes `test_app_window_properties` in `main.rs` possible
  without a display-dependent test harness. Displaying a window still
  requires a windowing backend (winit, via `i-slint-backend-winit`) and a
  display connection (X11/Wayland) — not guaranteed in every CI
  environment, which is why `TODO.md` Vaihe 8/9's test criterion is a
  manual `cargo run -p trango` check, not an automated one.
- Slint's winit backend only allows **one** platform/event-loop
  initialization per process, bound to the thread that created it. A
  second `AppWindow::new()` call from a different thread (e.g. a second
  `#[test]` function — `cargo test` runs each test on its own thread even
  with `--test-threads=1`) fails with "platform was initialized in
  another thread" / "EventLoop can't be recreated". All assertions that
  need a real `AppWindow` must live in a single test function.
- `font-family: "Inter"` / `"JetBrains Mono"` (used for the top bar text,
  per README.md's Design Tokens) resolve to those fonts only if they are
  installed as system fonts; Slint falls back to its default font
  silently otherwise, no build/runtime error. No font files are bundled
  with the app yet.
- `HorizontalLayout`/`VerticalLayout` default `cross-axis-alignment` is
  `stretch` — every direct child fills the full cross-axis extent (e.g.
  the full height of a `HorizontalLayout`) regardless of its own
  min/preferred height. This is a *different* property from
  `horizontal-stretch`/`vertical-stretch`, which only control growth
  along the layout's **main** axis and have no effect on cross-axis
  sizing. The top bar's row sets `cross-axis-alignment: center;` so the
  segmented control and ghost buttons size to their own padding instead
  of filling the whole 52px bar.
