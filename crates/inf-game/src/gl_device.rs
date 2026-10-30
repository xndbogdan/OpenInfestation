//! OpenGL device for the recovered Direct3D submissions.
//!
//! Retail draws through Direct3D only. Its renderer factory `004EB4B0` builds
//! the Direct3D renderer for settings `+38` (registry `Renderer`) = 1, and
//! startup `004631E0` rewrites 0, the game's own software renderer, to 1, so
//! that renderer is unreachable. Direct3D itself picks a hardware device and
//! otherwise its software emulation (enumeration callback `004F8570`).
//!
//! The port's CPU raster ([`crate::software_frame`]) reproduces that device's
//! pixel rules and stays the reference. This module is the hardware device
//! beside it: the shared consumers prepare, validate and clip exactly the same
//! TL vertex fans and render states, and this device draws them with OpenGL
//! 2.1 into an offscreen framebuffer. That framebuffer has the presented size,
//! which may exceed the native one: native coordinates still map through the
//! native size, so the same frame reaches the output's own pixel density.
//! Presentation then copies it into the window. Device precision, rounding
//! and fill-rule ties belong to the driver and are not native claims.

use std::{
    cell::RefCell,
    collections::HashMap,
    ffi::{c_void, CStr, CString},
    fmt,
    rc::Rc,
};

use sdl2::video::{GLContext, Window};
use thiserror::Error;

use crate::{
    d3d_texture::{CpuTexture, TEXTURE_SURFACE_SIZE},
    d3d_textured::D3dTextureBlend,
    d3d_viewport::D3dTlVertex,
    frame_device::{device_vertex, DeviceDraw, DeviceVertex},
    gl::{
        self,
        types::{GLchar, GLenum, GLint, GLsizei, GLsizeiptr, GLuint},
    },
    presentation::PresentationRect,
    software_frame::D3dTextureSampling,
};

/// Device textures unused for this many presentations are released. Content
/// keys let a texture outlive the CPU store that decoded it, so a later store
/// (the next renderer epoch, or a loading loan) reuses it without an upload.
const TEXTURE_IDLE_PRESENTS: u64 = 600;

const POSITION: GLuint = 0;
const TEXTURE: GLuint = 1;
const DIFFUSE: GLuint = 2;
const FOG: GLuint = 3;

/// Direct3D samples pixel centres at integer coordinates and GL at +0.5, so
/// the shift keeps coverage on the CPU raster's pixels. `u_scale` is
/// `2 / native size` whatever the target's size: native coordinate `x` lands
/// at target coordinate `(x + 0.5) * k` for a target `k` times larger, so
/// native pixel `i` covers target pixels `i*k .. (i+1)*k`. Frame row 0 (the
/// top of the Direct3D surface) is GL row 0, so readback is already top-down;
/// the window presentation flips it. Colour, fog and depth interpolate linearly
/// in screen space (w = 1), as in the CPU raster, and UV divides by the
/// interpolated reciprocal depth when the submission is perspective-correct.
const PRIMITIVE_VERTEX: &str = r#"#version 120
uniform vec2 u_scale;
attribute vec3 a_position;
attribute vec3 a_texture;
attribute vec4 a_diffuse;
attribute vec4 a_fog;
varying vec3 v_texture;
varying vec4 v_diffuse;
varying vec4 v_fog;
varying float v_depth;
void main() {
    gl_Position = vec4((a_position.xy + 0.5) * u_scale - 1.0, 0.0, 1.0);
    v_depth = a_position.z;
    v_texture = a_texture;
    v_diffuse = a_diffuse;
    v_fog = a_fog;
}
"#;

/// Depth comes from the fragment, so no TL depth is clipped by the GL near or
/// far plane; the CPU raster does not clip depth either. Stored depth never
/// exceeds the cleared 1.0, so LESSEQUAL rejects every fragment beyond it.
const FLAT_FRAGMENT: &str = r#"#version 120
uniform float u_depth_test;
varying vec4 v_diffuse;
varying float v_depth;
void main() {
    if (u_depth_test > 0.5 && v_depth > 1.0) discard;
    gl_FragDepth = clamp(v_depth, 0.0, 1.0);
    gl_FragColor = v_diffuse;
}
"#;

