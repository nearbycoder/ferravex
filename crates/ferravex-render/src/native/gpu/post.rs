//! Native post-processing targets and passes. Mirrors `web/post.js` with the
//! shared GLSL in `crate::post`: offscreen world target (MSAA-resolved when the
//! window is multisampled), dual-filter bloom chain and graded composite.
use super::super::gl::*;
use super::{create_program_stages, shader_header};
use crate::post::{MAX_BLOOM_LEVELS, POST_FRAME_FLOATS};
use std::ffi::{CStr, CString, c_int, c_uint};

const FRAMEBUFFER: c_uint = 0x8d40;
const READ_FRAMEBUFFER: c_uint = 0x8ca8;
const DRAW_FRAMEBUFFER: c_uint = 0x8ca9;
const RENDERBUFFER: c_uint = 0x8d41;
const COLOR_ATTACHMENT0: c_uint = 0x8ce0;
const DEPTH_ATTACHMENT: c_uint = 0x8d00;
const FRAMEBUFFER_COMPLETE: c_uint = 0x8cd5;
const TEXTURE_2D: c_uint = 0x0de1;
const TEXTURE0: c_uint = 0x84c0;
const COLOR_BUFFER_BIT: c_uint = 0x4000;
const DEPTH_BUFFER_BIT: c_uint = 0x0100;
const NEAREST: c_uint = 0x2600;
const LINEAR: c_int = 0x2601;
const CLAMP_TO_EDGE: c_int = 0x812f;
const RGBA: c_uint = 0x1908;
const RGBA8: c_uint = 0x8058;
const RGBA16F: c_uint = 0x881a;
const HALF_FLOAT: c_uint = 0x140b;
const UNSIGNED_BYTE: c_uint = 0x1401;
const DEPTH_COMPONENT24: c_uint = 0x81a6;
const SAMPLES: c_uint = 0x80a9;
const MAX_SAMPLES: c_uint = 0x8d57;
const BLEND: c_uint = 0x0be2;
const DEPTH_TEST: c_uint = 0x0b71;
const TRIANGLES: c_uint = 0x0004;

