//! First-party ALSA device binding; only the audio worker calls this module.
use super::PcmBlock;
use std::{
    ffi::{CString, c_char, c_int, c_long, c_uint, c_ulong, c_void},
    ptr,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};
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
pub(super) struct Device {
    library: *mut c_void,
    pcm: *mut c_void,
    close: Close,
    drop_pcm: Close,
    write: Write,
    wait: Wait,
    recover: Recover,
}
impl Device {
    pub(super) fn open(name: &str) -> Result<Self, i32> {
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
    pub(super) fn write(
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
