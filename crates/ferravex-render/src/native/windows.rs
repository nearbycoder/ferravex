//! First-party Win32/WGL host. No windowing or GL-loader crate is used.
//! ABI references: <https://learn.microsoft.com/windows/win32/api/wingdi/nf-wingdi-wglgetprocaddress>
//! and <https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-getrawinputdata>.
#![allow(non_snake_case)]
use super::{Conf, Event};
use std::{
    collections::BTreeSet,
    ffi::{CStr, c_char, c_void},
    mem::size_of,
    ptr,
};
type Handle = *mut c_void;
#[repr(C)]
#[derive(Default, Clone, Copy)]
struct Point {
    x: i32,
    y: i32,
}
#[repr(C)]
#[derive(Default, Clone, Copy)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}
#[repr(C)]
struct Message {
    hwnd: Handle,
    message: u32,
    wparam: usize,
    lparam: isize,
    time: u32,
    point: Point,
    private: u32,
}
#[repr(C)]
struct Class {
    size: u32,
    style: u32,
    proc: unsafe extern "system" fn(Handle, u32, usize, isize) -> isize,
    class_extra: i32,
    window_extra: i32,
    instance: Handle,
    icon: Handle,
    cursor: Handle,
    background: Handle,
    menu: *const u16,
    name: *const u16,
    small_icon: Handle,
}
#[repr(C)]
#[derive(Default)]
struct PixelFormat {
    size: u16,
    version: u16,
    flags: u32,
    pixel_type: u8,
    color_bits: u8,
    red_bits: u8,
    red_shift: u8,
    green_bits: u8,
    green_shift: u8,
    blue_bits: u8,
    blue_shift: u8,
    alpha_bits: u8,
    alpha_shift: u8,
    accum_bits: u8,
    accum_red: u8,
    accum_green: u8,
    accum_blue: u8,
    accum_alpha: u8,
    depth_bits: u8,
    stencil_bits: u8,
    aux: u8,
    layer_type: u8,
    reserved: u8,
    layer_mask: u32,
    visible_mask: u32,
    damage_mask: u32,
}
#[repr(C)]
struct RawDevice {
    page: u16,
    usage: u16,
    flags: u32,
    target: Handle,
}
#[repr(C)]
#[derive(Default)]
struct RawHeader {
    kind: u32,
    size: u32,
    device: Handle,
    wparam: usize,
}
#[repr(C)]
#[derive(Default)]
struct RawMouse {
    flags: u16,
    padding: u16,
    buttons: u32,
    raw_buttons: u32,
    x: i32,
    y: i32,
    extra: u32,
}
#[repr(C)]
#[derive(Default)]
struct RawInput {
    header: RawHeader,
    mouse: RawMouse,
}
#[link(name = "user32")]
unsafe extern "system" {
    fn LoadCursorW(instance: Handle, name: *const u16) -> Handle;
    fn RegisterClassExW(class: *const Class) -> u16;
    fn CreateWindowExW(
        ex: u32,
        class: *const u16,
        title: *const u16,
        style: u32,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        parent: Handle,
        menu: Handle,
        instance: Handle,
        param: Handle,
    ) -> Handle;
    fn DefWindowProcW(hwnd: Handle, msg: u32, wp: usize, lp: isize) -> isize;
    fn DestroyWindow(hwnd: Handle) -> i32;
    fn IsWindow(hwnd: Handle) -> i32;
    fn ShowWindow(hwnd: Handle, cmd: i32) -> i32;
    fn SetFocus(hwnd: Handle) -> Handle;
    fn GetFocus() -> Handle;
    fn GetDC(hwnd: Handle) -> Handle;
    fn ReleaseDC(hwnd: Handle, dc: Handle) -> i32;
    fn PeekMessageW(msg: *mut Message, hwnd: Handle, min: u32, max: u32, remove: u32) -> i32;
    fn TranslateMessage(msg: *const Message) -> i32;
    fn DispatchMessageW(msg: *const Message) -> isize;
    fn GetClientRect(hwnd: Handle, rect: *mut Rect) -> i32;
    fn AdjustWindowRectExForDpi(rect: *mut Rect, style: u32, menu: i32, ex: u32, dpi: u32) -> i32;
    fn AdjustWindowRectEx(rect: *mut Rect, style: u32, menu: i32, ex: u32) -> i32;
    fn SetWindowPos(hwnd: Handle, after: Handle, x: i32, y: i32, w: i32, h: i32, flags: u32)
    -> i32;
    fn PostMessageW(hwnd: Handle, msg: u32, wp: usize, lp: isize) -> i32;
    fn GetDpiForWindow(hwnd: Handle) -> u32;
    fn SetProcessDpiAwarenessContext(context: Handle) -> i32;
    fn RegisterRawInputDevices(devices: *const RawDevice, count: u32, size: u32) -> i32;
    fn GetRawInputData(
        input: Handle,
        command: u32,
        data: Handle,
        size: *mut u32,
        header_size: u32,
    ) -> u32;
    fn SetCapture(hwnd: Handle) -> Handle;
    fn ReleaseCapture() -> i32;
    fn ClipCursor(rect: *const Rect) -> i32;
    fn ClientToScreen(hwnd: Handle, point: *mut Point) -> i32;
    fn ShowCursor(show: i32) -> i32;
    fn IsIconic(hwnd: Handle) -> i32;
}
#[link(name = "gdi32")]
unsafe extern "system" {
    fn ChoosePixelFormat(dc: Handle, pfd: *const PixelFormat) -> i32;
    fn SetPixelFormat(dc: Handle, index: i32, pfd: *const PixelFormat) -> i32;
    fn SwapBuffers(dc: Handle) -> i32;
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetModuleHandleW(name: *const u16) -> Handle;
    fn GetModuleHandleA(name: *const c_char) -> Handle;
    fn GetProcAddress(module: Handle, name: *const c_char) -> *const c_void;
}
#[link(name = "opengl32")]
unsafe extern "system" {
    fn wglCreateContext(dc: Handle) -> Handle;
    fn wglDeleteContext(context: Handle) -> i32;
    fn wglMakeCurrent(dc: Handle, context: Handle) -> i32;
    fn wglGetProcAddress(name: *const c_char) -> *const c_void;
}
const CLOSE: u32 = 0x8001;
unsafe extern "system" fn window_proc(hwnd: Handle, msg: u32, wp: usize, lp: isize) -> isize {
    // SAFETY: Win32 invokes this with live OS-owned message data. DPI rectangles
    // are borrowed only during this call; no Rust reference escapes the callback.
    unsafe {
        match msg {
            0x10 => {
                PostMessageW(hwnd, CLOSE, 0, 0);
                0
            }
            0x2e0 => {
                let r = &*(lp as *const Rect);
                SetWindowPos(
                    hwnd,
                    ptr::null_mut(),
                    r.left,
                    r.top,
                    r.right - r.left,
                    r.bottom - r.top,
                    0x14,
                );
                0
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}
fn wide(s: &str) -> Vec<u16> {
    s.replace('\0', " ").encode_utf16().chain([0]).collect()
}
pub(super) fn proc_address(name: &CStr) -> *const c_void {
    // SAFETY: Called only with this thread's current WGL context. Legacy exports
    // come from opengl32; extension addresses are specific to the pixel format.
    unsafe {
        let p = wglGetProcAddress(name.as_ptr());
        if (p as usize) > 3 && p as isize != -1 {
            p
        } else {
            GetProcAddress(GetModuleHandleA(c"opengl32.dll".as_ptr()), name.as_ptr())
        }
    }
}
pub(super) fn default_framebuffer() -> u32 {
    0
}
pub(super) struct Host {
    hwnd: Handle,
    dc: Handle,
    context: Handle,
    width: u32,
    height: u32,
    scale: f32,
    high_dpi: bool,
    focused: bool,
    suspended: bool,
    grabbed: bool,
    visible: bool,
    quit: bool,
    keys: BTreeSet<u32>,
    buttons: BTreeSet<u32>,
}
impl Host {
    pub(super) fn new(conf: &Conf) -> Result<Self, String> {
        if !(1..=16384).contains(&conf.window_width) || !(1..=16384).contains(&conf.window_height) {
            return Err("Invalid window dimensions".into());
        }
        // SAFETY: This owner unwinds all allocated Win32/WGL handles on every error.
        unsafe {
            if conf.high_dpi {
                SetProcessDpiAwarenessContext(-4isize as Handle);
            }
            let instance = GetModuleHandleW(ptr::null());
            let class = wide("FerravexWindowV1");
            RegisterClassExW(&Class {
                size: size_of::<Class>() as u32,
                style: 0x20,
                proc: window_proc,
                class_extra: 0,
                window_extra: 0,
                instance,
                icon: ptr::null_mut(),
                cursor: LoadCursorW(ptr::null_mut(), 32512usize as *const u16),
                background: ptr::null_mut(),
                menu: ptr::null(),
                name: class.as_ptr(),
                small_icon: ptr::null_mut(),
            });
            let style = 0x00cf0000;
            let mut rect = Rect {
                right: conf.window_width,
                bottom: conf.window_height,
                ..Default::default()
            };
            AdjustWindowRectEx(&mut rect, style, 0, 0);
            let hwnd = CreateWindowExW(
                0,
                class.as_ptr(),
                wide(&conf.window_title).as_ptr(),
                style,
                0x80000000u32 as i32,
                0x80000000u32 as i32,
                rect.right - rect.left,
                rect.bottom - rect.top,
                ptr::null_mut(),
                ptr::null_mut(),
                instance,
                ptr::null_mut(),
            );
            if hwnd.is_null() {
                return Err("Win32 window creation failed".into());
            }
            let mut owner = Self {
                hwnd,
                dc: ptr::null_mut(),
                context: ptr::null_mut(),
                width: 0,
                height: 0,
                scale: 1.,
                high_dpi: conf.high_dpi,
                focused: false,
                suspended: false,
                grabbed: false,
                visible: true,
                quit: false,
                keys: BTreeSet::new(),
                buttons: BTreeSet::new(),
            };
            owner.dc = GetDC(hwnd);
            if owner.dc.is_null() {
                return Err("Win32 device context unavailable".into());
            }
            let pfd = PixelFormat {
                size: size_of::<PixelFormat>() as u16,
                version: 1,
                flags: 0x25,
                color_bits: 32,
                alpha_bits: 8,
                depth_bits: 24,
                stencil_bits: 8,
                ..Default::default()
            };
            let format = ChoosePixelFormat(owner.dc, &pfd);
            if format == 0 || SetPixelFormat(owner.dc, format, &pfd) == 0 {
                return Err("WGL RGBA/depth pixel format unavailable".into());
            }
            owner.context = wglCreateContext(owner.dc);
            if owner.context.is_null() || wglMakeCurrent(owner.dc, owner.context) == 0 {
                return Err("WGL bootstrap context failed".into());
            }
            let p = proc_address(c"wglCreateContextAttribsARB");
            if p.is_null() {
                return Err("OpenGL 3.3 context creation requires WGL_ARB_create_context".into());
            }
            let create = std::mem::transmute::<
                *const c_void,
                unsafe extern "system" fn(Handle, Handle, *const i32) -> Handle,
            >(p);
            let attributes = [0x2091, 3, 0x2092, 3, 0x9126, 1, 0];
            let context = create(owner.dc, ptr::null_mut(), attributes.as_ptr());
            if context.is_null() {
                return Err("OpenGL 3.3 core context unavailable".into());
            }
            wglMakeCurrent(ptr::null_mut(), ptr::null_mut());
            wglDeleteContext(owner.context);
            owner.context = context;
            if !owner.make_current() {
                return Err("WGL context activation failed".into());
            }
            let device = RawDevice {
                page: 1,
                usage: 2,
                flags: 0,
                target: hwnd,
            };
            if RegisterRawInputDevices(&device, 1, size_of::<RawDevice>() as u32) == 0 {
                return Err("Raw mouse registration failed".into());
            }
            ShowWindow(hwnd, 5);
            SetFocus(hwnd);
            owner.update_size();
            if owner.high_dpi {
                let mut r = Rect {
                    right: (conf.window_width as f32 * owner.scale).round() as i32,
                    bottom: (conf.window_height as f32 * owner.scale).round() as i32,
                    ..Default::default()
                };
                AdjustWindowRectExForDpi(&mut r, style, 0, 0, GetDpiForWindow(hwnd));
                SetWindowPos(
                    hwnd,
                    ptr::null_mut(),
                    0,
                    0,
                    r.right - r.left,
                    r.bottom - r.top,
                    0x16,
                );
                owner.update_size();
            }
            owner.set_swap_interval(1);
            Ok(owner)
        }
    }
    fn update_size(&mut self) {
        unsafe {
            let mut r = Rect::default();
            if GetClientRect(self.hwnd, &mut r) != 0 {
                self.width = (r.right - r.left).max(1) as u32;
                self.height = (r.bottom - r.top).max(1) as u32;
            }
            self.scale = if self.high_dpi {
                (GetDpiForWindow(self.hwnd) as f32 / 96.).clamp(0.5, 4.)
            } else {
                1.
            };
        }
    }
    pub(super) fn size(&self) -> (u32, u32) {
        (self.width, self.height)
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
    pub(super) fn make_current(&self) -> bool {
        unsafe {
            !self.context.is_null()
                && IsWindow(self.hwnd) != 0
                && wglMakeCurrent(self.dc, self.context) != 0
        }
    }
    pub(super) fn swap(&self) {
        unsafe {
            SwapBuffers(self.dc);
        }
    }
    pub(super) fn set_swap_interval(&mut self, interval: i32) {
        if interval < 0 || !self.make_current() {
            return;
        }
        let p = proc_address(c"wglSwapIntervalEXT");
        if !p.is_null() {
            unsafe {
                std::mem::transmute::<*const c_void, unsafe extern "system" fn(i32) -> i32>(p)(
                    interval,
                );
            }
        }
    }
    pub(super) fn set_cursor(&mut self, grab: bool, visible: bool) {
        unsafe {
            let grab = grab && self.focused && !self.suspended;
            if grab {
                let mut r = Rect::default();
                GetClientRect(self.hwnd, &mut r);
                let mut a = Point {
                    x: r.left,
                    y: r.top,
                };
                let mut b = Point {
                    x: r.right,
                    y: r.bottom,
                };
                ClientToScreen(self.hwnd, &mut a);
                ClientToScreen(self.hwnd, &mut b);
                ClipCursor(&Rect {
                    left: a.x,
                    top: a.y,
                    right: b.x,
                    bottom: b.y,
                });
            } else if self.grabbed {
                ClipCursor(ptr::null());
            }
            self.grabbed = grab;
            let visible = visible || !self.focused;
            if visible != self.visible {
                ShowCursor(i32::from(visible));
                self.visible = visible;
            }
        }
    }
    pub(super) fn poll(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        unsafe {
            if IsWindow(self.hwnd) == 0 {
                if !self.quit {
                    events.push(Event::Quit);
                    self.quit = true;
                }
                return events;
            }
            let old = self.size();
            self.update_size();
            if old != self.size() {
                let (w, h) = self.logical_size();
                events.push(Event::Resize(w, h));
            }
            let focused = GetFocus() == self.hwnd;
            if focused != self.focused {
                self.focused = focused;
                events.push(Event::Focus(focused));
                if !focused {
                    for k in std::mem::take(&mut self.keys) {
                        events.push(Event::Key(k, false));
                    }
                    for b in std::mem::take(&mut self.buttons) {
                        events.push(Event::MouseButton(b, false));
                    }
                    self.set_cursor(false, true);
                    ReleaseCapture();
                }
            }
            let suspended = IsIconic(self.hwnd) != 0;
            if suspended != self.suspended {
                self.suspended = suspended;
                events.push(Event::Suspend(suspended));
            }
            for _ in 0..4096 {
                let mut m: Message = std::mem::zeroed();
                if PeekMessageW(&mut m, ptr::null_mut(), 0, 0, 1) == 0 {
                    break;
                }
                if m.message == 0x12 || m.hwnd == self.hwnd && m.message == CLOSE {
                    if !self.quit {
                        events.push(Event::Quit);
                        self.quit = true;
                    }
                    continue;
                }
                if m.hwnd == self.hwnd {
                    match m.message {
                        0x100 | 0x104 | 0x101 | 0x105 => {
                            let code = key(m.wparam as u32, m.lparam);
                            let down = m.message == 0x100 || m.message == 0x104;
                            if code != 0
                                && if down {
                                    self.keys.insert(code)
                                } else {
                                    self.keys.remove(&code)
                                }
                            {
                                events.push(Event::Key(code, down));
                            }
                        }
                        0x200 => {
                            if !self.grabbed {
                                events.push(Event::MouseMove(
                                    (m.lparam as u16 as i16) as f32 / self.scale,
                                    ((m.lparam >> 16) as u16 as i16) as f32 / self.scale,
                                ));
                            }
                        }
                        0xff if self.grabbed => {
                            let mut raw = RawInput::default();
                            let mut n = size_of::<RawInput>() as u32;
                            let count = GetRawInputData(
                                m.lparam as Handle,
                                0x10000003,
                                std::ptr::from_mut(&mut raw).cast(),
                                &mut n,
                                size_of::<RawHeader>() as u32,
                            );
                            if count >= size_of::<RawInput>() as u32
                                && count != u32::MAX
                                && raw.header.kind == 0
                                && raw.mouse.flags & 1 == 0
                            {
                                events.push(Event::MouseDelta(
                                    raw.mouse.x as f32 / self.scale,
                                    raw.mouse.y as f32 / self.scale,
                                ));
                            }
                        }
                        0x201..=0x209 => {
                            let (button, down) = match m.message {
                                0x201 => (1, true),
                                0x202 => (1, false),
                                0x204 => (3, true),
                                0x205 => (3, false),
                                0x207 => (2, true),
                                0x208 => (2, false),
                                _ => (0, false),
                            };
                            if button != 0 {
                                if down {
                                    self.buttons.insert(button);
                                    SetCapture(self.hwnd);
                                } else {
                                    self.buttons.remove(&button);
                                    if self.buttons.is_empty() {
                                        ReleaseCapture();
                                    }
                                }
                                events.push(Event::MouseButton(button, down));
                            }
                        }
                        0x20a => events
                            .push(Event::Wheel(((m.wparam >> 16) as u16 as i16) as f32 / 120.)),
                        _ => {}
                    }
                }
                TranslateMessage(&m);
                DispatchMessageW(&m);
            }
        }
        events
    }
}
fn key(v: u32, lp: isize) -> u32 {
    match v {
        0x41..=0x5a => v + 32,
        0x30..=0x39 => v,
        0x70..=0x7b => 0xffbe + v - 0x70,
        0x20 => 32,
        0x0d => 0xff0d,
        0x1b => 0xff1b,
        0x08 => 0xff08,
        0x09 => 0xff09,
        0x25 => 0xff51,
        0x26 => 0xff52,
        0x27 => 0xff53,
        0x28 => 0xff54,
        0x10 => {
            if (lp >> 16) & 255 == 54 {
                0xffe2
            } else {
                0xffe1
            }
        }
        0x11 => {
            if lp & (1 << 24) != 0 {
                0xffe4
            } else {
                0xffe3
            }
        }
        0x12 => {
            if lp & (1 << 24) != 0 {
                0xffea
            } else {
                0xffe9
            }
        }
        0x2e => 0xffff,
        _ => 0,
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        unsafe {
            if self.grabbed {
                ClipCursor(ptr::null());
            }
            if !self.visible {
                ShowCursor(1);
            }
            if !self.context.is_null() {
                wglMakeCurrent(ptr::null_mut(), ptr::null_mut());
                wglDeleteContext(self.context);
            }
            if !self.dc.is_null() {
                ReleaseDC(self.hwnd, self.dc);
            }
            if !self.hwnd.is_null() && IsWindow(self.hwnd) != 0 {
                DestroyWindow(self.hwnd);
            }
        }
    }
}