/// D3DTBLEND_MODULATEALPHA, then vertex fog: visibility rides in specular
/// alpha and fogs RGB after texturing, before destination blending.
const TEXTURED_FRAGMENT: &str = r#"#version 120
uniform sampler2D u_texture;
uniform float u_depth_test;
varying vec3 v_texture;
varying vec4 v_diffuse;
varying vec4 v_fog;
varying float v_depth;
void main() {
    if (u_depth_test > 0.5 && v_depth > 1.0) discard;
    vec4 color = texture2D(u_texture, v_texture.xy / v_texture.z) * v_diffuse;
    color.rgb = mix(v_fog.rgb, color.rgb, v_fog.a);
    gl_FragDepth = clamp(v_depth, 0.0, 1.0);
    gl_FragColor = color;
}
"#;

const BLIT_VERTEX: &str = r#"#version 120
attribute vec2 a_position;
attribute vec2 a_texture;
varying vec2 v_texture;
void main() {
    gl_Position = vec4(a_position, 0.0, 1.0);
    v_texture = a_texture;
}
"#;

const BLIT_FRAGMENT: &str = r#"#version 120
uniform sampler2D u_texture;
uniform float u_opaque;
varying vec2 v_texture;
void main() {
    vec4 color = texture2D(u_texture, v_texture);
    gl_FragColor = vec4(color.rgb, max(color.a, u_opaque));
}
"#;

#[derive(Debug, Error)]
pub enum GlDeviceError {
    #[error("OpenGL context: {0}")]
    Context(String),
    #[error("OpenGL entry point {0} is unavailable")]
    Missing(&'static str),
    #[error("OpenGL {stage} shader: {log}")]
    Shader { stage: &'static str, log: String },
    #[error("OpenGL program: {0}")]
    Link(String),
    #[error("OpenGL {width}x{height} framebuffer is incomplete (status {status:#06x})")]
    Framebuffer {
        width: u32,
        height: u32,
        status: GLenum,
    },
    #[error("OpenGL error {code:#06x} while {operation}")]
    Call {
        code: GLenum,
        operation: &'static str,
    },
}

struct Program {
    id: GLuint,
    scale: GLint,
    depth_test: GLint,
    texture: GLint,
}

struct BlitProgram {
    id: GLuint,
    texture: GLint,
    opaque: GLint,
}

#[derive(Default)]
struct TextureCache {
    presents: u64,
    entries: HashMap<u64, CachedTexture>,
}

struct CachedTexture {
    name: GLuint,
    last_used: u64,
}

/// One OpenGL context with its programs and device textures. Frames share it
/// through `Rc`, so the context outlives every frame object that uses it.
pub struct GlDevice {
    description: String,
    flat: Program,
    textured: Program,
    blit: BlitProgram,
    vertices: GLuint,
    quad: GLuint,
    textures: RefCell<TextureCache>,
    /// Tells textures' cached names ([`CpuTexture::device_name`]) apart
    /// from another device's.
    serial: u64,
    /// Whether the attribute pointers are the primitive batch's; the blit
    /// sets its own.
    batch_layout: std::cell::Cell<bool>,
    _context: GLContext,
}

impl fmt::Debug for GlDevice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GlDevice")
            .field("description", &self.description)
            .finish_non_exhaustive()
    }
}

