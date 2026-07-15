//! Bridges Slint's borrowed `get_proc_address` closure — only valid inside a
//! single `RenderingState::RenderingSetup` notifier callback — to
//! `libmpv2`'s `OpenGLInitParams`, which requires a `'static` plain `fn`
//! pointer loader instead of a closure.

use std::cell::RefCell;
use std::ffi::{c_void, CStr, CString};

/// Marker type satisfying `libmpv2`'s `OpenGLInitParams<GLContext: 'static>`
/// bound. Carries no data itself — the real loader closure is threaded
/// through the thread-local below instead.
pub struct SlintGlContext;

/// Slint's `get_proc_address` closure shape, as handed out in
/// `GraphicsAPI::NativeOpenGL`.
type GetProcAddress = dyn Fn(&CStr) -> *const c_void;

thread_local! {
    /// Slint's `get_proc_address` closure, lifetime-extended to `'static`
    /// (see [`with_bridged_get_proc_address`]) and valid only while inside a
    /// call to it.
    static GET_PROC_ADDRESS: RefCell<Option<&'static GetProcAddress>> =
        const { RefCell::new(None) };
}

/// Runs `f` with `get_proc_address` available to
/// [`bridged_get_proc_address`], then clears it before returning. Callers
/// must only resolve GL functions synchronously within `f` (e.g. via
/// `Mpv::create_render_context`) — the lifetime-extended closure dangles
/// once this returns, and nothing may retain it past that point.
pub fn with_bridged_get_proc_address<R>(
    get_proc_address: &dyn Fn(&CStr) -> *const c_void,
    f: impl FnOnce() -> R,
) -> R {
    // SAFETY: the `'static` closure is only ever read back synchronously,
    // from within `f`, which cannot outlive this function call — it is
    // cleared again below before `get_proc_address`'s real, shorter
    // lifetime could otherwise be violated.
    let get_proc_address: &'static GetProcAddress =
        unsafe { std::mem::transmute(get_proc_address) };
    GET_PROC_ADDRESS.with(|cell| *cell.borrow_mut() = Some(get_proc_address));
    let result = f();
    GET_PROC_ADDRESS.with(|cell| *cell.borrow_mut() = None);
    result
}

/// Plain `fn` pointer suitable for `OpenGLInitParams::get_proc_address`;
/// forwards to whatever closure [`with_bridged_get_proc_address`] currently
/// has active.
///
/// # Panics
/// Panics if called outside a [`with_bridged_get_proc_address`] call — mpv
/// only invokes this synchronously while resolving GL functions during
/// `Mpv::create_render_context`, which callers are expected to run inside
/// `with_bridged_get_proc_address`.
pub fn bridged_get_proc_address(_ctx: &SlintGlContext, name: &str) -> *mut c_void {
    let get_proc_address = GET_PROC_ADDRESS
        .with(|cell| *cell.borrow())
        .expect("bridged_get_proc_address called outside with_bridged_get_proc_address");
    let name = CString::new(name).unwrap_or_default();
    get_proc_address(&name) as *mut c_void
}
