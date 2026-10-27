//! Orbit Breaker's original CPU raster.
//!
//! Each frame copies a cached, pre-graded backdrop (gradient sky, nebulae,
//! stars, orbit rings and a vignette), draws the world with anti-aliased
//! shapes, splats emissive light into a quarter-resolution buffer, blurs that
//! buffer with separable running-sum box passes and screen-blends it back
//! (bloom), then draws the HUD and cards crisp on top. Steady-state frames do
//! not allocate.
use crate::font::{self, Case};
use crate::game::{
    BALL_RADIUS, Feedback, Game, HEIGHT, LEFT, PADDLE_Y, Phase, Pickup, RIGHT, TOP, WIDTH,
    panel_color,
};
use crate::juice::{HOT, LIME, VIOLET};
use crate::presentation::Presentation;
use crate::quips;
use ferravex_engine::{
    graphics::{Color, Framebuffer, Look},
    play::animation::{LoopMode, SpriteClip, SpriteFrame},
    play::tween::{Ease, TransitionKind, TransitionPhase},
};
use std::fmt::{self, Write};

const INK: [u8; 4] = [9, 18, 25, 255];
const LINE: [u8; 4] = [42, 65, 72, 255];
const PAPER: [u8; 4] = [232, 240, 226, 255];
const MUTED: [u8; 4] = [138, 165, 166, 255];
const DIM: [u8; 4] = [84, 110, 116, 255];
const CYAN: [u8; 4] = [99, 215, 196, 255];
const GOLD: [u8; 4] = [216, 173, 109, 255];
const CORAL: [u8; 4] = [236, 132, 112, 255];
const FIELD_BOTTOM: i32 = 654;

/// Quarter-resolution emissive buffer for bloom.
const GW: usize = WIDTH as usize / 4;
const GH: usize = HEIGHT as usize / 4;
const BLUR: usize = 2;

fn alpha(color: [u8; 4], amount: f32) -> [u8; 4] {
    [
        color[0],
        color[1],
        color[2],
        (f32::from(color[3]) * amount.clamp(0., 1.)) as u8,
    ]
}

fn mix(a: [u8; 4], b: [u8; 4], t: f32) -> [u8; 4] {
    let t = t.clamp(0., 1.);
    std::array::from_fn(|i| (f32::from(a[i]) + (f32::from(b[i]) - f32::from(a[i])) * t) as u8)
}

fn scale_rgb(c: [u8; 4], k: f32) -> [u8; 4] {
    [
        (f32::from(c[0]) * k).min(255.) as u8,
        (f32::from(c[1]) * k).min(255.) as u8,
        (f32::from(c[2]) * k).min(255.) as u8,
        c[3],
    ]
}

/// A stack-allocated line of HUD copy, truncated on a character boundary.
struct Line {
    buf: [u8; 160],
    len: usize,
}
impl Line {
    fn as_str(&self) -> &str {
        std::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
    }
}
impl Write for Line {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        for c in text.chars() {
            let end = self.len + c.len_utf8();
            if end > self.buf.len() {
                return Err(fmt::Error);
            }
            c.encode_utf8(&mut self.buf[self.len..end]);
            self.len = end;
        }
        Ok(())
    }
}
fn line(args: fmt::Arguments) -> Line {
    let mut out = Line {
        buf: [0; 160],
        len: 0,
    };
    let _ = out.write_fmt(args);
    out
}

/// A gentle filmic grade: deeper teal shadows, slightly warm highlights and a
/// soft S-curve. Applied to world colors as they are drawn, never per pixel.
fn grade_table() -> [[u8; 256]; 3] {
    let mut lut = [[0; 256]; 3];
    for (channel, table) in lut.iter_mut().enumerate() {
        for (i, out) in table.iter_mut().enumerate() {
            let x = i as f32 / 255.;
            let curve = x + 0.10 * (x - 0.5) * (1. - (2. * x - 1.).abs());
            let shadow = (1. - x).powi(3);
            let highlight = x.powi(3);
            let lift = [-0.004, 0.010, 0.022][channel] * shadow;
            let warm = [0.03, 0.008, -0.03][channel] * highlight;
            *out = ((curve + lift + warm).clamp(0., 1.) * 255.).round() as u8;
        }
    }
    lut
}

