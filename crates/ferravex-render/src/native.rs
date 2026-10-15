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
    pub fn pixels(&self) -> Vec<u8> {
        Vec::new()
    }
    pub fn set_cursor(&mut self, _: bool, _: bool) {}
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
    pub struct NativeWindow {
        display: *mut Display,
        window: c_ulong,
        colormap: c_ulong,
        context: GlxContext,
        program: c_uint,
        vertex_buffer: c_uint,
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
                        }
                        FOCUS_OUT => {
                            self.focused = false;
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
            if !self.make_current() {
                return;
            }
            let Ok(byte_count) = isize::try_from(std::mem::size_of_val(vertices)) else {
                return;
            };
            self.last_clear = clear;
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
                    continue;
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
            self.draw_uploaded();
            // SAFETY: The live, current drawable owns the completed back buffer.
            unsafe {
                glXSwapBuffers(self.display, self.window);
            }
            self.presented = true;
        }
        fn draw_uploaded(&self) {
            // SAFETY: Called with this context current. Attribute pointers are byte offsets
            // into the owned VBO and all saved ranges were checked against its uploaded length.
            unsafe {
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
                glUseProgram(self.program);
                glBindBuffer(GL_ARRAY_BUFFER, self.vertex_buffer);
                glEnableVertexAttribArray(0);
                glEnableVertexAttribArray(1);
                glVertexAttribPointer(0, 4, GL_FLOAT, 0, 32, ptr::null());
                glVertexAttribPointer(1, 4, GL_FLOAT, 0, 32, 16_usize as *const c_void);
                for &(start, count, depth) in &self.last_batches {
                    if depth {
                        glEnable(GL_DEPTH_TEST);
                        glDepthMask(1);
                    } else {
                        glDisable(GL_DEPTH_TEST);
                        glDepthMask(0);
                    }
                    glDrawArrays(GL_TRIANGLES, start, count);
                }
                glDisableVertexAttribArray(0);
                glDisableVertexAttribArray(1);
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
                        if self.vertex_buffer != 0 {
                            glDeleteBuffers(1, &self.vertex_buffer);
                        }
                        if self.program != 0 {
                            glDeleteProgram(self.program);
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
        let vertex = compile_shader(GL_VERTEX_SHADER, c"#version 120\nattribute vec4 a_position;\nattribute vec4 a_color;\nvarying vec4 v_color;\nvoid main(){ gl_Position=a_position; v_color=a_color; }\n")?;
        let fragment = match compile_shader(
            GL_FRAGMENT_SHADER,
            c"#version 120\nvarying vec4 v_color;\nvoid main(){ gl_FragColor=v_color; }\n",
        ) {
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
            glBindAttribLocation(program, 0, c"a_position".as_ptr());
            glBindAttribLocation(program, 1, c"a_color".as_ptr());
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
        fn screenshot_rows_flip_without_channel_or_center_row_changes() {
            let mut bytes = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
            flip_rows(&mut bytes, 4, 3);
            assert_eq!(bytes, [9, 10, 11, 12, 5, 6, 7, 8, 1, 2, 3, 4]);
        }
    }
}
