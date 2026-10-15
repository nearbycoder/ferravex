//! Shared runtime audio. Rust owns clips, voices and mixing; hosts consume PCM.
use crate::prelude::{Vec3, vec3};
pub use ferravex_play::audio::*;
use std::cell::{Cell, RefCell};
#[cfg(not(target_arch = "wasm32"))]
mod capture;
mod game;
#[cfg(target_os = "linux")]
mod native;
pub use game::GameAudio;
pub const SAMPLE_RATE: u32 = 48000;
pub const MAX_PULL_FRAMES: usize = 4096;
struct Audio {
    mixer: Mixer,
    bank: Option<SoundBank>,
    fraction: f64,
}
thread_local! {
    static AUDIO:RefCell<Option<Audio>>=const {RefCell::new(None)};
    static MUTED:Cell<bool>=const {Cell::new(false)};
    static GAIN:Cell<f32>=const {Cell::new(0.7)};
    static BROWSER_RUNNING:Cell<bool>=const {Cell::new(false)};
    static PCM:RefCell<Vec<[f32;2]>>=const {RefCell::new(Vec::new())};
    static OFFLINE:Cell<bool>=const {Cell::new(false)};
}
fn initialize() {
    AUDIO.with(|cell| {
        let mut state = cell.borrow_mut();
        if state.is_none() {
            *state = Some(Audio {
                mixer: Mixer::new(SAMPLE_RATE, 64).unwrap(),
                bank: None,
                fraction: 0.,
            });
        }
    });
}
pub(crate) fn reset() {
    AUDIO.with(|a| *a.borrow_mut() = None);
    BROWSER_RUNNING.with(|v| v.set(false));
}
pub fn muted() -> bool {
    MUTED.with(Cell::get)
}
pub fn set_muted(value: bool) {
    if value && !muted() {
        suspend();
    }
    MUTED.with(|v| v.set(value));
}
pub fn suspend() {
    AUDIO.with(|a| {
        if let Some(a) = a.borrow_mut().as_mut() {
            a.mixer.suspend();
        }
    });
}
pub(crate) fn focus_lost() {
    if !OFFLINE.with(Cell::get) {
        suspend();
    }
}
pub fn set_master_gain(value: f32) -> bool {
    if !value.is_finite() || !(0. ..=1.).contains(&value) {
        return false;
    }
    GAIN.with(|v| v.set(value));
    true
}
pub fn set_bus_gain(bus: Bus, value: f32) -> bool {
    initialize();
    AUDIO.with(|a| {
        a.borrow_mut()
            .as_mut()
            .unwrap()
            .mixer
            .set_bus_gain(bus, value)
    })
}
pub fn listener(position: Vec3, right: Vec3) -> bool {
    if !position.is_finite() || !right.is_finite() || right.length_squared() < 1e-8 {
        return false;
    }
    initialize();
    AUDIO.with(|a| {
        let mut a = a.borrow_mut();
        let m = &mut a.as_mut().unwrap().mixer;
        m.listener = position.to_array();
        m.right = right.normalize().to_array();
    });
    true
}
/// Generate the built-in bank outside a frame/audio callback when desired.
pub fn preload_cues() {
    initialize();
    AUDIO.with(|a| {
        let mut a = a.borrow_mut();
        a.as_mut()
            .unwrap()
            .bank
            .get_or_insert_with(|| SoundBank::procedural(24000).unwrap());
    });
}
pub fn play_sound(sound: &Sound, params: VoiceParams) -> Option<VoiceHandle> {
    initialize();
    AUDIO.with(|a| a.borrow_mut().as_mut().unwrap().mixer.start(sound, params))
}
pub fn stop_all() {
    AUDIO.with(|a| {
        if let Some(a) = a.borrow_mut().as_mut() {
            a.mixer.stop_all();
        }
    });
}
pub fn play(cue: Cue, params: VoiceParams) -> Option<VoiceHandle> {
    initialize();
    AUDIO.with(|cell| {
        let mut state = cell.borrow_mut();
        let Audio { mixer, bank, .. } = state.as_mut().unwrap();
        let bank = bank.get_or_insert_with(|| SoundBank::procedural(24000).unwrap());
        mixer.start(bank.get(cue), params)
    })
}
pub fn set_params(voice: VoiceHandle, params: VoiceParams) -> bool {
    AUDIO.with(|a| {
        a.borrow_mut()
            .as_mut()
            .is_some_and(|a| a.mixer.set_params(voice, params))
    })
}
pub fn playing(voice: VoiceHandle) -> bool {
    AUDIO.with(|a| {
        a.borrow()
            .as_ref()
            .is_some_and(|a| a.mixer.params(voice).is_some())
    })
}
pub fn stop(voice: VoiceHandle, fade_frames: u32) -> bool {
    AUDIO.with(|a| {
        a.borrow_mut()
            .as_mut()
            .is_some_and(|a| a.mixer.stop(voice, fade_frames))
    })
}
pub fn stats() -> MixStats {
    AUDIO.with(|a| {
        a.borrow()
            .as_ref()
            .map_or(MixStats::default(), |a| a.mixer.stats())
    })
}
fn audible() -> bool {
    !muted() && (OFFLINE.with(Cell::get) || crate::read(|s| s.input.focused && !s.controls_open))
}
/// For a host output callback, limited to 4096 frames and a valid device rate.
/// This runs on the game/Wasm thread; devices never borrow this mixer directly.
pub fn render(out: &mut [[f32; 2]], rate: u32) -> bool {
    if out.len() > MAX_PULL_FRAMES || !(8000..=96000).contains(&rate) {
        out.fill([0.; 2]);
        return false;
    }
    let gain = if audible() { GAIN.with(Cell::get) } else { 0. };
    AUDIO.with(|a| {
        let mut a = a.borrow_mut();
        let Some(a) = a.as_mut() else {
            out.fill([0.; 2]);
            return true;
        };
        a.mixer.set_sample_rate(rate);
        a.mixer.set_master_gain(gain);
        a.mixer.render(out);
        true
    })
}
fn skip(dt: f32) {
    if !dt.is_finite() {
        return;
    }
    AUDIO.with(|a| {
        if let Some(a) = a.borrow_mut().as_mut() {
            let frames =
                f64::from(dt.clamp(0., 0.1)) * f64::from(a.mixer.sample_rate()) + a.fraction;
            a.fraction = frames.fract();
            a.mixer.skip(frames as u32);
        }
    });
}
#[cfg(not(target_arch = "wasm32"))]
struct Output {
    #[cfg(target_os = "linux")]
    device: Option<native::Output>,
    offline: bool,
    fraction: f64,
    capture: Option<capture::Capture>,
}
#[cfg(not(target_arch = "wasm32"))]
thread_local! {static OUTPUT:RefCell<Option<Output>>=const {RefCell::new(None)};}
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn boot_native(offline: bool) {
    OFFLINE.with(|v| v.set(offline));
    OUTPUT.with(|output| {
        let capture =
            std::env::var_os("FERRAVEX_AUDIO_CAPTURE").and_then(
                |path| match capture::Capture::new(std::path::Path::new(&path)) {
                    Ok(capture) => Some(capture),
                    Err(error) => {
                        eprintln!("Ferravex audio capture unavailable: {error}");
                        None
                    }
                },
            );
        #[cfg(target_os = "linux")]
        let device = if !offline && std::env::var("FERRAVEX_AUDIO").ok().as_deref() != Some("off") {
            Some(native::Output::new(
                std::env::var("FERRAVEX_AUDIO_DEVICE").unwrap_or_else(|_| "default".into()),
            ))
        } else {
            None
        };
        *output.borrow_mut() = Some(Output {
            #[cfg(target_os = "linux")]
            device,
            offline,
            fraction: 0.,
            capture,
        });
    });
}
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn shutdown_native() {
    OFFLINE.with(|v| v.set(false));
    OUTPUT.with(|o| *o.borrow_mut() = None);
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutputStats {
    pub ready: bool,
    pub error: i32,
    pub written_frames: usize,
    pub xruns: usize,
}
#[cfg(not(target_arch = "wasm32"))]
pub fn output_stats() -> OutputStats {
    OUTPUT.with(|o| {
        #[cfg(target_os = "linux")]
        if let Some(device) = o.borrow().as_ref().and_then(|o| o.device.as_ref()) {
            let status = device.status();
            return OutputStats {
                ready: status == 1,
                error: status.min(0),
                written_frames: device.written_frames(),
                xruns: device.xruns(),
            };
        }
        OutputStats {
            ready: false,
            error: 0,
            written_frames: 0,
            xruns: 0,
        }
    })
}
#[cfg(target_arch = "wasm32")]
pub fn output_stats() -> OutputStats {
    OutputStats {
        ready: BROWSER_RUNNING.with(Cell::get),
        error: 0,
        written_frames: stats().rendered_frames as usize,
        xruns: 0,
    }
}
pub(crate) fn end_frame(dt: f32) {
    #[cfg(target_arch = "wasm32")]
    if !BROWSER_RUNNING.with(Cell::get) {
        skip(dt);
    }
    #[cfg(not(target_arch = "wasm32"))]
    OUTPUT.with(|cell| {
        let mut output = cell.borrow_mut();
        let Some(output) = output.as_mut() else {
            skip(dt);
            return;
        };
        if output.offline {
            let frames = f64::from(dt.clamp(0., 0.1)) * f64::from(SAMPLE_RATE) + output.fraction;
            output.fraction = frames.fract();
            let mut left = frames as usize;
            let mut block = [[0.; 2]; 512];
            while left > 0 {
                let len = left.min(block.len());
                render(&mut block[..len], SAMPLE_RATE);
                if let Some(capture) = &mut output.capture {
                    capture.write(&block[..len]);
                }
                left -= len;
            }
            return;
        }
        #[cfg(target_os = "linux")]
        if let Some(device) = &output.device
            && device.status() == 1
        {
            device.pump(|block| {
                render(block, SAMPLE_RATE);
                if let Some(capture) = &mut output.capture {
                    capture.write(block);
                }
            });
            return;
        }
        skip(dt);
    });
}
#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::*;
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_audio_state(running: u32) {
        let previous = BROWSER_RUNNING.with(|v| v.replace(running != 0));
        if previous && running == 0 {
            suspend();
        }
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_audio_mute(mute: u32) {
        set_muted(mute != 0);
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_audio_muted() -> u32 {
        u32::from(muted())
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_audio_pcm() -> *const [f32; 2] {
        PCM.with(|v| v.borrow().as_ptr())
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_audio_render(frames: u32, rate: u32) -> u32 {
        if frames as usize > MAX_PULL_FRAMES || !(8000..=96000).contains(&rate) {
            return 0;
        }
        PCM.with(|pcm| {
            let mut pcm = pcm.borrow_mut();
            if pcm.capacity() < MAX_PULL_FRAMES {
                *pcm = Vec::with_capacity(MAX_PULL_FRAMES);
            }
            pcm.resize(frames as usize, [0.; 2]);
            render(&mut pcm, rate);
        });
        frames
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn runtime_mute_device_rate_and_reset_are_bounded() {
        crate::edit(|s| *s = crate::State::default());
        reset();
        set_muted(false);
        play(
            Cue::Fire,
            VoiceParams {
                ramp_frames: 0,
                ..Default::default()
            },
        )
        .unwrap();
        let mut out = [[0.; 2]; 128];
        assert!(render(&mut out, 44100));
        assert!(out.iter().flatten().any(|v| v.abs() > 0.001));
        set_muted(true);
        render(&mut out, 44100);
        render(&mut out, 44100);
        assert!(out.iter().flatten().all(|v| v.abs() < 1e-6));
        assert!(!render(&mut out, 1));
        assert!(out.iter().flatten().all(|v| *v == 0.));
        reset();
        set_muted(false);
        assert_eq!(stats().active, 0);
    }
}