impl GlDevice {
    /// Create and make current a context on `window`, which SDL must have
    /// built as an OpenGL window, then prepare the device programs. Any
    /// failure leaves the caller free to fall back to the CPU raster.
    pub fn new(window: &Window) -> Result<Rc<Self>, GlDeviceError> {
        let context = window.gl_create_context().map_err(GlDeviceError::Context)?;
        window
            .gl_make_current(&context)
            .map_err(GlDeviceError::Context)?;
        let video = window.subsystem();
        gl::load_with(|name| video.gl_get_proc_address(name) as *const c_void);
        for (loaded, name) in [
            (gl::GenFramebuffers::is_loaded(), "glGenFramebuffers"),
            (
                gl::FramebufferTexture2D::is_loaded(),
                "glFramebufferTexture2D",
            ),
            (gl::GenRenderbuffers::is_loaded(), "glGenRenderbuffers"),
            (
                gl::CheckFramebufferStatus::is_loaded(),
                "glCheckFramebufferStatus",
            ),
            (gl::CreateShader::is_loaded(), "glCreateShader"),
            (gl::BindAttribLocation::is_loaded(), "glBindAttribLocation"),
            (
                gl::VertexAttribPointer::is_loaded(),
                "glVertexAttribPointer",
            ),
            (gl::GenBuffers::is_loaded(), "glGenBuffers"),
            (gl::ActiveTexture::is_loaded(), "glActiveTexture"),
        ] {
            if !loaded {
                return Err(GlDeviceError::Missing(name));
            }
        }
        // SAFETY: the context created above is current on this thread and
        // every entry point used below was loaded for it.
        unsafe {
            let description = format!(
                "OpenGL {} ({})",
                gl_string(gl::VERSION),
                gl_string(gl::RENDERER)
            );
            let attributes = [
                (POSITION, "a_position"),
                (TEXTURE, "a_texture"),
                (DIFFUSE, "a_diffuse"),
                (FOG, "a_fog"),
            ];
            let flat = Program::new(PRIMITIVE_VERTEX, FLAT_FRAGMENT, &attributes)?;
            let textured = Program::new(PRIMITIVE_VERTEX, TEXTURED_FRAGMENT, &attributes)?;
            let blit_id = link(
                BLIT_VERTEX,
                BLIT_FRAGMENT,
                &[(POSITION, "a_position"), (TEXTURE, "a_texture")],
            )?;
            let blit = BlitProgram {
                id: blit_id,
                texture: uniform(blit_id, "u_texture"),
                opaque: uniform(blit_id, "u_opaque"),
            };
            let mut buffers = [0; 2];
            gl::GenBuffers(2, buffers.as_mut_ptr());
            let device = Self {
                description,
                flat,
                textured,
                blit,
                vertices: buffers[0],
                quad: buffers[1],
                textures: RefCell::default(),
                serial: {
                    static SERIAL: std::sync::atomic::AtomicU64 =
                        std::sync::atomic::AtomicU64::new(0);
                    SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                },
                batch_layout: Default::default(),
                _context: context,
            };
            check("preparing the OpenGL device")?;
            Ok(Rc::new(device))
        }
    }

    /// Driver version and renderer strings, for the startup log.
    pub fn description(&self) -> &str {
        &self.description
    }

    /// Read the window's back buffer, top row first. Call it after
    /// presenting a frame and before the swap.
    pub(crate) fn read_output(&self, output: [u32; 2]) -> Vec<u8> {
        let row = output[0] as usize * 4;
        let mut pixels = vec![0; row * output[1] as usize];
        // SAFETY: the device context is current; the buffer holds exactly
        // width*height RGBA bytes with byte packing.
        unsafe {
            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
            gl::ReadBuffer(gl::BACK);
            gl::PixelStorei(gl::PACK_ALIGNMENT, 1);
            gl::ReadPixels(
                0,
                0,
                output[0] as GLsizei,
                output[1] as GLsizei,
                gl::RGBA,
                gl::UNSIGNED_BYTE,
                pixels.as_mut_ptr().cast(),
            );
        }
        // GL windows count rows from the bottom.
        let mut flipped = Vec::with_capacity(pixels.len());
        for line in pixels.chunks_exact(row).rev() {
            flipped.extend_from_slice(line);
        }
        flipped
    }

    fn texture(&self, texture: &CpuTexture) -> GLuint {
        let mut cache = self.textures.borrow_mut();
        let presents = cache.presents;
        // Looked up since the last retirement: its entry is used this
        // presentation already, so it is live.
        if let Some((serial, at, name)) = texture.device_name.get() {
            if serial == self.serial && at == presents {
                return name;
            }
        }
        let key = texture.content_key();
        if let Some(entry) = cache.entries.get_mut(&key) {
            entry.last_used = presents;
            texture
                .device_name
                .set(Some((self.serial, presents, entry.name)));
            return entry.name;
        }
        let mut name = 0;
        // SAFETY: the device context is current; the CPU allocation is the
        // complete 256x256 RGBA surface.
        unsafe {
            gl::GenTextures(1, &mut name);
            gl::BindTexture(gl::TEXTURE_2D, name);
            // No mipmaps: the base level alone is complete. Each batch sets
            // the submission's filter and address modes.
            set_sampling(gl::NEAREST, gl::CLAMP_TO_EDGE);
            gl::PixelStorei(gl::UNPACK_ALIGNMENT, 1);
            gl::TexImage2D(
                gl::TEXTURE_2D,
                0,
                gl::RGBA8 as GLint,
                TEXTURE_SURFACE_SIZE as GLsizei,
                TEXTURE_SURFACE_SIZE as GLsizei,
                0,
                gl::RGBA,
                gl::UNSIGNED_BYTE,
                texture.rgba().as_ptr().cast(),
            );
        }
        cache.entries.insert(
            key,
            CachedTexture {
                name,
                last_used: presents,
            },
        );
        texture.device_name.set(Some((self.serial, presents, name)));
        name
    }

