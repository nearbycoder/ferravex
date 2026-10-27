//! Post-processing showcase: glowing additive orbs bloom in the world while the
//! HUD drawn after the boundary stays crisp. Keys 1–4 select off, subtle, neon
//! and warm film; 0 selects CRT. Run natively with
//! `cargo run -p ferravex-render --example post_fx --offline --locked`, or
//! check the frame contract without a display using `-- --headless`.
//! `-- --screenshot out.png 2` saves frame 30 with the given preset index.
use ferravex_render::{
    self as render, Camera2D, Color, KeyCode, Rect,
    post::{self, PostFx, PostTier},
};

const PRESETS: [(&str, PostFx); 5] = [
    ("OFF", PostFx::off()),
    ("SUBTLE", PostFx::subtle()),
    ("NEON", PostFx::neon()),
    ("WARM FILM", PostFx::warm_film()),
    ("CRT", PostFx::crt()),
];
const KEYS: [KeyCode; 5] = [
    KeyCode::Key1,
    KeyCode::Key2,
    KeyCode::Key3,
    KeyCode::Key4,
    KeyCode::Key0,
];

/// One frame: world (camera + glow), then the HUD after `set_default_camera`.
fn draw(time: f32, preset: usize) {
    let (w, h) = (render::screen_width(), render::screen_height());
    render::clear_background(Color::new(0.04, 0.05, 0.09, 1.));
    render::set_camera(&Camera2D::from_display_rect(Rect::new(0., 0., w, h)));
    // Dim world geometry stays below the bloom threshold.
    for i in 0..12 {
        let x = w * (i as f32 + 0.5) / 12.;
        render::draw_rectangle(
            x - 18.,
            h * 0.72,
            36.,
            h * 0.28,
            Color::new(0.12, 0.16, 0.24, 1.),
        );
    }
    // Emissive accents: additive, over-bright, and naturally fed to the bloom.
    let hues = [
        render::ORANGE,
        Color::new(0.3, 0.9, 1., 1.),
        Color::new(1., 0.3, 0.7, 1.),
    ];
    for (i, color) in hues.into_iter().enumerate() {
        let angle = time * (0.6 + i as f32 * 0.25) + i as f32 * 2.1;
        let (x, y) = (
            w * 0.5 + angle.cos() * w * 0.25,
            h * 0.45 + angle.sin() * h * 0.2,
        );
        post::glow_circle(x, y, 60., color, 1.6);
        render::draw_circle(x, y, 7., render::WHITE);
    }
    post::with_glow(|| render::draw_line(w * 0.1, h * 0.7, w * 0.9, h * 0.7, 3., hues[1]));
    // HUD: the automatic boundary keeps this text and panel crisp.
    render::set_default_camera();
    render::draw_rectangle(12., 12., 300., 56., Color::new(0., 0., 0., 0.6));
    render::draw_text(PRESETS[preset].0, 24., 38., 24., render::WHITE);
    let status = post::post_fx_status();
    let tier = match status.tier {
        PostTier::Off => "DIRECT",
        PostTier::Medium => "MEDIUM",
        PostTier::High => "HIGH",
    };
    render::draw_text(
        &format!("{tier} · KEYS 1-4, 0"),
        24.,
        60.,
        16.,
        render::LIGHTGRAY,
    );
}

async fn game() {
    let mut preset = 2;
    // `--screenshot PATH [PRESET]` captures frame 30 and exits (local proof).
    let args: Vec<String> = std::env::args().collect();
    let shot = args.iter().position(|a| a == "--screenshot");
    if let Some(index) = shot
        .and_then(|at| args.get(at + 2))
        .and_then(|p| p.parse().ok())
    {
        preset = usize::min(index, PRESETS.len() - 1);
    }
    for frame in 0.. {
        for (i, key) in KEYS.into_iter().enumerate() {
            if render::is_key_pressed(key) {
                preset = i;
            }
        }
        post::set_post_fx(PRESETS[preset].1);
        draw(render::get_time() as f32, preset);
        if let Some(path) = shot.and_then(|at| args.get(at + 1))
            && frame == 30
        {
            render::get_screen_data().export_png(path);
            render::next_frame().await;
            return;
        }
        render::next_frame().await;
    }
}

/// Display-free contract check: one boundary command and the expected tier.
fn headless() {
    for (i, (name, fx)) in PRESETS.into_iter().enumerate() {
        post::set_post_fx(fx);
        let status = render::input::host_frame(1. / 60., 640, 400, || {
            draw(0.5, i);
            post::post_fx_status()
        });
        assert_eq!(status.tier == PostTier::Off, fx.is_off(), "{name}");
        assert_eq!(
            status.hud_started,
            !fx.is_off(),
            "{name} records one HUD boundary"
        );
        println!(
            "{name}: tier {:?}, {} bloom levels",
            status.tier, status.bloom_levels
        );
    }
    post::set_post_fx(PostFx::off());
}

fn main() {
    if std::env::args().any(|arg| arg == "--headless") {
        headless();
        return;
    }
    render::run(render::window_conf("Ferravex post-processing"), game());
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_preset_records_its_boundary() {
        super::headless();
    }
}
