//! Runtime input uses logical display pixels. Legacy polling remains available;
//! PlayerInput consumes the same ordered, bounded event journal.
use crate::prelude::*;
pub use ferravex_play::input::*;
pub mod calibration;
mod platform;
mod player;
mod presets;
pub mod touch_layout;
pub use player::{CaptureDevice, ControlsStatus, HintDevice, HintError, PlayerInput};
pub use player::{VisibleHintError, VisibleTouchLabel};
pub use presets::Preset;
mod storage;
pub use platform::PadState;
pub(crate) use platform::{MAX_EVENTS, PlatformEvent, PlatformInput};
pub use storage::StorageError;
pub fn logical_touches() -> Vec<Touch> {
    touches()
}

/// Feed a logical-pixel touch from an embedding platform host.
pub fn touch_event(event: ferravex_play::input::touch::Event) {
    use ferravex_play::input::touch::Phase;
    crate::touch(
        event.id,
        match event.phase {
            Phase::Start => 0,
            Phase::Move => 1,
            Phase::End => 2,
            Phase::Cancel => 3,
        },
        event.position.x,
        event.position.y,
    );
}
/// Current controller states; at most four controllers and fixed channel counts.
pub fn gamepads() -> [PadState; 4] {
    crate::read(|s| s.input.pads)
}
/// Supply a controller sample from a first-party embedding host. The same bounded
/// journal and normalized axes/buttons are used by browser and Linux providers.
pub fn gamepad_sample(index: usize, state: PadState) -> bool {
    if index >= 4 {
        return false;
    }
    crate::edit(|s| {
        s.input.pad_connected(index, state.connected);
        if state.connected {
            for (i, v) in state.axes.into_iter().enumerate() {
                s.input.pad_axis(index, i, v);
            }
            for (i, v) in state.buttons.into_iter().enumerate() {
                s.input.pad_button(index, i, v);
            }
        }
    });
    true
}
#[cfg(target_os = "linux")]
mod gamepad;
#[cfg(target_os = "linux")]
pub(crate) fn poll_gamepads() {
    thread_local! {static PADS:std::cell::RefCell<gamepad::Gamepads>=std::cell::RefCell::new(gamepad::Gamepads::default());}
    if !crate::read(|s| s.input_enabled) {
        return;
    }
    let pads = PADS.with(|p| p.borrow_mut().poll());
    crate::edit(|s| {
        for (i, pad) in pads.iter().enumerate() {
            s.input.pad_connected(i, pad.connected);
            if pad.connected {
                for (a, &value) in pad.axes.iter().enumerate() {
                    s.input.pad_axis(i, a, value);
                }
                for (b, &value) in pad.buttons.iter().enumerate() {
                    s.input.pad_button(i, b, value);
                }
            }
        }
    });
}
#[cfg(all(not(target_os = "linux"), not(target_arch = "wasm32")))]
pub(crate) fn poll_gamepads() {}

/// Drive the shared action journal and optional immediate UI from a custom
/// renderer host. Call once per render frame, outside a `start` game future.
/// The callback can sample PlayerInput, advance its game, then draw settings.
/// UI vertices remain available through the normal renderer ABI after return.
pub fn host_frame<T>(dt: f32, width: u32, height: u32, run: impl FnOnce() -> T) -> T {
    crate::begin_host_frame(dt, width, height);
    let value = run();
    crate::end_host_frame();
    value
}
