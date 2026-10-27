use crate::game::{
    BALL_RADIUS, Feedback, Game, HEIGHT, LEFT, PADDLE_Y, Phase, Pickup, RIGHT, TOP, WIDTH,
    panel_color,
};
use crate::juice::{HOT, LIME, VIOLET};
use crate::presentation::Presentation;
use crate::quips;
use ferravex_engine::{
    graphics::{Color, Framebuffer},
    play::animation::{LoopMode, SpriteClip, SpriteFrame},
};

const INK: [u8; 4] = [9, 18, 25, 255];
const LINE: [u8; 4] = [42, 65, 72, 255];
const PAPER: [u8; 4] = [225, 234, 218, 255];
const MUTED: [u8; 4] = [120, 148, 151, 255];
const CYAN: [u8; 4] = [99, 215, 196, 255];
const GOLD: [u8; 4] = [216, 173, 109, 255];
const CORAL: [u8; 4] = [236, 132, 112, 255];
const CARD: [u8; 4] = [6, 15, 21, 240];

fn alpha(color: [u8; 4], amount: f32) -> [u8; 4] {
    [
        color[0],
        color[1],
        color[2],
        (f32::from(color[3]) * amount.clamp(0., 1.)) as u8,
    ]
}

pub struct Canvas {
    pub frame: Framebuffer,
    pulse: SpriteClip,
    look: ferravex_engine::graphics::Look,
    /// Camera kick applied to the playfield layer only; the HUD stays put.
    shift: [i32; 2],
}
impl Default for Canvas {
    fn default() -> Self {
        Self::new()
    }
}
impl Canvas {
    pub fn new() -> Self {
        Self {
            look: Default::default(),
            frame: Framebuffer::new(WIDTH, HEIGHT).expect("fixed canvas"),
            pulse: SpriteClip::new(
                (0..4)
                    .map(|index| SpriteFrame {
                        index,
                        duration: 0.12,
                    })
                    .collect(),
            )
            .expect("valid clip"),
            shift: [0; 2],
        }
    }
    pub fn render(&mut self, g: &Game) {
        self.render_session(g, false, &Presentation::default());
    }
    pub(crate) fn render_session(&mut self, g: &Game, paused: bool, presentation: &Presentation) {
        let juice = &presentation.juice;
        self.look = ferravex_engine::render::look();
        self.shift = [0; 2];
        self.frame.clear(self.look.shade(Color::from_hex(0x091219)));
        self.shift = juice.shake();
        self.draw_field(g);
        self.draw_panels(g, juice);
        self.draw_satellite(g, juice);
        self.draw_drops(g);
        self.draw_trail(g);
        for p in g.particles.iter() {
            let mut color = p.color.map(|n| (n.clamp(0.0, 1.0) * 255.0) as u8);
            color[3] = (f32::from(color[3]) * g.policy.intensity()) as u8;
            self.rect(
                p.position[0] as i32,
                p.position[1] as i32,
                p.size.ceil() as i32,
                p.size.ceil() as i32,
                color,
            );
        }
        let live = !matches!(g.phase, Phase::Won | Phase::Lost);
        for ring in juice.rings.iter().flatten().filter(|_| live) {
            let t = ring.age / ring.life;
            let fade = (1. - t) * juice.motion();
            self.ring(
                ring.at[0],
                ring.at[1],
                4. + ring.radius * (1. - (1. - t).powi(3)),
                2.,
                alpha(ring.color, 0.8 * fade),
            );
        }
        self.draw_paddle(g, presentation);
        self.draw_balls(g);
        for pop in juice.pops.iter().flatten().filter(|_| live) {
            let t = pop.age / pop.life;
            let rise = 26. * (1. - (1. - t).powi(2)) * if juice.motion() > 0. { 1. } else { 0. };
            let fade = ((1. - t) / 0.35).clamp(0., 1.);
            let text = pop.text.as_str();
            let x = pop.at[0] as i32 - text_width(text, pop.scale) / 2;
            let y = (pop.at[1] - 10. - rise) as i32;
            self.text(text, x + 1, y + 1, pop.scale, alpha(INK, fade));
            self.text(text, x, y, pop.scale, alpha(pop.color, fade));
        }
        self.shift = [0; 2];
        if juice.vignette > 0. {
            let a = juice.vignette * juice.flash();
            for i in 0..10 {
                let edge = alpha(CORAL, a * (1. - i as f32 / 10.) * 0.35);
                self.rect(LEFT as i32 + i, TOP as i32, 1, 548, edge);
                self.rect(RIGHT as i32 - i - 1, TOP as i32, 1, 548, edge);
                self.rect(LEFT as i32, 653 - i, (RIGHT - LEFT) as i32, 1, edge);
            }
        }
        if juice.glow > 0. {
            let a = juice.glow * juice.flash() * 0.16;
            self.rect(
                LEFT as i32,
                TOP as i32,
                (RIGHT - LEFT) as i32,
                548,
                alpha(VIOLET, a),
            );
        }
        self.look = Default::default();
        self.draw_hud(g, presentation);
        match g.phase {
            Phase::Ready => self.ready_card(g, presentation),
            Phase::Transition => self.transition_card(g),
            Phase::Won | Phase::Lost => self.results_card(g, presentation),
            Phase::Playing => self.callout(g, presentation),
        }
        if paused {
            self.pause_card(g, presentation);
        }
    }

