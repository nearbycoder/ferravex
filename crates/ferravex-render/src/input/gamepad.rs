//! Linux joystick-device binding using std nonblocking files and the OS mapping
//! ioctls. The semantic button map follows common Linux A/B/X/Y conventions;
//! devices with different conventions can be rebound by the player.
use super::PadState;
use std::{
    fs::{File, OpenOptions},
    io::{ErrorKind, Read},
    os::unix::{fs::OpenOptionsExt, io::AsRawFd},
    path::PathBuf,
    time::{Duration, Instant},
};
#[link(name = "c")]
unsafe extern "C" {
    fn ioctl(fd: std::os::raw::c_int, request: std::os::raw::c_ulong, ...) -> std::os::raw::c_int;
}
const AXIS_MAP: std::os::raw::c_ulong =
    (2 << 30) | (64 << 16) | (b'j' as std::os::raw::c_ulong) << 8 | 0x32;
const BUTTON_MAP: std::os::raw::c_ulong =
    (2 << 30) | (1024 << 16) | (b'j' as std::os::raw::c_ulong) << 8 | 0x34;
#[derive(Clone)]
struct Mapping {
    axes: [u8; 64],
    buttons: [u16; 512],
}
impl Default for Mapping {
    fn default() -> Self {
        let mut result = Self {
            axes: [0xff; 64],
            buttons: [0xffff; 512],
        };
        for (i, a) in [0, 1, 3, 4, 2, 5, 16, 17].into_iter().enumerate() {
            result.axes[i] = a;
        }
        for (i, b) in [
            0x130, 0x131, 0x133, 0x134, 0x136, 0x137, 0x13a, 0x13b, 0x13c, 0x13d, 0x13e,
        ]
        .into_iter()
        .enumerate()
        {
            result.buttons[i] = b;
        }
        result
    }
}
impl Mapping {
    fn load(file: &File) -> Self {
        let mut map = Self::default();
        let mut axes = [0_u8; 64];
        let mut buttons = [0_u16; 512];
        // SAFETY: each ioctl receives a live device fd and the complete writable
        // Linux UAPI array for that exact request; no pointer is retained.
        unsafe {
            if ioctl(file.as_raw_fd(), AXIS_MAP, axes.as_mut_ptr()) >= 0 {
                map.axes = axes;
            }
            if ioctl(file.as_raw_fd(), BUTTON_MAP, buttons.as_mut_ptr()) >= 0 {
                map.buttons = buttons;
            }
        }
        map
    }
    fn event(&self, bytes: [u8; 8], pad: &mut PadState) {
        let value = i16::from_ne_bytes([bytes[4], bytes[5]]);
        let kind = bytes[6] & 0x7f;
        let number = bytes[7] as usize;
        if kind == 1 {
            let target = match self.buttons[number] {
                0x130 => 0,
                0x131 => 1,
                0x133 => 2,
                0x134 => 3,
                0x136 => 4,
                0x137 => 5,
                0x138 => 6,
                0x139 => 7,
                0x13a => 8,
                0x13b => 9,
                0x13d => 10,
                0x13e => 11,
                0x220 => 12,
                0x221 => 13,
                0x222 => 14,
                0x223 => 15,
                0x13c => 16,
                _ => return,
            };
            pad.buttons[target] = f32::from(value != 0);
        } else if kind == 2 && number < self.axes.len() {
            let v = if value < 0 {
                f32::from(value) / 32768.
            } else {
                f32::from(value) / 32767.
            };
            match self.axes[number] {
                0 => pad.axes[0] = v,
                1 => pad.axes[1] = v,
                3 => pad.axes[2] = v,
                4 => pad.axes[3] = v,
                2 => {
                    pad.axes[4] = (v + 1.) * 0.5;
                    pad.buttons[6] = pad.axes[4];
                }
                5 => {
                    pad.axes[5] = (v + 1.) * 0.5;
                    pad.buttons[7] = pad.axes[5];
                }
                16 => {
                    pad.buttons[14] = f32::from(v < -0.5);
                    pad.buttons[15] = f32::from(v > 0.5);
                }
                17 => {
                    pad.buttons[12] = f32::from(v < -0.5);
                    pad.buttons[13] = f32::from(v > 0.5);
                }
                _ => {}
            }
        }
    }
}
struct Joystick {
    file: File,
    mapping: Mapping,
    pad: PadState,
    partial: [u8; 8],
    filled: usize,
}
impl Joystick {
    fn open(path: &PathBuf) -> std::io::Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(0x800 | 0x80000)
            .open(path)?; // O_NONBLOCK | O_CLOEXEC
        let mapping = Mapping::load(&file);
        Ok(Self {
            file,
            mapping,
            pad: PadState {
                connected: true,
                ..Default::default()
            },
            partial: [0; 8],
            filled: 0,
        })
    }
    fn poll(&mut self) -> bool {
        // Bound event work even for a very noisy/misbehaving device.
        for _ in 0..64 {
            match self.file.read(&mut self.partial[self.filled..]) {
                Ok(0) => return false,
                Ok(n) => {
                    self.filled += n;
                    if self.filled == 8 {
                        self.mapping.event(self.partial, &mut self.pad);
                        self.filled = 0;
                    }
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(_) => return false,
            }
        }
        true
    }
}
pub(super) struct Gamepads {
    devices: [Option<Joystick>; 4],
    paths: [PathBuf; 4],
    next_probe: Instant,
}
impl Default for Gamepads {
    fn default() -> Self {
        Self {
            devices: std::array::from_fn(|_| None),
            paths: std::array::from_fn(|n| PathBuf::from(format!("/dev/input/js{n}"))),
            next_probe: Instant::now(),
        }
    }
}
impl Gamepads {
    pub fn poll(&mut self) -> [PadState; 4] {
        let now = Instant::now();
        if now >= self.next_probe {
            self.next_probe = now + Duration::from_millis(500);
            for i in 0..4 {
                if self.devices[i].is_none() {
                    self.devices[i] = Joystick::open(&self.paths[i]).ok();
                }
            }
        }
        let mut result = [PadState::default(); 4];
        for (i, device) in self.devices.iter_mut().enumerate() {
            if let Some(pad) = device {
                if pad.poll() {
                    result[i] = pad.pad;
                } else {
                    *device = None;
                }
            }
        }
        result
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn event(value: i16, kind: u8, number: u8) -> [u8; 8] {
        let v = value.to_ne_bytes();
        [0, 0, 0, 0, v[0], v[1], kind, number]
    }
    #[test]
    fn linux_initial_events_axes_triggers_hats_and_button_mapping() {
        let m = Mapping::default();
        let mut pad = PadState::default();
        m.event(event(-32768, 0x82, 0), &mut pad);
        assert_eq!(pad.axes[0], -1.);
        m.event(event(32767, 2, 2), &mut pad);
        assert_eq!(pad.axes[2], 1.);
        m.event(event(-32768, 2, 4), &mut pad);
        assert_eq!(pad.buttons[6], 0.);
        m.event(event(32767, 2, 4), &mut pad);
        assert_eq!(pad.buttons[6], 1.);
        m.event(event(-32768, 2, 6), &mut pad);
        assert_eq!(pad.buttons[14], 1.);
        m.event(event(0, 2, 6), &mut pad);
        assert_eq!(pad.buttons[14], 0.);
        m.event(event(1, 0x81, 0), &mut pad);
        assert_eq!(pad.buttons[0], 1.);
        m.event(event(0, 1, 0), &mut pad);
        assert_eq!(pad.buttons[0], 0.);
        let before = pad;
        m.event(event(1, 7, 255), &mut pad);
        m.event(event(1, 2, 255), &mut pad);
        assert_eq!(pad, before);
    }
    #[test]
    fn linux_uapi_buffers_and_ioctl_sizes_match_layout() {
        assert_eq!(AXIS_MAP, 0x80406a32);
        assert_eq!(BUTTON_MAP, 0x84006a34);
        assert_eq!(std::mem::size_of::<[u16; 512]>(), 1024);
    }
}
