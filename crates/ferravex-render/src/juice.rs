//! Presentation "juice": bursts, sparkles, confetti, shockwave rings, floating
//! popups, toasts, screen shake and flashes that make accepted events *felt*
//! instead of described.
//!
//! [`Juice`] is a bounded, allocation-free (after construction) effect pool
//! advanced by presentation frame time. It never feeds back into simulation:
//! request effects only from events your rules actually accepted (a hit that
//! landed, an order that shipped), then call [`Juice::update`] once per
//! rendered frame and draw it in the same camera space as the game.
//!
//! Effects that cannot fit replace the oldest effect of the same kind, so a
//! storm of events never grows memory. [`Motion::Reduced`] disables shake,
//! keeps popups still and thins particles; [`Juice::set_reduced_flash`] keeps
//! full-screen flashes and vignettes subtle.
//!
//! ```
//! use ferravex_render::{Color, juice::Juice, vec2, icons::Icon};
//! let mut fx = Juice::new();
//! let gold = Color::from_hex(0xf7b553);
//! fx.burst(vec2(200., 120.), gold, 24);
//! fx.ring(vec2(200., 120.), gold, 48.);
//! fx.popup_icon(vec2(200., 100.), Icon::Coin, "+25", gold);
//! fx.shake(0.4);
//! assert!(fx.counts().sparks > 0 && fx.counts().popups == 1);
//! for _ in 0..240 {
//!     fx.update(1. / 60.);
//! }
//! assert!(fx.is_idle());
//! assert_eq!(fx.shake_offset(), vec2(0., 0.));
//! ```
use crate::icons::{self, Icon};
use crate::shapes::{self, Fill};
use crate::text::{TextAlign, TextStyle, draw_text_styled, text_width};
use crate::{Color, Rect, Vec2, vec2};
use ferravex_play::tween::{Ease, Motion};
use std::f32::consts::TAU;

/// Default number of simultaneous particles.
pub const DEFAULT_SPARKS: usize = 384;
/// Default number of simultaneous floating popups.
pub const DEFAULT_POPUPS: usize = 24;
/// Default number of simultaneous rings.
pub const DEFAULT_RINGS: usize = 24;
/// Longest popup or toast text kept, in bytes (cut at a character boundary).
pub const MAX_LABEL: usize = 40;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SparkKind {
    Dot,
    Star,
    Confetti,
    Puff,
}

#[derive(Clone, Copy, Debug)]
struct Spark {
    kind: SparkKind,
    pos: Vec2,
    vel: Vec2,
    gravity: f32,
    drag: f32,
    size: f32,
    color: Color,
    angle: f32,
    spin: f32,
    age: f32,
    life: f32,
}

#[derive(Clone, Copy, Debug)]
struct Label {
    bytes: [u8; MAX_LABEL],
    len: u8,
}

impl Label {
    fn new(text: &str) -> Self {
        let mut end = text.len().min(MAX_LABEL);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        let mut bytes = [0; MAX_LABEL];
        bytes[..end].copy_from_slice(&text.as_bytes()[..end]);
        Self {
            bytes,
            len: end as u8,
        }
    }
    fn as_str(&self) -> &str {
        // Constructed from a valid prefix ending on a character boundary.
        std::str::from_utf8(&self.bytes[..self.len as usize]).unwrap_or("")
    }
}

#[derive(Clone, Copy, Debug)]
struct Popup {
    pos: Vec2,
    label: Label,
    icon: Option<Icon>,
    color: Color,
    size: f32,
    age: f32,
    life: f32,
}

#[derive(Clone, Copy, Debug)]
struct Ring {
    pos: Vec2,
    radius: f32,
    /// Starting stroke width; `None` scales with the radius.
    width: Option<f32>,
    color: Color,
    age: f32,
    life: f32,
}

#[derive(Clone, Copy, Debug)]
struct Toast {
    label: Label,
    icon: Option<Icon>,
    color: Color,
    age: f32,
    life: f32,
}

/// Live effect counts, for diagnostics and tests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct JuiceCounts {
    pub sparks: usize,
    pub popups: usize,
    pub rings: usize,
    pub toast: bool,
}

