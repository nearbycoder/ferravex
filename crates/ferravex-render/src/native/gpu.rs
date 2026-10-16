//! Shared native GL submission. Platform hosts only own the drawable and OS events.
use super::gl::*;
use super::{Batch, Color, Conf, Event, host::Host};
use std::{
    collections::BTreeMap,
    ffi::{CStr, CString, c_int, c_uint, c_void},
    ptr,
};
const GL_ARRAY_BUFFER: c_uint = 0x8892;
const GL_STREAM_DRAW: c_uint = 0x88e0;
const GL_FLOAT: c_uint = 0x1406;
const GL_TRIANGLES: c_uint = 0x0004;
const GL_DEPTH_TEST: c_uint = 0x0b71;
const GL_BLEND: c_uint = 0x0be2;
const GL_CULL_FACE: c_uint = 0x0b44;
const GL_SCISSOR_TEST: c_uint = 0x0c11;
const GL_LEQUAL: c_uint = 0x0203;
const GL_SRC_ALPHA: c_uint = 0x0302;
const GL_ONE_MINUS_SRC_ALPHA: c_uint = 0x0303;
const GL_COLOR_BUFFER_BIT: c_uint = 0x4000;
const GL_DEPTH_BUFFER_BIT: c_uint = 0x0100;
const GL_VERTEX_SHADER: c_uint = 0x8b31;
const GL_FRAGMENT_SHADER: c_uint = 0x8b30;
const GL_COMPILE_STATUS: c_uint = 0x8b81;
const GL_LINK_STATUS: c_uint = 0x8b82;
const GL_INFO_LOG_LENGTH: c_uint = 0x8b84;
const MAX_READBACK_PIXELS: usize = 16_777_216;

/// The display, window and GL context must stay on their creating thread.
/// Raw-pointer ownership prevents automatic Send/Sync implementations.
struct GpuTimer {
    queries: [u32; 32],
    free: Vec<u32>,
    pending: std::collections::VecDeque<(u32, u64)>,
    completed: Vec<(u64, f64)>,
}
impl GpuTimer {
    fn new() -> Option<Self> {
        if !super::gl::timer_available() {
            return None;
        }
        // SAFETY: The caller owns the current context. Core 3.3 includes timer queries.
        unsafe {
            let p = glGetString(0x1f02);
            if p.is_null() {
                return None;
            }
            let v = CStr::from_ptr(p.cast()).to_string_lossy();
            let mut n = v.split(['.', ' ']);
            let major = n.next()?.parse::<u32>().ok()?;
            let minor = n.next()?.parse::<u32>().ok()?;
            if (major, minor) < (3, 3) {
                return None;
            }
            let mut queries = [0; 32];
            glGenQueries(32, queries.as_mut_ptr());
            if queries.contains(&0) {
                glDeleteQueries(32, queries.as_ptr());
                return None;
            }
            Some(Self {
                free: queries.to_vec(),
                queries,
                pending: std::collections::VecDeque::with_capacity(32),
                completed: Vec::with_capacity(32),
            })
        }
    }
    fn poll(&mut self) {
        // SAFETY: Queries are uniquely owned; an unavailable result is never read or waited on.
        unsafe {
            while let Some(&(query, frame)) = self.pending.front() {
                let mut ready = 0;
                glGetQueryObjectiv(query, 0x8867, &mut ready);
                if ready == 0 {
                    break;
                }
                let mut nanoseconds = 0;
                glGetQueryObjectui64v(query, 0x8866, &mut nanoseconds);
                self.pending.pop_front();
                self.free.push(query);
                if self.completed.len() < 32 {
                    self.completed.push((frame, nanoseconds as f64 / 1000.));
                }
            }
        }
    }
    fn begin(&mut self) -> Option<u32> {
        self.poll();
        let query = self.free.pop()?;
        unsafe {
            glBeginQuery(0x88bf, query);
        }
        Some(query)
    }
    fn end(&mut self, query: u32, frame: u64) {
        unsafe {
            glEndQuery(0x88bf);
        }
        self.pending.push_back((query, frame));
    }
}
struct GpuProgram {
    handle: c_uint,
    camera: c_int,
    light: c_int,
    params: c_int,
    points: c_int,
    point_count: c_int,
    shadow_matrix: c_int,
    shadow_texture: c_int,
    shadow_options: c_int,
    shadow_extra: c_int,
    skin_count: c_int,
    bones: c_int,
    bone_normals: c_int,
}
struct ProgramPair {
    color: GpuProgram,
    depth: GpuProgram,
}
struct ShadowTarget {
    framebuffer: c_uint,
    texture: c_uint,
    size: u32,
}
struct MaterialProgram {
    revision: u32,
    valid: Option<ProgramPair>,
}
struct RetainedGpu {
    builtin: ProgramPair,
    environment: crate::lighting::Environment,
    white_texture: c_uint,
    shadow: Option<ShadowTarget>,
    shadow_failed_size: u32,
    materials: BTreeMap<u32, MaterialProgram>,
    instances: c_uint,
    meshes: BTreeMap<u32, (c_uint, c_int)>,
    skin_matrices: Vec<[f32; 16]>,
    skin_normals: Vec<[f32; 9]>,
    batches: Vec<crate::retained::RetainedBatch>,
}
struct SpriteGpu {
    program: c_uint,
    geometry: c_uint,
    instances: c_uint,
    batches: Vec<crate::effects::SpriteBatch>,
}
impl SpriteGpu {
    fn new() -> Result<Self, String> {
        let vertex = CString::new(format!(
            "{}{}",
            shader_header(),
            crate::effects::SPRITE_VERTEX_BODY
                .replace("attribute ", "in ")
                .replace("varying ", "out ")
        ))
        .unwrap();
        let fragment = CString::new(format!(
            "{}out vec4 ferravex_pixel;\n{}",
            shader_header(),
            crate::effects::SPRITE_FRAGMENT_BODY
                .replace("varying ", "in ")
                .replace("gl_FragColor", "ferravex_pixel")
        ))
        .unwrap();
        let program = create_program_stages(
            &vertex,
            &fragment,
            &[
                (0, c"a_corner"),
                (1, c"a_center"),
                (2, c"a_right"),
                (3, c"a_up"),
                (4, c"a_tint"),
            ],
        )?;
        let mut geometry = 0;
        let mut instances = 0;
        // SAFETY: The caller owns the current context; these buffers persist
        // with it and copy the complete fixed corner array immediately.
        unsafe {
            glGenBuffers(1, &mut geometry);
            glGenBuffers(1, &mut instances);
            if geometry == 0 || instances == 0 {
                if geometry != 0 {
                    glDeleteBuffers(1, &geometry);
                }
                if instances != 0 {
                    glDeleteBuffers(1, &instances);
                }
                glDeleteProgram(program);
                return Err("allocate sprite buffers".into());
            }
            let corners: [f32; 18] = [
                -1., -1., -1., 1., 1., -1., 1., -1., -1., 1., 1., 1., 0., 1., -1., -1., 1., -1.,
            ];
            glBindBuffer(GL_ARRAY_BUFFER, geometry);
            glBufferData(GL_ARRAY_BUFFER, 72, corners.as_ptr().cast(), 0x88e4);
        }
        Ok(Self {
            program,
            geometry,
            instances,
            batches: Vec::new(),
        })
    }
    unsafe fn draw(&self, index: usize) {
        let Some(batch) = self.batches.get(index).filter(|b| b.count > 0) else {
            return;
        };
        // SAFETY: Upload validates every instance range/shape; byte offsets
        // address the owned fixed geometry and current frame instance VBO.
        unsafe {
            glUseProgram(self.program);
            set_depth(batch.flags & 1 != 0);
            glDepthMask((batch.flags & 1 != 0 && batch.flags & 2 == 0) as u8);
            glEnable(GL_BLEND);
            glBlendFuncSeparate(
                GL_SRC_ALPHA,
                if batch.flags & 4 != 0 {
                    1
                } else {
                    GL_ONE_MINUS_SRC_ALPHA
                },
                1,
                GL_ONE_MINUS_SRC_ALPHA,
            );
            glBindBuffer(GL_ARRAY_BUFFER, self.geometry);
            glEnableVertexAttribArray(0);
            glVertexAttribPointer(0, 2, GL_FLOAT, 0, 8, ptr::null());
            glBindBuffer(GL_ARRAY_BUFFER, self.instances);
            for i in 0..4 {
                let attribute = i + 1;
                glEnableVertexAttribArray(attribute);
                glVertexAttribPointer(
                    attribute,
                    4,
                    GL_FLOAT,
                    0,
                    64,
                    (batch.first as usize * 64 + i as usize * 16) as *const c_void,
                );
                glVertexAttribDivisor(attribute, 1);
            }
            glDrawArraysInstanced(
                GL_TRIANGLES,
                if batch.triangles == 1 { 6 } else { 0 },
                batch.triangles as i32 * 3,
                batch.count as i32,
            );
            for i in 0..5 {
                glDisableVertexAttribArray(i);
                if i > 0 {
                    glVertexAttribDivisor(i, 0);
                }
            }
        }
    }
    unsafe fn delete(&self) {
        unsafe {
            glDeleteBuffers(1, &self.geometry);
            glDeleteBuffers(1, &self.instances);
            glDeleteProgram(self.program);
        }
    }
}
pub struct NativeWindow {
    host: Host,
    conf: Conf,
    vertex_array: u32,
    program: c_uint,
    vertex_buffer: c_uint,
    retained: Option<RetainedGpu>,
    last_commands: Vec<crate::retained::Command>,
    mesh_upload_bytes: u64,
    gpu_timer: Option<GpuTimer>,
    present_count: u64,
    swap_interval: i32,
    width: u32,
    height: u32,
    last_clear: Color,
    last_batches: Vec<(i32, i32, bool)>,
    last_effects: Vec<crate::effects::EffectBatch>,
    sprites: Option<SpriteGpu>,
    sprite_uploaded_bytes: u64,
    presented: bool,
}
impl NativeWindow {
    pub fn new(conf: &Conf) -> Result<Self, String> {
        let host = Host::new(conf)?;
        #[cfg(not(target_os = "linux"))]
        super::gl::load()?;
        let (width, height) = host.size();
        let mut owner = Self {
            host,
            conf: conf.clone(),
            vertex_array: 0,
            width,
            height,
            program: 0,
            vertex_buffer: 0,
            retained: None,
            last_commands: Vec::new(),
            mesh_upload_bytes: 0,
            gpu_timer: None,
            present_count: 0,
            swap_interval: 1,
            last_clear: Color::new(0.0, 0.0, 0.0, 1.0),
            last_batches: Vec::new(),
            last_effects: Vec::new(),
            sprites: None,
            sprite_uploaded_bytes: 0,
            presented: false,
        };
        unsafe {
            glGenVertexArrays(1, &mut owner.vertex_array);
            if owner.vertex_array == 0 {
                return Err("OpenGL vertex array allocation failed".into());
            }
            glBindVertexArray(owner.vertex_array);
            owner.program = create_program()?;
            glGenBuffers(1, &mut owner.vertex_buffer);
            if owner.vertex_buffer == 0 {
                return Err("OpenGL vertex buffer allocation failed".into());
            }
            glEnable(GL_BLEND);
            glBlendFuncSeparate(
                GL_SRC_ALPHA,
                GL_ONE_MINUS_SRC_ALPHA,
                1,
                GL_ONE_MINUS_SRC_ALPHA,
            );
            glDisable(GL_CULL_FACE);
            glDisable(GL_SCISSOR_TEST);
            glDepthFunc(GL_LEQUAL);
            glClearDepth(1.);
        }
        Ok(owner)
    }
    pub fn size(&self) -> (u32, u32) {
        self.host.logical_size()
    }
    pub fn framebuffer_size(&self) -> (u32, u32) {
        (self.width, self.height)
    }
    pub fn dpi_scale(&self) -> f32 {
        self.host.dpi_scale()
    }
    /// Recreate the OS drawable and GPU caches. CPU mesh/material handles survive.
    /// Failure leaves the existing host in place.
    pub fn recreate(&mut self) -> Result<(), String> {
        let (w, h) = self.size();
        let mut conf = self.conf.clone();
        conf.window_width = w as i32;
        conf.window_height = h as i32;
        let timing = self.gpu_timer.is_some();
        self.finish_gpu_timing();
        let completed = self
            .gpu_timer
            .as_ref()
            .map(|t| t.completed.clone())
            .unwrap_or_default();
        let mut next = Self::new(&conf)?;
        next.present_count = self.present_count;
        next.set_swap_interval(self.swap_interval);
        if timing
            && next.enable_gpu_timing()
            && let Some(timer) = &mut next.gpu_timer
        {
            timer.completed.extend(completed);
        }
        *self = next;
        Ok(())
    }
    pub fn driver_info(&self) -> String {
        // SAFETY: The context remains owned by this thread. GL owns the returned C strings.
        unsafe {
            if !self.make_current() {
                return "OpenGL context unavailable".into();
            }
            let read = |name| {
                let bytes = glGetString(name);
                if bytes.is_null() {
                    "unknown".into()
                } else {
                    CStr::from_ptr(bytes.cast()).to_string_lossy().into_owned()
                }
            };
            format!("{} / {}", read(0x1f01), read(0x1f02))
        }
    }

