//! Original system window/input/context bindings and shared native GL submission.
//! Linux uses X11/GLX, Windows uses Win32/WGL, macOS uses AppKit/NSOpenGL,
//! and iOS uses UIKit scenes/CADisplayLink/EAGL. No third-party Rust crates.
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
    Suspend(bool),
    Touch { id: u64, phase: u32, x: f32, y: f32 },
    Quit,
}

mod gl;
mod gpu;
#[cfg(target_os = "linux")]
mod linux;
pub use gpu::NativeWindow;
#[cfg(target_os = "linux")]
use linux as host;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
use windows as host;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_vendor = "apple")]
mod objc;
#[cfg(target_os = "macos")]
use macos as host;

#[cfg(target_os = "ios")]
pub(crate) mod ios;
#[cfg(target_os = "ios")]
use ios as host;

#[cfg(target_vendor = "apple")]
fn apple_key(c: char) -> u32 {
    match c {
        '\u{1b}' => 0xff1b,
        '\r' | '\n' => 0xff0d,
        '\t' => 0xff09,
        '\u{7f}' => 0xff08,
        '\u{f700}' => 0xff52,
        '\u{f701}' => 0xff54,
        '\u{f702}' => 0xff51,
        '\u{f703}' => 0xff53,
        '\u{f704}'..='\u{f70f}' => 0xffbe + c as u32 - 0xf704,
        '\u{f728}' => 0xffff,
        c if c.is_ascii() => c.to_ascii_lowercase() as u32,
        _ => 0,
    }
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "windows",
    target_os = "macos",
    target_os = "ios"
)))]
mod unsupported;
#[cfg(not(any(
    target_os = "linux",
    target_os = "windows",
    target_os = "macos",
    target_os = "ios"
)))]
use unsupported as host;
