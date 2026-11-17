//! Header, on-table prompts, side panels and the Ready/Paused/Results cards.
//!
//! The table explains itself with pictograms instead of labels: the header is
//! the score, a trophy with the best, three chrome ball lamps, a multiplier
//! chip and a restart button. Key caps appear where they act (the launch key
//! beside the shooter lane, each flipper's key under its flipper) and retire
//! once learned. Cards are a headline, one short joke, icon rows and key caps
//! beside the pictogram of what they do.
use super::presentation::{AMBER, FLIPS_TO_LEARN, GOLD, Presentation, SAVE_SECONDS, Sub};
use super::quips;
use super::*;
use ferravex_render::icons::{self, Icon, IconStyle};
use ferravex_render::shapes::{self, Fill, Shadow, Stroke};

const CARD: Color = Color::new(0.06, 0.09, 0.09, 0.97);
pub(super) const RESULTS_HEIGHT: f32 = 248.;
/// Key cap height (including its 3px lip) in logical pixels.
const CAP_H: f32 = 24.;
/// Widest binding list drawn as key caps; longer lists point to Controls.
const CAPS_MAX: f32 = 150.;

/// Bindings formatted once per frame from the live input map.
pub(super) struct Hints<'a> {
    pub launch: &'a str,
    pub left: &'a str,
    pub right: &'a str,
    pub pause: &'a str,
    pub restart: &'a str,
    pub confirm: &'a str,
    pub touch: bool,
}

/// The finished game's comparison with stored records, captured once.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Results {
    /// The best stored score before this game, if any.
    pub previous_best: Option<u32>,
    /// Autoplay demonstrations are shown but never recorded or celebrated.
    pub demo: bool,
}

impl Results {
    pub fn new_best(&self, score: u32) -> bool {
        !self.demo && score > 0 && self.previous_best.is_none_or(|best| score > best)
    }
}

// ------------------------------------------------------------- pictograms

/// An engine pictogram with a soft shadow so it reads on the table.
pub(super) fn icon(icon: Icon, center: Vec2, size: f32, color: Color) {
    icons::draw_styled(
        icon,
        center,
        size,
        IconStyle::new(color).shadow(Color::new(0., 0., 0., 0.45 * color.a)),
    );
}

/// The crane's horseshoe magnet (no engine icon exists for it): the crane and
/// its jackpots. A red U with steel poles; `lit` while the crane is armed.
pub(super) fn magnet(c: Vec2, size: f32, alpha: f32, lit: bool) {
    let (body, poles) = if lit {
        (
            Color::from_rgba(222, 74, 52, 255),
            Color::from_rgba(232, 236, 230, 255),
        )
    } else {
        (
            Color::from_rgba(150, 70, 52, 255),
            Color::from_rgba(170, 176, 170, 255),
        )
    };
    let (body, poles) = (with_alpha(body, alpha), with_alpha(poles, alpha));
    let r = size * 0.25;
    let w = size * 0.24;
    let top = c.y - size * 0.4;
    let bend = c.y + size * 0.1;
    shapes::fill_circle(
        vec2(c.x + size * 0.04, bend + size * 0.06),
        r + w * 0.5,
        Color::new(0., 0., 0., 0.3 * alpha),
    );
    shapes::stroke_arc(
        vec2(c.x, bend),
        r,
        0.,
        std::f32::consts::PI,
        Stroke::new(w, body),
    );
    for side in [-1., 1.] {
        let leg = Rect::new(c.x + side * r - w * 0.5, top, w, bend - top + 0.5);
        shapes::fill_rounded_rect(leg, 0., body);
        shapes::fill_rounded_rect(Rect::new(leg.x, top, w, size * 0.18), 1., poles);
    }
}

/// A miniature bumper cap with its lamp: the bumper missions.
fn mini_bumper(c: Vec2, r: f32, lit: bool, alpha: f32) {
    shapes::fill_circle(c, r, with_alpha(if lit { GOLD } else { BRASS }, alpha));
    shapes::fill_circle(
        c,
        r * 0.7,
        with_alpha(Color::from_rgba(196, 92, 56, 255), alpha),
    );
    let lamp = c + vec2(0., r + 5.);
    let color = if lit {
        GOLD
    } else {
        Color::from_rgba(60, 52, 40, 255)
    };
    shapes::fill_circle(lamp, 2.6, with_alpha(color, alpha));
}

