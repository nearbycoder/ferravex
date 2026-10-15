//! Opt-in local recordings. GPU durations come only from tagged driver queries.
use std::cell::Cell;

#[derive(Clone, Copy, Debug, Default)]
pub struct PhysicsWork {
    pub steps: u64,
    pub candidate_pairs: u64,
    pub contact_points: u64,
    pub ccd_impacts: u64,
}
thread_local! {static PHYSICS: Cell<PhysicsWork> = Cell::new(PhysicsWork::default());}
/// Accumulate actual solver work for the current thread/frame, including catch-up ticks.
pub fn record_physics(pairs: usize, contacts: usize, ccd: usize) {
    PHYSICS.with(|cell| {
        let mut v = cell.get();
        v.steps += 1;
        v.candidate_pairs += pairs as u64;
        v.contact_points += contacts as u64;
        v.ccd_impacts += ccd as u64;
        cell.set(v);
    });
}
pub fn take_physics() -> PhysicsWork {
    PHYSICS.with(|cell| cell.replace(PhysicsWork::default()))
}

#[cfg(not(target_arch = "wasm32"))]
mod recording {
    use super::*;
    use std::{
        alloc::{GlobalAlloc, Layout, System},
        path::PathBuf,
        sync::atomic::{AtomicBool, AtomicU64, Ordering},
        time::Instant,
    };
    static ENABLED: AtomicBool = AtomicBool::new(false);
    static CALLS: AtomicU64 = AtomicU64::new(0);
    static BYTES: AtomicU64 = AtomicU64::new(0);
    /// Install once in an executable. Counts successful Rust allocations/reallocations,
    /// not C/driver allocations or live heap size. Counting is enabled only during recordings.
    pub struct TrackingAllocator;
    fn allocated(ptr: *mut u8, bytes: usize) {
        if !ptr.is_null() && ENABLED.load(Ordering::Relaxed) {
            CALLS.fetch_add(1, Ordering::Relaxed);
            BYTES.fetch_add(bytes as u64, Ordering::Relaxed);
        }
    }
    // SAFETY: Every operation forwards its unchanged allocator contract to System.
    unsafe impl GlobalAlloc for TrackingAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let p = unsafe { System.alloc(layout) };
            allocated(p, layout.size());
            p
        }
        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            let p = unsafe { System.alloc_zeroed(layout) };
            allocated(p, layout.size());
            p
        }
        unsafe fn realloc(&self, p: *mut u8, layout: Layout, size: usize) -> *mut u8 {
            let p = unsafe { System.realloc(p, layout, size) };
            allocated(p, size);
            p
        }
        unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
            unsafe { System.dealloc(p, layout) };
        }
    }
    #[derive(Clone, Debug, Default)]
    pub struct FrameSample {
        pub cpu_us: f64,
        /// CPU time issuing driver work and swapping; includes possible presentation blocking.
        pub submit_us: Option<f64>,
        pub software_render_us: Option<f64>,
        pub gpu_us: Option<f64>,
        pub draw_calls: Option<u64>,
        pub vertices: Option<u64>,
        pub upload_bytes: Option<u64>,
        pub allocation_calls: u64,
        pub allocation_bytes: u64,
        pub physics: PhysicsWork,
    }
    pub struct FrameToken {
        start: Instant,
        calls: u64,
        bytes: u64,
    }
    impl FrameToken {
        pub fn begin() -> Self {
            take_physics();
            Self {
                start: Instant::now(),
                calls: CALLS.load(Ordering::Relaxed),
                bytes: BYTES.load(Ordering::Relaxed),
            }
        }
        pub fn elapsed_us(&self) -> f64 {
            self.start.elapsed().as_secs_f64() * 1e6
        }
        pub fn finish(self, mut sample: FrameSample) -> FrameSample {
            sample.allocation_calls = CALLS.load(Ordering::Relaxed).saturating_sub(self.calls);
            sample.allocation_bytes = BYTES.load(Ordering::Relaxed).saturating_sub(self.bytes);
            sample.physics = take_physics();
            sample
        }
    }
    /// A bounded recording. Reporting and screenshot encoding happen outside recorded frames.
    pub struct Session {
        output: PathBuf,
        warmup: usize,
        seen: usize,
        dropped: usize,
        samples: Vec<FrameSample>,
        gpu_source: Option<&'static str>,
        render_quality: Option<u32>,
    }
    impl Session {
        pub fn from_env() -> Option<Self> {
            let output = std::env::var_os("FERRAVEX_PERF_OUTPUT")?;
            let warmup = std::env::var("FERRAVEX_PERF_WARMUP")
                .map_or(60, |s| {
                    s.parse::<usize>().expect("invalid FERRAVEX_PERF_WARMUP")
                })
                .min(100_000);
            let session = Self {
                output: output.into(),
                warmup,
                seen: 0,
                dropped: 0,
                samples: Vec::with_capacity(8192),
                gpu_source: None,
                render_quality: None,
            };
            ENABLED.store(true, Ordering::Relaxed);
            Some(session)
        }
        pub fn record(&mut self, sample: FrameSample) {
            self.seen += 1;
            if self.seen <= self.warmup {
                return;
            }
            if self.samples.len() < self.samples.capacity() {
                self.samples.push(sample);
            } else {
                self.dropped += 1;
            }
        }
        pub fn render_quality(&mut self, quality: u32) {
            self.render_quality = Some(quality);
        }
        pub fn gpu_source(&mut self, source: &'static str) {
            self.gpu_source = Some(source);
        }
        /// Results are tagged with the 1-based presented frame, including discarded warmup.
        pub fn gpu_result(&mut self, frame: u64, microseconds: f64) {
            if !microseconds.is_finite() || microseconds < 0. || frame <= self.warmup as u64 {
                return;
            }
            let index = frame - self.warmup as u64 - 1;
            if let Some(sample) = self.samples.get_mut(index as usize) {
                sample.gpu_us = Some(microseconds);
            }
        }
        pub fn write(&self, backend: &str, width: u32, height: u32) -> std::io::Result<()> {
            let mut out = format!(
                "{{\"schema\":2,\"backend\":{},\"os\":{},\"arch\":{},\"width\":{width},\"height\":{height},\"warmup\":{},\"dropped\":{},\"gpu_source\":{},\"render_quality\":{},\"frames\":[",
                json_string(backend),
                json_string(std::env::consts::OS),
                json_string(std::env::consts::ARCH),
                self.warmup,
                self.dropped,
                self.gpu_source.map_or_else(|| "null".into(), json_string),
                optional(self.render_quality)
            );
            for (i, s) in self.samples.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&format!("{{\"cpu_us\":{},\"submit_us\":{},\"software_render_us\":{},\"draw_calls\":{},\"vertices\":{},\"upload_bytes\":{},\"allocation_calls\":{},\"allocation_bytes\":{},\"physics_steps\":{},\"physics_pairs\":{},\"contact_points\":{},\"ccd_impacts\":{},\"gpu_us\":{}}}",s.cpu_us,optional(s.submit_us),optional(s.software_render_us),optional(s.draw_calls),optional(s.vertices),optional(s.upload_bytes),s.allocation_calls,s.allocation_bytes,s.physics.steps,s.physics.candidate_pairs,s.physics.contact_points,s.physics.ccd_impacts,optional(s.gpu_us)));
            }
            out.push_str("]}\n");
            std::fs::write(&self.output, out)
        }
    }
    fn optional<T: std::fmt::Display>(v: Option<T>) -> String {
        v.map_or_else(|| "null".into(), |n| n.to_string())
    }
    pub fn json_string(value: &str) -> String {
        let mut out = String::from("\"");
        for c in value.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                c if c < ' ' => out.push_str(&format!("\\u{:04x}", c as u32)),
                c => out.push(c),
            }
        }
        out.push('"');
        out
    }
    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn counters_sum_catchup_ticks_and_reset() {
            take_physics();
            record_physics(3, 4, 1);
            record_physics(2, 5, 0);
            let v = take_physics();
            assert_eq!(
                (v.steps, v.candidate_pairs, v.contact_points, v.ccd_impacts),
                (2, 5, 9, 1)
            );
            assert_eq!(take_physics().steps, 0);
        }
        #[test]
        fn strings_are_json_safe() {
            assert_eq!(json_string("a\n\"\\\0"), "\"a\\u000a\\\"\\\\\\u0000\"");
        }
        #[test]
        fn warmup_and_capacity_are_explicit() {
            let mut s = Session {
                output: PathBuf::new(),
                warmup: 2,
                seen: 0,
                dropped: 0,
                samples: Vec::with_capacity(2),
                gpu_source: None,
                render_quality: None,
            };
            for _ in 0..5 {
                s.record(FrameSample::default());
            }
            assert_eq!((s.samples.len(), s.dropped), (2, 1));
            s.gpu_result(2, 999.); // Warmup result arriving late.
            s.gpu_result(4, 17.);
            s.gpu_result(3, 12.);
            s.gpu_result(3, f64::NAN);
            s.gpu_result(4, -1.);
            s.gpu_result(8, 4.); // Unrecorded overflow.
            assert_eq!(s.samples[0].gpu_us, Some(12.));
            assert_eq!(s.samples[1].gpu_us, Some(17.));
        }
    }
}
#[cfg(not(target_arch = "wasm32"))]
pub use recording::*;
