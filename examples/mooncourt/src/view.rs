//! Court, interface and effects. Drawing only reads the model; effects come
//! from the events the latest step accepted, at the place they happened.
use crate::content::OPPONENTS;
use crate::model::{
    AIM_STRIP, Athlete, CARD_BUTTON, End, Event, FAR_COURT, Fallback, RACKET_HEIGHT, SKIP_BUTTON,
    SMASH_HEIGHT, SMASH_STAMINA, Shot, TIRED, inside, reach_at, recovery_at, speed_at,
};
use crate::{Game, Phase};
use ferravex_arcade::shapes::{self, Cap, Fill, Join, Stroke};
use ferravex_arcade::typography::{TextAlign, TextStyle, draw_text_styled, text_width};
use ferravex_arcade::*;
use ferravex_render::preferences::feedback_policy;
use std::f32::consts::TAU;

const BLUE: Color = Color::new(0.46, 0.78, 1., 1.);
const GOLD: Color = Color::new(1., 0.84, 0.43, 1.);
const RED: Color = Color::new(0.96, 0.43, 0.41, 1.);
const LINE: Color = Color::new(0.57, 0.69, 0.76, 1.);
const COURT: Color = Color::new(0.13, 0.20, 0.28, 1.);
const SPACE: Color = Color::new(0.03, 0.045, 0.085, 1.);
const PANEL: Color = Color::new(0.06, 0.085, 0.13, 1.);
const SOFT: Color = Color::new(0.84, 0.88, 0.93, 1.);
const SUIT: Color = Color::new(0.9, 0.93, 0.96, 1.);
const VISOR: Color = Color::new(0.05, 0.11, 0.18, 1.);
const DUST: Color = Color::new(0.8, 0.85, 0.9, 0.85);
/// One color per shot, shared by the dock, the ball's trail and contact bursts.
const DRIVE: Color = Color::new(0.5, 0.93, 0.78, 1.);
const LOB: Color = Color::new(0.76, 0.66, 1., 1.);
const SMASH: Color = Color::new(1., 0.6, 0.32, 1.);
const SHOTS: [Shot; 3] = [Shot::Drive, Shot::Lob, Shot::Smash];
/// The header area the host hands to `draw_objective`; effects aimed at the
/// score bug use the same anchors.
const BUG: Rect = Rect {
    x: 22.,
    y: 64.,
    w: 916.,
    h: 34.,
};