    fn draw_field(&mut self, _g: &Game) {
        for y in TOP as i32..654 {
            let tint = ((y - TOP as i32) as f32 / 550.0 * 5.0) as u8;
            self.rect(
                LEFT as i32,
                y,
                (RIGHT - LEFT) as i32,
                1,
                [13 + tint, 26 + tint, 33 + tint, 255],
            );
        }
        for i in 0..80 {
            let x = 49 + (i * 149 + 43) % 542;
            let y = 116 + (i * 97 + 19) % 522;
            self.rect(x, y, 1, 1, [54, 75, 79, if i % 7 == 0 { 170 } else { 85 }]);
        }
        for r in [100.0, 180.0, 260.0] {
            self.ring(320.0, 405.0, r, 1.0, [36, 57, 64, 150]);
        }
        self.line(320.0, TOP, 320.0, 653.0, [34, 52, 58, 100]);
        self.line(LEFT, 405.0, RIGHT, 405.0, [34, 52, 58, 100]);
        self.line(LEFT, TOP, LEFT, 653.0, LINE);
        self.line(RIGHT, TOP, RIGHT, 653.0, LINE);
        self.line(LEFT, TOP, RIGHT, TOP, LINE);
        for x in [LEFT, RIGHT] {
            self.rect(x as i32 - 2, TOP as i32, 4, 24, CYAN);
            self.rect(x as i32 - 2, 630, 4, 24, CYAN);
            for y in (360..420).step_by(12) {
                self.line(x - 3.0, y as f32, x + 3.0, y as f32 + 5.0, GOLD);
            }
        }
    }

    fn draw_panels(&mut self, g: &Game, juice: &crate::juice::Juice) {
        let last = g.remaining() == 1 && g.phase == Phase::Playing;
        for block in g.blocks.iter().filter(|b| b.health > 0) {
            let x = (block.center[0] - block.half[0]) as i32;
            let y = (block.center[1] - block.half[1]) as i32;
            let w = (block.half[0] * 2.0) as i32;
            let h = (block.half[1] * 2.0) as i32;
            let shade = panel_color(block.row).map(|c| (c * 255.).round() as u8);
            let shade = [shade[0], shade[1], shade[2], 255];
            self.rect(x + 2, y + 4, w, h, [3, 10, 15, 180]);
            let mut panel = shade;
            if block.flash > 0.0 && !g.policy.reduced_flash() {
                for channel in 0..3 {
                    panel[channel] = (f32::from(shade[channel])
                        + (f32::from(PAPER[channel]) - f32::from(shade[channel]))
                            * g.policy.intensity()) as u8;
                }
            }
            self.rect(x, y, w, h, panel);
            self.rect(x + 1, y + 1, w - 2, 2, [240, 240, 211, 80]);
            self.rect(x + 2, y + h - 5, w - 4, 4, [3, 18, 22, 45]);
            self.rect(x + 5, y + 5, 2, 2, [9, 27, 30, 120]);
            self.rect(x + w - 7, y + 5, 2, 2, [9, 27, 30, 120]);
            if block.original_health == 2 {
                // Reinforced panels carry a riveted steel band until cracked.
                self.rect(x + 3, y + h / 2 - 2, w - 6, 4, [44, 62, 66, 150]);
                for bolt in [x + 9, x + w / 2 - 1, x + w - 11] {
                    self.rect(bolt, y + h / 2 - 1, 2, 2, [214, 226, 214, 200]);
                }
                if block.health == 1 {
                    self.line(
                        block.center[0],
                        y as f32 + 2.0,
                        block.center[0] - 4.0,
                        y as f32 + 12.0,
                        INK,
                    );
                    self.line(
                        block.center[0] - 4.0,
                        y as f32 + 12.0,
                        block.center[0] + 3.0,
                        y as f32 + h as f32 - 1.0,
                        INK,
                    );
                    self.line(
                        block.center[0] - 4.0,
                        y as f32 + 12.0,
                        block.center[0] - 13.0,
                        y as f32 + 16.0,
                        INK,
                    );
                }
            }
            if last {
                // Spotlight the final panel with pulsing corner brackets.
                let pulse = if juice.motion() > 0. {
                    ((juice.time * 7.).sin() * 0.5 + 0.5) * 4.
                } else {
                    2.
                } as i32;
                let (l, t, r, b) = (
                    x - 4 - pulse,
                    y - 4 - pulse,
                    x + w + 3 + pulse,
                    y + h + 3 + pulse,
                );
                for (cx, cy, dx, dy) in [(l, t, 1, 1), (r, t, -1, 1), (l, b, 1, -1), (r, b, -1, -1)]
                {
                    let (cx, cy, dx, dy): (i32, i32, i32, i32) = (cx, cy, dx, dy);
                    self.rect(cx.min(cx + dx * 9), cy.min(cy + dy), 10, 2, GOLD);
                    self.rect(cx.min(cx + dx), cy.min(cy + dy * 9), 2, 10, GOLD);
                }
            }
        }
    }

    fn draw_satellite(&mut self, g: &Game, juice: &crate::juice::Juice) {
        let Some([x, y]) = g.satellite() else {
            return;
        };
        let blink = juice.motion() == 0. || ((g.time * 4.) as u32).is_multiple_of(2);
        self.rect(x as i32 - 15, y as i32 - 3, 9, 6, [70, 96, 170, 255]);
        self.rect(x as i32 + 7, y as i32 - 3, 9, 6, [70, 96, 170, 255]);
        self.line(x - 15., y, x + 15., y, [150, 170, 230, 255]);
        self.rect(x as i32 - 5, y as i32 - 5, 11, 10, [210, 214, 230, 255]);
        self.rect(x as i32 - 3, y as i32 - 3, 7, 3, [120, 128, 160, 255]);
        self.line(x, y - 5., x + 3., y - 10., [210, 214, 230, 255]);
        if blink {
            self.rect(x as i32 + 2, y as i32 - 12, 3, 3, CORAL);
        }
    }

