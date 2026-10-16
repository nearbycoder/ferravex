use crate::geometry::rgb;
use ferravex_fps::{Player, RELOAD_TICKS, TICK_RATE};
use ferravex_render::prelude::*;
use ferravex_render::ui::{Align, ButtonStyle, TextStyle, Ui, UiRect};

const INK: Color = Color::new(0.94, 0.93, 0.87, 1.0);
const MUTED: Color = Color::new(0.70, 0.74, 0.69, 1.0);

// Baseline-compatible labels queued in the bounded shared draw list.
fn label(ui: &mut Ui, text: &str, x: f32, y: f32, size: f32, color: Color) {
    ui.text(
        UiRect {
            min: [x, y - size * 0.875],
            size: [(screen_width() - x).max(0.), size],
        },
        text,
        TextStyle {
            size,
            color,
            shadow: true,
            ..Default::default()
        },
    )
    .unwrap();
}
fn right(ui: &mut Ui, text: &str, x: f32, y: f32, size: f32, color: Color) {
    ui.text(
        UiRect {
            min: [0., y - size * 0.875],
            size: [x.max(0.), size],
        },
        text,
        TextStyle {
            size,
            color,
            align: Align::Right,
            shadow: true,
            ..Default::default()
        },
    )
    .unwrap();
}
fn center(ui: &mut Ui, text: &str, x: f32, y: f32, size: f32, color: Color) {
    ui.text(
        UiRect {
            min: [0., y - size * 0.875],
            size: [(x * 2.).max(0.), size],
        },
        text,
        TextStyle {
            size,
            color,
            align: Align::Center,
            shadow: true,
            ..Default::default()
        },
    )
    .unwrap();
}

fn formatted(
    ui: &mut Ui,
    x: f32,
    y: f32,
    size: f32,
    color: Color,
    align: Align,
    args: std::fmt::Arguments<'_>,
) {
    let (left, width) = match align {
        Align::Left => (x, (screen_width() - x).max(0.)),
        Align::Right => (0., x.max(0.)),
        Align::Center => (0., (x * 2.).max(0.)),
    };
    ui.text_fmt(
        UiRect {
            min: [left, y - size * 0.875],
            size: [width, size],
        },
        TextStyle {
            size,
            color,
            align,
            shadow: true,
            ..Default::default()
        },
        args,
    )
    .unwrap();
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

pub fn overlay(ui: &mut Ui, player: &Player, state: HudState<'_>) {
    let w = screen_width();
    let h = screen_height();
    let accent = rgb(0xd7b56d);
    if !state.error.is_empty() {
        let size =
            16.0_f32.min(16. * (w - 40.) / measure_text(state.error, None, 16, 1.).width.max(1.));
        center(ui, state.error, w * 0.5, 28., size, rgb(0xe4ac83));
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
        formatted(
            ui,
            50.,
            baseline,
            30.,
            health_color,
            Align::Left,
            format_args!("{}", player.health),
        );

        // The equipped weapon is already visible. Only ammunition needs a label.
        formatted(
            ui,
            w - 45.,
            baseline,
            30.,
            INK,
            Align::Right,
            format_args!("{}", player.ammo),
        );
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
            right(ui, "RELOADING", w - 28., status_y, 13., accent);
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
            right(ui, "EMPTY", w - 28., status_y, 13., accent);
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
            center(ui, "DOWN", x, y - 7., 26., INK);
            formatted(
                ui,
                x,
                y + 17.,
                16.,
                MUTED,
                Align::Center,
                format_args!("Respawning in {seconds:.1}s"),
            );
        }
    }
    if state.diagnostics {
        draw_rectangle(16., 16., 242., 63., Color::new(0.055, 0.085, 0.075, 0.86));
        formatted(
            ui,
            28.,
            40.,
            16.,
            INK,
            Align::Left,
            format_args!(
                "{:.1} ms / {} FPS",
                state.frame_ms,
                if state.frame_ms > 0. {
                    1000. / state.frame_ms
                } else {
                    0.
                } as u32
            ),
        );
        formatted(
            ui,
            28.,
            62.,
            14.,
            MUTED,
            Align::Left,
            format_args!("{} tri / tick {}", state.triangles, state.tick),
        );
    }
}

pub fn menu(ui: &mut Ui, network: bool, has_played: bool) {
    let w = screen_width();
    let h = screen_height();
    let left = (w * 0.065).max(24.);
    let top = ((h - 244.) * 0.5).max(24.);
    // Fade into the scene instead of covering half the viewport with a panel.
    let fade_width = (w * 0.58).max(350.).min(w);
    let shade = Color::new(0.04, 0.065, 0.055, 0.78);
    ui.gradient(
        UiRect {
            min: [0.; 2],
            size: [fade_width, h],
        },
        shade,
        Color::new(shade.r, shade.g, shade.b, 0.),
    )
    .unwrap();
    label(ui, "STRIKE", left - 2., top + 52., 58., INK);
    label(
        ui,
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
    ui.button(
        1,
        UiRect {
            min: [left, top + 110.],
            size: [188., 44.],
        },
        if has_played { "Resume" } else { "Play" },
        true,
        ButtonStyle {
            fill: rgb(0xd7b56d),
            ink: rgb(0x17251f),
            focus: INK,
            size: 22.,
            ..Default::default()
        },
    )
    .unwrap();
    label(
        ui,
        "WASD  Move    Mouse  Aim / fire",
        left,
        top + 187.,
        14.,
        MUTED,
    );
    label(ui, "Space  Jump    R  Reload", left, top + 209., 14., MUTED);
    label(
        ui,
        "Tab  Players    Esc  Menu",
        left,
        top + 231.,
        14.,
        MUTED,
    );
}

pub fn roster(ui: &mut Ui, players: &[Player], local_id: u8) {
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
    label(ui, "PLAYERS", x + 20., y + 30., 16., MUTED);
    right(ui, "HP", x + width - 20., y + 30., 14., MUTED);
    for (i, p) in players.iter().enumerate() {
        let row = y + 60. + i as f32 * 30.;
        if p.id == local_id {
            label(ui, "You", x + 20., row, 19., INK);
        } else {
            formatted(
                ui,
                x + 20.,
                row,
                19.,
                INK,
                Align::Left,
                format_args!("Operator {:02}", p.id),
            );
        }
        formatted(
            ui,
            x + width - 20.,
            row,
            19.,
            INK,
            Align::Right,
            format_args!("{}", p.health),
        );
    }
}
