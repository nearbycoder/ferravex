//! Shared native lifecycle and frame driver. UIKit schedules the same runner
//! through CADisplayLink; desktop hosts drive it from their event loop.
use crate::*;
pub(crate) struct Runner {
    window: native::NativeWindow,
    fixed: bool,
    performance: Option<telemetry::Session>,
    last: std::time::Instant,
    suspended: bool,
    finished: bool,
    max_frame: Option<std::time::Duration>,
}
impl Runner {
    pub(crate) fn new(
        conf: Conf,
        future: impl Future<Output = ()> + 'static,
    ) -> Result<Self, String> {
        let window = native::NativeWindow::new(&conf)?;
        start(conf, future);
        Self::configure(window)
    }
    #[cfg(target_os = "ios")]
    pub(crate) fn resume(conf: Conf) -> Result<Self, String> {
        Self::configure(native::NativeWindow::new(&conf)?)
    }
    fn configure(mut window: native::NativeWindow) -> Result<Self, String> {
        if let Ok(value) = std::env::var("FERRAVEX_QUALITY") {
            quality_override(
                lighting::Quality::from_name(&value)
                    .ok_or("FERRAVEX_QUALITY must be low, medium or high")?,
            );
        }
        let fixed = std::env::args().any(|s| s == "--smoke-frames");
        let mut performance = telemetry::Session::from_env();
        if fixed || performance.is_some() {
            window.set_swap_interval(0);
        }
        if let Some(session) = &mut performance
            && window.enable_gpu_timing()
        {
            session.gpu_source("OpenGL TIME_ELAPSED");
        }
        let max_frame = match std::env::var("FERRAVEX_MAX_FPS").ok() {
            Some(v) => {
                let fps = v
                    .parse::<u32>()
                    .map_err(|_| "FERRAVEX_MAX_FPS must be 1–1000")?;
                if !(1..=1000).contains(&fps) {
                    return Err("FERRAVEX_MAX_FPS must be 1–1000".into());
                }
                Some(std::time::Duration::from_secs_f64(1. / f64::from(fps)))
            }
            None => None,
        };
        audio::boot_native(fixed);
        Ok(Self {
            window,
            fixed,
            performance,
            last: std::time::Instant::now(),
            suspended: false,
            finished: false,
            max_frame,
        })
    }
    pub(crate) fn step(&mut self) -> bool {
        if self.finished {
            return false;
        }
        let now = std::time::Instant::now();
        let mut dt = if self.fixed {
            1. / 60.
        } else {
            (now - self.last).as_secs_f32()
        };
        self.last = now;
        for e in self.window.poll() {
            use native::Event;
            match e {
                Event::Key(c, d) => key(c, d),
                Event::MouseButton(c, d) => button(c, d),
                Event::MouseMove(x, y) => {
                    let old = read(|s| s.mouse);
                    pointer(x, y, x - old.x, y - old.y);
                }
                Event::MouseDelta(dx, dy) => {
                    let old = read(|s| s.mouse);
                    pointer(old.x, old.y, dx, dy);
                }
                Event::Wheel(v) => wheel(v),
                Event::Focus(v) => focus(v),
                Event::Resize(_, _) => {}
                Event::Touch { id, phase, x, y } => touch(id, phase, x, y),
                Event::Suspend(value) => {
                    if self.suspended != value {
                        self.suspended = value;
                        focus(!value);
                        if value {
                            self.window.finish();
                            audio::suspend_native();
                        } else {
                            audio::resume_native();
                            self.last = std::time::Instant::now();
                            dt = 0.;
                        }
                    }
                }
                Event::Quit => {
                    self.finish();
                    return false;
                }
            }
        }
        if self.suspended {
            return true;
        }
        let reset = edit(|s| std::mem::take(&mut s.graphics_reset));
        if reset {
            focus(false);
            if let Err(e) = self.window.recreate() {
                eprintln!("Graphics recreation failed: {e}");
            }
            focus(true);
        }
        input::poll_gamepads();
        let (w, h) = self.window.size();
        set_display_scale(self.window.dpi_scale());
        let token = self
            .performance
            .as_ref()
            .map(|_| telemetry::FrameToken::begin());
        let active = frame(dt, w, h);
        read(|s| self.window.set_cursor(s.grab, s.visible));
        let cpu_us = token.as_ref().map_or(0., |t| t.elapsed_us());
        let submit_start = std::time::Instant::now();
        read(|s| {
            self.window
                .present_scene(&s.vertices, &s.batches, s.clear, Some(&s.retained))
        });
        if let (Some(session), Some(token)) = (&mut self.performance, token) {
            session.record(token.finish(read(|s| telemetry::FrameSample {
                cpu_us,
                submit_us: Some(submit_start.elapsed().as_secs_f64() * 1e6),
                draw_calls: Some(self.window.render_counts().0),
                vertices: Some(self.window.render_counts().1),
                upload_bytes: Some(
                    (s.vertices.len() * 32 + s.retained.instances.len() * 112) as u64
                        + self.window.last_mesh_upload_bytes()
                        + self.window.skin_upload_bytes()
                        + self.window.sprite_upload_bytes(),
                ),
                ..Default::default()
            })));
        }
        if let Some(session) = &mut self.performance {
            self.window
                .drain_gpu_results(|frame, time| session.gpu_result(frame, time));
        }
        let paths = edit(|s| std::mem::take(&mut s.screenshots));
        if !paths.is_empty() {
            let pixels = self.window.pixels();
            match ferravex_graphics::Framebuffer::new(
                self.window.framebuffer_size().0,
                self.window.framebuffer_size().1,
            ) {
                Ok(mut fb)
                    if pixels.len()
                        == self.window.framebuffer_size().0 as usize
                            * self.window.framebuffer_size().1 as usize
                            * 4 =>
                {
                    for (dst, src) in fb.pixels_mut().iter_mut().zip(pixels.as_chunks::<4>().0) {
                        *dst = *src;
                    }
                    for path in paths {
                        if let Err(error) = fb.save_png(&path) {
                            eprintln!("Screenshot {path}: {error}");
                        }
                    }
                }
                _ => eprintln!("Screenshot unavailable: invalid dimensions or GPU readback"),
            }
        }

        if !active {
            self.finish();
            return false;
        }
        if !self.fixed
            && let Some(cap) = self.max_frame
        {
            let elapsed = now.elapsed();
            if elapsed < cap {
                std::thread::sleep(cap - elapsed);
            }
        }
        true
    }
    fn finish(&mut self) {
        if self.finished {
            return;
        }
        self.finished = true;
        audio::shutdown_native();
        if let Some(mut session) = self.performance.take() {
            session.render_quality(quality() as u32);
            self.window.finish_gpu_timing();
            self.window
                .drain_gpu_results(|frame, time| session.gpu_result(frame, time));
            let (w, h) = self.window.framebuffer_size();
            if let Err(e) = session.write(&self.window.driver_info(), w, h) {
                eprintln!("Performance report: {e}");
            }
        }
    }
    #[cfg(not(target_os = "ios"))]
    pub(crate) fn suspended(&self) -> bool {
        self.suspended
    }
}
impl Drop for Runner {
    fn drop(&mut self) {
        self.finish();
    }
}