    fn draw_drops(&mut self, g: &Game) {
        let sprite = if g.policy.reduced_motion() || g.policy.intensity() == 0. {
            0
        } else {
            self.pulse
                .frame(g.time, LoopMode::Repeat)
                .expect("finite time")
        };
        for drop in &g.drops {
            let [x, y] = drop.position;
            let (fill, mark) = match drop.kind {
                Pickup::Wide => ([30, 76, 70, 255], CYAN),
                Pickup::Split => ([58, 50, 96, 255], VIOLET),
                Pickup::Sticky => ([46, 80, 44, 255], LIME),
            };
            self.rect(x as i32 - 9, y as i32 - 9, 18, 18, fill);
            self.rect(x as i32 - 9, y as i32 - 9, 18, 1, mark);
            match drop.kind {
                Pickup::Wide => self.rect(x as i32 - 6, y as i32 - 2, 12, 4, mark),
                Pickup::Split => {
                    for dx in [-5, 0, 5] {
                        self.circle(x + dx as f32, y + if dx == 0 { -3. } else { 2. }, 2.2, mark);
                    }
                }
                Pickup::Sticky => {
                    self.rect(x as i32 - 6, y as i32 - 4, 12, 3, mark);
                    for dx in [-4, 0, 3] {
                        self.rect(x as i32 + dx, y as i32 - 1, 2, 3 + (dx & 3), mark);
                    }
                }
            }
            let shift = sprite as f32;
            self.line(x - 11.0 - shift, y - 4.0, x - 15.0 - shift, y, mark);
            self.line(x - 15.0 - shift, y, x - 11.0 - shift, y + 4.0, mark);
            self.line(x + 11.0 + shift, y - 4.0, x + 15.0 + shift, y, mark);
            self.line(x + 15.0 + shift, y, x + 11.0 + shift, y + 4.0, mark);
        }
    }

    fn ball_color(g: &Game) -> [u8; 4] {
        if g.armor_piercing() {
            GOLD
        } else if g.volley >= 5 {
            HOT
        } else {
            CYAN
        }
    }

    fn draw_trail(&mut self, g: &Game) {
        let color = Self::ball_color(g);
        let heat = 1. + (g.volley.min(10) as f32) * 0.06;
        for (i, p) in g.trail.iter().enumerate() {
            let t = (i + 1) as f32 / (g.trail.len() + 1) as f32;
            self.circle(
                p[0],
                p[1],
                (2.0 + t * 3.0) * heat,
                alpha(color, t * 0.32 * heat * g.policy.intensity()),
            );
        }
    }

    fn draw_paddle(&mut self, g: &Game, presentation: &Presentation) {
        let (sx, sy) = presentation.juice.paddle_scale();
        let w = (g.paddle_half * 2.0 * sx) as i32;
        let h = (16. * sy).round() as i32;
        let x = (g.paddle_x - g.paddle_half * sx) as i32;
        let top = PADDLE_Y as i32 + 8 - h;
        let stripe = if g.sticky > 0 || g.held.is_some() {
            LIME
        } else {
            CYAN
        };
        self.rect(x + 3, top + 6, w, h - 1, [3, 9, 13, 200]);
        self.rect(x, top, w, h, [51, 81, 85, 255]);
        self.rect(x + 2, top, w - 4, 3, stripe);
        self.rect(
            x + 9,
            top + 4,
            w - 18,
            (5. * sy).max(2.) as i32,
            [188, 214, 201, 255],
        );
        self.rect(x + 5, top + h - 5, 4, 3, GOLD);
        self.rect(x + w - 9, top + h - 5, 4, 3, GOLD);
        if g.sticky > 0 || g.held.is_some() {
            for (i, dx) in [0.18_f32, 0.42, 0.66, 0.86].into_iter().enumerate() {
                self.rect(
                    x + (w as f32 * dx) as i32,
                    top + 3,
                    2,
                    2 + (i as i32 % 2) * 2,
                    LIME,
                );
            }
        }
        let emphasis = presentation.feedback.impact().emphasis;
        if emphasis > 0.01 {
            self.rect(
                x,
                top - 3,
                w,
                (emphasis * 6.).ceil() as i32,
                [99, 215, 196, 130],
            );
        }
        if g.wide_time > 0.0 {
            self.rect(
                x,
                PADDLE_Y as i32 + 11,
                (w as f32 * g.wide_time / 9.0) as i32,
                2,
                CYAN,
            );
        }
        if let Some(held) = g.held {
            // Aim preview: where the caught ball will fly on release.
            let speed = held.velocity[0].hypot(held.velocity[1]).max(1.);
            let dir = [held.velocity[0] / speed, held.velocity[1] / speed];
            for i in 1..7 {
                let d = i as f32 * 16.;
                self.circle(
                    g.ball[0] + dir[0] * d,
                    g.ball[1] + dir[1] * d,
                    1.6,
                    alpha(LIME, 1. - i as f32 / 8.),
                );
            }
        }
    }

    fn draw_ball(&mut self, at: [f32; 2], color: [u8; 4]) {
        self.circle(at[0], at[1], 12.0, alpha(color, 0.07));
        self.circle(at[0], at[1], 8.0, alpha(color, 0.26));
        self.circle(at[0], at[1], BALL_RADIUS - 0.2, PAPER);
        self.circle(at[0] - 1.0, at[1] - 1.0, 2.5, [255, 255, 241, 255]);
    }

