//! Compiles the `.slint` UI markup into generated Rust bindings, invoked
//! automatically by Cargo before `main.rs` is compiled.
fn main() {
    slint_build::compile("ui/app-window.slint").expect("failed to compile app-window.slint");
}
