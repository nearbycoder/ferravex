//! Shared native smoke-run support for small interactive examples.
//! Each game supplies its own autoplay inputs and uses the same simulation as a human.
use crate::prelude::*;

/// Native flags: `--autoplay`, `--smoke-frames N`, `--screenshot path.png`.
/// Browser and mobile launches default to interactive play.
#[derive(Default)]
pub struct Demo {
    pub autoplay: bool,
    pub frame: u64,
    limit: Option<u64>,
    #[cfg(not(target_arch = "wasm32"))]
    screenshot: Option<String>,
}

impl Demo {
    pub fn new() -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut demo = Self::default();
            let mut args = std::env::args().skip(1);
            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "--autoplay" => demo.autoplay = true,
                    "--smoke-frames" => {
                        demo.limit = Some(
                            args.next()
                                .and_then(|s| s.parse::<u64>().ok())
                                .filter(|n| *n > 0)
                                .expect("--smoke-frames requires a positive integer"),
                        );
                    }
                    "--screenshot" => {
                        demo.screenshot = Some(args.next().expect("--screenshot requires a path"))
                    }
                    "--help" => {
                        println!("Options: --autoplay --smoke-frames N --screenshot path.png");
                        std::process::exit(0);
                    }
                    _ => panic!("Unknown option: {arg}"),
                }
            }
            demo
        }
        #[cfg(target_arch = "wasm32")]
        Self {
            autoplay: crate::autoplay_enabled(),
            ..Self::default()
        }
    }

    /// Smoke runs use a reproducible render timestep; simulation should still use FixedClock.
    pub fn dt(&self) -> f32 {
        if self.limit.is_some() {
            1.0 / 60.0
        } else {
            get_frame_time().clamp(0.0, 0.1)
        }
    }

    pub fn time(&self) -> f32 {
        if self.limit.is_some() {
            self.frame as f32 / 60.0
        } else {
            get_time() as f32
        }
    }

    /// Call after all drawing and before next_frame. Returns true when the run is finished.
    pub fn finish_frame(&mut self) -> bool {
        self.frame += 1;
        if self.limit.is_some_and(|limit| self.frame >= limit) {
            #[cfg(not(target_arch = "wasm32"))]
            if let Some(path) = &self.screenshot {
                get_screen_data().export_png(path);
            }
            true
        } else {
            false
        }
    }
}