    fn draw_balls(&mut self, g: &Game) {
        if matches!(g.phase, Phase::Won | Phase::Lost | Phase::Transition) {
            return;
        }
        for ball in &g.extra_balls {
            self.draw_ball(ball.position, VIOLET);
        }
        self.draw_ball(g.ball, Self::ball_color(g));
    }

    fn chip(&mut self, x: i32, label: &str, color: [u8; 4]) -> i32 {
        let w = text_width(label, 1) + 12;
        self.rect(x, 84, w, 13, alpha(color, 0.16));
        self.rect(x, 84, 2, 13, color);
        self.text(label, x + 7, 87, 1, color);
        x + w + 6
    }

    fn draw_hud(&mut self, g: &Game, presentation: &Presentation) {
        self.text("ORBIT BREAKER", 38, 33, 3, PAPER);
        let (name, _) = quips::sector(g.sector);
        self.text(&format!("SECTOR 0{} / {name}", g.sector), 39, 68, 1, MUTED);
        let score = format!("{:05}", g.score);
        self.text_right(&score, 600, 32, 3, GOLD);
        let multiplier = g.chain.multiplier();
        if multiplier > 1 {
            let label = format!("X{multiplier}");
            let right = 600 - text_width(&score, 3) - 10;
            let w = text_width(&label, 2) + 10;
            self.rect(right - w, 34, w, 20, alpha(HOT, 0.18));
            self.rect(right - w, 34, w, 1, HOT);
            self.rect(right - w, 53, w, 1, HOT);
            self.text(&label, right - w + 5, 37, 2, HOT);
        }
        match presentation.best {
            Some(best) if g.score > best => self.text_right("NEW BEST PACE!", 599, 68, 1, HOT),
            Some(best) => self.text_right(&format!("BEST {best:05}"), 599, 68, 1, MUTED),
            None => self.text_right("SCORE", 599, 68, 1, MUTED),
        }
        if g.phase == Phase::Playing {
            let mut x = 38;
            if g.armor_piercing() {
                x = self.chip(x, "ARMOR PIERCING", GOLD);
            }
            if g.wide_time > 0. {
                x = self.chip(x, &format!("WIDE {:.1}S", g.wide_time), CYAN);
            }
            if !g.extra_balls.is_empty() {
                x = self.chip(
                    x,
                    &format!("MULTI-BALL X{}", g.extra_balls.len() + 1),
                    VIOLET,
                );
            }
            if g.held.is_some() {
                self.chip(x, &format!("{} TO FIRE", presentation.hints.launch), LIME);
            } else if g.sticky > 0 {
                self.chip(x, &format!("STICKY X{}", g.sticky), LIME);
            }
            if g.volley >= 2 {
                let color = if g.volley >= 5 { HOT } else { GOLD };
                self.text_right(&format!("VOLLEY {}", g.volley), 600, 87, 1, color);
            }
        }
        for i in 0..3 {
            let x = 46.0 + i as f32 * 17.0;
            if i < g.lives {
                self.circle(x, 682.0, 4.5, alpha(CYAN, 0.25));
                self.circle(x, 682.0, 3.2, PAPER);
            } else {
                self.circle(x, 682.0, 3.0, LINE);
            }
        }
        let remaining = g.remaining();
        let panels = if remaining == 1 {
            "1 PANEL LEFT!".to_owned()
        } else {
            format!("{remaining:02} PANELS LEFT")
        };
        self.text(
            &panels,
            104,
            679,
            1,
            if remaining == 1 { GOLD } else { MUTED },
        );
        if g.ball_save > 0. && g.phase == Phase::Playing {
            self.text_center("BALL SAVE", 672, 1, CYAN);
            self.rect(290, 683, 60, 2, LINE);
            self.rect(290, 683, (60. * g.ball_save / 2.5) as i32, 2, CYAN);
        }
        if g.precision.streak() > 0 {
            self.text_right(
                &format!("PRECISE X{}", g.precision.streak()),
                600,
                679,
                1,
                CYAN,
            );
        } else {
            let hint = format!(
                "{} PAUSE / {} RESTART",
                presentation.hints.pause, presentation.hints.restart
            );
            if text_width(&hint, 1) <= 200 {
                self.text_right(&hint, 600, 679, 1, [70, 96, 101, 255]);
            }
        }
    }

    fn callout(&mut self, g: &Game, presentation: &Presentation) {
        if let Some(callout) = &presentation.juice.callout {
            let a = callout.alpha();
            let motion = presentation.juice.motion();
            let bounce = if motion > 0. {
                ((1. - (callout.age / 0.18).min(1.)) * 6.) as i32
            } else {
                0
            };
            let head = callout.head.as_str();
            let scale = if text_width(head, 2) <= 540 { 2 } else { 1 };
            let x = (WIDTH as i32 - text_width(head, scale)) / 2;
            let y = 540 - bounce;
            self.text(head, x + 2, y + 2, scale, alpha(INK, a));
            self.text(head, x, y, scale, alpha(callout.color, a));
            let sub = callout.sub.as_str();
            let x = (WIDTH as i32 - text_width(sub, 1)) / 2;
            self.text(sub, x + 1, 563, 1, alpha(INK, a));
            self.text(sub, x, 562, 1, alpha(PAPER, a * 0.85));
        } else if g.feedback() == Some(Feedback::BallSaved) {
            self.text_center("BALL SAVED", 553, 1, CYAN);
        }
    }

