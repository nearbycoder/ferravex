//! Typed Objective-C runtime calls used by our AppKit and UIKit hosts.
#![allow(non_snake_case, dead_code)]
use std::ffi::{CStr, c_char, c_void};
pub(super) type Id = *mut c_void;
pub(super) type Sel = *mut c_void;
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(super) struct Point {
    pub x: f64,
    pub y: f64,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(super) struct Size {
    pub width: f64,
    pub height: f64,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(super) struct Rect {
    pub origin: Point,
    pub size: Size,
}
impl Rect {
    pub fn new(w: f64, h: f64) -> Self {
        Self {
            origin: Point::default(),
            size: Size {
                width: w,
                height: h,
            },
        }
    }
}
#[link(name = "objc")]
unsafe extern "C" {
    fn objc_getClass(name: *const c_char) -> Id;
    fn sel_registerName(name: *const c_char) -> Sel;
    pub(super) fn objc_msgSend();
    #[cfg(target_arch = "x86_64")]
    fn objc_msgSend_stret();
    pub(super) fn objc_allocateClassPair(superclass: Id, name: *const c_char, extra: usize) -> Id;
    pub(super) fn objc_getProtocol(name: *const c_char) -> Id;
    pub(super) fn class_addProtocol(class: Id, protocol: Id) -> i8;
    pub(super) fn objc_registerClassPair(class: Id);
    pub(super) fn class_addMethod(
        class: Id,
        selector: Sel,
        implementation: *const c_void,
        types: *const c_char,
    ) -> i8;
}
pub(super) fn class(name: &CStr) -> Id {
    unsafe { objc_getClass(name.as_ptr()) }
}
pub(super) fn sel(name: &CStr) -> Sel {
    unsafe { sel_registerName(name.as_ptr()) }
}
// Each helper fixes the complete ABI, including CGFloat/aggregate returns.
macro_rules! messages {
 ($($name:ident($($arg:ident:$ty:ty),*) -> $ret:ty;)*)=>{$(
 #[allow(dead_code)]
 pub(super) unsafe fn $name(receiver:Id,selector:&CStr,$($arg:$ty),*)->$ret {
  unsafe {std::mem::transmute::<unsafe extern "C" fn(),unsafe extern "C" fn(Id,Sel,$($ty),*)->$ret>(objc_msgSend)(receiver,sel(selector),$($arg),*)}
 }
 )*}
}
messages! {
 id()->Id; id_id(value:Id)->Id; id_usize(value:usize)->Id;
 void()->(); void_id(value:Id)->(); void_bool(value:i8)->(); void_u64(value:u64)->();
 bool()->i8; uint()->usize; float()->f64; point()->Point; cstr()->*const c_char;
 point_id(point:Point,from:Id)->Point;
 point_object(value:Id)->Point;
 display_link(target:Id,selector:Sel)->Id;
 add_runloop(runloop:Id,mode:Id)->();
 drawable(target:u64,layer:Id)->i8;
 bool_id(value:Id)->i8;
 event(mask:u64,date:Id,mode:Id,dequeue:i8)->Id;
 window(rect:Rect,style:u64,backing:u64,defer:i8)->Id;
 view(rect:Rect,format:Id)->Id;
 init_rect(rect:Rect)->Id;
 format(attributes:*const u32)->Id;
 context(format:Id,shared:Id)->Id;
 parameter(value:*const i32,parameter:u64)->();
 string(bytes:*const c_char)->Id;
}
pub(super) unsafe fn rect(receiver: Id, selector: &CStr) -> Rect {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let mut out = Rect::default();
        std::mem::transmute::<unsafe extern "C" fn(), unsafe extern "C" fn(*mut Rect, Id, Sel)>(
            objc_msgSend_stret,
        )(&mut out, receiver, sel(selector));
        out
    }
    #[cfg(not(target_arch = "x86_64"))]
    unsafe {
        std::mem::transmute::<unsafe extern "C" fn(), unsafe extern "C" fn(Id, Sel) -> Rect>(
            objc_msgSend,
        )(receiver, sel(selector))
    }
}
pub(super) fn text(value: &str) -> Id {
    let s = std::ffi::CString::new(value.replace('\0', " ")).unwrap();
    unsafe { string(class(c"NSString"), c"stringWithUTF8String:", s.as_ptr()) }
}
pub(super) struct Pool(Id);
impl Pool {
    pub fn new() -> Self {
        unsafe { Self(id(class(c"NSAutoreleasePool"), c"new")) }
    }
}
impl Drop for Pool {
    fn drop(&mut self) {
        unsafe {
            void(self.0, c"drain");
        }
    }
}
