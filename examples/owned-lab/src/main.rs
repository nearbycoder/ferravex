#[cfg(not(target_arch = "wasm32"))]
#[global_allocator]
static ALLOCATOR: ferravex_core::telemetry::TrackingAllocator =
    ferravex_core::telemetry::TrackingAllocator;
#[cfg(not(target_arch = "wasm32"))]
mod native {
    use ferravex_fps::InputFrame;
    use owned_lab::Lab;
    pub fn run() {
        use ferravex_core::telemetry::{FrameSample, FrameToken, Session};
        let mut performance = Session::from_env();
        let args: Vec<_> = std::env::args().collect();
        let frames = args
            .iter()
            .position(|v| v == "--frames")
            .and_then(|i| args.get(i + 1))
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(640)
            .min(100000);
        let temporary = std::env::temp_dir().join("ferravex-owned.png");
        let temporary = temporary.to_string_lossy();
        let output = args
            .iter()
            .position(|v| v == "--output")
            .and_then(|i| args.get(i + 1))
            .map(String::as_str)
            .unwrap_or(&temporary);
        let mut lab = Lab::new();
        let start = std::time::Instant::now();
        for _ in 0..frames {
            let token = performance.as_ref().map(|_| FrameToken::begin());
            lab.update(1. / 64., InputFrame::default(), 0, 16. / 9.);
            if let (Some(session), Some(token)) = (&mut performance, token) {
                let cpu_us = token.elapsed_us();
                let render_start = std::time::Instant::now();
                std::hint::black_box(lab.render(960, 540));
                session.record(token.finish(FrameSample {
                    cpu_us,
                    software_render_us: Some(render_start.elapsed().as_secs_f64() * 1e6),
                    ..Default::default()
                }));
            }
        }
        if let Some(session) = performance {
            session
                .write("software/courtyard", 960, 540)
                .expect("write performance report");
        }
        let simulation = start.elapsed();
        let render_start = std::time::Instant::now();
        let image = lab.render(960, 540);
        image.framebuffer.save_png(output).unwrap();
        let escaped: String = output
            .chars()
            .map(|c| match c {
                '\\' => "\\\\".to_string(),
                '"' => "\\\"".to_string(),
                c if c < ' ' => format!("\\u{:04x}", c as u32),
                c => c.to_string(),
            })
            .collect();
        println!(
            "{{\"frames\":{frames},\"simulation_ms\":{},\"render_ms\":{},\"visible_instances\":{},\"output\":\"{escaped}\"}}",
            simulation.as_secs_f64() * 1000.,
            render_start.elapsed().as_secs_f64() * 1000.,
            lab.instances.len() / 17
        );
    }
}
#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    native::run();
    Ok(())
}
#[cfg(target_arch = "wasm32")]
fn main() {}
