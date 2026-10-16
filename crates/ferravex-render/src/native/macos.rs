//! First-party AppKit/NSOpenGL host. Apple marks OpenGL deprecated; this host
//! requests its 4.1 core profile, sharing our GL renderer with Linux/Windows.
//! Reference: <https://developer.apple.com/documentation/appkit/nsopenglcontext>.
use super::{Conf, Event, objc::*};
use std::{
    cell::RefCell,
    collections::BTreeSet,
    ffi::{CStr, c_char, c_void},
    ptr,
};
#[link(name = "AppKit", kind = "framework")]
unsafe extern "C" {
    static NSApp: Id;
}
#[link(name = "Foundation", kind = "framework")]
unsafe extern "C" {
    static NSDefaultRunLoopMode: Id;
}
#[link(name = "OpenGL", kind = "framework")]
unsafe extern "C" {
    fn CGLGetCurrentContext() -> *mut c_void;
}
#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn CGAssociateMouseAndMouseCursorPosition(connected: u32) -> i32;
}
unsafe extern "C" {
    fn dlsym(handle: *mut c_void, name: *const c_char) -> *const c_void;
    fn pthread_main_np() -> i32;
}
thread_local! {static CLOSE:RefCell<Vec<Id>>=const{RefCell::new(Vec::new())};}
unsafe extern "C" fn should_close(_: Id, _: Sel, window: Id) -> i8 {
    CLOSE.with(|q| {
        let mut q = q.borrow_mut();
        if q.len() < 64 && !q.contains(&window) {
            q.push(window);
        }
    });
    0
}
fn delegate() -> Id {
    unsafe {
        let mut c = class(c"FerravexWindowDelegate");
        if c.is_null() {
            c = objc_allocateClassPair(class(c"NSObject"), c"FerravexWindowDelegate".as_ptr(), 0);
            class_addMethod(
                c,
                sel(c"windowShouldClose:"),
                should_close as *const c_void,
                c"c@:@".as_ptr(),
            );
            objc_registerClassPair(c);
        }
        id(c, c"new")
    }
}
pub(super) fn proc_address(name: &CStr) -> *const c_void {
    unsafe { dlsym(-2isize as *mut c_void, name.as_ptr()) }
}
pub(super) fn default_framebuffer() -> u32 {
    0
}
pub(super) struct Host {
    app: Id,
    window: Id,
    view: Id,
    context: Id,
    delegate: Id,
    width: u32,
    height: u32,
    logical: (u32, u32),
    scale: f32,
    focused: bool,
    suspended: bool,
    grabbed: bool,
    visible: bool,
    keys: BTreeSet<u32>,
    buttons: BTreeSet<u32>,
}
impl Host {
    pub(super) fn new(conf: &Conf) -> Result<Self, String> {
        if !(1..=16384).contains(&conf.window_width) || !(1..=16384).contains(&conf.window_height) {
            return Err("Invalid window dimensions".into());
        }
        unsafe {
            if pthread_main_np() == 0 {
                return Err("AppKit must run on the main thread".into());
            }
            let _pool = Pool::new();
            let first_launch = NSApp.is_null();
            let app = if first_launch {
                id(class(c"NSApplication"), c"sharedApplication")
            } else {
                NSApp
            };
            if app.is_null() {
                return Err("AppKit application unavailable".into());
            }
            if first_launch {
                void_u64(app, c"setActivationPolicy:", 0);
                void(app, c"finishLaunching");
            }
            let mut owner = Self {
                app,
                window: ptr::null_mut(),
                view: ptr::null_mut(),
                context: ptr::null_mut(),
                delegate: ptr::null_mut(),
                width: 0,
                height: 0,
                logical: (conf.window_width as u32, conf.window_height as u32),
                scale: 1.,
                focused: false,
                suspended: false,
                grabbed: false,
                visible: true,
                keys: BTreeSet::new(),
                buttons: BTreeSet::new(),
            };
            let bounds = Rect::new(conf.window_width as f64, conf.window_height as f64);
            owner.window = window(
                id(class(c"NSWindow"), c"alloc"),
                c"initWithContentRect:styleMask:backing:defer:",
                bounds,
                15,
                2,
                0,
            );
            if owner.window.is_null() {
                return Err("AppKit window creation failed".into());
            }
            void_bool(owner.window, c"setReleasedWhenClosed:", 0);
            void_id(owner.window, c"setTitle:", text(&conf.window_title));
            void_bool(owner.window, c"setAcceptsMouseMovedEvents:", 1);
            let attrs = [99, 0x4100, 73, 5, 8, 24, 11, 8, 12, 24, 13, 8, 0];
            let fmt = format(
                id(class(c"NSOpenGLPixelFormat"), c"alloc"),
                c"initWithAttributes:",
                attrs.as_ptr(),
            );
            if fmt.is_null() {
                return Err("macOS OpenGL 4.1 pixel format unavailable".into());
            }
            owner.view = view(
                id(class(c"NSOpenGLView"), c"alloc"),
                c"initWithFrame:pixelFormat:",
                bounds,
                fmt,
            );
            void(fmt, c"release");
            if owner.view.is_null() {
                return Err("NSOpenGLView creation failed".into());
            }
            void_u64(owner.view, c"setAutoresizingMask:", 18);
            void_bool(
                owner.view,
                c"setWantsBestResolutionOpenGLSurface:",
                i8::from(conf.high_dpi),
            );
            void_id(owner.window, c"setContentView:", owner.view);
            owner.context = id(owner.view, c"openGLContext");
            if owner.context.is_null() {
                return Err("NSOpenGLContext creation failed".into());
            }
            id(owner.context, c"retain");
            owner.delegate = delegate();
            void_id(owner.window, c"setDelegate:", owner.delegate);
            void(owner.window, c"center");
            void_id(owner.window, c"makeKeyAndOrderFront:", ptr::null_mut());
            void_bool(app, c"activateIgnoringOtherApps:", 1);
            if !owner.make_current() {
                return Err("NSOpenGLContext activation failed".into());
            }
            owner.update_size(conf.high_dpi);
            owner.set_swap_interval(1);
            Ok(owner)
        }
    }
    fn update_size(&mut self, high_dpi: bool) {
        unsafe {
            let bounds = rect(self.view, c"bounds");
            self.logical = (
                bounds.size.width.round().max(1.) as u32,
                bounds.size.height.round().max(1.) as u32,
            );
            self.scale = if high_dpi {
                (float(self.window, c"backingScaleFactor") as f32).clamp(0.5, 4.)
            } else {
                1.
            };
            self.width = (self.logical.0 as f32 * self.scale).round() as u32;
            self.height = (self.logical.1 as f32 * self.scale).round() as u32;
            void(self.context, c"update");
        }
    }
    pub(super) fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }
    pub(super) fn logical_size(&self) -> (u32, u32) {
        self.logical
    }
    pub(super) fn dpi_scale(&self) -> f32 {
        self.scale
    }
    pub(super) fn make_current(&self) -> bool {
        if self.context.is_null() {
            return false;
        }
        unsafe {
            void(self.context, c"makeCurrentContext");
            !CGLGetCurrentContext().is_null()
        }
    }
    pub(super) fn swap(&self) {
        unsafe {
            void(self.context, c"flushBuffer");
        }
    }
    pub(super) fn set_swap_interval(&mut self, v: i32) {
        if v >= 0 {
            unsafe {
                parameter(self.context, c"setValues:forParameter:", &v, 222);
            }
        }
    }
    pub(super) fn set_cursor(&mut self, grab: bool, visible: bool) {
        let grab = grab && self.focused && !self.suspended;
        unsafe {
            if grab != self.grabbed {
                CGAssociateMouseAndMouseCursorPosition(u32::from(!grab));
                self.grabbed = grab;
            }
            let visible = visible || !self.focused;
            if visible != self.visible {
                void(
                    class(c"NSCursor"),
                    if visible { c"unhide" } else { c"hide" },
                );
                self.visible = visible;
            }
        }
    }
    pub(super) fn poll(&mut self) -> Vec<Event> {
        let _pool = Pool::new();
        let mut out = Vec::new();
        unsafe {
            let focused = bool(self.window, c"isKeyWindow") != 0;
            if focused != self.focused {
                self.focused = focused;
                out.push(Event::Focus(focused));
                if !focused {
                    for k in std::mem::take(&mut self.keys) {
                        out.push(Event::Key(k, false));
                    }
                    for b in std::mem::take(&mut self.buttons) {
                        out.push(Event::MouseButton(b, false));
                    }
                    self.set_cursor(false, true);
                }
            }
            let suspended =
                bool(self.window, c"isMiniaturized") != 0 || bool(self.app, c"isHidden") != 0;
            if self.suspended != suspended {
                self.suspended = suspended;
                out.push(Event::Suspend(suspended));
            }
            let size = self.size();
            let high_dpi = bool(self.view, c"wantsBestResolutionOpenGLSurface") != 0;
            self.update_size(high_dpi);
            if size != self.size() {
                out.push(Event::Resize(self.logical.0, self.logical.1));
            }
            let close = CLOSE.with(|q| {
                let mut q = q.borrow_mut();
                if let Some(i) = q.iter().position(|w| *w == self.window) {
                    q.remove(i);
                    true
                } else {
                    false
                }
            });
            if close {
                out.push(Event::Quit);
            }
            for _ in 0..4096 {
                let e = event(
                    self.app,
                    c"nextEventMatchingMask:untilDate:inMode:dequeue:",
                    u64::MAX,
                    id(class(c"NSDate"), c"distantPast"),
                    NSDefaultRunLoopMode,
                    1,
                );
                if e.is_null() {
                    break;
                }
                let kind = uint(e, c"type");
                let own = id(e, c"window") == self.window;
                if own {
                    match kind {
                        10 | 11 => {
                            let chars = id(e, c"charactersIgnoringModifiers");
                            let p = cstr(chars, c"UTF8String");
                            let code = if p.is_null() {
                                0
                            } else {
                                CStr::from_ptr(p)
                                    .to_string_lossy()
                                    .chars()
                                    .next()
                                    .map_or(0, super::apple_key)
                            };
                            let down = kind == 10;
                            if down
                                && code == u32::from(b'q')
                                && uint(e, c"modifierFlags") & (1 << 20) != 0
                            {
                                out.push(Event::Quit);
                            } else if code != 0
                                && if down {
                                    self.keys.insert(code)
                                } else {
                                    self.keys.remove(&code)
                                }
                            {
                                out.push(Event::Key(code, down));
                            }
                        }
                        12 => {
                            let flags = uint(e, c"modifierFlags");
                            let (code, bit) = match uint(e, c"keyCode") {
                                56 => (0xffe1, 17),
                                60 => (0xffe2, 17),
                                59 => (0xffe3, 18),
                                62 => (0xffe4, 18),
                                58 => (0xffe9, 19),
                                61 => (0xffea, 19),
                                55 => (0xffeb, 20),
                                54 => (0xffec, 20),
                                _ => (0, 0),
                            };
                            if code != 0 {
                                let aggregate = flags & (1 << bit) != 0;
                                // flagsChanged identifies the changed physical key;
                                // the aggregate flag stays set while its other side is held.
                                let down = aggregate && !self.keys.contains(&code);
                                if !aggregate {
                                    let pair = match bit {
                                        17 => [0xffe1, 0xffe2],
                                        18 => [0xffe3, 0xffe4],
                                        19 => [0xffe9, 0xffea],
                                        _ => [0xffeb, 0xffec],
                                    };
                                    for other in pair {
                                        if other != code && self.keys.remove(&other) {
                                            out.push(Event::Key(other, false));
                                        }
                                    }
                                }
                                if if down {
                                    self.keys.insert(code)
                                } else {
                                    self.keys.remove(&code)
                                } {
                                    out.push(Event::Key(code, down));
                                }
                            }
                        }
                        5 | 6 | 7 | 27 => {
                            if self.grabbed {
                                out.push(Event::MouseDelta(
                                    float(e, c"deltaX") as f32,
                                    float(e, c"deltaY") as f32,
                                ));
                            } else {
                                let p = point_id(
                                    self.view,
                                    c"convertPoint:fromView:",
                                    point(e, c"locationInWindow"),
                                    ptr::null_mut(),
                                );
                                out.push(Event::MouseMove(
                                    p.x as f32,
                                    self.logical.1 as f32 - p.y as f32,
                                ));
                            }
                        }
                        1..=4 | 25 | 26 => {
                            let down = matches!(kind, 1 | 3 | 25);
                            let button = match kind {
                                1 | 2 => 1,
                                3 | 4 => 3,
                                _ => 2,
                            };
                            if down {
                                self.buttons.insert(button);
                            } else {
                                self.buttons.remove(&button);
                            }
                            out.push(Event::MouseButton(button, down));
                        }
                        22 => out.push(Event::Wheel(float(e, c"scrollingDeltaY") as f32)),
                        _ => {}
                    }
                }
                void_id(self.app, c"sendEvent:", e);
            }
            void(self.app, c"updateWindows");
        }
        out
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        unsafe {
            self.set_cursor(false, true);
            CLOSE.with(|q| q.borrow_mut().retain(|w| *w != self.window));
            if !self.context.is_null() {
                void(class(c"NSOpenGLContext"), c"clearCurrentContext");
                void(self.context, c"clearDrawable");
                void(self.context, c"release");
            }
            if !self.window.is_null() {
                void_id(self.window, c"setDelegate:", ptr::null_mut());
                void(self.window, c"close");
                void(self.window, c"release");
            }
            if !self.view.is_null() {
                void(self.view, c"release");
            }
            if !self.delegate.is_null() {
                void(self.delegate, c"release");
            }
        }
    }
}
