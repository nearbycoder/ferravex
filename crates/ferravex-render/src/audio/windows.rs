//! First-party WinMM PCM sink. Only the bounded audio worker waits for playback.
use super::{BLOCK, PcmBlock};
use std::{
    ffi::c_void,
    ptr,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    time::{Duration, Instant},
};
#[repr(C, packed(2))]
struct Format {
    tag: u16,
    channels: u16,
    rate: u32,
    bytes_per_second: u32,
    align: u16,
    bits: u16,
    extra: u16,
}
#[repr(C)]
struct Header {
    data: *mut i16,
    bytes: u32,
    recorded: u32,
    user: usize,
    flags: u32,
    loops: u32,
    next: *mut Header,
    reserved: usize,
}
#[link(name = "winmm")]
unsafe extern "system" {
    fn waveOutOpen(
        handle: *mut *mut c_void,
        device: u32,
        format: *const Format,
        callback: usize,
        instance: usize,
        flags: u32,
    ) -> u32;
    fn waveOutPrepareHeader(handle: *mut c_void, header: *mut Header, size: u32) -> u32;
    fn waveOutUnprepareHeader(handle: *mut c_void, header: *mut Header, size: u32) -> u32;
    fn waveOutWrite(handle: *mut c_void, header: *mut Header, size: u32) -> u32;
    fn waveOutReset(handle: *mut c_void) -> u32;
    fn waveOutClose(handle: *mut c_void) -> u32;
}
struct Credits {
    ready: std::sync::atomic::AtomicU32,
}
unsafe extern "system" fn completed(
    _: *mut c_void,
    message: u32,
    instance: usize,
    header: usize,
    _: usize,
) {
    if message == 0x3bd && instance != 0 && header != 0 {
        // waveOutReset/Close finish outstanding buffers before this owner drops.
        // The callback only returns slot credit; it never mixes or allocates.
        unsafe {
            let index = (*(header as *const Header)).user;
            if index < 3 {
                (*(instance as *const Credits))
                    .ready
                    .fetch_or(1 << index, Ordering::Release);
            }
        }
    }
}
pub(super) struct Device {
    handle: *mut c_void,
    samples: [Box<[i16; BLOCK * 2]>; 3],
    headers: [Box<Header>; 3],
    prepared: usize,
    credits: Box<Credits>,
}
impl Device {
    pub(super) fn open(_: &str) -> Result<Self, i32> {
        unsafe {
            let mut d = Self {
                handle: ptr::null_mut(),
                samples: std::array::from_fn(|_| Box::new([0; BLOCK * 2])),
                headers: std::array::from_fn(|i| {
                    Box::new(Header {
                        data: ptr::null_mut(),
                        bytes: (BLOCK * 4) as u32,
                        recorded: 0,
                        user: i,
                        flags: 0,
                        loops: 0,
                        next: ptr::null_mut(),
                        reserved: 0,
                    })
                }),
                prepared: 0,
                credits: Box::new(Credits {
                    ready: std::sync::atomic::AtomicU32::new(7),
                }),
            };
            let fmt = Format {
                tag: 1,
                channels: 2,
                rate: 48000,
                bytes_per_second: 192000,
                align: 4,
                bits: 16,
                extra: 0,
            };
            let instance = std::ptr::from_ref(&*d.credits) as usize;
            let result = waveOutOpen(
                &mut d.handle,
                u32::MAX,
                &fmt,
                completed as *const () as usize,
                instance,
                0x00030000,
            );
            if result != 0 {
                return Err(-(result as i32));
            }
            for i in 0..3 {
                d.headers[i].data = d.samples[i].as_mut_ptr();
                let result = waveOutPrepareHeader(
                    d.handle,
                    &mut *d.headers[i],
                    std::mem::size_of::<Header>() as u32,
                );
                if result != 0 {
                    return Err(-(result as i32));
                }
                d.prepared += 1;
            }
            Ok(d)
        }
    }
    pub(super) fn write(
        &mut self,
        block: &PcmBlock,
        running: &AtomicBool,
        xruns: &AtomicUsize,
    ) -> Result<usize, i32> {
        let deadline = Instant::now() + Duration::from_secs(1);
        let index = loop {
            if !running.load(Ordering::Acquire) {
                return Ok(0);
            }
            let ready = self.credits.ready.load(Ordering::Acquire);
            if ready != 0 {
                let i = ready.trailing_zeros();
                if self.credits.ready.fetch_and(!(1 << i), Ordering::AcqRel) & (1 << i) != 0 {
                    break i as usize;
                }
            }
            if Instant::now() >= deadline {
                xruns.fetch_add(1, Ordering::Relaxed);
                return Err(-1100);
            }
            std::thread::sleep(Duration::from_millis(1));
        };
        for (d, s) in self.samples[index].iter_mut().zip(block.iter().flatten()) {
            *d = if s.is_finite() {
                (s.clamp(-1., 1.) * 32767.).round() as i16
            } else {
                0
            };
        }
        unsafe {
            let result = waveOutWrite(
                self.handle,
                &mut *self.headers[index],
                std::mem::size_of::<Header>() as u32,
            );
            if result != 0 {
                return Err(-(result as i32));
            }
        }
        Ok(BLOCK)
    }
}
impl Drop for Device {
    fn drop(&mut self) {
        unsafe {
            if !self.handle.is_null() {
                waveOutReset(self.handle);
                for h in self.headers.iter_mut().take(self.prepared) {
                    waveOutUnprepareHeader(
                        self.handle,
                        &mut **h,
                        std::mem::size_of::<Header>() as u32,
                    );
                }
                waveOutClose(self.handle);
            }
        }
    }
}