/// A chrome ball lamp: lit for each ball still in hand.
fn ball_lamp(c: Vec2, r: f32, lit: bool) {
    shapes::fill_circle(c, r, if lit { CREAM } else { PANEL });
    shapes::stroke_circle(c, r, 1., if lit { BRASS } else { MINT });
    if lit {
        shapes::fill_circle(c - vec2(r * 0.28, r * 0.3), r * 0.32, WHITE);
    }
}

// --------------------------------------------------------------- key caps

fn key_icon(key: &str) -> Option<Icon> {
    match key {
        "Left" => Some(Icon::ArrowLeft),
        "Right" => Some(Icon::ArrowRight),
        "Up" => Some(Icon::ArrowUp),
        "Down" => Some(Icon::ArrowDown),
        _ => None,
    }
}

fn keycap_width(key: &str) -> f32 {
    if key_icon(key).is_some() {
        return CAP_H - 2.;
    }
    let text = fitted_text(key, 13., f32::MAX).map_or(0., |(_, w)| w);
    (text + 14.).max(CAP_H - 2.)
}

/// Total width of an `A / B` binding list drawn as caps 4px apart.
pub(super) fn keys_width(keys: &str) -> f32 {
    keys.split(" / ")
        .map(|key| keycap_width(key) + 4.)
        .sum::<f32>()
        - 4.
}

/// Whether a binding list can be drawn as key caps; otherwise the prompt
/// shows the Controls gear (unbound actions and oversized remaps).
pub(super) fn drawable(keys: &str) -> bool {
    keys != "Unbound" && keys != "SEE CONTROLS" && keys_width(keys) <= CAPS_MAX
}

/// One brass-trimmed key cap with its top-left at `(x, y)`; returns its width.
fn keycap(x: f32, y: f32, key: &str, alpha: f32) -> f32 {
    let w = keycap_width(key);
    let face = Rect::new(x, y, w, CAP_H - 3.);
    shapes::fill_rounded_rect(
        Rect::new(x, y + 3., w, CAP_H - 3.),
        6.,
        Color::new(0.02, 0.04, 0.04, 0.9 * alpha),
    );
    shapes::fill_rounded_rect(
        face,
        6.,
        Fill::vertical(
            face,
            Color::new(0.27, 0.32, 0.3, alpha),
            Color::new(0.15, 0.2, 0.19, alpha),
        ),
    );
    shapes::stroke_rounded_rect(face, 6., 1., with_alpha(BRASS, 0.85 * alpha));
    let c = face.center();
    match key_icon(key) {
        Some(arrow) => icon(arrow, c, 13., with_alpha(CREAM, alpha)),
        None => centered_in(
            key,
            vec2(c.x, c.y + 5.5),
            13.,
            w - 6.,
            with_alpha(CREAM, alpha),
        ),
    }
    w
}

/// Draw a binding list as caps from `x` with their tops at `y`; returns the end x.
fn keys(mut x: f32, y: f32, keys: &str, alpha: f32) -> f32 {
    for (i, key) in keys.split(" / ").enumerate() {
        if i > 0 {
            x += 4.;
        }
        x += keycap(x, y, key, alpha);
    }
    x
}

/// Key caps (a hand for touch, the Controls gear when unbound or oversized)
/// followed by the pictogram of what they do, centered on `c`.
fn key_action(c: Vec2, binding: &str, touch: bool, action: Icon, alpha: f32) {
    let caps = if touch || !drawable(binding) {
        CAP_H
    } else {
        keys_width(binding)
    };
    let left = c.x - (caps + 8. + 20.) * 0.5;
    let top = c.y - CAP_H * 0.5 + 1.;
    if touch {
        icon(
            Icon::Hand,
            vec2(left + caps * 0.5, c.y),
            20.,
            with_alpha(CREAM, alpha),
        );
    } else if drawable(binding) {
        keys(left, top, binding, alpha);
    } else {
        icon(
            Icon::Gear,
            vec2(left + caps * 0.5, c.y),
            18.,
            with_alpha(MINT, alpha),
        );
    }
    icon(
        action,
        vec2(left + caps + 8. + 10., c.y),
        20.,
        with_alpha(GOLD, alpha),
    );
}

// ----------------------------------------------------------------- header