    fn retire_idle_textures(&self) {
        let mut cache = self.textures.borrow_mut();
        cache.presents += 1;
        let presents = cache.presents;
        cache.entries.retain(|_, entry| {
            let keep = entry.last_used + TEXTURE_IDLE_PRESENTS >= presents;
            if !keep {
                // SAFETY: the device context is current, and every frame
                // flushes its batch before presenting, so no pending draw
                // names a texture idle for this long.
                unsafe { gl::DeleteTextures(1, &entry.name) };
            }
            keep
        });
    }

    /// Draw `texture` over the current viewport, opaque or with its alpha.
    /// `flip` puts texture row 0 at the top of the viewport.
    ///
    /// # Safety
    /// The device context must be current.
    unsafe fn blit(&self, texture: GLuint, flip: bool, opaque: bool) {
        gl::Disable(gl::DEPTH_TEST);
        gl::Disable(gl::BLEND);
        gl::DepthMask(gl::FALSE);
        gl::UseProgram(self.blit.id);
        gl::ActiveTexture(gl::TEXTURE0);
        gl::BindTexture(gl::TEXTURE_2D, texture);
        gl::Uniform1i(self.blit.texture, 0);
        gl::Uniform1f(self.blit.opaque, if opaque { 1.0 } else { 0.0 });
        let (top, bottom) = if flip { (0.0, 1.0) } else { (1.0, 0.0) };
        let quad: [f32; 16] = [
            -1.0, -1.0, 0.0, bottom, 1.0, -1.0, 1.0, bottom, -1.0, 1.0, 0.0, top, 1.0, 1.0, 1.0,
            top,
        ];
        gl::BindBuffer(gl::ARRAY_BUFFER, self.quad);
        gl::BufferData(
            gl::ARRAY_BUFFER,
            std::mem::size_of_val(&quad) as GLsizeiptr,
            quad.as_ptr().cast(),
            gl::STREAM_DRAW,
        );
        gl::VertexAttribPointer(POSITION, 2, gl::FLOAT, gl::FALSE, 16, std::ptr::null());
        gl::VertexAttribPointer(TEXTURE, 2, gl::FLOAT, gl::FALSE, 16, 8 as *const c_void);
        gl::EnableVertexAttribArray(POSITION);
        gl::EnableVertexAttribArray(TEXTURE);
        gl::DisableVertexAttribArray(DIFFUSE);
        gl::DisableVertexAttribArray(FOG);
        self.batch_layout.set(false);
        gl::DrawArrays(gl::TRIANGLE_STRIP, 0, 4);
    }
}

impl Drop for GlDevice {
    fn drop(&mut self) {
        // SAFETY: the context is still alive (dropped after this body) and
        // remains current on the thread that created it.
        unsafe {
            for program in [self.flat.id, self.textured.id, self.blit.id] {
                gl::DeleteProgram(program);
            }
            gl::DeleteBuffers(2, [self.vertices, self.quad].as_ptr());
            for entry in self.textures.get_mut().entries.values() {
                gl::DeleteTextures(1, &entry.name);
            }
        }
    }
}

