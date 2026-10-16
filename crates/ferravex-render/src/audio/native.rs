//! Bounded native audio worker shared by our ALSA, WinMM and AudioQueue sinks.
use std::{
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
#[cfg(target_os = "linux")]
#[path = "alsa.rs"]
mod backend;
#[cfg(target_os = "windows")]
#[path = "windows.rs"]
mod backend;
#[cfg(target_vendor = "apple")]
#[path = "apple.rs"]
mod backend;
#[cfg(not(any(target_os = "linux", target_os = "windows", target_vendor = "apple")))]
#[path = "unsupported.rs"]
mod backend;
use backend::Device;
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
#[cfg(all(test, target_os = "linux"))]
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
