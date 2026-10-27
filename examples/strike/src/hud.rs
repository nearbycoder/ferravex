use crate::{controls::hint, geometry::rgb};
use ferravex_fps::{Player, RELOAD_TICKS, TICK_RATE};
use ferravex_render::input::PlayerInput;
use ferravex_render::prelude::*;
use ferravex_render::ui::{Align, ButtonStyle, TextStyle, Ui, UiRect};

const INK: Color = Color::new(0.94, 0.93, 0.87, 1.0);
const MUTED: Color = Color::new(0.70, 0.74, 0.69, 1.0);

#[cfg(test)]
#[path = "journey200_196_tests.rs"]
mod polish_tests;

/// The browser fullscreen host pins a 44px menu button 8px from the top-right
/// corner. HUD text, counters and panels keep this corner (width, height) clear.
pub(super) const MENU_CORNER: [f32; 2] = [72., 60.];

/// Whether `rect` reaches into the reserved top-right corner of a `width` view.
#[cfg(test)]
pub(super) fn in_menu_corner(width: f32, rect: UiRect) -> bool {
    rect.min[0] + rect.size[0] > width - MENU_CORNER[0] + 0.01
        && rect.min[1] < MENU_CORNER[1] - 0.01
        && rect.size[0] > 0.
        && rect.size[1] > 0.
}

/// Characters a formatted value paints, without allocating.
pub(super) fn char_count(args: std::fmt::Arguments<'_>) -> usize {
    struct Count(usize);
    impl std::fmt::Write for Count {
        fn write_str(&mut self, s: &str) -> std::fmt::Result {
            self.0 += s.chars().count();
            Ok(())
        }
    }
    let mut count = Count(0);
    let _ = std::fmt::write(&mut count, args);
    count.0
}

/// Shrink an aligned label rectangle to its painted glyph box. Glyphs land at
/// exactly the same place; the tight box keeps inspection/layout bounds honest.
pub(super) fn tight(rect: UiRect, align: Align, chars: usize, size: f32) -> UiRect {
    let glyphs = chars as f32 * 6. * size / 8. + 1.;
    let left = match align {
        Align::Left => rect.min[0],
        Align::Right => rect.min[0] + rect.size[0] - glyphs,
        Align::Center => rect.min[0] + (rect.size[0] - glyphs) * 0.5,
    };
    UiRect {
        min: [left, rect.min[1]],
        size: [glyphs, rect.size[1]],
    }
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
    let (rect, style) = formatted_layout(screen_width(), x, y, size, color, align);
    let rect = tight(rect, align, char_count(args), size);
    let _ = ui.text_fmt(rect, style, args);
}

pub(super) fn formatted_layout(
    screen_width: f32,
    x: f32,
    y: f32,
    size: f32,
    color: Color,
    align: Align,
) -> (UiRect, TextStyle) {
    let (left, width) = match align {
        Align::Left => (x, (screen_width - x).max(0.)),
        Align::Right => (0., x.max(0.)),
        Align::Center => (0., (x * 2.).max(0.)),
    };
    (
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
    )
}

pub(super) const VITALS_SIZE: f32 = 30.;
pub(super) fn vitals_baseline(height: f32, touch: bool, round: bool) -> f32 {
    if touch {
        if round { 116. } else { 42. }
    } else {
        height - 26.
    }
}

pub struct HudState<'a> {
    pub input: &'a PlayerInput,
    pub active: bool,
    pub touch_mode: bool,
    pub diagnostics: bool,
    pub frame_ms: f32,
    pub triangles: usize,
    pub tick: u32,
    pub hit_timer: f32,
    pub shot_timer: f32,
    pub round_phase: Option<ferravex_play::rounds::Phase>,
    pub error: &'a str,
    pub marker: crate::juice::Marker,
    pub death: Option<crate::juice::Death>,
    /// Equipped weapon name in the modern face, or empty when not shown.
    pub weapon: &'static str,
}