impl Program {
    /// # Safety
    /// A context must be current.
    unsafe fn new(
        vertex: &str,
        fragment: &str,
        attributes: &[(GLuint, &str)],
    ) -> Result<Self, GlDeviceError> {
        let id = link(vertex, fragment, attributes)?;
        Ok(Self {
            id,
            scale: uniform(id, "u_scale"),
            depth_test: uniform(id, "u_depth_test"),
            texture: uniform(id, "u_texture"),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BatchKey {
    texture: Option<TextureBinding>,
    blend: D3dTextureBlend,
    depth_test: bool,
    depth_write: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TextureBinding {
    name: GLuint,
    filter: GLenum,
    address: GLenum,
}

/// The offscreen colour and depth surface of one frame, plus the pending
/// triangles that share one device state. Submissions are in native pixels;
/// the surface is `target` pixels, the native size or a larger presented size.
pub struct GlFrame {
    device: Rc<GlDevice>,
    native: [u32; 2],
    target: [u32; 2],
    framebuffer: GLuint,
    color: GLuint,
    depth: GLuint,
    surface: GLuint,
    batch: Vec<DeviceVertex>,
    key: Option<BatchKey>,
}

impl fmt::Debug for GlFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GlFrame")
            .field("native", &self.native)
            .field("target", &self.target)
            .field("pending_vertices", &self.batch.len())
            .finish_non_exhaustive()
    }
}

impl GlFrame {
    /// A frame for `native`-pixel submissions drawn at `target` pixels.
    pub(crate) fn new(
        device: &Rc<GlDevice>,
        native: [u32; 2],
        target: [u32; 2],
        clear: [u8; 4],
    ) -> Result<Self, GlDeviceError> {
        let [width, height] = target.map(|value| value as GLsizei);
        let mut frame = Self {
            device: Rc::clone(device),
            native,
            target,
            framebuffer: 0,
            color: 0,
            depth: 0,
            surface: 0,
            batch: Vec::new(),
            key: None,
        };
        // SAFETY: the device context is current. `frame` owns each object
        // as soon as it exists, so an early return releases them.
        unsafe {
            gl::GenTextures(1, &mut frame.color);
            gl::BindTexture(gl::TEXTURE_2D, frame.color);
            set_sampling(gl::NEAREST, gl::CLAMP_TO_EDGE);
            gl::TexImage2D(
                gl::TEXTURE_2D,
                0,
                gl::RGBA8 as GLint,
                width,
                height,
                0,
                gl::RGBA,
                gl::UNSIGNED_BYTE,
                std::ptr::null(),
            );
            gl::GenRenderbuffers(1, &mut frame.depth);
            gl::BindRenderbuffer(gl::RENDERBUFFER, frame.depth);
            gl::RenderbufferStorage(gl::RENDERBUFFER, gl::DEPTH_COMPONENT24, width, height);
            gl::GenFramebuffers(1, &mut frame.framebuffer);
            gl::BindFramebuffer(gl::FRAMEBUFFER, frame.framebuffer);
            gl::FramebufferTexture2D(
                gl::FRAMEBUFFER,
                gl::COLOR_ATTACHMENT0,
                gl::TEXTURE_2D,
                frame.color,
                0,
            );
            gl::FramebufferRenderbuffer(
                gl::FRAMEBUFFER,
                gl::DEPTH_ATTACHMENT,
                gl::RENDERBUFFER,
                frame.depth,
            );
            let status = gl::CheckFramebufferStatus(gl::FRAMEBUFFER);
            if status != gl::FRAMEBUFFER_COMPLETE {
                return Err(GlDeviceError::Framebuffer {
                    width: target[0],
                    height: target[1],
                    status,
                });
            }
        }
        frame.clear_color(clear);
        frame.clear_depth();
        check("creating an OpenGL frame")?;
        Ok(frame)
    }

    /// Append one clipped TL fan, drawing the pending batch first when the
    /// device state changes.
    pub(crate) fn fan(&mut self, polygon: &[D3dTlVertex], draw: &DeviceDraw<'_>) {
        if polygon.len() < 3 {
            return;
        }
        let key = BatchKey {
            texture: draw.texture.as_ref().map(|texturing| TextureBinding {
                name: self.device.texture(texturing.texture),
                filter: filter(texturing.sampling),
                address: address(texturing.sampling),
            }),
            blend: draw.blend,
            depth_test: draw.depth_test,
            depth_write: draw.depth_write,
        };
        if self.key != Some(key) {
            self.flush();
            self.key = Some(key);
        }
        let texturing = draw.texture.as_ref();
        let first = device_vertex(&polygon[0], texturing);
        for edge in polygon[1..].windows(2) {
            self.batch.extend([
                first,
                device_vertex(&edge[0], texturing),
                device_vertex(&edge[1], texturing),
            ]);
        }
    }

    pub(crate) fn clear_color(&mut self, rgba: [u8; 4]) {
        self.flush();
        let [red, green, blue, alpha] = rgba.map(|channel| f32::from(channel) / 255.0);
        // SAFETY: the device context is current; the framebuffer is owned.
        unsafe {
            self.bind();
            gl::ColorMask(gl::TRUE, gl::TRUE, gl::TRUE, gl::TRUE);
            gl::ClearColor(red, green, blue, alpha);
            gl::Clear(gl::COLOR_BUFFER_BIT);
        }
    }

    pub(crate) fn clear_depth(&mut self) {
        self.flush();
        // SAFETY: as above. The depth mask also gates clears.
        unsafe {
            self.bind();
            gl::DepthMask(gl::TRUE);
            gl::ClearDepth(1.0);
            gl::Clear(gl::DEPTH_BUFFER_BIT);
        }
    }

    /// The size of the drawn surface and of [`Self::read_rgba`]'s image.
    pub(crate) fn target(&self) -> [u32; 2] {
        self.target
    }

    /// Replace every colour pixel with top-down native-sized RGBA, keeping
    /// depth, as the CPU frame's surface replacement does. A larger target
    /// shows it enlarged with nearest sampling.
    pub(crate) fn replace_surface(&mut self, rgba: &[u8]) {
        debug_assert_eq!(
            rgba.len(),
            self.native[0] as usize * self.native[1] as usize * 4
        );
        self.flush();
        let [width, height] = self.native.map(|value| value as GLsizei);
        // SAFETY: the device context is current; `rgba` holds exactly one
        // frame of byte-packed pixels.
        unsafe {
            if self.surface == 0 {
                gl::GenTextures(1, &mut self.surface);
                gl::BindTexture(gl::TEXTURE_2D, self.surface);
                set_sampling(gl::NEAREST, gl::CLAMP_TO_EDGE);
                gl::TexImage2D(
                    gl::TEXTURE_2D,
                    0,
                    gl::RGBA8 as GLint,
                    width,
                    height,
                    0,
                    gl::RGBA,
                    gl::UNSIGNED_BYTE,
                    std::ptr::null(),
                );
            }
            gl::BindTexture(gl::TEXTURE_2D, self.surface);
            gl::PixelStorei(gl::UNPACK_ALIGNMENT, 1);
            gl::TexSubImage2D(
                gl::TEXTURE_2D,
                0,
                0,
                0,
                width,
                height,
                gl::RGBA,
                gl::UNSIGNED_BYTE,
                rgba.as_ptr().cast(),
            );
            self.bind();
            // Texture row 0 and frame row 0 are both the surface's top row.
            self.device.blit(self.surface, false, false);
        }
    }

    /// The frame's pixels at the target size, top row first, as the CPU
    /// frame stores its native ones.
    pub(crate) fn read_rgba(&mut self) -> Vec<u8> {
        self.flush();
        let mut pixels = vec![0; self.target[0] as usize * self.target[1] as usize * 4];
        // SAFETY: the device context is current; the buffer holds exactly
        // one frame of byte-packed RGBA.
        unsafe {
            self.bind();
            gl::PixelStorei(gl::PACK_ALIGNMENT, 1);
            gl::ReadPixels(
                0,
                0,
                self.target[0] as GLsizei,
                self.target[1] as GLsizei,
                gl::RGBA,
                gl::UNSIGNED_BYTE,
                pixels.as_mut_ptr().cast(),
            );
        }
        pixels
    }

    /// Copy the completed frame into `destination` of the window's back
    /// buffer, with black bars and nearest sampling, ready for the swap. A
    /// target of the destination's size copies one to one.
    pub(crate) fn present(&mut self, output: [u32; 2], destination: PresentationRect) {
        self.flush();
        // SAFETY: the device context is current on the window being drawn.
        unsafe {
            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
            gl::Disable(gl::SCISSOR_TEST);
            gl::Viewport(0, 0, output[0] as GLsizei, output[1] as GLsizei);
            gl::ColorMask(gl::TRUE, gl::TRUE, gl::TRUE, gl::TRUE);
            gl::ClearColor(0.0, 0.0, 0.0, 1.0);
            gl::Clear(gl::COLOR_BUFFER_BIT);
            // GL windows count rows from the bottom.
            let y = output[1] as i32 - destination.y - destination.height as i32;
            gl::Viewport(
                destination.x,
                y,
                destination.width as GLsizei,
                destination.height as GLsizei,
            );
            self.device.blit(self.color, true, true);
        }
        self.device.retire_idle_textures();
    }

    /// # Safety
    /// The device context must be current.
    unsafe fn bind(&self) {
        gl::BindFramebuffer(gl::FRAMEBUFFER, self.framebuffer);
        gl::Viewport(0, 0, self.target[0] as GLsizei, self.target[1] as GLsizei);
        gl::Disable(gl::SCISSOR_TEST);
        gl::Disable(gl::CULL_FACE);
    }

    fn flush(&mut self) {
        if self.batch.is_empty() {
            return;
        }
        let key = self.key.expect("pending vertices have a device state");
        let device = &*self.device;
        let program = if key.texture.is_some() {
            &device.textured
        } else {
            &device.flat
        };
        let stride = std::mem::size_of::<DeviceVertex>() as GLsizei;
        // SAFETY: the device context is current; the vertex buffer upload
        // copies the batch before it is cleared.
        unsafe {
            self.bind();
            match blend_factors(key.blend) {
                None => gl::Disable(gl::BLEND),
                Some((source, destination)) => {
                    gl::Enable(gl::BLEND);
                    gl::BlendFunc(source, destination);
                }
            }
            if key.depth_test {
                gl::Enable(gl::DEPTH_TEST);
                gl::DepthFunc(gl::LEQUAL);
            } else {
                gl::Disable(gl::DEPTH_TEST);
            }
            gl::DepthMask(if key.depth_test && key.depth_write {
                gl::TRUE
            } else {
                gl::FALSE
            });
            gl::UseProgram(program.id);
            gl::Uniform2f(
                program.scale,
                2.0 / self.native[0] as f32,
                2.0 / self.native[1] as f32,
            );
            gl::Uniform1f(program.depth_test, if key.depth_test { 1.0 } else { 0.0 });
            if let Some(binding) = key.texture {
                gl::ActiveTexture(gl::TEXTURE0);
                gl::BindTexture(gl::TEXTURE_2D, binding.name);
                set_sampling(binding.filter, binding.address);
                gl::Uniform1i(program.texture, 0);
            }
            gl::BindBuffer(gl::ARRAY_BUFFER, device.vertices);
            gl::BufferData(
                gl::ARRAY_BUFFER,
                std::mem::size_of_val(self.batch.as_slice()) as GLsizeiptr,
                self.batch.as_ptr().cast(),
                gl::STREAM_DRAW,
            );
            // The pointers name the buffer, not its storage: set once
            // until the blit replaces them.
            if !device.batch_layout.replace(true) {
                gl::VertexAttribPointer(
                    POSITION,
                    3,
                    gl::FLOAT,
                    gl::FALSE,
                    stride,
                    std::ptr::null(),
                );
                gl::VertexAttribPointer(
                    TEXTURE,
                    3,
                    gl::FLOAT,
                    gl::FALSE,
                    stride,
                    12 as *const c_void,
                );
                gl::VertexAttribPointer(
                    DIFFUSE,
                    4,
                    gl::UNSIGNED_BYTE,
                    gl::TRUE,
                    stride,
                    24 as *const c_void,
                );
                gl::VertexAttribPointer(
                    FOG,
                    4,
                    gl::UNSIGNED_BYTE,
                    gl::TRUE,
                    stride,
                    28 as *const c_void,
                );
                for attribute in [POSITION, TEXTURE, DIFFUSE, FOG] {
                    gl::EnableVertexAttribArray(attribute);
                }
            }
            gl::DrawArrays(gl::TRIANGLES, 0, self.batch.len() as GLsizei);
        }
        self.batch.clear();
    }
}

impl Drop for GlFrame {
    fn drop(&mut self) {
        // SAFETY: `device` keeps the context alive; it is current on this
        // thread. Zero names are ignored by GL.
        unsafe {
            gl::DeleteFramebuffers(1, &self.framebuffer);
            gl::DeleteRenderbuffers(1, &self.depth);
            gl::DeleteTextures(1, &self.color);
            gl::DeleteTextures(1, &self.surface);
        }
    }
}

/// Source and destination factors; `None` is the CPU raster's copy.
fn blend_factors(blend: D3dTextureBlend) -> Option<(GLenum, GLenum)> {
    match blend {
        D3dTextureBlend::Copy => None,
        D3dTextureBlend::Additive => Some((gl::ONE, gl::ONE)),
        D3dTextureBlend::InverseSourceColor => Some((gl::ZERO, gl::ONE_MINUS_SRC_COLOR)),
        D3dTextureBlend::SourceAlpha => Some((gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA)),
    }
}

/// The shared consumer admits only point (1) and linear (2) filters, with
/// equal MIN and MAG, before a fan reaches either device.
fn filter(sampling: D3dTextureSampling) -> GLenum {
    match sampling.min_filter {
        2 => gl::LINEAR,
        _ => gl::NEAREST,
    }
}

/// Wrap (1) or clamp (3), as admitted by the shared consumer.
fn address(sampling: D3dTextureSampling) -> GLenum {
    match sampling.address {
        1 => gl::REPEAT,
        _ => gl::CLAMP_TO_EDGE,
    }
}

/// # Safety
/// A context must be current with a texture bound to `TEXTURE_2D`.
unsafe fn set_sampling(filter: GLenum, address: GLenum) {
    gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, filter as GLint);
    gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, filter as GLint);
    gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, address as GLint);
    gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, address as GLint);
}

