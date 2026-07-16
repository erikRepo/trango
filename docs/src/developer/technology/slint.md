# slint

Declarative GUI toolkit for Rust — SPEC.md's handoff spec names it
explicitly ("Rust + Slint + libmpv"), so no alternative was considered.
UI layout/styling live in `.slint` markup (`crates/app/ui/app-window.slint`),
compiled at build time via `slint-build` (`build.rs`) into generated Rust
types `main.rs` instantiates with `slint::include_modules!()`.

## Pitfalls

- `.slint` files aren't Rust — `cargo fmt`/`clippy` don't touch them; no
  linting is wired into `scripts/check.sh`.
- Slint's winit backend allows only **one** platform/event-loop init per
  process, bound to its creating thread — a second `AppWindow::new()` on
  another thread (e.g. a second `#[test]`) fails ("platform was
  initialized in another thread"). All assertions needing a real
  `AppWindow` must live in one test function.
- `AppWindow::new()` needs a working windowing backend + X11/Wayland
  connection to construct at all (winit initializes its event loop
  immediately, even without ever showing the window) — it's not
  display-free. CI runs `scripts/test.sh` under `xvfb-run` for this
  reason (`.github/workflows/ci.yml`). Property-wiring tests never call
  `.show()`, so no compositor/renderer work happens beyond that; actual
  pixels-on-screen checks stay manual (`cargo run -p trango`).
- Custom fonts (`"Inter"`, `"JetBrains Mono"`) only render if installed as
  system fonts; Slint falls back silently otherwise — no fonts are
  bundled.
- `HorizontalLayout`/`VerticalLayout` default to `cross-axis-alignment:
  stretch` — children fill the full cross-axis extent unless a layout
  sets `cross-axis-alignment: center` (as the top bar does).
- No dashed-border support — dashed empty-state rows are approximated
  with a solid muted border instead.
- **`DropArea`/`DragArea` don't relay OS file drops** — only in-app
  `DragArea` sources fire `dropped` (confirmed by grepping
  `i-slint-backend-winit` 1.17.1 for `WindowEvent::DroppedFile` handling:
  there is none), and `DataTransfer` has no file/path payload type yet
  ([tracking issue](https://github.com/slint-ui/slint/issues/1967)). This
  is why subtitle/translation linking uses an in-app file picker instead
  of drag-and-drop — see [Design decisions](../specs.md).