    fn card(&mut self, x: i32, y: i32, w: i32, h: i32, accent: [u8; 4]) {
        self.rect(x + 4, y + 6, w, h, [0, 0, 0, 120]);
        self.rect(x, y, w, h, CARD);
        self.rect(x, y, w, 1, LINE);
        self.rect(x, y + h - 1, w, 1, LINE);
        self.rect(x, y, 1, h, LINE);
        self.rect(x + w - 1, y, 1, h, LINE);
        self.rect(WIDTH as i32 / 2 - 18, y + 14, 36, 2, accent);
    }

    fn ready_card(&mut self, g: &Game, presentation: &Presentation) {
        let (name, tagline) = quips::sector(g.sector);
        let (title, subtitle, sub_color) = if g.feedback() == Some(Feedback::LifeLost) {
            (
                format!(
                    "BALL LOST / {} {} LEFT",
                    g.lives,
                    if g.lives == 1 { "LIFE" } else { "LIVES" }
                ),
                quips::drain(g.lives_lost),
                CORAL,
            )
        } else if g.feedback() == Some(Feedback::Restarted) {
            (
                "FRESH RUN / 3 LIVES".to_owned(),
                quips::retry(presentation.restarts.max(1)),
                MUTED,
            )
        } else if g.lives_lost == 0 || g.clears == 0 || g.sector == 2 && g.remaining() == 35 {
            (format!("SECTOR 0{} / {name}", g.sector), tagline, MUTED)
        } else {
            ("READY FOR LAUNCH".to_owned(), tagline, MUTED)
        };
        self.card(104, 368, 432, 192, CYAN);
        self.text_fit_center(&title, 392, 2, 408, PAPER);
        self.text_fit_center(subtitle, 418, 1, 408, sub_color);
        let power = g.serve_charge.power();
        let (label, color) = if power >= 0.95 {
            ("FULL CHARGE / ARMOR PIERCING READY".to_owned(), GOLD)
        } else {
            (
                format!("CHARGING SERVE {:03}%", (power * 100.) as u32),
                MUTED,
            )
        };
        self.text_fit_center(&label, 444, 1, 408, color);
        self.rect(180, 458, 280, 4, LINE);
        self.rect(
            180,
            458,
            (280. * power) as i32,
            4,
            if power >= 0.95 { GOLD } else { CYAN },
        );
        self.text_fit_center(
            &format!("{} TO LAUNCH", presentation.hints.launch),
            480,
            2,
            408,
            CYAN,
        );
        let juice = &presentation.juice;
        if !juice.steering_learned() {
            let fade = 1. - juice.steer_learned / 0.75;
            self.text_fit_center(
                &format!("STEER: {}", presentation.hints.movement),
                512,
                1,
                408,
                alpha(PAPER, 0.4 + 0.6 * fade),
            );
        } else if !juice.charge_learned {
            self.text_fit_center(
                "TIP: WAIT FOR A FULL CHARGE TO PIERCE ARMOR",
                512,
                1,
                408,
                MUTED,
            );
        } else {
            self.text_fit_center(
                "CATCH CAPSULES / CHAIN PANELS / FIND THE SECRET",
                512,
                1,
                408,
                MUTED,
            );
        }
        self.text_fit_center(
            &format!(
                "{} {} / {} PANELS",
                g.lives,
                if g.lives == 1 { "LIFE" } else { "LIVES" },
                g.remaining()
            ),
            532,
            1,
            408,
            [70, 96, 101, 255],
        );
    }

    fn transition_card(&mut self, g: &Game) {
        self.card(104, 368, 432, 192, GOLD);
        self.text_fit_center(&format!("SECTOR 0{} SECURED", g.sector), 392, 2, 408, PAPER);
        if g.sector_lives_lost == 0 {
            self.text_fit_center(
                &format!("FLAWLESS! +{} BONUS", crate::game::FLAWLESS_POINTS),
                420,
                2,
                408,
                HOT,
            );
        } else {
            self.text_fit_center("NOT BAD FOR A SPACE JANITOR", 424, 1, 408, MUTED);
        }
        let (next, tagline) = quips::sector(g.sector + 1);
        self.text_fit_center(&format!("NEXT: {next}"), 452, 1, 408, GOLD);
        self.text_fit_center("REINFORCED PANELS NEED TWO HITS", 470, 1, 408, MUTED);
        self.text_fit_center(tagline, 486, 1, 408, MUTED);
        self.text_fit_center(
            &format!(
                "SECTOR 0{} IN {:.1} SECONDS",
                g.sector + 1,
                g.transition_remaining()
            ),
            512,
            1,
            408,
            CYAN,
        );
        self.rect(180, 530, 280, 3, LINE);
        let progress = (1. - g.transition_remaining() / 1.4).clamp(0., 1.);
        self.rect(180, 530, (280. * progress) as i32, 3, CYAN);
    }

    fn pause_card(&mut self, g: &Game, presentation: &Presentation) {
        self.card(104, 368, 432, 192, CYAN);
        self.text_center("PAUSED", 392, 3, PAPER);
        self.text_fit_center(
            &format!(
                "SECTOR {} / {} POINTS / {} LIVES",
                g.sector, g.score, g.lives
            ),
            432,
            1,
            408,
            MUTED,
        );
        self.text_fit_center(quips::pause(presentation.pauses), 452, 1, 408, GOLD);
        self.text_fit_center(
            &format!("{} TO RESUME", presentation.hints.pause),
            482,
            2,
            408,
            CYAN,
        );
        self.text_fit_center(
            &format!("{} TO RESTART", presentation.hints.restart),
            516,
            1,
            408,
            MUTED,
        );
    }

