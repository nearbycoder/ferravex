use crate::geometry::rgb;
use ferravex_fps::{Player, RELOAD_TICKS, TICK_RATE};
use ferravex_render::prelude::*;

const INK: Color = Color::new(0.94, 0.93, 0.87, 1.0);
const MUTED: Color = Color::new(0.70, 0.74, 0.69, 1.0);

// A subtle shadow keeps small text readable against both sky and dark materials.
fn label(text: &str, x: f32, y: f32, size: f32, color: Color) {
    draw_text(
        text,
        x + 1.,
        y + 1.,
        size,
        Color::new(0.02, 0.04, 0.03, 0.75),
    );
    draw_text(text, x, y, size, color);
}
fn right(text: &str, x: f32, y: f32, size: f32, color: Color) {
    let dim = measure_text(text, None, size as u16, 1.);
    label(text, x - dim.width, y, size, color);
}
fn center(text: &str, x: f32, y: f32, size: f32, color: Color) {
    let dim = measure_text(text, None, size as u16, 1.);
    label(text, x - dim.width * 0.5, y, size, color);
}

pub struct HudState<'a> {
    pub active: bool,
    pub touch_mode: bool,
    pub diagnostics: bool,
    pub frame_ms: f32,
    pub triangles: usize,
    pub tick: u32,
    pub hit_timer: f32,
    pub shot_timer: f32,
    pub error: &'a str,
}

pub fn overlay(player: &Player, state: HudState<'_>) {
    let w = screen_width();
    let h = screen_height();
    let accent = rgb(0xd7b56d);
    if !state.error.is_empty() {
        let size =
            16.0_f32.min(16. * (w - 40.) / measure_text(state.error, None, 16, 1.).width.max(1.));
        center(state.error, w * 0.5, 28., size, rgb(0xe4ac83));
    }
    if state.active {
        let x = w * 0.5;
        let y = h * 0.5;
        if player.health > 0 {
            let spread = 4. + state.shot_timer * 45.;
            for d in [-1., 1.] {
                draw_line(
                    x + d * spread,
                    y,
                    x + d * (spread + 5.),
                    y,
                    3.,
                    Color::new(0.02, 0.04, 0.03, 0.6),
                );
                draw_line(
                    x,
                    y + d * spread,
                    x,
                    y + d * (spread + 5.),
                    3.,
                    Color::new(0.02, 0.04, 0.03, 0.6),
                );
                draw_line(x + d * spread, y, x + d * (spread + 5.), y, 1., INK);
                draw_line(x, y + d * spread, x, y + d * (spread + 5.), 1., INK);
            }
            if state.hit_timer > 0. {
                for (dx, dy) in [(1., 1.), (-1., 1.), (1., -1.), (-1., -1.)] {
                    draw_line(
                        x + dx * 8.,
                        y + dy * 8.,
                        x + dx * 13.,
                        y + dy * 13.,
                        1.5,
                        accent,
                    );
                }
            }
        }

        // Touch controls occupy the lower corners; vitals move above them.
        let baseline = if state.touch_mode { 42. } else { h - 26. };
        let health_color = if player.health <= 25 {
            rgb(0xe69278)
        } else {
            INK
        };
        draw_line(27., baseline - 10., 39., baseline - 10., 2., health_color);
        draw_line(33., baseline - 16., 33., baseline - 4., 2., health_color);
        label(&player.health.to_string(), 50., baseline, 30., health_color);

        // The equipped weapon is already visible. Only ammunition needs a label.
        right(&player.ammo.to_string(), w - 45., baseline, 30., INK);
        draw_rectangle(w - 34., baseline - 14., 4., 12., MUTED);
        draw_triangle(
            vec2(w - 34., baseline - 15.),
            vec2(w - 30., baseline - 15.),
            vec2(w - 32., baseline - 19.),
            MUTED,
        );
        let status_y = if state.touch_mode {
            baseline + 22.
        } else {
            baseline - 34.
        };
        if player.reload_ticks > 0 {
            right("RELOADING", w - 28., status_y, 13., accent);
            let progress = 1. - player.reload_ticks as f32 / RELOAD_TICKS as f32;
            draw_rectangle(
                w - 108.,
                status_y + 6.,
                80.,
                2.,
                Color::new(0.1, 0.15, 0.12, 0.6),
            );
            draw_rectangle(
                w - 108.,
                status_y + 6.,
                80. * progress.clamp(0., 1.),
                2.,
                accent,
            );
        } else if player.ammo == 0 && player.health > 0 {
            right("EMPTY", w - 28., status_y, 13., accent);
        }
        if player.health == 0 {
            let seconds = player.respawn_ticks as f32 / TICK_RATE as f32;
            draw_rectangle(
                x - 128.,
                y - 37.,
                256.,
                72.,
                Color::new(0.055, 0.085, 0.075, 0.75),
            );
            center("DOWN", x, y - 7., 26., INK);
            center(
                &format!("Respawning in {seconds:.1}s"),
                x,
                y + 17.,
                16.,
                MUTED,
            );
        }
    }
    if state.diagnostics {
        draw_rectangle(16., 16., 242., 63., Color::new(0.055, 0.085, 0.075, 0.86));
        label(
            &format!(
                "{:.1} ms  /  {} FPS",
                state.frame_ms,
                if state.frame_ms > 0. {
                    1000. / state.frame_ms
                } else {
                    0.
                } as u32
            ),
            28.,
            40.,
            16.,
            INK,
        );
        label(
            &format!("{} tri  /  tick {}", state.triangles, state.tick),
            28.,
            62.,
            14.,
            MUTED,
        );
    }
}