/// The best score beside a trophy, or the trophy and "NEW BEST!" once this
/// game passes it. `at` is the left end of the baseline.
fn best_line(table: &Table, view: &Presentation, best: Option<u32>, at: Vec2, room: f32) {
    let Some(best) = best else {
        return;
    };
    let beaten = table.score > best && best > 0;
    let (text, color) = if beaten {
        let beat = 0.8 + 0.2 * (view.time * 5.).sin() * view.motion_scale.min(1.);
        ("NEW BEST!".to_owned(), with_alpha(GOLD, beat))
    } else {
        (quips::grouped(best), MINT)
    };
    icon(Icon::Trophy, vec2(at.x + 8., at.y - 5.), 16., color);
    if let Some((size, _)) = fitted_text(&text, 13., room - 20.) {
        draw_text(&text, at.x + 20., at.y, size, color);
    }
}

/// `2X` in a gold chip once the yard multiplier is above one; right-aligned.
fn multiplier_chip(multiplier: u32, right: f32, baseline: f32) {
    let text = format!("{multiplier}X");
    let Some((size, w)) = fitted_text(&text, 13., f32::MAX) else {
        return;
    };
    let hot = multiplier > 1;
    let chip = Rect::new(right - w - 16., baseline - 15., w + 16., 20.);
    shapes::fill_rounded_rect(
        chip,
        10.,
        if hot {
            with_alpha(GOLD, 0.22)
        } else {
            Color::new(1., 1., 1., 0.05)
        },
    );
    shapes::stroke_rounded_rect(
        chip,
        10.,
        1.,
        with_alpha(if hot { GOLD } else { MINT }, 0.6),
    );
    draw_text(
        &text,
        chip.x + 8.,
        baseline,
        size,
        if hot { GOLD } else { MINT },
    );
}

/// Score, best, balls and multiplier; the restart button sits at the far right.
pub(super) fn header(table: &Table, view: &Presentation, best: Option<u32>, hud: HudLayout) {
    let score = quips::grouped(view.shown_score.round() as u32);
    // The balls group needs ~64px; the score shrinks before it can collide.
    if let Some((size, _)) = fitted_text(&score, 32., hud.score_room) {
        draw_text(&score, hud.left, 46., size, CREAM);
    }
    best_line(table, view, best, vec2(hud.left, hud.best_y), hud.best_room);
    for i in 0..3_u8 {
        let x = hud.balls_right - 7. - f32::from(2 - i) * 19.;
        ball_lamp(vec2(x, 30.), 7., i < table.lives);
    }
    multiplier_chip(table.multiplier, hud.mult_right, hud.mult_y);
    let reset = hud.reset;
    let hover = view.reset_hover;
    draw_rectangle(
        reset.x,
        reset.y,
        reset.w,
        reset.h,
        Color::new(0.1 + hover * 0.05, 0.15, 0.14, 1.),
    );
    draw_rectangle_lines(reset.x, reset.y, reset.w, reset.h, 1., MINT);
    icon(Icon::Restart, reset.center(), 22., mix(MINT, CREAM, hover));
}

/// The idle dot-matrix message: what the player should do next, in priority
/// order, as a short headline and a row of points, lamps or a meter.
pub(super) fn objective(table: &Table, state: SessionState) -> (&'static str, Sub) {
    if state == SessionState::Ready {
        ("INSERT HARD HAT", Sub::Pips { lit: 3, total: 3 })
    } else if table.over() {
        ("SHIFT OVER", Sub::None)
    } else if state == SessionState::Paused {
        ("PAUSED", Sub::None)
    } else if table.crane_hold > 0. {
        ("MAGNET!", Sub::None)
    } else if table.waiting {
        ("SKILL SHOT", Sub::Points(crate::SKILL_SHOT_POINTS))
    } else if table.ball_save > 0. {
        ("BALL SAVE", Sub::Meter(table.ball_save / SAVE_SECONDS))
    } else if table.crane_armed {
        (
            "CRANE ARMED",
            Sub::Value(crate::JACKPOT_POINTS * table.multiplier),
        )
    } else {
        let lit = table.lamps.iter().filter(|l| **l).count() as u8;
        ("LIGHT BUMPERS", Sub::Pips { lit, total: 3 })
    }
}

// ---------------------------------------------------------- table prompts

