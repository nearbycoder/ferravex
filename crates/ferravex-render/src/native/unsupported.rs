//! Explicit fallback for native targets without an OS drawable implementation.
use super::{Conf, Event};
use std::ffi::{CStr, c_void};
pub(super) fn proc_address(_: &CStr) -> *const c_void {
    std::ptr::null()
}
pub(super) fn default_framebuffer() -> u32 {
    0
}
pub(super) struct Host;
impl Host {
    pub(super) fn new(_: &Conf) -> Result<Self, String> {
        Err("No interactive host for this native OS; use the portable core or browser build".into())
    }
    pub(super) fn size(&self) -> (u32, u32) {
        (0, 0)
    }
    pub(super) fn logical_size(&self) -> (u32, u32) {
        (0, 0)
    }
    pub(super) fn dpi_scale(&self) -> f32 {
        1.
    }
    pub(super) fn make_current(&self) -> bool {
        false
    }
    pub(super) fn swap(&self) {}
    pub(super) fn set_swap_interval(&mut self, _: i32) {}
    pub(super) fn set_cursor(&mut self, _: bool, _: bool) {}
    pub(super) fn poll(&mut self) -> Vec<Event> {
        vec![Event::Quit]
    }
}