/// Bounded pool of presentation effects. See the [module docs](self).
#[derive(Clone, Debug)]
pub struct Juice {
    sparks: Vec<Spark>,
    popups: Vec<Popup>,
    rings: Vec<Ring>,
    caps: [usize; 3],
    /// Next slot to recycle when a pool is full.
    victims: [usize; 3],
    toast: Option<Toast>,
    trauma: f32,
    flash: Color,
    vignette: Color,
    time: f32,
    seed: u32,
    motion: Motion,
    reduced_flash: bool,
    words: bool,
}

impl Default for Juice {
    fn default() -> Self {
        Self::new()
    }
}

impl Juice {
    /// A pool with the default capacities.
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_SPARKS, DEFAULT_POPUPS, DEFAULT_RINGS)
    }
    /// A pool holding at most the given numbers of live effects (each at least 1).
    pub fn with_capacity(sparks: usize, popups: usize, rings: usize) -> Self {
        let caps = [sparks.max(1), popups.max(1), rings.max(1)];
        Self {
            sparks: Vec::with_capacity(caps[0]),
            popups: Vec::with_capacity(caps[1]),
            rings: Vec::with_capacity(caps[2]),
            caps,
            victims: [0; 3],
            toast: None,
            trauma: 0.,
            flash: Color::new(0., 0., 0., 0.),
            vignette: Color::new(0., 0., 0., 0.),
            time: 0.,
            seed: 0x9e37_79b9,
            motion: Motion::Full,
            reduced_flash: false,
            words: true,
        }
    }
    /// Select full or reduced motion; reduced removes shake and drift.
    pub fn set_motion(&mut self, motion: Motion) {
        self.motion = motion;
        if motion.is_reduced() {
            self.trauma = 0.;
        }
    }
    /// Allow or ignore words: popups, callouts and toasts. Disable them for a
    /// background demonstration (for example behind a title screen) where
    /// floating text would collide with foreground copy; bursts, rings,
    /// confetti, shake and flashes still play.
    ///
    /// ```
    /// use ferravex_render::{Color, juice::Juice, vec2};
    /// let mut fx = Juice::new();
    /// fx.set_words(false);
    /// fx.popup(vec2(10., 10.), "+5", Color::new(1., 1., 1., 1.));
    /// fx.burst(vec2(10., 10.), Color::new(1., 1., 1., 1.), 4);
    /// assert_eq!(fx.counts().popups, 0);
    /// assert!(fx.counts().sparks > 0);
    /// ```
    pub fn set_words(&mut self, enabled: bool) {
        self.words = enabled;
        if !enabled {
            self.popups.clear();
            self.toast = None;
        }
    }
    /// Keep flashes and vignettes faint for players who reduce flashing.
    pub fn set_reduced_flash(&mut self, reduced: bool) {
        self.reduced_flash = reduced;
    }
    /// Remove floating words (popups, callouts and the toast) while keeping
    /// particles and rings, for example when a page or tab changes under them.
    ///
    /// ```
    /// use ferravex_render::{Color, juice::Juice, vec2};
    /// let mut fx = Juice::new();
    /// fx.callout(vec2(10., 10.), "PERFECT", Color::new(1., 1., 1., 1.));
    /// fx.ring(vec2(10., 10.), Color::new(1., 1., 1., 1.), 20.);
    /// fx.clear_words();
    /// assert_eq!((fx.counts().popups, fx.counts().rings), (0, 1));
    /// ```
    pub fn clear_words(&mut self) {
        self.popups.clear();
        self.toast = None;
        self.victims[1] = 0;
    }
    /// Remove every live effect (for example on restart).
    pub fn clear(&mut self) {
        self.sparks.clear();
        self.popups.clear();
        self.rings.clear();
        self.victims = [0; 3];
        self.toast = None;
        self.trauma = 0.;
        self.flash.a = 0.;
        self.vignette.a = 0.;
    }
    /// True when nothing is visible and nothing is moving.
    pub fn is_idle(&self) -> bool {
        self.sparks.is_empty()
            && self.popups.is_empty()
            && self.rings.is_empty()
            && self.toast.is_none()
            && self.trauma <= 0.
            && self.flash.a <= 0.
            && self.vignette.a <= 0.
    }
    /// Live effect counts.
    pub fn counts(&self) -> JuiceCounts {
        JuiceCounts {
            sparks: self.sparks.len(),
            popups: self.popups.len(),
            rings: self.rings.len(),
            toast: self.toast.is_some(),
        }
    }

    fn random(&mut self) -> f32 {
        // xorshift32: presentation variety only, never game state.
        let mut x = self.seed;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.seed = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }
    fn spread(&mut self, amount: f32) -> f32 {
        (self.random() * 2. - 1.) * amount
    }
    fn scaled_count(&self, count: u32) -> u32 {
        if self.motion.is_reduced() {
            count.div_ceil(4)
        } else {
            count
        }
    }
    fn push_spark(&mut self, spark: Spark) {
        if !(spark.pos.is_finite() && spark.vel.is_finite() && spark.life > 0.) {
            return;
        }
        push(&mut self.sparks, self.caps[0], &mut self.victims[0], spark);
    }

    /// A radial spray of `count` round sparks from `at`, as for a hit, a
    /// collected pickup or a completed task.
    pub fn burst(&mut self, at: Vec2, color: Color, count: u32) {
        for _ in 0..self.scaled_count(count.min(256)) {
            let angle = self.random() * TAU;
            let speed = 70. + self.random() * 190.;
            let life = 0.35 + self.random() * 0.45;
            let size = 1.8 + self.random() * 2.6;
            let tint = self.random() * 0.35;
            let spark = Spark {
                kind: SparkKind::Dot,
                pos: at,
                vel: vec2(angle.cos(), angle.sin()) * speed,
                gravity: 220.,
                drag: 3.2,
                size,
                color: shapes::mix(color, Color::new(1., 1., 1., color.a), tint),
                angle: 0.,
                spin: 0.,
                age: 0.,
                life,
            };
            self.push_spark(spark);
        }
    }
    /// Soft rising, swelling puffs: steam, smoke, dust and breath. Unlike
    /// [`Juice::burst`] they drift upward and grow as they fade.
    ///
    /// ```
    /// use ferravex_render::{Color, juice::Juice, vec2};
    /// let mut fx = Juice::new();
    /// fx.puff(vec2(120., 200.), Color::new(0.9, 0.95, 1., 0.7), 8);
    /// let before = fx.counts().sparks;
    /// assert!(before > 0);
    /// ```
    pub fn puff(&mut self, at: Vec2, color: Color, count: u32) {
        for _ in 0..self.scaled_count(count.min(64)) {
            let spark = Spark {
                kind: SparkKind::Puff,
                pos: at + vec2(self.spread(8.), self.spread(4.)),
                vel: vec2(self.spread(22.), -26. - self.random() * 34.),
                gravity: -12.,
                drag: 1.6,
                size: 4. + self.random() * 4.,
                color,
                angle: 0.,
                spin: 0.,
                age: -self.random() * 0.12,
                life: 0.7 + self.random() * 0.6,
            };
            self.push_spark(spark);
        }
    }
    /// A few twinkling four-point stars around `at`: rewards, selections, magic.
    pub fn sparkle(&mut self, at: Vec2, color: Color) {
        for _ in 0..self.scaled_count(7) {
            let offset = vec2(self.spread(22.), self.spread(18.));
            let spark = Spark {
                kind: SparkKind::Star,
                pos: at + offset,
                vel: vec2(self.spread(12.), -18. - self.random() * 30.),
                gravity: 0.,
                drag: 1.5,
                size: 4. + self.random() * 5.,
                color,
                angle: self.random() * 0.6,
                spin: self.spread(2.),
                age: -self.random() * 0.18,
                life: 0.55 + self.random() * 0.35,
            };
            self.push_spark(spark);
        }
    }
    /// A celebratory shower of multicolored paper from `at`.
    pub fn confetti(&mut self, at: Vec2, count: u32) {
        const COLORS: [u32; 6] = [0xf7b553, 0xe5484d, 0x5bd5d8, 0x8fd16a, 0xb78cf2, 0xfff3d6];
        for i in 0..self.scaled_count(count.min(256)) {
            let angle = -std::f32::consts::FRAC_PI_2 + self.spread(1.1);
            let speed = 180. + self.random() * 260.;
            let spark = Spark {
                kind: SparkKind::Confetti,
                pos: at + vec2(self.spread(30.), self.spread(8.)),
                vel: vec2(angle.cos(), angle.sin()) * speed,
                gravity: 340.,
                drag: 2.4,
                size: 3. + self.random() * 3.,
                color: Color::from_hex(COLORS[i as usize % COLORS.len()]),
                angle: self.random() * TAU,
                spin: self.spread(14.),
                age: 0.,
                life: 1.4 + self.random() * 1.,
            };
            self.push_spark(spark);
        }
    }
    /// An expanding shockwave that reaches `radius` and fades. Its stroke
    /// starts at 18% of the radius; use [`Juice::ring_width`] for large,
    /// thin reach indicators.
    pub fn ring(&mut self, at: Vec2, color: Color, radius: f32) {
        self.push_ring(at, color, radius, None);
    }
    /// An expanding ring whose stroke starts at `width` units and thins out,
    /// for wide areas (a hearing radius, a blast zone) that should stay crisp.
    ///
    /// ```
    /// use ferravex_render::{Color, juice::Juice, vec2};
    /// let mut fx = Juice::new();
    /// fx.ring_width(vec2(200., 200.), Color::new(1., 0.8, 0.3, 1.), 180., 3.);
    /// assert_eq!(fx.counts().rings, 1);
    /// ```
    pub fn ring_width(&mut self, at: Vec2, color: Color, radius: f32, width: f32) {
        if width.is_finite() && width > 0. {
            self.push_ring(at, color, radius, Some(width));
        }
    }
    fn push_ring(&mut self, at: Vec2, color: Color, radius: f32, width: Option<f32>) {
        if !(at.is_finite() && radius.is_finite() && radius > 0.) {
            return;
        }
        let ring = Ring {
            pos: at,
            radius,
            width,
            color,
            age: 0.,
            life: if radius > 120. { 0.6 } else { 0.45 },
        };
        push(&mut self.rings, self.caps[2], &mut self.victims[2], ring);
    }
    /// Short floating text (a number, a grade) that pops in at `at`, rises
    /// and fades. Keep it to a few characters: "+25", "x3", "PERFECT".
    pub fn popup(&mut self, at: Vec2, text: &str, color: Color) {
        self.push_popup(at, None, text, color, 18.);
    }
    /// A floating icon with optional short text beside it, such as a coin
    /// and "+25" rising from the tile that earned it.
    pub fn popup_icon(&mut self, at: Vec2, icon: Icon, text: &str, color: Color) {
        self.push_popup(at, Some(icon), text, color, 18.);
    }
    /// A popup at an explicit text `size` (popups default to 18, callouts to
    /// 32), with an optional icon; small sizes suit crowded boards.
    ///
    /// ```
    /// use ferravex_render::{Color, juice::Juice, vec2, icons::Icon};
    /// let mut fx = Juice::new();
    /// fx.popup_sized(vec2(50., 50.), Some(Icon::Bomb), "-4", Color::new(1., 0.4, 0.3, 1.), 12.);
    /// assert_eq!(fx.counts().popups, 1);
    /// ```
    pub fn popup_sized(
        &mut self,
        at: Vec2,
        icon: Option<Icon>,
        text: &str,
        color: Color,
        size: f32,
    ) {
        if size.is_finite() && size > 0. {
            self.push_popup(at, icon, text, color, size.clamp(6., 64.));
        }
    }
    /// A large floating callout for rare, important moments.
    pub fn callout(&mut self, at: Vec2, text: &str, color: Color) {
        self.push_popup(at, None, text, color, 32.);
    }
    fn push_popup(&mut self, at: Vec2, icon: Option<Icon>, text: &str, color: Color, size: f32) {
        if !at.is_finite() || !self.words {
            return;
        }
        // Stack fresh popups at the same spot instead of overprinting them.
        let mut pos = at;
        for other in &self.popups {
            if other.age < 0.25 && (other.pos - pos).length() < 14. {
                pos.y -= size * 1.1;
            }
        }
        let popup = Popup {
            pos,
            label: Label::new(text),
            icon,
            color,
            size,
            age: 0.,
            life: if size > 24. { 1.5 } else { 1.05 },
        };
        push(&mut self.popups, self.caps[1], &mut self.victims[1], popup);
    }
    /// Replace the current toast: a short line with an optional icon, shown
    /// by [`Juice::draw_toast`] for about two and a half seconds.
    pub fn toast(&mut self, icon: Option<Icon>, text: &str, color: Color) {
        if !self.words {
            return;
        }
        self.toast = Some(Toast {
            label: Label::new(text),
            icon,
            color,
            age: 0.,
            life: 2.6,
        });
    }
    /// Add camera trauma (0–1); shake grows with its square and decays.
    pub fn shake(&mut self, trauma: f32) {
        if trauma.is_finite() && !self.motion.is_reduced() {
            self.trauma = (self.trauma + trauma.max(0.)).min(1.);
        }
    }
    /// Tint the whole area with `color`; its alpha is the starting strength.
    pub fn flash(&mut self, color: Color) {
        if color.a.is_finite() && color.a > self.flash.a {
            self.flash = color;
        }
    }
    /// Glow the edges of the area with `color` (damage, danger, alarms).
    pub fn vignette(&mut self, color: Color) {
        if color.a.is_finite() && color.a > self.vignette.a {
            self.vignette = color;
        }
    }

    /// Advance every effect by `dt` seconds of presentation time.
    pub fn update(&mut self, dt: f32) {
        let dt = if dt.is_finite() {
            dt.clamp(0., 0.1)
        } else {
            0.
        };
        self.time += dt;
        let still = self.motion.is_reduced();
        self.sparks.retain_mut(|s| {
            s.age += dt;
            if s.age < 0. {
                return true;
            }
            if !still || s.kind == SparkKind::Confetti {
                s.vel *= (-s.drag * dt).exp();
                s.vel.y += s.gravity * dt;
                s.pos += s.vel * dt;
            }
            s.angle += s.spin * dt;
            s.age < s.life
        });
        self.popups.retain_mut(|p| {
            p.age += dt;
            p.age < p.life
        });
        self.rings.retain_mut(|r| {
            r.age += dt;
            r.age < r.life
        });
        if let Some(t) = &mut self.toast {
            t.age += dt;
            if t.age >= t.life {
                self.toast = None;
            }
        }
        self.victims = [0; 3];
        self.trauma = (self.trauma - dt * 1.8).max(0.);
        self.flash.a = (self.flash.a - dt * 2.2).max(0.);
        self.vignette.a = (self.vignette.a - dt * 1.6).max(0.);
    }

    /// Camera offset (logical units) for the current shake; add it to the
    /// game camera and keep HUD chrome steady.
    pub fn shake_offset(&self) -> Vec2 {
        if self.trauma <= 0. {
            return Vec2::ZERO;
        }
        let amount = 7. * self.trauma * self.trauma;
        let t = self.time;
        vec2(
            (t * 63.).sin() * 0.6 + (t * 97.).sin() * 0.4,
            (t * 71.).cos() * 0.6 + (t * 109.).cos() * 0.4,
        ) * amount
    }

    /// Draw sparks, rings and popups in the current camera's units.
    pub fn draw(&self) {
        for r in &self.rings {
            let t = (r.age / r.life).clamp(0., 1.);
            let grow = Ease::CubicOut.apply(t);
            let alpha = r.color.a * (1. - t) * (1. - t);
            let width = (r.width.unwrap_or(r.radius * 0.18) * (1. - t)).max(1.);
            shapes::stroke_circle(
                r.pos,
                r.radius * (0.25 + 0.75 * grow),
                width,
                with_alpha(r.color, alpha),
            );
        }
        for s in &self.sparks {
            if s.age < 0. {
                continue;
            }
            let t = (s.age / s.life).clamp(0., 1.);
            let fade = 1. - t * t;
            match s.kind {
                SparkKind::Dot => {
                    let radius = s.size * (1. - 0.6 * t);
                    shapes::fill_circle(s.pos, radius, with_alpha(s.color, s.color.a * fade));
                }
                SparkKind::Puff => {
                    // Swell and soften: a bright core inside a fading halo.
                    let radius = s.size * (1. + 1.6 * t);
                    let a = s.color.a * (1. - t) * (1. - t);
                    shapes::fill_circle(s.pos, radius, with_alpha(s.color, a * 0.55));
                    shapes::fill_circle(s.pos, radius * 0.6, with_alpha(s.color, a * 0.45));
                }
                SparkKind::Star => {
                    // Twinkle: grow in, shrink out.
                    let scale = (t * std::f32::consts::PI).sin();
                    star(
                        s.pos,
                        s.size * scale,
                        s.angle,
                        with_alpha(s.color, s.color.a * fade),
                    );
                }
                SparkKind::Confetti => {
                    let flutter = (s.angle * 1.7).cos().abs().max(0.2);
                    let (sin, cos) = s.angle.sin_cos();
                    let ax = vec2(cos, sin) * s.size * 1.4;
                    let ay = vec2(-sin, cos) * s.size * 0.7 * flutter;
                    let tail = if t > 0.7 { (1. - t) / 0.3 } else { 1. };
                    shapes::fill_polygon(
                        &[
                            s.pos - ax - ay,
                            s.pos + ax - ay,
                            s.pos + ax + ay,
                            s.pos - ax + ay,
                        ],
                        with_alpha(s.color, s.color.a * tail),
                    );
                }
            }
        }
        for p in &self.popups {
            let t = (p.age / p.life).clamp(0., 1.);
            let pop = Ease::BackOut.apply((p.age / 0.22).clamp(0., 1.));
            let rise = if self.motion.is_reduced() {
                0.
            } else {
                Ease::CubicOut.apply(t) * p.size * 1.8
            };
            let alpha = if t > 0.6 { 1. - (t - 0.6) / 0.4 } else { 1. } * p.color.a;
            let size = p.size * (0.55 + 0.45 * pop);
            let at = p.pos - vec2(0., rise);
            let text = p.label.as_str();
            let style = TextStyle::modern(size, with_alpha(p.color, alpha))
                .with_shadow(Color::new(0., 0., 0., 0.55 * alpha))
                .with_align(TextAlign::Left)
                .with_pixel_snap(false);
            let text_w = if text.is_empty() {
                0.
            } else {
                text_width(text, style)
            };
            let icon_w = if p.icon.is_some() { size * 1.15 } else { 0. };
            let gap = if p.icon.is_some() && !text.is_empty() {
                size * 0.2
            } else {
                0.
            };
            let left = at.x - (icon_w + gap + text_w) * 0.5;
            if let Some(icon) = p.icon {
                icons::draw_styled(
                    icon,
                    vec2(left + icon_w * 0.5, at.y),
                    icon_w,
                    icons::IconStyle::new(with_alpha(p.color, alpha)).shadow(Color::new(
                        0.,
                        0.,
                        0.,
                        0.45 * alpha,
                    )),
                );
            }
            if !text.is_empty() {
                draw_text_styled(
                    text,
                    left + icon_w + gap,
                    style.centered_baseline(at.y),
                    style,
                );
            }
        }
    }

    /// Draw the current flash and edge vignette over `area`.
    pub fn draw_flash(&self, area: Rect) {
        let limit = if self.reduced_flash { 0.12 } else { 1. };
        if self.flash.a > 0.003 {
            shapes::fill_rounded_rect(area, 0., with_alpha(self.flash, self.flash.a.min(limit)));
        }
        let a = self.vignette.a.min(limit * 0.9);
        if a > 0.003 {
            let edge = area.w.min(area.h) * 0.2;
            let solid = with_alpha(self.vignette, a);
            let clear = with_alpha(self.vignette, 0.);
            let top = Rect::new(area.x, area.y, area.w, edge);
            let bottom = Rect::new(area.x, area.y + area.h - edge, area.w, edge);
            let left = Rect::new(area.x, area.y, edge, area.h);
            let right = Rect::new(area.x + area.w - edge, area.y, edge, area.h);
            shapes::fill_rounded_rect(top, 0., Fill::vertical(top, solid, clear));
            shapes::fill_rounded_rect(bottom, 0., Fill::vertical(bottom, clear, solid));
            shapes::fill_rounded_rect(left, 0., Fill::horizontal(left, solid, clear));
            shapes::fill_rounded_rect(right, 0., Fill::horizontal(right, clear, solid));
        }
    }

    /// Draw the current toast as a pill centered on `anchor`.
    pub fn draw_toast(&self, anchor: Vec2) {
        let Some(toast) = &self.toast else {
            return;
        };
        let t = toast.age / toast.life;
        let enter = Ease::BackOut.apply((toast.age / 0.3).clamp(0., 1.));
        let alpha = if t > 0.8 { 1. - (t - 0.8) / 0.2 } else { 1. }.clamp(0., 1.);
        let slide = if self.motion.is_reduced() {
            0.
        } else {
            (1. - enter) * 18.
        };
        let text = toast.label.as_str();
        let style = TextStyle::modern(16., with_alpha(Color::new(0.96, 0.97, 0.98, 1.), alpha));
        let text_w = text_width(text, style);
        let icon_w = if toast.icon.is_some() { 22. } else { 0. };
        let w = text_w + icon_w + if toast.icon.is_some() { 30. } else { 24. };
        let h = 34.;
        let r = Rect::new(anchor.x - w * 0.5, anchor.y - h * 0.5 + slide, w, h);
        shapes::drop_shadow(
            r,
            h * 0.5,
            shapes::Shadow::soft(Color::new(0., 0., 0., 0.45 * alpha))
                .offset(vec2(0., 4.))
                .blur(14.),
        );
        shapes::fill_rounded_rect(r, h * 0.5, Color::new(0.06, 0.08, 0.11, 0.92 * alpha));
        shapes::stroke_rounded_rect(r, h * 0.5, 1.5, with_alpha(toast.color, 0.8 * alpha));
        let mut x = r.x + 12.;
        if let Some(icon) = toast.icon {
            icons::draw(
                icon,
                vec2(x + icon_w * 0.5, r.y + h * 0.5),
                icon_w,
                with_alpha(toast.color, alpha),
            );
            x += icon_w + 6.;
        }
        draw_text_styled(text, x, style.centered_baseline(r.y + h * 0.5), style);
    }
}