/// Contextual prompts drawn over the table while playing: chevrons climb the
/// shooter lane and the launch key waits beside it; after launch each flipper
/// shows its key until the player has flipped a few times.
pub(super) fn table_prompts(
    layout: &layout::Layout,
    table: &Table,
    view: &Presentation,
    hints: &Hints,
) {
    let scale = layout.scale;
    let p = |xy: [f32; 2]| layout.origin + Vec2::from_array(xy) * scale;
    let moving = view.motion_scale > 0.;
    if table.waiting {
        for k in 0..3 {
            let t = if moving {
                (view.time * 1.2 + k as f32 / 3.).fract()
            } else {
                k as f32 / 3. + 0.15
            };
            let fade = (1. - (t - 0.5).abs() * 2.).clamp(0., 1.);
            let c = p([9.25, 14.1 - t * 2.6]);
            let s = 0.22 * scale;
            shapes::polyline(
                &[c + vec2(-s, s * 0.6), c, c + vec2(s, s * 0.6)],
                Stroke::new((0.07 * scale).max(2.), with_alpha(GOLD, fade))
                    .cap(shapes::Cap::Round)
                    .join(shapes::Join::Round),
            );
        }
        let anchor = p([8.62, 14.75]);
        let caps = if hints.touch || !drawable(hints.launch) {
            CAP_H
        } else {
            keys_width(hints.launch)
        };
        let left = anchor.x - caps;
        if hints.touch {
            icon(Icon::Hand, vec2(left + caps * 0.5, anchor.y), 22., CREAM);
        } else if drawable(hints.launch) {
            keys(left, anchor.y - CAP_H * 0.5, hints.launch, 1.);
        } else {
            icon(Icon::Gear, vec2(left + caps * 0.5, anchor.y), 20., MINT);
        }
    } else if view.learned.flips < FLIPS_TO_LEARN {
        let fade = 1. - view.learned.flips as f32 / FLIPS_TO_LEARN as f32;
        let alpha = 0.35 + 0.65 * fade;
        for (binding, at) in [
            (hints.left, p([2.25, 14.5])),
            (hints.right, p([7.65, 14.5])),
        ] {
            if hints.touch {
                icon(Icon::Hand, at, 22., with_alpha(CREAM, alpha));
            } else if drawable(binding) {
                let w = keys_width(binding);
                keys(at.x - w * 0.5, at.y - CAP_H * 0.5, binding, alpha);
            } else {
                icon(Icon::Gear, at, 20., with_alpha(MINT, alpha));
            }
        }
    }
}

// ------------------------------------------------------------------ cards

/// A card centered on the table, clamped to the canvas width.
fn card(center: Vec2, height: f32, accent: Color) -> (Rect, f32) {
    let width = (screen_width() - 24.).clamp(1., 460.);
    let top = (center.y - height * 0.5).max(hud_layout(screen_width()).header + 4.);
    let rect = Rect::new(center.x - width * 0.5, top, width, height);
    shapes::drop_shadow(
        rect,
        16.,
        Shadow::soft(Color::new(0., 0., 0., 0.45))
            .offset(vec2(0., 7.))
            .blur(18.),
    );
    shapes::fill_rounded_rect(rect, 16., CARD);
    shapes::fill_rounded_rect(
        Rect::new(rect.x + 16., rect.y, rect.w - 32., 4.),
        2.,
        accent,
    );
    shapes::stroke_rounded_rect(rect, 16., 1., BRASS);
    (rect, width - 28.)
}

/// The highlighted next-action strip across the bottom of a card; returns
/// its center for the key caps and pictogram drawn on it.
fn action_bar(rect: Rect, y: f32) -> Vec2 {
    let bar = Rect::new(rect.x + 14., y - 21., rect.w - 28., 32.);
    draw_rectangle(
        bar.x,
        bar.y,
        bar.w,
        bar.h,
        Color::new(0.75, 0.62, 0.40, 0.18),
    );
    draw_rectangle_lines(bar.x, bar.y, bar.w, bar.h, 1., BRASS);
    bar.center()
}

/// Three lit bumpers arm the magnet, and the magnet pays the jackpot.
fn storyboard(c: Vec2) {
    let label = format!("+{}", quips::grouped(crate::JACKPOT_POINTS));
    let label_w = fitted_text(&label, 13., f32::MAX).map_or(0., |(_, w)| w);
    let width = 3. * 20. + 24. + 28. + 24. + 24. + label_w;
    let mut x = c.x - width * 0.5;
    for _ in 0..3 {
        mini_bumper(vec2(x + 10., c.y - 3.), 8., true, 1.);
        x += 20.;
    }
    icon(Icon::ArrowRight, vec2(x + 12., c.y), 12., MINT);
    x += 24.;
    magnet(vec2(x + 14., c.y), 26., 1., true);
    x += 28.;
    icon(Icon::ArrowRight, vec2(x + 12., c.y), 12., MINT);
    x += 24.;
    icon(Icon::Star, vec2(x + 10., c.y), 20., GOLD);
    x += 24.;
    draw_text(&label, x, c.y + 5., 13., GOLD);
}

