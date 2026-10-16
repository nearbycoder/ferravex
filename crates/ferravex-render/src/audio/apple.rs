//! First-party AudioToolbox output. The callback only returns buffer credit;
//! Rust mixing and all queue operations run on our bounded audio worker.
use super::{BLOCK, PcmBlock};
use std::{
    ffi::c_void,
    ptr,
    sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, AtomicUsize, Ordering},
    time::{Duration, Instant},
};
#[repr(C)]
struct Description {
    rate: f64,
    format: u32,
    flags: u32,
    bytes_per_packet: u32,
    frames_per_packet: u32,
    bytes_per_frame: u32,
    channels: u32,
    bits: u32,
    reserved: u32,
}
#[repr(C)]
struct Buffer {
    capacity: u32,
    data: *mut c_void,
    size: u32,
}
type Queue = *mut c_void;
#[link(name = "AudioToolbox", kind = "framework")]
unsafe extern "C" {
    fn AudioQueueNewOutput(
        format: *const Description,
        callback: unsafe extern "C" fn(*mut c_void, Queue, *mut Buffer),
        data: *mut c_void,
        runloop: *mut c_void,
        mode: *mut c_void,
        flags: u32,
        queue: *mut Queue,
    ) -> i32;
    fn AudioQueueAllocateBuffer(queue: Queue, size: u32, buffer: *mut *mut Buffer) -> i32;
    fn AudioQueueEnqueueBuffer(
        queue: Queue,
        buffer: *mut Buffer,
        count: u32,
        descriptions: *const c_void,
    ) -> i32;
    fn AudioQueueStart(queue: Queue, time: *const c_void) -> i32;
    fn AudioQueueDispose(queue: Queue, immediate: u8) -> i32;
}
struct Shared {
    ready: AtomicU32,
    buffers: [AtomicPtr<Buffer>; 3],
}
unsafe extern "C" fn returned(data: *mut c_void, _: Queue, buffer: *mut Buffer) {
    unsafe {
        let shared = &*(data.cast::<Shared>());
        for (i, p) in shared.buffers.iter().enumerate() {
            if p.load(Ordering::Acquire) == buffer {
                shared.ready.fetch_or(1 << i, Ordering::Release);
                break;
            }
        }
    }
}
pub(super) struct Device {
    queue: Queue,
    shared: Box<Shared>,
    started: bool,
}
impl Device {
    pub(super) fn open(_: &str) -> Result<Self, i32> {
        unsafe {
            let mut d = Self {
                queue: ptr::null_mut(),
                shared: Box::new(Shared {
                    ready: AtomicU32::new(7),
                    buffers: std::array::from_fn(|_| AtomicPtr::new(ptr::null_mut())),
                }),
                started: false,
            };
            let fmt = Description {
                rate: 48000.,
                format: u32::from_be_bytes(*b"lpcm"),
                flags: 1 | 8,
                bytes_per_packet: 8,
                frames_per_packet: 1,
                bytes_per_frame: 8,
                channels: 2,
                bits: 32,
                reserved: 0,
            };
            let status = AudioQueueNewOutput(
                &fmt,
                returned,
                std::ptr::from_mut(&mut *d.shared).cast(),
                ptr::null_mut(),
                ptr::null_mut(),
                0,
                &mut d.queue,
            );
            if status != 0 {
                return Err(status.min(-1));
            }
            for p in &d.shared.buffers {
                let mut b = ptr::null_mut();
                let status = AudioQueueAllocateBuffer(d.queue, (BLOCK * 8) as u32, &mut b);
                if status != 0 || b.is_null() {
                    return Err(status.min(-1));
                }
                p.store(b, Ordering::Release);
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
            let ready = self.shared.ready.load(Ordering::Acquire);
            if ready != 0 {
                let i = ready.trailing_zeros();
                if self.shared.ready.fetch_and(!(1 << i), Ordering::AcqRel) & (1 << i) != 0 {
                    break i as usize;
                }
            }
            if Instant::now() >= deadline {
                xruns.fetch_add(1, Ordering::Relaxed);
                return Err(-1200);
            }
            std::thread::sleep(Duration::from_millis(1));
        };
        unsafe {
            let buffer = self.shared.buffers[index].load(Ordering::Acquire);
            if (*buffer).capacity < (BLOCK * 8) as u32 {
                return Err(-1201);
            }
            ptr::copy_nonoverlapping(
                block.as_ptr().cast::<u8>(),
                (*buffer).data.cast::<u8>(),
                BLOCK * 8,
            );
            (*buffer).size = (BLOCK * 8) as u32;
            let status = AudioQueueEnqueueBuffer(self.queue, buffer, 0, ptr::null());
            if status != 0 {
                return Err(status.min(-1));
            }
            if !self.started {
                let status = AudioQueueStart(self.queue, ptr::null());
                if status != 0 {
                    return Err(status.min(-1));
                }
                self.started = true;
            }
        }
        Ok(BLOCK)
    }
}
impl Drop for Device {
    fn drop(&mut self) {
        if !self.queue.is_null() {
            unsafe {
                AudioQueueDispose(self.queue, 1);
            }
        }
    }
}
