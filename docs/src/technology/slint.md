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
main window shell, `#1c1d22` background, and a 52px top bar (`#202127`)
showing the "TrangoPlayer" wordmark and the app version. `main.rs` pulls
in the generated bindings with `slint::include_modules!()`, then:

```rust
let window = AppWindow::new()?;
window.set_version(env!("CARGO_PKG_VERSION").into());
window.run()
```

Further top bar content (segmented control, ghost buttons), the video
column, and the sentence panel are added in later `TODO.md` steps.

## Pitfalls

- `.slint` files are not Rust — `cargo fmt`/`cargo clippy` don't touch
  them; formatting/linting is Slint's own responsibility (no equivalent
  tool wired into `scripts/check.sh` yet).
- `AppWindow::new()` can be constructed and its properties set/read
  without a visible window appearing (no `.show()`/`.run()` call), which
  is what makes `test_window_version_property_reflects_cargo_version` in
  `main.rs` possible without a display-dependent test harness. Displaying
  a window still requires a windowing backend (winit, via
  `i-slint-backend-winit`) and a display connection (X11/Wayland) — not
  guaranteed in every CI environment, which is why `TODO.md` Vaihe 8's
  test criterion is a manual `cargo run -p trango` check, not an
  automated one.
