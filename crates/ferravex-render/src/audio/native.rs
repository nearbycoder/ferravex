//! Linux system ALSA binding. Dynamic loading keeps the audio device optional.
//! Only the worker thread touches ALSA; the game thread has a bounded PCM queue.
use std::{
    ffi::{CString, c_char, c_int, c_long, c_uint, c_ulong, c_void},
    ptr,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicI32, AtomicUsize, Ordering},
        mpsc::{RecvTimeoutError, SyncSender, TrySendError, sync_channel},
    },
    thread::{self, JoinHandle},
    time::Duration,
};
pub const BLOCK: usize = 512;
const QUEUE: usize = 3;
pub type PcmBlock = [[f32; 2]; BLOCK];
#[link(name = "dl")]
unsafe extern "C" {
    fn dlopen(name: *const c_char, flags: c_int) -> *mut c_void;
    fn dlsym(library: *mut c_void, name: *const c_char) -> *mut c_void;
    fn dlclose(library: *mut c_void) -> c_int;
}
type Open = unsafe extern "C" fn(*mut *mut c_void, *const c_char, c_int, c_int) -> c_int;
type Close = unsafe extern "C" fn(*mut c_void) -> c_int;
type Params =
    unsafe extern "C" fn(*mut c_void, c_int, c_int, c_uint, c_uint, c_int, c_uint) -> c_int;