struct Program {
    handle: c_uint,
    source: c_int,
    bloom: c_int,
    texel: c_int,
    settings: c_int,
}
impl Program {
    fn new(fragment: &str) -> Result<Self, String> {
        let vertex = CString::new(format!(
            "{}{}",
            shader_header(),
            crate::post::POST_VERTEX_BODY
        ))
        .unwrap();
        let fragment = CString::new(format!("{}{}", shader_header(), fragment)).unwrap();
        let handle = create_program_stages(&vertex, &fragment, &[])?;
        let location = |name: &CStr| unsafe { glGetUniformLocation(handle, name.as_ptr()) };
        let source = match location(c"u_source") {
            -1 => location(c"u_scene"),
            found => found,
        };
        Ok(Self {
            handle,
            source,
            // Threshold vec4 in the downsample pass; bloom sampler in the composite.
            bloom: location(c"u_bloom"),
            texel: location(c"u_texel"),
            settings: location(c"u_post"),
        })
    }
}
#[derive(Clone, Copy, Default)]
struct Level {
    framebuffer: c_uint,
    texture: c_uint,
    width: i32,
    height: i32,
}
/// GPU resources for one context. Targets are reallocated only when the
/// framebuffer size changes; frames allocate nothing.
pub(super) struct PostGpu {
    vertex_array: c_uint,
    down: Program,
    up: Program,
    composite: Program,
    samples: i32,
    float: bool,
    width: i32,
    height: i32,
    scene_framebuffer: c_uint,
    color_buffer: c_uint,
    depth_buffer: c_uint,
    resolve_framebuffer: c_uint,
    scene_texture: c_uint,
    levels: [Level; MAX_BLOOM_LEVELS],
}
impl PostGpu {
    /// Compile the passes and read the default framebuffer's sample count.
    /// Call with the owning context current and the default framebuffer bound.
    pub(super) fn new() -> Result<Self, String> {
        let down = Program::new(crate::post::POST_DOWN_FRAGMENT_BODY)?;
        let up = Program::new(crate::post::POST_UP_FRAGMENT_BODY);
        let composite = Program::new(crate::post::POST_COMPOSITE_FRAGMENT_BODY);
        let (up, composite) = match (up, composite) {
            (Ok(up), Ok(composite)) => (up, composite),
            (up, composite) => {
                // SAFETY: Programs compiled above belong to the current context.
                unsafe {
                    glDeleteProgram(down.handle);
                    if let Ok(p) = &up {
                        glDeleteProgram(p.handle);
                    }
                    if let Ok(p) = &composite {
                        glDeleteProgram(p.handle);
                    }
                }
                return Err(up.err().or(composite.err()).unwrap_or_default());
            }
        };
        let mut vertex_array = 0;
        let (mut samples, mut max_samples) = (0, 0);
        // SAFETY: Current context; integer queries write one value each.
        unsafe {
            glGenVertexArrays(1, &mut vertex_array);
            glGetIntegerv(SAMPLES, &mut samples);
            glGetIntegerv(MAX_SAMPLES, &mut max_samples);
        }
        let gpu = Self {
            vertex_array,
            down,
            up,
            composite,
            samples: samples.min(max_samples).clamp(0, 4),
            float: !cfg!(target_os = "ios"),
            width: 0,
            height: 0,
            scene_framebuffer: 0,
            color_buffer: 0,
            depth_buffer: 0,
            resolve_framebuffer: 0,
            scene_texture: 0,
            levels: [Level::default(); MAX_BLOOM_LEVELS],
        };
        if vertex_array == 0 {
            // SAFETY: Releases the programs above.
            unsafe { gpu.delete() };
            return Err("post vertex array allocation failed".into());
        }
        Ok(gpu)
    }
    pub(super) fn describe(&self) -> (i32, bool) {
        (self.samples, self.float)
    }
    /// Ensure targets for `width`×`height`, downgrading MSAA then half-float
    /// when a configuration is incomplete. Unchanged sizes cost nothing.
    pub(super) fn prepare(&mut self, width: u32, height: u32) -> Result<(), String> {
        let (w, h) = (width.clamp(1, 16384) as i32, height.clamp(1, 16384) as i32);
        if (w, h) == (self.width, self.height) && self.scene_framebuffer != 0 {
            return Ok(());
        }
        // SAFETY: Every handle is owned by the current context.
        unsafe {
            self.delete_targets();
            loop {
                match self.allocate(w, h) {
                    Ok(()) => break,
                    Err(error) => {
                        self.delete_targets();
                        if self.samples > 0 {
                            self.samples = 0;
                        } else if self.float {
                            self.float = false;
                        } else {
                            glBindFramebuffer(
                                FRAMEBUFFER,
                                super::super::host::default_framebuffer(),
                            );
                            return Err(error);
                        }
                    }
                }
            }
            glBindFramebuffer(FRAMEBUFFER, super::super::host::default_framebuffer());
        }
        (self.width, self.height) = (w, h);
        Ok(())
    }
    unsafe fn texture(&self, w: i32, h: i32) -> c_uint {
        let mut texture = 0;
        // SAFETY: Current context; a null pixel pointer only allocates storage.
        unsafe {
            glGenTextures(1, &mut texture);
            glBindTexture(TEXTURE_2D, texture);
            glTexParameteri(TEXTURE_2D, 0x2801, LINEAR);
            glTexParameteri(TEXTURE_2D, 0x2800, LINEAR);
            glTexParameteri(TEXTURE_2D, 0x2802, CLAMP_TO_EDGE);
            glTexParameteri(TEXTURE_2D, 0x2803, CLAMP_TO_EDGE);
            let (internal, kind) = if self.float {
                (RGBA16F, HALF_FLOAT)
            } else {
                (RGBA8, UNSIGNED_BYTE)
            };
            glTexImage2D(
                TEXTURE_2D,
                0,
                internal as c_int,
                w,
                h,
                0,
                RGBA,
                kind,
                std::ptr::null(),
            );
        }
        texture
    }
    unsafe fn complete(name: &str) -> Result<(), String> {
        // SAFETY: Queries the currently bound framebuffer.
        let status = unsafe { glCheckFramebufferStatus(FRAMEBUFFER) };
        if status == FRAMEBUFFER_COMPLETE {
            Ok(())
        } else {
            Err(format!("post {name} framebuffer incomplete: 0x{status:x}"))
        }
    }
    unsafe fn allocate(&mut self, w: i32, h: i32) -> Result<(), String> {
        // SAFETY: Current context; all generated names are stored for deletion.
        unsafe {
            let format = if self.float { RGBA16F } else { RGBA8 };
            self.scene_texture = self.texture(w, h);
            glGenFramebuffers(1, &mut self.scene_framebuffer);
            glGenRenderbuffers(1, &mut self.depth_buffer);
            glBindFramebuffer(FRAMEBUFFER, self.scene_framebuffer);
            glBindRenderbuffer(RENDERBUFFER, self.depth_buffer);
            if self.samples > 0 {
                glRenderbufferStorageMultisample(
                    RENDERBUFFER,
                    self.samples,
                    DEPTH_COMPONENT24,
                    w,
                    h,
                );
                glGenRenderbuffers(1, &mut self.color_buffer);
                glBindRenderbuffer(RENDERBUFFER, self.color_buffer);
                glRenderbufferStorageMultisample(RENDERBUFFER, self.samples, format, w, h);
                glFramebufferRenderbuffer(
                    FRAMEBUFFER,
                    COLOR_ATTACHMENT0,
                    RENDERBUFFER,
                    self.color_buffer,
                );
            } else {
                glRenderbufferStorage(RENDERBUFFER, DEPTH_COMPONENT24, w, h);
                glFramebufferTexture2D(
                    FRAMEBUFFER,
                    COLOR_ATTACHMENT0,
                    TEXTURE_2D,
                    self.scene_texture,
                    0,
                );
            }
            glFramebufferRenderbuffer(
                FRAMEBUFFER,
                DEPTH_ATTACHMENT,
                RENDERBUFFER,
                self.depth_buffer,
            );
            glDrawBuffer(COLOR_ATTACHMENT0);
            Self::complete("scene")?;
            if self.samples > 0 {
                glGenFramebuffers(1, &mut self.resolve_framebuffer);
                glBindFramebuffer(FRAMEBUFFER, self.resolve_framebuffer);
                glFramebufferTexture2D(
                    FRAMEBUFFER,
                    COLOR_ATTACHMENT0,
                    TEXTURE_2D,
                    self.scene_texture,
                    0,
                );
                glDrawBuffer(COLOR_ATTACHMENT0);
                Self::complete("resolve")?;
            }
            for i in 0..MAX_BLOOM_LEVELS {
                let level = Level {
                    width: (w >> (i + 1)).max(1),
                    height: (h >> (i + 1)).max(1),
                    texture: 0,
                    framebuffer: 0,
                };
                self.levels[i] = level;
                self.levels[i].texture = self.texture(level.width, level.height);
                glGenFramebuffers(1, &mut self.levels[i].framebuffer);
                glBindFramebuffer(FRAMEBUFFER, self.levels[i].framebuffer);
                glFramebufferTexture2D(
                    FRAMEBUFFER,
                    COLOR_ATTACHMENT0,
                    TEXTURE_2D,
                    self.levels[i].texture,
                    0,
                );
                glDrawBuffer(COLOR_ATTACHMENT0);
                Self::complete("bloom")?;
            }
            Ok(())
        }
    }
    /// Bind the offscreen world target for this frame's clear and draws.
    pub(super) unsafe fn bind_scene(&self) {
        // SAFETY: Targets were prepared for the current size.
        unsafe {
            glBindFramebuffer(FRAMEBUFFER, self.scene_framebuffer);
            glDrawBuffer(COLOR_ATTACHMENT0);
            glViewport(0, 0, self.width, self.height);
        }
    }
    /// Resolve, bloom and composite into `screen`, then clear its depth so
    /// following HUD commands draw on top. Returns the number of passes drawn.
    pub(super) unsafe fn finish(
        &self,
        settings: &[f32; POST_FRAME_FLOATS],
        screen: c_uint,
        screen_buffer: c_uint,
        owner_vertex_array: c_uint,
    ) -> u32 {
        let levels = (settings[1].max(0.) as usize).min(MAX_BLOOM_LEVELS);
        let mut passes = 0;
        // SAFETY: Current context with prepared targets; uniforms read stack arrays.
        unsafe {
            if self.resolve_framebuffer != 0 {
                glBindFramebuffer(READ_FRAMEBUFFER, self.scene_framebuffer);
                glBindFramebuffer(DRAW_FRAMEBUFFER, self.resolve_framebuffer);
                glBlitFramebuffer(
                    0,
                    0,
                    self.width,
                    self.height,
                    0,
                    0,
                    self.width,
                    self.height,
                    COLOR_BUFFER_BIT,
                    NEAREST,
                );
            }
            glBindVertexArray(self.vertex_array);
            glDisable(DEPTH_TEST);
            glDepthMask(0);
            glDisable(BLEND);
            glActiveTexture(TEXTURE0);
            if levels > 0 {
                glUseProgram(self.down.handle);
                glUniform1i(self.down.source, 0);
                glUniform4fv(
                    self.down.bloom,
                    1,
                    [settings[5], settings[6], 0., 0.].as_ptr(),
                );
                for i in 0..levels {
                    let (source, sw, sh) = if i == 0 {
                        (self.scene_texture, self.width, self.height)
                    } else {
                        let l = self.levels[i - 1];
                        (l.texture, l.width, l.height)
                    };
                    let target = self.levels[i];
                    glBindFramebuffer(FRAMEBUFFER, target.framebuffer);
                    glViewport(0, 0, target.width, target.height);
                    glBindTexture(TEXTURE_2D, source);
                    let prefilter = if i == 0 { 1. } else { 0. };
                    glUniform4fv(
                        self.down.texel,
                        1,
                        [1. / sw as f32, 1. / sh as f32, prefilter, 0.].as_ptr(),
                    );
                    glDrawArrays(TRIANGLES, 0, 3);
                    passes += 1;
                }
                glUseProgram(self.up.handle);
                glUniform1i(self.up.source, 0);
                glEnable(BLEND);
                glBlendFuncSeparate(1, 1, 1, 1);
                for i in (0..levels - 1).rev() {
                    let source = self.levels[i + 1];
                    let target = self.levels[i];
                    glBindFramebuffer(FRAMEBUFFER, target.framebuffer);
                    glViewport(0, 0, target.width, target.height);
                    glBindTexture(TEXTURE_2D, source.texture);
                    glUniform4fv(
                        self.up.texel,
                        1,
                        [
                            1. / source.width as f32,
                            1. / source.height as f32,
                            settings[7],
                            0.,
                        ]
                        .as_ptr(),
                    );
                    glDrawArrays(TRIANGLES, 0, 3);
                    passes += 1;
                }
                glDisable(BLEND);
            }
            glBindFramebuffer(FRAMEBUFFER, screen);
            glDrawBuffer(screen_buffer);
            glViewport(0, 0, self.width, self.height);
            glUseProgram(self.composite.handle);
            glUniform1i(self.composite.source, 0);
            glUniform1i(self.composite.bloom, 1);
            let bloom = self.levels[0];
            glUniform4fv(
                self.composite.texel,
                1,
                [
                    1. / bloom.width as f32,
                    1. / bloom.height as f32,
                    1. / self.width as f32,
                    1. / self.height as f32,
                ]
                .as_ptr(),
            );
            glUniform4fv(self.composite.settings, 7, settings.as_ptr());
            glActiveTexture(TEXTURE0 + 1);
            glBindTexture(TEXTURE_2D, bloom.texture);
            glActiveTexture(TEXTURE0);
            glBindTexture(TEXTURE_2D, self.scene_texture);
            glDrawArrays(TRIANGLES, 0, 3);
            passes += 1;
            glEnable(BLEND);
            glBlendFuncSeparate(0x0302, 0x0303, 1, 0x0303);
            glDepthMask(1);
            glClear(DEPTH_BUFFER_BIT);
            glUseProgram(0);
            glBindVertexArray(owner_vertex_array);
        }
        passes
    }
    unsafe fn delete_targets(&mut self) {
        // SAFETY: Names are owned by this context; zero names are skipped.
        unsafe {
            for f in [self.scene_framebuffer, self.resolve_framebuffer] {
                if f != 0 {
                    glDeleteFramebuffers(1, &f);
                }
            }
            for r in [self.color_buffer, self.depth_buffer] {
                if r != 0 {
                    glDeleteRenderbuffers(1, &r);
                }
            }
            if self.scene_texture != 0 {
                glDeleteTextures(1, &self.scene_texture);
            }
            for level in &self.levels {
                if level.framebuffer != 0 {
                    glDeleteFramebuffers(1, &level.framebuffer);
                }
                if level.texture != 0 {
                    glDeleteTextures(1, &level.texture);
                }
            }
        }
        self.scene_framebuffer = 0;
        self.resolve_framebuffer = 0;
        self.color_buffer = 0;
        self.depth_buffer = 0;
        self.scene_texture = 0;
        self.levels = [Level::default(); MAX_BLOOM_LEVELS];
        (self.width, self.height) = (0, 0);
    }
    /// Release every GL object. The context must be current.
    pub(super) unsafe fn delete(mut self) {
        // SAFETY: Called once with the owning context current.
        unsafe {
            self.delete_targets();
            for p in [&self.down, &self.up, &self.composite] {
                glDeleteProgram(p.handle);
            }
            if self.vertex_array != 0 {
                glDeleteVertexArrays(1, &self.vertex_array);
            }
        }
    }
}