fn check(operation: &'static str) -> Result<(), GlDeviceError> {
    // SAFETY: GetError has no preconditions beyond a current context.
    let code = unsafe { gl::GetError() };
    if code == gl::NO_ERROR {
        Ok(())
    } else {
        Err(GlDeviceError::Call { code, operation })
    }
}

/// # Safety
/// A context must be current.
unsafe fn gl_string(name: GLenum) -> String {
    let value = gl::GetString(name);
    if value.is_null() {
        return "unknown".into();
    }
    CStr::from_ptr(value.cast()).to_string_lossy().into_owned()
}

/// # Safety
/// A context must be current.
unsafe fn uniform(program: GLuint, name: &str) -> GLint {
    let name = CString::new(name).expect("uniform names have no NUL");
    gl::GetUniformLocation(program, name.as_ptr())
}

/// # Safety
/// A context must be current.
unsafe fn compile(
    stage: GLenum,
    label: &'static str,
    source: &str,
) -> Result<GLuint, GlDeviceError> {
    let shader = gl::CreateShader(stage);
    let source = CString::new(source).expect("shader sources have no NUL");
    gl::ShaderSource(shader, 1, &source.as_ptr(), std::ptr::null());
    gl::CompileShader(shader);
    let mut status = 0;
    gl::GetShaderiv(shader, gl::COMPILE_STATUS, &mut status);
    if status == 0 {
        let mut length = 0;
        gl::GetShaderiv(shader, gl::INFO_LOG_LENGTH, &mut length);
        let log = info_log(length, |capacity, written, buffer| {
            gl::GetShaderInfoLog(shader, capacity, written, buffer)
        });
        gl::DeleteShader(shader);
        return Err(GlDeviceError::Shader { stage: label, log });
    }
    Ok(shader)
}