fn push<T>(pool: &mut Vec<T>, cap: usize, victim: &mut usize, item: T) {
    if pool.len() < cap {
        pool.push(item);
    } else {
        // Replace the oldest surviving entries first (insertion order).
        let slot = *victim % cap;
        pool[slot] = item;
        *victim = slot + 1;
    }
}

fn with_alpha(c: Color, a: f32) -> Color {
    Color::new(c.r, c.g, c.b, a.clamp(0., 1.))
}

/// A four-point twinkle star of outer radius `r`.
fn star(at: Vec2, r: f32, angle: f32, color: Color) {
    if r <= 0.2 {
        return;
    }
    let (s, c) = angle.sin_cos();
    let ax = vec2(c, s);
    let ay = vec2(-s, c);
    let thin = r * 0.28;
    shapes::fill_polygon(
        &[at - ax * r, at - ay * thin, at + ax * r, at + ay * thin],
        color,
    );
    shapes::fill_polygon(
        &[at - ay * r, at + ax * thin, at + ay * r, at - ax * thin],
        color,
    );
    shapes::fill_circle(
        at,
        thin * 0.9,
        with_alpha(Color::new(1., 1., 1., 1.), color.a * 0.8),
    );
}

/// A smooth 0–1 pulse at `hz` cycles per second, for idle "look at me" motion.
///
/// ```
/// assert!(ferravex_render::juice::pulse(0., 1.).abs() < 1e-5);
/// assert!((ferravex_render::juice::pulse(0.5, 1.) - 1.).abs() < 1e-5);
/// ```
pub fn pulse(time: f32, hz: f32) -> f32 {
    0.5 + 0.5 * (time * hz * TAU - std::f32::consts::FRAC_PI_2).sin()
}