pub fn menu(network: bool, has_played: bool) -> bool {
    let w = screen_width();
    let h = screen_height();
    let left = (w * 0.065).max(24.);
    let top = ((h - 244.) * 0.5).max(24.);
    // Fade into the scene instead of covering half the viewport with a panel.
    let fade_width = (w * 0.58).max(350.).min(w);
    let shade = Color::new(0.04, 0.065, 0.055, 0.78);
    let clear = Color::new(0.04, 0.065, 0.055, 0.0);
    draw_mesh(&Mesh {
        vertices: vec![
            Vertex::new2(vec3(0., 0., 0.), Vec2::ZERO, shade),
            Vertex::new2(vec3(fade_width, 0., 0.), Vec2::ZERO, clear),
            Vertex::new2(vec3(fade_width, h, 0.), Vec2::ZERO, clear),
            Vertex::new2(vec3(0., h, 0.), Vec2::ZERO, shade),
        ],
        indices: vec![0, 1, 2, 0, 2, 3],
        texture: None,
    });
    label("STRIKE", left - 2., top + 52., 58., INK);
    label(
        if network {
            "Dockyard / Multiplayer"
        } else {
            "Dockyard / Practice"
        },
        left,
        top + 82.,
        16.,
        MUTED,
    );
    let button = Rect::new(left, top + 110., 188., 44.);
    let hover = button.contains(Vec2::from(mouse_position()));
    draw_rectangle(
        button.x,
        button.y,
        button.w,
        button.h,
        if hover { rgb(0xe5c27b) } else { rgb(0xd7b56d) },
    );
    // No shadow on the filled button; its contrast is already stable.
    draw_text(
        if has_played { "Resume" } else { "Play" },
        left + 19.,
        button.y + 28.,
        22.,
        rgb(0x17251f),
    );
    label(
        "WASD  Move    Mouse  Aim / fire",
        left,
        top + 187.,
        14.,
        MUTED,
    );
    label("Space  Jump    R  Reload", left, top + 209., 14., MUTED);
    label("Tab  Players    Esc  Menu", left, top + 231., 14., MUTED);
    let touched = ferravex_render::input::logical_touches()
        .iter()
        .any(|t| t.phase == TouchPhase::Started && button.contains(t.position));
    (hover && is_mouse_button_pressed(MouseButton::Left))
        || is_key_pressed(KeyCode::Enter)
        || touched
}

pub fn roster(players: &[Player], local_id: u8) {
    let w = screen_width();
    let h = screen_height();
    let width = 400_f32.min(w - 32.);
    let x = (w - width) / 2.;
    let y = ((h - 54. - players.len() as f32 * 30.) / 2.).max(16.);
    draw_rectangle(
        x,
        y,
        width,
        54. + players.len() as f32 * 30.,
        Color::new(0.055, 0.085, 0.075, 0.92),
    );
    label("PLAYERS", x + 20., y + 30., 16., MUTED);
    right("HP", x + width - 20., y + 30., 14., MUTED);
    for (i, p) in players.iter().enumerate() {
        let row = y + 60. + i as f32 * 30.;
        label(
            &if p.id == local_id {
                "You".to_owned()
            } else {
                format!("Operator {:02}", p.id)
            },
            x + 20.,
            row,
            19.,
            INK,
        );
        right(&p.health.to_string(), x + width - 20., row, 19., INK);
    }
}