/// Court units to logical canvas units (feet on the ground plane).
fn position(p: Vec2) -> Vec2 {
    vec2(350. + p.x * 1.57, 343. + p.y * 1.18)
}
/// A point `height` units above court point `p`.
fn aloft(p: Vec2, height: f32) -> Vec2 {
    position(p) - vec2(0., height * 0.8)
}
/// The three aim targets on the rival's half, in court units.
fn lane_target(lane: usize) -> Vec2 {
    Vec2::new([-146., 0., 146.][lane.min(2)], -104.)
}
/// The lane under the pointer, on the far court or on the aim strip.
fn lane_at(p: Vec2) -> Option<usize> {
    if inside(FAR_COURT, p) {
        let third = (FAR_COURT[2] - FAR_COURT[0]) / 3.;
        Some((((p.x - FAR_COURT[0]) / third) as usize).min(2))
    } else if inside(AIM_STRIP, p) {
        Some((((p.x - AIM_STRIP[0]) / 157.) as usize).min(2))
    } else {
        None
    }
}
fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::new(x, y, w, h)
}
fn region(a: [f32; 4]) -> Rect {
    Rect::new(a[0], a[1], a[2] - a[0], a[3] - a[1])
}
/// A warning flash that holds steady when the player reduces flashing.
fn flicker(t: f32, hz: f32) -> f32 {
    if feedback_policy().reduced_flash() {
        0.5
    } else {
        pulse(t, hz)
    }
}
/// Keep an effect or marker on the visible court.
fn on_court(p: Vec2) -> Vec2 {
    vec2(p.x.clamp(56., 644.), p.y.clamp(192., 500.))
}
/// Shadowless modern text for clean surfaces; returns the drawn width.
fn ink(value: &str, x: f32, y: f32, size: f32, color: Color) -> f32 {
    draw_text_styled(value, x, y, TextStyle::modern(size, color)).width
}
/// Text vertically centered on `cy`, with the host's legibility shadow.
fn label(value: &str, x: f32, cy: f32, size: f32, color: Color, align: TextAlign) -> f32 {
    let s = style(size, color).with_align(align);
    draw_text_styled(value, x, s.centered_baseline(cy), s).width
}
fn keycap_width(key: &str) -> f32 {
    (text_width(key, TextStyle::modern(13., PAPER)) + 12.).max(20.)
}
/// Recessed track with a lit fill.
fn bar(x: f32, y: f32, w: f32, h: f32, fraction: f32, color: Color) {
    let f = if fraction.is_finite() {
        fraction.clamp(0., 1.)
    } else {
        0.
    };
    let radius = (h * 0.5).min(4.);
    shapes::fill_rounded_rect(rect(x, y, w, h), radius, Color::new(0.01, 0.015, 0.03, 0.7));
    if f > 0. {
        let fill = rect(x, y, (w * f).max(h), h);
        shapes::fill_rounded_rect(
            fill,
            radius,
            Fill::vertical(fill, lighten(color, 0.25), color),
        );
    }
}
/// Raised panel: lit gradient and hairline rim, without a blur pass.
fn slab(x: f32, y: f32, w: f32, h: f32, fill: Color) {
    let r = rect(x, y, w, h);
    shapes::fill_rounded_rect(r, 12., Fill::vertical(r, lighten(fill, 0.04), fill));
    shapes::stroke_rounded_rect(r, 12., 1., Color::new(1., 1., 1., 0.07));
}
/// Soft light pool with normal blending (no extra draw call).
fn pool(x: f32, y: f32, radius: f32, color: Color) {
    let c = vec2(x, y);
    shapes::fill_circle(
        c,
        radius,
        Fill::radial(c, radius, color, with_alpha(color, 0.)),
    );
}
fn court_line(a: Vec2, b: Vec2, width: f32, color: Color) {
    let a = position(a);
    let b = position(b);
    draw_line(a.x, a.y, b.x, b.y, width, color);
}
/// Three level bars for a training track.
fn level_pips(x: f32, y: f32, filled: u8, color: Color) {
    for i in 0..3_u8 {
        let r = rect(x + f32::from(i) * 20., y, 17., 5.);
        let c = if i < filled {
            color
        } else {
            Color::new(1., 1., 1., 0.1)
        };
        shapes::fill_rounded_rect(r, 2.5, c);
    }
}
/// Outline of a ground circle of `radius` court units around screen point `c`.
fn ground_ring(c: Vec2, radius: f32, width: f32, color: Color) {
    let mut points = [Vec2::ZERO; 28];
    for (i, p) in points.iter_mut().enumerate() {
        let a = i as f32 / 28. * TAU;
        *p = c + vec2(a.cos() * radius * 1.57, a.sin() * radius * 1.18);
    }
    shapes::polyline(
        &points,
        Stroke::new(width, color).closed(true).join(Join::Round),
    );
}
/// A filled ground circle of `radius` court units.
fn ground_disc(c: Vec2, radius: f32, color: Color) {
    draw_ellipse(c.x, c.y, radius * 1.57, radius * 1.18, 0., color);
}
fn shot_color(shot: Shot) -> Color {
    match shot {
        Shot::Drive => DRIVE,
        Shot::Lob => LOB,
        Shot::Smash => SMASH,
    }
}
fn shot_key(shot: Shot) -> &'static str {
    match shot {
        Shot::Drive => "Space",
        Shot::Lob => "X",
        Shot::Smash => "C",
    }
}
/// The contact stamina cost as a signed label; a test keeps it equal to the rules.
fn cost_label(shot: Shot) -> &'static str {
    match shot {
        Shot::Drive => "-7",
        Shot::Lob => "-4",
        Shot::Smash => "-26",
    }
}
/// A shot's flight as a pictogram `w` wide: a flat drive, a high lob or a
/// steep smash, from a ball to an arrowhead on the ground line.
fn shot_glyph(shot: Shot, c: Vec2, w: f32, color: Color) {
    let h = w * 0.5;
    let left = c.x - w * 0.5;
    let ground = c.y + h * 0.5;
    draw_line(
        left,
        ground,
        left + w,
        ground,
        1.2,
        with_alpha(color, 0.3 * color.a),
    );
    let (start, end, apex) = match shot {
        Shot::Drive => (
            vec2(left + w * 0.06, ground - h * 0.5),
            vec2(left + w * 0.9, ground),
            h * 0.32,
        ),
        Shot::Lob => (
            vec2(left + w * 0.06, ground - h * 0.3),
            vec2(left + w * 0.9, ground),
            h * 1.05,
        ),
        Shot::Smash => (
            vec2(left + w * 0.12, ground - h * 1.05),
            vec2(left + w * 0.78, ground),
            0.,
        ),
    };
    let mut points = [Vec2::ZERO; 10];
    for (i, p) in points.iter_mut().enumerate() {
        let t = i as f32 / 9.;
        *p = start.lerp(end, t) - vec2(0., apex * 4. * t * (1. - t));
    }
    let width = (w * 0.055).max(1.5);
    shapes::polyline(
        &points[..9],
        Stroke::new(width, color).cap(Cap::Round).join(Join::Round),
    );
    let dir = (points[9] - points[7]).normalize_or_zero();
    let side = vec2(-dir.y, dir.x);
    let s = w * 0.13;
    let tip = points[9];
    shapes::fill_triangle(
        tip + dir * s * 0.35,
        tip - dir * s + side * s * 0.62,
        tip - dir * s - side * s * 0.62,
        color,
    );
    if shot == Shot::Smash {
        // Speed streaks beside the strike.
        for k in [-1., 1.] {
            let off = side * (w * 0.1 * k);
            let a = start.lerp(end, 0.2) + off;
            let b = start.lerp(end, 0.5) + off;
            draw_line(a.x, a.y, b.x, b.y, 1., with_alpha(color, 0.45 * color.a));
        }
    }
    shapes::fill_circle(start, (w * 0.075).max(2.), with_alpha(GOLD, color.a));
}
/// The racket: an oval head on a short handle, for reach.
fn racket_glyph(c: Vec2, size: f32, color: Color) {
    let head = c + vec2(size * 0.12, -size * 0.12);
    let (sin, cos) = (-0.7_f32).sin_cos();
    let rotate = |v: Vec2| vec2(v.x * cos - v.y * sin, v.x * sin + v.y * cos);
    shapes::fill_capsule(
        c + vec2(-size * 0.4, size * 0.4),
        head + rotate(vec2(-size * 0.26, 0.)),
        size * 0.07,
        color,
    );
    let mut rim = [Vec2::ZERO; 20];
    for (i, p) in rim.iter_mut().enumerate() {
        let a = i as f32 / 20. * TAU;
        *p = head + rotate(vec2(a.cos() * size * 0.28, a.sin() * size * 0.21));
    }
    shapes::polyline(
        &rim,
        Stroke::new(size * 0.075, color)
            .closed(true)
            .join(Join::Round),
    );
    for k in [-0.5, 0., 0.5] {
        let a = head + rotate(vec2(-size * 0.24, k * size * 0.3));
        let b = head + rotate(vec2(size * 0.24, k * size * 0.3));
        draw_line(a.x, a.y, b.x, b.y, 0.8, with_alpha(color, 0.55 * color.a));
    }
}
/// The game ball with its seam.
fn ball_glyph(c: Vec2, r: f32, alpha: f32) {
    shapes::fill_circle(
        c,
        r,
        Fill::radial(
            c - vec2(r * 0.35, r * 0.35),
            r * 1.5,
            Color::new(1., 0.97, 0.8, alpha),
            with_alpha(GOLD, alpha),
        ),
    );
    shapes::stroke_arc(
        c + vec2(-r * 0.6, 0.),
        r * 0.75,
        -0.95,
        1.9,
        Stroke::new((r * 0.16).max(0.8), Color::new(1., 1., 1., 0.65 * alpha)),
    );
}
/// An athlete's helmet as a portrait badge: dome, tinted visor and beacon.
fn helmet(c: Vec2, d: f32, suit: Color, alpha: f32) {
    let r = d * 0.5;
    shapes::fill_circle(c, r, Color::new(0.04, 0.06, 0.1, 0.92 * alpha));
    shapes::stroke_circle(
        c,
        r - 0.5,
        (d * 0.06).max(1.),
        with_alpha(suit, 0.85 * alpha),
    );
    let head = c + vec2(0., r * 0.06);
    let hr = r * 0.6;
    shapes::fill_circle(
        head,
        hr,
        Fill::radial(
            head - vec2(hr * 0.35, hr * 0.4),
            hr * 1.5,
            with_alpha(PAPER, alpha),
            Color::new(0.66, 0.72, 0.79, alpha),
        ),
    );
    let visor = shapes::mix(VISOR, suit, 0.22);
    draw_ellipse(
        head.x + hr * 0.1,
        head.y + hr * 0.06,
        hr * 0.72,
        hr * 0.48,
        0.,
        with_alpha(visor, alpha),
    );
    draw_ellipse(
        head.x - hr * 0.16,
        head.y - hr * 0.1,
        hr * 0.22,
        hr * 0.12,
        -20.,
        Color::new(1., 1., 1., 0.6 * alpha),
    );
    shapes::fill_circle(
        c + vec2(r * 0.46, -r * 0.5),
        (r * 0.13).max(1.3),
        with_alpha(suit, alpha),
    );
}
/// The rival's half as a tiny three-lane court with `lane` lit.
fn lane_glyph(c: Vec2, lane: usize, lit: Color, dim: Color) {
    let (w, h) = (42., 20.);
    let (x, y) = (c.x - w * 0.5, c.y - h * 0.5);
    for i in 0..3 {
        let cell = rect(x + i as f32 * w / 3. + 1., y + 1., w / 3. - 2., h - 2.);
        shapes::fill_rounded_rect(cell, 2., if i == lane { lit } else { dim });
    }
    let dot = vec2(x + (lane as f32 + 0.5) * w / 3., c.y);
    shapes::stroke_circle(dot, 3.2, 1.2, with_alpha(SPACE, 0.7 * lit.a));
}
/// An astronaut at screen point `p` (feet), `k` times the court size.
/// `flash` animates the racket swing.
fn astronaut(p: Vec2, c: Color, flash: u16, step: f32, blink: bool, k: f32) {
    let swing = f32::from(flash) / 14.;
    let at = |x: f32, y: f32| p + vec2(x, y) * k;
    let line = |a: Vec2, b: Vec2, width: f32, color: Color| {
        draw_line(a.x, a.y, b.x, b.y, width * k, color);
    };
    let block = |x: f32, y: f32, w: f32, h: f32, color: Color| {
        let o = at(x, y);
        panel(o.x, o.y, w * k, h * k, color);
    };
    // Backpack, legs and boots.
    block(-12., -23., 5., 14., darken(c, 0.45));
    line(at(-4., -8.), at(-6. - step, -1.), 4., SUIT);
    line(at(4., -8.), at(6. + step, -1.), 4., SUIT);
    block(-9. - step, -2., 6., 3., darken(c, 0.35));
    block(3. + step, -2., 6., 3., darken(c, 0.35));
    // Suit torso with a lit chest plate.
    let o = at(-9., -25.);
    let body = rect(o.x, o.y, 18. * k, 18. * k);
    shapes::fill_rounded_rect(
        body,
        5. * k,
        Fill::vertical(body, lighten(c, 0.2), darken(c, 0.2)),
    );
    block(-5., -20., 10., 7., with_alpha(SUIT, 0.9));
    block(-3., -18., 2., 2., c);
    // Helmet, visor and antenna beacon.
    let head = at(0., -32.);
    shapes::fill_circle(
        head,
        10.5 * k,
        Fill::radial(
            head - vec2(3., 4.) * k,
            15. * k,
            PAPER,
            Color::new(0.68, 0.74, 0.8, 1.),
        ),
    );
    let visor = at(1., -32.);
    draw_ellipse(visor.x, visor.y, 7.5 * k, 5. * k, 0., VISOR);
    let glint = at(-2., -34.);
    draw_ellipse(
        glint.x,
        glint.y,
        2.5 * k,
        1.4 * k,
        -20.,
        Color::new(1., 1., 1., 0.55),
    );
    line(at(6., -40.), at(9., -47.), 1.2, LINE);
    let beacon = at(9., -47.);
    draw_poly(
        beacon.x,
        beacon.y,
        8,
        2. * k,
        0.,
        with_alpha(c, if blink { 1. } else { 0.55 }),
    );
    // Racket arm: raised through the swing, resting otherwise.
    let racket = at(18. + 7. * swing, -9. - 18. * swing);
    line(at(7., -20.), racket, 3., SUIT);
    let head_c = racket + vec2(6., -5.) * k;
    if flash > 0 {
        shapes::stroke_arc(
            at(4., -18.),
            26. * k,
            -2.6 + 1.6 * (1. - swing),
            1.5,
            Stroke::new(3. * k, with_alpha(GOLD, 0.45 * swing)),
        );
    }
    let rim = if flash > 0 { GOLD } else { c };
    shapes::stroke_circle(head_c, 7.5 * k, 2. * k, rim);
    let strings = with_alpha(LINE, 0.7);
    line(
        head_c - vec2(6., 0.) * k,
        head_c + vec2(6., 0.) * k,
        0.7,
        strings,
    );
    line(
        head_c - vec2(0., 6.) * k,
        head_c + vec2(0., 6.) * k,
        0.7,
        strings,
    );
}
/// A medal: a gold disc on a ribbon.
fn medal_glyph(c: Vec2, size: f32, alpha: f32) {
    let r = size * 0.3;
    let disc = c + vec2(0., size * 0.14);
    for s in [-1., 1.] {
        shapes::fill_polygon(
            &[
                c + vec2(s * size * 0.28, -size * 0.46),
                c + vec2(s * size * 0.08, -size * 0.46),
                disc + vec2(s * size * 0.06, -r * 0.4),
                disc + vec2(s * size * 0.22, -r * 0.6),
            ],
            if s < 0. {
                Color::new(0.4, 0.62, 0.95, alpha)
            } else {
                Color::new(0.85, 0.32, 0.36, alpha)
            },
        );
    }
    shapes::fill_circle(
        disc,
        r,
        Fill::radial(
            disc - vec2(r * 0.35, r * 0.4),
            r * 1.5,
            Color::new(1., 0.95, 0.72, alpha),
            Color::new(0.86, 0.62, 0.2, alpha),
        ),
    );
    shapes::stroke_circle(
        disc,
        r * 0.62,
        (size * 0.04).max(0.8),
        Color::new(0.7, 0.48, 0.12, 0.8 * alpha),
    );
}
/// A rally: the ball and two flights crossing the net in turn.
fn rally_glyph(c: Vec2, size: f32, alpha: f32) {
    let w = size * 0.5;
    let mut out = [Vec2::ZERO; 9];
    let mut back = [Vec2::ZERO; 9];
    for i in 0..9 {
        let t = i as f32 / 8.;
        let arc = 4. * t * (1. - t);
        out[i] = c + vec2(-w + 2. * w * t, size * 0.05 - arc * size * 0.36);
        back[i] = c + vec2(w - 2. * w * t, size * 0.32 - arc * size * 0.16);
    }
    let width = (size * 0.06).max(1.2);
    shapes::polyline(
        &out,
        Stroke::new(width, with_alpha(BLUE, alpha))
            .cap(Cap::Round)
            .join(Join::Round),
    );
    shapes::polyline(
        &back,
        Stroke::new(width, with_alpha(LINE, 0.8 * alpha))
            .cap(Cap::Round)
            .join(Join::Round),
    );
    draw_line(
        c.x,
        c.y + size * 0.42,
        c.x,
        c.y + size * 0.12,
        width,
        with_alpha(SOFT, 0.5 * alpha),
    );
    ball_glyph(out[4], size * 0.16, alpha);
}
/// An ace: a ball streaking past, untouched.
fn ace_glyph(c: Vec2, size: f32, alpha: f32) {
    let ball = c + vec2(size * 0.18, 0.);
    for (i, dy) in [-0.2_f32, 0., 0.2].into_iter().enumerate() {
        let len = size * (0.46 - 0.1 * (i as f32 - 1.).abs());
        let y = ball.y + dy * size;
        shapes::fill_capsule(
            vec2(ball.x - len, y),
            vec2(ball.x - size * 0.12, y),
            (size * 0.035).max(0.8),
            with_alpha(GOLD, 0.6 * alpha),
        );
    }
    glow(ball.x, ball.y, size * 0.4, with_alpha(GOLD, 0.25 * alpha));
    ball_glyph(ball, size * 0.22, alpha);
}
/// A wide call-to-action: icon, one word and the key that also triggers it.
fn button(r: Rect, glyph: Icon, word: &str, color: Color, hovered: bool, t: f32) {
    let lift = if hovered { 2. } else { 0. };
    let face = rect(r.x, r.y - lift, r.w, r.h);
    shapes::fill_rounded_rect(rect(r.x, r.y + 3., r.w, r.h), 11., darken(color, 0.6));
    if hovered {
        shapes::glow_disc(
            vec2(face.x + face.w * 0.5, face.y + face.h * 0.5),
            face.w * 0.42,
            with_alpha(color, 0.12 + 0.06 * pulse(t, 1.2)),
        );
    }
    shapes::fill_rounded_rect(
        face,
        11.,
        Fill::vertical(
            face,
            lighten(color, if hovered { 0.34 } else { 0.2 }),
            darken(color, 0.1),
        ),
    );
    let s = TextStyle::modern(18., INK);
    let word_w = text_width(word, s);
    let key_w = keycap_width("Space");
    let total = 18. + 8. + word_w + 14. + key_w;
    let x = face.x + face.w * 0.5 - total * 0.5;
    let cy = face.y + face.h * 0.5;
    icon(glyph, x + 9., cy, 18., INK);
    draw_text_styled(word, x + 26., s.centered_baseline(cy), s);
    keycap("Space", "", x + 26. + word_w + 14., cy - 10., PAPER);
}
impl ArcadeGame for Game {
    fn new() -> Self {
        Self::new()
    }
    fn update(&mut self, input: &Input) -> Cue {
        self.step(input)
    }
    fn draw(&self) {
        // Idle motion rests when the player reduces motion.
        let t = if feedback_policy().reduced_motion() {
            0.
        } else {
            clock()
        };
        let hover = pointer();
        self.draw_backdrop(t);
        self.draw_court(t, hover);
        self.draw_aim(hover);
        self.draw_panel(t);
        match self.phase {
            Phase::Briefing
            | Phase::Training
            | Phase::Setback
            | Phase::Champion
            | Phase::Eliminated => self.draw_card(t, hover),
            _ => {}
        }
    }
    fn hud(&self) -> Hud {
        Hud {
            stage: self.opponent().court.into(),
            objective: "Win two games".into(),
            status: self.status(),
            score: self.score,
        }
    }
    fn outcome(&self) -> Outcome {
        self.outcome()
    }
    fn summary(&self) -> String {
        self.summary()
    }
    fn bot_input(&self, tick: u64) -> Input {
        self.pilot(tick)
    }
    fn progress(&self) -> Option<(u32, u32)> {
        Some((self.match_number() as u32, OPPONENTS.len() as u32))
    }
    fn effects(&self, _cue: Cue, fx: &mut Juice) {
        for &event in self.feed.events() {
            self.effect(event, fx);
        }
    }
    fn draw_objective(&self, area: Rect) -> bool {
        self.draw_scorebug(area);
        true
    }
    fn draw_result(&self, area: Rect) -> bool {
        let col = area.w / 4.;
        let top = area.y + 44.;
        let value_y = area.y + 108.;
        for i in 0..4 {
            let cx = area.x + col * (i as f32 + 0.5);
            let value = match i {
                0 => {
                    glow(cx, top, 34., with_alpha(GOLD, 0.16));
                    icon(Icon::Trophy, cx, top, 44., GOLD);
                    format!("{}/6", self.matches_won())
                }
                1 => {
                    medal_glyph(vec2(cx, top), 46., 1.);
                    self.medal_total().to_string()
                }
                2 => {
                    ace_glyph(vec2(cx, top), 48., 1.);
                    self.aces.to_string()
                }
                _ => {
                    rally_glyph(vec2(cx, top), 52., 1.);
                    self.best_rally.to_string()
                }
            };
            label(&value, cx, value_y, 32., PAPER, TextAlign::Center);
        }
        true
    }
}
impl Game {
    fn color(&self) -> Color {
        let [r, g, b] = self.opponent().color;
        Color::new(r, g, b, 1.)
    }
    fn side_color(&self, side: usize) -> Color {
        if side == 0 { BLUE } else { self.color() }
    }
    /// Where an athlete's racket head is drawn.
    fn racket_at(&self, side: usize) -> Vec2 {
        position(self.athletes[side].pos) + vec2(24., -14.)
    }
    /// Status line for screen readers; the canvas shows all of this visually.
    fn status(&self) -> String {
        match self.phase {
            Phase::Briefing => self.opponent().advice.into(),
            Phase::Serve | Phase::Rally | Phase::Point => format!(
                "Games {}-{}, points {}-{}, stamina {}, rally {}",
                self.games[0],
                self.games[1],
                self.point_text(0),
                self.point_text(1),
                self.athletes[0].stamina.round() as u32,
                self.rally
            ),
            Phase::Training => "Match won: train speed, stamina or reach, or skip.".into(),
            Phase::Setback => format!("Match lost: {} wildcards left.", self.retries),
            Phase::Champion | Phase::Eliminated => self.summary(),
        }
    }
    /// 0..1: how close the rival's shot is to the player's racket.
    fn incoming_near(&self) -> f32 {
        if self.phase != Phase::Rally || self.ball.hitter != 1 {
            return 0.;
        }
        let d = (self.ball.pos - self.athletes[0].pos).length();
        (1. - (d - self.reach()) / 120.).clamp(0., 1.)
    }
    /// The ball is in the player's reach and a fresh swing would strike it.
    fn ready(&self) -> bool {
        self.phase == Phase::Rally && self.can_hit(0) && self.athletes[0].cooldown == 0
    }
    fn draw_backdrop(&self, t: f32) {
        let area = rect(20., 116., 646., 452.);
        shapes::fill_rounded_rect(
            area,
            14.,
            Fill::vertical(area, SPACE, Color::new(0.06, 0.08, 0.13, 1.)),
        );
        // Stars twinkle on the presentation clock, so cards stay alive too.
        for i in 0..42_u32 {
            let x = 28. + ((i * 137 + 31) % 626) as f32;
            let y = 124. + ((i * 79 + 17) % 429) as f32;
            let size = if i % 7 == 0 { 2. } else { 1.2 };
            let a = 0.3 + 0.5 * pulse(t + i as f32 * 0.37, 0.25 + (i % 5) as f32 * 0.06);
            panel(x, y, size, size, with_alpha(LINE, a));
        }
        // Earth, lit from the left, with a thin atmosphere and drifting cloud.
        let earth = vec2(607., 155.);
        pool(earth.x, earth.y, 46., Color::new(0.3, 0.6, 1., 0.18));
        shapes::fill_circle(
            earth,
            26.,
            Fill::radial(
                earth - vec2(9., 8.),
                36.,
                Color::new(0.33, 0.62, 0.86, 1.),
                Color::new(0.08, 0.2, 0.36, 1.),
            ),
        );
        draw_ellipse(599., 150., 8., 15., -25., Color::new(0.36, 0.66, 0.5, 0.9));
        draw_ellipse(615., 162., 6., 10., 35., Color::new(0.36, 0.62, 0.48, 0.8));
        let drift = bob(t, 0.05, 5.);
        draw_ellipse(
            603. + drift,
            141.,
            9.,
            3.,
            -10.,
            Color::new(0.92, 0.95, 0.97, 0.7),
        );
        shapes::stroke_circle(earth, 26.5, 1.2, Color::new(0.6, 0.85, 1., 0.45));
    }
    fn draw_court(&self, t: f32, hover: Vec2) {
        // Court surround and surface.
        shapes::fill_rounded_rect(
            rect(46., 190., 612., 322.),
            14.,
            Color::new(0., 0., 0., 0.35),
        );
        let surround = rect(44., 184., 612., 322.);
        shapes::fill_rounded_rect(
            surround,
            14.,
            Fill::vertical(
                surround,
                Color::new(0.09, 0.12, 0.18, 1.),
                Color::new(0.12, 0.16, 0.22, 1.),
            ),
        );
        shapes::stroke_rounded_rect(surround, 14., 1., Color::new(1., 1., 1., 0.06));
        let surface = rect(67.4, 195.5, 565.2, 295.);
        shapes::fill_rounded_rect(
            surface,
            2.,
            Fill::vertical(surface, darken(COURT, 0.15), lighten(COURT, 0.06)),
        );
        for band in 0..6 {
            if band % 2 == 0 {
                panel(
                    67.4,
                    195.5 + band as f32 * 49.2,
                    565.2,
                    49.2,
                    Color::new(1., 1., 1., 0.025),
                );
            }
        }
        let live = matches!(self.phase, Phase::Serve | Phase::Rally);
        let hovered = lane_at(hover).filter(|_| live);
        if live {
            // The aimed third of the far court, and a fainter hovered third.
            let x = 67.4 + self.aim as f32 * 188.4;
            panel(x, 195.5, 188.4, 147.5, with_alpha(BLUE, 0.07));
            if let Some(lane) = hovered.filter(|&l| l != self.aim) {
                let x = 67.4 + lane as f32 * 188.4;
                panel(x, 195.5, 188.4, 147.5, Color::new(1., 1., 1., 0.035));
            }
        }
        let line = Color::new(0.82, 0.9, 0.95, 0.75);
        let service = Color::new(0.5, 0.64, 0.74, 0.55);
        for x in [-180., 180.] {
            court_line(vec2(x, -125.), vec2(x, 125.), 1.6, line);
        }
        for y in [-125., 125.] {
            court_line(vec2(-180., y), vec2(180., y), 1.6, line);
        }
        for y in [-64., 64.] {
            court_line(vec2(-180., y), vec2(180., y), 1.2, service);
        }
        court_line(vec2(0., -64.), vec2(0., 64.), 1.2, service);
        if live {
            self.draw_targets(t, hovered);
        }
        self.draw_net();
        if self.phase == Phase::Rally {
            self.draw_landing();
        }
        if live {
            self.draw_reach(t);
        }
        self.draw_athlete(self.athletes[1], self.color(), false, t);
        self.draw_athlete(self.athletes[0], BLUE, true, t);
        if self.phase == Phase::Rally {
            self.draw_preview(t);
        }
        if matches!(self.phase, Phase::Serve | Phase::Rally | Phase::Point) {
            self.draw_ball();
        }
        match self.phase {
            Phase::Point => {
                self.draw_point_mark();
                self.draw_point_pill();
            }
            Phase::Serve => self.draw_serve_cue(t),
            _ => {}
        }
    }
    /// The three aim targets: the chosen one glows and breathes, a hovered
    /// one brightens, each shows the number key that selects it.
    fn draw_targets(&self, t: f32, hovered: Option<usize>) {
        for lane in 0..3 {
            let c = position(lane_target(lane));
            let digit = ["1", "2", "3"][lane];
            if lane == self.aim {
                let k = pulse(t, 0.9);
                glow(c.x, c.y, 36., with_alpha(BLUE, 0.2 + 0.08 * k));
                ground_disc(c, 13., with_alpha(BLUE, 0.18));
                ground_ring(c, 13., 2., BLUE);
                ground_ring(c, 6.5, 1.4, with_alpha(BLUE, 0.85));
                ground_ring(c, 16. + 6. * k, 1.2, with_alpha(BLUE, 0.5 * (1. - k)));
                shapes::fill_circle(c, 2.4, PAPER);
            } else {
                let on = hovered == Some(lane);
                if on {
                    ground_disc(c, 13., with_alpha(BLUE, 0.12));
                }
                ground_ring(
                    c,
                    13.,
                    if on { 1.8 } else { 1.2 },
                    with_alpha(if on { BLUE } else { LINE }, if on { 0.85 } else { 0.32 }),
                );
                let s = TextStyle::modern(12., with_alpha(SOFT, if on { 0.95 } else { 0.45 }))
                    .with_align(TextAlign::Center);
                draw_text_styled(digit, c.x, s.centered_baseline(c.y), s);
            }
        }
    }
    /// Net: shadow, mesh, tape and posts. A net fault makes it shudder.
    fn draw_net(&self) {
        let shudder = if self.phase == Phase::Point && self.last_end == Some(End::Net) {
            let k = (1. - f32::from(self.timer) / 50.).max(0.);
            (f32::from(self.timer) * 0.9).sin() * 4. * k
        } else {
            0.
        };
        panel(50., 343., 600., 8., Color::new(0., 0., 0., 0.22));
        panel(50., 333.4, 600., 9.6, Color::new(0.04, 0.06, 0.1, 0.45));
        let sag = |x: f32| shudder * ((x - 50.) / 600. * std::f32::consts::PI).sin();
        for i in 0..20 {
            let x = 56. + i as f32 * 30.;
            draw_line(x, 343., x, 333.4 + sag(x), 0.8, with_alpha(LINE, 0.45));
        }
        draw_line(50., 338.2, 650., 338.2, 0.7, with_alpha(LINE, 0.45));
        if shudder == 0. {
            shapes::fill_rounded_rect(rect(50., 332., 600., 3.), 1.5, PAPER);
        } else {
            let mut tape = [Vec2::ZERO; 13];
            for (i, p) in tape.iter_mut().enumerate() {
                let x = 50. + i as f32 * 50.;
                *p = vec2(x, 333.5 + sag(x));
            }
            shapes::polyline(
                &tape,
                Stroke::new(3., PAPER).cap(Cap::Round).join(Join::Round),
            );
        }
        for x in [50., 650.] {
            shapes::fill_capsule(
                vec2(x, 330.),
                vec2(x, 345.),
                3.,
                Color::new(0.75, 0.8, 0.86, 1.),
            );
        }
    }
    /// Where the flight lands: a fixed ring with a second ring that closes on
    /// it as the ball arrives. After the bounce, a red mark shows where an
    /// unreturned ball would bounce again and end the point.
    fn draw_landing(&self) {
        let incoming = self.ball.hitter == 1;
        // The color of whoever must play the ball.
        let color = if incoming { BLUE } else { self.color() };
        if !self.ball.bounced {
            let target = position(self.ball.target);
            let speed = self.ball.velocity.length().max(1.);
            let remaining = (self.ball.target - self.ball.pos).length() / speed;
            let elapsed = f32::from(self.ball.flight_ticks) * TICK_DT;
            let f = (remaining / (remaining + elapsed).max(0.01)).clamp(0., 1.);
            let a = if incoming { 1. } else { 0.55 };
            ground_disc(target, 11., with_alpha(color, 0.16 * a));
            ground_ring(target, 11., 1.6, with_alpha(color, 0.9 * a));
            ground_ring(
                target,
                11. + 20. * f,
                1.4,
                with_alpha(color, (0.25 + 0.55 * (1. - f)) * a),
            );
            draw_line(
                target.x - 26.,
                target.y,
                target.x - 19.,
                target.y,
                1.,
                with_alpha(color, a),
            );
            draw_line(
                target.x + 19.,
                target.y,
                target.x + 26.,
                target.y,
                1.,
                with_alpha(color, a),
            );
        } else if incoming {
            let g = self.opponent().gravity;
            let v = self.ball.vertical;
            let h = self.ball.height.max(0.);
            let time = (v + (v * v + 2. * g * h).sqrt()) / g;
            let land = position(self.ball.pos + self.ball.velocity * time);
            if inside([56., 192., 644., 500.], land) {
                ground_ring(land, 7., 1.4, with_alpha(RED, 0.8));
                for s in [-1., 1.] {
                    draw_line(
                        land.x - 4.,
                        land.y - 3. * s,
                        land.x + 4.,
                        land.y + 3. * s,
                        1.4,
                        with_alpha(RED, 0.8),
                    );
                }
            }
        }
    }
    /// The racket's real reach around the player's feet. It turns gold when a
    /// swing would connect.
    fn draw_reach(&self, t: f32) {
        let p = position(self.athletes[0].pos);
        let reach = self.reach();
        if self.ready() {
            let k = pulse(t, 3.);
            pool(p.x, p.y - 14., 42., with_alpha(GOLD, 0.18 + 0.1 * k));
            ground_disc(p, reach, with_alpha(GOLD, 0.08));
            ground_ring(p, reach, 2.2 + k, GOLD);
        } else {
            let near = self.incoming_near();
            ground_ring(p, reach, 1.1, with_alpha(BLUE, 0.14 + 0.36 * near));
        }
    }
    /// A dotted preview of the drive a swing would send to the chosen target.
    fn draw_preview(&self, t: f32) {
        let ready = self.ready();
        let near = self.incoming_near();
        let a = if ready { 0.85 } else { 0.4 * near * near };
        if a < 0.02 {
            return;
        }
        let g = self.opponent().gravity;
        let start = self.ball.pos;
        let h0 = self.ball.height.max(5.);
        let target = lane_target(self.aim);
        let duration = 1.25;
        let v0 = (g * 0.5 * duration * duration - h0) / duration;
        let march = (t * 1.6).fract();
        for i in 0..16 {
            let u = (i as f32 + march) / 16.;
            let s = u * duration;
            let h = (h0 + v0 * s - g * 0.5 * s * s).max(0.);
            let p = aloft(start.lerp(target, u), h);
            let fade = (u * 5.).min(1.) * a;
            shapes::fill_circle(p, 1.6 + 0.8 * (1. - u), with_alpha(BLUE, fade));
        }
    }
    fn draw_ball(&self) {
        let ground = position(self.ball.pos);
        let h = self.ball.height.max(0.);
        let lift = (h / 60.).clamp(0., 1.);
        draw_ellipse(
            ground.x,
            ground.y,
            7. - 3. * lift,
            3. - lift,
            0.,
            Color::new(0.01, 0.02, 0.04, 0.75 - 0.4 * lift),
        );
        let ball = aloft(self.ball.pos, h);
        // The height line turns gold while an incoming ball is low enough to hit.
        let low = self.ball.hitter == 1 && h <= RACKET_HEIGHT && self.ball.pos.y > 0.;
        draw_line(
            ground.x,
            ground.y,
            ball.x,
            ball.y,
            1.,
            if low {
                with_alpha(GOLD, 0.5)
            } else {
                Color::new(0.65, 0.75, 0.85, 0.2)
            },
        );
        if self.phase == Phase::Rally {
            // The recent arc, traced back through the same ballistic flight.
            let color = lighten(shot_color(self.ball_shot), 0.2);
            let g = self.opponent().gravity;
            let mut prev = ball;
            for i in 1..=9_u16 {
                let ticks = i * 2;
                if ticks > self.ball.flight_ticks {
                    break;
                }
                let s = f32::from(ticks) * TICK_DT;
                let pos = self.ball.pos - self.ball.velocity * s;
                let height = (self.ball.height - self.ball.vertical * s - g * 0.5 * s * s).max(0.);
                let point = aloft(pos, height);
                let fade = 1. - f32::from(i) / 10.;
                let width = if self.ball_shot == Shot::Smash {
                    6.5
                } else {
                    4.5
                };
                draw_line(
                    prev.x,
                    prev.y,
                    point.x,
                    point.y,
                    1. + width * fade,
                    with_alpha(color, 0.55 * fade),
                );
                prev = point;
            }
        }
        pool(ball.x, ball.y, 15., with_alpha(GOLD, 0.35));
        ball_glyph(ball, 5.5, 1.);
    }
    /// The spot where the rally died stays marked through the pause.
    fn draw_point_mark(&self) {
        let at = on_court(position(self.ball.pos));
        match self.last_end {
            Some(End::Out) => {
                ground_ring(at, 10., 1.6, with_alpha(RED, 0.8));
                for s in [-1., 1.] {
                    shapes::fill_capsule(at + vec2(-7., -6. * s), at + vec2(7., 6. * s), 1.8, RED);
                }
            }
            Some(End::Bounce | End::Passed) => {
                ground_ring(at, 12., 2., self.side_color(self.point_winner));
            }
            _ => {}
        }
    }
    /// The new score, center court, through the pause between points.
    fn draw_point_pill(&self) {
        let k = f32::from(self.timer);
        let a = (k / 8.).min((90. - k) / 14.).clamp(0., 1.);
        let pop = 1. + 0.2 * (1. - (k / 12.).clamp(0., 1.));
        let winner = self.point_winner;
        let wc = self.side_color(winner);
        let c = vec2(350., 292.);
        let r = rect(c.x - 128., c.y - 30., 256., 60.);
        shapes::fill_rounded_rect(
            rect(r.x, r.y + 5., r.w, r.h),
            30.,
            Color::new(0., 0., 0., 0.4 * a),
        );
        shapes::fill_rounded_rect(r, 30., Color::new(0.04, 0.06, 0.1, 0.94 * a));
        shapes::stroke_rounded_rect(r, 30., 1.5, with_alpha(wc, 0.75 * a));
        if self.game_point_won() {
            glow(c.x - 70., c.y, 34., with_alpha(wc, 0.3 * a));
            helmet(c + vec2(-70., 0.), 38., wc, a);
            label(
                "GAME",
                c.x + 22.,
                c.y,
                32. * pop,
                with_alpha(wc, a),
                TextAlign::Center,
            );
            return;
        }
        helmet(c + vec2(-98., 0.), 32., BLUE, a);
        helmet(c + vec2(98., 0.), 32., self.color(), a);
        for side in 0..2 {
            let won = side == winner;
            let size = if won { 30. * pop } else { 26. };
            let color = with_alpha(if won { wc } else { SOFT }, if won { a } else { 0.75 * a });
            let (x, align) = if side == 0 {
                (c.x - 16., TextAlign::Right)
            } else {
                (c.x + 16., TextAlign::Left)
            };
            label(self.point_text(side), x, c.y, size, color, align);
        }
        shapes::fill_circle(c - vec2(0., 6.), 2.4, with_alpha(MUTED, a));
        shapes::fill_circle(c + vec2(0., 6.), 2.4, with_alpha(MUTED, a));
    }
    /// Who serves: the key over the player's head, or the rival's countdown.
    fn draw_serve_cue(&self, t: f32) {
        if self.server == 0 {
            let p = position(self.athletes[0].pos);
            let w = keycap_width("Space");
            let y = p.y - 86. + bob(t, 1.1, 3.);
            glow(
                p.x,
                y + 10.,
                30.,
                with_alpha(GOLD, 0.16 + 0.08 * pulse(t, 1.1)),
            );
            keycap("Space", "", p.x - w * 0.5, y, PAPER);
        } else {
            let p = position(self.athletes[1].pos);
            let f = f32::from(self.timer) / 90.;
            ring_meter(p.x, p.y - 24., 30., 3., f, self.color());
        }
    }
    /// Aim strip: each lane's key and a tiny court with that third lit.
    fn draw_aim(&self, hover: Vec2) {
        let live = matches!(self.phase, Phase::Serve | Phase::Rally);
        icon(
            Icon::Target,
            92.,
            536.,
            20.,
            with_alpha(MUTED, if live { 0.9 } else { 0.5 }),
        );
        let track = rect(116., 517., 479., 38.);
        shapes::fill_rounded_rect(track, 12., Color::new(0.02, 0.03, 0.06, 0.75));
        shapes::stroke_rounded_rect(track, 12., 1., Color::new(1., 1., 1., 0.06));
        let hovered = if live && inside(AIM_STRIP, hover) {
            lane_at(hover)
        } else {
            None
        };
        for i in 0..3 {
            let x = AIM_STRIP[0] + i as f32 * 157.;
            let r = rect(x, 520., 149., 32.);
            let selected = i == self.aim;
            if selected {
                shapes::fill_rounded_rect(
                    r,
                    10.,
                    Fill::vertical(r, lighten(BLUE, 0.15), darken(BLUE, 0.15)),
                );
            } else if hovered == Some(i) {
                shapes::fill_rounded_rect(r, 10., Color::new(1., 1., 1., 0.06));
                shapes::stroke_rounded_rect(r, 10., 1., with_alpha(BLUE, 0.5));
            }
            keycap(["1", "2", "3"][i], "", x + 14., 526., PAPER);
            let (lit, dim) = if selected {
                (PAPER, with_alpha(INK, 0.25))
            } else {
                (with_alpha(BLUE, 0.85), with_alpha(LINE, 0.18))
            };
            lane_glyph(vec2(x + 96., 536.), i, lit, dim);
        }
    }
    fn draw_athlete(&self, a: Athlete, c: Color, near: bool, t: f32) {
        let p = position(a.pos);
        let moving = a.velocity.length_squared() > 100.;
        let step = (self.tick as f32 * 0.24).sin() * if moving { 3. } else { 0. };
        let lift = if moving {
            0.
        } else {
            bob(t + if near { 0. } else { 0.7 }, 0.6, 1.2).abs()
        };
        draw_ellipse(
            p.x,
            p.y + 2.,
            14.,
            5.,
            0.,
            Color::new(0.01, 0.02, 0.04, 0.55),
        );
        if moving {
            // Low-gravity dust kicked up behind the boots.
            let back = -a.velocity.normalize_or_zero();
            for i in 1..=3 {
                let d = back * (7. * i as f32) + vec2(0., -(i as f32) * 1.5);
                let puff = 1. - i as f32 / 4.;
                draw_poly(
                    p.x + d.x * 1.57,
                    p.y + d.y * 1.18,
                    10,
                    2. + i as f32,
                    0.,
                    Color::new(0.75, 0.8, 0.86, 0.22 * puff),
                );
            }
        }
        let blink = pulse(t, 0.8) > 0.4;
        astronaut(vec2(p.x, p.y - lift), c, a.flash, step, blink, 1.);
        if near {
            draw_triangle(
                vec2(p.x - 5., p.y + 13.),
                vec2(p.x + 5., p.y + 13.),
                vec2(p.x, p.y + 8.),
                BLUE,
            );
            // Too tired to smash: a bolt flickers over the helmet.
            if a.stamina < SMASH_STAMINA && matches!(self.phase, Phase::Serve | Phase::Rally) {
                let k = flicker(t, if a.stamina < TIRED { 3. } else { 1.5 });
                icon(
                    Icon::Bolt,
                    p.x - 14.,
                    p.y - 58. - lift,
                    14.,
                    with_alpha(if a.stamina < TIRED { RED } else { GOLD }, 0.45 + 0.55 * k),
                );
            }
        }
    }
    /// Header score bug: helmets, games won as balls, points, the server's
    /// ball, deuce, and the remaining wildcards as hearts.
    fn draw_scorebug(&self, area: Rect) {
        let cy = area.y + area.h * 0.5;
        let x0 = area.x + 4.;
        let plate = rect(x0 - 2., cy - 16., 240., 32.);
        shapes::fill_rounded_rect(plate, 16., Color::new(1., 1., 1., 0.04));
        let live = matches!(self.phase, Phase::Serve | Phase::Rally | Phase::Point);
        let pause = if self.phase == Phase::Point {
            1. - (f32::from(self.timer) / 14.).clamp(0., 1.)
        } else {
            0.
        };
        for side in 0..2 {
            let color = self.side_color(side);
            let (helm, games, pill) = if side == 0 {
                (x0 + 14., x0 + 38., x0 + 64.)
            } else {
                (x0 + 222., x0 + 184., x0 + 130.)
            };
            helmet(vec2(helm, cy), 28., color, 1.);
            if live && self.server == side {
                let b = vec2(helm + 11., cy - 10.);
                glow(b.x, b.y, 9., with_alpha(GOLD, 0.4));
                ball_glyph(b, 4.2, 1.);
            }
            for i in 0..2 {
                let at = vec2(games + i as f32 * 14., cy);
                if self.games[side] > i as u8 {
                    shapes::fill_circle(at, 5., color);
                    shapes::stroke_arc(
                        at + vec2(-3.4, 0.),
                        3.8,
                        -0.8,
                        1.6,
                        Stroke::new(0.9, Color::new(1., 1., 1., 0.55)),
                    );
                } else {
                    shapes::stroke_circle(at, 4.5, 1.4, with_alpha(color, 0.4));
                }
            }
            let leads = self.points[side] > self.points[1 - side];
            let scored = self.phase == Phase::Point && self.point_winner == side;
            let r = rect(pill, cy - 13., 46., 26.);
            shapes::fill_rounded_rect(r, 8., Color::new(0.01, 0.02, 0.04, 0.75));
            if leads || scored {
                shapes::stroke_rounded_rect(r, 8., 1.2, with_alpha(color, 0.8));
            }
            let size = 19. * (1. + 0.25 * if scored { pause } else { 0. });
            label(
                self.point_text(side),
                pill + 23.,
                cy,
                size,
                if leads || scored {
                    lighten(color, 0.2)
                } else {
                    PAPER
                },
                TextAlign::Center,
            );
        }
        shapes::fill_circle(vec2(x0 + 120., cy - 4.), 1.8, MUTED);
        shapes::fill_circle(vec2(x0 + 120., cy + 4.), 1.8, MUTED);
        let name = self.opponent().name;
        let w = label(name, x0 + 246., cy, 17., self.color(), TextAlign::Left);
        if self.points[0] >= 3 && self.points[0] == self.points[1] {
            chip("DEUCE", x0 + 256. + w, cy - 10., 13., GOLD);
        }
        for i in 0..3 {
            let lit = i < self.retries;
            icon(
                Icon::Heart,
                694. + f32::from(i) * 21.,
                cy,
                16.,
                if lit { RED } else { with_alpha(MUTED, 0.28) },
            );
        }
    }
    /// Where the score bug draws side `side`'s `i`-th game ball.
    fn bug_game(side: usize, i: u8) -> Vec2 {
        let x = BUG.x + 4. + if side == 0 { 38. } else { 184. };
        vec2(x + f32::from(i) * 14., BUG.y + BUG.h * 0.5)
    }
    fn draw_panel(&self, t: f32) {
        slab(680., 120., 256., 440., PANEL);
        // Stamina: a bolt meter with the smash threshold notched into it.
        let me = self.athletes[0].stamina;
        let tired = me < TIRED;
        let low = me < SMASH_STAMINA;
        let k = flicker(t, if tired { 3. } else { 1.6 });
        let (bolt, fill) = if tired {
            (shapes::mix(RED, PAPER, 0.35 * k), RED)
        } else if low {
            let c = shapes::mix(GOLD, RED, k);
            (c, c)
        } else {
            (BLUE, BLUE)
        };
        if low {
            glow(708., 156., 20., with_alpha(fill, 0.25 * k));
        }
        icon(Icon::Bolt, 708., 156., 24., bolt);
        bar(726., 149., 128., 14., me / 100., fill);
        let notch = 726. + 128. * SMASH_STAMINA / 100.;
        draw_line(
            notch,
            146.,
            notch,
            166.,
            2.,
            Color::new(0.02, 0.03, 0.05, 0.9),
        );
        draw_triangle(
            vec2(notch - 4., 170.),
            vec2(notch + 4., 170.),
            vec2(notch, 165.),
            with_alpha(SMASH, if low { 0.5 + 0.5 * k } else { 0.75 }),
        );
        label(
            &(me.round() as u32).to_string(),
            918.,
            156.,
            20.,
            PAPER,
            TextAlign::Right,
        );
        let them = self.athletes[1].stamina;
        helmet(vec2(708., 196.), 22., self.color(), 1.);
        bar(726., 192., 128., 8., them / 100., self.color());
        ink_right(&(them.round() as u32).to_string(), 918., 201., 15., MUTED);
        divider(694., 222., 228., Color::new(1., 1., 1., 0.12));
        for (i, &shot) in SHOTS.iter().enumerate() {
            self.draw_shot_row(i, shot, t);
        }
        divider(694., 418., 228., Color::new(1., 1., 1., 0.12));
        // Rally: the ball and the current count, the best rally beside a crown.
        let hit = f32::from(self.athletes[0].flash.max(self.athletes[1].flash)) / 14.;
        let pop = if self.phase == Phase::Rally {
            1. + 0.22 * hit
        } else {
            1.
        };
        rally_glyph(vec2(710., 450.), 30., 1.);
        label(
            &self.rally.to_string(),
            730.,
            451.,
            30. * pop,
            if self.rally > 0 { GOLD } else { MUTED },
            TextAlign::Left,
        );
        icon(Icon::Crown, 864., 450., 18., with_alpha(GOLD, 0.8));
        ink_right(&self.best_rally.to_string(), 918., 456., 17., SOFT);
        divider(694., 482., 228., Color::new(1., 1., 1., 0.12));
        // Training levels: footwork, endurance and reach.
        for i in 0..3 {
            let cx = 722. + i as f32 * 76.;
            self.training_icon(i, vec2(cx, 508.), 22., BLUE);
            level_pips(cx - 28.5, 528., self.training[i], BLUE);
        }
    }
    fn training_icon(&self, track: usize, c: Vec2, size: f32, color: Color) {
        match track {
            0 => icon(Icon::Wind, c.x, c.y, size, color),
            1 => icon(Icon::Battery, c.x, c.y, size, color),
            _ => racket_glyph(c, size * 1.15, color),
        }
    }
    /// One shot key: its cap, its flight and its stamina cost. It lights when
    /// a swing would connect, and dims red when that shot is unavailable.
    fn draw_shot_row(&self, i: usize, shot: Shot, t: f32) {
        let y = 232. + i as f32 * 60.;
        let r = rect(692., y, 232., 52.);
        let color = shot_color(shot);
        let reachable = self.ready();
        let serving = self.phase == Phase::Serve && self.server == 0;
        let usable = shot != Shot::Smash
            || (self.athletes[0].stamina >= SMASH_STAMINA
                && (!reachable || self.ball.height >= SMASH_HEIGHT));
        let lit = usable && (reachable || (serving && shot == Shot::Drive));
        let swinging = self.phase == Phase::Rally
            && self.athletes[0].flash > 0
            && self.athletes[0].shot == shot;
        if lit {
            let k = pulse(t, 2.4);
            shapes::glow_disc(
                vec2(r.x + r.w * 0.5, r.y + r.h * 0.5),
                r.w * 0.45,
                with_alpha(color, 0.08 + 0.08 * k),
            );
            shapes::fill_rounded_rect(
                r,
                12.,
                Fill::vertical(r, with_alpha(color, 0.24), with_alpha(color, 0.1)),
            );
            shapes::stroke_rounded_rect(r, 12., 1.5, with_alpha(color, 0.55 + 0.4 * k));
        } else {
            shapes::fill_rounded_rect(r, 12., Color::new(1., 1., 1., 0.035));
        }
        if swinging {
            shapes::stroke_rounded_rect(r, 12., 2., with_alpha(PAPER, 0.85));
        }
        keycap(shot_key(shot), "", 704., y + 16., PAPER);
        let glyph = if usable {
            color
        } else {
            with_alpha(MUTED, 0.45)
        };
        shot_glyph(shot, vec2(806., y + 26.), 58., glyph);
        let cost = if usable { SOFT } else { RED };
        let w = ink_right(cost_label(shot), 916., y + 32., 15., cost);
        icon(Icon::Bolt, 916. - w - 10., y + 26.5, 14., cost);
    }
    fn draw_card(&self, t: f32, hover: Vec2) {
        let accent = self.color();
        shapes::fill_rounded_rect(rect(20., 116., 646., 452.), 14., with_alpha(SPACE, 0.6));
        card(103., 168., 539., 360., Color::new(0.05, 0.075, 0.12, 0.98));
        shapes::fill_rounded_rect(rect(123., 168., 64., 3.), 1.5, accent);
        match self.phase {
            Phase::Briefing => self.draw_briefing(t, hover),
            Phase::Training => self.draw_training(t, hover),
            Phase::Setback => {
                let o = self.opponent();
                let w = label(o.name, 123., 212., 30., accent, TextAlign::Left);
                icon(Icon::Crown, 141. + w, 210., 24., GOLD);
                // The match result, as games won.
                helmet(vec2(139., 274.), 30., BLUE, 1.);
                let games = |side: usize| self.games[side].to_string();
                label(&games(0), 180., 274., 32., BLUE, TextAlign::Center);
                label(":", 208., 274., 26., MUTED, TextAlign::Center);
                label(&games(1), 236., 274., 32., accent, TextAlign::Center);
                helmet(vec2(277., 274.), 30., accent, 1.);
                // Wildcards: the heart a retry spends pulses.
                for i in 0..3_u8 {
                    let c = vec2(139. + f32::from(i) * 46., 352.);
                    if i < self.retries {
                        let next = i + 1 == self.retries;
                        let k = if next { pulse(t, 1.4) } else { 0. };
                        if next {
                            glow(c.x, c.y, 26., with_alpha(RED, 0.18 + 0.14 * k));
                        }
                        icon(Icon::Heart, c.x, c.y, 34. + 4. * k, RED);
                    } else {
                        icon(Icon::Heart, c.x, c.y, 30., with_alpha(MUTED, 0.22));
                    }
                }
                let spend = vec2(139. + f32::from(self.retries.max(1) - 1) * 46., 352.);
                icon_chip(Icon::Heart, "-1", spend.x + 20., spend.y - 36., 13., RED);
                // A retry rolls the score back to the start of this match.
                let lost = self.score.saturating_sub(self.match_score_start);
                if lost > 0 {
                    icon(Icon::Star, 133., 414., 20., with_alpha(RED, 0.9));
                    label(&format!("-{lost}"), 148., 414., 20., RED, TextAlign::Left);
                }
                // The rival, racket raised.
                let lift = bob(t, 0.6, 4.).abs();
                pool(540., 330., 84., with_alpha(accent, 0.14));
                draw_ellipse(540., 424., 34. - lift, 8., 0., Color::new(0., 0., 0., 0.4));
                astronaut(
                    vec2(534., 420. - lift),
                    accent,
                    11,
                    0.,
                    pulse(t, 0.8) > 0.4,
                    2.,
                );
                let hovered = inside(CARD_BUTTON, hover);
                button(
                    rect(180., 477., 450., 38.),
                    Icon::Restart,
                    "Retry",
                    GOLD,
                    hovered,
                    t,
                );
            }
            Phase::Champion => {
                label("Earthrise Cup", 123., 214., 30., PAPER, TextAlign::Left);
                pool(
                    372.,
                    320.,
                    90.,
                    with_alpha(GOLD, 0.2 + 0.06 * pulse(t, 0.5)),
                );
                shapes::stroke_circle(vec2(346., 312.), 12., 4., GOLD);
                shapes::stroke_circle(vec2(398., 312.), 12., 4., GOLD);
                let cup = vec2(372., 308.);
                shapes::fill_circle(
                    cup,
                    40.,
                    Fill::radial(
                        cup - vec2(12., 14.),
                        56.,
                        lighten(GOLD, 0.45),
                        darken(GOLD, 0.2),
                    ),
                );
                shapes::fill_rounded_rect(rect(360., 344., 24., 30.), 4., darken(GOLD, 0.1));
                shapes::fill_rounded_rect(rect(330., 372., 84., 14.), 5., GOLD);
                shapes::fill_circle(cup, 25., Color::new(0.2, 0.28, 0.38, 1.));
                icon(Icon::Moon, cup.x, cup.y, 30., GOLD);
                self.draw_ladder(vec2(267., 446.), t);
            }
            Phase::Eliminated => {
                label("Tour over", 123., 214., 30., PAPER, TextAlign::Left);
                helmet(vec2(145., 280.), 44., accent, 1.);
                label(
                    self.opponent().name,
                    178.,
                    280.,
                    26.,
                    accent,
                    TextAlign::Left,
                );
                for i in 0..3 {
                    icon(
                        Icon::Heart,
                        145. + i as f32 * 46.,
                        352.,
                        30.,
                        with_alpha(MUTED, 0.22),
                    );
                }
                self.draw_ladder(vec2(267., 446.), t);
            }
            _ => {}
        }
    }
    /// The six rivals in order: beaten ones with their medals, the current
    /// one glowing, later ones dim.
    fn draw_ladder(&self, at: Vec2, t: f32) {
        for (i, o) in OPPONENTS.iter().enumerate() {
            let c = at + vec2(i as f32 * 42., 0.);
            let color = Color::new(o.color[0], o.color[1], o.color[2], 1.);
            let current = i == self.match_index && self.phase != Phase::Champion;
            let medals = self.medals[i];
            if current {
                glow(
                    c.x,
                    c.y,
                    24.,
                    with_alpha(color, 0.25 + 0.15 * pulse(t, 0.8)),
                );
            }
            let a = if medals > 0 || current { 1. } else { 0.35 };
            helmet(c, if current { 32. } else { 26. }, color, a);
            for m in 0..medals {
                let x = c.x + (f32::from(m) - f32::from(medals - 1) * 0.5) * 9.;
                medal_glyph(vec2(x, c.y + 23.), 11., 1.);
            }
        }
    }
    fn draw_briefing(&self, t: f32, hover: Vec2) {
        let o = self.opponent();
        let accent = self.color();
        self.draw_ladder(vec2(141., 204.), t);
        let s = style(13., accent);
        let w = text_width(o.court, s) + 13. * 1.3;
        chip(o.court, 622. - w, 194., 13., accent);
        label(o.name, 123., 262., 34., PAPER, TextAlign::Left);
        ink(o.title, 123., 294., 15., MUTED);
        // The rival's speed and reflexes, then the shots they like.
        for (row, (glyph, value)) in [
            (Icon::Wind, o.speed / 160.),
            (Icon::Eye, 1. - f32::from(o.reaction) / 30.),
        ]
        .into_iter()
        .enumerate()
        {
            let y = 330. + row as f32 * 32.;
            icon(glyph, 134., y, 20., accent);
            bar(154., y - 4., 230., 8., value, accent);
        }
        let shots: &[Shot] = match o.style {
            3 => &[Shot::Drive, Shot::Lob],
            4 => &[Shot::Drive, Shot::Smash],
            5 => &SHOTS,
            _ => &[Shot::Drive],
        };
        for (i, &shot) in shots.iter().enumerate() {
            let x = 123. + i as f32 * 76.;
            let r = rect(x, 390., 68., 38.);
            shapes::fill_rounded_rect(r, 10., with_alpha(shot_color(shot), 0.1));
            shapes::stroke_rounded_rect(r, 10., 1., with_alpha(shot_color(shot), 0.35));
            shot_glyph(shot, vec2(x + 34., 408.), 46., shot_color(shot));
        }
        // Portrait.
        let lift = bob(t, 0.45, 5.).abs();
        pool(540., 330., 84., with_alpha(accent, 0.14));
        draw_ellipse(540., 424., 34. - lift, 8., 0., Color::new(0., 0., 0., 0.4));
        astronaut(
            vec2(534., 420. - lift),
            accent,
            0,
            0.,
            pulse(t, 0.8) > 0.4,
            2.,
        );
        let hovered = inside(CARD_BUTTON, hover);
        button(
            rect(180., 477., 450., 38.),
            Icon::Play,
            "Play",
            BLUE,
            hovered,
            t,
        );
    }
    fn draw_training(&self, t: f32, hover: Vec2) {
        icon(Icon::Trophy, 137., 199., 26., GOLD);
        label("Court won", 158., 199., 22., PAPER, TextAlign::Left);
        let medals = self.medals[self.match_index];
        for m in 0..3_u8 {
            medal_glyph(
                vec2(560. + f32::from(m) * 26., 199.),
                26.,
                if m < medals { 1. } else { 0.18 },
            );
        }
        let levels = self.training;
        for (i, name) in ["Speed", "Stamina", "Reach"].into_iter().enumerate() {
            let y = 225. + i as f32 * 83.;
            let maxed = levels[i] >= 3;
            let hovered = !maxed && inside([123., y, 622., y + 72.], hover);
            let lift = if hovered { 2. } else { 0. };
            let r = rect(123., y - lift, 499., 72.);
            if hovered {
                shapes::glow_disc(
                    vec2(r.x + r.w * 0.5, r.y + r.h * 0.5),
                    250.,
                    with_alpha(BLUE, 0.05 + 0.04 * pulse(t, 1.2)),
                );
            }
            let fade = if maxed { 0.5 } else { 1. };
            shapes::fill_rounded_rect(
                r,
                12.,
                Fill::vertical(
                    r,
                    Color::new(0.1, 0.16, 0.23, fade),
                    Color::new(0.07, 0.12, 0.18, fade),
                ),
            );
            shapes::stroke_rounded_rect(
                r,
                12.,
                if hovered { 1.5 } else { 1. },
                with_alpha(BLUE, if hovered { 0.75 } else { 0.18 }),
            );
            keycap(["1", "2", "3"][i], "", 141., r.y + 26., PAPER);
            let ink_c = if maxed { MUTED } else { PAPER };
            self.training_icon(
                i,
                vec2(200., r.y + 36.),
                32.,
                if maxed { MUTED } else { BLUE },
            );
            ink(name, 232., r.y + 31., 18., ink_c);
            // Preview: the stat now and after this level.
            let at = |level: u8| match i {
                0 => speed_at(level),
                1 => recovery_at(level),
                _ => reach_at(level),
            };
            let now = (at(levels[i]) as u32).to_string();
            let x = 232. + ink(&now, 232., r.y + 56., 15., if maxed { MUTED } else { SOFT });
            if maxed {
                chip("MAX", x + 10., r.y + 44., 11., GOLD);
            } else {
                icon(Icon::ArrowRight, x + 13., r.y + 51., 13., BLUE);
                let next = (at(levels[i] + 1) as u32).to_string();
                ink(&next, x + 24., r.y + 56., 15., lighten(BLUE, 0.2));
            }
            level_pips(546., r.y + 44., levels[i], if maxed { GOLD } else { BLUE });
        }
        // The next rival, then the skip button.
        if let Some(next) = OPPONENTS.get(self.match_index + 1) {
            let color = Color::new(next.color[0], next.color[1], next.color[2], 1.);
            icon(Icon::ArrowRight, 132., 495., 14., MUTED);
            helmet(vec2(156., 495.), 24., color, 1.);
            ink(next.name, 174., 500., 15., SOFT);
        }
        let r = region(SKIP_BUTTON);
        let hovered = inside(SKIP_BUTTON, hover);
        shapes::fill_rounded_rect(
            r,
            10.,
            if hovered {
                Color::new(1., 1., 1., 0.1)
            } else {
                Color::new(1., 1., 1., 0.04)
            },
        );
        shapes::stroke_rounded_rect(
            r,
            10.,
            1.,
            with_alpha(BLUE, if hovered { 0.6 } else { 0.2 }),
        );
        let w = keycap_width("Space") + 7. + text_width("Skip", TextStyle::modern(13., SOFT)) + 22.;
        let x = r.x + r.w * 0.5 - w * 0.5;
        let kw = keycap("Space", "Skip", x, r.y + r.h * 0.5 - 10., SOFT);
        icon(Icon::ArrowRight, x + kw + 12., r.y + r.h * 0.5, 13., SOFT);
    }
    /// Effects for one accepted event, at the place it happened.
    fn effect(&self, event: Event, fx: &mut Juice) {
        match event {
            Event::Enter => {
                let p = position(self.athletes[0].pos);
                fx.ring(p, BLUE, 46.);
                fx.sparkle(p - vec2(0., 34.), BLUE);
            }
            Event::Aim(lane) => {
                let p = position(lane_target(lane));
                fx.ring(p, BLUE, 32.);
                fx.burst(p, BLUE, 6);
            }
            Event::Swing(shot) => {
                fx.ring(self.racket_at(0), with_alpha(shot_color(shot), 0.6), 18.);
            }
            Event::Whiff => fx.burst(self.racket_at(0), DUST, 5),
            Event::Hit {
                side,
                shot,
                at,
                height,
                serve,
                sweet,
                fallback,
            } => {
                let p = aloft(at, height);
                let color = if side == 0 {
                    shot_color(shot)
                } else {
                    lighten(self.color(), 0.2)
                };
                if serve {
                    fx.ring(p, GOLD, 30.);
                    fx.burst(p, GOLD, 10);
                } else {
                    match shot {
                        Shot::Drive => {
                            fx.burst(p, color, 14);
                            fx.ring(p, color, 26.);
                        }
                        Shot::Lob => {
                            fx.burst(p, color, 10);
                            fx.ring(p, color, 36.);
                        }
                        Shot::Smash => {
                            fx.burst(p, SMASH, 28);
                            fx.ring(p, SMASH, 56.);
                            fx.shake(if side == 0 { 0.22 } else { 0.3 });
                        }
                    }
                }
                if sweet && side == 0 && !serve {
                    fx.sparkle(p, GOLD);
                }
                match fallback {
                    Some(Fallback::Tired) => {
                        fx.popup_icon(p - vec2(0., 28.), Icon::Bolt, "Low", RED);
                        fx.ring(position(self.athletes[0].pos), RED, 34.);
                    }
                    Some(Fallback::Low) => {
                        fx.popup_icon(p - vec2(0., 28.), Icon::ArrowDown, "Low", RED);
                    }
                    None => {}
                }
            }
            Event::Bounce(at) => {
                let p = position(at);
                fx.ring(p, DUST, 18.);
                fx.burst(p, DUST, 6);
            }
            Event::Dead { end, at } => {
                let p = on_court(position(at));
                match end {
                    End::Net => {
                        // Below the tape, clear of the score pill above it.
                        let p = vec2(p.x, 334.);
                        fx.popup(p + vec2(0., 46.), "NET", RED);
                        fx.ring(p, RED, 42.);
                        fx.shake(0.18);
                    }
                    End::Out => {
                        fx.popup(p - vec2(0., 24.), "OUT", RED);
                        fx.ring(p, RED, 36.);
                    }
                    End::Bounce | End::Passed => {
                        let c = self.side_color(self.point_winner);
                        fx.ring(p, c, 40.);
                        fx.burst(p, c, 12);
                    }
                }
            }
            Event::Point { winner, ace } => {
                let c = self.side_color(winner);
                let pill = vec2(
                    BUG.x + 4. + if winner == 0 { 87. } else { 153. },
                    BUG.y + 17.,
                );
                fx.ring(pill, c, 30.);
                if ace {
                    let at = vec2(350., 236.);
                    if winner == 0 {
                        fx.callout(at, "ACE", GOLD);
                        fx.sparkle(at, GOLD);
                    } else {
                        fx.popup(at, "ACE", c);
                    }
                } else if self.points == [3, 3] {
                    fx.callout(vec2(350., 236.), "DEUCE", GOLD);
                }
            }
            Event::Game(winner) => {
                let c = self.side_color(winner);
                let pip = Self::bug_game(winner, self.games[winner].saturating_sub(1));
                fx.burst(pip, c, 18);
                fx.ring(pip, c, 24.);
                if winner == 0 {
                    fx.sparkle(pip, GOLD);
                }
            }
            Event::Match { winner, medals } => {
                if winner == 0 {
                    // Beside the card's title, rising into open sky.
                    fx.callout(vec2(470., 212.), "MATCH", GOLD);
                    fx.confetti(vec2(470., 222.), 60);
                    for m in 0..medals {
                        fx.sparkle(vec2(560. + f32::from(m) * 26., 203.), GOLD);
                    }
                }
            }
            Event::Train(track) => {
                let p = vec2(200., 261. + track as f32 * 83.);
                fx.sparkle(p, BLUE);
                fx.popup_icon(p, Icon::ArrowUp, "+1", BLUE);
            }
            Event::Maxed(track) => {
                let p = vec2(372., 261. + track as f32 * 83.);
                fx.ring(p, RED, 44.);
                fx.popup(p - vec2(0., 22.), "Max", RED);
                fx.shake(0.12);
            }
            Event::Skip => {
                let r = region(SKIP_BUTTON);
                fx.ring(vec2(r.x + r.w * 0.5, r.y + r.h * 0.5), BLUE, 40.);
            }
            Event::Retry => {
                let heart = vec2(694. + f32::from(self.retries) * 21., BUG.y + 17.);
                fx.popup_icon(heart + vec2(0., 14.), Icon::Heart, "-1", RED);
                fx.ring(heart, RED, 24.);
            }
        }
    }
}
/// Right-aligned shadowless text; returns its width.
fn ink_right(value: &str, right: f32, y: f32, size: f32, color: Color) -> f32 {
    draw_text_styled(
        value,
        right,
        y,
        TextStyle::modern(size, color).with_align(TextAlign::Right),
    )
    .width
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::META;
    use crate::model::Ball;

    fn primary() -> Input {
        Input {
            primary: true,
            ..Input::default()
        }
    }
    fn click(x: f32, y: f32) -> Input {
        Input {
            click: true,
            pointer: Vec2::new(x, y),
            ..Input::default()
        }
    }
    /// A live rally with the rival's shot arriving at the player's racket.
    fn incoming() -> Game {
        let mut game = Game::new();
        game.step(&primary());
        game.step(&primary());
        game.ball = Ball {
            pos: Vec2::new(0., 80.),
            velocity: Vec2::new(0., 90.),
            height: 25.,
            vertical: -20.,
            target: Vec2::new(0., 104.),
            hitter: 1,
            bounced: false,
            flight_ticks: 60,
        };
        game.athletes[0].pos = game.ball.pos;
        game.athletes[0].stamina = 100.;
        game
    }

    #[test]
    fn progress_counts_matches_from_first_to_final() {
        let mut game = Game::new();
        assert_eq!(ArcadeGame::progress(&game), Some((1, 6)));
        for tick in 0..150_000 {
            game.step(&game.pilot(tick));
            if game.outcome() != Outcome::Playing {
                break;
            }
        }
        assert_eq!(game.outcome(), Outcome::Won);
        assert_eq!(ArcadeGame::progress(&game), Some((6, 6)));
        let hud = ArcadeGame::hud(&game);
        assert_eq!(hud.stage, "Earthrise Center");
        assert!(hud.objective.split_whitespace().count() <= 5);
        assert!(
            game.summary().matches(". ").count() == 0,
            "{}",
            game.summary()
        );
    }

    #[test]
    fn a_return_bursts_at_the_racket_and_a_smash_shakes() {
        let mut game = incoming();
        assert_eq!(game.step(&primary()), Cue::Good);
        let mut fx = Juice::new();
        game.effects(Cue::Good, &mut fx);
        let counts = fx.counts();
        assert!(counts.sparks >= 14 && counts.rings >= 2, "{counts:?}");

        let mut smash = incoming();
        smash.step(&Input {
            tertiary: true,
            ..Input::default()
        });
        assert!(smash.feed.events().iter().any(|e| matches!(
            e,
            Event::Hit {
                side: 0,
                shot: Shot::Smash,
                ..
            }
        )));
        let mut fx = Juice::new();
        smash.effects(Cue::Good, &mut fx);
        fx.update(1. / 60.);
        assert_ne!(fx.shake_offset(), Vec2::ZERO);
    }

    #[test]
    fn a_tired_smash_reports_the_fallback_with_a_popup() {
        let mut game = incoming();
        game.athletes[0].stamina = 20.;
        game.step(&Input {
            tertiary: true,
            ..Input::default()
        });
        assert!(game.feed.events().iter().any(|e| matches!(
            e,
            Event::Hit {
                shot: Shot::Drive,
                fallback: Some(Fallback::Tired),
                ..
            }
        )));
        let mut fx = Juice::new();
        game.effects(Cue::Good, &mut fx);
        assert!(fx.counts().popups >= 1);
    }

    #[test]
    fn events_last_one_step_and_refusals_are_felt() {
        let mut game = incoming();
        game.step(&primary());
        assert!(!game.feed.events().is_empty());
        game.step(&Input::default());
        assert!(
            !game
                .feed
                .events()
                .iter()
                .any(|e| matches!(e, Event::Hit { side: 0, .. }))
        );
        // A maxed training track refuses the choice with a red ring and shake.
        let mut training = Game::new();
        training.phase = Phase::Training;
        training.training[0] = 3;
        let before = training.clone();
        assert_eq!(
            training.step(&Input {
                slot: Some(1),
                ..Input::default()
            }),
            Cue::None
        );
        assert_eq!(training, before);
        assert_eq!(training.feed.events(), &[Event::Maxed(0)]);
        let mut fx = Juice::new();
        training.effects(Cue::None, &mut fx);
        fx.update(1. / 60.);
        assert!(fx.counts().rings == 1 && fx.counts().popups == 1);
        assert_ne!(fx.shake_offset(), Vec2::ZERO);
    }

    #[test]
    fn clicking_a_far_court_third_aims_like_its_number_key() {
        let mut game = Game::new();
        game.step(&primary());
        assert_eq!(game.phase, Phase::Serve);
        for (lane, x) in [(0, 90.), (2, 600.), (1, 350.)] {
            let mut keyed = game.clone();
            keyed.step(&Input {
                slot: Some(lane + 1),
                ..Input::default()
            });
            game.step(&click(x, 260.));
            assert_eq!(game.aim, lane);
            assert_eq!(game.aim, keyed.aim);
        }
        assert_eq!(game.feed.events(), &[Event::Aim(1)]);
        // Clicks outside the rival's half, or repeats, change nothing.
        game.step(&click(350., 420.));
        game.step(&click(350., 260.));
        assert_eq!(game.aim, 1);
        assert!(game.feed.events().is_empty());
        let mut fx = Juice::new();
        game.step(&click(600., 300.));
        game.effects(Cue::None, &mut fx);
        assert_eq!(fx.counts().rings, 1);
    }

    #[test]
    fn card_buttons_mirror_space_for_retry_and_skip() {
        let mut lost = Game::new();
        lost.phase = Phase::Setback;
        lost.score = 900;
        lost.match_score_start = 400;
        let mut keyed = lost.clone();
        assert_eq!(lost.step(&click(100., 495.)), Cue::None);
        assert_eq!(lost.phase, Phase::Setback);
        assert_eq!(lost.step(&click(405., 495.)), Cue::Action);
        assert_eq!(keyed.step(&primary()), Cue::Action);
        assert_eq!(lost, keyed);
        assert_eq!(
            (lost.retries, lost.score, lost.phase),
            (2, 400, Phase::Briefing)
        );
        assert_eq!(lost.feed.events(), &[Event::Retry]);

        let mut won = Game::new();
        won.phase = Phase::Training;
        let mut keyed = won.clone();
        assert_eq!(won.step(&click(537., 495.)), Cue::Action);
        keyed.step(&primary());
        assert_eq!(won, keyed);
        assert_eq!((won.match_number(), won.training), (2, [0; 3]));
    }

    #[test]
    fn ace_counts_an_untouched_serve_won_by_the_player() {
        let mut game = Game::new();
        game.step(&primary());
        game.step(&Input {
            primary: true,
            slot: Some(1),
            ..Input::default()
        });
        assert_eq!(game.rally, 1);
        // Hold the rival still so the serve can never be returned.
        let mut cue = Cue::None;
        for _ in 0..600 {
            game.athletes[1].pos = Vec2::new(175., -22.);
            game.ai_delay = 99;
            cue = game.step(&Input::default());
            if game.phase == Phase::Point {
                break;
            }
        }
        assert_eq!(cue, Cue::Good);
        assert_eq!((game.points, game.aces, game.points_won), ([1, 0], 1, 1));
        assert!(game.feed.events().contains(&Event::Point {
            winner: 0,
            ace: true
        }));
        let mut fx = Juice::new();
        game.effects(cue, &mut fx);
        assert!(fx.counts().popups >= 1);
    }

    #[test]
    fn cost_labels_match_the_rules_and_help_stays_short() {
        for shot in SHOTS {
            assert_eq!(cost_label(shot), format!("-{}", shot.cost()));
        }
        assert!(META.help.len() <= 4);
        for line in META.help {
            assert!(line.split_whitespace().count() < 15, "{line}");
        }
    }
}
