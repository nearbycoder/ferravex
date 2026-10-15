//! Original native platform bridge. Linux uses the system X11/GLX/OpenGL APIs directly.
//! Vertices are homogeneous clip-space XYZW followed by straight RGBA; each batch is triangles.
//! A single streamed GPU vertex buffer and shader program persist for the window lifetime.
//! No third-party Rust crate or windowing implementation is used here.
use crate::{Batch, Color, Conf};

/// Keyboard values are unshifted X11 keysyms (ASCII lowercase for letter keys).
/// Mouse buttons use X11 numbering: 1 left, 2 middle, 3 right. Wheel events are ±1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    Key(u32, bool),
    MouseMove(f32, f32),
    MouseDelta(f32, f32),
    MouseButton(u32, bool),
    Wheel(f32),
    Resize(u32, u32),
    Focus(bool),
    Quit,
}

#[cfg(target_os = "linux")]
pub use linux::NativeWindow;

#[cfg(not(target_os = "linux"))]
pub struct NativeWindow;
#[cfg(not(target_os = "linux"))]
impl NativeWindow {
    pub fn new(_: &Conf) -> Result<Self, String> {
        Err("The original native renderer currently supports Linux/X11; this platform backend is not implemented".into())
    }
    pub fn size(&self) -> (u32, u32) {
        (0, 0)
    }
    pub fn poll(&mut self) -> Vec<Event> {
        vec![Event::Quit]
    }
    pub fn present(&mut self, _: &[[f32; 8]], _: &[Batch], _: Color) {}
    pub(crate) fn present_scene(
        &mut self,
        _: &[[f32; 8]],
        _: &[Batch],
        _: Color,
        _: Option<&crate::retained::Frame>,
    ) {
    }
    pub fn last_mesh_upload_bytes(&self) -> u64 {
        0
    }
    pub fn skin_upload_bytes(&self) -> u64 {
        0
    }
    pub fn enable_gpu_timing(&mut self) -> bool {
        false
    }
    pub fn drain_gpu_results(&mut self, _: impl FnMut(u64, f64)) {}
    pub fn finish_gpu_timing(&mut self) {}
    pub fn render_counts(&self) -> (u64, u64) {
        (0, 0)
    }
    pub fn pixels(&self) -> Vec<u8> {
        Vec::new()
    }
    pub fn set_cursor(&mut self, _: bool, _: bool) {}
    pub fn set_swap_interval(&self, _: i32) {}
    pub fn driver_info(&self) -> String {
        "unsupported".into()
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};
    use std::ffi::{CStr, CString, c_char, c_int, c_long, c_short, c_uint, c_ulong, c_void};
    use std::ptr;

    const KEY_PRESS: c_int = 2;
    const KEY_RELEASE: c_int = 3;
    const BUTTON_PRESS: c_int = 4;
    const BUTTON_RELEASE: c_int = 5;
    const MOTION_NOTIFY: c_int = 6;
    const FOCUS_IN: c_int = 9;
    const FOCUS_OUT: c_int = 10;
    const DESTROY_NOTIFY: c_int = 17;
    const CONFIGURE_NOTIFY: c_int = 22;
    const CLIENT_MESSAGE: c_int = 33;
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