/// A vertical bob offset of `amplitude` units at `hz`, for floating pickups,
/// markers and selected pieces.
///
/// ```
/// assert_eq!(ferravex_render::juice::bob(0., 1., 4.), 0.);
/// ```
pub fn bob(time: f32, hz: f32, amplitude: f32) -> f32 {
    (time * hz * TAU).sin() * amplitude
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shapes::tests::capture;

    fn white() -> Color {
        Color::new(1., 1., 1., 1.)
    }

    #[test]
    fn pools_are_bounded_and_recycle_oldest() {
        let mut fx = Juice::with_capacity(16, 3, 2);
        for i in 0..10 {
            fx.burst(vec2(i as f32, 0.), white(), 40);
            fx.popup(vec2(0., i as f32 * 100.), "+1", white());
            fx.ring(vec2(0., 0.), white(), 20.);
        }
        let c = fx.counts();
        assert_eq!((c.sparks, c.popups, c.rings), (16, 3, 2));
        // Newest popup survives replacement.
        assert!(fx.popups.iter().any(|p| p.pos.y == 900.));
    }

    #[test]
    fn effects_expire_and_shake_settles() {
        let mut fx = Juice::new();
        fx.burst(vec2(10., 10.), white(), 30);
        fx.sparkle(vec2(10., 10.), white());
        fx.confetti(vec2(10., 10.), 40);
        fx.puff(vec2(10., 10.), white(), 10);
        fx.ring(vec2(10., 10.), white(), 40.);
        fx.popup_icon(vec2(10., 10.), Icon::Coin, "+5", white());
        fx.callout(vec2(10., 10.), "PERFECT", white());
        fx.toast(Some(Icon::Check), "Order shipped", white());
        fx.shake(2.);
        fx.flash(Color::new(1., 1., 1., 0.5));
        fx.vignette(Color::new(1., 0., 0., 0.6));
        assert!(!fx.is_idle());
        fx.update(1. / 60.);
        assert_ne!(fx.shake_offset(), Vec2::ZERO);
        assert!(fx.shake_offset().length() <= 7. * std::f32::consts::SQRT_2 + 0.01);
        for _ in 0..60 * 4 {
            fx.update(1. / 60.);
        }
        assert!(fx.is_idle(), "{:?}", fx.counts());
        assert_eq!(fx.shake_offset(), Vec2::ZERO);
    }

    #[test]
    fn reduced_motion_removes_shake_and_thins_particles() {
        let mut fx = Juice::new();
        fx.set_motion(Motion::Reduced);
        fx.shake(1.);
        assert_eq!(fx.shake_offset(), Vec2::ZERO);
        fx.burst(vec2(0., 0.), white(), 40);
        assert_eq!(fx.counts().sparks, 10);
        let before = fx.sparks[0].pos;
        fx.update(0.1);
        assert_eq!(fx.sparks[0].pos, before, "reduced-motion sparks stay put");
    }

    #[test]
    fn invalid_requests_are_ignored_and_labels_truncate_on_char_boundaries() {
        let mut fx = Juice::new();
        fx.burst(vec2(f32::NAN, 0.), white(), 10);
        fx.ring(vec2(0., 0.), white(), -1.);
        fx.popup(vec2(f32::INFINITY, 0.), "x", white());
        fx.shake(f32::NAN);
        fx.update(f32::NAN);
        assert!(fx.is_idle());
        let long = "é".repeat(40);
        let label = Label::new(&long);
        assert!(label.as_str().len() <= MAX_LABEL && label.as_str().chars().all(|c| c == 'é'));
        assert_eq!(Label::new("+25").as_str(), "+25");
    }

    #[test]
    fn drawing_every_effect_is_finite() {
        let mut fx = Juice::new();
        fx.burst(vec2(100., 100.), white(), 20);
        fx.sparkle(vec2(100., 100.), white());
        fx.confetti(vec2(100., 100.), 20);
        fx.puff(vec2(100., 100.), white(), 6);
        fx.ring(vec2(100., 100.), white(), 30.);
        fx.ring_width(vec2(100., 100.), white(), 90., 2.);
        fx.popup_icon(vec2(100., 100.), Icon::Star, "+10", white());
        fx.toast(Some(Icon::Alert), "Careful", white());
        fx.flash(Color::new(1., 1., 1., 0.4));
        fx.vignette(Color::new(1., 0., 0., 0.4));
        fx.update(0.05);
        let v = capture(320, 240, || {
            fx.draw();
            fx.draw_flash(Rect::new(0., 0., 320., 240.));
            fx.draw_toast(vec2(160., 200.));
        });
        assert!(!v.is_empty());
        assert!(v.iter().flatten().all(|x| x.is_finite()));
    }

    #[test]
    fn pulse_and_bob_are_periodic() {
        assert!(pulse(0., 2.).abs() < 1e-5);
        assert!((pulse(0.25, 2.) - 1.).abs() < 1e-5);
        assert!((pulse(0.5, 1.) - 1.).abs() < 1e-5);
        assert!((bob(0.25, 1., 3.) - 3.).abs() < 1e-4);
    }
}