    fn confetti(&mut self, celebrate: f32) {
        const COLORS: [[u8; 4]; 5] = [GOLD, CYAN, VIOLET, LIME, HOT];
        for i in 0..56_u32 {
            let hash = i.wrapping_mul(2_654_435_761).rotate_left(7);
            let x0 = LEFT + (hash % 560) as f32;
            let speed = 50. + (hash >> 10) as f32 % 90.;
            let y = TOP - 20. + (celebrate * speed + (hash >> 4) as f32 % 560.) % 560.;
            let sway = (celebrate * 2.3 + i as f32).sin() * 9.;
            let wide = (celebrate * 6. + i as f32).sin() > 0.;
            let (w, h) = if wide { (5, 2) } else { (2, 5) };
            self.rect((x0 + sway) as i32, y as i32, w, h, COLORS[i as usize % 5]);
        }
    }

    fn results_card(&mut self, g: &Game, presentation: &Presentation) {
        let juice = &presentation.juice;
        self.rect(
            LEFT as i32 + 1,
            TOP as i32 + 1,
            (RIGHT - LEFT) as i32 - 1,
            547,
            [4, 10, 14, 200],
        );
        if g.phase == Phase::Won && juice.motion() > 0. && juice.celebrate > 0. {
            self.confetti(juice.celebrate);
        }
        let won = g.phase == Phase::Won;
        self.card(72, 160, 496, 452, if won { GOLD } else { CORAL });
        self.text_center(
            if won { "FIELD CLEARED" } else { "SIGNAL LOST" },
            186,
            2,
            if won { CYAN } else { CORAL },
        );
        let Some(results) = &presentation.results else {
            self.text_fit_center(
                &format!("{} POINTS / {} PANELS CLEARED", g.score, g.clears),
                230,
                1,
                464,
                MUTED,
            );
            self.text_fit_center(
                &format!("{} TO RETRY", presentation.hints.launch),
                560,
                2,
                464,
                CYAN,
            );
            return;
        };
        self.text_center("YOUR TITLE", 218, 1, MUTED);
        self.text_fit_center(results.rank.title, 232, 3, 464, GOLD);
        self.text_fit_center(results.rank.quip, 264, 1, 464, PAPER);
        self.text_center(&format!("{}", g.score), 290, 4, PAPER);
        self.text_center("POINTS", 324, 1, MUTED);
        match (results.new_best, results.previous_best) {
            (true, previous) => {
                let pulse = if juice.motion() > 0. {
                    (juice.celebrate * 5.).sin() * 0.25 + 0.75
                } else {
                    1.
                };
                self.text_center("NEW PERSONAL BEST!", 342, 2, alpha(HOT, pulse));
                if let Some(previous) = previous {
                    self.text_center(&format!("OLD BEST {previous}"), 364, 1, MUTED);
                }
            }
            (false, Some(best)) => {
                self.text_center(&format!("PERSONAL BEST {best}"), 344, 1, GOLD);
                self.text_center(
                    &format!(
                        "{} MORE POINTS TO BEAT IT",
                        best.saturating_sub(g.score).saturating_add(1)
                    ),
                    360,
                    1,
                    MUTED,
                );
            }
            (false, None) => {}
        }
        let stats: [(&str, String); 8] = [
            ("TIME", format!("{:.0}S", g.time)),
            ("PANELS", format!("{}", g.clears)),
            ("BEST VOLLEY", format!("{}", g.best_volley)),
            ("BEST PRECISE", format!("{}", g.precision.best())),
            ("BALL SAVES", format!("{}", g.saves)),
            ("SATELLITES", format!("{}", g.satellites)),
            ("FLAWLESS", format!("{}", g.flawless_sectors)),
            ("BALLS LOST", format!("{}", g.lives_lost)),
        ];
        self.rect(112, 384, 416, 1, LINE);
        for (i, (label, value)) in stats.iter().enumerate() {
            let column = (i % 2) as i32;
            let row = (i / 2) as i32;
            let x = 112 + column * 224;
            let y = 396 + row * 20;
            self.text(label, x, y, 1, MUTED);
            self.text_right(value, x + 192, y, 1, PAPER);
        }
        self.rect(112, 474, 416, 1, LINE);
        if let Some((amount, unit, title)) = quips::next_goal(results.summary) {
            self.text_fit_center(
                &format!("NEXT TITLE: {title} IN {amount} {unit}"),
                488,
                1,
                464,
                CYAN,
            );
        } else {
            self.text_fit_center("TOP TITLE EARNED. THE ORBIT IS YOURS.", 488, 1, 464, CYAN);
        }
        if g.satellites == 0 {
            self.text_fit_center(
                "PSST... SOMETHING ORBITS ABOVE THE PANELS",
                506,
                1,
                464,
                VIOLET,
            );
        }
        self.text_fit_center(
            &format!("{} TO RETRY", presentation.hints.launch),
            552,
            2,
            464,
            CYAN,
        );
        self.text_fit_center("ONE MORE ORBIT?", 584, 1, 464, MUTED);
    }
    fn text_fit_center(&mut self, text: &str, y: i32, scale: i32, width: i32, color: [u8; 4]) {
        let scale = if scale > 1 && text_width(text, scale) > width {
            1
        } else {
            scale
        };
        let fitted = fit_label(text, scale, width);
        self.text_center(&fitted, y, scale, color);
    }
    fn text_center(&mut self, t: &str, y: i32, s: i32, c: [u8; 4]) {
        self.text(t, (WIDTH as i32 - text_width(t, s)) / 2, y, s, c);
    }
    fn text_right(&mut self, t: &str, x: i32, y: i32, s: i32, c: [u8; 4]) {
        self.text(t, x - text_width(t, s), y, s, c);
    }
    fn text(&mut self, text: &str, mut x: i32, y: i32, scale: i32, color: [u8; 4]) {
        let look = self.look;
        self.look = Default::default();
        for letter in text.chars() {
            let rows = glyph(letter);
            for (row, bits) in rows.into_iter().enumerate() {
                for col in 0..5 {
                    if bits & (1 << (4 - col)) != 0 {
                        self.rect(x + col * scale, y + row as i32 * scale, scale, scale, color);
                    }
                }
            }
            x += 6 * scale;
        }
        self.look = look;
    }
    fn tint(&self, color: [u8; 4]) -> [u8; 4] {
        self.look
            .shade(Color::from_rgba(color[0], color[1], color[2], color[3]))
            .to_rgba()
    }
    fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: [u8; 4]) {
        let (x, y) = (x + self.shift[0], y + self.shift[1]);
        let color = self.tint(color);
        if color[3] == 255 {
            let left = x.clamp(0, WIDTH as i32) as usize;
            let right = x.saturating_add(w).clamp(left as i32, WIDTH as i32) as usize;
            for py in y.max(0)..y.saturating_add(h).min(HEIGHT as i32) {
                self.frame.pixels_mut()
                    [py as usize * WIDTH as usize + left..py as usize * WIDTH as usize + right]
                    .fill(color);
            }
            return;
        }
        for py in y.max(0)..(y + h).min(HEIGHT as i32) {
            for px in x.max(0)..(x + w).min(WIDTH as i32) {
                self.pixel(px, py, color);
            }
        }
    }
    fn pixel(&mut self, x: i32, y: i32, color: [u8; 4]) {
        if !(0..WIDTH as i32).contains(&x) || !(0..HEIGHT as i32).contains(&y) {
            return;
        }
        let dst = &mut self.frame.pixels_mut()[(y as u32 * WIDTH + x as u32) as usize];
        let alpha = u32::from(color[3]);
        for i in 0..3 {
            dst[i] =
                ((u32::from(color[i]) * alpha + u32::from(dst[i]) * (255 - alpha)) / 255) as u8;
        }
        dst[3] = 255;
    }
    fn circle(&mut self, x: f32, y: f32, r: f32, color: [u8; 4]) {
        let (x, y) = (x + self.shift[0] as f32, y + self.shift[1] as f32);
        let color = self.tint(color);
        for py in (y - r - 1.0) as i32..=(y + r + 1.0) as i32 {
            for px in (x - r - 1.0) as i32..=(x + r + 1.0) as i32 {
                let d = (px as f32 + 0.5 - x).hypot(py as f32 + 0.5 - y);
                let coverage = (r + 0.5 - d).clamp(0.0, 1.0);
                if coverage > 0.0 {
                    let mut c = color;
                    c[3] = (f32::from(c[3]) * coverage) as u8;
                    self.pixel(px, py, c);
                }
            }
        }
    }
    fn ring(&mut self, x: f32, y: f32, r: f32, thickness: f32, color: [u8; 4]) {
        let steps = (r * 7.0) as i32;
        for i in 0..steps {
            let a = i as f32 / steps as f32 * std::f32::consts::TAU;
            self.circle(x + a.cos() * r, y + a.sin() * r, thickness * 0.5, color);
        }
    }
    fn line(&mut self, x: f32, y: f32, end_x: f32, end_y: f32, color: [u8; 4]) {
        let [dx, dy] = self.shift.map(|n| n as f32);
        let (x, y, end_x, end_y) = (x + dx, y + dy, end_x + dx, end_y + dy);
        let color = self.tint(color);
        let steps = (end_x - x).abs().max((end_y - y).abs()).ceil() as i32;
        if steps == 0 {
            self.pixel(x as i32, y as i32, color);
            return;
        }
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            self.pixel(
                (x + (end_x - x) * t).round() as i32,
                (y + (end_y - y) * t).round() as i32,
                color,
            );
        }
    }
}
fn text_width(text: &str, scale: i32) -> i32 {
    let metrics = ferravex_engine::render::get_default_font()
        .try_metrics(8, scale as f32)
        .expect("positive bitmap scale");
    (metrics.measure_line(text).expect("bounded HUD label").width as i32 - scale).max(0)
}
fn fit_label(text: &str, scale: i32, width: i32) -> std::borrow::Cow<'_, str> {
    if text_width(text, scale) <= width {
        return std::borrow::Cow::Borrowed(text);
    }
    let cells = ((width + scale) / (6 * scale)).max(3) as usize;
    let mut fitted: String = text.chars().take(cells - 3).collect();
    fitted.push_str("...");
    std::borrow::Cow::Owned(fitted)
}
fn glyph(c: char) -> [u8; 7] {
    match c {
        'A' => [14, 17, 17, 31, 17, 17, 17],
        'B' => [30, 17, 17, 30, 17, 17, 30],
        'C' => [14, 17, 16, 16, 16, 17, 14],
        'D' => [30, 17, 17, 17, 17, 17, 30],
        'E' => [31, 16, 16, 30, 16, 16, 31],
        'F' => [31, 16, 16, 30, 16, 16, 16],
        'G' => [14, 17, 16, 23, 17, 17, 15],
        'H' => [17, 17, 17, 31, 17, 17, 17],
        'I' => [31, 4, 4, 4, 4, 4, 31],
        'J' => [7, 2, 2, 2, 18, 18, 12],
        'K' => [17, 18, 20, 24, 20, 18, 17],
        'L' => [16, 16, 16, 16, 16, 16, 31],
        'M' => [17, 27, 21, 21, 17, 17, 17],
        'N' => [17, 25, 21, 19, 17, 17, 17],
        'O' => [14, 17, 17, 17, 17, 17, 14],
        'P' => [30, 17, 17, 30, 16, 16, 16],
        'Q' => [14, 17, 17, 17, 21, 18, 13],
        'R' => [30, 17, 17, 30, 20, 18, 17],
        'S' => [15, 16, 16, 14, 1, 1, 30],
        'T' => [31, 4, 4, 4, 4, 4, 4],
        'U' => [17, 17, 17, 17, 17, 17, 14],
        'V' => [17, 17, 17, 17, 17, 10, 4],
        'W' => [17, 17, 17, 21, 21, 21, 10],
        'X' => [17, 17, 10, 4, 10, 17, 17],
        'Y' => [17, 17, 10, 4, 4, 4, 4],
        'Z' => [31, 1, 2, 4, 8, 16, 31],
        '0' => [14, 17, 19, 21, 25, 17, 14],
        '1' => [4, 12, 4, 4, 4, 4, 14],
        '2' => [14, 17, 1, 2, 4, 8, 31],
        '3' => [30, 1, 1, 14, 1, 1, 30],
        '4' => [2, 6, 10, 18, 31, 2, 2],
        '5' => [31, 16, 16, 30, 1, 1, 30],
        '6' => [14, 16, 16, 30, 17, 17, 14],
        '7' => [31, 1, 2, 4, 8, 8, 8],
        '8' => [14, 17, 17, 14, 17, 17, 14],
        '9' => [14, 17, 17, 15, 1, 1, 14],
        '/' => [1, 1, 2, 4, 8, 16, 16],
        '+' => [0, 4, 4, 31, 4, 4, 0],
        ':' => [0, 12, 12, 0, 12, 12, 0],
        '-' => [0, 0, 0, 31, 0, 0, 0],
        '.' => [0, 0, 0, 0, 0, 12, 12],
        '%' => [25, 26, 2, 4, 8, 11, 19],
        '!' => [4, 4, 4, 4, 4, 0, 4],
        '?' => [14, 17, 1, 2, 4, 0, 4],
        '\'' => [4, 4, 8, 0, 0, 0, 0],
        ',' => [0, 0, 0, 0, 12, 4, 8],
        '(' => [2, 4, 8, 8, 8, 4, 2],
        ')' => [8, 4, 2, 2, 2, 4, 8],
        '#' => [10, 10, 31, 10, 31, 10, 10],
        _ => [0; 7],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presentation::Presentation;
    use ferravex_engine::play::feedback::FeedbackPolicy;

    #[test]
    fn legacy_retry_glyph_geometry_explains_why_narrow_status_uses_native_dom_text() {
        let mut canvas = Canvas::new();
        canvas.frame.clear(Color::from_hex(0x091219));
        canvas.text("J", 0, 0, 1, PAPER);
        let lit: Vec<_> = canvas
            .frame
            .pixels()
            .iter()
            .enumerate()
            .filter(|(_, pixel)| **pixel == PAPER)
            .map(|(index, _)| (index % WIDTH as usize, index / WIDTH as usize))
            .collect();
        assert_eq!(lit.iter().map(|p| p.0).max(), Some(4));
        assert_eq!(lit.iter().map(|p| p.1).max(), Some(6));
        assert_eq!(glyph('J'), [7, 2, 2, 2, 18, 18, 12]);
        for css_width in [300.796875_f64, 320., 327.109375, 366.59375, 390.] {
            let glyph_height = 7. * css_width / f64::from(WIDTH);
            assert!(glyph_height < 4.3, "{glyph_height}");
        }
        assert_eq!((canvas.frame.width(), canvas.frame.height()), (640, 720));
    }

    #[test]
    fn shared_measurements_fit_labels_inside_the_panel_and_empty_labels_have_zero_width() {
        assert_eq!(text_width("", 1), 0);
        assert_eq!(text_width("ORBIT BREAKER", 3), 231);
        for label in [
            "SPACE / ENTER / MOUSE 1 TO LAUNCH".to_owned(),
            "PAD 10 / ".repeat(64),
        ] {
            let fitted = fit_label(&label, 1, 408);
            assert!(text_width(&fitted, 1) <= 408);
            let mut canvas = Canvas::new();
            canvas.frame.clear(Color::from_hex(0x091219));
            canvas.text_center(&fitted, 493, 1, CYAN);
            for (i, pixel) in canvas.frame.pixels().iter().enumerate() {
                if *pixel == CYAN {
                    assert!((116..524).contains(&(i % WIDTH as usize)));
                }
            }
        }
    }

    #[test]
    fn reduced_motion_stops_capsule_pulses_and_pause_is_visible_in_the_software_frame() {
        let mut game = Game::new();
        game.drops.push(crate::game::Drop {
            position: [320., 340.],
            kind: crate::game::Pickup::Wide,
        });
        game.apply_policy(FeedbackPolicy::new(true, false, 1.).unwrap());
        let mut canvas = Canvas::new();
        let presentation = Presentation::default();
        canvas.render_session(&game, false, &presentation);
        let still = canvas.frame.pixels().to_vec();
        game.time = 0.25;
        canvas.render_session(&game, false, &presentation);
        assert_eq!(canvas.frame.pixels(), still);
        canvas.render_session(&game, true, &presentation);
        assert_ne!(canvas.frame.pixels(), still);
        assert!(canvas.frame.pixels().iter().all(|p| p[3] == 255));
        game.apply_policy(FeedbackPolicy::default());
        canvas.render_session(&game, false, &presentation);
        assert_ne!(canvas.frame.pixels(), still);
    }
}