    // These repr(C) records describe the public Xlib ABI, not an implementation of Xlib.
    type Display = c_void;
    type Visual = c_void;
    type GlxContext = *mut c_void;
    #[repr(C)]
    struct XVisualInfo {
        visual: *mut Visual,
        visualid: c_ulong,
        screen: c_int,
        depth: c_int,
        class: c_int,
        red_mask: c_ulong,
        green_mask: c_ulong,
        blue_mask: c_ulong,
        colormap_size: c_int,
        bits_per_rgb: c_int,
    }
    #[repr(C)]
    #[derive(Default)]
    struct XSetWindowAttributes {
        background_pixmap: c_ulong,
        background_pixel: c_ulong,
        border_pixmap: c_ulong,
        border_pixel: c_ulong,
        bit_gravity: c_int,
        win_gravity: c_int,
        backing_store: c_int,
        backing_planes: c_ulong,
        backing_pixel: c_ulong,
        save_under: c_int,
        event_mask: c_long,
        do_not_propagate_mask: c_long,
        override_redirect: c_int,
        colormap: c_ulong,
        cursor: c_ulong,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct XKeyEvent {
        kind: c_int,
        serial: c_ulong,
        send_event: c_int,
        display: *mut Display,
        window: c_ulong,
        root: c_ulong,
        subwindow: c_ulong,
        time: c_ulong,
        x: c_int,
        y: c_int,
        x_root: c_int,
        y_root: c_int,
        state: c_uint,
        keycode: c_uint,
        same_screen: c_int,
    }
    // XButtonEvent has the same layout as XKeyEvent; its detail field is a button number.
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct XMotionEvent {
        kind: c_int,
        serial: c_ulong,
        send_event: c_int,
        display: *mut Display,
        window: c_ulong,
        root: c_ulong,
        subwindow: c_ulong,
        time: c_ulong,
        x: c_int,
        y: c_int,
        x_root: c_int,
        y_root: c_int,
        state: c_uint,
        is_hint: c_char,
        same_screen: c_int,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct XConfigureEvent {
        kind: c_int,
        serial: c_ulong,
        send_event: c_int,
        display: *mut Display,
        event: c_ulong,
        window: c_ulong,
        x: c_int,
        y: c_int,
        width: c_int,
        height: c_int,
        border_width: c_int,
        above: c_ulong,
        override_redirect: c_int,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    union ClientData {
        bytes: [c_char; 20],
        shorts: [c_short; 10],
        longs: [c_long; 5],
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct XClientMessageEvent {
        kind: c_int,
        serial: c_ulong,
        send_event: c_int,
        display: *mut Display,
        window: c_ulong,
        message_type: c_ulong,
        format: c_int,
        data: ClientData,
    }
    #[repr(C)]
    struct XColor {
        pixel: c_ulong,
        red: u16,
        green: u16,
        blue: u16,
        flags: c_char,
        pad: c_char,
    }
    #[repr(C)]
    union XEvent {
        kind: c_int,
        key: XKeyEvent,
        motion: XMotionEvent,
        configure: XConfigureEvent,
        client: XClientMessageEvent,
        padding: [c_long; 24],
    }
    impl XEvent {
        fn empty() -> Self {
            Self { padding: [0; 24] }
        }
    }

    #[link(name = "X11")]
    unsafe extern "C" {
        fn XOpenDisplay(name: *const c_char) -> *mut Display;
        fn XCloseDisplay(display: *mut Display) -> c_int;
        fn XDefaultScreen(display: *mut Display) -> c_int;
        fn XRootWindow(display: *mut Display, screen: c_int) -> c_ulong;
        fn XCreateColormap(
            display: *mut Display,
            window: c_ulong,
            visual: *mut Visual,
            allocation: c_int,
        ) -> c_ulong;
        fn XFreeColormap(display: *mut Display, colormap: c_ulong) -> c_int;
        fn XCreateWindow(
            display: *mut Display,
            parent: c_ulong,
            x: c_int,
            y: c_int,
            width: c_uint,
            height: c_uint,
            border: c_uint,
            depth: c_int,
            class: c_uint,
            visual: *mut Visual,
            mask: c_ulong,
            attributes: *mut XSetWindowAttributes,
        ) -> c_ulong;
        fn XDestroyWindow(display: *mut Display, window: c_ulong) -> c_int;
        fn XStoreName(display: *mut Display, window: c_ulong, name: *const c_char) -> c_int;
        fn XInternAtom(
            display: *mut Display,
            name: *const c_char,
            only_if_exists: c_int,
        ) -> c_ulong;
        fn XSetWMProtocols(
            display: *mut Display,
            window: c_ulong,
            protocols: *mut c_ulong,
            count: c_int,
        ) -> c_int;
        fn XMapWindow(display: *mut Display, window: c_ulong) -> c_int;
        fn XFlush(display: *mut Display) -> c_int;
        fn XPending(display: *mut Display) -> c_int;
        fn XNextEvent(display: *mut Display, event: *mut XEvent) -> c_int;
        fn XPeekEvent(display: *mut Display, event: *mut XEvent) -> c_int;
        fn XLookupKeysym(event: *mut XKeyEvent, index: c_int) -> c_ulong;
        fn XFree(data: *mut c_void) -> c_int;
        fn XGrabPointer(
            display: *mut Display,
            window: c_ulong,
            owner_events: c_int,
            event_mask: c_uint,
            pointer_mode: c_int,
            keyboard_mode: c_int,
            confine_to: c_ulong,
            cursor: c_ulong,
            time: c_ulong,
        ) -> c_int;
        fn XUngrabPointer(display: *mut Display, time: c_ulong) -> c_int;
        fn XWarpPointer(
            display: *mut Display,
            source: c_ulong,
            destination: c_ulong,
            source_x: c_int,
            source_y: c_int,
            source_width: c_uint,
            source_height: c_uint,
            destination_x: c_int,
            destination_y: c_int,
        ) -> c_int;
        fn XCreateBitmapFromData(
            display: *mut Display,
            drawable: c_ulong,
            data: *const c_char,
            width: c_uint,
            height: c_uint,
        ) -> c_ulong;
        fn XCreatePixmapCursor(
            display: *mut Display,
            source: c_ulong,
            mask: c_ulong,
            foreground: *mut XColor,
            background: *mut XColor,
            x: c_uint,
            y: c_uint,
        ) -> c_ulong;
        fn XFreePixmap(display: *mut Display, pixmap: c_ulong) -> c_int;
        fn XFreeCursor(display: *mut Display, cursor: c_ulong) -> c_int;
        fn XDefineCursor(display: *mut Display, window: c_ulong, cursor: c_ulong) -> c_int;
        fn XUndefineCursor(display: *mut Display, window: c_ulong) -> c_int;
    }
    #[link(name = "GL")]
    unsafe extern "C" {
        fn glXChooseVisual(
            display: *mut Display,
            screen: c_int,
            attributes: *mut c_int,
        ) -> *mut XVisualInfo;
        fn glXCreateContext(
            display: *mut Display,
            visual: *mut XVisualInfo,
            shared: GlxContext,
            direct: c_int,
        ) -> GlxContext;
        fn glXMakeCurrent(display: *mut Display, drawable: c_ulong, context: GlxContext) -> c_int;
        fn glXDestroyContext(display: *mut Display, context: GlxContext);
        fn glXSwapBuffers(display: *mut Display, drawable: c_ulong);
        fn glXGetProcAddressARB(name: *const u8) -> Option<unsafe extern "C" fn()>;
        fn glXQueryExtensionsString(display: *mut Display, screen: c_int) -> *const c_char;
        fn glViewport(x: c_int, y: c_int, width: c_int, height: c_int);
        fn glClearColor(r: f32, g: f32, b: f32, a: f32);
        fn glClearDepth(depth: f64);
        fn glClear(mask: c_uint);
        fn glEnable(capability: c_uint);
        fn glDisable(capability: c_uint);
        fn glDepthFunc(function: c_uint);
        fn glDepthMask(enabled: u8);
        fn glBlendFuncSeparate(
            source_rgb: c_uint,
            destination_rgb: c_uint,
            source_alpha: c_uint,
            destination_alpha: c_uint,
        );
        fn glCreateShader(kind: c_uint) -> c_uint;
        fn glShaderSource(
            shader: c_uint,
            count: c_int,
            source: *const *const c_char,
            length: *const c_int,
        );
        fn glCompileShader(shader: c_uint);
        fn glGetShaderiv(shader: c_uint, parameter: c_uint, value: *mut c_int);
        fn glGetShaderInfoLog(shader: c_uint, size: c_int, length: *mut c_int, log: *mut c_char);
        fn glDeleteShader(shader: c_uint);
        fn glCreateProgram() -> c_uint;
        fn glAttachShader(program: c_uint, shader: c_uint);
        fn glBindAttribLocation(program: c_uint, index: c_uint, name: *const c_char);
        fn glLinkProgram(program: c_uint);
        fn glGetProgramiv(program: c_uint, parameter: c_uint, value: *mut c_int);
        fn glGetProgramInfoLog(program: c_uint, size: c_int, length: *mut c_int, log: *mut c_char);
        fn glDeleteProgram(program: c_uint);
        fn glUseProgram(program: c_uint);
        fn glGenBuffers(count: c_int, buffers: *mut c_uint);
        fn glDeleteBuffers(count: c_int, buffers: *const c_uint);
        fn glBindBuffer(target: c_uint, buffer: c_uint);
        fn glBufferData(target: c_uint, bytes: isize, data: *const c_void, usage: c_uint);
        fn glBufferSubData(target: c_uint, offset: isize, bytes: isize, data: *const c_void);
        fn glEnableVertexAttribArray(index: c_uint);
        fn glDisableVertexAttribArray(index: c_uint);
        fn glVertexAttribPointer(
            index: c_uint,
            components: c_int,
            kind: c_uint,
            normalized: u8,
            stride: c_int,
            offset: *const c_void,
        );
        fn glDrawArrays(mode: c_uint, first: c_int, count: c_int);
        fn glDrawArraysInstanced(mode: c_uint, first: c_int, count: c_int, instances: c_int);
        fn glVertexAttribDivisor(index: c_uint, divisor: c_uint);
        fn glGetUniformLocation(program: c_uint, name: *const c_char) -> c_int;
        fn glUniformMatrix4fv(location: c_int, count: c_int, transpose: u8, values: *const f32);
        fn glUniformMatrix3fv(location: c_int, count: c_int, transpose: u8, values: *const f32);
        fn glGenQueries(count: c_int, queries: *mut c_uint);
        fn glDeleteQueries(count: c_int, queries: *const c_uint);
        fn glBeginQuery(target: c_uint, query: c_uint);
        fn glEndQuery(target: c_uint);
        fn glGetQueryObjectiv(query: c_uint, parameter: c_uint, value: *mut c_int);
        fn glGetQueryObjectui64v(query: c_uint, parameter: c_uint, value: *mut u64);
        fn glUniform1i(location: c_int, value: c_int);
        fn glGenTextures(count: c_int, textures: *mut c_uint);
        fn glDeleteTextures(count: c_int, textures: *const c_uint);
        fn glBindTexture(target: c_uint, texture: c_uint);
        fn glActiveTexture(texture: c_uint);
        fn glTexParameteri(target: c_uint, parameter: c_uint, value: c_int);
        fn glTexImage2D(
            target: c_uint,
            level: c_int,
            internal: c_int,
            width: c_int,
            height: c_int,
            border: c_int,
            format: c_uint,
            kind: c_uint,
            data: *const c_void,
        );
        fn glGenFramebuffers(count: c_int, framebuffers: *mut c_uint);
        fn glDeleteFramebuffers(count: c_int, framebuffers: *const c_uint);
        fn glBindFramebuffer(target: c_uint, framebuffer: c_uint);
        fn glFramebufferTexture2D(
            target: c_uint,
            attachment: c_uint,
            texture_target: c_uint,
            texture: c_uint,
            level: c_int,
        );
        fn glCheckFramebufferStatus(target: c_uint) -> c_uint;
        fn glDrawBuffer(buffer: c_uint);
        fn glUniform4fv(location: c_int, count: c_int, values: *const f32);
        fn glReadBuffer(buffer: c_uint);
        fn glPixelStorei(parameter: c_uint, value: c_int);
        fn glReadPixels(
            x: c_int,
            y: c_int,
            width: c_int,
            height: c_int,
            format: c_uint,
            kind: c_uint,
            pixels: *mut c_void,
        );
        fn glFinish();
        fn glGetString(name: c_uint) -> *const u8;
    }

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
    pub struct NativeWindow {
        display: *mut Display,
        window: c_ulong,
        colormap: c_ulong,
        context: GlxContext,
        program: c_uint,
        vertex_buffer: c_uint,
        retained: Option<RetainedGpu>,
        last_commands: Vec<crate::retained::Command>,
        mesh_upload_bytes: u64,
        gpu_timer: Option<GpuTimer>,
        present_count: u64,
        width: u32,
        height: u32,
        delete_atom: c_ulong,
        protocols_atom: c_ulong,
        alive: bool,
        quit_sent: bool,
        focused: bool,
        grabbed: bool,
        cursor_visible: bool,
        hidden_cursor: c_ulong,
        relative_position: (i32, i32),
        warp_pending: bool,
        last_clear: Color,
        last_batches: Vec<(i32, i32, bool)>,
        presented: bool,
        keys: BTreeMap<u32, u32>,
        buttons: BTreeSet<u32>,
    }
    impl NativeWindow {
        pub fn new(conf: &Conf) -> Result<Self, String> {
            if !(1..=16384).contains(&conf.window_width)
                || !(1..=16384).contains(&conf.window_height)
            {
                return Err("Window dimensions must be between 1 and 16384 pixels".into());
            }
            // SAFETY: Each system allocation is retained in the owner below and released in Drop.
            // Xlib structs have their public C ABI layout; current-thread GL ownership is preserved.
            unsafe {
                let display = XOpenDisplay(ptr::null());
                if display.is_null() {
                    return Err("Cannot open X11 display; set DISPLAY to a running X server".into());
                }
                let mut owner = Self {
                    display,
                    window: 0,
                    colormap: 0,
                    context: ptr::null_mut(),
                    program: 0,
                    vertex_buffer: 0,
                    retained: None,
                    last_commands: Vec::new(),
                    mesh_upload_bytes: 0,
                    gpu_timer: None,
                    present_count: 0,
                    width: conf.window_width as u32,
                    height: conf.window_height as u32,
                    delete_atom: 0,
                    protocols_atom: 0,
                    alive: false,
                    quit_sent: false,
                    focused: true,
                    grabbed: false,
                    cursor_visible: true,
                    hidden_cursor: 0,
                    relative_position: (0, 0),
                    warp_pending: false,
                    last_clear: Color::new(0.0, 0.0, 0.0, 1.0),
                    last_batches: Vec::new(),
                    presented: false,
                    keys: BTreeMap::new(),
                    buttons: BTreeSet::new(),
                };
                let screen = XDefaultScreen(display);
                // GLX_RGBA, DOUBLEBUFFER, RGB/A channel sizes and a 24-bit depth attachment.
                let base = [4, 5, 8, 8, 9, 8, 10, 8, 11, 8, 12, 24];
                let mut attributes = base.to_vec();
                if conf.sample_count > 1 {
                    attributes.extend([100000, 1, 100001, conf.sample_count.min(16)]);
                }
                attributes.push(0);
                let mut visual = glXChooseVisual(display, screen, attributes.as_mut_ptr());
                if visual.is_null() && conf.sample_count > 1 {
                    attributes = base.to_vec();
                    attributes.push(0);
                    visual = glXChooseVisual(display, screen, attributes.as_mut_ptr());
                }
                if visual.is_null() {
                    return Err("No double-buffered RGBA/depth GLX visual is available".into());
                }
                let root = XRootWindow(display, screen);
                owner.colormap = XCreateColormap(display, root, (*visual).visual, 0);
                let mut window_attributes = XSetWindowAttributes {
                    colormap: owner.colormap,
                    event_mask: (1 << 0)
                        | (1 << 1)
                        | (1 << 2)
                        | (1 << 3)
                        | (1 << 6)
                        | (1 << 15)
                        | (1 << 17)
                        | (1 << 21),
                    ..Default::default()
                };
                owner.window = XCreateWindow(
                    display,
                    root,
                    0,
                    0,
                    owner.width,
                    owner.height,
                    0,
                    (*visual).depth,
                    1,
                    (*visual).visual,
                    (1 << 3) | (1 << 11) | (1 << 13),
                    &mut window_attributes,
                );
                owner.alive = owner.window != 0;
                if !owner.alive {
                    XFree(visual.cast());
                    return Err("X11 window creation failed".into());
                }
                owner.context = glXCreateContext(display, visual, ptr::null_mut(), 1);
                XFree(visual.cast());
                if owner.context.is_null() {
                    return Err("GLX could not create an OpenGL context".into());
                }
                let title = CString::new(conf.window_title.replace('\0', " "))
                    .map_err(|e| e.to_string())?;
                XStoreName(display, owner.window, title.as_ptr());
                owner.delete_atom = XInternAtom(display, c"WM_DELETE_WINDOW".as_ptr(), 0);
                owner.protocols_atom = XInternAtom(display, c"WM_PROTOCOLS".as_ptr(), 0);
                XSetWMProtocols(display, owner.window, &mut owner.delete_atom, 1);
                XMapWindow(display, owner.window);
                XFlush(display);
                if glXMakeCurrent(display, owner.window, owner.context) == 0 {
                    return Err("Cannot make the GLX context current".into());
                }
                owner.program = create_program()?;
                glGenBuffers(1, &mut owner.vertex_buffer);
                if owner.vertex_buffer == 0 {
                    return Err("OpenGL could not allocate a vertex buffer".into());
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
                glClearDepth(1.0);
                owner.set_swap_interval(1);
                Ok(owner)
            }
        }
        pub fn size(&self) -> (u32, u32) {
            (self.width, self.height)
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
        /// Requests a nonnegative swap interval when an installed GLX extension provides it.
        pub fn set_swap_interval(&mut self, interval: i32) {
            if interval < 0 || !self.alive {
                return;
            }
            // SAFETY: Procedure signatures are the published GLX extension ABI. A null address
            // leaves the driver default unchanged; no unvalidated pointer is called.
            unsafe {
                if !self.make_current() {
                    return;
                }
                let extensions =
                    glXQueryExtensionsString(self.display, XDefaultScreen(self.display));
                if extensions.is_null() {
                    return;
                }
                let extensions = CStr::from_ptr(extensions).to_string_lossy();
                if extensions
                    .split_whitespace()
                    .any(|name| name == "GLX_EXT_swap_control")
                {
                    if let Some(address) =
                        glXGetProcAddressARB(c"glXSwapIntervalEXT".as_ptr().cast())
                    {
                        let call = std::mem::transmute::<
                            unsafe extern "C" fn(),
                            unsafe extern "C" fn(*mut Display, c_ulong, c_int),
                        >(address);
                        call(self.display, self.window, interval);
                    }
                } else if extensions
                    .split_whitespace()
                    .any(|name| name == "GLX_MESA_swap_control")
                    && let Some(address) =
                        glXGetProcAddressARB(c"glXSwapIntervalMESA".as_ptr().cast())
                {
                    let call = std::mem::transmute::<
                        unsafe extern "C" fn(),
                        unsafe extern "C" fn(c_uint) -> c_int,
                    >(address);
                    call(interval as c_uint);
                }
            }
        }
        /// Idempotent cursor control. A successful grab emits relative MouseDelta events.
        /// Failed grabs are retried on later calls, and focus loss releases confinement.
        pub fn set_cursor(&mut self, grab: bool, visible: bool) {
            if !self.alive {
                return;
            }
            // SAFETY: Cursor/window handles belong to this connection. The one-bit empty
            // bitmap creates a transparent cursor; the temporary pixmap is released immediately.
            unsafe {
                if visible != self.cursor_visible {
                    if visible {
                        XUndefineCursor(self.display, self.window);
                    } else {
                        if self.hidden_cursor == 0 {
                            let blank = [0_i8];
                            let pixmap = XCreateBitmapFromData(
                                self.display,
                                self.window,
                                blank.as_ptr(),
                                1,
                                1,
                            );
                            if pixmap != 0 {
                                let mut foreground = XColor {
                                    pixel: 0,
                                    red: 0,
                                    green: 0,
                                    blue: 0,
                                    flags: 0,
                                    pad: 0,
                                };
                                let mut background = XColor {
                                    pixel: 0,
                                    red: 0,
                                    green: 0,
                                    blue: 0,
                                    flags: 0,
                                    pad: 0,
                                };
                                self.hidden_cursor = XCreatePixmapCursor(
                                    self.display,
                                    pixmap,
                                    pixmap,
                                    &mut foreground,
                                    &mut background,
                                    0,
                                    0,
                                );
                                XFreePixmap(self.display, pixmap);
                            }
                        }
                        if self.hidden_cursor != 0 {
                            XDefineCursor(self.display, self.window, self.hidden_cursor);
                        }
                    }
                    self.cursor_visible = visible;
                }
                let desired = grab && self.focused;
                if !desired && self.grabbed {
                    XUngrabPointer(self.display, 0);
                    self.grabbed = false;
                    self.warp_pending = false;
                } else if desired
                    && !self.grabbed
                    && XGrabPointer(
                        self.display,
                        self.window,
                        0,
                        (1 << 2) | (1 << 3) | (1 << 6),
                        1,
                        1,
                        self.window,
                        0,
                        0,
                    ) == 0
                {
                    self.grabbed = true;
                    self.relative_position = self.center();
                    self.warp_to_center();
                }
                XFlush(self.display);
            }
        }
        fn center(&self) -> (i32, i32) {
            ((self.width / 2) as i32, (self.height / 2) as i32)
        }
        fn warp_to_center(&mut self) {
            let (x, y) = self.center();
            // SAFETY: Warp targets the owned, live window with coordinates inside its bounds.
            unsafe {
                XWarpPointer(self.display, 0, self.window, 0, 0, 0, 0, x, y);
                XFlush(self.display);
            }
            self.warp_pending = true;
        }
        pub fn poll(&mut self) -> Vec<Event> {
            let mut output = Vec::new();
            if !self.alive {
                self.queue_quit(&mut output);
                return output;
            }
            // SAFETY: XNextEvent fills a correctly sized XEvent. Its tag selects the union field.
            // Reads from XPeekEvent happen only when an event is available.
            unsafe {
                for _ in 0..4096 {
                    if XPending(self.display) <= 0 {
                        break;
                    }
                    let mut event = XEvent::empty();
                    XNextEvent(self.display, &mut event);
                    match event.kind {
                        KEY_PRESS => {
                            let mut key = event.key;
                            let symbol = XLookupKeysym(&mut key, 0) as u32;
                            if let std::collections::btree_map::Entry::Vacant(entry) =
                                self.keys.entry(key.keycode)
                            {
                                entry.insert(symbol);
                                output.push(Event::Key(symbol, true));
                            }
                        }
                        KEY_RELEASE => {
                            let key = event.key;
                            // Classic X11 autorepeat synthesizes a release/press pair with the
                            // same hardware key and timestamp. Retain held state for that pair.
                            let mut next = XEvent::empty();
                            if XPending(self.display) > 0 {
                                XPeekEvent(self.display, &mut next);
                                if next.kind == KEY_PRESS
                                    && next.key.keycode == key.keycode
                                    && next.key.time == key.time
                                {
                                    continue;
                                }
                            }
                            if let Some(symbol) = self.keys.remove(&key.keycode) {
                                output.push(Event::Key(symbol, false));
                            }
                        }
                        BUTTON_PRESS | BUTTON_RELEASE => {
                            let button = event.key;
                            let down = event.kind == BUTTON_PRESS;
                            // A captured pointer is recentered; those absolute coordinates must
                            // not enter the runtime's absolute-to-relative motion calculation.
                            if !self.grabbed {
                                output.push(Event::MouseMove(button.x as f32, button.y as f32));
                            }
                            match button.keycode {
                                4 if down => output.push(Event::Wheel(1.0)),
                                5 if down => output.push(Event::Wheel(-1.0)),
                                4..=7 => {}
                                code => {
                                    let changed = if down {
                                        self.buttons.insert(code)
                                    } else {
                                        self.buttons.remove(&code)
                                    };
                                    if changed {
                                        output.push(Event::MouseButton(code, down));
                                    }
                                }
                            }
                        }
                        MOTION_NOTIFY => {
                            let position = (event.motion.x, event.motion.y);
                            if self.grabbed {
                                if self.warp_pending && position == self.center() {
                                    self.warp_pending = false;
                                    self.relative_position = position;
                                } else {
                                    let delta = (
                                        position.0 - self.relative_position.0,
                                        position.1 - self.relative_position.1,
                                    );
                                    self.relative_position = position;
                                    if delta != (0, 0) {
                                        output.push(Event::MouseDelta(
                                            delta.0 as f32,
                                            delta.1 as f32,
                                        ));
                                    }
                                }
                            } else {
                                output.push(Event::MouseMove(position.0 as f32, position.1 as f32));
                            }
                        }
                        CONFIGURE_NOTIFY => {
                            let configure = event.configure;
                            let size = (
                                configure.width.max(1) as u32,
                                configure.height.max(1) as u32,
                            );
                            if size != self.size() {
                                self.width = size.0;
                                self.height = size.1;
                                output.push(Event::Resize(size.0, size.1));
                            }
                        }
                        FOCUS_IN => {
                            self.focused = true;
                            output.push(Event::Focus(true));
                        }
                        FOCUS_OUT => {
                            self.focused = false;
                            output.push(Event::Focus(false));
                            if self.grabbed {
                                XUngrabPointer(self.display, 0);
                                self.grabbed = false;
                                self.warp_pending = false;
                            }
                            for (_, symbol) in std::mem::take(&mut self.keys) {
                                output.push(Event::Key(symbol, false));
                            }
                            for button in std::mem::take(&mut self.buttons) {
                                output.push(Event::MouseButton(button, false));
                            }
                        }
                        CLIENT_MESSAGE => {
                            let message = event.client;
                            if message.message_type == self.protocols_atom
                                && message.format == 32
                                && message.data.longs[0] as c_ulong == self.delete_atom
                            {
                                self.queue_quit(&mut output);
                            }
                        }
                        DESTROY_NOTIFY => {
                            self.alive = false;
                            self.queue_quit(&mut output);
                        }
                        _ => {}
                    }
                }
            }
            if self.alive && self.grabbed && self.relative_position != self.center() {
                self.warp_to_center();
            }
            output
        }
        fn queue_quit(&mut self, events: &mut Vec<Event>) {
            if !self.quit_sent {
                self.quit_sent = true;
                events.push(Event::Quit);
            }
        }
        fn make_current(&self) -> bool {
            // SAFETY: All three handles are owned and live while alive is true.
            unsafe {
                self.alive
                    && !self.context.is_null()
                    && glXMakeCurrent(self.display, self.window, self.context) != 0
            }
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
            // SAFETY: The live, current drawable owns the completed back buffer.
            unsafe {
                glXSwapBuffers(self.display, self.window);
            }
            self.presented = true;
        }
        fn upload_retained(&mut self, frame: &crate::retained::Frame) {
            if frame.batches.is_empty() && crate::material::count() == 0 && self.retained.is_none()
            {
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
                glBindFramebuffer(0x8d40, 0);
                glDrawBuffer(0x0405);
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
                    } else if let Some(&(start, count, depth)) =
                        self.last_batches.get(command.index as usize)
                    {
                        glUseProgram(self.program);
                        glBindBuffer(GL_ARRAY_BUFFER, self.vertex_buffer);
                        glEnableVertexAttribArray(0);
                        glEnableVertexAttribArray(1);
                        glVertexAttribPointer(0, 4, GL_FLOAT, 0, 32, ptr::null());
                        glVertexAttribPointer(1, 4, GL_FLOAT, 0, 32, 16_usize as *const c_void);
                        set_depth(depth);
                        glDrawArrays(GL_TRIANGLES, start, count);
                        glDisableVertexAttribArray(0);
                        glDisableVertexAttribArray(1);
                    }
                }
                glBindBuffer(GL_ARRAY_BUFFER, 0);
                glUseProgram(0);
                glDepthMask(1);
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
                glReadBuffer(0x0405); // GL_BACK
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
            // SAFETY: Each nonzero handle is uniquely owned; reverse teardown keeps X11 and
            // the context alive until their dependent GL resources are released.
            unsafe {
                if self.grabbed {
                    XUngrabPointer(self.display, 0);
                }
                if self.hidden_cursor != 0 {
                    XFreeCursor(self.display, self.hidden_cursor);
                }
                if !self.context.is_null() {
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
                    }
                    glXMakeCurrent(self.display, 0, ptr::null_mut());
                    glXDestroyContext(self.display, self.context);
                }
                if self.alive && self.window != 0 {
                    XDestroyWindow(self.display, self.window);
                }
                if self.colormap != 0 {
                    XFreeColormap(self.display, self.colormap);
                }
                if !self.display.is_null() {
                    XCloseDisplay(self.display);
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
                glBindFramebuffer(0x8d40, 0);
                glDrawBuffer(0x0405);
                glReadBuffer(0x0405);
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
        fn compile_stage(
            source: &crate::material::ShaderSource,
            depth: bool,
        ) -> Result<Self, String> {
            let vertex = CString::new(format!(
                "#version 330\n{}\n{}",
                crate::retained::VERTEX_BODY
                    .replace("attribute ", "in ")
                    .replace("varying ", "out "),
                source.vertex
            ))
            .map_err(|e| e.to_string())?;
            let fragment = if depth {
                CString::new("#version 330\nvoid main(){}\n").unwrap()
            } else {
                CString::new(format!(
                    "#version 330\nout vec4 ferravex_pixel;\n{}\n{}",
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
                for index in 0..self.batches.len() {
                    self.draw(index, true);
                }
                glEnable(GL_BLEND);
                glBindFramebuffer(0x8d40, 0);
                glDrawBuffer(0x0405);
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
                    glVertexAttribPointer(
                        attribute,
                        size,
                        GL_FLOAT,
                        0,
                        40,
                        offset as *const c_void,
                    );
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
            String::from_utf8_lossy(&bytes[..(written.max(0) as usize).min(bytes.len())])
                .into_owned()
        }
    }
    fn create_program() -> Result<c_uint, String> {
        create_program_with(c"#version 120\nattribute vec4 a_position;\nattribute vec4 a_color;\nvarying vec4 v_color;\nvoid main(){ gl_Position=a_position; v_color=a_color; }\n", &[(0,c"a_position"),(1,c"a_color")])
    }
    fn create_program_with(source: &CStr, attributes: &[(u32, &CStr)]) -> Result<c_uint, String> {
        create_program_stages(
            source,
            c"#version 120\nvarying vec4 v_color;\nvoid main(){ gl_FragColor=v_color; }\n",
            attributes,
        )
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
                    (Mat4::from_translation(Vec3::X * x)
                        * Mat4::from_scale(Vec3::new(1., 1.5, 0.5)))
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
                .chunks_exact(4)
                .zip(animated.chunks_exact(4))
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
            let moved =
                SkinPalette::new(&[Mat4::from_translation(Vec3::X * -1.5).to_cols_array(); 4])
                    .unwrap();
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
                    .chunks_exact(4)
                    .zip(changed.chunks_exact(4))
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
                2,
                "skin mesh/weight resource was not collected"
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
                .chunks_exact(4)
                .zip(shaded.chunks_exact(4))
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
                dark.chunks_exact(4)
                    .zip(points.chunks_exact(4))
                    .filter(
                        |(a, b)| b[0] > a[0].saturating_add(20) && b[0] > b[1].saturating_add(20)
                    )
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
                    Mat4::from_translation(Vec3::new(x, 0., 0.))
                        * Mat4::from_scale(Vec3::splat(0.5)),
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
        fn event_union_covers_all_public_records() {
            assert_eq!(
                std::mem::size_of::<XEvent>(),
                24 * std::mem::size_of::<c_long>()
            );
            assert!(std::mem::size_of::<XKeyEvent>() <= std::mem::size_of::<XEvent>());
            assert!(std::mem::size_of::<XMotionEvent>() <= std::mem::size_of::<XEvent>());
            assert!(std::mem::size_of::<XConfigureEvent>() <= std::mem::size_of::<XEvent>());
            assert!(std::mem::size_of::<XClientMessageEvent>() <= std::mem::size_of::<XEvent>());
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
                ..Default::default()
            })
            .unwrap();
            window.set_swap_interval(0);
            assert!(window.enable_gpu_timing());
            for _ in 0..4 {
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
        #[test]
        fn screenshot_rows_flip_without_channel_or_center_row_changes() {
            let mut bytes = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
            flip_rows(&mut bytes, 4, 3);
            assert_eq!(bytes, [9, 10, 11, 12, 5, 6, 7, 8, 1, 2, 3, 4]);
        }
    }
}