pub fn overlay(ui: &mut Ui, player: &Player, state: HudState<'_>) {
    use crate::style::{self, BODY, SMALL, TITLE, fade};
    use ferravex_render::shapes::{self, Fill, Stroke};
    use ferravex_render::text::TextAlign;
    let w = screen_width();
    let h = screen_height();
    let amber = style::AMBER;
    let signal = style::SIGNAL;
    if !state.error.is_empty() {
        // Centered and kept clear of the fullscreen menu corner on both sides.
        let max = w - 2. * MENU_CORNER[0] - 32.;
        let text = style::fit(
            state.error,
            style::modern(BODY, Color::from_hex(0xffc39a)),
            max,
            10.,
        );
        let tw = style::width(state.error, text);
        let rect = ferravex_render::Rect::new((w - tw) * 0.5 - 12., 10., tw + 24., 26.);
        style::plate(rect, 6., style::PLATE_SOLID, 1.);
        style::line(state.error, w * 0.5, 28., text, TextAlign::Center);
    }
    if state.active {
        let x = w * 0.5;
        let y = h * 0.5;
        let c = vec2(x, y);
        if player.health > 0 {
            // Anti-aliased crosshair: a dot and four ticks with a dark keyline,
            // opening briefly with each accepted shot.
            let spread = 5. + state.shot_timer * 45.;
            let keyline = Color::new(0.01, 0.02, 0.02, 0.7);
            for (dx, dy) in [(1., 0.), (-1., 0.), (0., 1.), (0., -1.)] {
                let d = vec2(dx, dy);
                let a = c + d * spread;
                let b = c + d * (spread + 7.);
                shapes::polyline(&[a, b], Stroke::new(3.5, keyline));
                shapes::polyline(&[a, b], Stroke::new(1.5, INK));
            }
            shapes::fill_circle(c, 2.2, keyline);
            shapes::fill_circle(c, 1.2, INK);
            if state.hit_timer > 0. {
                // Accepted hits only: a crisp X, bigger and redder for heads
                // and eliminations, with a short label that names which.
                use crate::juice::Marker;
                let t = (state.hit_timer / 0.25).clamp(0., 1.);
                let (outer, width, color, text) = match state.marker {
                    Marker::Hit => (13., 2., INK, "Hit"),
                    Marker::Headshot => (17., 2.5, signal, "Headshot"),
                    Marker::Kill => (19., 3., signal, "Frag"),
                };
                let outer = outer + 4. * t * t;
                for (dx, dy) in [(1., 1.), (-1., 1.), (1., -1.), (-1., -1.)] {
                    let d = vec2(dx, dy);
                    let (a, b) = (c + d * 8., c + d * outer);
                    shapes::polyline(&[a, b], Stroke::new(width + 2.5, keyline));
                    shapes::polyline(&[a, b], Stroke::new(width, fade(color, 0.4 + 0.6 * t)));
                }
                if state.marker != Marker::Hit {
                    ferravex_render::post::with_glow(|| {
                        shapes::glow_disc(c, 26., fade(color, 0.22 * t));
                    });
                }
                style::line(
                    text,
                    x,
                    y + 38.,
                    style::modern(SMALL, fade(color, 0.5 + 0.5 * t)),
                    TextAlign::Center,
                );
            }
        }

        // Touch controls occupy the lower corners; vitals move above them.
        let baseline = vitals_baseline(h, state.touch_mode, state.round_phase.is_some());
        let low = player.health <= 25;
        let health_color = if low { rgb(0xe69278) } else { INK };
        // Health: AA cross, the classic number (kept for its punch and for the
        // browser vitals tests) and a modern bar under it.
        let cross = if low { signal } else { amber };
        shapes::fill_rounded_rect(
            ferravex_render::Rect::new(27., baseline - 11., 12., 3.),
            1.,
            cross,
        );
        shapes::fill_rounded_rect(
            ferravex_render::Rect::new(31.5, baseline - 15.5, 3., 12.),
            1.,
            cross,
        );
        formatted(
            ui,
            50.,
            baseline,
            VITALS_SIZE,
            health_color,
            Align::Left,
            format_args!("{}", player.health),
        );
        let bar = ferravex_render::Rect::new(27., baseline + 8., 118., 5.);
        if bar.y + bar.h < h - 2. {
            shapes::fill_rounded_rect(bar, 2.5, Color::new(0., 0., 0., 0.45));
            let fill = ferravex_render::Rect::new(
                bar.x,
                bar.y,
                bar.w * f32::from(player.health.min(100)) / 100.,
                bar.h,
            );
            if fill.w > 0. {
                shapes::fill_rounded_rect(
                    fill,
                    2.5,
                    Fill::horizontal(
                        fill,
                        if low { signal } else { amber },
                        if low { signal } else { style::LIME },
                    ),
                );
            }
        }

        // Ammunition: classic number, magazine pips and the weapon name.
        // Touch vitals near the top move left of the fullscreen menu corner.
        let inset = if baseline - VITALS_SIZE < MENU_CORNER[1] {
            MENU_CORNER[0]
        } else {
            0.
        };
        let w = w - inset;
        formatted(
            ui,
            w - 45.,
            baseline,
            VITALS_SIZE,
            INK,
            Align::Right,
            format_args!("{}", player.ammo),
        );
        shapes::fill_capsule(
            vec2(w - 32., baseline - 13.),
            vec2(w - 32., baseline - 4.),
            2.,
            MUTED,
        );
        let pips = ferravex_fps::MAGAZINE_SIZE as usize;
        let pip_w = 3.;
        let span = pips as f32 * (pip_w + 1.);
        let pip_y = baseline + 8.;
        if pip_y + 5. < h - 2. {
            for i in 0..pips {
                let filled = i < usize::from(player.ammo);
                let px = w - 28. - span + i as f32 * (pip_w + 1.);
                shapes::fill_rounded_rect(
                    ferravex_render::Rect::new(px, pip_y, pip_w, 5.),
                    1.,
                    if filled {
                        if player.ammo <= 5 { signal } else { INK }
                    } else {
                        Color::new(0., 0., 0., 0.4)
                    },
                );
            }
        }
        let status_y = if state.touch_mode {
            baseline + 24.
        } else {
            baseline - 36.
        };
        if player.reload_ticks > 0 {
            let progress = 1. - player.reload_ticks as f32 / RELOAD_TICKS as f32;
            style::line(
                "Reloading",
                w - 28.,
                status_y,
                style::modern(SMALL, amber),
                TextAlign::Right,
            );
            let track = ferravex_render::Rect::new(w - 110., status_y + 5., 82., 3.);
            shapes::fill_rounded_rect(track, 1.5, Color::new(0., 0., 0., 0.45));
            shapes::fill_rounded_rect(
                ferravex_render::Rect::new(
                    track.x,
                    track.y,
                    track.w * progress.clamp(0., 1.),
                    track.h,
                ),
                1.5,
                amber,
            );
        } else if player.ammo == 0 && player.health > 0 {
            let mut key = [0; 64];
            let key = hint(state.input, "reload", &mut key);
            style::line_fmt(
                w - 28.,
                status_y,
                style::modern(SMALL, signal),
                TextAlign::Right,
                format_args!("Empty · press {key} to reload"),
            );
        } else if !state.weapon.is_empty() {
            style::line(
                state.weapon,
                w - 28.,
                status_y,
                style::modern(SMALL, MUTED),
                TextAlign::Right,
            );
        }
        if player.health == 0 {
            let seconds = player.respawn_ticks as f32 / TICK_RATE as f32;
            let tall = state.death.is_some();
            let width = (w - 32.).clamp(0., if tall { 440. } else { 280. });
            let height = if tall { 132. } else { 84. };
            let card = ferravex_render::Rect::new(x - width * 0.5, y - 44., width, height);
            style::plate(card, 10., style::PLATE_SOLID, 1.);
            shapes::fill_rounded_rect(
                ferravex_render::Rect::new(card.x, card.y, card.w, 3.),
                1.5,
                signal,
            );
            let title = style::fit("ELIMINATED", style::classic(TITLE, INK), width - 32., 14.);
            style::line("ELIMINATED", x, y - 10., title, TextAlign::Center);
            // Who did it and what they said, then when you are back.
            let mut status_y = y + 16.;
            if let Some(death) = state.death {
                let by = crate::style::Buf::<48>::fmt(format_args!("by {}", death.killer));
                let by_style =
                    style::fit(by.as_str(), style::modern(BODY, signal), width - 32., 10.);
                style::line(by.as_str(), x, y + 14., by_style, TextAlign::Center);
                let line = death.gloat.unwrap_or(death.retry);
                let quoted = if death.gloat.is_some() {
                    crate::style::Buf::<96>::fmt(format_args!("“{line}”"))
                } else {
                    crate::style::Buf::<96>::fmt(format_args!("{line}"))
                };
                let q = style::fit(quoted.as_str(), style::modern(BODY, INK), width - 32., 10.);
                style::line(quoted.as_str(), x, y + 38., q, TextAlign::Center);
                status_y = y + 70.;
            }
            if let Some(hint) = death_hint(state.round_phase) {
                style::line(
                    hint,
                    x,
                    status_y,
                    style::modern(SMALL, MUTED),
                    TextAlign::Center,
                );
            } else {
                let total = ferravex_fps::RESPAWN_TICKS as f32 / TICK_RATE as f32;
                let ring = vec2(x - 52., status_y - 4.);
                shapes::stroke_circle(ring, 7., 2., Color::new(1., 1., 1., 0.12));
                shapes::stroke_arc(
                    ring,
                    7.,
                    -std::f32::consts::FRAC_PI_2,
                    std::f32::consts::TAU * (1. - seconds / total).clamp(0., 1.),
                    Stroke::new(2., amber),
                );
                style::line_fmt(
                    x - 38.,
                    status_y,
                    style::modern(SMALL, MUTED),
                    TextAlign::Left,
                    format_args!("Back in {seconds:.1}s"),
                );
            }
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

fn menu_scale(width: f32, height: f32) -> f32 {
    ((width - 32.) / 360.)
        .min((height - 24.) / 350.)
        .clamp(0.1, 1.)
}

// Every block below owns its full line height. The frozen Ui API already wraps
// words; explicit line breaks keep the four authority explanations predictable.
enum MenuItem<'a> {
    Text(UiRect, std::fmt::Arguments<'a>, f32, Color),
    Button(u64, UiRect, &'static str, f32, bool),
}

fn menu_explanation(mode: crate::host::Mode, has_played: bool) -> &'static str {
    use crate::host::Mode;
    match mode {
        Mode::Network => "Match continues in menus.\nYou remain vulnerable.",
        Mode::Replay => "Replay continues in menus.\nUse Pause replay to pause.",
        Mode::Practice if has_played => "",
        Mode::Practice => "5 bots. 2 minutes. Go.",
    }
}

// Never shrink a saved binding into unreadable text or cut its last key off.
// The complete current profile remains in the existing Controls editor.
pub(super) fn menu_binding<'a>(
    input: &PlayerInput,
    action: &str,
    label: &str,
    width: f32,
    buffer: &'a mut [u8],
) -> &'a str {
    let key = hint(input, action, buffer);
    let advance = measure_text("M", None, 14, 1.).width;
    if (label.chars().count() + 2 + key.chars().count()) as f32 * advance <= width {
        key
    } else if (label.chars().count() + 2 + 17) as f32 * advance <= width {
        "see Controls (F2)"
    } else {
        "Controls (F2)"
    }
}

fn menu_items(
    width: f32,
    height: f32,
    mode: crate::host::Mode,
    has_played: bool,
    _touch: bool,
    _input: &PlayerInput,
    mut emit: impl FnMut(MenuItem<'_>),
) {
    use crate::host::Mode;
    // The browser reserves at least 480 logical pixels for the game. Smaller
    // native windows retain a fitted fallback; normal/narrow layouts keep 14px
    // guidance and 44px actions instead of scaling the entire menu by its width.
    let scale = (width / 320.).min(height / 480.).clamp(0.1, 1.);
    let w = width / scale;
    let h = height / scale;
    let compact = w < 640. || h < 560.;
    let content_width = (w - 32.).min(if compact { 500. } else { 560. });
    let left = if compact {
        (w - content_width) * 0.5
    } else {
        (w * 0.065).min(w - content_width - 16.).max(16.)
    };
    let title = if compact { 28. } else { 48. * menu_scale(w, h) };
    let secondary = mode == Mode::Replay || (mode == Mode::Practice && has_played);
    let secondary_height = if !secondary {
        0.
    } else if compact || mode == Mode::Replay {
        48.
    } else {
        100.
    };
    let content_height = title + 100. + secondary_height;
    // Leave a separate bottom band for the existing recording/replay status.
    let top = ((h - 150. - content_height) * 0.5).max(12.);
    let rect = |x: f32, y: f32, width: f32, height: f32| UiRect {
        min: [(left + x) * scale, (top + y) * scale],
        size: [width * scale, height * scale],
    };
    let mut y = 0.;
    emit(MenuItem::Text(
        rect(0., y, content_width, title),
        format_args!("STRIKE"),
        title * scale,
        INK,
    ));
    y += title + 2.;
    emit(MenuItem::Text(
        rect(0., y, content_width, 16.),
        format_args!(
            "{}",
            match mode {
                Mode::Network => "Dockyard · Multiplayer",
                Mode::Replay => "Dockyard · Replay",
                Mode::Practice => "Dockyard · Practice",
            }
        ),
        16. * scale,
        MUTED,
    ));
    y += 18.;
    emit(MenuItem::Text(
        rect(0., y, content_width, 28.),
        format_args!("{}", menu_explanation(mode, has_played)),
        14. * scale,
        INK,
    ));
    y += 30.;
    emit(MenuItem::Button(
        1,
        rect(0., y, if compact { content_width } else { 300. }, 44.),
        match mode {
            Mode::Replay => "Return to replay",
            Mode::Network => "Return to match",
            Mode::Practice if has_played => "Resume practice",
            Mode::Practice => "Start practice",
        },
        20. * scale,
        true,
    ));
    y += 44.;
    if secondary {
        y += 4.;
        if mode == Mode::Replay {
            emit(MenuItem::Button(
                3,
                rect(0., y, if compact { content_width } else { 300. }, 44.),
                "Return to practice",
                17. * scale,
                false,
            ));
        } else if compact {
            let column = (content_width - 8.) * 0.5;
            emit(MenuItem::Button(
                2,
                rect(0., y, column, 44.),
                "Restart",
                17. * scale,
                false,
            ));
            emit(MenuItem::Button(
                3,
                rect(column + 8., y, column, 44.),
                "End practice",
                13.5 * scale,
                false,
            ));
        } else {
            emit(MenuItem::Button(
                2,
                rect(0., y, 300., 44.),
                "Restart practice",
                17. * scale,
                false,
            ));
            y += 52.;
            emit(MenuItem::Button(
                3,
                rect(0., y, 300., 44.),
                "End practice / briefing",
                16. * scale,
                false,
            ));
        }
    }
}

/// Left edge and lowest row of the menu blocks, for cards placed below it.
pub fn menu_extent(
    width: f32,
    height: f32,
    mode: crate::host::Mode,
    has_played: bool,
    touch: bool,
    input: &PlayerInput,
) -> (f32, f32) {
    let (mut left, mut bottom) = (f32::MAX, 0_f32);
    menu_items(width, height, mode, has_played, touch, input, |item| {
        let rect = match item {
            MenuItem::Text(rect, ..) | MenuItem::Button(_, rect, ..) => rect,
        };
        left = left.min(rect.min[0]);
        bottom = bottom.max(rect.min[1] + rect.size[1]);
    });
    (left, bottom)
}

pub fn menu(
    ui: &mut Ui,
    mode: crate::host::Mode,
    has_played: bool,
    touch: bool,
    input: &PlayerInput,
) {
    let w = screen_width();
    let h = screen_height();
    let shade = Color::new(0.03, 0.05, 0.055, 0.8);
    ui.gradient(
        UiRect {
            min: [0.; 2],
            size: [(w * 0.58).max(350.).min(w), h],
        },
        shade,
        Color::new(shade.r, shade.g, shade.b, if w < 640. { 0.4 } else { 0. }),
    )
    .unwrap();
    // `Ui` keeps input, focus and the flat button fills (the primary fill is
    // the browser tests' button contract); `menu_overlay` paints the copy and
    // the modern button faces on top after `Ui::paint`.
    menu_items(w, h, mode, has_played, touch, input, |item| {
        if let MenuItem::Button(id, rect, text, size, primary) = item {
            let fill = if primary {
                rgb(0xd7b56d)
            } else {
                rgb(0x263b35)
            };
            ui.button(
                id,
                rect,
                text,
                true,
                ButtonStyle {
                    fill,
                    ink: fill,
                    focus: Color::new(0., 0., 0., 0.),
                    size,
                    ..Default::default()
                },
            )
            .unwrap();
        }
    });
}

/// Eased menu state: time since the menu (or results) appeared and one
/// hover/press/focus spring per button id.
pub struct MenuFx {
    pub opened: f32,
    visible: bool,
    buttons: [ferravex_render::ui::kit::ButtonMotion; 4],
}

impl MenuFx {
    pub fn new() -> Self {
        Self {
            opened: 0.,
            visible: false,
            buttons: Default::default(),
        }
    }
    /// Call once per frame after `Ui::finish`, with whether a menu shows.
    pub fn update(
        &mut self,
        ui: &Ui,
        visible: bool,
        dt: f32,
        motion: ferravex_play::tween::Motion,
    ) {
        use ferravex_render::ui::kit;
        if visible && !self.visible {
            self.opened = 0.;
            for (id, b) in self.buttons.iter_mut().enumerate() {
                b.snap(kit::ui_button_state(ui, id as u64, true));
            }
        }
        self.visible = visible;
        if visible {
            self.opened += dt.clamp(0., 0.1);
            for (id, b) in self.buttons.iter_mut().enumerate() {
                b.update(kit::ui_button_state(ui, id as u64, true), dt, motion);
            }
        }
    }
    /// Paint a `Ui` button `id`'s modern face over its flat fill.
    pub fn button(&self, id: u64, rect: UiRect, label: &str, primary: bool, alpha: f32) {
        use ferravex_render::ui::kit::{self, ButtonKind};
        let Some(motion) = self.buttons.get(id as usize) else {
            return;
        };
        let r = ferravex_render::Rect::new(rect.min[0], rect.min[1], rect.size[0], rect.size[1]);
        kit::button(
            r,
            label,
            if primary {
                ButtonKind::Primary
            } else {
                ButtonKind::Secondary
            },
            motion,
            &kit_theme().faded(alpha),
        );
    }
}

/// Strike's kit palette: flat fills match the `Ui` button bases underneath.
pub fn kit_theme() -> ferravex_render::ui::kit::KitTheme {
    use crate::style;
    ferravex_render::ui::kit::KitTheme {
        surface: Color::from_hex(0x131c1d),
        raised: rgb(0x263b35),
        header: Color::from_hex(0x2f4741),
        ink: style::INK,
        muted: style::MUTED,
        accent: rgb(0xd7b56d),
        accent_ink: rgb(0x17251f),
        focus: style::INK,
        radius: 6.,
        ..Default::default()
    }
}

/// Copy, logo and modern button faces for the briefing, pause and replay
/// menus, drawn after `Ui::paint` with a staggered entrance.
pub fn menu_overlay(
    fx: &MenuFx,
    mode: crate::host::Mode,
    has_played: bool,
    touch: bool,
    input: &PlayerInput,
) {
    use crate::style::{self, fade};
    use ferravex_play::tween::{Ease, Entrance};
    use ferravex_render::shapes;
    use ferravex_render::text::TextAlign;
    let w = screen_width();
    let h = screen_height();
    let motion = style::motion();
    let entrance = Entrance::fade_up(10.).with_stagger(0.06);
    let t = fx.opened;
    let mut index = 0;
    let mut primary_rect = None;
    menu_items(w, h, mode, has_played, touch, input, |item| {
        let look = entrance.sample(t, index, motion);
        index += 1;
        let dy = look.offset[1];
        match item {
            MenuItem::Text(rect, text, size, authored_color) => {
                let text = style::Buf::<96>::fmt(text);
                let x = rect.min[0];
                let top = rect.min[1] + dy;
                if text.as_str() == "STRIKE" {
                    // Logo: classic letters stamp in one by one, an amber
                    // rule sweeps under them and a warm glow breathes.
                    let base = style::classic(size, style::INK);
                    let mut cursor = x;
                    for (i, ch) in text.as_str().char_indices() {
                        let glyph = &text.as_str()[i..i + ch.len_utf8()];
                        let pop = Entrance::pop(1.6).with_stagger(0.05).sample(t, i, motion);
                        let s = base
                            .with_size(size * pop.scale)
                            .with_color(fade(style::INK, pop.alpha));
                        let gw = style::width(glyph, base);
                        style::line(
                            glyph,
                            cursor + gw * 0.5,
                            top + size * 0.875 + (size * pop.scale - size) * 0.4,
                            s,
                            TextAlign::Center,
                        );
                        cursor += gw + size / 8.;
                    }
                    let sweep = if motion.is_reduced() {
                        1.
                    } else {
                        Ease::CubicOut.apply(((t - 0.25) / 0.5).clamp(0., 1.))
                    };
                    let logo_w = cursor - x;
                    let rule = ferravex_render::Rect::new(
                        x,
                        top + size + 2.,
                        (logo_w * sweep).max(0.),
                        3.,
                    );
                    shapes::fill_rounded_rect(rule, 1.5, style::AMBER);
                    let breathe = if motion.is_reduced() {
                        0.5
                    } else {
                        0.5 + 0.5 * (t * 1.6).sin()
                    };
                    ferravex_render::post::with_glow(|| {
                        shapes::glow_disc(
                            vec2(x + logo_w * 0.5, top + size * 0.5),
                            logo_w * 0.55,
                            fade(style::AMBER, 0.05 + 0.04 * breathe),
                        )
                    });
                } else {
                    let face = style::modern(16., style::INK);
                    let color = if text.as_str().contains('·') {
                        style::MUTED
                    } else {
                        authored_color
                    };
                    let s = style::fit(
                        text.as_str(),
                        face.with_color(fade(color, look.alpha)),
                        rect.size[0],
                        10.,
                    );
                    if !text.as_str().is_empty() {
                        let lines = text.as_str().lines().count().max(1) as f32;
                        let line_h = rect.size[1] / lines;
                        for (n, part) in text.as_str().lines().enumerate() {
                            style::line(
                                part,
                                x,
                                top + line_h * n as f32 + line_h * 0.8,
                                s,
                                TextAlign::Left,
                            );
                        }
                    }
                }
            }
            MenuItem::Button(id, rect, label, _, primary) => {
                let r = UiRect {
                    min: [rect.min[0], rect.min[1] + dy],
                    size: rect.size,
                };
                fx.button(id, r, label, primary, look.alpha);
                if primary {
                    primary_rect = Some(rect);
                }
            }
        }
    });
    // Attract prompt beside the primary action on roomy keyboard layouts.
    if let Some(rect) = primary_rect
        && !touch
        && mode == crate::host::Mode::Practice
        && !has_played
    {
        let x = rect.min[0] + rect.size[0] + 18.;
        let room = w - x - 16.;
        if room > 150. {
            let pulse = if motion.is_reduced() {
                1.
            } else {
                0.55 + 0.45 * (t * 2.6).sin().abs()
            };
            let theme = kit_theme().faded(pulse * entrance.sample(t, index, motion).alpha);
            ferravex_render::ui::kit::key_prompt(
                x,
                rect.min[1] + 10.,
                &["Enter"],
                "to start",
                &theme,
            );
        }
    }
}

/// Hold-to-view scoreboard: call signs (practice) or operators, frags,
/// deaths and health, sorted by frags. Counts start when this view began.
pub fn roster(
    _ui: &mut Ui,
    players: &[Player],
    local_id: u8,
    board: &crate::scoreboard::Tracker,
    bots: bool,
) {
    use crate::style::{self, BODY, SMALL, TITLE};
    use ferravex_render::Rect;
    use ferravex_render::shapes;
    use ferravex_render::text::TextAlign;
    let w = screen_width();
    let h = screen_height();
    let width = 460_f32.min(w - 32.);
    let x = (w - width) / 2.;
    let rows = players.len().min(ferravex_fps::MAX_PLAYERS);
    let row_h = 32.;
    let height = 70. + rows as f32 * row_h;
    let y = ((h - height) / 2.).max(MENU_CORNER[1] + 8.);
    let card = Rect::new(x, y, width, height);
    style::plate(card, 12., style::PLATE_SOLID, 1.);
    shapes::fill_rounded_rect(Rect::new(x, y, width, 3.), 1.5, style::AMBER);
    let mut order = [0usize; ferravex_fps::MAX_PLAYERS];
    for (i, slot) in order.iter_mut().enumerate().take(rows) {
        *slot = i;
    }
    let line = |id: u8| {
        board
            .lines
            .get(usize::from(id))
            .copied()
            .unwrap_or_default()
    };
    order[..rows].sort_by_key(|&i| {
        let p = &players[i];
        (std::cmp::Reverse(line(p.id).kills), line(p.id).deaths, p.id)
    });
    let narrow = width < 360.;
    let (k, d, hp) = if narrow {
        (x + width - 104., x + width - 62., x + width - 16.)
    } else {
        (x + width - 150., x + width - 92., x + width - 22.)
    };
    let title = if bots { "THE GAUNTLET" } else { "PLAYERS" };
    style::line(
        title,
        x + 20.,
        y + 34.,
        style::classic(if narrow { BODY } else { TITLE }, style::INK),
        TextAlign::Left,
    );
    let head = style::modern(SMALL, style::MUTED);
    style::line("K", k, y + 33., head, TextAlign::Right);
    style::line("D", d, y + 33., head, TextAlign::Right);
    style::line("HP", hp, y + 33., head, TextAlign::Right);
    for (row, &i) in order[..rows].iter().enumerate() {
        let p = &players[i];
        let ry = y + 50. + row as f32 * row_h;
        let you = p.id == local_id;
        let color = if you { style::AMBER } else { style::INK };
        if you {
            let band = Rect::new(x + 8., ry, width - 16., row_h - 4.);
            shapes::fill_rounded_rect(band, 6., Color::new(0.3, 0.22, 0.06, 0.5));
            style::accent(band, style::AMBER, 1.);
        } else if row % 2 == 1 {
            shapes::fill_rounded_rect(
                Rect::new(x + 8., ry, width - 16., row_h - 4.),
                6.,
                Color::new(1., 1., 1., 0.03),
            );
        }
        let base = ry + 20.;
        let name = crate::crew::Who {
            id: p.id,
            local: local_id,
            bots,
        };
        let size = if narrow { SMALL } else { BODY };
        style::line_fmt(
            x + 20.,
            base,
            style::modern(size, color),
            TextAlign::Left,
            format_args!("{name}"),
        );
        let l = line(p.id);
        let nums = style::modern(size, color);
        style::line_fmt(k, base, nums, TextAlign::Right, format_args!("{}", l.kills));
        style::line_fmt(
            d,
            base,
            nums.with_color(style::MUTED),
            TextAlign::Right,
            format_args!("{}", l.deaths),
        );
        style::line_fmt(
            hp,
            base,
            nums.with_color(if p.health == 0 { style::SIGNAL } else { color }),
            TextAlign::Right,
            format_args!("{}", p.health),
        );
    }
}
fn death_hint(phase: Option<ferravex_play::rounds::Phase>) -> Option<&'static str> {
    use ferravex_play::rounds::Phase;
    phase.map(|phase| {
        if phase == Phase::Finished {
            "Match complete - open Menu"
        } else {
            "Wait for the next round"
        }
    })
}

fn round_banner(phase: ferravex_play::rounds::Phase, score: [u16; 2]) -> Option<&'static str> {
    use ferravex_play::rounds::Phase;
    match phase {
        Phase::PostRound => Some("ROUND COMPLETE"),
        Phase::Finished => Some(match score[0].cmp(&score[1]) {
            std::cmp::Ordering::Greater => "ATTACK WINS",
            std::cmp::Ordering::Less => "DEFEND WINS",
            std::cmp::Ordering::Equal => "MATCH COMPLETE",
        }),
        _ => None,
    }
}

