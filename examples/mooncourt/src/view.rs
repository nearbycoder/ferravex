use crate::{Game, Phase, model::Athlete};
use ferravex_arcade::shapes::{self, Fill, Stroke};
use ferravex_arcade::typography::{TextAlign, TextStyle, draw_text_styled, text_width};
use ferravex_arcade::*;
const BLUE: Color = Color::new(0.46, 0.78, 1., 1.);
const GOLD: Color = Color::new(1., 0.84, 0.43, 1.);
const LINE: Color = Color::new(0.57, 0.69, 0.76, 1.);
const COURT: Color = Color::new(0.13, 0.20, 0.28, 1.);
const SPACE: Color = Color::new(0.03, 0.045, 0.085, 1.);
const PANEL: Color = Color::new(0.06, 0.085, 0.13, 1.);
const SOFT: Color = Color::new(0.84, 0.88, 0.93, 1.);
const SUIT: Color = Color::new(0.9, 0.93, 0.96, 1.);
const VISOR: Color = Color::new(0.05, 0.11, 0.18, 1.);
fn position(p: Vec2) -> Vec2 {
    vec2(350. + p.x * 1.57, 343. + p.y * 1.18)
}
fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::new(x, y, w, h)
}
/// Shadowless modern text for clean surfaces; returns the drawn width.
fn ink(value: &str, x: f32, y: f32, size: f32, color: Color) -> f32 {
    draw_text_styled(value, x, y, TextStyle::modern(size, color)).width
}
fn ink_at(value: &str, x: f32, y: f32, size: f32, color: Color, align: TextAlign) -> f32 {
    draw_text_styled(
        value,
        x,
        y,
        TextStyle::modern(size, color).with_align(align),
    )
    .width
}
/// Shadowless text shrunk to `width` when needed.
fn ink_fit(value: &str, x: f32, y: f32, width: f32, size: f32, color: Color) {
    let s = TextStyle::modern(size, color);
    let w = text_width(value, s);
    let s = if w > width && w > 0. {
        s.with_size(size * width / w)
    } else {
        s
    };
    draw_text_styled(value, x, y, s);
}
fn eyebrow(value: &str, x: f32, y: f32, color: Color) {
    draw_text_styled(
        value,
        x,
        y,
        TextStyle::modern(11., color).with_letter_spacing(1.5),
    );
}
/// Filled pill label without the host chip's outline pass; returns its width.
fn tag(label: &str, x: f32, y: f32, size: f32, color: Color) -> f32 {
    let s = TextStyle::modern(size, color);
    let w = text_width(label, s) + size * 1.3;
    let h = size * 1.45;
    shapes::fill_rounded_rect(rect(x, y, w, h), h * 0.5, with_alpha(color, 0.16));
    draw_text_styled(label, x + size * 0.65, s.centered_baseline(y + h * 0.5), s);
    w
}
fn tag_width(label: &str, size: f32) -> f32 {
    text_width(label, TextStyle::modern(size, PAPER)) + size * 1.3
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
fn pips(x: f32, y: f32, filled: u8, color: Color) {
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
/// An astronaut at screen point `p` (feet). `flash` animates the racket swing.
fn astronaut(p: Vec2, c: Color, flash: u16, step: f32, tick: u64) {
    let swing = f32::from(flash) / 14.;
    // Backpack, legs and boots.
    panel(p.x - 12., p.y - 23., 5., 14., darken(c, 0.45));
    draw_line(p.x - 4., p.y - 8., p.x - 6. - step, p.y - 1., 4., SUIT);
    draw_line(p.x + 4., p.y - 8., p.x + 6. + step, p.y - 1., 4., SUIT);
    panel(p.x - 9. - step, p.y - 2., 6., 3., darken(c, 0.35));
    panel(p.x + 3. + step, p.y - 2., 6., 3., darken(c, 0.35));
    // Suit torso with a lit chest plate.
    let body = rect(p.x - 9., p.y - 25., 18., 18.);
    shapes::fill_rounded_rect(
        body,
        5.,
        Fill::vertical(body, lighten(c, 0.2), darken(c, 0.2)),
    );
    panel(p.x - 5., p.y - 20., 10., 7., with_alpha(SUIT, 0.9));
    panel(p.x - 3., p.y - 18., 2., 2., c);
    // Helmet, visor and antenna beacon.
    let head = vec2(p.x, p.y - 32.);
    shapes::fill_circle(
        head,
        10.5,
        Fill::radial(
            head - vec2(3., 4.),
            15.,
            PAPER,
            Color::new(0.68, 0.74, 0.8, 1.),
        ),
    );
    draw_ellipse(p.x + 1., p.y - 32., 7.5, 5., 0., VISOR);
    draw_ellipse(
        p.x - 2.,
        p.y - 34.,
        2.5,
        1.4,
        -20.,
        Color::new(1., 1., 1., 0.55),
    );
    draw_line(p.x + 6., p.y - 40., p.x + 9., p.y - 47., 1.2, LINE);
    let blink = if (tick / 20).is_multiple_of(3) {
        1.
    } else {
        0.55
    };
    draw_poly(p.x + 9., p.y - 47., 8, 2., 0., with_alpha(c, blink));
    // Racket arm: raised through the swing, resting otherwise.
    let racket = vec2(p.x + 18. + 7. * swing, p.y - 9. - 18. * swing);
    draw_line(p.x + 7., p.y - 20., racket.x, racket.y, 3., SUIT);
    let head_c = racket + vec2(6., -5.);
    if flash > 0 {
        shapes::stroke_arc(
            vec2(p.x + 4., p.y - 18.),
            26.,
            -2.6 + 1.6 * (1. - swing),
            1.5,
            Stroke::new(3., with_alpha(GOLD, 0.45 * swing)),
        );
    }
    let rim = if flash > 0 { GOLD } else { c };
    shapes::stroke_circle(head_c, 7.5, 2., rim);
    draw_line(
        head_c.x - 6.,
        head_c.y,
        head_c.x + 6.,
        head_c.y,
        0.7,
        with_alpha(LINE, 0.7),
    );
    draw_line(
        head_c.x,
        head_c.y - 6.,
        head_c.x,
        head_c.y + 6.,
        0.7,
        with_alpha(LINE, 0.7),
    );
}
impl ArcadeGame for Game {
    fn new() -> Self {
        Self::new()
    }
    fn update(&mut self, input: &Input) -> Cue {
        self.step(input)
    }
    fn draw(&self) {
        self.draw_court();
        self.draw_score();
        match self.phase {
            Phase::Briefing
            | Phase::Training
            | Phase::Setback
            | Phase::Champion
            | Phase::Eliminated => self.draw_card(),
            _ => {}
        }
    }
    fn hud(&self) -> Hud {
        Hud {
            stage: format!("Match {}/6 - {}", self.match_number(), self.opponent().name),
            objective: "Win two games.".into(),
            status: format!(
                "{} matches won | {} wildcards | longest rally {}",
                self.medals.iter().filter(|&&v| v > 0).count(),
                self.retries,
                self.best_rally
            ),
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
}
impl Game {
    fn color(&self) -> Color {
        let [r, g, b] = self.opponent().color;
        Color::new(r, g, b, 1.)
    }
    fn draw_backdrop(&self) {
        let area = rect(20., 116., 646., 452.);
        shapes::fill_rounded_rect(
            area,
            14.,
            Fill::vertical(area, SPACE, Color::new(0.06, 0.08, 0.13, 1.)),
        );
        // Stars twinkle on the match's own tick counter.
        for i in 0..42_u64 {
            let x = 28. + ((i * 137 + 31) % 626) as f32;
            let y = 124. + ((i * 79 + 17) % 429) as f32;
            let size = if i % 7 == 0 { 2. } else { 1.2 };
            let wave = ((self.tick / 6 + i * 13) % 40) as f32 / 40.;
            let a = 0.35 + 0.45 * (1. - (wave * 2. - 1.).abs());
            panel(x, y, size, size, with_alpha(LINE, a));
        }
        // Earth, lit from the left, with a thin atmosphere.
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
        draw_ellipse(603., 141., 9., 3., -10., Color::new(0.92, 0.95, 0.97, 0.7));
        shapes::stroke_circle(earth, 26.5, 1.2, Color::new(0.6, 0.85, 1., 0.45));
        // Card phases carry their own heading; keep the backdrop quiet.
        if matches!(self.phase, Phase::Serve | Phase::Rally | Phase::Point) {
            ink_fit(self.opponent().court, 40., 146., 420., 21., self.color());
            ink_fit(self.opponent().title, 40., 166., 420., 13., MUTED);
        }
    }
    fn draw_court(&self) {
        self.draw_backdrop();
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
        // The aimed third of the far court.
        if matches!(self.phase, Phase::Serve | Phase::Rally) {
            let x = 67.4 + self.aim as f32 * 188.4;
            panel(x, 195.5, 188.4, 147.5, with_alpha(BLUE, 0.07));
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
        // Net: shadow, mesh, tape and posts.
        panel(50., 343., 600., 8., Color::new(0., 0., 0., 0.22));
        panel(50., 333.4, 600., 9.6, Color::new(0.04, 0.06, 0.1, 0.45));
        for i in 0..20 {
            let x = 56. + i as f32 * 30.;
            draw_line(x, 343., x, 333.4, 0.8, with_alpha(LINE, 0.45));
        }
        draw_line(50., 338.2, 650., 338.2, 0.7, with_alpha(LINE, 0.45));
        shapes::fill_rounded_rect(rect(50., 332., 600., 3.), 1.5, PAPER);
        for x in [50., 650.] {
            shapes::fill_capsule(
                vec2(x, 330.),
                vec2(x, 345.),
                3.,
                Color::new(0.75, 0.8, 0.86, 1.),
            );
        }
        let pulse = (self.tick as f32 * 0.2).sin();
        if self.phase == Phase::Rally {
            let target = position(self.ball.target);
            let color = if self.ball.hitter == 1 {
                BLUE
            } else {
                self.color()
            };
            draw_ellipse(target.x, target.y, 15., 8., 0., with_alpha(color, 0.18));
            shapes::stroke_circle(target, 12. + 2. * pulse, 1.6, with_alpha(color, 0.9));
            draw_line(target.x - 18., target.y, target.x - 8., target.y, 1., color);
            draw_line(target.x + 8., target.y, target.x + 18., target.y, 1., color);
        }
        let ready = self.phase == Phase::Rally && self.can_hit(0);
        if ready {
            let p = position(self.athletes[0].pos);
            pool(p.x, p.y - 14., 34., with_alpha(GOLD, 0.22));
            shapes::stroke_circle(p, 19. + 1.5 * pulse, 2., GOLD);
        }
        self.draw_athlete(self.athletes[1], self.color(), false);
        self.draw_athlete(self.athletes[0], BLUE, true);
        if ready {
            let p = position(self.athletes[0].pos);
            let w = tag_width("Swing!", 12.);
            shapes::fill_rounded_rect(rect(p.x - w * 0.5, p.y - 76., w, 17.4), 8.7, SPACE);
            tag("Swing!", p.x - w * 0.5, p.y - 76., 12., GOLD);
        }
        if matches!(self.phase, Phase::Serve | Phase::Rally | Phase::Point) {
            self.draw_ball();
        }
        self.draw_aim();
        if self.phase == Phase::Point {
            let won = self.point_winner == 0;
            let color = if won { BLUE } else { self.color() };
            let r = rect(200., 268., 300., 86.);
            shapes::fill_rounded_rect(
                rect(r.x, r.y + 5., r.w, r.h),
                14.,
                Color::new(0., 0., 0., 0.4),
            );
            slab(r.x, r.y, r.w, r.h, Color::new(0.05, 0.07, 0.11, 0.97));
            shapes::fill_rounded_rect(rect(r.x + 24., r.y, 56., 3.), 1.5, color);
            eyebrow("POINT", r.x + 24., r.y + 26., MUTED);
            ink(
                if won { "Your point" } else { "Rival's point" },
                r.x + 24.,
                r.y + 60.,
                26.,
                color,
            );
            ink_at(
                &format!("{} - {}", self.point_text(0), self.point_text(1)),
                r.x + r.w - 24.,
                r.y + 60.,
                22.,
                PAPER,
                TextAlign::Right,
            );
        }
        if self.phase == Phase::Serve {
            let r = rect(230., 282., 240., 38.);
            shapes::fill_rounded_rect(r, 19., Color::new(0.03, 0.05, 0.08, 0.85));
            shapes::stroke_rounded_rect(r, 19., 1., with_alpha(GOLD, 0.35));
            if self.server == 0 {
                let label = "Serve";
                let s = TextStyle::modern(17., PAPER);
                let kw = keycap_width("Space");
                let total = kw + 10. + text_width(label, s);
                let x = 350. - total * 0.5;
                keycap("Space", "", x, 291., PAPER);
                draw_text_styled(label, x + kw + 10., s.centered_baseline(301.), s);
            } else {
                let s = TextStyle::modern(17., GOLD).with_align(TextAlign::Center);
                draw_text_styled("Rival serves...", 350., s.centered_baseline(301.), s);
            }
        }
    }
    fn draw_ball(&self) {
        let ground = position(self.ball.pos);
        let height = self.ball.height * 0.8;
        let lift = (self.ball.height / 60.).clamp(0., 1.);
        draw_ellipse(
            ground.x,
            ground.y,
            7. - 3. * lift,
            3. - 1. * lift,
            0.,
            Color::new(0.01, 0.02, 0.04, 0.75 - 0.4 * lift),
        );
        draw_line(
            ground.x,
            ground.y,
            ground.x,
            ground.y - height,
            1.,
            Color::new(0.65, 0.75, 0.85, 0.2),
        );
        let ball = vec2(ground.x, ground.y - height);
        if self.phase == Phase::Rally {
            let trail_color = if self.ball.hitter == 0 {
                BLUE
            } else {
                self.color()
            };
            let step = self.ball.velocity * TICK_DT * 1.6;
            let mut prev = ball;
            for i in 1..=5 {
                let back = position(self.ball.pos - step * i as f32);
                let point = vec2(back.x, back.y - height);
                let fade = 1. - i as f32 / 6.;
                draw_line(
                    prev.x,
                    prev.y,
                    point.x,
                    point.y,
                    5. * fade,
                    with_alpha(lighten(trail_color, 0.3), 0.45 * fade),
                );
                prev = point;
            }
        }
        pool(ball.x, ball.y, 15., with_alpha(GOLD, 0.35));
        shapes::fill_circle(
            ball,
            5.5,
            Fill::radial(ball - vec2(2., 2.), 8., Color::new(1., 0.97, 0.8, 1.), GOLD),
        );
    }
    /// Segmented aim control over the model's click strip (x 120..591, y 520..558).
    fn draw_aim(&self) {
        eyebrow("AIM", 44., 543., MUTED);
        let track = rect(116., 517., 479., 38.);
        shapes::fill_rounded_rect(track, 12., Color::new(0.02, 0.03, 0.06, 0.75));
        shapes::stroke_rounded_rect(track, 12., 1., Color::new(1., 1., 1., 0.06));
        for (i, name) in ["Left", "Center", "Right"].into_iter().enumerate() {
            let x = 120. + i as f32 * 157.;
            let selected = i == self.aim;
            let r = rect(x, 520., 149., 32.);
            if selected {
                shapes::fill_rounded_rect(
                    r,
                    10.,
                    Fill::vertical(r, lighten(BLUE, 0.15), darken(BLUE, 0.15)),
                );
            }
            let digit = ["1", "2", "3"][i];
            let badge = rect(x + 12., 526., 20., 20.);
            shapes::fill_rounded_rect(
                badge,
                5.,
                if selected {
                    Color::new(0.02, 0.06, 0.12, 0.35)
                } else {
                    Color::new(1., 1., 1., 0.07)
                },
            );
            ink_at(
                digit,
                x + 22.,
                541.,
                13.,
                if selected { INK } else { MUTED },
                TextAlign::Center,
            );
            ink(name, x + 42., 542., 16., if selected { INK } else { SOFT });
        }
    }
    fn draw_athlete(&self, a: Athlete, c: Color, near: bool) {
        let p = position(a.pos);
        let moving = a.velocity.length_squared() > 100.;
        let step = (self.tick as f32 * 0.24).sin() * if moving { 3. } else { 0. };
        let bob = if moving {
            0.
        } else {
            (self.tick as f32 * 0.06).sin().abs() * 1.2
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
        astronaut(vec2(p.x, p.y - bob), c, a.flash, step, self.tick);
        if near {
            draw_triangle(
                vec2(p.x - 5., p.y + 13.),
                vec2(p.x + 5., p.y + 13.),
                vec2(p.x, p.y + 8.),
                BLUE,
            );
        }
    }
    fn draw_score(&self) {
        let x = 700.;
        let right = 916.;
        slab(680., 120., 256., 440., PANEL);
        eyebrow("MATCH", x, 146., MUTED);
        let label = "First to 2 games";
        tag(label, right - tag_width(label, 11.), 135., 11., MUTED);
        let s = TextStyle::modern(10., MUTED)
            .with_letter_spacing(1.2)
            .with_align(TextAlign::Center);
        draw_text_styled("GAMES", 846., 170., s);
        draw_text_styled("PTS", 896., 170., s);
        for side in 0..2 {
            let y = 178. + side as f32 * 46.;
            let color = if side == 0 { BLUE } else { self.color() };
            let leading = self.games[side] > self.games[1 - side];
            let r = rect(x - 6., y, 228., 40.);
            shapes::fill_rounded_rect(r, 9., with_alpha(color, if leading { 0.12 } else { 0.06 }));
            shapes::fill_rounded_rect(rect(x - 6., y + 8., 3., 24.), 1.5, color);
            let name = if side == 0 {
                "You"
            } else {
                self.opponent().name
            };
            ink_fit(name, x + 6., y + 26., 100., 16., PAPER);
            if self.server == side && matches!(self.phase, Phase::Serve | Phase::Rally) {
                let w = text_width(name, TextStyle::modern(16., PAPER)).min(100.);
                draw_poly(x + 16. + w, y + 20., 12, 4., 0., GOLD);
            }
            ink_at(
                &self.games[side].to_string(),
                846.,
                y + 30.,
                26.,
                color,
                TextAlign::Center,
            );
            ink_at(
                self.point_text(side),
                896.,
                y + 28.,
                20.,
                PAPER,
                TextAlign::Center,
            );
        }
        if self.points[0] >= 3 && self.points[1] >= 3 {
            tag("Deuce: win by two", x, 270., 11., GOLD);
        }
        divider(694., 294., 236., Color::new(1., 1., 1., 0.12));
        eyebrow("STAMINA", x, 316., MUTED);
        for (i, (name, stamina, color)) in [
            ("You", self.athletes[0].stamina, BLUE),
            (self.opponent().name, self.athletes[1].stamina, self.color()),
        ]
        .into_iter()
        .enumerate()
        {
            let y = 336. + i as f32 * 30.;
            let tired = i == 0 && stamina < 12.;
            ink_fit(name, x, y, 120., 12., SOFT);
            ink_at(
                &format!("{}", stamina.round() as u32),
                right,
                y,
                12.,
                if tired { GOLD } else { MUTED },
                TextAlign::Right,
            );
            bar(
                x,
                y + 6.,
                216.,
                if i == 0 { 8. } else { 6. },
                stamina / 100.,
                if tired { GOLD } else { color },
            );
        }
        divider(694., 388., 236., Color::new(1., 1., 1., 0.12));
        eyebrow("RALLY", x, 410., MUTED);
        ink(&self.rally.to_string(), x, 444., 30., GOLD);
        let ready = self.phase == Phase::Rally && self.can_hit(0);
        let (state, color) = if ready {
            ("Swing now!", GOLD)
        } else if self.perfect {
            ("Sweet-spot height", BLUE)
        } else {
            ("Watch the shadow", MUTED)
        };
        tag(state, right - tag_width(state, 12.), 426., 12., color);
        wrapped(self.note, x, 468., 216., 12., SOFT);
        divider(694., 512., 236., Color::new(1., 1., 1., 0.12));
        for (i, name) in ["Speed", "Recovery", "Reach"].into_iter().enumerate() {
            let px = x + i as f32 * 74.;
            ink(name, px, 532., 11., MUTED);
            pips(px, 540., self.training[i], BLUE);
        }
    }
    fn draw_card(&self) {
        let accent = self.color();
        shapes::fill_rounded_rect(rect(20., 116., 646., 452.), 14., with_alpha(SPACE, 0.6));
        card(103., 168., 539., 360., Color::new(0.05, 0.075, 0.12, 0.98));
        shapes::fill_rounded_rect(rect(123., 168., 64., 3.), 1.5, accent);
        match self.phase {
            Phase::Briefing => {
                eyebrow(
                    &format!("COURT {} OF 6", self.match_number()),
                    123.,
                    198.,
                    accent,
                );
                let court = self.opponent().court;
                tag(court, 622. - tag_width(court, 12.), 186., 12., accent);
                ink_fit(self.opponent().name, 123., 236., 330., 32., PAPER);
                ink(self.opponent().title, 123., 258., 15., MUTED);
                // Rival portrait.
                pool(548., 306., 50., with_alpha(accent, 0.14));
                draw_ellipse(548., 334., 22., 6., 0., Color::new(0., 0., 0., 0.4));
                astronaut(vec2(548., 332.), accent, 0, 0., self.tick);
                let y = wrapped(self.opponent().advice, 123., 296., 360., 17., PAPER) + 4.;
                wrapped("Win two games to advance.", 123., y, 360., 15., MUTED);
                let opponent = self.opponent();
                for (i, (label, value)) in [
                    ("Footspeed", opponent.speed / 160.),
                    ("Reflexes", 1. - f32::from(opponent.reaction) / 30.),
                ]
                .into_iter()
                .enumerate()
                {
                    let px = 123. + i as f32 * 250.;
                    eyebrow(&label.to_uppercase(), px, 404., MUTED);
                    bar(px, 414., 230., 8., value, accent);
                }
                let r = rect(180., 476., 450., 38.);
                shapes::fill_rounded_rect(rect(r.x, r.y + 3., r.w, r.h), 11., darken(BLUE, 0.6));
                shapes::fill_rounded_rect(
                    r,
                    11.,
                    Fill::vertical(r, lighten(BLUE, 0.2), darken(BLUE, 0.1)),
                );
                let s = TextStyle::modern(18., INK);
                let label = "Enter court";
                let kw = keycap_width("Space");
                let total = text_width(label, s) + 12. + kw;
                let lx = 405. - total * 0.5;
                draw_text_styled(label, lx, s.centered_baseline(495.), s);
                keycap("Space", "", lx + text_width(label, s) + 12., 485., PAPER);
            }
            Phase::Training => {
                eyebrow("COURT WON", 123., 194., GOLD);
                ink("Choose your training", 123., 216., 20., PAPER);
                for (i, (name, detail)) in [
                    ("Footwork", "+15 movement speed"),
                    ("Endurance", "+5 stamina recovered per second"),
                    ("Racket", "+4 units of racket reach"),
                ]
                .iter()
                .enumerate()
                {
                    let y = 225. + i as f32 * 83.;
                    let maxed = self.training[i] >= 3;
                    let r = rect(123., y, 499., 72.);
                    shapes::fill_rounded_rect(
                        r,
                        12.,
                        Fill::vertical(
                            r,
                            Color::new(0.1, 0.16, 0.23, if maxed { 0.5 } else { 1. }),
                            Color::new(0.07, 0.12, 0.18, if maxed { 0.5 } else { 1. }),
                        ),
                    );
                    shapes::stroke_rounded_rect(r, 12., 1., with_alpha(BLUE, 0.18));
                    keycap(["1", "2", "3"][i], "", 141., y + 26., PAPER);
                    let ink_c = if maxed { MUTED } else { PAPER };
                    ink(name, 175., y + 32., 18., ink_c);
                    ink_fit(
                        detail,
                        175.,
                        y + 54.,
                        330.,
                        14.,
                        if maxed { MUTED } else { SOFT },
                    );
                    if maxed {
                        tag("Maxed", 604. - tag_width("Maxed", 11.), y + 14., 11., GOLD);
                    } else {
                        ink_at(
                            &format!("Level {}/3", self.training[i]),
                            604.,
                            y + 30.,
                            12.,
                            MUTED,
                            TextAlign::Right,
                        );
                    }
                    pips(546., y + 44., self.training[i], BLUE);
                }
                keycap("Space", "Skip and continue", 123., 486., MUTED);
            }
            Phase::Setback => {
                eyebrow("MATCH LOST", 123., 198., accent);
                ink_fit(
                    &format!("{} takes the match", self.opponent().name),
                    123.,
                    236.,
                    499.,
                    28.,
                    PAPER,
                );
                wrapped(
                    "Retry resets this match and restores stamina. Points from the lost match are not kept.",
                    123.,
                    276.,
                    480.,
                    17.,
                    SOFT,
                );
                eyebrow("WILDCARDS LEFT", 123., 400., MUTED);
                for i in 0..3_u8 {
                    let r = rect(123. + f32::from(i) * 34., 412., 26., 34.);
                    if i < self.retries {
                        shapes::fill_rounded_rect(
                            r,
                            5.,
                            Fill::vertical(r, lighten(GOLD, 0.3), GOLD),
                        );
                        ink_at("W", r.x + 13., r.y + 23., 14., INK, TextAlign::Center);
                    } else {
                        shapes::stroke_rounded_rect(r, 5., 1., with_alpha(MUTED, 0.5));
                    }
                }
                keycap("Space", "Use a wildcard", 123., 486., PAPER);
            }
            Phase::Champion => {
                eyebrow("TOURNAMENT COMPLETE", 123., 198., GOLD);
                ink("The Earthrise Cup", 123., 236., 28., PAPER);
                pool(372., 340., 90., with_alpha(GOLD, 0.2));
                shapes::stroke_circle(vec2(346., 318.), 12., 4., GOLD);
                shapes::stroke_circle(vec2(398., 318.), 12., 4., GOLD);
                let cup = vec2(372., 314.);
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
                shapes::fill_rounded_rect(rect(360., 350., 24., 30.), 4., darken(GOLD, 0.1));
                shapes::fill_rounded_rect(rect(330., 378., 84., 14.), 5., GOLD);
                shapes::fill_circle(cup, 25., Color::new(0.2, 0.28, 0.38, 1.));
                ink_at("MC", 372., 322., 22., GOLD, TextAlign::Center);
                ink("Six courts. One champion.", 123., 440., 19., BLUE);
            }
            Phase::Eliminated => {
                eyebrow("TOUR COMPLETE", 123., 198., accent);
                ink("The tour ends here", 123., 236., 28., PAPER);
                wrapped("No wildcards remaining.", 123., 276., 480., 17., SOFT);
            }
            _ => {}
        }
    }
}
fn keycap_width(key: &str) -> f32 {
    (text_width(key, TextStyle::modern(13., PAPER)) + 12.).max(20.)
}
