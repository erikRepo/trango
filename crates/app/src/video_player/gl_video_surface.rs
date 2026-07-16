//! Raw OpenGL bindings for compositing mpv's rendered frame into a
//! sub-rectangle of the window's framebuffer.
//!
//! mpv's render API always draws starting at `(0, 0)` of whatever
//! framebuffer/size it's told about, scaled to fill it — there is no
//! parameter to offset it into an arbitrary sub-rectangle of an
//! already-bound, larger framebuffer (see `render_frame`'s doc comment in
//! the parent module). So instead mpv renders into its own offscreen
//! texture sized to exactly the video frame's box, and
//! [`VideoSurface::blit_into`] copies that into the right place in the
//! window's framebuffer afterwards, via `glBlitFramebuffer` — which *does*
//! take independent source/destination rectangles.

use std::ffi::{c_void, CStr, CString};
use std::os::raw::{c_int, c_uint};

type GLenum = c_uint;
type GLuint = c_uint;
type GLint = c_int;
type GLsizei = c_int;
type GLbitfield = c_uint;

const GL_TEXTURE_2D: GLenum = 0x0DE1;
const GL_RGBA8: GLint = 0x8058;
const GL_RGBA: GLenum = 0x1908;
const GL_UNSIGNED_BYTE: GLenum = 0x1401;
const GL_TEXTURE_MIN_FILTER: GLenum = 0x2801;
const GL_TEXTURE_MAG_FILTER: GLenum = 0x2800;
const GL_LINEAR: GLint = 0x2601;
const GL_TEXTURE_WRAP_S: GLenum = 0x2802;
const GL_TEXTURE_WRAP_T: GLenum = 0x2803;
const GL_CLAMP_TO_EDGE: GLint = 0x812F;
const GL_FRAMEBUFFER: GLenum = 0x8D40;
const GL_READ_FRAMEBUFFER: GLenum = 0x8CA8;
const GL_DRAW_FRAMEBUFFER: GLenum = 0x8CA9;
const GL_COLOR_ATTACHMENT0: GLenum = 0x8CE0;
const GL_COLOR_BUFFER_BIT: GLbitfield = 0x0000_4000;
const GL_FRAMEBUFFER_COMPLETE: GLenum = 0x8CD5;

type GenObjectsFn = unsafe extern "C" fn(GLsizei, *mut GLuint);
type DeleteObjectsFn = unsafe extern "C" fn(GLsizei, *const GLuint);
type BindFramebufferFn = unsafe extern "C" fn(GLenum, GLuint);
type BindTextureFn = unsafe extern "C" fn(GLenum, GLuint);
type TexImage2DFn = unsafe extern "C" fn(
    GLenum,
    GLint,
    GLint,
    GLsizei,
    GLsizei,
    GLint,
    GLenum,
    GLenum,
    *const c_void,
);
type TexParameteriFn = unsafe extern "C" fn(GLenum, GLenum, GLint);
type FramebufferTexture2DFn = unsafe extern "C" fn(GLenum, GLenum, GLenum, GLuint, GLint);
type CheckFramebufferStatusFn = unsafe extern "C" fn(GLenum) -> GLenum;
#[allow(clippy::too_many_arguments)]
type BlitFramebufferFn = unsafe extern "C" fn(
    GLint,
    GLint,
    GLint,
    GLint,
    GLint,
    GLint,
    GLint,
    GLint,
    GLbitfield,
    GLenum,
);