pub(super) fn ready(center: Vec2, best: Option<u32>, hints: &Hints, seed: u32) {
    let (rect, inner) = card(center, 176., BRASS);
    let mid = rect.x + rect.w * 0.5;
    centered_in("CLOCK IN!", vec2(mid, rect.y + 38.), 21., inner, CREAM);
    centered_in(
        quips::ready(seed),
        vec2(mid, rect.y + 60.),
        13.,
        inner,
        BRASS,
    );
    storyboard(vec2(mid, rect.y + 90.));
    if let Some(best) = best {
        let text = quips::grouped(best);
        let w = fitted_text(&text, 13., inner).map_or(0., |(_, w)| w) + 20.;
        let x = mid - w * 0.5;
        icon(Icon::Trophy, vec2(x + 7., rect.y + 117.), 16., GOLD);
        draw_text(&text, x + 20., rect.y + 122., 13., GOLD);
    }
    let bar = action_bar(rect, rect.y + rect.h - 16.);
    key_action(bar, hints.launch, hints.touch, Icon::Play, 1.);
}

pub(super) fn paused(center: Vec2, input: &PlayerInput, hints: &Hints, seed: u32) {
    let (rect, inner) = card(center, 128., MINT);
    let mid = rect.x + rect.w * 0.5;
    centered_in("PAUSED", vec2(mid, rect.y + 36.), 21., inner, CREAM);
    centered_in(
        quips::pause(seed),
        vec2(mid, rect.y + 60.),
        13.,
        inner,
        BRASS,
    );
    let bar = action_bar(rect, rect.y + rect.h - 16.);
    if input.controls_open() {
        icon(Icon::Gear, bar - vec2(14., 0.), 20., MINT);
        icon(Icon::Cross, bar + vec2(14., 0.), 16., GOLD);
    } else if input.paused() {
        key_action(bar - vec2(60., 0.), hints.pause, false, Icon::Play, 1.);
        key_action(bar + vec2(60., 0.), hints.restart, false, Icon::Restart, 1.);
    } else {
        // Focus left the table: point back at it.
        icon(Icon::Pointer, bar - vec2(14., 0.), 20., CREAM);
        icon(Icon::Play, bar + vec2(14., 0.), 18., GOLD);
    }
}

/// An icon (or the magnet) and a number, centered on `c`; dim when zero.
fn stat(c: Vec2, glyph: Option<Icon>, value: u32, color: Color) {
    let text = value.to_string();
    let w = fitted_text(&text, 16., f32::MAX).map_or(0., |(_, w)| w);
    let left = c.x - (22. + w) * 0.5;
    let alpha = if value == 0 { 0.45 } else { 1. };
    let at = vec2(left + 9., c.y - 6.);
    match glyph {
        Some(glyph) => icon(glyph, at, 18., with_alpha(color, alpha)),
        None => magnet(at, 20., alpha, true),
    }
    draw_text(&text, left + 22., c.y, 16., with_alpha(CREAM, alpha));
}