    pub fn set_swap_interval(&mut self, interval: i32) {
        if interval >= 0 {
            self.swap_interval = interval;
        }
        self.host.set_swap_interval(interval);
    }
    pub fn set_cursor(&mut self, grab: bool, visible: bool) {
        self.host.set_cursor(grab, visible);
    }
    pub fn poll(&mut self) -> Vec<Event> {
        let events = self.host.poll();
        (self.width, self.height) = self.host.size();
        events
    }
    fn make_current(&self) -> bool {
        if !self.host.make_current() {
            return false;
        }
        unsafe {
            glBindVertexArray(self.vertex_array);
        }
        true
    }
    /// Uploads one complete clip-space stream and draws valid triangle batches.
    /// Invalid or out-of-range batch ranges are ignored. Call on the creating thread.
    pub fn present(&mut self, vertices: &[[f32; 8]], batches: &[Batch], clear: Color) {
        self.present_scene(vertices, batches, clear, None);
    }
    pub fn last_mesh_upload_bytes(&self) -> u64 {
        self.mesh_upload_bytes
    }
    /// Actual palette uniform bytes submitted in color and shadow programs.
    pub fn sprite_upload_bytes(&self) -> u64 {
        self.sprite_uploaded_bytes
    }
    pub fn skin_upload_bytes(&self) -> u64 {
        let Some(gpu) = &self.retained else {
            return 0;
        };
        let mut bytes = 0;
        for b in &gpu.batches {
            if b.reserved[1] == 0 {
                continue;
            }
            let pair = gpu
                .materials
                .get(&b.material)
                .and_then(|m| m.valid.as_ref())
                .unwrap_or(&gpu.builtin);
            for shadow in [false, true] {
                let used = if shadow {
                    gpu.environment.shadow_size > 0
                        && gpu.shadow.is_some()
                        && b.depth & 1 != 0
                        && b.depth & 8 == 0
                        && b.light[15] > 0.5
                } else {
                    b.depth & 32 == 0
                };
                if used {
                    let p = if shadow { &pair.depth } else { &pair.color };
                    bytes += b.reserved[1] as u64
                        * (if p.bones >= 0 { 64 } else { 0 }
                            + if p.bone_normals >= 0 { 36 } else { 0 });
                }
            }
        }
        bytes
    }
    pub fn enable_gpu_timing(&mut self) -> bool {
        if !self.make_current() {
            return false;
        }
        if self.gpu_timer.is_none() {
            self.gpu_timer = GpuTimer::new();
        }
        self.gpu_timer.is_some()
    }
    pub fn drain_gpu_results(&mut self, mut f: impl FnMut(u64, f64)) {
        if !self.make_current() {
            return;
        }
        if let Some(timer) = &mut self.gpu_timer {
            timer.poll();
            for (frame, time) in timer.completed.drain(..) {
                f(frame, time);
            }
        }
    }
    pub(crate) fn finish(&self) {
        if self.make_current() {
            unsafe {
                glFinish();
            }
        }
    }
    pub fn finish_gpu_timing(&mut self) {
        if self.gpu_timer.is_some() && self.make_current() {
            unsafe {
                glFinish();
            }
            if let Some(timer) = &mut self.gpu_timer {
                timer.poll();
            }
        }
    }
    pub fn render_counts(&self) -> (u64, u64) {
        let mut calls = 0;
        let mut vertices = 0;
        for &(_, count, _) in &self.last_batches {
            if count > 0 {
                calls += 1;
                vertices += count as u64;
            }
        }
        for b in &self.last_effects {
            if b.count > 0 {
                calls += 1;
                vertices += b.count as u64;
            }
        }
        if let Some(sprites) = &self.sprites {
            for b in &sprites.batches {
                if b.count > 0 {
                    calls += 1;
                    vertices += b.count as u64 * b.triangles as u64 * 3;
                }
            }
        }
        if let Some(gpu) = &self.retained {
            for b in &gpu.batches {
                if let Some((_, count)) = gpu.meshes.get(&b.mesh) {
                    let main = b.depth & 32 == 0;
                    let shadow = gpu.environment.shadow_size > 0
                        && gpu.shadow.is_some()
                        && b.depth & 1 != 0
                        && b.depth & 8 == 0
                        && b.light[15] > 0.5;
                    let passes = main as u64 + shadow as u64;
                    calls += passes;
                    vertices += *count as u64 * b.count as u64 * passes;
                }
            }
        }
        (calls, vertices)
    }