/// Resolved function pointers for the (widely available on desktop
/// GL/GLES3) framebuffer-object calls [`VideoSurface`] needs. Resolved
/// once, during `RenderingState::RenderingSetup` — the only point Slint's
/// `get_proc_address` closure is available — the same as `glGetIntegerv` in
/// the parent module.
pub struct GlFns {
    gen_framebuffers: GenObjectsFn,
    delete_framebuffers: DeleteObjectsFn,
    bind_framebuffer: BindFramebufferFn,
    gen_textures: GenObjectsFn,
    delete_textures: DeleteObjectsFn,
    bind_texture: BindTextureFn,
    tex_image_2d: TexImage2DFn,
    tex_parameteri: TexParameteriFn,
    framebuffer_texture_2d: FramebufferTexture2DFn,
    check_framebuffer_status: CheckFramebufferStatusFn,
    blit_framebuffer: BlitFramebufferFn,
}

impl GlFns {
    /// Resolves every function pointer `GlFns` needs via `get_proc_address`.
    /// Returns `None` (logging which symbol failed) if any of them are
    /// unavailable — callers fall back to drawing mpv's frame across the
    /// whole window rather than panicking.
    pub fn resolve(get_proc_address: &dyn Fn(&CStr) -> *const c_void) -> Option<Self> {
        macro_rules! resolve_fn {
            ($name:literal, $ty:ty) => {{
                let cname = CString::new($name).expect("static string has no NUL bytes");
                let ptr = get_proc_address(&cname);
                if ptr.is_null() {
                    tracing::error!(symbol = $name, "failed to resolve GL function");
                    return None;
                }
                unsafe { std::mem::transmute::<*const c_void, $ty>(ptr) }
            }};
        }

        Some(Self {
            gen_framebuffers: resolve_fn!("glGenFramebuffers", GenObjectsFn),
            delete_framebuffers: resolve_fn!("glDeleteFramebuffers", DeleteObjectsFn),
            bind_framebuffer: resolve_fn!("glBindFramebuffer", BindFramebufferFn),
            gen_textures: resolve_fn!("glGenTextures", GenObjectsFn),
            delete_textures: resolve_fn!("glDeleteTextures", DeleteObjectsFn),
            bind_texture: resolve_fn!("glBindTexture", BindTextureFn),
            tex_image_2d: resolve_fn!("glTexImage2D", TexImage2DFn),
            tex_parameteri: resolve_fn!("glTexParameteri", TexParameteriFn),
            framebuffer_texture_2d: resolve_fn!("glFramebufferTexture2D", FramebufferTexture2DFn),
            check_framebuffer_status: resolve_fn!(
                "glCheckFramebufferStatus",
                CheckFramebufferStatusFn
            ),
            blit_framebuffer: resolve_fn!("glBlitFramebuffer", BlitFramebufferFn),
        })
    }
}

/// An offscreen texture-backed framebuffer sized to exactly the video
/// frame's on-screen box, in physical pixels. mpv renders into it via
/// `RenderContext::render` (using [`framebuffer_id`](Self::framebuffer_id)
/// as the target `fbo`); [`blit_into`](Self::blit_into) then copies it into
/// the window's own framebuffer at the right position.
pub struct VideoSurface {
    framebuffer: GLuint,
    texture: GLuint,
    width: i32,
    height: i32,
}