pub struct Canvas {
    pub frame: Framebuffer,
    pulse: SpriteClip,
    look: Look,
    /// Camera kick applied to the playfield layer only; the HUD stays put.
    shift: [i32; 2],
    graded: bool,
    grade: [[u8; 256]; 3],
    background: Vec<[u8; 4]>,
    background_look: Option<Look>,
    glow: Vec<[u16; 3]>,
    scratch: Vec<[u32; 3]>,
    /// Dirty quarter-cell bounds `[x0, y0, x1, y1)`; empty when x0 >= x1.
    glow_box: [usize; 4],
    glow_gain: f32,
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
            graded: false,
            grade: grade_table(),
            background: vec![[0; 4]; (WIDTH * HEIGHT) as usize],
            background_look: None,
            glow: vec![[0; 3]; GW * GH],
            scratch: vec![[0; 3]; GW.max(GH)],
            glow_box: [GW, GH, 0, 0],
            glow_gain: 1.,
        }
    }
    pub fn render(&mut self, g: &Game) {
        self.render_session(g, false, &Presentation::default());
    }
    pub(crate) fn render_session(&mut self, g: &Game, paused: bool, presentation: &Presentation) {
        let juice = &presentation.juice;
        self.look = ferravex_engine::render::look();
        if self.background_look != Some(self.look) {
            self.build_background();
        }
        // Reduced flash caps bloom, as the engine's post stack does.
        self.glow_gain = g.policy.intensity() * if g.policy.reduced_flash() { 0.6 } else { 1. };
        self.shift = juice.shake();
        self.graded = true;
        self.copy_background();
        self.draw_ambient(g);
        self.draw_panels(g, juice);
        self.draw_satellite(g, juice);
        self.draw_drops(g);
        self.draw_trail(g);
        for p in g.particles.iter() {
            let mut color = p.color.map(|n| (n.clamp(0.0, 1.0) * 255.0) as u8);
            color[3] = (f32::from(color[3]) * g.policy.intensity()) as u8;
            let size = p.size.ceil() as i32;
            self.rect(
                p.position[0] as i32,
                p.position[1] as i32,
                size,
                size,
                color,
            );
            if size >= 3 {
                self.glow(p.position[0], p.position[1], 3., color, 0.25);
            }
        }
        let live = !matches!(g.phase, Phase::Won | Phase::Lost);
        for ring in juice.rings.iter().flatten().filter(|_| live) {
            let t = ring.age / ring.life;
            let fade = (1. - t) * juice.motion();
            let radius = 4. + ring.radius * Ease::CubicOut.apply(t);
            self.ring(
                ring.at[0],
                ring.at[1],
                radius,
                2.,
                alpha(ring.color, 0.85 * fade),
            );
            self.glow(ring.at[0], ring.at[1], radius * 0.6, ring.color, 0.5 * fade);
        }
        self.draw_paddle(g, presentation);
        self.draw_balls(g);
        for pop in juice.pops.iter().flatten().filter(|_| live) {
            let t = pop.age / pop.life;
            let rise = 26. * Ease::QuadOut.apply(t) * if juice.motion() > 0. { 1. } else { 0. };
            let fade = ((1. - t) / 0.35).clamp(0., 1.);
            let text = pop.text.as_str();
            let x = pop.at[0] as i32 - text_width(text, pop.scale) / 2;
            let y = (pop.at[1] - 10. - rise) as i32;
            self.text(text, x + 1, y + 1, pop.scale, alpha(INK, fade));
            self.text(text, x, y, pop.scale, alpha(pop.color, fade));
            self.glow(pop.at[0], y as f32 + 4., 10., pop.color, 0.3 * fade);
        }
        if g.phase == Phase::Playing {
            self.callout(g, presentation);
        }
        self.draw_transition(presentation);
        self.shift = [0; 2];
        self.composite_glow();
        if juice.vignette > 0. {
            let a = juice.vignette * juice.flash();
            for i in 0..14 {
                let edge = alpha(CORAL, a * (1. - i as f32 / 14.).powi(2) * 0.4);
                self.rect(LEFT as i32 + i, TOP as i32, 1, 548, edge);
                self.rect(RIGHT as i32 - i - 1, TOP as i32, 1, 548, edge);
                self.rect(
                    LEFT as i32,
                    FIELD_BOTTOM - 1 - i,
                    (RIGHT - LEFT) as i32,
                    1,
                    edge,
                );
            }
        }
        if juice.glow > 0. {
            let a = juice.glow * juice.flash() * 0.14;
            self.rect(
                LEFT as i32,
                TOP as i32,
                (RIGHT - LEFT) as i32,
                548,
                alpha(VIOLET, a),
            );
        }
        self.graded = false;
        self.look = Default::default();
        self.draw_hud(g, presentation);
        match g.phase {
            Phase::Ready => self.ready_card(g, presentation, paused),
            Phase::Transition => self.transition_card(g, presentation, paused),
            Phase::Won | Phase::Lost => self.results_card(g, presentation, paused),
            Phase::Playing => {}
        }
        if paused {
            self.pause_card(g, presentation);
        }
    }

    // ---------------------------------------------------------------- backdrop

    fn build_background(&mut self) {
        self.background_look = Some(self.look);
        self.graded = true;
        self.shift = [0; 2];
        let pixels = self.frame.pixels_mut();
        for y in 0..HEIGHT as usize {
            let t = y as f32 / HEIGHT as f32;
            let c = mix([6, 13, 19, 255], [9, 18, 24, 255], t);
            pixels[y * WIDTH as usize..(y + 1) * WIDTH as usize].fill(c);
        }
        // Deep-space gradient inside the arena.
        for y in TOP as i32..FIELD_BOTTOM {
            let t = (y - TOP as i32) as f32 / 548.;
            let c = mix([11, 24, 33, 255], [17, 35, 43, 255], t);
            self.rect(LEFT as i32, y, (RIGHT - LEFT) as i32, 1, c);
        }
        // Soft nebulae: violet high left, teal low right, a faint warm core.
        for (cx, cy, r, color, strength) in [
            (170., 230., 230., [120, 96, 200, 255], 0.13),
            (480., 540., 270., [60, 170, 160, 255], 0.10),
            (330., 400., 140., [216, 173, 109, 255], 0.05),
        ] {
            self.soft_disc(cx, cy, r, color, strength);
        }
        for i in 0..150_u32 {
            let hash = i.wrapping_mul(2_654_435_761).rotate_left(11);
            let x = LEFT as i32 + 4 + (hash % 556) as i32;
            let y = TOP as i32 + 4 + ((hash >> 9) % 540) as i32;
            let level = 60 + (hash >> 20) % 120;
            let c = [
                level as u8 / 2 + 30,
                level as u8 / 2 + 50,
                level as u8 / 2 + 60,
                255,
            ];
            self.rect(x, y, 1, 1, alpha(c, 0.55 + (hash % 7) as f32 * 0.06));
            if hash % 23 == 0 {
                for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                    self.rect(x + dx, y + dy, 1, 1, alpha(c, 0.35));
                }
            }
        }
        for r in [100.0, 180.0, 260.0] {
            self.ring(320.0, 405.0, r, 1.2, [52, 82, 90, 120]);
        }
        self.line(
            320.0,
            TOP,
            320.0,
            FIELD_BOTTOM as f32 - 1.,
            [34, 56, 62, 70],
        );
        self.line(LEFT, 405.0, RIGHT, 405.0, [34, 56, 62, 70]);
        // Vignette, baked once: darker toward the arena's corners and edges.
        let pixels = self.frame.pixels_mut();
        for y in TOP as usize..FIELD_BOTTOM as usize {
            let ny = (y as f32 - 380.) / 300.;
            for x in LEFT as usize..RIGHT as usize {
                let nx = (x as f32 - 320.) / 300.;
                let d = (nx * nx + ny * ny).sqrt();
                let k = 1. - 0.42 * ((d - 0.55) / 0.75).clamp(0., 1.).powi(2);
                let p = &mut pixels[y * WIDTH as usize + x];
                for c in &mut p[..3] {
                    *c = (f32::from(*c) * k) as u8;
                }
            }
        }
        self.line(LEFT, TOP, LEFT, FIELD_BOTTOM as f32 - 1., LINE);
        self.line(RIGHT, TOP, RIGHT, FIELD_BOTTOM as f32 - 1., LINE);
        self.line(LEFT, TOP, RIGHT, TOP, LINE);
        for x in [LEFT, RIGHT] {
            self.round_rect(
                x as i32 - 2,
                TOP as i32,
                4,
                24,
                2,
                CYAN,
                scale_rgb(CYAN, 0.6),
            );
            self.round_rect(x as i32 - 2, 630, 4, 24, 2, scale_rgb(CYAN, 0.6), CYAN);
            for y in (360..420).step_by(12) {
                self.line(x - 3.0, y as f32, x + 3.0, y as f32 + 5.0, GOLD);
            }
        }
        self.background.copy_from_slice(self.frame.pixels());
    }

    fn soft_disc(&mut self, cx: f32, cy: f32, r: f32, color: [u8; 4], strength: f32) {
        let x0 = (cx - r).max(LEFT) as i32;
        let x1 = (cx + r).min(RIGHT) as i32;
        let y0 = (cy - r).max(TOP) as i32;
        let y1 = (cy + r).min(FIELD_BOTTOM as f32) as i32;
        for y in y0..y1 {
            for x in x0..x1 {
                let d = ((x as f32 - cx).hypot(y as f32 - cy) / r).min(1.);
                let a = (1. - d * d).powi(2) * strength;
                if a > 0.004 {
                    self.pixel(x, y, alpha(color, a));
                }
            }
        }
    }

    fn copy_background(&mut self) {
        let [sx, sy] = self.shift;
        let w = WIDTH as usize;
        let pixels = self.frame.pixels_mut();
        if sx == 0 && sy == 0 {
            pixels.copy_from_slice(&self.background);
            return;
        }
        for y in 0..HEIGHT as i32 {
            let src_y = (y - sy).clamp(0, HEIGHT as i32 - 1) as usize;
            let src = &self.background[src_y * w..(src_y + 1) * w];
            let dst = &mut pixels[y as usize * w..(y as usize + 1) * w];
            if sx >= 0 {
                let sx = sx as usize;
                dst[sx..].copy_from_slice(&src[..w - sx]);
                dst[..sx].fill(src[0]);
            } else {
                let sx = (-sx) as usize;
                dst[..w - sx].copy_from_slice(&src[sx..]);
                dst[w - sx..].fill(src[w - 1]);
            }
        }
    }

    fn draw_ambient(&mut self, g: &Game) {
        let moving = !g.policy.reduced_motion() && g.policy.intensity() > 0.;
        let t = if moving { g.time } else { 0. };
        // Little moons travel the orbit rings: the arena always feels alive.
        for (i, (r, speed, color)) in [
            (100., 0.35, CYAN),
            (180., -0.22, GOLD),
            (260., 0.14, VIOLET),
        ]
        .into_iter()
        .enumerate()
        {
            let angle = i as f32 * 2.1 + t * speed;
            let (x, y) = (320. + angle.cos() * r, 405. + angle.sin() * r);
            if y > TOP + 6. && y < FIELD_BOTTOM as f32 - 6. && x > LEFT + 4. && x < RIGHT - 4. {
                self.circle(x, y, 2.2, alpha(color, 0.75));
                self.glow(x, y, 6., color, 0.35);
            }
        }
        if moving {
            for i in 0..14_u32 {
                let hash = i.wrapping_mul(0x9e37_79b9).rotate_left(5);
                let x = LEFT + 8. + (hash % 548) as f32;
                let y = TOP + 8. + ((hash >> 10) % 520) as f32;
                let twinkle =
                    ((t * (1.3 + (i % 4) as f32 * 0.4) + i as f32).sin() * 0.5 + 0.5).powi(3);
                self.rect(x as i32, y as i32, 1, 1, alpha(PAPER, twinkle * 0.8));
            }
        }
        for x in [LEFT, RIGHT] {
            self.glow(x, TOP + 12., 10., CYAN, 0.6);
            self.glow(x, 642., 10., CYAN, 0.6);
        }
    }

    // ------------------------------------------------------------------ world

    fn draw_panels(&mut self, g: &Game, juice: &crate::juice::Juice) {
        let last = g.remaining() == 1 && g.phase == Phase::Playing;
        for block in g.blocks.iter().filter(|b| b.health > 0) {
            let x = (block.center[0] - block.half[0]) as i32;
            let y = (block.center[1] - block.half[1]) as i32;
            let w = (block.half[0] * 2.0) as i32;
            let h = (block.half[1] * 2.0) as i32;
            let shade = panel_color(block.row).map(|c| (c * 255.).round() as u8);
            let mut base = [shade[0], shade[1], shade[2], 255];
            if block.flash > 0.0 && !g.policy.reduced_flash() {
                base = mix(base, PAPER, g.policy.intensity());
                self.glow(block.center[0], block.center[1], 26., PAPER, 0.8);
            }
            self.round_rect(x + 2, y + 5, w, h, 5, [2, 8, 12, 150], [2, 8, 12, 90]);
            self.round_rect(x, y, w, h, 5, scale_rgb(base, 1.12), scale_rgb(base, 0.82));
            self.rect(x + 4, y + 1, w - 8, 1, [255, 250, 225, 110]);
            self.rect(x + 3, y + h - 4, w - 6, 2, [3, 18, 22, 50]);
            self.circle(x as f32 + 6.5, y as f32 + 6.5, 1.3, [9, 27, 30, 140]);
            self.circle(
                x as f32 + w as f32 - 6.5,
                y as f32 + 6.5,
                1.3,
                [9, 27, 30, 140],
            );
            if block.original_health == 2 {
                // Reinforced panels carry a riveted steel band until cracked.
                self.round_rect(
                    x + 3,
                    y + h / 2 - 3,
                    w - 6,
                    6,
                    2,
                    [70, 92, 98, 200],
                    [36, 52, 58, 200],
                );
                for bolt in [x + 10, x + w / 2, x + w - 10] {
                    self.circle(bolt as f32, (y + h / 2) as f32, 1.4, [222, 232, 220, 220]);
                }
                if block.health == 1 {
                    let (cx, top) = (block.center[0], y as f32);
                    self.line(cx, top + 2., cx - 4., top + 12., INK);
                    self.line(cx - 4., top + 12., cx + 3., top + h as f32 - 1., INK);
                    self.line(cx - 4., top + 12., cx - 13., top + 16., INK);
                    self.line(cx + 3., top + 20., cx + 12., top + 22., INK);
                }
            }
            if last {
                // Spotlight the final panel with pulsing corner brackets.
                let pulse = if juice.motion() > 0. {
                    ((juice.time * 7.).sin() * 0.5 + 0.5) * 4.
                } else {
                    2.
                } as i32;
                self.glow(block.center[0], block.center[1], 46., GOLD, 0.9);
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
        let panel = ([96, 128, 214, 255], [52, 70, 140, 255]);
        self.round_rect(x as i32 - 16, y as i32 - 3, 10, 7, 1, panel.0, panel.1);
        self.round_rect(x as i32 + 7, y as i32 - 3, 10, 7, 1, panel.0, panel.1);
        self.line(x - 6., y, x + 7., y, [150, 170, 230, 255]);
        self.round_rect(
            x as i32 - 5,
            y as i32 - 5,
            11,
            10,
            3,
            [236, 238, 248, 255],
            [160, 166, 190, 255],
        );
        self.line(x, y - 5., x + 3., y - 10., [210, 214, 230, 255]);
        if blink {
            self.circle(x + 3.5, y - 11., 1.6, CORAL);
            self.glow(x + 3.5, y - 11., 6., CORAL, 1.2);
        }
        self.glow(x, y, 14., VIOLET, 0.35);
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
            let mark = match drop.kind {
                Pickup::Wide => CYAN,
                Pickup::Split => VIOLET,
                Pickup::Sticky => LIME,
            };
            self.glow(x, y, 16., mark, 1.1);
            let (xi, yi) = (x as i32, y as i32);
            self.round_rect(
                xi - 10,
                yi - 8,
                20,
                16,
                6,
                scale_rgb(mark, 0.42),
                scale_rgb(mark, 0.22),
            );
            self.round_rect(xi - 8, yi - 7, 16, 2, 1, alpha(mark, 0.8), alpha(mark, 0.8));
            match drop.kind {
                Pickup::Wide => self.round_rect(xi - 6, yi - 1, 12, 4, 2, PAPER, mark),
                Pickup::Split => {
                    for dx in [-5., 0., 5.] {
                        self.circle(x + dx, y + if dx == 0. { -2. } else { 2. }, 2.1, PAPER);
                    }
                }
                Pickup::Sticky => {
                    self.round_rect(xi - 6, yi - 3, 12, 3, 1, PAPER, mark);
                    for dx in [-4, 0, 3] {
                        self.round_rect(xi + dx, yi - 1, 2, 3 + (dx & 3), 1, mark, PAPER);
                    }
                }
            }
            let shift = sprite as f32;
            self.line(x - 12.0 - shift, y - 4.0, x - 16.0 - shift, y, mark);
            self.line(x - 16.0 - shift, y, x - 12.0 - shift, y + 4.0, mark);
            self.line(x + 12.0 + shift, y - 4.0, x + 16.0 + shift, y, mark);
            self.line(x + 16.0 + shift, y, x + 12.0 + shift, y + 4.0, mark);
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
        let count = g.trail.len();
        for (i, p) in g.trail.iter().enumerate() {
            let t = (i + 1) as f32 / (count + 1) as f32;
            let core = mix(color, PAPER, t * t * 0.6);
            self.circle(
                p[0],
                p[1],
                (1.5 + t * 3.5) * heat,
                alpha(core, t * 0.42 * heat * g.policy.intensity()),
            );
            if i % 3 == 0 {
                self.glow(p[0], p[1], 7. * heat, color, 0.32 * t * heat);
            }
        }
    }

    fn draw_paddle(&mut self, g: &Game, presentation: &Presentation) {
        let (sx, sy) = presentation.juice.paddle_scale();
        let w = (g.paddle_half * 2.0 * sx) as i32;
        let h = (16. * sy).round() as i32;
        let x = (g.paddle_x - g.paddle_half * sx) as i32;
        let top = PADDLE_Y as i32 + 8 - h;
        let sticky = g.sticky > 0 || g.held.is_some();
        let stripe = if sticky { LIME } else { CYAN };
        // A pool of light on the arena floor follows the paddle.
        self.glow(g.paddle_x, PADDLE_Y + 4., g.paddle_half * 0.9, stripe, 0.45);
        self.round_rect(
            x + 3,
            top + 6,
            w,
            h - 1,
            h / 2,
            [2, 6, 10, 170],
            [2, 6, 10, 90],
        );
        self.round_rect(x, top, w, h, h / 2, [72, 108, 112, 255], [34, 54, 60, 255]);
        self.round_rect(x + 3, top + 1, w - 6, 3, 2, stripe, scale_rgb(stripe, 0.7));
        self.round_rect(
            x + 10,
            top + 5,
            w - 20,
            (5. * sy).max(2.) as i32,
            2,
            [214, 232, 220, 255],
            [160, 190, 180, 255],
        );
        self.circle((x + 7) as f32, (top + h - 5) as f32, 1.8, GOLD);
        self.circle((x + w - 7) as f32, (top + h - 5) as f32, 1.8, GOLD);
        for frac in [0.2, 0.5, 0.8] {
            self.glow(x as f32 + w as f32 * frac, top as f32 + 2., 7., stripe, 0.5);
        }
        if sticky {
            for (i, dx) in [0.18_f32, 0.42, 0.66, 0.86].into_iter().enumerate() {
                self.round_rect(
                    x + (w as f32 * dx) as i32,
                    top + 3,
                    2,
                    3 + (i as i32 % 2) * 2,
                    1,
                    LIME,
                    LIME,
                );
            }
        }
        let emphasis = presentation.feedback.impact().emphasis;
        if emphasis > 0.01 {
            self.round_rect(
                x,
                top - 3,
                w,
                (emphasis * 6.).ceil() as i32,
                2,
                [99, 215, 196, 130],
                [99, 215, 196, 30],
            );
        }
        if g.wide_time > 0.0 {
            let span = (w as f32 * g.wide_time / 9.0) as i32;
            self.round_rect(x, PADDLE_Y as i32 + 11, span.max(2), 2, 1, CYAN, CYAN);
        }
        if let Some(held) = g.held {
            // Aim preview: where the caught ball will fly on release.
            let speed = held.velocity[0].hypot(held.velocity[1]).max(1.);
            let dir = [held.velocity[0] / speed, held.velocity[1] / speed];
            for i in 1..8 {
                let d = i as f32 * 15.;
                let (px, py) = (g.ball[0] + dir[0] * d, g.ball[1] + dir[1] * d);
                self.circle(px, py, 1.8, alpha(LIME, 1. - i as f32 / 9.));
            }
        }
    }

    fn draw_ball(&mut self, at: [f32; 2], color: [u8; 4], core: [u8; 4]) {
        self.glow(at[0], at[1], 14., color, 1.6);
        self.circle(at[0], at[1], 10.0, alpha(color, 0.10));
        self.circle(at[0], at[1], 7.5, alpha(color, 0.30));
        self.circle(at[0], at[1], BALL_RADIUS - 0.2, core);
        self.circle(at[0] - 1.2, at[1] - 1.2, 2.4, [255, 255, 244, 255]);
    }

    fn draw_balls(&mut self, g: &Game) {
        if matches!(g.phase, Phase::Won | Phase::Lost | Phase::Transition) {
            return;
        }
        for ball in &g.extra_balls {
            self.draw_ball(ball.position, VIOLET, [226, 216, 255, 255]);
        }
        self.draw_ball(g.ball, Self::ball_color(g), PAPER);
    }

    fn callout(&mut self, g: &Game, presentation: &Presentation) {
        if let Some(callout) = &presentation.juice.callout {
            let a = callout.alpha();
            let motion = presentation.juice.motion();
            // Spring in: overshoot then settle (still fade-only when reduced).
            let pop = if motion > 0. {
                Ease::BackOut.apply((callout.age / 0.28).min(1.))
            } else {
                1.
            };
            let head = callout.head.as_str();
            let scale = if text_width(head, 2) <= 540 { 2 } else { 1 };
            let x = (WIDTH as i32 - text_width(head, scale)) / 2;
            let y = 540 + ((1. - pop) * 10.) as i32;
            self.glow(
                320.,
                y as f32 + 6.,
                text_width(head, scale) as f32 * 0.35,
                callout.color,
                0.55 * a,
            );
            self.text(head, x + 2, y + 2, scale, alpha(INK, a));
            self.text(head, x, y, scale, alpha(callout.color, a));
            let sub = callout.sub.as_str();
            self.mtext_center(
                sub,
                320,
                574,
                1,
                alpha(PAPER, a * 0.9),
                Case::Sentence,
                true,
            );
        } else if g.feedback() == Some(Feedback::BallSaved) {
            self.mtext_center("Ball saved", 320, 560, 1, CYAN, Case::AsIs, true);
        }
    }

    fn draw_transition(&mut self, presentation: &Presentation) {
        let screen = &presentation.screen;
        let coverage = screen.transition.coverage();
        if coverage <= 0. {
            return;
        }
        let (x0, x1) = (LEFT as i32 + 1, RIGHT as i32);
        let (top, bottom) = (TOP as i32 + 1, FIELD_BOTTOM);
        let height = (bottom - top) as f32;
        let fill = [7, 15, 21, 255];
        match screen.transition.kind(screen.motion) {
            TransitionKind::Fade => {
                self.rect(x0, top, x1 - x0, bottom - top, alpha(fill, coverage))
            }
            _ => {
                // A soft-edged band sweeps down, then uncovers from the top.
                let (from, to) = if screen.transition.phase() == TransitionPhase::Revealing {
                    (top as f32 + (1. - coverage) * height, bottom as f32)
                } else {
                    (top as f32, top as f32 + coverage * height)
                };
                self.rect(x0, from as i32, x1 - x0, (to - from) as i32, fill);
                let edge = if screen.transition.phase() == TransitionPhase::Revealing {
                    from
                } else {
                    to
                };
                for i in 0..10 {
                    let y = edge as i32
                        + if screen.transition.phase() == TransitionPhase::Revealing {
                            -i - 1
                        } else {
                            i
                        };
                    self.rect(x0, y, x1 - x0, 1, alpha(fill, 1. - i as f32 / 10.));
                }
                self.rect(x0, edge as i32, x1 - x0, 2, CYAN);
                for x in (x0..x1).step_by(48) {
                    self.glow(x as f32 + 24., edge, 18., CYAN, 0.8);
                }
            }
        }
    }

    // ------------------------------------------------------------------ bloom

    /// Splat emissive light into the quarter-resolution buffer.
    fn glow(&mut self, x: f32, y: f32, radius: f32, color: [u8; 4], intensity: f32) {
        let intensity = intensity * self.glow_gain * f32::from(color[3]) / 255.;
        if intensity <= 0.01 || !x.is_finite() || !y.is_finite() {
            return;
        }
        let cx = (x + self.shift[0] as f32) / 4.;
        let cy = (y + self.shift[1] as f32) / 4.;
        let r = (radius / 4.).max(0.75) + 0.75;
        let x0 = (cx - r).floor().max(0.) as usize;
        let y0 = (cy - r).floor().max(0.) as usize;
        let x1 = ((cx + r).ceil() as usize + 1).min(GW);
        let y1 = ((cy + r).ceil() as usize + 1).min(GH);
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let rgb = [
            f32::from(color[0]) * intensity,
            f32::from(color[1]) * intensity,
            f32::from(color[2]) * intensity,
        ];
        for gy in y0..y1 {
            for gx in x0..x1 {
                let d = ((gx as f32 + 0.5 - cx).hypot(gy as f32 + 0.5 - cy) / r).min(1.);
                let w = (1. - d) * (1. - d);
                if w <= 0. {
                    continue;
                }
                let cell = &mut self.glow[gy * GW + gx];
                for c in 0..3 {
                    cell[c] = cell[c].saturating_add((rgb[c] * w * 4.) as u16);
                }
            }
        }
        let b = &mut self.glow_box;
        *b = [b[0].min(x0), b[1].min(y0), b[2].max(x1), b[3].max(y1)];
    }

    /// Blur the dirty glow region (two running-sum box passes per axis, a
    /// cheap Gaussian approximation), screen-blend it into the frame and
    /// clear it for the next frame.
    fn composite_glow(&mut self) {
        let [bx0, by0, bx1, by1] = self.glow_box;
        if bx0 >= bx1 || by0 >= by1 {
            return;
        }
        let pad = 2 * BLUR + 1;
        let (x0, y0) = (bx0.saturating_sub(pad), by0.saturating_sub(pad));
        let (x1, y1) = ((bx1 + pad).min(GW), (by1 + pad).min(GH));
        let window = (2 * BLUR + 1) as u32;
        for _ in 0..2 {
            for gy in y0..y1 {
                self.box_pass(gy * GW + x0, 1, x1 - x0, window);
            }
            for gx in x0..x1 {
                self.box_pass(y0 * GW + gx, GW, y1 - y0, window);
            }
        }
        let pixels = self.frame.pixels_mut();
        let field = (
            LEFT as usize + 1,
            TOP as usize + 1,
            RIGHT as usize,
            FIELD_BOTTOM as usize,
        );
        for gy in y0..y1.saturating_sub(1) {
            for gx in x0..x1.saturating_sub(1) {
                let v00 = self.glow[gy * GW + gx];
                let v10 = self.glow[gy * GW + gx + 1];
                let v01 = self.glow[(gy + 1) * GW + gx];
                let v11 = self.glow[(gy + 1) * GW + gx + 1];
                let peak = [v00, v10, v01, v11]
                    .iter()
                    .map(|v| v[0].max(v[1]).max(v[2]))
                    .max()
                    .unwrap_or(0);
                if peak < 8 {
                    continue;
                }
                // Cell centers sit at 4n + 2; fill the 4x4 block between them.
                for j in 0..4_u32 {
                    let py = gy * 4 + 2 + j as usize;
                    if py < field.1 || py >= field.3 {
                        continue;
                    }
                    for i in 0..4_u32 {
                        let px = gx * 4 + 2 + i as usize;
                        if px < field.0 || px >= field.2 {
                            continue;
                        }
                        let (wx, wy) = (i * 64, j * 64);
                        let p = &mut pixels[py * WIDTH as usize + px];
                        for c in 0..3 {
                            let top = u32::from(v00[c]) * (256 - wx) + u32::from(v10[c]) * wx;
                            let bottom = u32::from(v01[c]) * (256 - wx) + u32::from(v11[c]) * wx;
                            let value = (top * (256 - wy) + bottom * wy) >> 16;
                            // Gain, then a screen blend so light never clips harshly.
                            let light = (value * 3 / 8).min(255);
                            let base = u32::from(p[c]);
                            p[c] = (base + light * (255 - base) / 255) as u8;
                        }
                    }
                }
            }
        }
        for gy in y0..y1 {
            self.glow[gy * GW + x0..gy * GW + x1].fill([0; 3]);
        }
        self.glow_box = [GW, GH, 0, 0];
    }

    fn box_pass(&mut self, start: usize, stride: usize, len: usize, window: u32) {
        let radius = BLUR as isize;
        for i in 0..len {
            let v = self.glow[start + i * stride];
            self.scratch[i] = [u32::from(v[0]), u32::from(v[1]), u32::from(v[2])];
        }
        let mut sum = [0_u32; 3];
        for k in 0..=radius.min(len as isize - 1) {
            for (c, value) in sum.iter_mut().enumerate() {
                *value += self.scratch[k as usize][c];
            }
        }
        for i in 0..len as isize {
            let out = &mut self.glow[start + i as usize * stride];
            for c in 0..3 {
                out[c] = (sum[c] / window).min(u32::from(u16::MAX)) as u16;
            }
            let add = i + radius + 1;
            let remove = i - radius;
            for (c, value) in sum.iter_mut().enumerate() {
                if add < len as isize {
                    *value += self.scratch[add as usize][c];
                }
                if remove >= 0 {
                    *value -= self.scratch[remove as usize][c];
                }
            }
        }
    }

    // -------------------------------------------------------------------- HUD

    fn chip(&mut self, x: i32, label: &str, color: [u8; 4]) -> i32 {
        let w = font::width(label, 1, Case::AsIs) + 18;
        self.round_rect(x, 82, w, 17, 8, alpha(color, 0.22), alpha(color, 0.12));
        self.circle(x as f32 + 7.5, 90.5, 2.4, color);
        self.mtext(label, x + 13, 94, 1, color, Case::AsIs, false);
        x + w + 6
    }

    fn draw_hud(&mut self, g: &Game, presentation: &Presentation) {
        let screen = &presentation.screen;
        let attract = g.phase == Phase::Ready && g.clears == 0 && g.lives == 3 && g.sector == 1;
        let wave = attract && screen.motion == ferravex_engine::play::tween::Motion::Full;
        // Logo: arcade face with a cyan underline; letters bob on the title.
        let mut x = 38;
        for (i, c) in "ORBIT BREAKER".chars().enumerate() {
            let dy = if wave {
                ((screen.clock * 3.2 - i as f32 * 0.45).sin() * 2.5).round() as i32
            } else {
                0
            };
            let mut buf = [0; 4];
            let s = c.encode_utf8(&mut buf);
            self.text(s, x + 2, 33 + dy + 2, 3, [2, 6, 10, 255]);
            self.text(s, x, 33 + dy, 3, PAPER);
            x += 18;
        }
        self.round_rect(38, 60, 54, 3, 1, CYAN, scale_rgb(CYAN, 0.6));
        let (name, _) = quips::sector(g.sector);
        let sector = line(format_args!("Sector {} \u{b7} ", g.sector));
        let w = font::width(sector.as_str(), 1, Case::AsIs);
        self.mtext(sector.as_str(), 98, 66, 1, MUTED, Case::AsIs, false);
        self.mtext(name, 98 + w, 66, 1, MUTED, Case::Sentence, false);

        let shown = screen.score.shown().max(0) as u32;
        let score = line(format_args!("{shown:05}"));
        self.text_right(score.as_str(), 602, 34, 3, [40, 26, 10, 255]);
        self.text_right(score.as_str(), 600, 32, 3, GOLD);
        let multiplier = g.chain.multiplier();
        if multiplier > 1 {
            let label = line(format_args!("\u{d7}{multiplier} chain"));
            let right = 600 - text_width(score.as_str(), 3) - 12;
            let w = font::width(label.as_str(), 1, Case::AsIs) + 16;
            self.round_rect(right - w, 35, w, 18, 9, alpha(HOT, 0.30), alpha(HOT, 0.14));
            self.mtext(label.as_str(), right - w + 8, 48, 1, HOT, Case::AsIs, false);
        }
        match presentation.best {
            Some(best) if g.score > best => {
                self.mtext_right("New best pace!", 600, 70, 1, HOT, Case::AsIs);
            }
            Some(best) => {
                let label = line(format_args!("Best {best}"));
                self.mtext_right(label.as_str(), 600, 70, 1, MUTED, Case::AsIs);
            }
            None => self.mtext_right("Score", 600, 70, 1, MUTED, Case::AsIs),
        }
        if g.phase == Phase::Playing {
            let mut x = 38;
            if g.armor_piercing() {
                x = self.chip(x, "Armor piercing", GOLD);
            }
            if g.wide_time > 0. {
                let label = line(format_args!("Wide {:.1}s", g.wide_time));
                x = self.chip(x, label.as_str(), CYAN);
            }
            if !g.extra_balls.is_empty() {
                let label = line(format_args!("Multi-ball \u{d7}{}", g.extra_balls.len() + 1));
                x = self.chip(x, label.as_str(), VIOLET);
            }
            if g.held.is_some() {
                let label = line(format_args!("{} to fire", presentation.hints.launch));
                self.chip(x, label.as_str(), LIME);
            } else if g.sticky > 0 {
                let label = line(format_args!("Sticky \u{d7}{}", g.sticky));
                self.chip(x, label.as_str(), LIME);
            }
            if g.volley >= 2 {
                let color = if g.volley >= 5 { HOT } else { GOLD };
                let label = line(format_args!("Volley {}", g.volley));
                self.mtext_right(label.as_str(), 600, 94, 1, color, Case::AsIs);
            }
        }
        // Footer: lives as glowing orbs, panels left, ball save, streak/keys.
        for i in 0..3 {
            let x = 47.0 + i as f32 * 18.0;
            if i < g.lives {
                self.circle(x, 684.0, 6., alpha(CYAN, 0.18));
                self.circle(x, 684.0, 4., PAPER);
                self.circle(x - 1., 683.0, 1.5, [255, 255, 255, 255]);
            } else {
                self.circle(x, 684.0, 3.5, LINE);
            }
        }
        let remaining = g.remaining();
        let panels = if remaining == 1 {
            line(format_args!("1 panel left!"))
        } else {
            line(format_args!("{remaining} panels left"))
        };
        let color = if remaining == 1 { GOLD } else { MUTED };
        self.mtext(panels.as_str(), 104, 688, 1, color, Case::AsIs, false);
        if g.ball_save > 0. && g.phase == Phase::Playing {
            self.mtext_center("Ball save", 320, 682, 1, CYAN, Case::AsIs, false);
            self.round_rect(286, 688, 68, 4, 2, LINE, LINE);
            let fill = (68. * g.ball_save / 2.5) as i32;
            self.round_rect(286, 688, fill.max(4), 4, 2, CYAN, scale_rgb(CYAN, 0.7));
        }
        if g.precision.streak() > 0 {
            let label = line(format_args!("Precise \u{d7}{}", g.precision.streak()));
            self.mtext_right(label.as_str(), 600, 688, 1, CYAN, Case::AsIs);
        } else {
            let pause = presentation.hints.pause.as_str();
            let restart = presentation.hints.restart.as_str();
            let width = self.keycap_width(pause) + self.keycap_width(restart) + 100;
            if width <= 240 {
                let mut x = 600 - width;
                x = self.keycap(x, 688, pause, alpha(PAPER, 0.7));
                self.mtext(" pause", x, 688, 1, DIM, Case::AsIs, false);
                x += font::width(" pause", 1, Case::AsIs) + 10;
                x = self.keycap(x, 688, restart, alpha(PAPER, 0.7));
                self.mtext(" restart", x, 688, 1, DIM, Case::AsIs, false);
            }
        }
    }

    // ------------------------------------------------------------------ cards

    /// A rounded card with a soft shadow; returns its eased vertical offset
    /// and alpha (cards fade up as they enter).
    fn card(&mut self, rect: [i32; 4], accent: [u8; 4], appear: (f32, i32)) {
        let [x, y, w, h] = rect;
        let (a, dy) = appear;
        let y = y + dy;
        for d in 1..=12 {
            let k = (1. - d as f32 / 12.).powi(2) * 0.30 * a;
            let spread = d;
            self.round_rect_outline(
                x - spread,
                y - spread + 6,
                w + 2 * spread,
                h + 2 * spread,
                12 + spread,
                alpha([0, 0, 0, 255], k),
            );
        }
        self.round_rect(
            x,
            y,
            w,
            h,
            12,
            alpha([58, 88, 96, 255], a),
            alpha([30, 48, 56, 255], a),
        );
        self.round_rect(
            x + 1,
            y + 1,
            w - 2,
            h - 2,
            11,
            alpha([17, 32, 41, 252], a),
            alpha([8, 17, 24, 252], a),
        );
        self.round_rect(
            WIDTH as i32 / 2 - 20,
            y + 12,
            40,
            3,
            1,
            alpha(accent, a),
            alpha(accent, a),
        );
    }

    fn ready_card(&mut self, g: &Game, presentation: &Presentation, paused: bool) {
        if paused {
            return;
        }
        let screen = &presentation.screen;
        let (name, tagline) = quips::sector(g.sector);
        let lost = g.feedback() == Some(Feedback::LifeLost);
        let restarted = g.feedback() == Some(Feedback::Restarted);
        let title = if lost {
            line(format_args!(
                "BALL LOST / {} {} LEFT",
                g.lives,
                if g.lives == 1 { "LIFE" } else { "LIVES" }
            ))
        } else if restarted {
            line(format_args!("FRESH RUN / 3 LIVES"))
        } else if g.lives_lost == 0 || g.clears == 0 || g.sector == 2 && g.remaining() == 35 {
            line(format_args!("SECTOR 0{} / {name}", g.sector))
        } else {
            line(format_args!("READY FOR LAUNCH"))
        };
        let (subtitle, sub_color) = if lost {
            (quips::drain(g.lives_lost), CORAL)
        } else if restarted {
            (quips::retry(presentation.restarts.max(1)), MUTED)
        } else {
            (tagline, MUTED)
        };
        self.card([96, 356, 448, 214], CYAN, screen.appear(0));
        let (a, dy) = screen.appear(1);
        self.text_fit_center(title.as_str(), 380 + dy, 2, 416, alpha(PAPER, a));
        let (a, dy) = screen.appear(2);
        self.mtext_center(
            subtitle,
            320,
            418 + dy,
            1,
            alpha(sub_color, a),
            Case::Sentence,
            false,
        );
        let power = g.serve_charge.power();
        let full = power >= 0.95;
        let (a, dy) = screen.appear(3);
        let bar = [176, 432 + dy, 288, 8];
        self.round_rect(
            bar[0],
            bar[1],
            bar[2],
            bar[3],
            4,
            alpha([28, 44, 52, 255], a),
            alpha([20, 34, 40, 255], a),
        );
        let fill = ((bar[2] as f32 * power) as i32).max(8);
        let (c0, c1) = if full {
            (HOT, GOLD)
        } else {
            (CYAN, scale_rgb(CYAN, 0.7))
        };
        self.round_rect(bar[0], bar[1], fill, bar[3], 4, alpha(c0, a), alpha(c1, a));
        if full {
            let glint = (screen.clock * 2.).fract();
            let gx = bar[0] + (glint * bar[2] as f32) as i32;
            if screen.motion == ferravex_engine::play::tween::Motion::Full {
                self.round_rect(
                    gx,
                    bar[1] + 1,
                    10,
                    3,
                    1,
                    alpha(PAPER, 0.6 * a),
                    alpha(PAPER, 0.2 * a),
                );
            }
        }
        let charge = if full {
            line(format_args!("Full charge \u{b7} armor piercing ready"))
        } else {
            line(format_args!("Charging serve {:.0}%", power * 100.))
        };
        self.mtext_center(
            charge.as_str(),
            320,
            458 + dy,
            1,
            alpha(if full { GOLD } else { MUTED }, a),
            Case::AsIs,
            false,
        );
        let (a, dy) = screen.appear(4);
        let pulse = if screen.motion == ferravex_engine::play::tween::Motion::Full {
            0.75 + 0.25 * (screen.clock * 4.).sin()
        } else {
            1.
        };
        self.prompt(
            320,
            494 + dy,
            &presentation.hints.launch,
            "to launch",
            a,
            pulse,
        );
        let (a, dy) = screen.appear(5);
        let juice = &presentation.juice;
        if !juice.steering_learned() {
            let fade = 1. - juice.steer_learned / 0.75;
            let hint = line(format_args!("Steer: {}", presentation.hints.movement));
            self.mtext_fit_center(
                hint.as_str(),
                528 + dy,
                416,
                alpha(PAPER, a * (0.45 + 0.55 * fade)),
            );
        } else if !juice.charge_learned {
            self.mtext_fit_center(
                "Tip: wait for a full charge to pierce armor",
                528 + dy,
                416,
                alpha(MUTED, a),
            );
        } else {
            self.mtext_fit_center(
                "Catch capsules \u{b7} chain panels \u{b7} find the secret",
                528 + dy,
                416,
                alpha(MUTED, a),
            );
        }
        let footer = line(format_args!(
            "{} {} \u{b7} {} panels",
            g.lives,
            if g.lives == 1 { "life" } else { "lives" },
            g.remaining()
        ));
        self.mtext_center(
            footer.as_str(),
            320,
            552 + dy,
            1,
            alpha(DIM, a),
            Case::AsIs,
            false,
        );
    }

    fn transition_card(&mut self, g: &Game, presentation: &Presentation, paused: bool) {
        if paused {
            return;
        }
        let screen = &presentation.screen;
        self.card([96, 356, 448, 214], GOLD, screen.appear(0));
        let (a, dy) = screen.appear(1);
        let title = line(format_args!("SECTOR 0{} SECURED", g.sector));
        self.text_fit_center(title.as_str(), 380 + dy, 2, 416, alpha(PAPER, a));
        let (a, dy) = screen.appear(2);
        if g.sector_lives_lost == 0 {
            let bonus = line(format_args!("FLAWLESS! +{}", crate::game::FLAWLESS_POINTS));
            self.halo(320, 416 + dy, text_width(bonus.as_str(), 2), HOT, a);
            self.text_fit_center(bonus.as_str(), 410 + dy, 2, 416, alpha(HOT, a));
        } else {
            self.mtext_center(
                "Not bad for a space janitor",
                320,
                422 + dy,
                1,
                alpha(MUTED, a),
                Case::AsIs,
                false,
            );
        }
        let (next, tagline) = quips::sector(g.sector + 1);
        let (a, dy) = screen.appear(3);
        let label = line(format_args!("Next: "));
        let w = font::width(label.as_str(), 1, Case::AsIs) + font::width(next, 1, Case::Sentence);
        self.mtext(
            label.as_str(),
            320 - w / 2,
            452 + dy,
            1,
            alpha(MUTED, a),
            Case::AsIs,
            false,
        );
        self.mtext(
            next,
            320 - w / 2 + font::width(label.as_str(), 1, Case::AsIs),
            452 + dy,
            1,
            alpha(GOLD, a),
            Case::Sentence,
            false,
        );
        let (a, dy) = screen.appear(4);
        self.mtext_center(
            "Reinforced panels need two hits",
            320,
            474 + dy,
            1,
            alpha(PAPER, a),
            Case::AsIs,
            false,
        );
        self.mtext_center(
            tagline,
            320,
            492 + dy,
            1,
            alpha(MUTED, a),
            Case::Sentence,
            false,
        );
        let (a, dy) = screen.appear(5);
        let countdown = line(format_args!(
            "Sector {} in {:.1}s",
            g.sector + 1,
            g.transition_remaining()
        ));
        self.mtext_center(
            countdown.as_str(),
            320,
            524 + dy,
            1,
            alpha(CYAN, a),
            Case::AsIs,
            false,
        );
        let progress = (1. - g.transition_remaining() / 1.4).clamp(0., 1.);
        self.round_rect(
            176,
            536 + dy,
            288,
            6,
            3,
            alpha([28, 44, 52, 255], a),
            alpha([20, 34, 40, 255], a),
        );
        self.round_rect(
            176,
            536 + dy,
            ((288. * progress) as i32).max(6),
            6,
            3,
            alpha(CYAN, a),
            alpha(scale_rgb(CYAN, 0.7), a),
        );
    }

    fn pause_card(&mut self, g: &Game, presentation: &Presentation) {
        let screen = &presentation.screen;
        // Dim the arena so the card owns attention.
        let (a0, _) = screen.appear(0);
        self.rect(
            LEFT as i32 + 1,
            TOP as i32 + 1,
            (RIGHT - LEFT) as i32 - 1,
            547,
            [4, 10, 14, (120. * a0) as u8],
        );
        self.card([96, 356, 448, 214], CYAN, screen.appear(0));
        let (a, dy) = screen.appear(1);
        self.text_center("PAUSED", 382 + dy, 3, alpha(PAPER, a));
        let (a, dy) = screen.appear(2);
        let status = line(format_args!(
            "Sector {} \u{b7} {} points \u{b7} {} {}",
            g.sector,
            g.score,
            g.lives,
            if g.lives == 1 { "life" } else { "lives" }
        ));
        self.mtext_center(
            status.as_str(),
            320,
            428 + dy,
            1,
            alpha(MUTED, a),
            Case::AsIs,
            false,
        );
        let (a, dy) = screen.appear(3);
        self.mtext_center(
            quips::pause(presentation.pauses),
            320,
            450 + dy,
            1,
            alpha(GOLD, a),
            Case::Sentence,
            false,
        );
        let (a, dy) = screen.appear(4);
        self.prompt(320, 494 + dy, &presentation.hints.pause, "to resume", a, 1.);
        let (a, dy) = screen.appear(5);
        self.prompt(
            320,
            536 + dy,
            &presentation.hints.restart,
            "to restart",
            a * 0.8,
            1.,
        );
    }

    fn confetti(&mut self, celebrate: f32) {
        const COLORS: [[u8; 4]; 5] = [GOLD, CYAN, VIOLET, LIME, HOT];
        for i in 0..64_u32 {
            let hash = i.wrapping_mul(2_654_435_761).rotate_left(7);
            let x0 = LEFT + (hash % 560) as f32;
            let speed = 50. + (hash >> 10) as f32 % 90.;
            let y = TOP - 20. + (celebrate * speed + (hash >> 4) as f32 % 560.) % 560.;
            let sway = (celebrate * 2.3 + i as f32).sin() * 9.;
            let spin = (celebrate * 6. + i as f32).sin();
            let (w, h) = (
                2 + (spin.abs() * 4.) as i32,
                2 + ((1. - spin.abs()) * 4.) as i32,
            );
            self.round_rect(
                (x0 + sway) as i32,
                y as i32,
                w,
                h,
                1,
                COLORS[i as usize % 5],
                COLORS[i as usize % 5],
            );
        }
    }

    fn results_card(&mut self, g: &Game, presentation: &Presentation, paused: bool) {
        let juice = &presentation.juice;
        let screen = &presentation.screen;
        let (a0, _) = screen.appear(0);
        self.rect(
            LEFT as i32 + 1,
            TOP as i32 + 1,
            (RIGHT - LEFT) as i32 - 1,
            547,
            [4, 10, 14, (200. * a0) as u8],
        );
        if paused {
            return;
        }
        if g.phase == Phase::Won && juice.motion() > 0. && juice.celebrate > 0. {
            self.confetti(juice.celebrate);
        }
        let won = g.phase == Phase::Won;
        self.card(
            [64, 150, 512, 474],
            if won { GOLD } else { CORAL },
            screen.appear(0),
        );
        let (a, dy) = screen.appear(1);
        self.text_center(
            if won { "FIELD CLEARED" } else { "SIGNAL LOST" },
            176 + dy,
            2,
            alpha(if won { CYAN } else { CORAL }, a),
        );
        let Some(results) = &presentation.results else {
            let summary = line(format_args!(
                "{} points \u{b7} {} panels cleared",
                g.score, g.clears
            ));
            self.mtext_center(summary.as_str(), 320, 230, 1, MUTED, Case::AsIs, false);
            self.prompt(320, 566, &presentation.hints.launch, "to retry", 1., 1.);
            return;
        };
        let (a, dy) = screen.appear(2);
        self.mtext_center(
            "Your title",
            320,
            216 + dy,
            1,
            alpha(MUTED, a),
            Case::AsIs,
            false,
        );
        let (a, dy) = screen.appear(3);
        self.halo(
            320,
            236 + dy,
            text_width(results.rank.title, 3).min(464),
            GOLD,
            a * 0.8,
        );
        self.text_fit_center(results.rank.title, 224 + dy, 3, 464, alpha(GOLD, a));
        self.mtext_center(
            results.rank.quip,
            320,
            268 + dy,
            1,
            alpha(PAPER, a),
            Case::Sentence,
            false,
        );
        // The score rolls up as the card lands.
        let (a, dy) = screen.appear(4);
        let roll = if screen.motion == ferravex_engine::play::tween::Motion::Full {
            Ease::CubicOut.apply(((screen.age - 0.2) / 0.9).clamp(0., 1.))
        } else {
            1.
        };
        let shown = line(format_args!("{}", (g.score as f32 * roll).round() as u32));
        self.text_center(shown.as_str(), 284 + dy, 4, alpha(PAPER, a));
        self.mtext_center(
            "points",
            320,
            328 + dy,
            1,
            alpha(MUTED, a),
            Case::AsIs,
            false,
        );
        let (a, dy) = screen.appear(5);
        match (results.new_best, results.previous_best) {
            (true, previous) => {
                let pulse = if juice.motion() > 0. {
                    (screen.clock * 5.).sin() * 0.2 + 0.8
                } else {
                    1.
                };
                self.halo(
                    320,
                    350 + dy,
                    text_width("NEW PERSONAL BEST!", 2),
                    HOT,
                    a * pulse,
                );
                self.text_center("NEW PERSONAL BEST!", 343 + dy, 2, alpha(HOT, a * pulse));
                if let Some(previous) = previous {
                    let old = line(format_args!("Old best {previous}"));
                    self.mtext_center(
                        old.as_str(),
                        320,
                        374 + dy,
                        1,
                        alpha(MUTED, a),
                        Case::AsIs,
                        false,
                    );
                }
            }
            (false, Some(best)) => {
                let label = line(format_args!("Personal best {best}"));
                self.mtext_center(
                    label.as_str(),
                    320,
                    352 + dy,
                    1,
                    alpha(GOLD, a),
                    Case::AsIs,
                    false,
                );
                let gap = line(format_args!(
                    "{} more points to beat it",
                    best.saturating_sub(g.score).saturating_add(1)
                ));
                self.mtext_center(
                    gap.as_str(),
                    320,
                    370 + dy,
                    1,
                    alpha(MUTED, a),
                    Case::AsIs,
                    false,
                );
            }
            (false, None) => {}
        }
        let stats: [(&str, Line); 8] = [
            ("Time", line(format_args!("{:.0}s", g.time))),
            ("Panels", line(format_args!("{}", g.clears))),
            ("Best volley", line(format_args!("{}", g.best_volley))),
            ("Best precise", line(format_args!("{}", g.precision.best()))),
            ("Ball saves", line(format_args!("{}", g.saves))),
            ("Satellites", line(format_args!("{}", g.satellites))),
            ("Flawless", line(format_args!("{}", g.flawless_sectors))),
            ("Balls lost", line(format_args!("{}", g.lives_lost))),
        ];
        let (a, dy) = screen.appear(6);
        self.rect(104, 386 + dy, 432, 1, alpha(LINE, a));
        for (i, (label, value)) in stats.iter().enumerate() {
            let (a, dy) = screen.appear(6 + i / 2);
            let column = (i % 2) as i32;
            let row = (i / 2) as i32;
            let x = 108 + column * 224;
            let y = 408 + row * 21 + dy;
            if column == 0 {
                self.round_rect(
                    104,
                    y - 13,
                    432,
                    19,
                    6,
                    alpha([255, 255, 255, 8 + 6 * (row as u8 % 2)], a),
                    alpha([255, 255, 255, 8 + 6 * (row as u8 % 2)], a),
                );
            }
            self.mtext(label, x + 4, y, 1, alpha(MUTED, a), Case::AsIs, false);
            self.mtext_right(value.as_str(), x + 196, y, 1, alpha(PAPER, a), Case::AsIs);
        }
        let (a, dy) = screen.appear(10);
        self.rect(104, 488 + dy, 432, 1, alpha(LINE, a));
        if let Some((amount, unit, title)) = quips::next_goal(results.summary) {
            let next = line(format_args!(
                "Next title: {title} in {amount} {}",
                if unit == "POINTS" { "points" } else { "panels" }
            ));
            self.mtext_fit_center(next.as_str(), 508 + dy, 464, alpha(CYAN, a));
        } else {
            self.mtext_fit_center(
                "Top title earned. The orbit is yours.",
                508 + dy,
                464,
                alpha(CYAN, a),
            );
        }
        if g.satellites == 0 {
            self.mtext_fit_center(
                "Psst\u{2026} something orbits above the panels",
                526 + dy,
                464,
                alpha(VIOLET, a),
            );
        }
        let (a, dy) = screen.appear(11);
        let pulse = if screen.motion == ferravex_engine::play::tween::Motion::Full {
            0.75 + 0.25 * (screen.clock * 4.).sin()
        } else {
            1.
        };
        self.prompt(
            320,
            568 + dy,
            &presentation.hints.launch,
            "to retry",
            a,
            pulse,
        );
        self.mtext_center(
            "One more orbit?",
            320,
            604 + dy,
            1,
            alpha(DIM, a),
            Case::AsIs,
            false,
        );
    }

    /// A soft additive-looking halo behind important text (HUD layer).
    fn halo(&mut self, cx: i32, cy: i32, width: i32, color: [u8; 4], a: f32) {
        for (spread, k) in [(14, 0.05), (9, 0.07), (4, 0.09)] {
            let w = width + spread * 2;
            self.round_rect(
                cx - w / 2,
                cy - 8 - spread / 2,
                w,
                16 + spread,
                8 + spread / 2,
                alpha(color, k * a),
                alpha(color, k * a),
            );
        }
    }

    // ---------------------------------------------------------- key prompts

    fn keycap_width(&self, key: &str) -> i32 {
        font::width(key, 1, Case::AsIs) + 12
    }

    /// A rounded key cap with the binding name; returns the next x.
    fn keycap(&mut self, x: i32, baseline: i32, key: &str, color: [u8; 4]) -> i32 {
        let w = self.keycap_width(key);
        let a = f32::from(color[3]) / 255.;
        self.round_rect(
            x,
            baseline - 13,
            w,
            18,
            5,
            alpha([60, 88, 96, 255], a),
            alpha([40, 60, 68, 255], a),
        );
        self.round_rect(
            x + 1,
            baseline - 12,
            w - 2,
            15,
            4,
            alpha([30, 50, 58, 255], a),
            alpha([20, 34, 42, 255], a),
        );
        self.mtext(key, x + 6, baseline, 1, color, Case::AsIs, false);
        x + w
    }

    /// `[SPACE] [ENTER] [TAP] to launch`, centered; long remaps fall back to text.
    fn prompt(&mut self, cx: i32, baseline: i32, keys: &str, action: &str, a: f32, pulse: f32) {
        let scale = 2;
        let action_w = font::width(action, scale, Case::AsIs);
        let mut caps = 0;
        let mut width = action_w + 10;
        for key in keys.split(" / ") {
            width += self.keycap_width(key) + 6;
            caps += 1;
        }
        if caps > 4 || width > 440 || keys.is_empty() {
            let text = line(format_args!("{keys} {action}"));
            self.mtext_fit_center(text.as_str(), baseline, 440, alpha(CYAN, a * pulse));
            return;
        }
        let mut x = cx - width / 2;
        for key in keys.split(" / ") {
            x = self.keycap(x, baseline - 2, key, alpha(PAPER, a)) + 6;
        }
        self.mtext(
            action,
            x + 4,
            baseline + 4,
            scale,
            alpha(CYAN, a * pulse),
            Case::AsIs,
            true,
        );
    }

    // ------------------------------------------------------------ typography

    /// Modern-face text with its baseline at `baseline`; returns the width.
    #[allow(clippy::too_many_arguments)]
    fn mtext(
        &mut self,
        text: &str,
        x: i32,
        baseline: i32,
        scale: i32,
        color: [u8; 4],
        case: Case,
        shadow: bool,
    ) -> i32 {
        if shadow {
            self.mtext_raw(
                text,
                x + scale,
                baseline + scale,
                scale,
                alpha(INK, f32::from(color[3]) / 255. * 0.85),
                case,
            );
        }
        self.mtext_raw(text, x, baseline, scale, color, case)
    }

    fn mtext_raw(
        &mut self,
        text: &str,
        mut x: i32,
        baseline: i32,
        scale: i32,
        color: [u8; 4],
        case: Case,
    ) -> i32 {
        let start = x;
        let top = baseline - font::ASCENT * scale;
        let mut previous: Option<char> = None;
        for c in font::cased(text, case) {
            if let Some(p) = previous {
                x += font::kerning(p, c) * scale;
            }
            previous = Some(c);
            let Some(g) = font::glyph(c) else {
                x += 4 * scale;
                continue;
            };
            let w = i32::from(g.width);
            for (row, bits) in g.rows.iter().enumerate() {
                if *bits == 0 {
                    continue;
                }
                for col in 0..w {
                    if bits & (1 << (w - 1 - col)) != 0 {
                        self.rect(
                            x + col * scale,
                            top + row as i32 * scale,
                            scale,
                            scale,
                            color,
                        );
                    }
                }
            }
            x += (w + 1) * scale;
        }
        x - start
    }

    #[allow(clippy::too_many_arguments)]
    fn mtext_center(
        &mut self,
        text: &str,
        cx: i32,
        baseline: i32,
        scale: i32,
        color: [u8; 4],
        case: Case,
        shadow: bool,
    ) {
        let w = font::width(text, scale, case);
        self.mtext(text, cx - w / 2, baseline, scale, color, case, shadow);
    }

    fn mtext_right(
        &mut self,
        text: &str,
        right: i32,
        baseline: i32,
        scale: i32,
        color: [u8; 4],
        case: Case,
    ) {
        let w = font::width(text, scale, case);
        self.mtext(text, right - w, baseline, scale, color, case, false);
    }

    /// Centered Modern copy; drops trailing characters with an ellipsis when
    /// it would exceed `width`.
    fn mtext_fit_center(&mut self, text: &str, baseline: i32, width: i32, color: [u8; 4]) {
        if font::width(text, 1, Case::AsIs) <= width {
            self.mtext_center(text, 320, baseline, 1, color, Case::AsIs, false);
            return;
        }
        let mut end = text.len();
        while end > 0 {
            end = text[..end].char_indices().last().map_or(0, |(i, _)| i);
            let fitted = line(format_args!("{}\u{2026}", &text[..end]));
            if font::width(fitted.as_str(), 1, Case::AsIs) <= width {
                self.mtext_center(fitted.as_str(), 320, baseline, 1, color, Case::AsIs, false);
                return;
            }
        }
    }

    fn text_fit_center(&mut self, text: &str, y: i32, scale: i32, width: i32, color: [u8; 4]) {
        let scale = if scale > 1 && text_width(text, scale) > width {
            1
        } else {
            scale
        };
        let fitted = fit_label(text, scale, width);
        self.text_center(
            &fitted,
            y + scale,
            scale,
            alpha([2, 6, 10, 255], f32::from(color[3]) / 255.),
        );
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

    // -------------------------------------------------------------- shapes

    fn tint(&self, color: [u8; 4]) -> [u8; 4] {
        let shaded = self
            .look
            .shade(Color::from_rgba(color[0], color[1], color[2], color[3]))
            .to_rgba();
        if self.graded {
            [
                self.grade[0][shaded[0] as usize],
                self.grade[1][shaded[1] as usize],
                self.grade[2][shaded[2] as usize],
                shaded[3],
            ]
        } else {
            shaded
        }
    }
    fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: [u8; 4]) {
        if color[3] == 0 {
            return;
        }
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

    /// Anti-aliased rounded rectangle with a vertical gradient. Rows outside
    /// the corners are spans (fast opaque fills); corner pixels get coverage.
    #[allow(clippy::too_many_arguments)]
    fn round_rect(
        &mut self,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        r: i32,
        top: [u8; 4],
        bottom: [u8; 4],
    ) {
        if w <= 0 || h <= 0 || top[3] == 0 && bottom[3] == 0 {
            return;
        }
        let r = r.min(w / 2).min(h / 2).max(0);
        let rf = r as f32;
        for row in 0..h {
            let color = if h > 1 {
                mix(top, bottom, row as f32 / (h - 1) as f32)
            } else {
                top
            };
            let dy = if row < r {
                rf - row as f32 - 0.5
            } else if row >= h - r {
                row as f32 + 0.5 - (h - r) as f32
            } else {
                -1.
            };
            if dy < 0. {
                self.rect(x, y + row, w, 1, color);
                continue;
            }
            // Corner rows: inner span plus per-pixel coverage near the arc.
            let half = (rf * rf - dy * dy).max(0.).sqrt();
            let inset = (rf - half).floor() as i32;
            self.rect(x + inset + 1, y + row, w - 2 * inset - 2, 1, color);
            for col in 0..=inset.min(w / 2) {
                let dx = rf - col as f32 - 0.5;
                let coverage = (rf + 0.5 - dx.hypot(dy)).clamp(0., 1.);
                if coverage > 0. {
                    let c = alpha(color, coverage);
                    self.rect(x + col, y + row, 1, 1, c);
                    self.rect(x + w - 1 - col, y + row, 1, 1, c);
                }
            }
        }
    }

    /// One-pixel anti-aliased rounded outline (used for soft shadows).
    fn round_rect_outline(&mut self, x: i32, y: i32, w: i32, h: i32, r: i32, color: [u8; 4]) {
        if color[3] == 0 {
            return;
        }
        let r = r.min(w / 2).min(h / 2);
        self.rect(x + r, y, w - 2 * r, 1, color);
        self.rect(x + r, y + h - 1, w - 2 * r, 1, color);
        self.rect(x, y + r, 1, h - 2 * r, color);
        self.rect(x + w - 1, y + r, 1, h - 2 * r, color);
        let steps = (r * 2).max(4);
        for i in 0..=steps {
            let t = i as f32 / steps as f32 * std::f32::consts::FRAC_PI_2;
            let (ox, oy) = ((r as f32 - 0.5) * t.cos(), (r as f32 - 0.5) * t.sin());
            for (cx, cy, sx, sy) in [
                (x + r, y + r, -1., -1.),
                (x + w - 1 - r, y + r, 1., -1.),
                (x + r, y + h - 1 - r, -1., 1.),
                (x + w - 1 - r, y + h - 1 - r, 1., 1.),
            ] {
                self.rect(cx + (ox * sx) as i32, cy + (oy * sy) as i32, 1, 1, color);
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
        if color[3] == 0 {
            return;
        }
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
    /// Anti-aliased ring of the given thickness, scanning only its band.
    fn ring(&mut self, x: f32, y: f32, r: f32, thickness: f32, color: [u8; 4]) {
        if color[3] == 0 {
            return;
        }
        let (x, y) = (x + self.shift[0] as f32, y + self.shift[1] as f32);
        let color = self.tint(color);
        let half = thickness * 0.5;
        let outer = r + half + 1.;
        for py in (y - outer) as i32..=(y + outer) as i32 {
            let dy = py as f32 + 0.5 - y;
            if dy.abs() > outer {
                continue;
            }
            let span_out = (outer * outer - dy * dy).max(0.).sqrt();
            let inner = (r - half - 1.).max(0.);
            let span_in = if dy.abs() < inner {
                (inner * inner - dy * dy).sqrt()
            } else {
                0.
            };
            for side in [-1., 1.] {
                let (a, b) = if side < 0. {
                    (x - span_out, x - span_in)
                } else {
                    (x + span_in, x + span_out)
                };
                for px in a as i32..=b as i32 {
                    let d = (px as f32 + 0.5 - x).hypot(dy);
                    let coverage = (half + 0.5 - (d - r).abs()).clamp(0., 1.);
                    if coverage > 0. {
                        let mut c = color;
                        c[3] = (f32::from(c[3]) * coverage) as u8;
                        self.pixel(px, py, c);
                    }
                }
            }
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