    pub(crate) fn present_scene(
        &mut self,
        vertices: &[[f32; 8]],
        batches: &[Batch],
        clear: Color,
        frame: Option<&crate::retained::Frame>,
    ) {
        if !self.make_current() {
            return;
        }
        let Ok(byte_count) = isize::try_from(std::mem::size_of_val(vertices)) else {
            return;
        };
        self.last_clear = clear;
        self.last_commands.clear();
        if let Some(frame) = frame {
            self.last_commands.extend_from_slice(&frame.commands);
        }
        self.mesh_upload_bytes = 0;
        self.sprite_uploaded_bytes = 0;
        if let Some(sprites) = &mut self.sprites {
            sprites.batches.clear();
        }
        if let Some(frame) = frame {
            self.upload_sprites(frame);
        }
        if let Some(frame) = frame {
            self.upload_retained(frame);
        }
        if let Some(gpu) = &mut self.retained {
            gpu.batches.clear();
            gpu.skin_matrices.clear();
            gpu.skin_normals.clear();
            if let Some(frame) = frame {
                gpu.batches.extend_from_slice(&frame.batches);
                gpu.skin_matrices.extend_from_slice(&frame.skin_matrices);
                gpu.skin_normals.extend_from_slice(&frame.skin_normals);
            }
        }
        self.last_batches.clear();
        for batch in batches {
            if batch.count == 0
                || !batch.count.is_multiple_of(3)
                || batch.start > i32::MAX as u32
                || batch.count > i32::MAX as u32
                || batch
                    .start
                    .checked_add(batch.count)
                    .is_none_or(|end| end as usize > vertices.len())
            {
                self.last_batches.push((0, 0, false));
                continue;
            }
            if frame.is_none() {
                self.last_commands.push(crate::retained::Command {
                    kind: 0,
                    index: self.last_batches.len() as u32,
                });
            }
            self.last_batches
                .push((batch.start as i32, batch.count as i32, batch.depth));
        }
        self.last_effects.clear();
        if let Some(frame) = frame {
            for batch in &frame.effects {
                let valid = batch.count > 0
                    && batch.count.is_multiple_of(3)
                    && batch.start <= i32::MAX as u32
                    && batch.count <= i32::MAX as u32
                    && batch
                        .start
                        .checked_add(batch.count)
                        .is_some_and(|end| end as usize <= vertices.len())
                    && batch.flags & !7 == 0;
                self.last_effects.push(if valid {
                    *batch
                } else {
                    crate::effects::EffectBatch {
                        start: 0,
                        count: 0,
                        flags: 0,
                    }
                });
            }
        }
        // SAFETY: The source slice is valid for byte_count bytes and GL copies it during
        // BufferData. Subsequent saved draw ranges refer to this persistent GPU buffer.
        unsafe {
            glBindBuffer(GL_ARRAY_BUFFER, self.vertex_buffer);
            glBufferData(
                GL_ARRAY_BUFFER,
                byte_count,
                vertices.as_ptr().cast(),
                GL_STREAM_DRAW,
            );
        }
        self.present_count += 1;
        let query = self.gpu_timer.as_mut().and_then(|t| t.begin());
        self.draw_uploaded();
        if let (Some(timer), Some(query)) = (&mut self.gpu_timer, query) {
            timer.end(query, self.present_count);
        }
        self.host.swap();
        self.presented = true;
    }
    fn upload_sprites(&mut self, frame: &crate::retained::Frame) {
        if frame.sprites.is_empty() {
            return;
        }
        if self.sprites.is_none() {
            self.sprites = Some(SpriteGpu::new().expect("native sprite shader requires GL 3.3"));
            self.sprite_uploaded_bytes += 72;
        }
        let gpu = self.sprites.as_mut().unwrap();
        for batch in &frame.sprite_batches {
            let valid = (1..=2).contains(&batch.triangles)
                && batch.flags & !7 == 0
                && batch.count > 0
                && batch.count <= i32::MAX as u32
                && batch
                    .first
                    .checked_add(batch.count)
                    .is_some_and(|end| end as usize <= frame.sprites.len());
            gpu.batches.push(if valid {
                *batch
            } else {
                crate::effects::SpriteBatch {
                    first: 0,
                    count: 0,
                    flags: 0,
                    triangles: 1,
                }
            });
        }
        let bytes = frame.sprites.len() * 64;
        // SAFETY: Frame storage is live for this synchronous bounded upload.
        unsafe {
            glBindBuffer(GL_ARRAY_BUFFER, gpu.instances);
            glBufferData(
                GL_ARRAY_BUFFER,
                bytes as isize,
                frame.sprites.as_ptr().cast(),
                GL_STREAM_DRAW,
            );
        }
        self.sprite_uploaded_bytes += bytes as u64;
    }
    fn upload_retained(&mut self, frame: &crate::retained::Frame) {
        if frame.batches.is_empty() && crate::material::count() == 0 && self.retained.is_none() {
            return;
        }
        if self.retained.is_none() {
            // SAFETY: The current context owns this static driver string.
            let version_pointer = unsafe { glGetString(0x1f02) };
            assert!(!version_pointer.is_null(), "OpenGL version unavailable");
            // SAFETY: GL returned a non-null context-owned, NUL-terminated version string.
            let version = unsafe { CStr::from_ptr(version_pointer.cast()) }.to_string_lossy();
            let mut numbers = version.split(['.', ' ']);
            let major = numbers
                .next()
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(0);
            let minor = numbers
                .next()
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(0);
            assert!(
                (major, minor) >= (3, 3),
                "retained rendering requires OpenGL 3.3 or newer: {version}"
            );
            let builtin = ProgramPair::compile(&crate::material::ShaderSource::default())
                .expect("native retained shader");
            // SAFETY: The owned context is current. Names are NUL terminated.
            unsafe {
                let mut instances = 0;
                glGenBuffers(1, &mut instances);
                assert_ne!(instances, 0, "allocate retained instance buffer");
                self.retained = Some(RetainedGpu {
                    builtin,
                    environment: Default::default(),
                    white_texture: create_texture(1, false).expect("fallback texture"),
                    shadow: None,
                    shadow_failed_size: 0,
                    materials: BTreeMap::new(),
                    instances,
                    meshes: BTreeMap::new(),
                    skin_matrices: Vec::new(),
                    skin_normals: Vec::new(),
                    batches: Vec::new(),
                });
            }
        }
        let gpu = self.retained.as_mut().unwrap();
        gpu.environment = frame.environment;
        gpu.configure_shadow();
        gpu.prepare_materials();
        // SAFETY: All handles are owned by this context; uploads copy live bounded slices.
        unsafe {
            gpu.meshes.retain(|id, (buffer, _)| {
                if crate::retained::is_live(*id) {
                    true
                } else {
                    glDeleteBuffers(1, buffer);
                    false
                }
            });
            for batch in &frame.batches {
                if let std::collections::btree_map::Entry::Vacant(slot) =
                    gpu.meshes.entry(batch.mesh)
                {
                    let mesh = crate::retained::geometry(batch.mesh).expect("live frame mesh");
                    let mut buffer = 0;
                    glGenBuffers(1, &mut buffer);
                    assert_ne!(buffer, 0, "allocate retained mesh buffer");
                    glBindBuffer(GL_ARRAY_BUFFER, buffer);
                    let bytes = mesh.vertices.len() * 40;
                    if let Some(skin) = &mesh.skin {
                        // One static buffer holds positions then influences.
                        // Static-only cache entries keep their original size.
                        let skin_bytes = skin.len() * 32;
                        glBufferData(
                            GL_ARRAY_BUFFER,
                            (bytes + skin_bytes) as isize,
                            ptr::null(),
                            0x88e4,
                        );
                        glBufferSubData(
                            GL_ARRAY_BUFFER,
                            0,
                            bytes as isize,
                            mesh.vertices.as_ptr().cast(),
                        );
                        glBufferSubData(
                            GL_ARRAY_BUFFER,
                            bytes as isize,
                            skin_bytes as isize,
                            skin.as_ptr().cast(),
                        );
                        self.mesh_upload_bytes += skin_bytes as u64;
                    } else {
                        glBufferData(
                            GL_ARRAY_BUFFER,
                            bytes as isize,
                            mesh.vertices.as_ptr().cast(),
                            0x88e4,
                        );
                    }
                    self.mesh_upload_bytes += bytes as u64;
                    slot.insert((buffer, mesh.vertices.len() as i32));
                }
            }
            glBindBuffer(GL_ARRAY_BUFFER, gpu.instances);
            glBufferData(
                GL_ARRAY_BUFFER,
                (frame.instances.len() * 112) as isize,
                frame.instances.as_ptr().cast(),
                GL_STREAM_DRAW,
            );
        }
    }
    fn draw_uploaded(&self) {
        // SAFETY: Called with this context current. Attribute pointers are byte offsets
        // into the owned VBO and all saved ranges were checked against its uploaded length.
        unsafe {
            if let Some(gpu) = &self.retained {
                gpu.render_shadow();
            }
            glBindFramebuffer(0x8d40, super::host::default_framebuffer());
            glDrawBuffer(screen_buffer());
            glViewport(
                0,
                0,
                self.width.min(i32::MAX as u32) as i32,
                self.height.min(i32::MAX as u32) as i32,
            );
            glDepthMask(1);
            glClearColor(
                self.last_clear.r,
                self.last_clear.g,
                self.last_clear.b,
                self.last_clear.a,
            );
            glClear(GL_COLOR_BUFFER_BIT | GL_DEPTH_BUFFER_BIT);
            for command in &self.last_commands {
                if command.kind == 1 {
                    if let Some(gpu) = &self.retained {
                        gpu.draw(command.index as usize, false);
                    }
                } else if command.kind == 3 {
                    if let Some(sprites) = &self.sprites {
                        sprites.draw(command.index as usize);
                    }
                } else {
                    let draw = if command.kind == 0 {
                        self.last_batches
                            .get(command.index as usize)
                            .map(|&(start, count, depth)| (start, count, depth as u32))
                    } else if command.kind == 2 {
                        self.last_effects
                            .get(command.index as usize)
                            .map(|b| (b.start as i32, b.count as i32, b.flags))
                    } else {
                        None
                    };
                    if let Some((start, count, flags)) = draw.filter(|(_, count, _)| *count > 0) {
                        glUseProgram(self.program);
                        glBindBuffer(GL_ARRAY_BUFFER, self.vertex_buffer);
                        glEnableVertexAttribArray(0);
                        glEnableVertexAttribArray(1);
                        glVertexAttribPointer(0, 4, GL_FLOAT, 0, 32, ptr::null());
                        glVertexAttribPointer(1, 4, GL_FLOAT, 0, 32, 16_usize as *const c_void);
                        set_depth(flags & 1 != 0);
                        glDepthMask((flags & 1 != 0 && flags & 2 == 0) as u8);
                        glEnable(GL_BLEND);
                        glBlendFuncSeparate(
                            GL_SRC_ALPHA,
                            if flags & 4 != 0 {
                                1
                            } else {
                                GL_ONE_MINUS_SRC_ALPHA
                            },
                            1,
                            GL_ONE_MINUS_SRC_ALPHA,
                        );
                        glDrawArrays(GL_TRIANGLES, start, count);
                        glDisableVertexAttribArray(0);
                        glDisableVertexAttribArray(1);
                    }
                }
            }
            glBindBuffer(GL_ARRAY_BUFFER, 0);
            glUseProgram(0);
            glDepthMask(1);
            glBlendFuncSeparate(
                GL_SRC_ALPHA,
                GL_ONE_MINUS_SRC_ALPHA,
                1,
                GL_ONE_MINUS_SRC_ALPHA,
            );
        }
    }
    /// Reads the last presented image as top-to-bottom RGBA8.
    /// Returns empty if closed, unavailable, or larger than 16,777,216 pixels.
    pub fn pixels(&self) -> Vec<u8> {
        let Some(count) = (self.width as usize)
            .checked_mul(self.height as usize)
            .filter(|&count| count <= MAX_READBACK_PIXELS)
        else {
            return Vec::new();
        };
        if !self.presented || !self.make_current() {
            return Vec::new();
        }
        // Window-system front buffers can be undefined after asynchronous/composited swaps.
        // Replay the saved GPU stream into the back buffer only when readback is requested.
        // This avoids per-frame CPU copies and does not change the visible front buffer.
        self.draw_uploaded();
        let mut bytes = vec![0_u8; count * 4];
        // SAFETY: The target allocation has exactly width*height*4 bytes and pack alignment
        // is one. ReadPixels synchronizes the completed back-buffer rendering.
        unsafe {
            glFinish();
            glReadBuffer(if cfg!(target_os = "ios") {
                0x8ce0
            } else {
                0x0405
            });
            glPixelStorei(0x0d05, 1); // GL_PACK_ALIGNMENT
            glReadPixels(
                0,
                0,
                self.width as i32,
                self.height as i32,
                0x1908,
                0x1401,
                bytes.as_mut_ptr().cast(),
            );
        }
        flip_rows(&mut bytes, self.width as usize * 4, self.height as usize);
        bytes
    }
}
impl Drop for NativeWindow {
    fn drop(&mut self) {
        // SAFETY: The host stays live/current until dependent GL allocations are released.
        unsafe {
            if self.make_current() {
                if let Some(timer) = &self.gpu_timer {
                    glDeleteQueries(timer.queries.len() as i32, timer.queries.as_ptr());
                }
                if self.vertex_buffer != 0 {
                    glDeleteBuffers(1, &self.vertex_buffer);
                }
                if self.program != 0 {
                    glDeleteProgram(self.program);
                }
                if let Some(sprites) = &self.sprites {
                    sprites.delete();
                }
                if let Some(gpu) = &self.retained {
                    for (buffer, _) in gpu.meshes.values() {
                        glDeleteBuffers(1, buffer);
                    }
                    glDeleteBuffers(1, &gpu.instances);
                    gpu.builtin.delete();
                    glDeleteTextures(1, &gpu.white_texture);
                    if let Some(target) = &gpu.shadow {
                        target.delete();
                    }
                    for m in gpu.materials.values() {
                        if let Some(p) = &m.valid {
                            p.delete();
                        }
                    }
                }
                if self.vertex_array != 0 {
                    glDeleteVertexArrays(1, &self.vertex_array);
                }
            }
        }
    }
}
unsafe fn set_depth(depth: bool) {
    // SAFETY: Caller owns the current GL context.
    unsafe {
        if depth {
            glEnable(GL_DEPTH_TEST);
            glDepthMask(1);
        } else {
            glDisable(GL_DEPTH_TEST);
            glDepthMask(0);
        }
    }
}
fn create_texture(size: u32, depth: bool) -> Result<c_uint, String> {
    // SAFETY: A live current context owns the allocation and copies the optional pixel.
    unsafe {
        let mut texture = 0;
        glGenTextures(1, &mut texture);
        if texture == 0 {
            return Err("texture allocation failed".into());
        }
        glBindTexture(0x0de1, texture);
        glTexParameteri(0x0de1, 0x2801, 0x2600);
        glTexParameteri(0x0de1, 0x2800, 0x2600);
        glTexParameteri(0x0de1, 0x2802, 0x812f);
        glTexParameteri(0x0de1, 0x2803, 0x812f);
        if depth {
            glTexImage2D(
                0x0de1,
                0,
                0x81a6,
                size as i32,
                size as i32,
                0,
                0x1902,
                0x1405,
                ptr::null(),
            );
        } else {
            let white = [255u8; 4];
            glTexImage2D(
                0x0de1,
                0,
                0x8058,
                1,
                1,
                0,
                0x1908,
                0x1401,
                white.as_ptr().cast(),
            );
        }
        Ok(texture)
    }
}
impl ShadowTarget {
    fn new(size: u32) -> Result<Self, String> {
        let texture = create_texture(size, true)?;
        // SAFETY: The target and texture are owned by this current context.
        unsafe {
            let mut framebuffer = 0;
            glGenFramebuffers(1, &mut framebuffer);
            glBindFramebuffer(0x8d40, framebuffer);
            glFramebufferTexture2D(0x8d40, 0x8d00, 0x0de1, texture, 0);
            glDrawBuffer(0);
            glReadBuffer(0);
            let status = glCheckFramebufferStatus(0x8d40);
            glBindFramebuffer(0x8d40, super::host::default_framebuffer());
            glDrawBuffer(screen_buffer());
            glReadBuffer(screen_buffer());
            if framebuffer == 0 || status != 0x8cd5 {
                glDeleteFramebuffers(1, &framebuffer);
                glDeleteTextures(1, &texture);
                return Err(format!("shadow framebuffer incomplete: {status:#x}"));
            }
            Ok(Self {
                framebuffer,
                texture,
                size,
            })
        }
    }
    unsafe fn delete(&self) {
        unsafe {
            glDeleteFramebuffers(1, &self.framebuffer);
            glDeleteTextures(1, &self.texture);
        }
    }
}
impl ProgramPair {
    fn compile(source: &crate::material::ShaderSource) -> Result<Self, String> {
        let color = GpuProgram::compile_stage(source, false)?;
        match GpuProgram::compile_stage(source, true) {
            Ok(depth) => Ok(Self { color, depth }),
            Err(error) => {
                unsafe {
                    glDeleteProgram(color.handle);
                }
                Err(error)
            }
        }
    }
    unsafe fn delete(&self) {
        unsafe {
            glDeleteProgram(self.color.handle);
            glDeleteProgram(self.depth.handle);
        }
    }
}
impl GpuProgram {
    fn compile_stage(source: &crate::material::ShaderSource, depth: bool) -> Result<Self, String> {
        let vertex = CString::new(format!(
            "{}{}\n{}",
            shader_header(),
            crate::retained::VERTEX_BODY
                .replace("attribute ", "in ")
                .replace("varying ", "out "),
            source.vertex
        ))
        .map_err(|e| e.to_string())?;
        let fragment = if depth {
            CString::new(format!("{}void main(){{}}\n", shader_header())).unwrap()
        } else {
            CString::new(format!(
                "{}out vec4 ferravex_pixel;\n{}\n{}",
                shader_header(),
                crate::material::FRAGMENT_BODY
                    .replace("varying ", "in ")
                    .replace("gl_FragColor", "ferravex_pixel"),
                source.fragment
            ))
            .map_err(|e| e.to_string())?
        };
        let attributes = [
            (0, c"a_position"),
            (1, c"a_normal"),
            (2, c"a_color"),
            (3, c"a_model0"),
            (4, c"a_model1"),
            (5, c"a_model2"),
            (6, c"a_model3"),
            (7, c"a_normal0"),
            (8, c"a_normal1"),
            (9, c"a_normal2"),
            (10, c"a_joints"),
            (11, c"a_weights"),
        ];
        let handle = create_program_stages(&vertex, &fragment, &attributes)?;
        // SAFETY: Compilation returned a live program owned by the current GL context.
        unsafe {
            Ok(Self {
                handle,
                camera: glGetUniformLocation(handle, c"u_camera".as_ptr()),
                light: glGetUniformLocation(handle, c"u_light[0]".as_ptr()),
                params: glGetUniformLocation(handle, c"u_params[0]".as_ptr()),
                points: glGetUniformLocation(handle, c"u_points[0]".as_ptr()),
                point_count: glGetUniformLocation(handle, c"u_point_count".as_ptr()),
                shadow_matrix: glGetUniformLocation(handle, c"u_shadow_matrix".as_ptr()),
                shadow_texture: glGetUniformLocation(handle, c"u_shadow_texture".as_ptr()),
                shadow_options: glGetUniformLocation(handle, c"u_shadow_options".as_ptr()),
                shadow_extra: glGetUniformLocation(handle, c"u_shadow_extra".as_ptr()),
                skin_count: glGetUniformLocation(handle, c"u_skin_count".as_ptr()),
                bones: glGetUniformLocation(handle, c"u_bones[0]".as_ptr()),
                bone_normals: glGetUniformLocation(handle, c"u_bone_normals[0]".as_ptr()),
            })
        }
    }
}
impl RetainedGpu {
    fn configure_shadow(&mut self) {
        let wanted = self.environment.shadow_size;
        if wanted == 0 {
            if let Some(old) = self.shadow.take() {
                unsafe {
                    old.delete();
                }
            }
            self.shadow_failed_size = 0;
            return;
        }
        if self.shadow.as_ref().is_some_and(|s| s.size == wanted)
            || self.shadow_failed_size == wanted
        {
            return;
        }
        match ShadowTarget::new(wanted) {
            Ok(target) => {
                if let Some(old) = self.shadow.replace(target) {
                    unsafe {
                        old.delete();
                    }
                }
                self.shadow_failed_size = 0;
            }
            Err(error) => {
                eprintln!("Ferravex shadows: {error}");
                self.shadow_failed_size = wanted;
            }
        }
    }
    unsafe fn render_shadow(&self) {
        let Some(target) = &self.shadow else {
            return;
        };
        if self.environment.shadow_size == 0 {
            return;
        }
        // SAFETY: This context owns the complete depth target and all recorded ranges.
        unsafe {
            glActiveTexture(0x84c0);
            glBindTexture(0x0de1, self.white_texture);
            glBindFramebuffer(0x8d40, target.framebuffer);
            glViewport(0, 0, target.size as i32, target.size as i32);
            glDepthMask(1);
            glClear(GL_DEPTH_BUFFER_BIT);
            glDisable(GL_BLEND);
            glEnable(0x8037);
            glPolygonOffset(1.5, 3.);
            for index in 0..self.batches.len() {
                self.draw(index, true);
            }
            glDisable(0x8037);
            glEnable(GL_BLEND);
            glBindFramebuffer(0x8d40, super::host::default_framebuffer());
            glDrawBuffer(screen_buffer());
        }
    }
    fn prepare_materials(&mut self) {
        self.materials.retain(|id, entry| {
            if crate::material::lookup(*id).is_some() {
                true
            } else {
                // SAFETY: This cache owns the program and its context is current.
                if let Some(program) = &entry.valid {
                    unsafe {
                        program.delete();
                    }
                }
                false
            }
        });
        crate::material::for_each(|data| {
            let id = data.id;
            let revision = data.revision.get();
            let entry = self.materials.entry(id).or_insert(MaterialProgram {
                revision: 0,
                valid: None,
            });
            if entry.revision == revision {
                return;
            }
            entry.revision = revision;
            match ProgramPair::compile(&data.source.borrow()) {
                Ok(program) => {
                    if let Some(old) = entry.valid.replace(program) {
                        unsafe {
                            old.delete();
                        }
                    }
                    crate::material::feedback(id, revision, Ok(()));
                }
                Err(error) => {
                    if entry.valid.is_none()
                        && let Some(source) = data.last_valid_source.borrow().as_ref()
                    {
                        match ProgramPair::compile(source) {
                            Ok(program) => entry.valid = Some(program),
                            Err(e) => eprintln!("Ferravex material {id} restore failed: {e}"),
                        }
                    }
                    eprintln!("Ferravex material {id} revision {revision} rejected: {error}");
                    crate::material::feedback(id, revision, Err(&error));
                }
            }
        });
    }
    unsafe fn draw(&self, index: usize, shadow: bool) {
        let Some(batch) = self.batches.get(index) else {
            return;
        };
        if (shadow && (batch.depth & 1 == 0 || batch.depth & 8 != 0 || batch.light[15] < 0.5))
            || (!shadow && batch.depth & 32 != 0)
        {
            return;
        }
        let Some(&(buffer, count)) = self.meshes.get(&batch.mesh) else {
            return;
        };
        // SAFETY: Recorded batch/instance ranges were produced by the bounded Frame recorder.
        // Attributes refer to the owned static mesh and streamed instance buffers.
        unsafe {
            set_depth(batch.depth & 1 != 0);
            glDepthMask((shadow || (batch.depth & 1 != 0 && batch.depth & 2 == 0)) as u8);
            if shadow {
                glDisable(GL_BLEND);
            } else {
                glEnable(GL_BLEND);
                glBlendFuncSeparate(
                    GL_SRC_ALPHA,
                    if batch.depth & 4 != 0 {
                        1
                    } else {
                        GL_ONE_MINUS_SRC_ALPHA
                    },
                    1,
                    GL_ONE_MINUS_SRC_ALPHA,
                );
            }
            let pair = self
                .materials
                .get(&batch.material)
                .and_then(|m| m.valid.as_ref())
                .unwrap_or(&self.builtin);
            let program = if shadow { &pair.depth } else { &pair.color };
            glUseProgram(program.handle);
            let first = batch.reserved[0] as usize;
            let bones = batch.reserved[1] as usize;
            glUniform1i(program.skin_count, bones as i32);
            if bones > 0 {
                glUniformMatrix4fv(
                    program.bones,
                    bones as i32,
                    0,
                    self.skin_matrices[first..first + bones].as_ptr().cast(),
                );
                glUniformMatrix3fv(
                    program.bone_normals,
                    bones as i32,
                    0,
                    self.skin_normals[first..first + bones].as_ptr().cast(),
                );
            }
            glUniformMatrix4fv(
                program.camera,
                1,
                0,
                if shadow {
                    self.environment.shadow_matrix.as_ptr()
                } else {
                    batch.camera.as_ptr()
                },
            );
            glUniform4fv(program.light, 4, batch.light.as_ptr());
            glUniform4fv(program.params, 4, batch.params.as_ptr());
            if !shadow {
                let e = &self.environment;
                let count = if batch.light[3] > 0.5 {
                    e.light_count
                } else {
                    0
                };
                glUniform1i(program.point_count, count as i32);
                if count > 0 {
                    glUniform4fv(program.points, 16, e.points.as_ptr());
                }
                glUniformMatrix4fv(program.shadow_matrix, 1, 0, e.shadow_matrix.as_ptr());
                let mut options = e.shadow_options;
                let matches = batch.light[0] * e.shadow_direction[0]
                    + batch.light[1] * e.shadow_direction[1]
                    + batch.light[2] * e.shadow_direction[2]
                    > 0.999;
                options[3] = if self.shadow.is_some()
                    && e.shadow_size > 0
                    && batch.depth & 16 == 0
                    && matches
                {
                    1.
                } else {
                    0.
                };
                let mut extra = e.shadow_extra;
                if let Some(target) = &self.shadow {
                    extra[1] = 1. / target.size as f32;
                }
                glUniform4fv(program.shadow_options, 1, options.as_ptr());
                glUniform4fv(program.shadow_extra, 1, extra.as_ptr());
                glUniform1i(program.shadow_texture, 0);
                glActiveTexture(0x84c0);
                glBindTexture(
                    0x0de1,
                    self.shadow
                        .as_ref()
                        .filter(|_| e.shadow_size > 0)
                        .map_or(self.white_texture, |s| s.texture),
                );
            }
            glBindBuffer(GL_ARRAY_BUFFER, buffer);
            for (attribute, size, offset) in [(0, 3, 0), (1, 3, 12), (2, 4, 24)] {
                glEnableVertexAttribArray(attribute);
                glVertexAttribPointer(attribute, size, GL_FLOAT, 0, 40, offset as *const c_void);
            }
            if bones > 0 {
                for (attribute, offset) in [(10, 0), (11, 16)] {
                    glEnableVertexAttribArray(attribute);
                    glVertexAttribPointer(
                        attribute,
                        4,
                        GL_FLOAT,
                        0,
                        32,
                        (count as usize * 40 + offset) as *const c_void,
                    );
                }
            }
            glBindBuffer(GL_ARRAY_BUFFER, self.instances);
            for i in 0..7 {
                let at = 3 + i;
                glEnableVertexAttribArray(at);
                glVertexAttribPointer(
                    at,
                    4,
                    GL_FLOAT,
                    0,
                    112,
                    (batch.first as usize * 112 + i as usize * 16) as *const c_void,
                );
                glVertexAttribDivisor(at, 1);
            }
            glDrawArraysInstanced(GL_TRIANGLES, 0, count, batch.count as i32);
            glBlendFuncSeparate(
                GL_SRC_ALPHA,
                GL_ONE_MINUS_SRC_ALPHA,
                1,
                GL_ONE_MINUS_SRC_ALPHA,
            );
            for at in 0..12 {
                glDisableVertexAttribArray(at);
                if at >= 3 {
                    glVertexAttribDivisor(at, 0);
                }
            }
        }
    }
}
fn flip_rows(bytes: &mut [u8], stride: usize, height: usize) {
    for row in 0..height / 2 {
        let opposite = (height - 1 - row) * stride;
        let (top, bottom) = bytes.split_at_mut(opposite);
        top[row * stride..(row + 1) * stride].swap_with_slice(&mut bottom[..stride]);
    }
}
fn compile_shader(kind: c_uint, source: &CStr) -> Result<c_uint, String> {
    // SAFETY: Called only with a current context. source remains NUL-terminated and alive
    // while ShaderSource copies it; shader handles are released on every failure path.
    unsafe {
        let shader = glCreateShader(kind);
        if shader == 0 {
            return Err("OpenGL 2.0 shader creation failed".into());
        }
        let pointer = source.as_ptr();
        glShaderSource(shader, 1, &pointer, ptr::null());
        glCompileShader(shader);
        let mut success = 0;
        glGetShaderiv(shader, GL_COMPILE_STATUS, &mut success);
        if success == 0 {
            let message = shader_log(shader);
            glDeleteShader(shader);
            return Err(format!("Native shader compilation failed: {message}"));
        }
        Ok(shader)
    }
}
fn shader_log(shader: c_uint) -> String {
    // SAFETY: The driver writes at most the allocated byte length.
    unsafe {
        let mut count = 0;
        glGetShaderiv(shader, GL_INFO_LOG_LENGTH, &mut count);
        let mut bytes = vec![0; count.clamp(1, 16384) as usize];
        let mut written = 0;
        glGetShaderInfoLog(
            shader,
            bytes.len() as i32,
            &mut written,
            bytes.as_mut_ptr().cast(),
        );
        String::from_utf8_lossy(&bytes[..(written.max(0) as usize).min(bytes.len())]).into_owned()
    }
}
fn create_program() -> Result<c_uint, String> {
    let vertex=CString::new(format!("{}in vec4 a_position;in vec4 a_color;out vec4 v_color;void main(){{gl_Position=a_position;v_color=a_color;}}",shader_header())).unwrap();
    let fragment = CString::new(format!(
        "{}in vec4 v_color;out vec4 ferravex_pixel;void main(){{ferravex_pixel=v_color;}}",
        shader_header()
    ))
    .unwrap();
    create_program_stages(&vertex, &fragment, &[(0, c"a_position"), (1, c"a_color")])
}
fn screen_buffer() -> u32 {
    if cfg!(target_os = "ios") {
        0x8ce0
    } else {
        0x0405
    }
}
fn shader_header() -> &'static str {
    if cfg!(target_os = "ios") {
        "#version 300 es\nprecision highp float;\nprecision highp int;\n"
    } else {
        "#version 330\n"
    }
}
fn create_program_stages(
    source: &CStr,
    fragment_source: &CStr,
    attributes: &[(u32, &CStr)],
) -> Result<c_uint, String> {
    let vertex = compile_shader(GL_VERTEX_SHADER, source)?;
    let fragment = match compile_shader(GL_FRAGMENT_SHADER, fragment_source) {
        Ok(shader) => shader,
        Err(error) => {
            // SAFETY: vertex is the live handle just compiled above.
            unsafe {
                glDeleteShader(vertex);
            }
            return Err(error);
        }
    };
    // SAFETY: All handles refer to the current context; linkage status is checked before use.
    unsafe {
        let program = glCreateProgram();
        if program == 0 {
            glDeleteShader(vertex);
            glDeleteShader(fragment);
            return Err("OpenGL program allocation failed".into());
        }
        glAttachShader(program, vertex);
        glAttachShader(program, fragment);
        for &(index, name) in attributes {
            glBindAttribLocation(program, index, name.as_ptr());
        }
        glLinkProgram(program);
        glDeleteShader(vertex);
        glDeleteShader(fragment);
        let mut success = 0;
        glGetProgramiv(program, GL_LINK_STATUS, &mut success);
        if success == 0 {
            let mut count = 0;
            glGetProgramiv(program, GL_INFO_LOG_LENGTH, &mut count);
            let mut bytes = vec![0; count.clamp(1, 16384) as usize];
            let mut written = 0;
            glGetProgramInfoLog(
                program,
                bytes.len() as i32,
                &mut written,
                bytes.as_mut_ptr().cast(),
            );
            let message =
                String::from_utf8_lossy(&bytes[..(written.max(0) as usize).min(bytes.len())]);
            glDeleteProgram(program);
            return Err(format!("Native shader linkage failed: {message}"));
        }
        Ok(program)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires a live X11 display and OpenGL context"]
    fn instanced_particle_shapes_match_cpu_reference_and_reuse_their_buffers() {
        use crate::{
            Mat4, Vec3,
            effects::{
                self, BillboardView, EffectBlend, EffectStyle, ParticleRenderer, ParticleShape,
                ParticleStyle,
            },
        };
        use ferravex_play::particles::{ParticleDesc, ParticleMotion, ParticlePool};
        let mut window = NativeWindow::new(&Conf {
            window_width: 128,
            window_height: 96,
            high_dpi: false,
            ..Default::default()
        })
        .unwrap();
        window.set_swap_interval(0);
        let mut pool = ParticlePool::new(3);
        for (x, z, color) in [
            (-0.25, 0., [1., 0.2, 0., 0.6]),
            (0.2, 0.2, [0., 1., 0.3, 0.5]),
            (0., -0.2, [0.2, 0., 1., 0.7]),
        ] {
            pool.spawn_with(
                ParticleDesc {
                    position: [x, 0., z],
                    velocity: [0.3, 0.1, 0.],
                    start_size: 0.7,
                    end_size: 0.7,
                    start_color: color,
                    end_color: color,
                    lifetime: 2.,
                    ..Default::default()
                },
                ParticleMotion {
                    rotation: 0.35,
                    ..Default::default()
                },
            )
            .unwrap();
        }
        let view = BillboardView::new(Vec3::new(0., 0., 5.), Vec3::X, Vec3::Y).unwrap();
        let mut renderer = ParticleRenderer::new(3).unwrap();
        for shape in [
            ParticleShape::Square,
            ParticleShape::Diamond,
            ParticleShape::Triangle,
        ] {
            for blend in [EffectBlend::Alpha, EffectBlend::Additive] {
                let style = ParticleStyle {
                    shape,
                    draw: EffectStyle {
                        blend,
                        ..Default::default()
                    },
                    stretch: 1.,
                    max_stretch: 2.,
                    ..Default::default()
                };
                let mut reference: Vec<_> = pool.iter().collect();
                if blend == EffectBlend::Alpha {
                    reference.sort_by(|a, b| a.position[2].total_cmp(&b.position[2]));
                }
                crate::input::host_frame(1. / 60., 128, 96, || {
                    crate::clear_background(crate::BLACK);
                    crate::edit(|s| {
                        s.matrix = Mat4::IDENTITY;
                        s.depth = true;
                    });
                    let triangles = reference.iter().flat_map(|p| {
                        effects::particle_triangles(*p, view, style)
                            .unwrap()
                            .into_iter()
                            .flatten()
                    });
                    effects::draw_triangles(triangles, style.draw);
                });
                crate::read(|s| {
                    window.present_scene(&s.vertices, &s.batches, crate::BLACK, Some(&s.retained))
                });
                let expected = window.pixels();
                crate::input::host_frame(1. / 60., 128, 96, || {
                    crate::clear_background(crate::BLACK);
                    crate::edit(|s| {
                        s.matrix = Mat4::IDENTITY;
                        s.depth = true;
                    });
                    renderer.draw_3d(&pool, view, style).unwrap();
                });
                crate::read(|s| {
                    window.present_scene(&s.vertices, &s.batches, crate::BLACK, Some(&s.retained))
                });
                let actual = window.pixels();
                let different = expected
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .zip(actual.as_chunks::<4>().0.iter())
                    .filter(|(a, b)| a.iter().zip(*b).any(|(a, b)| a.abs_diff(*b) > 2))
                    .count();
                assert!(
                    different < 12,
                    "{shape:?}/{blend:?} mismatch: {different} pixels"
                );
                let triangles = if shape == ParticleShape::Triangle {
                    1
                } else {
                    2
                };
                assert_eq!(window.render_counts(), (1, 3 * triangles * 3));
                crate::read(|s| {
                    window.present_scene(&s.vertices, &s.batches, crate::BLACK, Some(&s.retained))
                });
                assert_eq!(
                    window.sprite_upload_bytes(),
                    3 * 64,
                    "warmed sprites should upload only instances"
                );
            }
        }
    }
    #[test]
    #[ignore = "requires a live X11 display and OpenGL context"]
    fn effect_alpha_additive_depth_and_hud_order_match_actual_gpu_pixels() {
        use crate::{
            Mat4,
            effects::{self, EffectBlend, EffectStyle, RibbonTriangle, RibbonVertex},
        };
        let mut window = NativeWindow::new(&Conf {
            window_width: 96,
            window_height: 64,
            high_dpi: false,
            ..Default::default()
        })
        .unwrap();
        window.set_swap_interval(0);
        let triangle = |z: f32, color: [f32; 4], small: bool| {
            let p = if small {
                [[-0.4, -0.4, z], [0.4, -0.4, z], [0., 0.4, z]]
            } else {
                [[-1., -1., z], [3., -1., z], [-1., 3., z]]
            };
            RibbonTriangle {
                vertices: p.map(|position| RibbonVertex {
                    position,
                    color,
                    uv: [0.; 2],
                }),
            }
        };
        let render =
            |window: &mut NativeWindow, blue_z: f32, red: bool, hud: bool, additive: bool| {
                crate::input::host_frame(1. / 60., 96, 64, || {
                    crate::clear_background(crate::BLACK);
                    crate::edit(|s| {
                        s.matrix = Mat4::IDENTITY;
                        s.depth = true;
                    });
                    effects::draw_triangles(
                        [triangle(blue_z, [0., 0., 1., 1.], false)],
                        EffectStyle {
                            depth_write: true,
                            ..Default::default()
                        },
                    );
                    effects::draw_triangles(
                        [triangle(
                            0.,
                            if additive {
                                [1., 0., 0., 0.25]
                            } else {
                                [0., 1., 0., 0.5]
                            },
                            false,
                        )],
                        EffectStyle {
                            blend: if additive {
                                EffectBlend::Additive
                            } else {
                                EffectBlend::Alpha
                            },
                            ..Default::default()
                        },
                    );
                    if red {
                        effects::draw_triangles(
                            [triangle(0.25, [1., 0., 0., 1.], true)],
                            EffectStyle {
                                depth_write: true,
                                ..Default::default()
                            },
                        );
                    }
                    if hud {
                        effects::draw_triangles(
                            [triangle(
                                0.9,
                                if additive {
                                    [0., 1., 0., 0.5]
                                } else {
                                    [1., 1., 0., 1.]
                                },
                                true,
                            )],
                            EffectStyle {
                                depth_test: Some(false),
                                ..Default::default()
                            },
                        );
                    }
                });
                crate::read(|s| {
                    window.present_scene(&s.vertices, &s.batches, crate::BLACK, Some(&s.retained))
                });
                let pixels = window.pixels();
                let (w, h) = window.size();
                let at = ((h / 2 * w + w / 2) * 4) as usize;
                pixels[at..at + 4].to_vec()
            };
        let alpha = render(&mut window, 0.5, false, false, false);
        assert!(
            alpha[0] < 2 && (126..=129).contains(&alpha[1]) && (126..=129).contains(&alpha[2]),
            "bad alpha: {alpha:?}"
        );
        let no_write = render(&mut window, 0.5, true, false, false);
        assert_eq!(
            &no_write[..3],
            &[255, 0, 0],
            "transparent effect wrote depth"
        );
        let hud = render(&mut window, 0.5, true, true, false);
        assert_eq!(
            &hud[..3],
            &[255, 255, 0],
            "HUD did not restore its overlay state"
        );
        let occluded = render(&mut window, -0.2, false, false, false);
        assert_eq!(&occluded[..3], &[0, 0, 255], "effect ignored opaque depth");
        let additive = render(&mut window, 0.5, false, false, true);
        assert!(
            (62..=65).contains(&additive[0]) && additive[2] == 255,
            "bad additive: {additive:?}"
        );
        let restored = render(&mut window, 0.5, false, true, true);
        assert!(
            (30..=33).contains(&restored[0])
                && (126..=129).contains(&restored[1])
                && (126..=129).contains(&restored[2]),
            "additive leaked into HUD: {restored:?}"
        );
    }
    #[test]
    #[ignore = "requires a live X11 display and OpenGL context"]
    fn four_weight_gpu_skin_matches_static_geometry_normals_and_shadows() {
        use crate::{
            Mat4, Vec2, Vec3,
            lighting::{Quality, SunShadow},
            mesh::Mesh3d,
            retained::{DrawStyle, Frame},
            shaders::LitSettings,
            skinning::{SkinPalette, SkinWeights, SkinnedMesh},
        };
        let mut window = NativeWindow::new(&Conf {
            window_width: 192,
            window_height: 128,
            high_dpi: false,
            ..Default::default()
        })
        .unwrap();
        window.set_swap_interval(0);
        let floor = Mesh3d::plane(Vec2::splat(8.), crate::WHITE, 1);
        let cube = Mesh3d::cuboid(Vec3::ONE, crate::RED);
        let skin = SkinnedMesh::new(
            &cube.mesh,
            &vec![
                SkinWeights {
                    joints: [0, 1, 2, 3],
                    weights: [1.; 4]
                };
                cube.mesh.vertices.len()
            ],
            4,
        )
        .unwrap();
        let matrices: Vec<_> = [0.2, 0.6, 1., 1.4]
            .into_iter()
            .map(|x| {
                (Mat4::from_translation(Vec3::X * x) * Mat4::from_scale(Vec3::new(1., 1.5, 0.5)))
                    .to_cols_array()
            })
            .collect();
        let palette = SkinPalette::new(&matrices).unwrap();
        let eye = Vec3::new(4., 5., 6.);
        let camera = Mat4::perspective_rh_gl(0.8, 1.5, 0.1, 50.)
            * Mat4::look_at_rh(eye, Vec3::ZERO, Vec3::Y);
        let sun = SunShadow {
            direction: Vec3::new(-0.5, 1., -0.2),
            extent: 6.,
            depth: 15.,
            strength: 1.,
            ..Default::default()
        };
        let light = LitSettings {
            camera_position: eye,
            light_direction: sun.direction,
            ambient: 0.3,
            fog_start: 100.,
            fog_end: 200.,
            ..Default::default()
        }
        .sanitized();
        let mut frame = Frame::default();
        frame
            .environment
            .update(&[], Some(sun), Quality::High, Some(eye));
        frame.record(floor.resident(), Mat4::IDENTITY, camera, true, Some(light));
        frame.record(
            cube.resident(),
            Mat4::from_translation(Vec3::new(0.8, 0.75, 0.))
                * Mat4::from_scale(Vec3::new(1., 1.5, 0.5)),
            camera,
            true,
            Some(light),
        );
        window.present_scene(&[], &[], crate::BLACK, Some(&frame));
        let reference = window.pixels();
        frame.clear();
        frame.record(floor.resident(), Mat4::IDENTITY, camera, true, Some(light));
        frame.record_style(
            skin.resident(),
            Mat4::from_translation(Vec3::Y * 0.75),
            camera,
            true,
            DrawStyle {
                light: Some(light),
                skin: Some(&palette),
                ..Default::default()
            },
        );
        frame.finalize();
        window.present_scene(&[], &[], crate::BLACK, Some(&frame));
        let animated = window.pixels();
        let mismatches = reference
            .as_chunks::<4>()
            .0
            .iter()
            .zip(animated.as_chunks::<4>().0.iter())
            .filter(|(a, b)| a.iter().zip(*b).any(|(a, b)| a.abs_diff(*b) > 3))
            .count();
        assert!(
            mismatches < 10,
            "skin color/normal/shadow mismatch: {mismatches} pixels"
        );
        assert_eq!(
            window.last_mesh_upload_bytes(),
            skin.resident().vertex_count() as u64 * 72
        );
        window.present_scene(&[], &[], crate::BLACK, Some(&frame));
        assert_eq!(window.last_mesh_upload_bytes(), 0);
        assert_eq!(window.skin_upload_bytes(), 4 * (100 + 64));
        window.recreate().unwrap();
        window.set_swap_interval(0);
        window.present_scene(&[], &[], crate::BLACK, Some(&frame));
        assert_eq!(
            window.pixels(),
            animated,
            "skin palette, retained mesh and shadows rebuild after context recreation"
        );
        assert!(window.last_mesh_upload_bytes() > 0);
        let moved =
            SkinPalette::new(&[Mat4::from_translation(Vec3::X * -1.5).to_cols_array(); 4]).unwrap();
        frame.clear();
        frame.record(floor.resident(), Mat4::IDENTITY, camera, true, Some(light));
        frame.record_style(
            skin.resident(),
            Mat4::from_translation(Vec3::Y * 0.75),
            camera,
            true,
            DrawStyle {
                light: Some(light),
                skin: Some(&moved),
                ..Default::default()
            },
        );
        window.present_scene(&[], &[], crate::BLACK, Some(&frame));
        let changed = window.pixels();
        assert!(
            animated
                .as_chunks::<4>()
                .0
                .iter()
                .zip(changed.as_chunks::<4>().0.iter())
                .filter(|(a, b)| a != b)
                .count()
                > 100,
            "palette did not animate GPU geometry/shadows"
        );
        drop(skin);
        frame.clear();
        window.present_scene(&[], &[], crate::BLACK, Some(&frame));
        assert_eq!(
            window.retained.as_ref().unwrap().meshes.len(),
            1,
            "only the used floor survives the fresh context after skin collection"
        );
    }
    #[test]
    #[ignore = "requires a live X11 display and OpenGL context"]
    fn shared_shadow_map_and_point_lights_change_rendered_pixels() {
        use crate::{
            Mat4, Vec3,
            lighting::{Environment, PointLight, Quality, SunShadow},
            mesh::Mesh3d,
            retained::Frame,
            shaders::LitSettings,
        };
        let mut window = NativeWindow::new(&Conf {
            window_width: 192,
            window_height: 128,
            high_dpi: false,
            ..Default::default()
        })
        .unwrap();
        window.set_swap_interval(0);
        let floor = Mesh3d::plane(crate::Vec2::splat(8.), crate::WHITE, 1);
        let cube = Mesh3d::cuboid(Vec3::ONE, crate::RED);
        let eye = Vec3::new(4., 5., 6.);
        let camera = Mat4::perspective_rh_gl(0.8, 1.5, 0.1, 50.)
            * Mat4::look_at_rh(eye, Vec3::ZERO, Vec3::Y);
        let sun = SunShadow {
            direction: Vec3::new(-0.5, 1., -0.2),
            extent: 6.,
            depth: 15.,
            strength: 1.,
            ..Default::default()
        };
        let light = LitSettings {
            camera_position: eye,
            light_direction: sun.direction,
            ambient: 0.3,
            fog_start: 100.,
            fog_end: 200.,
            ..Default::default()
        }
        .sanitized();
        let mut frame = Frame::default();
        frame.record(floor.resident(), Mat4::IDENTITY, camera, true, Some(light));
        frame.record(
            cube.resident(),
            Mat4::from_translation(Vec3::Y * 0.5),
            camera,
            true,
            Some(light),
        );
        window.present_scene(&[], &[], crate::BLACK, Some(&frame));
        let lit = window.pixels();
        frame
            .environment
            .update(&[], Some(sun), Quality::High, Some(eye));
        window.present_scene(&[], &[], crate::BLACK, Some(&frame));
        let shaded = window.pixels();
        let darker = lit
            .as_chunks::<4>()
            .0
            .iter()
            .zip(shaded.as_chunks::<4>().0.iter())
            .filter(|(a, b)| {
                a[0] > 150
                    && a[0].abs_diff(a[1]) < 3
                    && a[1].abs_diff(a[2]) < 3
                    && a[0].saturating_sub(b[0]) > 40
            })
            .count();
        assert!(darker > 20, "no cast shadow found: {darker}");
        assert_eq!(
            window
                .retained
                .as_ref()
                .unwrap()
                .shadow
                .as_ref()
                .unwrap()
                .size,
            1024
        );
        frame
            .environment
            .update(&[], Some(sun), Quality::Medium, Some(eye));
        window.present_scene(&[], &[], crate::BLACK, Some(&frame));
        assert_eq!(
            window
                .retained
                .as_ref()
                .unwrap()
                .shadow
                .as_ref()
                .unwrap()
                .size,
            512
        );
        drop(cube);
        frame.clear();
        frame.environment = Environment::default();
        window.present_scene(&[], &[], crate::BLACK, Some(&frame));
        assert!(
            window.retained.as_ref().unwrap().shadow.is_none(),
            "empty Low frame must release the old shadow target"
        );
        assert_eq!(
            window.retained.as_ref().unwrap().meshes.len(),
            1,
            "empty frame must collect expired GPU meshes"
        );
        let light = LitSettings {
            light_direction: Vec3::ZERO,
            ambient: 0.1,
            ..light
        };
        frame.record(floor.resident(), Mat4::IDENTITY, camera, true, Some(light));
        window.present_scene(&[], &[], crate::BLACK, Some(&frame));
        let dark = window.pixels();
        frame.environment.update(
            &[PointLight {
                position: Vec3::new(0., 1., 0.),
                color: Color::new(1., 0., 0., 1.),
                radius: 4.,
                intensity: 2.,
            }],
            None,
            Quality::Medium,
            Some(eye),
        );
        window.present_scene(&[], &[], crate::BLACK, Some(&frame));
        let points = window.pixels();
        assert!(
            dark.as_chunks::<4>()
                .0
                .iter()
                .zip(points.as_chunks::<4>().0.iter())
                .filter(|(a, b)| b[0] > a[0].saturating_add(20) && b[0] > b[1].saturating_add(20))
                .count()
                > 50,
            "point light did not illuminate floor"
        );
    }
    #[test]
    #[ignore = "requires a live X11 display and OpenGL context"]
    fn material_alpha_and_additive_preserve_depth_and_following_ui() {
        use crate::{
            Mat4,
            material::{BlendMode, MaterialOptions, ShaderMaterial, ShaderSource},
            mesh::Mesh3d,
            retained::{Command, DrawStyle, Frame},
        };
        let mut window = NativeWindow::new(&Conf {
            window_width: 96,
            window_height: 64,
            high_dpi: false,
            ..Default::default()
        })
        .unwrap();
        window.set_swap_interval(0);
        let mesh = Mesh3d::plane(crate::Vec2::ONE, crate::WHITE, 1);
        let model = Mat4::from_rotation_x(std::f32::consts::FRAC_PI_2);
        let alpha = ShaderMaterial::new(
            ShaderSource::fragment(
                "vec4 ferravex_fragment(vec4 c,vec3 w,vec3 n){return vec4(0.0,0.0,1.0,0.5);}",
            ),
            Default::default(),
        )
        .unwrap();
        let mut frame = Frame::default();
        frame.record_style(
            mesh.resident(),
            model,
            Mat4::IDENTITY,
            true,
            DrawStyle {
                material: Some(&alpha),
                ..Default::default()
            },
        );
        let center = |window: &NativeWindow| {
            let pixels = window.pixels();
            let (w, h) = window.size();
            let at = ((h / 2 * w + w / 2) * 4) as usize;
            pixels[at..at + 4].to_vec()
        };
        window.present_scene(&[], &[], Color::new(1., 0., 0., 1.), Some(&frame));
        let p = center(&window);
        assert!((127..=129).contains(&p[0]) && (127..=129).contains(&p[2]));
        let behind = [
            [-1., -1., 0.8, 1., 0., 1., 0., 1.],
            [3., -1., 0.8, 1., 0., 1., 0., 1.],
            [-1., 3., 0.8, 1., 0., 1., 0., 1.],
        ];
        frame.commands.push(Command { kind: 0, index: 0 });
        window.present_scene(
            &behind,
            &[Batch {
                start: 0,
                count: 3,
                depth: true,
            }],
            Color::new(1., 0., 0., 1.),
            Some(&frame),
        );
        assert_eq!(
            center(&window),
            [0, 255, 0, 255],
            "alpha must not write depth"
        );
        let additive = ShaderMaterial::new(
            ShaderSource::fragment(
                "vec4 ferravex_fragment(vec4 c,vec3 w,vec3 n){return vec4(1.0,0.0,0.0,0.25);}",
            ),
            MaterialOptions {
                blend: BlendMode::Additive,
                ..Default::default()
            },
        )
        .unwrap();
        frame.clear();
        frame.record_style(
            mesh.resident(),
            model,
            Mat4::IDENTITY,
            true,
            DrawStyle {
                material: Some(&additive),
                ..Default::default()
            },
        );
        window.present_scene(&[], &[], Color::new(0., 1., 0., 1.), Some(&frame));
        let p = center(&window);
        assert!((63..=65).contains(&p[0]));
        assert_eq!(p[1], 255);
        let mut ui = behind;
        for v in &mut ui {
            v[4..8].copy_from_slice(&[0., 0., 1., 1.]);
        }
        frame.commands.push(Command { kind: 0, index: 0 });
        window.present_scene(
            &ui,
            &[Batch {
                start: 0,
                count: 3,
                depth: false,
            }],
            Color::new(0., 1., 0., 1.),
            Some(&frame),
        );
        assert_eq!(
            center(&window),
            [0, 0, 255, 255],
            "additive blend must not leak into UI"
        );
    }
    #[test]
    #[ignore = "requires a live X11 display and OpenGL context"]
    fn material_reload_keeps_last_valid_program_and_updates_vertex_hooks() {
        use crate::{
            Mat4, Vec3,
            material::{CompileStatus, MaterialOptions, ShaderMaterial, ShaderSource},
            mesh::Mesh3d,
            retained::{DrawStyle, Frame},
        };
        let mut window = NativeWindow::new(&Conf {
            window_width: 96,
            window_height: 64,
            high_dpi: false,
            ..Default::default()
        })
        .unwrap();
        window.set_swap_interval(0);
        let mesh = Mesh3d::cuboid(Vec3::ONE, crate::GREEN);
        let material = ShaderMaterial::new(
            ShaderSource::fragment(
                "vec4 ferravex_fragment(vec4 c,vec3 w,vec3 n){return u_params[0];}",
            ),
            MaterialOptions {
                blend: crate::material::BlendMode::Opaque,
                bounds_padding: 1.,
                ..Default::default()
            },
        )
        .unwrap();
        let mut frame = Frame::default();
        let mut params = [0.; 16];
        params[..4].copy_from_slice(&[1., 0., 0., 1.]);
        frame.record_style(
            mesh.resident(),
            Mat4::IDENTITY,
            Mat4::IDENTITY,
            true,
            DrawStyle {
                material: Some(&material),
                params,
                ..Default::default()
            },
        );
        let center = |window: &NativeWindow| {
            let pixels = window.pixels();
            let (w, h) = window.size();
            let at = ((h / 2 * w + w / 2) * 4) as usize;
            pixels[at..at + 4].to_vec()
        };
        window.present_scene(&[], &[], crate::BLACK, Some(&frame));
        assert_eq!(center(&window), [255, 0, 0, 255]);
        assert_eq!(material.status().status, CompileStatus::Ready);
        material
            .reload(ShaderSource::fragment("this is invalid GLSL"))
            .unwrap();
        window.present_scene(&[], &[], crate::BLACK, Some(&frame));
        assert_eq!(center(&window), [255, 0, 0, 255]);
        assert!(matches!(material.status().status, CompileStatus::Failed(_)));
        assert_eq!(material.status().last_valid_revision, Some(1));
        // A device reset must not discard the last good source after an invalid
        // edit. CPU handles and requested/valid revision identities survive.
        window.recreate().unwrap();
        window.set_swap_interval(0);
        window.present_scene(&[], &[], crate::BLACK, Some(&frame));
        assert_eq!(center(&window), [255, 0, 0, 255]);
        assert!(window.last_mesh_upload_bytes() > 0);
        assert!(matches!(material.status().status, CompileStatus::Failed(_)));
        assert_eq!(material.status().last_valid_revision, Some(1));
        material
            .reload(ShaderSource::fragment(
                "vec4 ferravex_fragment(vec4 c,vec3 w,vec3 n){return vec4(0.0,0.0,1.0,1.0);}",
            ))
            .unwrap();
        window.present_scene(&[], &[], crate::BLACK, Some(&frame));
        assert_eq!(center(&window), [0, 0, 255, 255]);
        assert_eq!(material.status().last_valid_revision, Some(3));
        material.reload(ShaderSource{vertex:"vec3 ferravex_vertex(vec3 p){return p+vec3(0.8,0.0,0.0);}\nvec3 ferravex_normal(vec3 n){return n;}".into(),..Default::default()}).unwrap();
        window.present_scene(&[], &[], crate::BLACK, Some(&frame));
        assert_eq!(center(&window), [0, 0, 0, 255]);
        assert_eq!(material.status().last_valid_revision, Some(4));
    }
    #[test]
    #[ignore = "requires a live X11 display and OpenGL context"]
    fn retained_instances_share_upload_and_preserve_ui_order() {
        use crate::{
            Mat4, Mesh, Vec2, Vec3, Vertex,
            retained::{Command, Frame, ResidentMesh},
        };
        let mut window = NativeWindow::new(&Conf {
            window_width: 96,
            window_height: 64,
            high_dpi: false,
            ..Default::default()
        })
        .unwrap();
        window.set_swap_interval(0);
        let mesh = ResidentMesh::new(&Mesh {
            vertices: vec![
                Vertex::new2(Vec3::new(-0.6, -0.6, -0.5), Vec2::ZERO, crate::GREEN),
                Vertex::new2(Vec3::new(0.6, -0.6, -0.5), Vec2::ZERO, crate::GREEN),
                Vertex::new2(Vec3::new(0., 0.6, -0.5), Vec2::ZERO, crate::GREEN),
            ],
            indices: vec![0, 1, 2],
            texture: None,
        })
        .unwrap();
        let mut frame = Frame::default();
        frame.commands.push(Command { kind: 0, index: 0 });
        for x in [-0.5, 0.5] {
            frame.record(
                &mesh,
                Mat4::from_translation(Vec3::new(x, 0., 0.)) * Mat4::from_scale(Vec3::splat(0.5)),
                Mat4::IDENTITY,
                true,
                None,
            );
        }
        frame.commands.push(Command { kind: 0, index: 1 });
        assert_eq!(frame.batches[0].count, 2);
        let vertices = [
            [-1., -1., 0.8, 1., 1., 0., 0., 1.],
            [3., -1., 0.8, 1., 1., 0., 0., 1.],
            [-1., 3., 0.8, 1., 1., 0., 0., 1.],
            [-0.15, -0.15, 0., 1., 0., 0., 1., 1.],
            [0.15, -0.15, 0., 1., 0., 0., 1., 1.],
            [0., 0.15, 0., 1., 0., 0., 1., 1.],
        ];
        let batches = [
            Batch {
                start: 0,
                count: 3,
                depth: true,
            },
            Batch {
                start: 3,
                count: 3,
                depth: false,
            },
        ];
        window.present_scene(&vertices, &batches, crate::BLACK, Some(&frame));
        assert_eq!(window.last_mesh_upload_bytes(), 120);
        window.present_scene(&vertices, &batches, crate::BLACK, Some(&frame));
        assert_eq!(window.last_mesh_upload_bytes(), 0);
        let pixels = window.pixels();
        let (w, h) = window.size();
        for x in [w / 4, w * 3 / 4] {
            let at = ((h / 2 * w + x) * 4) as usize;
            assert!(
                pixels[at + 1] > 150 && pixels[at] < 80,
                "instanced green triangle missing"
            );
        }
        let at = ((h / 2 * w + w / 2) * 4) as usize;
        assert_eq!(&pixels[at..at + 4], &[0, 0, 255, 255]);
    }
    #[test]
    #[ignore = "requires a live X11 display and OpenGL context"]
    fn shared_ui_clipping_and_paint_order_match_gpu_pixels() {
        use crate::ui::{Ui, UiRect, Viewport};
        let mut window = NativeWindow::new(&Conf {
            window_width: 96,
            window_height: 64,
            high_dpi: false,
            ..Default::default()
        })
        .unwrap();
        window.set_swap_interval(0);
        let mut ui = Ui::new();
        crate::input::host_frame(1. / 60., 96, 64, || {
            crate::set_default_camera();
            crate::draw_rectangle(0., 0., 96., 64., crate::Color::new(0., 0., 1., 1.));
            ui.begin(Viewport::fit([96., 64.], [96., 64.], [0.; 4]).unwrap());
            ui.push_clip(UiRect {
                min: [16., 16.],
                size: [32., 32.],
            })
            .unwrap();
            ui.panel(
                UiRect {
                    min: [0.; 2],
                    size: [96., 64.],
                },
                crate::Color::new(1., 0., 0., 1.),
            )
            .unwrap();
            ui.panel(
                UiRect {
                    min: [32., 32.],
                    size: [64.; 2],
                },
                crate::Color::new(0., 1., 0., 1.),
            )
            .unwrap();
            ui.finish(false);
            ui.paint();
        });
        crate::read(|s| {
            window.present_scene(&s.vertices, &s.batches, crate::BLACK, Some(&s.retained))
        });
        let pixels = window.pixels();
        let (w, _) = window.size();
        let pixel = |x: usize, y: usize| {
            let at = (y * w as usize + x) * 4;
            pixels[at..at + 3].to_vec()
        };
        assert_eq!(pixel(8, 8), vec![0, 0, 255]);
        assert_eq!(pixel(20, 20), vec![255, 0, 0]);
        assert_eq!(pixel(40, 40), vec![0, 255, 0]);
        assert_eq!(pixel(56, 40), vec![0, 0, 255]);
    }
    #[test]
    #[ignore = "requires a live X11 display and OpenGL context"]
    fn native_gpu_depth_and_readback() {
        let conf = Conf {
            window_title: "Ferravex native GPU verification".into(),
            window_width: 96,
            window_height: 64,
            high_dpi: false,
            sample_count: 0,
        };
        let mut window = NativeWindow::new(&conf).expect("create native window");
        window.set_swap_interval(0);
        println!("{}", window.driver_info());
        let vertices = [
            [-1.0, -1.0, 0.7, 1.0, 1.0, 0.0, 0.0, 1.0],
            [3.0, -1.0, 0.7, 1.0, 1.0, 0.0, 0.0, 1.0],
            [-1.0, 3.0, 0.7, 1.0, 1.0, 0.0, 0.0, 1.0],
            [-0.8, -0.8, -0.5, 1.0, 0.0, 1.0, 0.0, 1.0],
            [0.8, -0.8, -0.5, 1.0, 0.0, 1.0, 0.0, 1.0],
            [0.0, 0.8, -0.5, 1.0, 0.0, 1.0, 0.0, 1.0],
        ];
        let batches = [
            Batch {
                start: 3,
                count: 3,
                depth: true,
            },
            Batch {
                start: 0,
                count: 3,
                depth: true,
            },
        ];
        window.poll();
        window.present(&vertices, &batches, Color::new(0.0, 0.0, 0.0, 1.0));
        let (width, height) = window.size();
        let pixels = window.pixels();
        assert_eq!(pixels.len(), (width * height * 4) as usize);
        let center = ((height / 2 * width + width / 2) * 4) as usize;
        assert_eq!(
            &pixels[center..center + 4],
            &[0, 255, 0, 255],
            "near triangle must occlude later far triangle"
        );
        let corner = ((2 * width + 2) * 4) as usize;
        assert_eq!(&pixels[corner..corner + 4], &[255, 0, 0, 255]);
        window.set_cursor(true, false);
        window.poll();
        window.set_cursor(false, true);
        if let Ok(path) = std::env::var("FERRAVEX_NATIVE_SCREENSHOT") {
            let mut frame = ferravex_graphics::Framebuffer::new(width, height).unwrap();
            for (destination, source) in
                frame.pixels_mut().iter_mut().zip(pixels.as_chunks::<4>().0)
            {
                destination.copy_from_slice(source);
            }
            frame.save_png(path).unwrap();
        }
    }
    #[test]
    #[ignore = "requires a live X11 display and OpenGL context"]
    fn timer_results_preserve_frame_tags_and_render_counts() {
        let mut window = NativeWindow::new(&Conf {
            window_width: 96,
            window_height: 64,
            high_dpi: false,
            ..Default::default()
        })
        .unwrap();
        window.set_swap_interval(0);
        assert!(window.enable_gpu_timing());
        for i in 0..4 {
            if i == 2 {
                window.recreate().unwrap();
            }
            window.present(&[], &[], crate::BLACK);
        }
        window.finish_gpu_timing();
        let mut results = Vec::new();
        window.drain_gpu_results(|frame, us| results.push((frame, us)));
        assert_eq!(
            results.iter().map(|r| r.0).collect::<Vec<_>>(),
            [1, 2, 3, 4]
        );
        assert!(results.iter().all(|r| r.1.is_finite() && r.1 >= 0.));
        assert_eq!(window.render_counts(), (0, 0));
    }
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires a live X11 display and OpenGL context"]
    fn high_dpi_preserves_logical_coordinates_and_physical_readback() {
        if std::env::var_os("FERRAVEX_DPI_TEST_CHILD").is_none() {
            let result=std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","native::gpu::tests::high_dpi_preserves_logical_coordinates_and_physical_readback","--ignored"]).env("FERRAVEX_DPI_TEST_CHILD","1").env("FERRAVEX_DPI_SCALE","2").output().unwrap();
            assert!(
                result.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&result.stdout),
                String::from_utf8_lossy(&result.stderr)
            );
            return;
        }
        let mut window = NativeWindow::new(&Conf {
            window_width: 96,
            window_height: 64,
            high_dpi: true,
            ..Default::default()
        })
        .unwrap();
        window.set_swap_interval(0);
        assert_eq!(window.size(), (96, 64));
        assert_eq!(window.framebuffer_size(), (192, 128));
        assert_eq!(window.dpi_scale(), 2.);
        crate::input::host_frame(1. / 60., 96, 64, || {
            crate::clear_background(crate::BLUE);
            crate::draw_rectangle(0., 0., 16., 16., crate::RED);
        });
        crate::read(|s| window.present_scene(&s.vertices, &s.batches, s.clear, Some(&s.retained)));
        let pixels = window.pixels();
        assert_eq!(pixels.len(), 192 * 128 * 4);
        let pixel = |x: usize, y: usize| &pixels[(y * 192 + x) * 4..(y * 192 + x) * 4 + 4];
        assert!(
            pixel(24, 24)
                .iter()
                .zip(crate::RED.to_rgba())
                .all(|(a, b)| a.abs_diff(b) <= 1)
        );
        assert!(
            pixel(40, 40)
                .iter()
                .zip(crate::BLUE.to_rgba())
                .all(|(a, b)| a.abs_diff(b) <= 1)
        );
    }
    #[test]
    fn screenshot_rows_flip_without_channel_or_center_row_changes() {
        let mut bytes = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
        flip_rows(&mut bytes, 4, 3);
        assert_eq!(bytes, [9, 10, 11, 12, 5, 6, 7, 8, 1, 2, 3, 4]);
    }
    #[test]
    #[ignore = "requires a live X11 display and OpenGL context"]
    fn world_palette_matches_cpu_reference() {
        use crate::{
            Color, Look, Mat4, Vec3,
            mesh::Mesh3d,
            retained::{DrawStyle, Frame},
        };
        let mut window = NativeWindow::new(&Conf {
            window_width: 96,
            window_height: 64,
            high_dpi: false,
            ..Default::default()
        })
        .unwrap();
        window.set_swap_interval(0);
        let color = Color::new(0.8, 0.5, 0.2, 1.);
        let mesh = Mesh3d::cuboid(Vec3::ONE, color);
        for index in 0..4 {
            let look = Look::from_index(index).unwrap();
            let mut frame = Frame::default();
            frame.record_style(
                mesh.resident(),
                Mat4::IDENTITY,
                Mat4::IDENTITY,
                true,
                DrawStyle {
                    look,
                    ..Default::default()
                },
            );
            window.present_scene(&[], &[], crate::BLACK, Some(&frame));
            let pixels = window.pixels();
            let at = (32 * 96 + 48) * 4;
            for (actual, expected) in pixels[at..at + 4].iter().zip(look.shade(color).to_rgba()) {
                assert!(
                    actual.abs_diff(expected) <= 1,
                    "look {look:?}: {actual} != {expected}"
                );
            }
        }
    }
}
