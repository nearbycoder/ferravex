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
    let (rect, style) = formatted_layout(screen_width(), x, y, size, color, align);
    ui.text_fmt(rect, style, args).unwrap();
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
            draw_circle(x, y, 2.5, Color::new(0.02, 0.04, 0.03, 0.85));
            draw_circle(x, y, 1., INK);
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
                        4.,
                        Color::new(0.02, 0.04, 0.03, 0.85),
                    );
                    draw_line(
                        x + dx * 8.,
                        y + dy * 8.,
                        x + dx * 13.,
                        y + dy * 13.,
                        2.,
                        accent,
                    );
                }
                center(ui, "HIT", x, y + 36., 13., INK);
            }
        }

        // Touch controls occupy the lower corners; vitals move above them.
        let baseline = vitals_baseline(h, state.touch_mode, state.round_phase.is_some());
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
            VITALS_SIZE,
            health_color,
            Align::Left,
            format_args!("{}", player.health),
        );

        // The equipped weapon is already visible. Only ammunition needs a label.
        formatted(
            ui,
            w - 45.,
            baseline,
            VITALS_SIZE,
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
            let mut key = [0; 64];
            let key = hint(state.input, "reload", &mut key);
            formatted(
                ui,
                w - 28.,
                status_y,
                13.,
                accent,
                Align::Right,
                format_args!("EMPTY - RELOAD {key}"),
            );
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
            center(ui, "ELIMINATED", x, y - 7., 24., INK);
            if let Some(hint) = death_hint(state.round_phase) {
                center(ui, hint, x, y + 17., 16., MUTED);
            } else {
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
        Mode::Practice => "5 BOTS",
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
                Mode::Network => "DOCKYARD / MULTIPLAYER",
                Mode::Replay => "DOCKYARD / REPLAY",
                Mode::Practice => "DOCKYARD / PRACTICE",
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

pub fn menu(
    ui: &mut Ui,
    mode: crate::host::Mode,
    has_played: bool,
    touch: bool,
    input: &PlayerInput,
) {
    let w = screen_width();
    let h = screen_height();
    let shade = Color::new(0.04, 0.065, 0.055, 0.78);
    ui.gradient(
        UiRect {
            min: [0.; 2],
            size: [(w * 0.58).max(350.).min(w), h],
        },
        shade,
        Color::new(shade.r, shade.g, shade.b, if w < 640. { 0.35 } else { 0. }),
    )
    .unwrap();
    menu_items(w, h, mode, has_played, touch, input, |item| match item {
        MenuItem::Text(rect, text, size, color) => {
            ui.text_fmt(
                rect,
                TextStyle {
                    size,
                    color,
                    shadow: true,
                    wrap: true,
                    ..Default::default()
                },
                text,
            )
            .unwrap();
        }
        MenuItem::Button(id, rect, text, size, primary) => {
            ui.button(
                id,
                rect,
                text,
                true,
                ButtonStyle {
                    fill: if primary {
                        rgb(0xd7b56d)
                    } else {
                        rgb(0x263b35)
                    },
                    ink: if primary { rgb(0x17251f) } else { INK },
                    focus: INK,
                    size,
                    ..Default::default()
                },
            )
            .unwrap();
        }
    });
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

/// One compact objective strip; connection/tools stay in optional browser chrome.
pub fn tactical(
    ui: &mut Ui,
    view: Option<&ferravex_fps::tactical::View>,
    lobby: Option<(u8, u8)>,
    id: u8,
    active: bool,
    input: &PlayerInput,
) {
    use ferravex_play::rounds::{Phase, Team, Weapon};
    let w = screen_width();
    let Some(view) = view else {
        if let Some((players, required)) = lobby {
            formatted(
                ui,
                w * 0.5,
                52.,
                16.,
                INK,
                Align::Center,
                format_args!("LOBBY  {players} / {required}  - WAITING FOR PLAYERS"),
            );
        }
        return;
    };
    let phase = match view.phase {
        Phase::Buy => "BUY",
        Phase::Live => "LIVE",
        Phase::Planted => "BOMB PLANTED",
        Phase::PostRound => "ROUND OVER",
        Phase::Finished => "MATCH OVER",
    };
    let width = (w - 16.).clamp(0., 650.);
    ui.panel(
        UiRect {
            min: [(w - width) * 0.5, 30.],
            size: [width, 57.],
        },
        Color::new(0.04, 0.065, 0.055, 0.88),
    )
    .unwrap();
    if w < 640. {
        formatted(
            ui,
            w * 0.5,
            52.,
            15.,
            INK,
            Align::Center,
            format_args!("A {} / {} D  {phase}", view.score[0], view.score[1]),
        );
        if let Some(player) = view.player(id) {
            let role = if player.team == Team::Attack {
                "ATTACK"
            } else {
                "DEFEND"
            };
            formatted(
                ui,
                w * 0.5,
                75.,
                13.,
                MUTED,
                Align::Center,
                format_args!(
                    "R{}  {}s  {role}  ${}",
                    view.round,
                    view.remaining_ticks.div_ceil(TICK_RATE),
                    player.money
                ),
            );
        }
    } else {
        formatted(
            ui,
            w * 0.5,
            52.,
            16.,
            INK,
            Align::Center,
            format_args!(
                "ATTACK {}  /  {} DEFEND    {phase}  {}s",
                view.score[0],
                view.score[1],
                view.remaining_ticks.div_ceil(TICK_RATE)
            ),
        );
        if let Some(player) = view.player(id) {
            let role = if player.team == Team::Attack {
                "ATTACK"
            } else {
                "DEFEND"
            };
            let weapon = match player.weapon {
                Weapon::Pistol => "PISTOL",
                Weapon::Rifle => "RIFLE",
                Weapon::Sniper => "SNIPER",
            };
            formatted(
                ui,
                w * 0.5,
                75.,
                14.,
                MUTED,
                Align::Center,
                format_args!("ROUND {}  {role}  ${}  {weapon}", view.round, player.money),
            );
        }
    }
    if active && view.phase == Phase::Buy {
        let mut keys = [[0; 64]; 4];
        let [rifle, sniper, armor, kit] = &mut keys;
        let rifle = hint(input, "buy_rifle", rifle);
        let sniper = hint(input, "buy_sniper", sniper);
        let armor = hint(input, "buy_armor", armor);
        let kit = hint(input, "buy_kit", kit);
        formatted(
            ui,
            w * 0.5,
            screen_height() - 78.,
            14.,
            MUTED,
            Align::Center,
            format_args!("RIFLE {rifle} / SNIPER {sniper} / ARMOR {armor} / KIT {kit}"),
        );
    }
    if active && let Some(title) = round_banner(view.phase, view.score) {
        let width = (w - 32.).clamp(0., 420.);
        let y = (screen_height() * 0.29).max(145.);
        ui.panel(
            UiRect {
                min: [(w - width) * 0.5, y - 30.],
                size: [width, 70.],
            },
            Color::new(0.04, 0.065, 0.055, 0.94),
        )
        .unwrap();
        center(ui, title, w * 0.5, y, 24., rgb(0xd7b56d));
        if view.phase == Phase::PostRound {
            formatted(
                ui,
                w * 0.5,
                y + 24.,
                15.,
                INK,
                Align::Center,
                format_args!(
                    "Intermission: {}s",
                    view.remaining_ticks.div_ceil(TICK_RATE)
                ),
            );
        } else {
            let mut key = [0; 64];
            let key = hint(input, "pause", &mut key);
            formatted(
                ui,
                w * 0.5,
                y + 24.,
                15.,
                INK,
                Align::Center,
                format_args!("Menu {key}"),
            );
        }
    }
    if active && matches!(view.phase, Phase::Live | Phase::Planted) {
        let mut use_key = [0; 64];
        let mut drop_key = [0; 64];
        let use_key = hint(input, "interact", &mut use_key);
        let drop_key = hint(input, "drop_bomb", &mut drop_key);
        formatted(
            ui,
            w * 0.5,
            screen_height() - 78.,
            14.,
            MUTED,
            Align::Center,
            format_args!("HOLD {use_key} INTERACT / {drop_key} DROP"),
        );
    }
    if let Some((owner, progress)) = view.interaction
        && owner == id
    {
        let width = (w - 64.).min(240.);
        let x = (w - width) * 0.5;
        let y = screen_height() * 0.6;
        ui.panel(
            UiRect {
                min: [x, y],
                size: [width, 6.],
            },
            Color::new(0.12, 0.17, 0.16, 0.95),
        )
        .unwrap();
        ui.panel(
            UiRect {
                min: [x, y],
                size: [width * f32::from(progress) / 65535., 6.],
            },
            rgb(0xd7b56d),
        )
        .unwrap();
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