type Write = unsafe extern "C" fn(*mut c_void, *const c_void, c_ulong) -> c_long;
type Wait = unsafe extern "C" fn(*mut c_void, c_int) -> c_int;
type Recover = unsafe extern "C" fn(*mut c_void, c_int, c_int) -> c_int;
struct Device {
    library: *mut c_void,
    pcm: *mut c_void,
    close: Close,
    drop_pcm: Close,
    write: Write,
    wait: Wait,
    recover: Recover,
}
impl Device {
    fn open(name: &str) -> Result<Self, i32> {
        let name = CString::new(name).map_err(|_| -1001)?;
        // SAFETY: static NUL-terminated library/symbol names. Function signatures
        // match the installed ALSA C ABI; failures close the opened library.
        unsafe {
            let library = dlopen(c"libasound.so.2".as_ptr(), 2);
            if library.is_null() {
                return Err(-1000);
            }
            macro_rules! symbol {
                ($name:literal,$ty:ty) => {{
                    let pointer = dlsym(library, $name.as_ptr());
                    if pointer.is_null() {
                        dlclose(library);
                        return Err(-1000);
                    }
                    std::mem::transmute::<*mut c_void, $ty>(pointer)
                }};
            }
            let open = symbol!(c"snd_pcm_open", Open);
            let close = symbol!(c"snd_pcm_close", Close);
            let drop_pcm = symbol!(c"snd_pcm_drop", Close);
            let params = symbol!(c"snd_pcm_set_params", Params);
            let write = symbol!(c"snd_pcm_writei", Write);
            let wait = symbol!(c"snd_pcm_wait", Wait);
            let recover = symbol!(c"snd_pcm_recover", Recover);
            let mut pcm = ptr::null_mut();
            let error = open(&mut pcm, name.as_ptr(), 0, 1); // playback, nonblocking
            if error < 0 {
                dlclose(library);
                return Err(error);
            }
            let format = if cfg!(target_endian = "little") {
                14
            } else {
                15
            }; // ALSA FLOAT_LE/FLOAT_BE
            let error = params(pcm, format, 3, 2, 48000, 1, 10000); // RW_INTERLEAVED, stereo, 10 ms target
            if error < 0 {
                close(pcm);
                dlclose(library);
                return Err(error);
            }
            Ok(Self {
                library,
                pcm,
                close,
                drop_pcm,
                write,
                wait,
                recover,
            })
        }
    }
    fn write(
        &mut self,
        block: &PcmBlock,
        running: &AtomicBool,
        xruns: &AtomicUsize,
    ) -> Result<usize, i32> {
        let mut offset = 0;
        let mut recoveries = 0;
        while offset < block.len() && running.load(Ordering::Acquire) {
            // SAFETY: pcm is live on this worker, the interleaved frame slice
            // remains live and ALSA receives its exact remaining frame count.
            let count = unsafe {
                (self.write)(
                    self.pcm,
                    block[offset..].as_ptr().cast(),
                    (block.len() - offset) as c_ulong,
                )
            };
            if count > 0 {
                if count as usize > block.len() - offset {
                    return Err(-1002);
                }
                offset += count as usize;
                continue;
            }
            let mut error = count as c_int;
            if count == 0 || count == -11 {
                // EAGAIN; bounded wait permits prompt shutdown
                let result = unsafe { (self.wait)(self.pcm, 20) };
                if result >= 0 {
                    continue;
                }
                if result != -32 && result != -86 {
                    return Err(result);
                }
                error = result;
            }
            if recoveries >= 3 {
                return Err(error);
            }
            recoveries += 1;
            if error == -32 || error == -86 {
                xruns.fetch_add(1, Ordering::Relaxed);
            }
            // SAFETY: ALSA recovery operates on the same live PCM handle.
            let result = unsafe { (self.recover)(self.pcm, error, 1) };
            if result < 0 {
                return Err(result);
            }
        }
        Ok(offset)
    }
}
impl Drop for Device {
    fn drop(&mut self) {
        // SAFETY: this owns both handles; no callback or other thread uses them.
        unsafe {
            (self.drop_pcm)(self.pcm);
            (self.close)(self.pcm);
            dlclose(self.library);
        }
    }
}
struct Shared {
    running: AtomicBool,
    status: AtomicI32,
    free: AtomicUsize,
    xruns: AtomicUsize,
    written: AtomicUsize,
}
pub struct Output {
    send: Option<SyncSender<PcmBlock>>,
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}
impl Output {
    pub fn new(device: String) -> Self {
        let (send, receive) = sync_channel::<PcmBlock>(QUEUE);
        let shared = Arc::new(Shared {
            running: AtomicBool::new(true),
            status: AtomicI32::new(0),
            free: AtomicUsize::new(0),
            xruns: AtomicUsize::new(0),
            written: AtomicUsize::new(0),
        });
        let worker = shared.clone();
        let thread = thread::Builder::new()
            .name("ferravex-audio".into())
            .spawn(move || {
                let mut device = match Device::open(&device) {
                    Ok(device) => device,
                    Err(error) => {
                        worker.status.store(error, Ordering::Release);
                        return;
                    }
                };
                worker.status.store(1, Ordering::Release);
                worker.free.store(QUEUE, Ordering::Release);
                while worker.running.load(Ordering::Acquire) {
                    let block = match receive.recv_timeout(Duration::from_millis(20)) {
                        Ok(block) => block,
                        Err(RecvTimeoutError::Timeout) => continue,
                        Err(RecvTimeoutError::Disconnected) => break,
                    };
                    worker.free.fetch_add(1, Ordering::Release);
                    match device.write(&block, &worker.running, &worker.xruns) {
                        Ok(frames) => {
                            worker.written.fetch_add(frames, Ordering::Relaxed);
                        }
                        Err(error) => {
                            worker.status.store(error, Ordering::Release);
                            break;
                        }
                    }
                }
            });
        let thread = match thread {
            Ok(thread) => Some(thread),
            Err(_) => {
                shared.status.store(-1003, Ordering::Release);
                None
            }
        };
        Self {
            send: Some(send),
            shared,
            thread,
        }
    }
    pub fn status(&self) -> i32 {
        self.shared.status.load(Ordering::Acquire)
    }
    pub fn xruns(&self) -> usize {
        self.shared.xruns.load(Ordering::Relaxed)
    }
    pub fn written_frames(&self) -> usize {
        self.shared.written.load(Ordering::Relaxed)
    }
    /// Never wait for a device/queue. Generate only after reserving a free slot.
    pub fn pump(&self, mut render: impl FnMut(&mut PcmBlock)) -> usize {
        if self.status() != 1 {
            return 0;
        }
        let mut count = 0;
        for _ in 0..QUEUE {
            if self
                .shared
                .free
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |free| {
                    free.checked_sub(1)
                })
                .is_err()
            {
                break;
            }
            let mut block = [[0.; 2]; BLOCK];
            render(&mut block);
            match self.send.as_ref().unwrap().try_send(block) {
                Ok(()) => count += BLOCK,
                Err(TrySendError::Full(_)) => {
                    self.shared.free.fetch_add(1, Ordering::Release);
                    break;
                }
                Err(TrySendError::Disconnected(_)) => {
                    self.shared.status.store(-1004, Ordering::Release);
                    break;
                }
            }
        }
        count
    }
}
impl Drop for Output {
    fn drop(&mut self) {
        self.shared.running.store(false, Ordering::Release);
        self.send.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn until(output: &Output, condition: impl Fn(&Output) -> bool) {
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while !condition(output) && std::time::Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        assert!(condition(output), "audio worker status {}", output.status());
    }
    #[test]
    #[ignore = "requires Linux system ALSA null and file PCM plugins"]
    fn system_null_pcm_accepts_audio_and_missing_devices_fail_without_blocking() {
        let output = Output::new("null".into());
        until(&output, |o| o.status() != 0);
        assert_eq!(output.status(), 1);
        assert!(output.pump(|block| block.fill([0.1, -0.1])) > 0);
        until(&output, |o| o.written_frames() >= BLOCK);
        drop(output);
        let path = std::env::temp_dir().join(format!("ferravex-alsa-{}.f32", std::process::id()));
        let file = Output::new(format!("file:{}", path.display()));
        until(&file, |o| o.status() != 0);
        assert_eq!(file.status(), 1);
        assert!(file.pump(|block| block.fill([0.25, -0.5])) > 0);
        until(&file, |o| o.written_frames() >= BLOCK);
        drop(file);
        let bytes = std::fs::read(&path).unwrap();
        std::fs::remove_file(path).unwrap();
        assert!(bytes.len() >= BLOCK * 8);
        assert_eq!(&bytes[..4], &0.25_f32.to_ne_bytes());
        assert_eq!(&bytes[4..8], &(-0.5_f32).to_ne_bytes());
        let missing = Output::new("ferravex-nonexistent-device".into());
        until(&missing, |o| o.status() != 0);
        assert!(missing.status() < 0);
        assert_eq!(
            missing.pump(|_| panic!("unavailable device must not mix")),
            0
        );
    }
}