impl VideoSurface {
    /// Creates a new `width`x`height` (physical pixels) offscreen surface.
    /// Returns `None` (logging the incomplete status) if the framebuffer
    /// doesn't come out complete — in practice only reachable from a driver
    /// bug, since callers already guard against `width`/`height` of `0`.
    pub fn new(gl: &GlFns, width: i32, height: i32) -> Option<Self> {
        let mut texture: GLuint = 0;
        let mut framebuffer: GLuint = 0;
        unsafe {
            (gl.gen_textures)(1, &mut texture);
            (gl.bind_texture)(GL_TEXTURE_2D, texture);
            (gl.tex_image_2d)(
                GL_TEXTURE_2D,
                0,
                GL_RGBA8,
                width,
                height,
                0,
                GL_RGBA,
                GL_UNSIGNED_BYTE,
                std::ptr::null(),
            );
            (gl.tex_parameteri)(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_LINEAR);
            (gl.tex_parameteri)(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_LINEAR);
            (gl.tex_parameteri)(GL_TEXTURE_2D, GL_TEXTURE_WRAP_S, GL_CLAMP_TO_EDGE);
            (gl.tex_parameteri)(GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, GL_CLAMP_TO_EDGE);

            (gl.gen_framebuffers)(1, &mut framebuffer);
            (gl.bind_framebuffer)(GL_FRAMEBUFFER, framebuffer);
            (gl.framebuffer_texture_2d)(
                GL_FRAMEBUFFER,
                GL_COLOR_ATTACHMENT0,
                GL_TEXTURE_2D,
                texture,
                0,
            );

            let status = (gl.check_framebuffer_status)(GL_FRAMEBUFFER);
            if status != GL_FRAMEBUFFER_COMPLETE {
                tracing::error!(status, "offscreen video framebuffer incomplete");
                (gl.delete_framebuffers)(1, &framebuffer);
                (gl.delete_textures)(1, &texture);
                return None;
            }
        }

        Some(Self {
            framebuffer,
            texture,
            width,
            height,
        })
    }

    /// This surface's own framebuffer id, as passed to
    /// `RenderContext::render`'s `fbo` argument.
    pub fn framebuffer_id(&self) -> i32 {
        self.framebuffer as i32
    }

    /// This surface's width, in the physical pixels it was created with.
    pub fn width(&self) -> i32 {
        self.width
    }

    /// This surface's height, in the physical pixels it was created with.
    pub fn height(&self) -> i32 {
        self.height
    }

    /// Blits this surface's rendered content into `target_framebuffer` at
    /// physical-pixel rectangle `(dst_x, dst_y, dst_width, dst_height)`,
    /// where `dst_y` is measured from the *top* of the window (Slint/screen
    /// convention) — converted here to OpenGL's bottom-left-origin
    /// convention, which `glBlitFramebuffer`'s destination rectangle always
    /// uses. Leaves `target_framebuffer` bound as both the read and draw
    /// framebuffer afterwards, since that's what Slint's own rendering
    /// (running right after this, in the same frame) expects to find.
    #[allow(clippy::too_many_arguments)]
    pub fn blit_into(
        &self,
        gl: &GlFns,
        target_framebuffer: i32,
        target_physical_height: i32,
        dst_x: i32,
        dst_y: i32,
        dst_width: i32,
        dst_height: i32,
    ) {
        let target_framebuffer = target_framebuffer as GLuint;
        let gl_dst_y0 = target_physical_height - dst_y - dst_height;
        let gl_dst_y1 = target_physical_height - dst_y;
        unsafe {
            (gl.bind_framebuffer)(GL_READ_FRAMEBUFFER, self.framebuffer);
            (gl.bind_framebuffer)(GL_DRAW_FRAMEBUFFER, target_framebuffer);
            (gl.blit_framebuffer)(
                0,
                0,
                self.width,
                self.height,
                dst_x,
                gl_dst_y0,
                dst_x + dst_width,
                gl_dst_y1,
                GL_COLOR_BUFFER_BIT,
                GL_LINEAR as GLenum,
            );
            (gl.bind_framebuffer)(GL_FRAMEBUFFER, target_framebuffer);
        }
    }

    /// Explicitly deletes this surface's GL objects. A plain `Drop` impl
    /// can't do this itself since it needs `gl`'s resolved function
    /// pointers, which live one level up on `VideoPlayerInner` — callers
    /// invoke this themselves when replacing a `VideoSurface` with a
    /// differently-sized one (`render_frame`). The final surface is left
    /// for the OS to reclaim at process exit, the same trade-off already
    /// made for the deliberately-leaked `'static Mpv` in the parent module.
    pub fn destroy(self, gl: &GlFns) {
        unsafe {
            (gl.delete_framebuffers)(1, &self.framebuffer);
            (gl.delete_textures)(1, &self.texture);
        }
    }
}