/// The objective strip: centered, or shifted left when centering would put
/// it under the fullscreen menu corner. Returns (left, width).
pub(super) fn tactical_strip(w: f32) -> (f32, f32) {
    let width = (w - 16.).clamp(0., 650.);
    if (w + width) * 0.5 <= w - MENU_CORNER[0] {
        ((w - width) * 0.5, width)
    } else {
        (8., (w - 8. - MENU_CORNER[0] - 8.).max(0.))
    }
}

/// One compact objective strip; connection/tools stay in optional browser chrome.
pub fn tactical(
    _ui: &mut Ui,
    view: Option<&ferravex_fps::tactical::View>,
    lobby: Option<(u8, u8)>,
    id: u8,
    active: bool,
    input: &PlayerInput,
) {
    use crate::style::{self, BODY, SMALL, TITLE, fade};
    use ferravex_play::rounds::{Phase, Team, Weapon};
    use ferravex_render::Rect;
    use ferravex_render::shapes;
    use ferravex_render::text::TextAlign;
    let w = screen_width();
    let h = screen_height();
    let Some(view) = view else {
        if let Some((players, required)) = lobby {
            let text = style::Buf::<64>::fmt(format_args!(
                "Lobby {players} / {required} · waiting for players"
            ));
            let s = style::fit(text.as_str(), style::modern(BODY, style::INK), w - 48., 10.);
            let tw = style::width(text.as_str(), s);
            style::plate(
                Rect::new((w - tw) * 0.5 - 14., MENU_CORNER[1] + 4., tw + 28., 28.),
                8.,
                style::PLATE,
                1.,
            );
            style::line(
                text.as_str(),
                w * 0.5,
                MENU_CORNER[1] + 16.,
                s,
                TextAlign::Center,
            );
        }
        return;
    };
    let phase = match view.phase {
        Phase::Buy => "Buy",
        Phase::Live => "Live",
        Phase::Planted => "Bomb planted",
        Phase::PostRound => "Round over",
        Phase::Finished => "Match over",
    };
    let (left, width) = tactical_strip(w);
    let cx = left + width * 0.5;
    let strip = Rect::new(left, 30., width, 57.);
    style::plate(strip, 10., style::PLATE, 1.);
    let seconds = view.remaining_ticks.div_ceil(TICK_RATE);
    let urgent =
        matches!(view.phase, Phase::Planted) || (view.phase == Phase::Live && seconds <= 10);
    // Attack amber on the left, defend teal on the right, the clock between.
    let score = style::classic(TITLE, style::INK);
    style::line_fmt(
        left + 16.,
        62.,
        score.with_color(style::AMBER),
        TextAlign::Left,
        format_args!("{}", view.score[0]),
    );
    style::line_fmt(
        left + width - 16.,
        62.,
        score.with_color(style::TEAL),
        TextAlign::Right,
        format_args!("{}", view.score[1]),
    );
    style::line_fmt(
        cx,
        60.,
        style::classic(TITLE, if urgent { style::SIGNAL } else { style::INK }),
        TextAlign::Center,
        format_args!("{}:{:02}", seconds / 60, seconds % 60),
    );
    let compact = width < 420.;
    if !compact {
        let label = style::modern(SMALL, style::MUTED);
        style::line("Attack", left + 46., 59., label, TextAlign::Left);
        style::line("Defend", left + width - 46., 59., label, TextAlign::Right);
    }
    if let Some(player) = view.player(id) {
        let role = if player.team == Team::Attack {
            "Attack"
        } else {
            "Defend"
        };
        let weapon = match player.weapon {
            Weapon::Pistol => "Pistol",
            Weapon::Rifle => "Rifle",
            Weapon::Sniper => "Sniper",
        };
        let info = style::Buf::<80>::fmt(format_args!(
            "Round {} · {phase} · {role} · ${} · {weapon}",
            view.round, player.money
        ));
        let s = style::fit(
            info.as_str(),
            style::modern(SMALL, style::MUTED),
            width - 24.,
            8.,
        );
        style::line(info.as_str(), cx, 80., s, TextAlign::Center);
    }
    let prompt_y = h - 92.;
    if active && view.phase == Phase::Buy {
        let mut keys = [[0; 64]; 4];
        let [rifle, sniper, armor, kit] = &mut keys;
        let rifle = hint(input, "buy_rifle", rifle);
        let sniper = hint(input, "buy_sniper", sniper);
        let armor = hint(input, "buy_armor", armor);
        let kit = hint(input, "buy_kit", kit);
        let text = style::Buf::<128>::fmt(format_args!(
            "Rifle {rifle} · Sniper {sniper} · Armor {armor} · Kit {kit}"
        ));
        let s = style::fit(text.as_str(), style::modern(SMALL, style::INK), w - 48., 8.);
        let tw = style::width(text.as_str(), s);
        style::plate(
            Rect::new((w - tw) * 0.5 - 12., prompt_y - 17., tw + 24., 24.),
            8.,
            style::PLATE,
            1.,
        );
        style::line(text.as_str(), w * 0.5, prompt_y, s, TextAlign::Center);
    }
    if active && let Some(title) = round_banner(view.phase, view.score) {
        let width = (w - 32.).clamp(0., 420.);
        let y = (h * 0.29).max(145.);
        let card = Rect::new((w - width) * 0.5, y - 34., width, 74.);
        style::plate(card, 12., style::PLATE_SOLID, 1.);
        shapes::fill_rounded_rect(Rect::new(card.x, card.y, card.w, 3.), 1.5, style::AMBER);
        let t = style::fit(title, style::classic(TITLE, style::AMBER), width - 32., 12.);
        style::line(title, w * 0.5, y, t, TextAlign::Center);
        let sub = style::modern(BODY, style::INK);
        if view.phase == Phase::PostRound {
            style::line_fmt(
                w * 0.5,
                y + 26.,
                sub,
                TextAlign::Center,
                format_args!(
                    "Next round in {}s",
                    view.remaining_ticks.div_ceil(TICK_RATE)
                ),
            );
        } else {
            let mut key = [0; 64];
            let key = hint(input, "pause", &mut key);
            style::line_fmt(
                w * 0.5,
                y + 26.,
                sub,
                TextAlign::Center,
                format_args!("Menu {key}"),
            );
        }
    }
    if active && matches!(view.phase, Phase::Live | Phase::Planted) {
        let mut use_key = [0; 64];
        let mut drop_key = [0; 64];
        let use_key = hint(input, "interact", &mut use_key);
        let drop_key = hint(input, "drop_bomb", &mut drop_key);
        let text =
            style::Buf::<96>::fmt(format_args!("Hold {use_key} to interact · {drop_key} drop"));
        let s = style::fit(
            text.as_str(),
            style::modern(SMALL, style::MUTED),
            w - 48.,
            8.,
        );
        style::line(text.as_str(), w * 0.5, prompt_y, s, TextAlign::Center);
    }
    if let Some((owner, progress)) = view.interaction
        && owner == id
    {
        let width = (w - 64.).min(240.);
        let x = (w - width) * 0.5;
        let y = h * 0.6;
        let track = Rect::new(x, y, width, 8.);
        shapes::fill_rounded_rect(track, 4., Color::new(0., 0., 0., 0.5));
        let fill = Rect::new(x, y, width * f32::from(progress) / 65535., 8.);
        if fill.w > 0. {
            ferravex_render::post::with_glow(|| {
                shapes::fill_rounded_rect(fill, 4., fade(style::AMBER, 0.9));
            });
        }
    }
}
pub fn rule_feedback(error: ferravex_play::rounds::RuleError) -> &'static str {
    use ferravex_play::rounds::RuleError;
    match error {
        RuleError::InsufficientFunds => "NOT ENOUGH CREDITS",
        RuleError::AlreadyOwned => "ALREADY EQUIPPED",
        RuleError::WrongPhase => "ACTION UNAVAILABLE IN THIS PHASE",
        RuleError::Dead => "WAIT FOR THE NEXT ROUND",
        RuleError::WrongTeam => "THAT ACTION IS FOR THE OTHER TEAM",
        RuleError::OutOfRange => "MOVE CLOSER TO THE OBJECTIVE",
        RuleError::BombUnavailable => "BOMB UNAVAILABLE",
        RuleError::Busy => "AN INTERACTION IS ALREADY IN PROGRESS",
        _ => "ACTION UNAVAILABLE",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn banners_follow_authoritative_phase_and_never_guess_round_winners() {
        use ferravex_play::rounds::Phase;
        assert_eq!(round_banner(Phase::Live, [2, 1]), None);
        assert_eq!(round_banner(Phase::Buy, [2, 1]), None);
        assert_eq!(round_banner(Phase::Planted, [2, 1]), None);
        assert_eq!(
            round_banner(Phase::PostRound, [2, 1]),
            Some("ROUND COMPLETE")
        );
        assert_eq!(round_banner(Phase::Finished, [3, 1]), Some("ATTACK WINS"));
        assert_eq!(round_banner(Phase::Finished, [1, 3]), Some("DEFEND WINS"));
        assert_eq!(
            round_banner(Phase::Finished, [3, 3]),
            Some("MATCH COMPLETE")
        );
    }

    #[test]
    fn tactical_death_never_promises_a_timed_respawn_or_an_extra_round() {
        use ferravex_play::rounds::Phase;
        assert_eq!(death_hint(None), None);
        assert_eq!(
            death_hint(Some(Phase::Live)),
            Some("Wait for the next round")
        );
        assert_eq!(
            death_hint(Some(Phase::Finished)),
            Some("Match complete - open Menu")
        );
    }

    #[test]
    fn briefing_and_menu_actions_fit_small_viewports() {
        for (w, h) in [(320., 480.), (640., 360.), (240., 240.), (1440., 900.)] {
            let scale = menu_scale(w, h);
            assert!(360. * scale + 32. <= w + 0.001);
            assert!(350. * scale + 24. <= h + 0.001);
            assert!(scale > 0. && scale <= 1.);
        }
    }
}