/// # Safety
/// A context must be current.
unsafe fn link(
    vertex: &str,
    fragment: &str,
    attributes: &[(GLuint, &str)],
) -> Result<GLuint, GlDeviceError> {
    let vertex = compile(gl::VERTEX_SHADER, "vertex", vertex)?;
    let fragment = match compile(gl::FRAGMENT_SHADER, "fragment", fragment) {
        Ok(fragment) => fragment,
        Err(error) => {
            gl::DeleteShader(vertex);
            return Err(error);
        }
    };
    let program = gl::CreateProgram();
    gl::AttachShader(program, vertex);
    gl::AttachShader(program, fragment);
    for (index, name) in attributes {
        let name = CString::new(*name).expect("attribute names have no NUL");
        gl::BindAttribLocation(program, *index, name.as_ptr());
    }
    gl::LinkProgram(program);
    // Attached shaders are released with their program.
    gl::DeleteShader(vertex);
    gl::DeleteShader(fragment);
    let mut status = 0;
    gl::GetProgramiv(program, gl::LINK_STATUS, &mut status);
    if status == 0 {
        let mut length = 0;
        gl::GetProgramiv(program, gl::INFO_LOG_LENGTH, &mut length);
        let log = info_log(length, |capacity, written, buffer| {
            gl::GetProgramInfoLog(program, capacity, written, buffer)
        });
        gl::DeleteProgram(program);
        return Err(GlDeviceError::Link(log));
    }
    Ok(program)
}

fn info_log(length: GLint, read: impl FnOnce(GLsizei, *mut GLsizei, *mut GLchar)) -> String {
    let mut buffer = vec![0_u8; length.max(1) as usize];
    let mut written = 0;
    read(
        buffer.len() as GLsizei,
        &mut written,
        buffer.as_mut_ptr().cast(),
    );
    buffer.truncate(written.max(0) as usize);
    String::from_utf8_lossy(&buffer).trim().to_owned()
}

#[cfg(test)]
#[path = "gl_device_tests.rs"]
mod tests;