pub(super) fn results(center: Vec2, table: &Table, results: Results, hints: &Hints) {
    let rank = quips::rank(table.score, table.overtime);
    let (rect, inner) = card(center, RESULTS_HEIGHT, GOLD);
    let mid = rect.x + rect.w * 0.5;
    centered_in("SHIFT COMPLETE", vec2(mid, rect.y + 26.), 13., inner, BRASS);
    centered_in(
        &quips::grouped(table.score),
        vec2(mid, rect.y + 67.),
        34.,
        inner,
        CREAM,
    );
    centered_in(rank.title, vec2(mid, rect.y + 94.), 20., inner, GOLD);
    centered_in(rank.quip, vec2(mid, rect.y + 112.), 13., inner, MINT);
    // Jackpots, best chain and skill shots as icon stats.
    let spread = (inner / 3.).min(96.);
    stat(
        vec2(mid - spread, rect.y + 143.),
        None,
        table.jackpots,
        GOLD,
    );
    stat(
        vec2(mid, rect.y + 143.),
        Some(Icon::Bolt),
        table.best_chain,
        AMBER,
    );
    stat(
        vec2(mid + spread, rect.y + 143.),
        Some(Icon::Target),
        table.skill_shots,
        GOLD,
    );
    // The record: a trophy with the news, or the best and a meter toward it.
    let y = rect.y + 170.;
    if results.demo {
        let chip = Rect::new(mid - 34., y - 15., 68., 20.);
        shapes::fill_rounded_rect(chip, 10., Color::new(1., 1., 1., 0.06));
        shapes::stroke_rounded_rect(chip, 10., 1., with_alpha(MINT, 0.6));
        centered_in("DEMO", vec2(mid, y), 13., 60., MINT);
    } else if results.new_best(table.score) {
        let text = match results.previous_best {
            Some(best) if best > 0 => format!("NEW BEST!  +{}", quips::grouped(table.score - best)),
            _ => "NEW BEST!".to_owned(),
        };
        let w = fitted_text(&text, 13., inner - 24.).map_or(0., |(_, w)| w) + 24.;
        icon(Icon::Trophy, vec2(mid - w * 0.5 + 8., y - 5.), 18., GOLD);
        centered_in(&text, vec2(mid + 12., y), 13., inner - 24., GOLD);
    } else {
        let best = results.previous_best.unwrap_or(0);
        let text = quips::grouped(best);
        let w = fitted_text(&text, 13., inner).map_or(0., |(_, w)| w);
        let left = mid - (20. + w + 10. + 96.) * 0.5;
        icon(Icon::Trophy, vec2(left + 8., y - 5.), 16., BRASS);
        draw_text(&text, left + 20., y, 13., BRASS);
        let bar = Rect::new(left + 30. + w, y - 9., 96., 6.);
        shapes::fill_rounded_rect(bar, 3., Color::new(1., 1., 1., 0.08));
        let fill = (table.score as f32 / best.max(1) as f32).clamp(0., 1.);
        shapes::fill_rounded_rect(
            Rect::new(bar.x, bar.y, (bar.w * fill).max(6.), bar.h),
            3.,
            MINT,
        );
    }
    // The next rank to chase, or the crown at the top.
    let y = rect.y + 196.;
    match rank.next {
        Some((title, floor)) => {
            let more = format!("+{}", quips::grouped(floor.saturating_sub(table.score)));
            let title_w = fitted_text(title, 13., f32::MAX).map_or(0., |(_, w)| w);
            let more_w = fitted_text(&more, 13., f32::MAX).map_or(0., |(_, w)| w);
            let width = 20. + title_w + 12. + more_w;
            let scale = (inner / width).min(1.);
            let left = mid - width * scale * 0.5;
            icon(Icon::Flag, vec2(left + 7., y - 5.), 15., MINT);
            draw_text(title, left + 20., y, 13. * scale, MINT);
            draw_text(
                &more,
                left + (20. + title_w + 12.) * scale,
                y,
                13. * scale,
                CREAM,
            );
        }
        None => {
            let w = fitted_text("TOP RANK", 13., f32::MAX).map_or(0., |(_, w)| w) + 22.;
            icon(Icon::Crown, vec2(mid - w * 0.5 + 8., y - 5.), 16., GOLD);
            draw_text("TOP RANK", mid - w * 0.5 + 22., y, 13., GOLD);
        }
    }
    let bar = action_bar(rect, rect.y + rect.h - 16.);
    let binding = format!("{} / {}", hints.restart, hints.confirm);
    key_action(bar, &binding, hints.touch, Icon::Restart, 1.);
}

/// Reused text storage for the table and dot matrix display.
pub(super) struct Buffers {
    pub pop: String,
    pub head: String,
    pub sub: String,
}
impl Buffers {
    pub fn new() -> Self {
        Self {
            pop: String::with_capacity(64),
            head: String::with_capacity(128),
            sub: String::with_capacity(128),
        }
    }
}

// ------------------------------------------------------------ side panels

/// One work-order row: a pictogram, its progress and a check once done.
fn mission(x: f32, y: f32, width: f32, done: bool, draw: impl FnOnce(Vec2, f32)) {
    let row = Rect::new(x - 8., y - 18., width + 16., 36.);
    shapes::fill_rounded_rect(
        row,
        10.,
        if done {
            with_alpha(GOLD, 0.1)
        } else {
            Color::new(1., 1., 1., 0.035)
        },
    );
    draw(vec2(x + 14., y), x + 44.);
    if done {
        icon(Icon::Check, vec2(x + width - 10., y), 18., GOLD);
    }
}

