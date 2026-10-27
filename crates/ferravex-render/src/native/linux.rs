//! First-party X11/GLX context, input and window lifetime.
use super::{Conf, Event};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::{CStr, CString, c_char, c_int, c_long, c_short, c_uint, c_ulong, c_void},
    ptr,
};
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
    fn XResourceManagerString(display: *mut Display) -> *const c_char;
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
    fn XInternAtom(display: *mut Display, name: *const c_char, only_if_exists: c_int) -> c_ulong;
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
}
pub(super) struct Host {
    display: *mut Display,
    window: c_ulong,
    colormap: c_ulong,
    context: GlxContext,
    width: u32,
    height: u32,
    scale: f32,
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
    keys: BTreeMap<u32, u32>,
    buttons: BTreeSet<u32>,
}
impl Host {
    pub fn new(conf: &Conf) -> Result<Self, String> {
        if !(1..=16384).contains(&conf.window_width) || !(1..=16384).contains(&conf.window_height) {
            return Err("Window dimensions must be between 1 and 16384 pixels".into());
        }
        // SAFETY: Each system allocation is retained in the owner below and released in Drop.
        // Xlib structs have their public C ABI layout; current-thread GL ownership is preserved.
        unsafe {
            let display = XOpenDisplay(ptr::null());
            if display.is_null() {
                return Err("Cannot open X11 display; set DISPLAY to a running X server".into());
            }
            let resource = XResourceManagerString(display);
            let resource = if resource.is_null() {
                std::borrow::Cow::Borrowed("")
            } else {
                CStr::from_ptr(resource).to_string_lossy()
            };
            let scale = if conf.high_dpi {
                display_scale(
                    std::env::var("FERRAVEX_DPI_SCALE").ok().as_deref(),
                    &resource,
                )
            } else {
                1.
            };
            let width = (conf.window_width as f32 * scale).round() as u32;
            let height = (conf.window_height as f32 * scale).round() as u32;
            if width > 16384 || height > 16384 {
                XCloseDisplay(display);
                return Err("Scaled window exceeds 16384 pixels".into());
            }
            let mut owner = Self {
                display,
                window: 0,
                colormap: 0,
                context: ptr::null_mut(),
                width,
                height,
                scale,
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
            let title =
                CString::new(conf.window_title.replace('\0', " ")).map_err(|e| e.to_string())?;
            XStoreName(display, owner.window, title.as_ptr());
            owner.delete_atom = XInternAtom(display, c"WM_DELETE_WINDOW".as_ptr(), 0);
            owner.protocols_atom = XInternAtom(display, c"WM_PROTOCOLS".as_ptr(), 0);
            XSetWMProtocols(display, owner.window, &mut owner.delete_atom, 1);
            XMapWindow(display, owner.window);
            XFlush(display);
            if glXMakeCurrent(display, owner.window, owner.context) == 0 {
                return Err("Cannot make the GLX context current".into());
            }
            owner.set_swap_interval(1);
            Ok(owner)
        }
    }
    pub(super) fn size(&self) -> (u32, u32) {
        (self.width, self.height)
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
            let extensions = glXQueryExtensionsString(self.display, XDefaultScreen(self.display));
            if extensions.is_null() {
                return;
            }
            let extensions = CStr::from_ptr(extensions).to_string_lossy();
            if extensions
                .split_whitespace()
                .any(|name| name == "GLX_EXT_swap_control")
            {
                if let Some(address) = glXGetProcAddressARB(c"glXSwapIntervalEXT".as_ptr().cast()) {
                    let call = std::mem::transmute::<
                        unsafe extern "C" fn(),
                        unsafe extern "C" fn(*mut Display, c_ulong, c_int),
                    >(address);
                    call(self.display, self.window, interval);
                }
            } else if extensions
                .split_whitespace()
                .any(|name| name == "GLX_MESA_swap_control")
                && let Some(address) = glXGetProcAddressARB(c"glXSwapIntervalMESA".as_ptr().cast())
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
            // Flush only after issuing requests: an unconditional XFlush polls the
            // connection (a syscall and Xlib allocation) on every frame.
            let mut requested = false;
            if visible != self.cursor_visible {
                requested = true;
                if visible {
                    XUndefineCursor(self.display, self.window);
                } else {
                    if self.hidden_cursor == 0 {
                        let blank = [0_i8];
                        let pixmap =
                            XCreateBitmapFromData(self.display, self.window, blank.as_ptr(), 1, 1);
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
                requested = true;
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
            if requested || desired && !self.grabbed {
                // A failed grab attempt is retried (and flushed) every frame, as before.
                XFlush(self.display);
            }
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
                            output.push(Event::MouseMove(
                                button.x as f32 / self.scale,
                                button.y as f32 / self.scale,
                            ));
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
                                        delta.0 as f32 / self.scale,
                                        delta.1 as f32 / self.scale,
                                    ));
                                }
                            }
                        } else {
                            output.push(Event::MouseMove(
                                position.0 as f32 / self.scale,
                                position.1 as f32 / self.scale,
                            ));
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
                            output
                                .push(Event::Resize(self.logical_size().0, self.logical_size().1));
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
                    18 => output.push(Event::Suspend(true)),
                    19 => output.push(Event::Suspend(false)),
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
    pub(super) fn make_current(&self) -> bool {
        // SAFETY: All three handles are owned and live while alive is true.
        unsafe {
            self.alive
                && !self.context.is_null()
                && glXMakeCurrent(self.display, self.window, self.context) != 0
        }
    }

    pub(super) fn swap(&self) {
        unsafe {
            glXSwapBuffers(self.display, self.window);
        }
    }
    pub(super) fn logical_size(&self) -> (u32, u32) {
        (
            (self.width as f32 / self.scale).round().max(1.) as u32,
            (self.height as f32 / self.scale).round().max(1.) as u32,
        )
    }
    pub(super) fn dpi_scale(&self) -> f32 {
        self.scale
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        unsafe {
            if self.grabbed {
                XUngrabPointer(self.display, 0);
            }
            if self.hidden_cursor != 0 {
                XFreeCursor(self.display, self.hidden_cursor);
            }
            if !self.context.is_null() {
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
}

pub(super) fn default_framebuffer() -> u32 {
    0
}

// DPI uses a bounded decimal grammar with checked mantissa arithmetic.
// Exponent work is capped at 38.
fn decimal(value: &str) -> Option<f32> {
    let value = value.trim().strip_prefix('+').unwrap_or(value.trim());
    if value.is_empty() || value.len() > 32 {
        return None;
    }
    let (mantissa, exponent) = value
        .find(['e', 'E'])
        .map_or((value, None), |i| (&value[..i], Some(&value[i + 1..])));
    let (mut coefficient, mut fraction, mut dot, mut digits) = (0u64, 0u32, false, false);
    for b in mantissa.bytes() {
        match b {
            b'0'..=b'9' => {
                coefficient = coefficient
                    .checked_mul(10)?
                    .checked_add(u64::from(b - b'0'))?;
                digits = true;
                if dot {
                    fraction += 1;
                }
            }
            b'.' if !dot => dot = true,
            _ => return None,
        }
    }
    if !digits {
        return None;
    }
    let mut number = coefficient as f64;
    for _ in 0..fraction {
        number *= 0.1;
    }
    if let Some(exponent) = exponent {
        let (negative, text) = if let Some(v) = exponent.strip_prefix('-') {
            (true, v)
        } else {
            (false, exponent.strip_prefix('+').unwrap_or(exponent))
        };
        if text.is_empty() {
            return None;
        }
        let mut power = 0u32;
        for b in text.bytes() {
            if !b.is_ascii_digit() {
                return None;
            }
            power = power.checked_mul(10)?.checked_add(u32::from(b - b'0'))?;
            if power > 38 {
                return None;
            }
        }
        for _ in 0..power {
            number *= if negative { 0.1 } else { 10. };
        }
    }
    let number = number as f32;
    number.is_finite().then_some(number)
}
fn display_scale(override_value: Option<&str>, resources: &str) -> f32 {
    let valid = |n: f32| (0.5..=4.).contains(&n).then_some(n);
    override_value
        .and_then(decimal)
        .and_then(valid)
        .or_else(|| {
            resources.lines().find_map(|line| {
                let (k, v) = line.split_once(':')?;
                if k.trim().eq_ignore_ascii_case("Xft.dpi") {
                    decimal(v).and_then(|dpi| valid(dpi / 96.))
                } else {
                    None
                }
            })
        })
        .unwrap_or(1.)
}
#[cfg(test)]
mod dpi_tests {
    use super::*;
    #[test]
    fn logical_scaling_rejects_bad_settings_and_honors_desktop_dpi() {
        assert_eq!(display_scale(None, "Xft.dpi:\t144"), 1.5);
        assert_eq!(display_scale(Some("2"), "Xft.dpi:96"), 2.);
        assert_eq!(display_scale(Some("NaN"), "Xft.dpi:192"), 2.);
        assert_eq!(display_scale(None, "Xft.dpi:100000"), 1.);
        assert_eq!(display_scale(Some("+1.25"), ""), 1.25);
        assert_eq!(display_scale(Some(".5"), ""), 0.5);
        assert_eq!(display_scale(None, "Xft.dpi:1.44e2"), 1.5);
        for bad in [
            "",
            ".",
            "-1",
            "1e",
            "1e9999999",
            "999999999999999999999999999999",
            "1.2.3",
            "inf",
            "1e2e0",
        ] {
            assert!(decimal(bad).is_none(), "{bad}");
        }
    }
}
