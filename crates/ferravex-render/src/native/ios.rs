//! Original UIKit scene/CADisplayLink/EAGL host. Requires an app bundle with the
//! FerravexSceneDelegate scene manifest. SDK/device checks are intentionally
//! excluded on the Linux development host; see docs/PLATFORMS.md.
use super::{Conf, Event, objc::*};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    ffi::{CStr, c_char, c_void},
    future::Future,
    ptr,
};
#[link(name = "UIKit", kind = "framework")]
unsafe extern "C" {
    fn UIApplicationMain(argc: i32, argv: *mut *mut c_char, principal: Id, delegate: Id) -> i32;
}
#[link(name = "QuartzCore", kind = "framework")]
unsafe extern "C" {}
#[link(name = "OpenGLES", kind = "framework")]
unsafe extern "C" {
    fn glClearDepthf(depth: f32);
    fn glDrawBuffers(n: i32, buffers: *const u32);
}
#[link(name = "Foundation", kind = "framework")]
unsafe extern "C" {
    static NSRunLoopCommonModes: Id;
}
unsafe extern "C" {
    fn dlsym(handle: *mut c_void, name: *const c_char) -> *const c_void;
}
thread_local! {
 static WINDOW:Cell<Id>=const {Cell::new(ptr::null_mut())};
 static SCENE:Cell<Id>=const {Cell::new(ptr::null_mut())};
 static ACTIVE:Cell<bool>=const{Cell::new(true)};
 static CURRENT:Cell<(Id,u32)>=const{Cell::new((ptr::null_mut(),0))};
 static LINK:Cell<Id>=const{Cell::new(ptr::null_mut())};
 static EVENTS:RefCell<VecDeque<Event>>=const{RefCell::new(VecDeque::new())};
 static BOOT:RefCell<Option<Box<dyn FnOnce()->Result<crate::runner::Runner,String>>>>=const{RefCell::new(None)};
 static RUNNER:RefCell<Option<crate::runner::Runner>>=const{RefCell::new(None)};
 static CONFIG:RefCell<Option<Conf>>=const{RefCell::new(None)};
}
fn push(event: Event) {
    EVENTS.with(|q| {
        let mut q = q.borrow_mut();
        if q.len() >= 256 {
            q.clear();
            q.push_back(Event::Focus(false));
            if ACTIVE.with(Cell::get) {
                q.push_back(Event::Focus(true));
            }
        }
        q.push_back(event);
    });
}
unsafe extern "system" fn clear_depth(depth: f64) {
    unsafe {
        glClearDepthf(depth as f32);
    }
}
unsafe extern "system" fn draw_buffer(buffer: u32) {
    unsafe {
        glDrawBuffers(1, &buffer);
    }
}
pub(super) fn proc_address(name: &CStr) -> *const c_void {
    match name.to_bytes() {
        b"glClearDepth" => clear_depth as *const c_void,
        b"glDrawBuffer" => draw_buffer as *const c_void,
        _ => unsafe { dlsym(-2isize as *mut c_void, name.as_ptr()) },
    }
}
pub(super) fn default_framebuffer() -> u32 {
    CURRENT.with(|c| c.get().1)
}
unsafe extern "C" fn layer_class(_: Id, _: Sel) -> Id {
    class(c"CAEAGLLayer")
}
unsafe extern "C" fn scene_window(_: Id, _: Sel) -> Id {
    WINDOW.with(Cell::get)
}
unsafe extern "C" fn hide_status(_: Id, _: Sel) -> i8 {
    1
}
fn touch_events(view: Id, touches: Id, phase: u32) {
    unsafe {
        let array = id(touches, c"allObjects");
        for i in 0..uint(array, c"count").min(64) {
            let t = id_usize(array, c"objectAtIndex:", i);
            let p = point_object(t, c"locationInView:", view);
            push(Event::Touch {
                id: t as u64,
                phase,
                x: p.x as f32,
                y: p.y as f32,
            });
        }
    }
}
unsafe extern "C" fn touch_start(view: Id, _: Sel, touches: Id, _: Id) {
    touch_events(view, touches, 0);
}
unsafe extern "C" fn touch_move(view: Id, _: Sel, touches: Id, _: Id) {
    touch_events(view, touches, 1);
}
unsafe extern "C" fn touch_end(view: Id, _: Sel, touches: Id, _: Id) {
    touch_events(view, touches, 2);
}
unsafe extern "C" fn touch_cancel(view: Id, _: Sel, touches: Id, _: Id) {
    touch_events(view, touches, 3);
}
fn keys(presses: Id, down: bool) {
    unsafe {
        let a = id(presses, c"allObjects");
        for i in 0..uint(a, c"count").min(64) {
            let press = id_usize(a, c"objectAtIndex:", i);
            let k = id(press, c"key");
            if k.is_null() {
                continue;
            }
            let code = match uint(k, c"keyCode") {
                0xe0 => 0xffe3,
                0xe1 => 0xffe1,
                0xe2 => 0xffe9,
                0xe3 => 0xffeb,
                0xe4 => 0xffe4,
                0xe5 => 0xffe2,
                0xe6 => 0xffea,
                0xe7 => 0xffec,
                0x3a..=0x45 => 0xffbe + uint(k, c"keyCode") as u32 - 0x3a,
                0x4f => 0xff53,
                0x50 => 0xff51,
                0x51 => 0xff54,
                0x52 => 0xff52,
                0x28 => 0xff0d,
                0x29 => 0xff1b,
                0x2a => 0xff08,
                0x2b => 0xff09,
                0x4c => 0xffff,
                _ => 0,
            };
            if code != 0 {
                push(Event::Key(code, down));
                continue;
            }
            let chars = id(k, c"charactersIgnoringModifiers");
            let p = cstr(chars, c"UTF8String");
            if p.is_null() {
                continue;
            }
            let code = CStr::from_ptr(p)
                .to_string_lossy()
                .chars()
                .next()
                .map_or(0, super::apple_key);
            if code != 0 {
                push(Event::Key(code, down));
            }
        }
    }
}
unsafe extern "C" fn keys_start(_: Id, _: Sel, presses: Id, _: Id) {
    keys(presses, true);
}
unsafe extern "C" fn keys_end(_: Id, _: Sel, presses: Id, _: Id) {
    keys(presses, false);
}
unsafe extern "C" fn can_respond(_: Id, _: Sel) -> i8 {
    1
}
unsafe extern "C" fn launched(_: Id, _: Sel, _: Id, _: Id) -> i8 {
    1
}
unsafe extern "C" fn draw(_: Id, _: Sel, _: Id) {
    if !ACTIVE.with(Cell::get) {
        return;
    }
    let alive = RUNNER.with(|r| {
        r.borrow_mut()
            .as_mut()
            .is_none_or(crate::runner::Runner::step)
    });
    if !alive {
        LINK.with(|l| unsafe {
            void_bool(l.get(), c"setPaused:", 1);
        });
    }
}
unsafe extern "C" fn scene_connect(_: Id, _: Sel, scene: Id, _: Id, _: Id) {
    SCENE.with(|s| s.set(scene));
    ACTIVE.with(|a| a.set(true));
    let boot = BOOT.with(|b| b.borrow_mut().take());
    let result = if let Some(boot) = boot {
        boot()
    } else {
        CONFIG
            .with(|c| c.borrow().clone())
            .ok_or_else(|| "Missing scene configuration".to_owned())
            .and_then(crate::runner::Runner::resume)
    };
    match result {
        Ok(runner) => {
            RUNNER.with(|r| *r.borrow_mut() = Some(runner));
            unsafe {
                let target = id(class(c"FerravexSceneDelegate"), c"new");
                let link = display_link(
                    class(c"CADisplayLink"),
                    c"displayLinkWithTarget:selector:",
                    target,
                    sel(c"ferravexFrame:"),
                );
                void(target, c"release");
                id(link, c"retain");
                LINK.with(|l| l.set(link));
                void_u64(link, c"setPreferredFramesPerSecond:", 0);
                add_runloop(
                    link,
                    c"addToRunLoop:forMode:",
                    id(class(c"NSRunLoop"), c"mainRunLoop"),
                    NSRunLoopCommonModes,
                );
            }
        }
        Err(error) => {
            eprintln!("Ferravex UIKit host: {error}");
        }
    }
}
unsafe extern "C" fn active(_: Id, _: Sel, _: Id) {
    ACTIVE.with(|a| a.set(true));
    push(Event::Suspend(false));
    push(Event::Focus(true));
    LINK.with(|l| unsafe {
        void_bool(l.get(), c"setPaused:", 0);
    });
}
unsafe extern "C" fn inactive(_: Id, _: Sel, _: Id) {
    // Finish current GPU work while the scene is still foreground, then stop the
    // display link. No GL commands are issued while ACTIVE is false.
    push(Event::Suspend(true));
    RUNNER.with(|r| {
        if let Some(r) = r.borrow_mut().as_mut() {
            r.step();
        }
    });
    ACTIVE.with(|a| a.set(false));
    LINK.with(|l| unsafe {
        void_bool(l.get(), c"setPaused:", 1);
    });
}
unsafe extern "C" fn disconnected(_: Id, _: Sel, _: Id) {
    ACTIVE.with(|a| a.set(false));
    LINK.with(|l| unsafe {
        let link = l.replace(ptr::null_mut());
        if !link.is_null() {
            void(link, c"invalidate");
            void(link, c"release");
        }
    });
    RUNNER.with(|r| *r.borrow_mut() = None);
    SCENE.with(|s| s.set(ptr::null_mut()));
    push(Event::Focus(false));
}
unsafe extern "C" fn memory_warning(_: Id, _: Sel, _: Id) {
    crate::request_graphics_reset();
}
fn register_class(name: &CStr, parent: &CStr, methods: &[(&CStr, *const c_void, &CStr)]) -> Id {
    unsafe {
        let mut c = class(name);
        if c.is_null() {
            c = objc_allocateClassPair(class(parent), name.as_ptr(), 0);
            for (s, f, types) in methods {
                class_addMethod(c, sel(s), *f, types.as_ptr());
            }
            objc_registerClassPair(c);
        }
        c
    }
}
pub(crate) fn launch(conf: Conf, future: impl Future<Output = ()> + 'static) {
    CONFIG.with(|c| *c.borrow_mut() = Some(conf.clone()));
    BOOT.with(|b| {
        *b.borrow_mut() = Some(Box::new(move || crate::runner::Runner::new(conf, future)))
    });
    unsafe {
        let view = register_class(
            c"FerravexEaglView",
            c"UIView",
            &[
                (
                    c"touchesBegan:withEvent:",
                    touch_start as *const c_void,
                    c"v@:@@",
                ),
                (
                    c"touchesMoved:withEvent:",
                    touch_move as *const c_void,
                    c"v@:@@",
                ),
                (
                    c"touchesEnded:withEvent:",
                    touch_end as *const c_void,
                    c"v@:@@",
                ),
                (
                    c"touchesCancelled:withEvent:",
                    touch_cancel as *const c_void,
                    c"v@:@@",
                ),
                (
                    c"pressesBegan:withEvent:",
                    keys_start as *const c_void,
                    c"v@:@@",
                ),
                (
                    c"pressesEnded:withEvent:",
                    keys_end as *const c_void,
                    c"v@:@@",
                ),
                (
                    c"pressesCancelled:withEvent:",
                    keys_end as *const c_void,
                    c"v@:@@",
                ),
                (
                    c"canBecomeFirstResponder",
                    can_respond as *const c_void,
                    c"c@:",
                ),
            ],
        );
        // +layerClass belongs to the metaclass, not to view instances.
        class_addMethod(
            object_getClass(view),
            sel(c"layerClass"),
            layer_class as *const c_void,
            c"#@:".as_ptr(),
        );
        register_class(
            c"FerravexViewController",
            c"UIViewController",
            &[(
                c"prefersStatusBarHidden",
                hide_status as *const c_void,
                c"c@:",
            )],
        );
        register_class(
            c"FerravexAppDelegate",
            c"UIResponder",
            &[
                (
                    c"application:didFinishLaunchingWithOptions:",
                    launched as *const c_void,
                    c"c@:@@",
                ),
                (
                    c"applicationDidReceiveMemoryWarning:",
                    memory_warning as *const c_void,
                    c"v@:@",
                ),
            ],
        );
        register_class(
            c"FerravexSceneDelegate",
            c"UIResponder",
            &[
                (c"window", scene_window as *const c_void, c"@@:"),
                (
                    c"scene:willConnectToSession:options:",
                    scene_connect as *const c_void,
                    c"v@:@@@",
                ),
                (c"sceneDidBecomeActive:", active as *const c_void, c"v@:@"),
                (
                    c"sceneWillResignActive:",
                    inactive as *const c_void,
                    c"v@:@",
                ),
                (
                    c"sceneDidDisconnect:",
                    disconnected as *const c_void,
                    c"v@:@",
                ),
                (c"ferravexFrame:", draw as *const c_void, c"v@:@"),
            ],
        );
        class_addProtocol(
            class(c"FerravexAppDelegate"),
            objc_getProtocol(c"UIApplicationDelegate".as_ptr()),
        );
        class_addProtocol(
            class(c"FerravexSceneDelegate"),
            objc_getProtocol(c"UIWindowSceneDelegate".as_ptr()),
        );
        let _pool = Pool::new();
        UIApplicationMain(
            0,
            ptr::null_mut(),
            ptr::null_mut(),
            text("FerravexAppDelegate"),
        );
    }
}
#[link(name = "objc")]
unsafe extern "C" {
    fn object_getClass(object: Id) -> Id;
}
pub(super) struct Host {
    scene: Id,
    window: Id,
    controller: Id,
    view: Id,
    context: Id,
    framebuffer: u32,
    color: u32,
    depth: u32,
    width: u32,
    height: u32,
    logical: (u32, u32),
    scale: f32,
    loaded: bool,
}
impl Host {
    pub(super) fn new(conf: &Conf) -> Result<Self, String> {
        let scene = SCENE.with(Cell::get);
        if scene.is_null() {
            return Err("UIKit host requires the Ferravex scene manifest".into());
        }
        let _pool = Pool::new();
        unsafe {
            id(scene, c"retain");
            let mut h = Self {
                scene,
                window: ptr::null_mut(),
                controller: ptr::null_mut(),
                view: ptr::null_mut(),
                context: ptr::null_mut(),
                framebuffer: 0,
                color: 0,
                depth: 0,
                width: 0,
                height: 0,
                logical: (1, 1),
                scale: 1.,
                loaded: false,
            };
            let space = id(scene, c"coordinateSpace");
            let bounds = rect(space, c"bounds");
            h.window = id_id(
                id(class(c"UIWindow"), c"alloc"),
                c"initWithWindowScene:",
                scene,
            );
            if h.window.is_null() {
                return Err("UIWindowScene window creation failed".into());
            }
            h.view = init_rect(
                id(class(c"FerravexEaglView"), c"alloc"),
                c"initWithFrame:",
                bounds,
            );
            if h.view.is_null() {
                return Err("EAGL view creation failed".into());
            }
            h.scale = if conf.high_dpi {
                (float(id(scene, c"screen"), c"scale") as f32).clamp(0.5, 4.)
            } else {
                1.
            };
            set_float(h.view, c"setContentScaleFactor:", f64::from(h.scale));
            void_bool(h.view, c"setMultipleTouchEnabled:", 1);
            void_u64(h.view, c"setAutoresizingMask:", 18);
            let layer = id(h.view, c"layer");
            set_float(layer, c"setContentsScale:", f64::from(h.scale));
            void_bool(layer, c"setOpaque:", 1);
            h.controller = id(class(c"FerravexViewController"), c"new");
            void_id(h.controller, c"setView:", h.view);
            void_id(h.window, c"setRootViewController:", h.controller);
            void(h.window, c"makeKeyAndVisible");
            WINDOW.with(|w| w.set(h.window));
            bool(h.view, c"becomeFirstResponder");
            h.context = id_usize(id(class(c"EAGLContext"), c"alloc"), c"initWithAPI:", 3);
            if h.context.is_null() || !h.make_current() {
                return Err("OpenGL ES 3 context unavailable".into());
            }
            super::gl::load()?;
            h.loaded = true;
            super::gl::glGenFramebuffers(1, &mut h.framebuffer);
            super::gl::glGenRenderbuffers(1, &mut h.color);
            super::gl::glGenRenderbuffers(1, &mut h.depth);
            if h.framebuffer == 0 || h.color == 0 || h.depth == 0 {
                return Err("EAGL drawable allocation failed".into());
            }
            h.resize()?;
            Ok(h)
        }
    }
    fn resize(&mut self) -> Result<(), String> {
        use super::gl::*;
        unsafe {
            if !self.make_current() {
                return Err("EAGL context unavailable".into());
            }
            let b = rect(self.view, c"bounds");
            self.logical = (
                b.size.width.round().max(1.) as u32,
                b.size.height.round().max(1.) as u32,
            );
            glBindRenderbuffer(0x8d41, self.color);
            if drawable(
                self.context,
                c"renderbufferStorage:fromDrawable:",
                0x8d41,
                id(self.view, c"layer"),
            ) == 0
            {
                return Err("EAGL drawable storage unavailable".into());
            }
            let (mut w, mut h) = (0, 0);
            glGetRenderbufferParameteriv(0x8d41, 0x8d42, &mut w);
            glGetRenderbufferParameteriv(0x8d41, 0x8d43, &mut h);
            if !(1..=16384).contains(&w) || !(1..=16384).contains(&h) {
                return Err("Invalid EAGL drawable dimensions".into());
            }
            self.width = w as u32;
            self.height = h as u32;
            glBindFramebuffer(0x8d40, self.framebuffer);
            glFramebufferRenderbuffer(0x8d40, 0x8ce0, 0x8d41, self.color);
            glBindRenderbuffer(0x8d41, self.depth);
            glRenderbufferStorage(0x8d41, 0x81a6, w, h);
            glFramebufferRenderbuffer(0x8d40, 0x8d00, 0x8d41, self.depth);
            if glCheckFramebufferStatus(0x8d40) != 0x8cd5 {
                return Err("Incomplete EAGL framebuffer".into());
            }
            CURRENT.with(|c| c.set((self.context, self.framebuffer)));
            Ok(())
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
        if self.context.is_null() || !ACTIVE.with(Cell::get) {
            return false;
        }
        let ok =
            unsafe { bool_id(class(c"EAGLContext"), c"setCurrentContext:", self.context) != 0 };
        if ok {
            CURRENT.with(|c| c.set((self.context, self.framebuffer)));
        }
        ok
    }
    pub(super) fn set_swap_interval(&mut self, _: i32) {}
    pub(super) fn set_cursor(&mut self, _: bool, _: bool) {}
    pub(super) fn swap(&self) {
        if self.make_current() {
            unsafe {
                super::gl::glBindRenderbuffer(0x8d41, self.color);
                if !bool_u64(self.context, c"presentRenderbuffer:", 0x8d41) {
                    crate::request_graphics_reset();
                }
            }
        }
    }
    pub(super) fn poll(&mut self) -> Vec<Event> {
        let mut events = EVENTS.with(|q| q.borrow_mut().drain(..).collect::<Vec<_>>());
        if !ACTIVE.with(Cell::get) || events.iter().any(|e| matches!(e, Event::Suspend(true))) {
            return events;
        }
        let bounds = unsafe { rect(self.view, c"bounds") };
        let logical = (
            bounds.size.width.round().max(1.) as u32,
            bounds.size.height.round().max(1.) as u32,
        );
        if logical != self.logical {
            match self.resize() {
                Ok(()) => events.push(Event::Resize(logical.0, logical.1)),
                Err(e) => {
                    eprintln!("EAGL resize: {e}");
                    crate::request_graphics_reset();
                }
            }
        }
        events
    }
}
unsafe fn set_float(object: Id, name: &CStr, value: f64) {
    unsafe {
        std::mem::transmute::<unsafe extern "C" fn(), unsafe extern "C" fn(Id, Sel, f64)>(
            objc_msgSend,
        )(object, sel(name), value);
    }
}
unsafe fn bool_u64(object: Id, name: &CStr, value: u64) -> bool {
    unsafe {
        std::mem::transmute::<unsafe extern "C" fn(), unsafe extern "C" fn(Id, Sel, u64) -> i8>(
            objc_msgSend,
        )(object, sel(name), value)
            != 0
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        unsafe {
            if self.loaded && self.make_current() {
                use super::gl::*;
                glDeleteRenderbuffers(1, &self.depth);
                glDeleteRenderbuffers(1, &self.color);
                glDeleteFramebuffers(1, &self.framebuffer);
            }
            if CURRENT.with(|c| c.get().0 == self.context) {
                CURRENT.with(|c| c.set((ptr::null_mut(), 0)));
                bool_id(
                    class(c"EAGLContext"),
                    c"setCurrentContext:",
                    ptr::null_mut(),
                );
            }
            if !self.context.is_null() {
                void(self.context, c"release");
            }
            if !self.window.is_null() {
                WINDOW.with(|w| {
                    if w.get() == self.window {
                        w.set(ptr::null_mut());
                    }
                });
                void_bool(self.window, c"setHidden:", 1);
                void(self.window, c"release");
            }
            if !self.controller.is_null() {
                void(self.controller, c"release");
            }
            if !self.view.is_null() {
                void(self.view, c"release");
            }
            void(self.scene, c"release");
        }
    }
}