pub(super) fn side_panels(
    layout: &layout::Layout,
    table: &Table,
    view: &Presentation,
    best: Option<u32>,
    hints: &Hints,
) {
    use ferravex_render::text::{self, TextStyle};
    for rect in [
        layout.left,
        Rect::new(
            layout.right.x,
            layout.dmd.y + layout.dmd.h + 16.,
            layout.right.w,
            layout.right.h - layout.dmd.h - 16.,
        ),
    ] {
        shapes::fill_rounded_rect(rect, 18., PANEL);
        shapes::stroke_rounded_rect(rect, 18., 1., with_alpha(BRASS, 0.5));
    }
    // Backglass: name, score, best, balls and the multiplier lamps.
    let x = layout.left.x + 20.;
    let y = layout.left.y + 34.;
    text::draw_text_styled("Salvage Pinball", x, y, TextStyle::modern(24., CREAM));
    text::draw_text_styled(
        &quips::grouped(view.shown_score.round() as u32),
        x,
        y + 72.,
        TextStyle::classic(32., CREAM),
    );
    best_line(table, view, best, vec2(x, y + 104.), layout.left.w - 40.);
    for i in 0..3_u8 {
        ball_lamp(
            vec2(x + 11. + f32::from(i) * 28., y + 140.),
            10.,
            i < table.lives,
        );
    }
    for (n, level) in (2..=crate::MAX_MULTIPLIER).enumerate() {
        let c = vec2(x + 16. + n as f32 * 38., y + 186.);
        let lit = table.multiplier >= level;
        shapes::fill_circle(c, 15., Color::from_rgba(10, 14, 14, 255));
        shapes::fill_circle(
            c,
            12.,
            if lit {
                GOLD
            } else {
                Color::from_rgba(52, 46, 36, 255)
            },
        );
        let label = format!("{level}X");
        centered_in(
            &label,
            c + vec2(0., 5.),
            13.,
            22.,
            if lit { INK } else { with_alpha(CREAM, 0.4) },
        );
    }
    let reset = layout.reset;
    shapes::fill_rounded_rect(
        reset,
        10.,
        Color::new(0.1 + view.reset_hover * 0.05, 0.15, 0.14, 1.),
    );
    let c = reset.center();
    icon(
        Icon::Restart,
        c - vec2(18., 0.),
        22.,
        mix(MINT, CREAM, view.reset_hover),
    );
    if drawable(hints.restart) {
        keys(c.x + 2., c.y - CAP_H * 0.5 + 1., hints.restart, 0.8);
    }

    // Work orders: each mission as a pictogram and its progress.
    let x = layout.right.x + 26.;
    let width = layout.right.w - 52.;
    let y = layout.dmd.y + layout.dmd.h + 50.;
    text::draw_text_styled("Work orders", x - 4., y, TextStyle::modern(24., CREAM));
    let pulse = 0.65 + 0.35 * (view.time * 4.).sin() * view.flash_scale().min(1.);
    let all_lit = table.crane_armed || table.lamps.iter().all(|l| *l);
    mission(x, y + 42., width, all_lit, |at, left| {
        mini_bumper(at - vec2(0., 3.), 9., all_lit, 1.);
        for (i, lit) in table.lamps.into_iter().enumerate() {
            let on = lit || table.crane_armed;
            let c = vec2(left + 10. + i as f32 * 26., at.y);
            shapes::fill_circle(c, 9., Color::from_rgba(10, 14, 14, 255));
            shapes::fill_circle(
                c,
                7.,
                if on {
                    GOLD
                } else {
                    Color::from_rgba(52, 46, 36, 255)
                },
            );
        }
    });
    mission(x, y + 84., width, table.jackpots > 0, |at, left| {
        let armed = table.crane_armed;
        magnet(at, 26., if armed { pulse } else { 1. }, armed);
        let count = format!("\u{d7}{}", table.jackpots);
        text::draw_text_styled(&count, left, at.y + 6., TextStyle::modern(16., CREAM));
    });
    let skill_lit = table.waiting || table.launch_flight;
    mission(x, y + 126., width, table.skill_shots > 0, |at, left| {
        let color = if skill_lit {
            with_alpha(GOLD, pulse)
        } else {
            BRASS
        };
        icon(Icon::Target, at, 22., color);
        let count = format!("\u{d7}{}", table.skill_shots);
        text::draw_text_styled(&count, left, at.y + 6., TextStyle::modern(16., CREAM));
    });
    mission(x, y + 168., width, false, |at, left| {
        let saving = table.ball_save > 0.;
        icon(Icon::Shield, at, 22., if saving { MINT } else { BRASS });
        let bar = Rect::new(left, at.y - 4., (width - 64.).max(40.), 8.);
        shapes::fill_rounded_rect(bar, 4., Color::new(1., 1., 1., 0.07));
        let fill = (table.ball_save / SAVE_SECONDS).clamp(0., 1.);
        if fill > 0. {
            shapes::fill_rounded_rect(
                Rect::new(bar.x, bar.y, (bar.w * fill).max(8.), bar.h),
                4.,
                MINT,
            );
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_celebrate_only_real_improvements() {
        let first = Results::default();
        assert!(first.new_best(100));
        assert!(!first.new_best(0), "an empty game is not a record");
        let chase = Results {
            previous_best: Some(5_000),
            demo: false,
        };
        assert!(!chase.new_best(5_000) && chase.new_best(5_001));
        let demo = Results {
            previous_best: None,
            demo: true,
        };
        assert!(!demo.new_best(1_000_000));
    }

    #[test]
    fn objective_names_the_next_step_in_priority_order() {
        let mut table = Table::new();
        let objective = |table: &Table| objective(table, SessionState::Playing);
        assert_eq!(
            super::objective(&table, SessionState::Ready),
            ("INSERT HARD HAT", Sub::Pips { lit: 3, total: 3 })
        );
        assert_eq!(super::objective(&table, SessionState::Paused).0, "PAUSED");
        assert_eq!(
            objective(&table),
            ("SKILL SHOT", Sub::Points(crate::SKILL_SHOT_POINTS))
        );
        table.waiting = false;
        table.ball_save = 2.5;
        assert_eq!(objective(&table), ("BALL SAVE", Sub::Meter(0.5)));
        table.ball_save = 0.;
        table.lamps = [true, false, true];
        assert_eq!(
            objective(&table),
            ("LIGHT BUMPERS", Sub::Pips { lit: 2, total: 3 })
        );
        table.crane_armed = true;
        table.multiplier = 2;
        assert_eq!(objective(&table), ("CRANE ARMED", Sub::Value(5_000)));
        table.crane_hold = 0.5;
        assert_eq!(objective(&table).0, "MAGNET!");
        table.lives = 0;
        assert_eq!(objective(&table), ("SHIFT OVER", Sub::None));
    }

    #[test]
    fn idle_and_callout_headlines_stay_short_and_fit_the_narrowest_display() {
        // 320px canvas: 296px DMD with 276px of glass; 268px of card text.
        let inner = (320_f32 - 24.).clamp(1., 460.) - 28.;
        let mut table = Table::new();
        let mut heads = Vec::new();
        for state in [
            SessionState::Ready,
            SessionState::Playing,
            SessionState::Paused,
        ] {
            heads.push(objective(&table, state).0);
        }
        table.waiting = false;
        for setup in [
            |t: &mut Table| t.ball_save = 3.,
            |t: &mut Table| t.crane_armed = true,
            |t: &mut Table| t.crane_hold = 1.,
            |t: &mut Table| t.lives = 0,
        ] {
            let mut t = Table::new();
            t.waiting = false;
            setup(&mut t);
            heads.push(objective(&t, SessionState::Playing).0);
        }
        heads.extend([
            "CRANE ARMED!",
            "SKILL SHOT!",
            "OVERTIME PAY!",
            "ABSOLUTE CARNAGE! x12",
        ]);
        for head in heads {
            assert!(head.split_whitespace().count() <= 3, "{head}");
            let (size, _) = fitted_text(head, 16., inner).unwrap();
            assert!(size >= 13., "{head} shrinks to {size}");
        }
        for line in quips::all_lines() {
            let (size, _) = fitted_text(line, 13., inner).unwrap();
            assert!(size >= 11.5, "{line} shrinks to {size}");
        }
        for title in [
            "CLOCK IN!",
            "PAUSED",
            "OVERTIME LEGEND",
            "NEW BEST!  +12,300",
        ] {
            assert!(fitted_text(title, 21., inner).unwrap().0 >= 17.);
        }
    }

    #[test]
    fn oversized_or_unbound_bindings_fall_back_to_the_controls_gear() {
        assert!(drawable("Space") && drawable("A / Left") && drawable("R / Enter"));
        assert!(!drawable("Unbound"));
        assert!(!drawable("SEE CONTROLS"));
        assert!(!drawable(
            "Left Shift / Space / J / Pad 0 / Mouse 1 / Touch 0 / Touch 1"
        ));
        // Arrow keys draw as pictogram caps no wider than a letter cap.
        assert!(keycap_width("Left") <= keycap_width("A"));
    }
}
