//! World rendering: the reef, its light, the horde and every projectile.
//!
//! All shapes go through the batched [`Painter`]; nothing here mutates the game.
//! Positions interpolate between the last two fixed ticks with `alpha`.
//! With the post-processing pipeline, [`draw`] is the "world" pass: its glow
//! stream is the emissive input for bloom, and [`darkness`] becomes a vignette.
use crate::fx::Fx;
use crate::gfx::{Painter, mix3, rgba, scale3, with_alpha};
use crate::sim::content::*;
use crate::sim::{
    Foe, Game, Mote, PickupKind, Shape, ShotKind, aura_radius, orbit_radius, weapon_stats,
};
use ferravex_render::prelude::{Camera2D, Vec2, vec2};

/// Reference framing: roughly this many world units are visible on any screen.
const FRAME_AREA: f32 = 1150. * 720.;

#[derive(Clone, Copy, Debug)]
pub struct View {
    pub center: Vec2,
    pub scale: f32,
    pub screen: Vec2,
    pub half: Vec2,
}

impl View {
    pub fn new(screen: Vec2, center: Vec2, zoom: f32) -> Self {
        let screen = screen.max(Vec2::ONE);
        let scale = ((screen.x * screen.y / FRAME_AREA).sqrt() * zoom).clamp(0.3, 4.);
        Self {
            center,
            scale,
            screen,
            half: screen * 0.5 / scale,
        }
    }
    pub fn camera(&self) -> Camera2D {
        Camera2D {
            target: self.center,
            zoom: vec2(
                2. * self.scale / self.screen.x,
                2. * self.scale / self.screen.y,
            ),
            ..Camera2D::default()
        }
    }
    pub fn to_screen(self, p: Vec2) -> Vec2 {
        (p - self.center) * self.scale + self.screen * 0.5
    }
    pub fn visible(&self, p: Vec2, margin: f32) -> bool {
        let d = (p - self.center).abs();
        d.x < self.half.x + margin && d.y < self.half.y + margin
    }
}

/// Current blended tide palette: (deep, floor, light).
pub fn palette(fx: &Fx) -> (Rgb, Rgb, Rgb) {
    let a = &TIDES[fx.tide_from];
    let b = &TIDES[fx.tide_to];
    let t = crate::gfx::ease::in_out_sine(fx.tide_t);
    (
        mix3(a.deep, b.deep, t),
        mix3(a.floor, b.floor, t),
        mix3(a.light, b.light, t),
    )
}

#[inline]
fn hash(x: i32, y: i32, salt: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x8da6_b343)
        ^ (y as u32).wrapping_mul(0xd816_3841)
        ^ salt.wrapping_mul(0xcb1a_b31f);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    (h & 0x00ff_ffff) as f32 / 16_777_216.
}

#[inline]
fn lerp(a: Vec2, b: Vec2, t: f32) -> Vec2 {
    a + (b - a) * t
}

pub struct Scene<'a> {
    pub game: &'a Game,
    pub fx: &'a Fx,
    pub view: View,
    pub alpha: f32,
    pub time: f32,
    /// 0..=1 multiplier for hit flashes/blinks (reduced flash = 0).
    pub flashes: f32,
    pub motion: f32,
}

/// Draw the whole world pass. The caller has set the world camera.
pub fn draw(p: &mut Painter, s: &Scene) {
    let crowd = s.game.foes.len();
    p.lod = if crowd > 380 { 2 } else { 1 };
    let (deep, floor, light) = palette(s.fx);
    backdrop(p, s, deep, floor, light);
    decor(p, s, floor, light);
    p.flush();
    darkness(p, s, deep);
    p.flush_alpha();
    hazards(p, s, light);
    motes(p, s);
    pickups(p, s);
    shadows(p, s);
    foes(p, s);
    player(p, s);
    weapons(p, s);
    bullets(p, s);
    rings(p, s);
    p.flush();
}

fn backdrop(p: &mut Painter, s: &Scene, deep: Rgb, floor: Rgb, light: Rgb) {
    let _ = deep;
    let v = s.view;
    let min = v.center - v.half - Vec2::splat(4.);
    let size = v.half * 2. + Vec2::splat(8.);
    p.rect_v(
        false,
        min,
        size,
        rgba(mix3(deep, floor, 0.25), 1.),
        rgba(mix3(deep, floor, 0.45), 1.),
    );
    // Low-frequency sand patches give the floor large-scale shape.
    let patch = 560.;
    let px0 = ((v.center.x - v.half.x) / patch).floor() as i32 - 1;
    let px1 = ((v.center.x + v.half.x) / patch).ceil() as i32 + 1;
    let py0 = ((v.center.y - v.half.y) / patch).floor() as i32 - 1;
    let py1 = ((v.center.y + v.half.y) / patch).ceil() as i32 + 1;
    for cy in py0..=py1 {
        for cx in px0..=px1 {
            let h = hash(cx, cy, 17);
            let at = vec2(cx as f32 + hash(cx, cy, 18), cy as f32 + hash(cx, cy, 19)) * patch;
            let r = 260. + 220. * h;
            let tint = if h > 0.5 {
                mix3(floor, light, 0.18)
            } else {
                scale3(deep, 0.6)
            };
            p.ellipse(
                false,
                at,
                r * 1.3,
                r,
                h * 3.,
                rgba(tint, 0.16),
                rgba(tint, 0.),
                32,
            );
        }
    }
    // Big lantern light pool on the sea floor.
    let player = lerp(s.game.player.prev, s.game.player.pos, s.alpha);
    p.glow(player, 560., rgba(mix3(floor, light, 0.45), 0.2));
    // Caustic light drifting across the floor.
    let cell = 420.;
    let x0 = ((v.center.x - v.half.x) / cell).floor() as i32 - 1;
    let x1 = ((v.center.x + v.half.x) / cell).ceil() as i32 + 1;
    let y0 = ((v.center.y - v.half.y) / cell).floor() as i32 - 1;
    let y1 = ((v.center.y + v.half.y) / cell).ceil() as i32 + 1;
    for cy in y0..=y1 {
        for cx in x0..=x1 {
            let h = hash(cx, cy, 11);
            let base = vec2(cx as f32 + hash(cx, cy, 3), cy as f32 + hash(cx, cy, 5)) * cell;
            let drift = vec2(
                (s.time * 0.21 + h * 6.).sin(),
                (s.time * 0.17 + h * 9.).cos(),
            ) * 60.;
            let pulse = 0.6 + 0.4 * (s.time * 0.7 + h * 12.).sin();
            p.glow(base + drift, 170. + 90. * h, rgba(light, 0.03 * pulse));
        }
    }
    // Sunken light shafts, slowly swaying (parallax: they hang above the floor).
    let shaft_dir = vec2(0.42, 1.).normalize();
    let across = vec2(shaft_dir.y, -shaft_dir.x);
    let anchor = v.center * 0.55;
    let spacing = 330.;
    let base_k = ((v.center - anchor).dot(across) / spacing).floor() as i32;
    let reach = v.half.length() + 200.;
    for k in base_k - 4..=base_k + 4 {
        let h = hash(k, 0, 77);
        if h < 0.35 {
            continue;
        }
        let sway = (s.time * 0.13 + h * 20.).sin() * 40.;
        let mid = anchor
            + across * (k as f32 * spacing + sway)
            + shaft_dir * (v.center - anchor).dot(shaft_dir);
        let half_w = 40. + 70. * h;
        let pulse = 0.55 + 0.45 * (s.time * 0.4 + h * 30.).sin();
        let a = 0.05 * pulse;
        let top = mid - shaft_dir * reach;
        let bottom = mid + shaft_dir * reach;
        let c = rgba(light, a);
        let z = rgba(light, 0.);
        p.quad(
            true,
            [top - across * half_w, top, bottom, bottom - across * half_w],
            [z, c, c, z],
        );
        p.quad(
            true,
            [top, top + across * half_w, bottom + across * half_w, bottom],
            [c, z, z, c],
        );
    }
    // Sand ripples: world-anchored wavy bands.
    let spacing = 46.;
    let r0 = ((v.center.y - v.half.y) / spacing).floor() as i32 - 1;
    let r1 = ((v.center.y + v.half.y) / spacing).ceil() as i32 + 1;
    let seg = 64.;
    let xs = ((v.center.x - v.half.x) / seg).floor() as i32 - 1;
    let xe = ((v.center.x + v.half.x) / seg).ceil() as i32 + 1;
    let ripple = rgba(mix3(floor, light, 0.25), 0.07);
    for row in r0..=r1 {
        let y = row as f32 * spacing;
        let phase = hash(row, 0, 21) * 6.;
        let wave =
            |x: f32| y + (x * 0.012 + phase).sin() * 9. + (x * 0.031 + phase * 2.).sin() * 3.;
        let mut prev = vec2(xs as f32 * seg, wave(xs as f32 * seg));
        for k in xs + 1..=xe {
            let x = k as f32 * seg;
            let next = vec2(x, wave(x));
            let fade = hash(k / 3, row, 4);
            if fade > 0.35 {
                p.line(false, prev, next, 2.5, with_alpha(ripple, fade));
            }
            prev = next;
        }
    }
    // Retro dither: chunky pixels scattered on the floor.
    let px = 28.;
    let gx0 = ((v.center.x - v.half.x) / px).floor() as i32;
    let gx1 = ((v.center.x + v.half.x) / px).ceil() as i32;
    let gy0 = ((v.center.y - v.half.y) / px).floor() as i32;
    let gy1 = ((v.center.y + v.half.y) / px).ceil() as i32;
    let dot = mix3(floor, light, 0.35);
    for gy in gy0..=gy1 {
        for gx in gx0..=gx1 {
            let h = hash(gx, gy, 7);
            if h < 0.16 {
                let size = if h < 0.03 { 4. } else { 3. };
                let a = 0.05 + h * 0.5;
                p.rect(
                    false,
                    vec2(gx as f32, gy as f32) * px,
                    Vec2::splat(size),
                    rgba(dot, a),
                );
            }
        }
    }
}

fn decor(p: &mut Painter, s: &Scene, floor: Rgb, light: Rgb) {
    let v = s.view;
    let cell = 190.;
    let x0 = ((v.center.x - v.half.x - 80.) / cell).floor() as i32;
    let x1 = ((v.center.x + v.half.x + 80.) / cell).ceil() as i32;
    let y0 = ((v.center.y - v.half.y - 120.) / cell).floor() as i32;
    let y1 = ((v.center.y + v.half.y + 80.) / cell).ceil() as i32;
    let t = s.time;
    for cy in y0..=y1 {
        for cx in x0..=x1 {
            let h = hash(cx, cy, 99);
            let base = vec2(
                (cx as f32 + 0.15 + hash(cx, cy, 1) * 0.7) * cell,
                (cy as f32 + 0.15 + hash(cx, cy, 2) * 0.7) * cell,
            );
            // Keep the spawn origin clear for the opening seconds.
            if base.length_squared() < 120. * 120. {
                continue;
            }
            let hue = hash(cx, cy, 8);
            match (h * 10.) as u32 {
                0 | 1 => kelp(p, base, t, h, floor, light),
                2 => coral(p, base, t, hue, light),
                3 => anemone(p, base, t, hue),
                4 => rock(p, base, h, floor, light),
                5 => sea_pens(p, base, t, hue),
                6 => {
                    rock(p, base, h, floor, light);
                    vent(p, base + vec2(18., -6.), t, light);
                }
                7 => starfish_floor(p, base, hue),
                _ => {}
            }
        }
    }
    // Plankton: two parallax layers of drifting motes.
    for layer in 0..2 {
        let depth = if layer == 0 { 1. } else { 0.72 };
        let cell = if layer == 0 { 70. } else { 95. };
        let shift = v.center * (1. - depth);
        let local = v.center - shift;
        let x0 = ((local.x - v.half.x) / cell).floor() as i32 - 1;
        let x1 = ((local.x + v.half.x) / cell).ceil() as i32 + 1;
        let y0 = ((local.y - v.half.y) / cell).floor() as i32 - 1;
        let y1 = ((local.y + v.half.y) / cell).ceil() as i32 + 1;
        for cy in y0..=y1 {
            for cx in x0..=x1 {
                let h = hash(cx, cy, 31 + layer);
                if h > 0.45 {
                    continue;
                }
                let drift = vec2(
                    (t * (0.15 + h) + h * 30.).sin() * 18.,
                    -((t * 9. * (0.5 + h) + h * 400.) % cell),
                );
                let pos = vec2(cx as f32 + hash(cx, cy, 41), cy as f32 + hash(cx, cy, 43)) * cell
                    + drift
                    + shift;
                let tw = 0.5 + 0.5 * (t * (1.5 + h * 3.) + h * 50.).sin();
                let size = if layer == 0 { 2.2 } else { 1.6 };
                p.rect(true, pos, Vec2::splat(size), rgba(light, 0.25 + 0.45 * tw));
                if h < 0.06 {
                    p.glow(pos, 14., rgba(light, 0.18 * tw));
                }
            }
        }
    }
}

fn kelp(p: &mut Painter, base: Vec2, t: f32, h: f32, floor: Rgb, light: Rgb) {
    let strands = 2 + (h * 30.) as usize % 3;
    for k in 0..strands {
        let mut at = base + vec2(k as f32 * 9. - 9., 0.);
        let height = 70. + 50. * ((h * 7. + k as f32) % 1.);
        let segs = 7;
        let stem = rgba(mix3(floor, [0.2, 0.55, 0.4], 0.7), 0.85);
        for i in 0..segs {
            let f = i as f32 / segs as f32;
            let sway = (t * 1.1 + h * 9. + k as f32 + f * 2.5).sin() * 10. * f;
            let next = base + vec2(k as f32 * 9. - 9. + sway, -height * (f + 1. / segs as f32));
            let w = 5. * (1. - f * 0.7);
            p.line(false, at, next, w, with_alpha(stem, 0.55 + 0.45 * (1. - f)));
            if i % 2 == 1 {
                let leaf = vec2(if i % 4 == 1 { 9. } else { -9. }, -3.);
                p.line(false, next, next + leaf, 3., with_alpha(stem, 0.6));
            }
            at = next;
        }
        if k == 0 {
            p.glow(at, 22., rgba(mix3(light, [0.5, 1., 0.7], 0.5), 0.25));
            p.disc(true, at, 2.5, rgba([0.7, 1., 0.85], 0.6), 6);
        }
    }
}

fn coral(p: &mut Painter, base: Vec2, t: f32, hue: f32, light: Rgb) {
    let c = scale3(mix3([1., 0.45, 0.6], [1., 0.65, 0.35], hue), 0.7);
    let dark = scale3(c, 0.45);
    for k in 0..5 {
        let a = -std::f32::consts::FRAC_PI_2 + (k as f32 - 2.) * 0.38;
        let len = 34. + 14. * ((hue * 13. + k as f32) % 1.);
        let sway = (t * 0.8 + k as f32 + hue * 5.).sin() * 0.05;
        let tip = base + vec2((a + sway).cos(), (a + sway).sin()) * len;
        p.line_grad(false, base, tip, 4., rgba(dark, 0.9), rgba(c, 0.9));
        let side = base + (tip - base) * 0.6;
        let twig = side + vec2((a + 0.6).cos(), (a + 0.6).sin()) * 12.;
        p.line(false, side, twig, 2.5, rgba(c, 0.8));
        let pulse = 0.6 + 0.4 * (t * 2. + k as f32 * 1.3 + hue * 7.).sin();
        p.disc(true, tip, 2.5, rgba(mix3(c, [1.; 3], 0.4), 0.9 * pulse), 6);
        p.glow(tip, 12., rgba(c, 0.25 * pulse));
    }
    p.glow(base + vec2(0., -20.), 60., rgba(mix3(c, light, 0.5), 0.06));
}

fn anemone(p: &mut Painter, base: Vec2, t: f32, hue: f32) {
    // Soft coral "fingers": rounded, upright and calm, unlike spiky urchins.
    let c = mix3([0.35, 0.8, 0.75], [0.75, 0.5, 0.85], hue);
    let pulse = 0.65 + 0.35 * (t * 1.6 + hue * 9.).sin();
    p.glow(base + vec2(0., -14.), 64., rgba(c, 0.1 * pulse));
    for k in 0..6 {
        let x = (k as f32 - 2.5) * 6.;
        let lean = (t * 0.9 + k as f32 * 0.8 + hue * 4.).sin() * 3.;
        let h = 16. + 9. * ((hue * 11. + k as f32 * 0.37) % 1.);
        let root = base + vec2(x, 0.);
        let tip = base + vec2(x * 1.3 + lean, -h);
        p.line_grad(
            false,
            root,
            tip,
            6.,
            rgba(scale3(c, 0.35), 0.9),
            rgba(scale3(c, 0.7), 0.9),
        );
        p.disc(false, tip, 3., rgba(scale3(c, 0.75), 0.95), 8);
        p.disc(true, tip, 1.8, rgba(c, 0.55 * pulse), 6);
    }
}

fn rock(p: &mut Painter, base: Vec2, h: f32, floor: Rgb, light: Rgb) {
    let r = 22. + 18. * h;
    let body = scale3(floor, 0.55);
    p.ellipse(
        false,
        base + vec2(4., 6.),
        r * 1.15,
        r * 0.7,
        0.,
        [0., 0., 0., 0.25],
        [0., 0., 0., 0.],
        16,
    );
    p.ellipse(
        false,
        base,
        r * 1.1,
        r * 0.75,
        h,
        rgba(mix3(body, floor, 0.4), 1.),
        rgba(body, 1.),
        16,
    );
    p.arc(
        false,
        base,
        r * 0.55,
        r * 0.72,
        3.6,
        5.2,
        rgba(mix3(floor, light, 0.4), 0.0),
        rgba(mix3(floor, light, 0.4), 0.35),
        6,
    );
}

fn vent(p: &mut Painter, base: Vec2, t: f32, light: Rgb) {
    for k in 0..4 {
        let phase = (t * 0.6 + k as f32 * 0.25) % 1.;
        let pos = base + vec2((phase * 9. + k as f32).sin() * 5., -phase * 90.);
        let r = 2. + phase * 4.;
        let a = (1. - phase) * 0.7;
        p.ring(
            false,
            pos,
            r - 1.,
            r,
            rgba(light, a * 0.6),
            rgba(light, a),
            8,
        );
    }
}

fn sea_pens(p: &mut Painter, base: Vec2, t: f32, hue: f32) {
    let c = mix3([0.55, 0.85, 1.], [1., 0.9, 0.5], hue);
    for k in 0..3 {
        let root = base + vec2(k as f32 * 14. - 14., k as f32 % 2. * 6.);
        let sway = (t * 1.3 + k as f32 + hue * 4.).sin() * 4.;
        let tip = root + vec2(sway, -26. - k as f32 * 6.);
        p.line(false, root, tip, 2.5, rgba(scale3(c, 0.4), 0.9));
        let pulse = 0.55 + 0.45 * (t * 2.6 + k as f32 * 2. + hue * 5.).sin();
        p.ellipse(
            false,
            tip,
            4.,
            6.,
            0.,
            rgba(mix3(c, [1.; 3], 0.5), 1.),
            rgba(c, 0.9),
            8,
        );
        p.glow(tip, 24., rgba(c, 0.32 * pulse));
    }
}

fn starfish_floor(p: &mut Painter, base: Vec2, hue: f32) {
    let c = mix3([1., 0.5, 0.4], [1., 0.75, 0.4], hue);
    p.star(
        false,
        base,
        11.,
        4.5,
        5,
        hue * 6.,
        rgba(c, 0.8),
        rgba(scale3(c, 0.6), 0.8),
    );
}

/// Lantern darkness: a soft radial falloff centered on the spirit.
fn darkness(p: &mut Painter, s: &Scene, deep: Rgb) {
    let player = lerp(s.game.player.prev, s.game.player.pos, s.alpha);
    let lights_out = s
        .game
        .boss()
        .filter(|b| b.kind == FoeKind::Gulpjaw && b.hp / b.max_hp < 0.6)
        .map_or(0., |_| 1.);
    let inner = 230. - lights_out * 70.;
    let outer = 720. - lights_out * 230.;
    let shade = scale3(deep, 0.3);
    let edge = 0.58 + lights_out * 0.2;
    let far = (s.view.half.length() + 300.).max(outer + 10.);
    let mid = inner + (outer - inner) * 0.45;
    p.ring(
        false,
        player,
        inner,
        mid,
        rgba(shade, 0.),
        rgba(shade, edge * 0.35),
        48,
    );
    p.ring(
        false,
        player,
        mid,
        outer,
        rgba(shade, edge * 0.35),
        rgba(shade, edge),
        48,
    );
    p.ring(
        false,
        player,
        outer,
        far,
        rgba(shade, edge),
        rgba(shade, edge + 0.1),
        32,
    );
}

fn hazards(p: &mut Painter, s: &Scene, light: Rgb) {
    let t = s.time;
    for tg in &s.game.telegraphs {
        let prog = tg.progress();
        let urgent = 0.5 + 0.5 * (t * (8. + prog * 16.)).sin();
        match tg.shape {
            Shape::Circle { pos, radius } => {
                let c = [1., 0.32, 0.5];
                p.disc(false, pos, radius, rgba(c, 0.13), 32);
                p.disc(false, pos, radius * prog, rgba(c, 0.2 + 0.1 * prog), 32);
                p.ring(
                    true,
                    pos,
                    radius - 3.,
                    radius + 2.,
                    rgba(c, 0.5 + 0.4 * urgent * prog),
                    rgba(c, 0.),
                    32,
                );
                p.ring(
                    false,
                    pos,
                    radius - 2.5,
                    radius,
                    rgba([1., 0.6, 0.7], 0.8),
                    rgba([1., 0.6, 0.7], 0.8),
                    32,
                );
            }
            Shape::Lane {
                from,
                dir,
                length,
                width,
            } => {
                let side = vec2(-dir.y, dir.x) * width * 0.5;
                let end = from + dir * length;
                let c = [1., 0.4, 0.35];
                p.quad(
                    false,
                    [from + side, end + side, end - side, from - side],
                    [rgba(c, 0.18), rgba(c, 0.02), rgba(c, 0.02), rgba(c, 0.18)],
                );
                p.line(false, from + side, end + side, 3., rgba(c, 0.6));
                p.line(false, from - side, end - side, 3., rgba(c, 0.6));
                // Chevrons racing toward the target.
                let n = 8;
                for k in 0..n {
                    let f = ((k as f32 / n as f32) + t * 0.9) % 1.;
                    let at = from + dir * (f * length);
                    let back = at - dir * 26.;
                    let a = (1. - f) * (0.4 + 0.6 * prog);
                    p.line(true, back + side * 0.6, at, 5., rgba(c, a));
                    p.line(true, back - side * 0.6, at, 5., rgba(c, a));
                }
            }
        }
    }
    // Depth-charge whirlpools.
    for z in &s.game.zones {
        let fade = (1. - z.age / z.life).clamp(0., 1.) * (z.age / 0.2).min(1.);
        let c = [0.45, 0.65, 1.];
        p.disc_grad(true, z.pos, z.radius, rgba(c, 0.18 * fade), rgba(c, 0.), 24);
        for arm in 0..4 {
            let a0 = arm as f32 * std::f32::consts::FRAC_PI_2 - t * 5.;
            for k in 0..6 {
                let f = k as f32 / 6.;
                let a = a0 + f * 2.2;
                let r = z.radius * (1. - f * 0.8);
                let q = z.pos + vec2(a.cos(), a.sin()) * r;
                p.disc(
                    true,
                    q,
                    3.5 * (1. - f * 0.5),
                    rgba(mix3(c, light, 0.4), 0.55 * fade),
                    6,
                );
            }
        }
    }
}

fn mote_style(value: u32) -> (Rgb, f32) {
    match value {
        0..=2 => ([0.45, 0.95, 1.], 3.6),
        3..=9 => ([0.45, 1., 0.6], 4.6),
        10..=49 => ([0.8, 0.55, 1.], 6.),
        _ => ([1., 0.82, 0.4], 7.5),
    }
}

fn mote(p: &mut Painter, s: &Scene, m: &Mote) {
    let pos = lerp(m.prev, m.pos, s.alpha);
    let (c, size) = mote_style(m.value);
    let bob = (s.time * 3. + m.seed * 20.).sin() * 1.5 * s.motion;
    let pos = pos + vec2(0., bob);
    if m.pulled && m.vel.length_squared() > 1000. {
        let tail = pos - m.vel.normalize_or_zero() * (size * 3.5);
        p.line_grad(true, tail, pos, size * 1.2, rgba(c, 0.), rgba(c, 0.8));
    }
    let r = size;
    let pts = [
        pos + vec2(0., -r * 1.25),
        pos + vec2(r, 0.),
        pos + vec2(0., r * 1.25),
        pos + vec2(-r, 0.),
    ];
    let dark = rgba(scale3(c, 0.55), 1.);
    let bright = rgba(mix3(c, [1.; 3], 0.55), 1.);
    p.quad(false, pts, [bright, dark, dark, dark]);
    p.glint(pos, r * 3.2, rgba(c, 0.3));
    let tw = (s.time * 2.3 + m.seed * 40.).sin();
    if tw > 0.92 {
        let k = (tw - 0.92) / 0.08;
        p.line(
            true,
            pos - vec2(r * 2.2, 0.),
            pos + vec2(r * 2.2, 0.),
            1.5,
            rgba([1.; 3], 0.8 * k),
        );
        p.line(
            true,
            pos - vec2(0., r * 2.2),
            pos + vec2(0., r * 2.2),
            1.5,
            rgba([1.; 3], 0.8 * k),
        );
    }
}

fn motes(p: &mut Painter, s: &Scene) {
    for m in &s.game.motes {
        if s.view.visible(m.pos, 20.) {
            mote(p, s, m);
        }
    }
}

fn pickups(p: &mut Painter, s: &Scene) {
    let t = s.time;
    for pk in &s.game.pickups {
        let pos = lerp(pk.prev, pk.pos, s.alpha);
        if !s.view.visible(pos, 40.) {
            continue;
        }
        let bob = (t * 2.5 + pos.x * 0.01).sin() * 3. * s.motion;
        let at = pos + vec2(0., bob);
        match pk.kind {
            PickupKind::Coin(_) => {
                let spin = (t * 4. + pos.y * 0.02).sin();
                p.glow(at, 22., rgba([1., 0.8, 0.35], 0.4));
                p.ellipse(
                    false,
                    at,
                    7. * spin.abs().max(0.25),
                    7.,
                    0.,
                    rgba([1., 0.93, 0.6], 1.),
                    rgba([0.85, 0.6, 0.2], 1.),
                    12,
                );
                p.ellipse(
                    false,
                    at,
                    3. * spin.abs().max(0.2),
                    3.,
                    0.,
                    rgba([1., 1., 0.8], 1.),
                    rgba([1., 0.85, 0.4], 1.),
                    8,
                );
            }
            PickupKind::Heal => {
                p.glow(at, 30., rgba([0.5, 1., 0.55], 0.35));
                for (k, off) in [vec2(-5., 2.), vec2(5., 2.), vec2(0., -6.)]
                    .into_iter()
                    .enumerate()
                {
                    p.disc_grad(
                        false,
                        at + off,
                        6.,
                        rgba([0.75, 1., 0.7], 1.),
                        rgba([0.25, 0.7, 0.35], 1.),
                        12,
                    );
                    p.disc(
                        false,
                        at + off + vec2(-2., -2.),
                        1.6 + k as f32 * 0.1,
                        rgba([1.; 3], 0.8),
                        6,
                    );
                }
                p.line(
                    false,
                    at + vec2(0., -12.),
                    at + vec2(3., -18.),
                    2.,
                    rgba([0.4, 0.8, 0.4], 1.),
                );
            }
            PickupKind::Magnet => {
                let pulse = 0.6 + 0.4 * (t * 5.).sin();
                p.glow(at, 40., rgba([1., 0.5, 0.85], 0.4 * pulse));
                p.arc(
                    false,
                    at,
                    6.,
                    11.,
                    0.,
                    std::f32::consts::PI,
                    rgba([1., 0.45, 0.6], 1.),
                    rgba([1., 0.45, 0.6], 1.),
                    10,
                );
                p.rect(
                    false,
                    at + vec2(-11., -8.),
                    vec2(5., 8.),
                    rgba([0.85, 0.9, 1.], 1.),
                );
                p.rect(
                    false,
                    at + vec2(6., -8.),
                    vec2(5., 8.),
                    rgba([0.85, 0.9, 1.], 1.),
                );
            }
            PickupKind::Chest(tier) => {
                let gold = if tier >= 2 {
                    [1., 0.75, 0.95]
                } else {
                    [1., 0.82, 0.35]
                };
                let pulse = 0.7 + 0.3 * (t * 3.).sin();
                p.glow(at, 90., rgba(gold, 0.35 * pulse));
                // Light rays.
                for k in 0..6 {
                    let a = k as f32 / 6. * std::f32::consts::TAU + t * 0.6;
                    let d = vec2(a.cos(), a.sin());
                    p.tri(
                        true,
                        at,
                        at + rotate(d, 0.12) * 70.,
                        at + rotate(d, -0.12) * 70.,
                        rgba(gold, 0.25 * pulse),
                        rgba(gold, 0.),
                        rgba(gold, 0.),
                    );
                }
                p.ellipse(
                    false,
                    at + vec2(3., 14.),
                    18.,
                    6.,
                    0.,
                    [0., 0., 0., 0.35],
                    [0., 0., 0., 0.],
                    12,
                );
                p.round_rect_v(
                    false,
                    at + vec2(-16., -8.),
                    vec2(32., 20.),
                    4.,
                    rgba([0.6, 0.32, 0.16], 1.),
                    rgba([0.38, 0.18, 0.1], 1.),
                );
                p.round_rect_v(
                    false,
                    at + vec2(-17., -16.),
                    vec2(34., 11.),
                    5.,
                    rgba([0.75, 0.42, 0.2], 1.),
                    rgba([0.55, 0.3, 0.15], 1.),
                );
                p.rect(false, at + vec2(-17., -6.), vec2(34., 3.), rgba(gold, 1.));
                p.rect(false, at + vec2(-3., -9.), vec2(6., 9.), rgba(gold, 1.));
                p.rect(
                    false,
                    at + vec2(-1., -6.),
                    vec2(2., 3.),
                    rgba([0.3, 0.15, 0.05], 1.),
                );
            }
        }
    }
}

fn shadows(p: &mut Painter, s: &Scene) {
    let sh = [0., 0., 0., 0.28];
    let clear = [0., 0., 0., 0.];
    for f in &s.game.foes {
        let pos = lerp(f.prev, f.pos, s.alpha);
        if !s.view.visible(pos, f.radius + 20.) {
            continue;
        }
        let r = f.radius;
        p.ellipse(
            false,
            pos + vec2(r * 0.25, r * 0.85),
            r * 0.95,
            r * 0.42,
            0.,
            sh,
            clear,
            10,
        );
    }
    let pl = lerp(s.game.player.prev, s.game.player.pos, s.alpha);
    p.ellipse(false, pl + vec2(3., 13.), 11., 5., 0., sh, clear, 12);
}

fn flash_mix(c: Rgb, f: &Foe, flashes: f32) -> Rgb {
    let k = if f.flash > 0. {
        (f.flash / 0.11) * flashes
    } else {
        0.
    };
    mix3(c, [1.; 3], k.clamp(0., 1.) * 0.85)
}

fn eyes(p: &mut Painter, at: Vec2, look: Vec2, spread: f32, size: f32, sleepy: bool) {
    for side in [-1., 1.] {
        let e = at + vec2(side * spread, 0.);
        p.disc(false, e, size, rgba([1., 1., 1.], 0.95), 8);
        p.disc(
            false,
            e + look * size * 0.45,
            size * 0.55,
            rgba([0.04, 0.05, 0.12], 1.),
            8,
        );
        if sleepy {
            p.rect(
                false,
                e - vec2(size * 1.1, size * 1.1),
                vec2(size * 2.2, size * 0.9),
                rgba([0.15, 0.3, 0.45], 0.9),
            );
        }
    }
}

fn foes(p: &mut Painter, s: &Scene) {
    let player = lerp(s.game.player.prev, s.game.player.pos, s.alpha);
    let t = s.time;
    // Bosses draw last so the horde never covers them.
    for pass in 0..2 {
        for f in &s.game.foes {
            if f.kind.is_boss() != (pass == 1) {
                continue;
            }
            let pos = lerp(f.prev, f.pos, s.alpha);
            if !s.view.visible(pos, f.radius * 2. + 30.) {
                continue;
            }
            let look = (player - pos).normalize_or_zero();
            let info = f.kind.info();
            let c = flash_mix(info.color, f, s.flashes);
            let r = f.radius;
            let ft = t + f.seed * 10.;
            if f.elite > 0 {
                let spin = t * 1.5;
                p.glow(pos, r * 3.2, rgba([1., 0.85, 0.4], 0.32));
                for k in 0..8 {
                    let a = spin + k as f32 / 8. * std::f32::consts::TAU;
                    let q = pos + vec2(a.cos(), a.sin()) * (r + 9.);
                    p.disc(true, q, 2.6, rgba([1., 0.9, 0.5], 0.85), 6);
                }
                // Tiny crown.
                let top = pos + vec2(0., -r - 10.);
                p.star(
                    false,
                    top,
                    7.,
                    3.5,
                    3,
                    -std::f32::consts::FRAC_PI_2,
                    rgba([1., 0.9, 0.45], 1.),
                    rgba([1., 0.7, 0.25], 1.),
                );
            }
            match f.kind {
                FoeKind::Driftlet => driftlet(p, pos, r, ft, c, look, f.flash > 0.),
                FoeKind::Mothling => mothling(p, pos, r, ft, c, f.vel),
                FoeKind::Grumpfish => grumpfish(p, pos, r, ft, c, look),
                FoeKind::Sneel => sneel(p, pos, r, ft, c, f, look),
                FoeKind::Urchin => urchin(p, pos, r, ft, c, f, look),
                FoeKind::Crabbo => crabbo(p, pos, r, ft, c, look),
                FoeKind::MotherBrine => mother_brine(p, s, pos, r, ft, c, f, look),
                FoeKind::Gulpjaw => gulpjaw(p, s, pos, r, ft, c, f, look),
            }
        }
    }
}

fn driftlet(p: &mut Painter, pos: Vec2, r: f32, t: f32, c: Rgb, look: Vec2, hit: bool) {
    let squeeze = (t * 3.2).sin();
    let rx = r * (1. + 0.12 * squeeze);
    let ry = r * (0.9 - 0.1 * squeeze);
    // Tentacles trail away from travel.
    for k in 0..4 {
        let x = (k as f32 - 1.5) * r * 0.42;
        let mut at = pos + vec2(x, ry * 0.3);
        for i in 1..4 {
            let f = i as f32 / 3.;
            let next = pos
                + vec2(
                    x + (t * 4. + k as f32 + f * 3.).sin() * 3.,
                    ry * 0.3 + r * 1.3 * f,
                );
            p.line(
                false,
                at,
                next,
                2.2 * (1. - f * 0.5),
                rgba(c, 0.55 * (1. - f * 0.6)),
            );
            at = next;
        }
    }
    p.ellipse(
        false,
        pos,
        rx,
        ry,
        0.,
        rgba(mix3(c, [1.; 3], 0.35), 0.9),
        rgba(scale3(c, 0.55), 0.75),
        14,
    );
    p.ellipse(
        false,
        pos + vec2(-rx * 0.3, -ry * 0.35),
        rx * 0.32,
        ry * 0.22,
        -0.4,
        rgba([1.; 3], 0.55),
        rgba([1.; 3], 0.0),
        8,
    );
    p.glow(pos, r * 2.6, rgba(c, 0.3));
    eyes(p, pos + vec2(0., ry * 0.05), look, r * 0.36, r * 0.2, !hit);
}

fn mothling(p: &mut Painter, pos: Vec2, r: f32, t: f32, c: Rgb, vel: Vec2) {
    let flap = (t * 18.).sin().abs();
    let dir = vel.normalize_or_zero();
    let dir = if dir == Vec2::ZERO {
        vec2(0., -1.)
    } else {
        dir
    };
    let side = vec2(-dir.y, dir.x);
    let body = rgba(scale3(c, 0.5), 1.);
    for s in [-1., 1.] {
        let tip = pos + side * s * r * (0.6 + 1.3 * flap) - dir * r * 0.4;
        let back = pos - dir * r * 1.1 + side * s * r * 0.7 * flap;
        p.tri(
            false,
            pos + dir * r * 0.3,
            tip,
            back,
            rgba(mix3(c, [1.; 3], 0.4), 0.9),
            rgba(c, 0.8),
            rgba(scale3(c, 0.6), 0.7),
        );
    }
    p.ellipse(
        false,
        pos,
        r * 0.4,
        r * 0.75,
        dir.y.atan2(dir.x) + std::f32::consts::FRAC_PI_2,
        body,
        body,
        8,
    );
    p.glow(pos, r * 3., rgba(c, 0.38));
    p.disc(
        true,
        pos - dir * r * 0.5,
        r * 0.35,
        rgba([1., 0.95, 0.7], 0.8),
        6,
    );
}

fn grumpfish(p: &mut Painter, pos: Vec2, r: f32, t: f32, c: Rgb, look: Vec2) {
    let n = 12;
    for k in 0..n {
        let a = k as f32 / n as f32 * std::f32::consts::TAU + (t * 0.5).sin() * 0.1;
        let d = vec2(a.cos(), a.sin());
        let side = vec2(-d.y, d.x) * r * 0.16;
        p.tri(
            false,
            pos + d * r * 0.85 + side,
            pos + d * r * 1.32,
            pos + d * r * 0.85 - side,
            rgba(scale3(c, 0.8), 1.),
            rgba([1., 0.95, 0.75], 1.),
            rgba(scale3(c, 0.8), 1.),
        );
    }
    p.disc_grad(
        false,
        pos,
        r,
        rgba(mix3(c, [1., 1., 0.8], 0.4), 1.),
        rgba(scale3(c, 0.6), 1.),
        16,
    );
    // Grumpy brows over glaring eyes.
    let eye_at = pos + look * r * 0.25 + vec2(0., -r * 0.12);
    eyes(p, eye_at, look, r * 0.32, r * 0.2, false);
    for s in [-1., 1.] {
        let a = eye_at + vec2(s * r * 0.55, -r * 0.32);
        let b = eye_at + vec2(s * r * 0.12, -r * 0.2);
        p.line(false, a, b, 2.5, rgba([0.2, 0.08, 0.05], 1.));
    }
    p.line(
        false,
        pos + vec2(-r * 0.25, r * 0.45),
        pos + vec2(r * 0.25, r * 0.4),
        2.,
        rgba([0.25, 0.1, 0.06], 0.9),
    );
    p.glow(pos, r * 2.2, rgba(c, 0.22));
}

fn sneel(p: &mut Painter, pos: Vec2, r: f32, t: f32, c: Rgb, f: &Foe, look: Vec2) {
    let dir = if f.state >= 1 {
        f.aim
    } else {
        f.vel.normalize_or(look)
    };
    let side = vec2(-dir.y, dir.x);
    let segs = 6;
    for k in (1..=segs).rev() {
        let fk = k as f32;
        let wiggle = (t * 9. - fk * 0.9).sin() * 4. * (fk / segs as f32);
        let q = pos - dir * fk * r * 0.75 + side * wiggle;
        let rr = r * (1. - fk / (segs as f32 + 2.));
        p.disc(
            false,
            q,
            rr,
            rgba(mix3(scale3(c, 0.5), c, 1. - fk / segs as f32), 1.),
            10,
        );
        if k % 2 == 0 {
            p.disc(true, q, rr * 0.4, rgba(c, 0.6), 6);
        }
    }
    p.disc_grad(
        false,
        pos,
        r,
        rgba(mix3(c, [1.; 3], 0.3), 1.),
        rgba(scale3(c, 0.6), 1.),
        12,
    );
    let angry = f.state == 1;
    let eye = pos + dir * r * 0.35;
    let ec = if angry { [1., 0.2, 0.2] } else { [1., 1., 0.8] };
    p.disc(false, eye + side * r * 0.35, r * 0.22, rgba(ec, 1.), 8);
    p.disc(false, eye - side * r * 0.35, r * 0.22, rgba(ec, 1.), 8);
    if angry {
        // Telegraph: a trembling aim line and a red glare.
        let prog = (f.timer / 0.75).min(1.);
        p.glow(pos, r * 3., rgba([1., 0.25, 0.2], 0.45 * prog));
        let end = pos + dir * (430. * 0.45);
        p.line_grad(
            true,
            pos,
            end,
            3. + prog * 3.,
            rgba([1., 0.3, 0.25], 0.5 * prog),
            rgba([1., 0.3, 0.25], 0.),
        );
    } else {
        p.glow(pos, r * 2.4, rgba(c, 0.25));
    }
}

fn urchin(p: &mut Painter, pos: Vec2, r: f32, t: f32, c: Rgb, f: &Foe, look: Vec2) {
    let charging = f.state == 1;
    let prog = if charging {
        (f.timer / 0.7).min(1.)
    } else {
        0.
    };
    let tip = mix3(c, [1., 0.35, 0.4], prog);
    let n = 16;
    for k in 0..n {
        let a = k as f32 / n as f32 * std::f32::consts::TAU + t * 0.4;
        let d = vec2(a.cos(), a.sin());
        let len = r * (1.55 + 0.15 * (t * 3. + k as f32).sin() + prog * 0.3);
        p.line_grad(
            false,
            pos + d * r * 0.6,
            pos + d * len,
            2.4,
            rgba(scale3(c, 0.4), 1.),
            rgba(tip, 1.),
        );
        if k % 2 == 0 {
            p.disc(true, pos + d * len, 2., rgba(tip, 0.8), 6);
        }
    }
    p.disc_grad(
        false,
        pos,
        r * 0.85,
        rgba(mix3(c, [0.2, 0.1, 0.3], 0.3), 1.),
        rgba([0.12, 0.05, 0.2], 1.),
        14,
    );
    eyes(p, pos, look, r * 0.3, r * 0.2, false);
    p.glow(pos, r * (2.4 + prog), rgba(tip, 0.2 + 0.35 * prog));
}

fn crabbo(p: &mut Painter, pos: Vec2, r: f32, t: f32, c: Rgb, look: Vec2) {
    let dark = scale3(c, 0.55);
    // Legs scuttle.
    for s in [-1., 1.] {
        for k in 0..3 {
            let phase = (t * 10. + k as f32 * 2.).sin() * 3.;
            let root = pos + vec2(s * r * 0.6, (k as f32 - 1.) * r * 0.35);
            let knee = root + vec2(s * r * 0.55, -4. + phase);
            let foot = knee + vec2(s * r * 0.3, r * 0.4);
            p.line(false, root, knee, 3., rgba(dark, 1.));
            p.line(false, knee, foot, 2.5, rgba(dark, 1.));
        }
    }
    // Claws snap toward the player.
    let snap = (t * 4.).sin().abs();
    for s in [-1., 1.] {
        let arm = pos + vec2(s * r * 0.75, -r * 0.4) + look * r * 0.4;
        let claw = arm + look * r * 0.55 + vec2(s * r * 0.2, -r * 0.2);
        p.line(
            false,
            pos + vec2(s * r * 0.5, -r * 0.2),
            arm,
            4.,
            rgba(dark, 1.),
        );
        p.disc_grad(
            false,
            claw,
            r * 0.42,
            rgba(mix3(c, [1.; 3], 0.25), 1.),
            rgba(c, 1.),
            10,
        );
        let open = r * 0.2 * snap;
        p.tri(
            false,
            claw,
            claw + look * r * 0.5 + vec2(-look.y, look.x) * open,
            claw + look * r * 0.5 - vec2(-look.y, look.x) * open,
            rgba(scale3(c, 0.3), 1.),
            rgba(scale3(c, 0.3), 1.),
            rgba(scale3(c, 0.3), 1.),
        );
    }
    // Spiral shell home.
    p.disc_grad(
        false,
        pos,
        r,
        rgba(mix3(c, [1., 0.9, 0.7], 0.35), 1.),
        rgba(dark, 1.),
        18,
    );
    for k in 0..3 {
        let rr = r * (0.75 - k as f32 * 0.22);
        p.arc(
            false,
            pos + vec2(k as f32 * 2., k as f32 * 1.5),
            rr - 2.,
            rr,
            0.5 + k as f32,
            3.6 + k as f32,
            rgba(scale3(c, 0.4), 0.9),
            rgba(scale3(c, 0.4), 0.9),
            8,
        );
    }
    eyes(
        p,
        pos + vec2(0., -r * 0.95) + look * 2.,
        look,
        r * 0.2,
        r * 0.14,
        false,
    );
    p.glow(pos, r * 2., rgba(c, 0.18));
}

#[allow(clippy::too_many_arguments)]
fn mother_brine(
    p: &mut Painter,
    s: &Scene,
    pos: Vec2,
    r: f32,
    t: f32,
    c: Rgb,
    f: &Foe,
    look: Vec2,
) {
    let brain = s.game.director.boss[0];
    let casting = brain.casting;
    let pulse = (t * 2.).sin();
    p.glow(
        pos,
        r * 4.5,
        rgba(c, 0.22 + if casting { 0.15 } else { 0. }),
    );
    // Long frilly tentacles.
    for k in 0..9 {
        let x = (k as f32 - 4.) * r * 0.2;
        let mut at = pos + vec2(x, r * 0.35);
        for i in 1..8 {
            let fi = i as f32 / 7.;
            let next = pos
                + vec2(
                    x * (1. + fi * 0.6) + (t * 2.5 + k as f32 + fi * 4.).sin() * 10. * fi,
                    r * 0.35 + r * 2.2 * fi,
                );
            p.line(
                false,
                at,
                next,
                5. * (1. - fi * 0.7),
                rgba(mix3(c, [1.; 3], 0.2), 0.6 * (1. - fi * 0.5)),
            );
            if i % 2 == 0 {
                p.disc(true, next, 3., rgba(c, 0.7), 6);
            }
            at = next;
        }
    }
    let rx = r * (1.05 + 0.06 * pulse);
    let ry = r * (0.82 - 0.05 * pulse);
    p.ellipse(
        false,
        pos,
        rx,
        ry,
        0.,
        rgba(mix3(c, [1.; 3], 0.45), 0.92),
        rgba(scale3(c, 0.55), 0.85),
        32,
    );
    // Bell frills and inner organs.
    for k in 0..14 {
        let a = std::f32::consts::PI * (k as f32 / 13.);
        let q = pos + vec2(-a.cos() * rx, ry * 0.35 - a.sin() * 0.0 + 2.);
        p.disc(false, q + vec2(0., 4.), 6., rgba(scale3(c, 0.7), 0.9), 8);
    }
    p.ring(
        false,
        pos + vec2(0., -ry * 0.1),
        r * 0.3,
        r * 0.42,
        rgba([1., 0.85, 0.95], 0.5),
        rgba([1., 0.85, 0.95], 0.1),
        24,
    );
    p.ellipse(
        false,
        pos + vec2(-rx * 0.35, -ry * 0.45),
        rx * 0.3,
        ry * 0.18,
        -0.3,
        rgba([1.; 3], 0.6),
        rgba([1.; 3], 0.),
        12,
    );
    eyes(p, pos + vec2(0., ry * 0.05), look, r * 0.3, r * 0.15, false);
    // Disappointed eyebrows.
    for sgn in [-1., 1.] {
        p.line(
            false,
            pos + vec2(sgn * r * 0.15, -r * 0.18),
            pos + vec2(sgn * r * 0.5, -r * 0.1),
            4.,
            rgba([0.3, 0.05, 0.2], 1.),
        );
    }
    if casting {
        let prog = 1. - (brain.timer / 0.9).clamp(0., 1.);
        p.ring(
            true,
            pos,
            r * (1.8 - prog * 0.6),
            r * (1.9 - prog * 0.6),
            rgba(c, 0.7 * prog),
            rgba(c, 0.),
            32,
        );
    }
    let _ = f;
}

#[allow(clippy::too_many_arguments)]
fn gulpjaw(p: &mut Painter, s: &Scene, pos: Vec2, r: f32, t: f32, c: Rgb, f: &Foe, look: Vec2) {
    let brain = s.game.director.boss[1];
    let side = vec2(-look.y, look.x);
    let body = [0.08, 0.1, 0.2];
    let enraged = brain.phase >= 2;
    // Lure on its stalk: the brightest thing in the abyss.
    let stalk_base = pos - look * r * 0.2 + side * r * 0.1 - vec2(0., r * 0.7);
    let lure =
        pos + look * r * 1.2 + vec2(0., -r * 1.0) + vec2((t * 2.).sin() * 8., (t * 3.).cos() * 5.);
    let mid = (stalk_base + lure) * 0.5 + vec2(0., -r * 0.5);
    p.line(false, stalk_base, mid, 5., rgba([0.2, 0.25, 0.4], 1.));
    p.line(false, mid, lure, 4., rgba([0.2, 0.25, 0.4], 1.));
    let lure_glow = 0.8
        + 0.2 * (t * 6.).sin()
        + if brain.casting && brain.pattern == 0 {
            0.6
        } else {
            0.
        };
    p.glow(lure, 120. * lure_glow, rgba(c, 0.4 * lure_glow));
    p.disc(false, lure, 10., rgba([0.9, 1., 1.], 1.), 16);
    // Body: big dark fish with a pale glowing belly line.
    p.glow(
        pos,
        r * 2.5,
        rgba(if enraged { [1., 0.3, 0.3] } else { c }, 0.12),
    );
    let fin = pos - look * r * 1.15;
    let wag = (t * 5.).sin() * 0.4;
    p.tri(
        false,
        fin,
        fin - rotate(look, 0.7 + wag) * r * 0.9,
        fin - rotate(look, -0.7 + wag) * r * 0.9,
        rgba(body, 1.),
        rgba([0.15, 0.2, 0.35], 1.),
        rgba([0.15, 0.2, 0.35], 1.),
    );
    p.ellipse(
        false,
        pos,
        r * 1.2,
        r * 0.9,
        look.y.atan2(look.x),
        rgba([0.16, 0.2, 0.34], 1.),
        rgba(body, 1.),
        32,
    );
    for k in 0..7 {
        let f2 = k as f32 / 6. - 0.5;
        let q = pos + side * f2 * r * 1.2 + look * r * (0.1 - f2.abs() * 0.4);
        p.disc(true, q, 3., rgba(c, 0.6), 6);
    }
    // Gaping jaw with crooked teeth.
    let open = if brain.casting && matches!(brain.pattern, 1 | 3) && brain.step == 1 {
        1.
    } else {
        0.45 + 0.15 * (t * 2.).sin()
    };
    let mouth = pos + look * r * 0.85;
    let jaw_w = r * 0.8;
    p.tri(
        false,
        mouth + side * jaw_w,
        mouth - side * jaw_w,
        mouth + look * r * 0.5 * open,
        rgba([0.02, 0.02, 0.05], 1.),
        rgba([0.02, 0.02, 0.05], 1.),
        rgba([0.3, 0.05, 0.1], 1.),
    );
    for k in 0..7 {
        let f2 = k as f32 / 6. * 2. - 1.;
        let root = mouth + side * f2 * jaw_w * 0.9;
        let tip = root + look * (8. + (k % 3) as f32 * 4.) * (0.6 + open * 0.6);
        p.tri(
            false,
            root - side * 4.,
            root + side * 4.,
            tip,
            rgba([1., 1., 0.95], 1.),
            rgba([1., 1., 0.95], 1.),
            rgba([0.9, 0.95, 1.], 1.),
        );
    }
    let eye = pos + look * r * 0.35 + side * r * 0.45 - vec2(0., r * 0.25);
    p.disc(false, eye, r * 0.2, rgba([1., 1., 0.85], 1.), 16);
    p.disc(false, eye + look * 3., r * 0.09, rgba([0., 0., 0.], 1.), 10);
    p.glow(
        eye,
        r * 0.6,
        rgba(
            if enraged {
                [1., 0.3, 0.2]
            } else {
                [1., 1., 0.7]
            },
            0.35,
        ),
    );
    let _ = f;
}

fn player(p: &mut Painter, s: &Scene) {
    let g = s.game;
    let pos = lerp(g.player.prev, g.player.pos, s.alpha);
    let ch = g.character();
    let c = ch.color;
    let t = s.time;
    // Dash afterimages.
    for &(at, age) in &s.fx.ghosts {
        if age < 0.35 {
            let k = 1. - age / 0.35;
            p.glow(at, 26. * k, rgba(c, 0.5 * k));
            p.disc(false, at, 9. * k, rgba(c, 0.35 * k), 12);
        }
    }
    if g.player.dash_time > 0. {
        let back = pos - g.player.dash_dir * 46.;
        p.line_grad(true, back, pos, 18., rgba(c, 0.), rgba(c, 0.7));
    }
    let blink = if g.player.invuln > 0. && g.player.hurt <= 0. && g.player.dash_time <= 0. {
        0.55 + 0.45 * (t * 30.).sin().abs() * s.flashes + (1. - s.flashes) * 0.45
    } else {
        1.
    };
    let hurt = (g.player.hurt / 0.3).clamp(0., 1.) * s.flashes;
    let body = mix3(c, [1., 0.3, 0.3], hurt);
    let flicker = 1. + 0.06 * (t * 23.).sin() + 0.04 * (t * 37.).sin();
    // Lantern halo.
    p.glow(pos, 95. * flicker, rgba(body, 0.42 * blink));
    p.glow(pos, 34., rgba(mix3(body, [1.; 3], 0.5), 0.55 * blink));
    // Flame tail streams opposite to motion.
    let vel = g.player.vel;
    let back = if vel.length_squared() > 100. {
        -vel.normalize()
    } else {
        vec2(0., 1.)
    };
    let tail = pos + back * (14. + vel.length() * 0.03) + vec2((t * 9.).sin() * 2., 0.);
    let side = vec2(-back.y, back.x) * 8.;
    p.tri(
        false,
        pos + side,
        pos - side,
        tail,
        rgba(body, 0.9 * blink),
        rgba(body, 0.9 * blink),
        rgba(body, 0.),
    );
    // Character accessories.
    match g.config.character {
        1 => {
            for k in 0..10 {
                let a = k as f32 / 10. * std::f32::consts::TAU;
                let d = vec2(a.cos(), a.sin());
                p.tri(
                    false,
                    pos + d * 9. + vec2(-d.y, d.x) * 2.5,
                    pos + d * 15.,
                    pos + d * 9. - vec2(-d.y, d.x) * 2.5,
                    rgba(scale3(c, 0.7), blink),
                    rgba([1.; 3], blink),
                    rgba(scale3(c, 0.7), blink),
                );
            }
        }
        2 => {
            for k in 0..5 {
                let x = (k as f32 - 2.) * 4.;
                let wav = (t * 6. + k as f32).sin() * 3.;
                p.line(
                    false,
                    pos + vec2(x, 6.),
                    pos + vec2(x + wav, 20.),
                    2.,
                    rgba(c, 0.7 * blink),
                );
            }
        }
        3 => {
            let shell = pos + vec2(0., -3.) - g.player.facing * 6.;
            p.disc_grad(
                false,
                shell,
                11.,
                rgba([1., 0.75, 0.6], blink),
                rgba([0.6, 0.3, 0.25], blink),
                16,
            );
            p.arc(
                false,
                shell,
                4.,
                6.5,
                0.3,
                4.5,
                rgba([0.4, 0.15, 0.1], blink),
                rgba([0.4, 0.15, 0.1], blink),
                8,
            );
        }
        _ => {}
    }
    let bob = (t * 3.).sin() * 1.2 * s.motion;
    let pos = pos + vec2(0., bob);
    p.disc(false, pos, 14., rgba([0.05, 0.03, 0.08], 0.55 * blink), 20);
    p.disc_grad(
        false,
        pos,
        12.,
        rgba(mix3(body, [1.; 3], 0.85), blink),
        rgba(mix3(body, [1., 0.5, 0.2], 0.25), blink),
        24,
    );
    p.disc(
        false,
        pos + vec2(-3.5, -4.5),
        3.4,
        rgba([1.; 3], 0.85 * blink),
        8,
    );
    // Little determined eyes.
    let look = g.player.facing;
    let eye = pos + look * 2.5;
    for sgn in [-1., 1.] {
        p.ellipse(
            false,
            eye + vec2(sgn * 3.6, -0.5),
            1.6,
            2.6,
            0.,
            rgba([0.12, 0.08, 0.15], blink),
            rgba([0.12, 0.08, 0.15], blink),
            8,
        );
    }
    // Dash cooldown arc.
    let mods = g.mods;
    let total = 1.5 * mods.dash_cd;
    if g.player.dash_cd > 0. {
        let prog = 1. - g.player.dash_cd / total.max(0.01);
        let a0 = -std::f32::consts::FRAC_PI_2;
        p.arc(
            true,
            pos,
            17.,
            19.5,
            a0,
            a0 + prog * std::f32::consts::TAU,
            rgba(c, 0.5),
            rgba(c, 0.5),
            16,
        );
    }
}

fn weapons(p: &mut Painter, s: &Scene) {
    let g = s.game;
    let player = lerp(g.player.prev, g.player.pos, s.alpha);
    let t = s.time;
    for slot in g.weapons.iter().flatten() {
        let st = weapon_stats(slot.id, slot.level, slot.evolved);
        let area = st.area * g.mods.area;
        let c = slot.id.info().color;
        match slot.id {
            crate::sim::content::WeaponId::LanternAura => {
                let r = aura_radius(area);
                let pulse = 0.85 + 0.15 * (t * 3.).sin();
                let warm = if slot.evolved { [1., 0.85, 0.5] } else { c };
                p.disc_grad(
                    true,
                    player,
                    r,
                    rgba(warm, 0.1 * pulse),
                    rgba(warm, 0.03),
                    32,
                );
                p.ring(
                    true,
                    player,
                    r - 3.,
                    r + 1.,
                    rgba(warm, 0.0),
                    rgba(warm, 0.35),
                    32,
                );
                for k in 0..10 {
                    let a = t * 0.9 + k as f32 / 10. * std::f32::consts::TAU;
                    p.arc(
                        true,
                        player,
                        r - 2.,
                        r + 1.,
                        a,
                        a + 0.25,
                        rgba(warm, 0.5),
                        rgba(warm, 0.),
                        3,
                    );
                }
            }
            crate::sim::content::WeaponId::HaloOrbit if slot.active > 0. || slot.evolved => {
                let amount = st.amount + g.mods.amount;
                let radius = orbit_radius(area);
                let fade = if slot.evolved {
                    1.
                } else {
                    (slot.active / 0.3).min(1.)
                };
                let oc = if slot.evolved { [1., 0.75, 0.35] } else { c };
                p.ring(
                    true,
                    player,
                    radius - 1.,
                    radius + 1.,
                    rgba(oc, 0.12 * fade),
                    rgba(oc, 0.12 * fade),
                    32,
                );
                for k in 0..amount {
                    let a = slot.angle + k as f32 * std::f32::consts::TAU / amount as f32;
                    let q = player + vec2(a.cos(), a.sin()) * radius;
                    p.arc(
                        true,
                        player,
                        radius - 6.,
                        radius + 6.,
                        a - 0.5,
                        a,
                        rgba(oc, 0.),
                        rgba(oc, 0.0),
                        4,
                    );
                    for trail in 1..5 {
                        let ta = a - trail as f32 * 0.12;
                        let tq = player + vec2(ta.cos(), ta.sin()) * radius;
                        p.disc(
                            true,
                            tq,
                            9. * area * (1. - trail as f32 * 0.18),
                            rgba(oc, 0.16 * fade),
                            8,
                        );
                    }
                    p.glow(q, 36. * area, rgba(oc, 0.5 * fade));
                    p.disc_grad(
                        false,
                        q,
                        9. * area,
                        rgba([1., 1., 0.9], fade),
                        rgba(oc, fade),
                        12,
                    );
                }
            }
            _ => {}
        }
    }
    for l in &g.lashes {
        let k = 1. - l.age / 0.24;
        let c = [0.7, 1., 0.65];
        if l.full {
            let r = l.reach * (0.6 + 0.4 * (1. - k));
            p.ring(true, player, r * 0.6, r, rgba(c, 0.), rgba(c, 0.55 * k), 32);
            p.ring(
                false,
                player,
                r - 4.,
                r,
                rgba([0.85, 1., 0.8], 0.6 * k),
                rgba([0.85, 1., 0.8], 0.6 * k),
                32,
            );
        } else {
            let a = l.dir.y.atan2(l.dir.x);
            let sweep = 0.9;
            let a0 = a - sweep * 0.5 + (1. - k) * 0.3;
            p.arc(
                true,
                player,
                l.reach * 0.35,
                l.reach,
                a0,
                a0 + sweep,
                rgba(c, 0.),
                rgba(c, 0.55 * k),
                10,
            );
            p.arc(
                false,
                player,
                l.reach - 6.,
                l.reach,
                a0,
                a0 + sweep,
                rgba([0.9, 1., 0.85], 0.8 * k),
                rgba([0.9, 1., 0.85], 0.8 * k),
                10,
            );
        }
    }
    for z in &g.zaps {
        let k = 1. - z.age / 0.22;
        let c = if z.evolved {
            [0.75, 0.8, 1.]
        } else {
            [0.65, 0.85, 1.]
        };
        let mut prev = z.a;
        let n = 6;
        let d = z.b - z.a;
        let side = vec2(-d.y, d.x).normalize_or_zero();
        for i in 1..=n {
            let f = i as f32 / n as f32;
            let jitter = if i == n {
                0.
            } else {
                (hash(z.seed as i32, i, (t * 30.) as u32) - 0.5) * 22.
            };
            let q = z.a + d * f + side * jitter;
            p.soft_line(prev, q, 9., rgba(c, 0.8 * k));
            p.line(true, prev, q, 2., rgba([1.; 3], 0.9 * k));
            prev = q;
        }
        p.glow(z.b, 26., rgba(c, 0.6 * k));
    }
    for sh in &g.shots {
        let pos = lerp(sh.prev, sh.pos, s.alpha);
        if !s.view.visible(pos, 60.) {
            continue;
        }
        let c = sh.weapon.info().color;
        match sh.kind {
            ShotKind::Wisp => {
                let dir = sh.vel.normalize_or_zero();
                for k in 1..5 {
                    let q = pos - dir * k as f32 * 5.;
                    p.disc(true, q, 5. - k as f32, rgba(c, 0.35 - k as f32 * 0.06), 8);
                }
                p.glow(pos, 22., rgba(c, 0.55));
                p.disc_grad(false, pos, 4.5, rgba([1.; 3], 1.), rgba(c, 1.), 10);
            }
            ShotKind::Lance => {
                let dir = sh.vel.normalize_or_zero();
                let c = [0.8, 0.88, 1.];
                p.soft_line(pos - dir * 54., pos + dir * 6., 16., rgba(c, 0.7));
                p.line(
                    true,
                    pos - dir * 40.,
                    pos + dir * 4.,
                    3.,
                    rgba([1.; 3], 0.95),
                );
                p.glow(pos, 30., rgba(c, 0.5));
            }
            ShotKind::Bubble => {
                let f = (sh.age / sh.life).clamp(0., 1.);
                let height = (f * std::f32::consts::PI).sin() * 60.;
                let air = pos - vec2(0., height);
                // Landing marker.
                p.ring(
                    false,
                    sh.target,
                    sh.radius * 0.9,
                    sh.radius,
                    rgba(c, 0.25 * f),
                    rgba(c, 0.25 * f),
                    24,
                );
                p.disc(false, sh.target, sh.radius * f, rgba(c, 0.06), 24);
                p.ellipse(
                    false,
                    pos + vec2(0., 4.),
                    8.,
                    3.,
                    0.,
                    [0., 0., 0., 0.25],
                    [0., 0., 0., 0.],
                    10,
                );
                let r = 9. + 2. * (t * 12.).sin();
                p.glow(air, 30., rgba(c, 0.35));
                p.disc_grad(
                    false,
                    air,
                    r,
                    rgba(mix3(c, [1.; 3], 0.3), 0.35),
                    rgba(c, 0.55),
                    14,
                );
                p.ring(
                    false,
                    air,
                    r - 1.5,
                    r,
                    rgba([0.9, 0.95, 1.], 0.9),
                    rgba([0.9, 0.95, 1.], 0.9),
                    14,
                );
                p.disc(
                    false,
                    air + vec2(-r * 0.35, -r * 0.35),
                    r * 0.25,
                    rgba([1.; 3], 0.9),
                    6,
                );
            }
            ShotKind::Shell => {
                let r = sh.radius;
                let spin = sh.spin * 14.;
                // Evolved Nautilus shells pick up a pearly gold sheen.
                let evolved = g.weapon(sh.weapon).is_some_and(|s| s.evolved);
                let shell = if evolved {
                    mix3(c, [1., 0.88, 0.6], 0.55)
                } else {
                    c
                };
                p.glow(pos, r * 3., rgba(shell, 0.4));
                p.disc_grad(
                    false,
                    pos,
                    r,
                    rgba(mix3(shell, [1.; 3], 0.4), 1.),
                    rgba(scale3(shell, 0.6), 1.),
                    14,
                );
                for k in 0..3 {
                    let a = spin + k as f32 * 2.1;
                    p.line(
                        false,
                        pos,
                        pos + vec2(a.cos(), a.sin()) * r,
                        2.,
                        rgba(scale3(shell, 0.35), 1.),
                    );
                }
            }
            ShotKind::Star => {
                let r = sh.radius;
                p.glow(pos, r * 3.2, rgba(c, 0.45));
                p.star(
                    false,
                    pos,
                    r,
                    r * 0.45,
                    5,
                    sh.age * 9.,
                    rgba([1., 1., 0.85], 1.),
                    rgba(c, 1.),
                );
            }
            ShotKind::Stardust => {
                let k = 1. - sh.age / sh.life;
                p.star(
                    true,
                    pos,
                    6. * k + 2.,
                    2.,
                    4,
                    sh.age * 12.,
                    rgba([1., 1., 0.85], 0.9 * k),
                    rgba(c, 0.5 * k),
                );
            }
        }
    }
}

fn bullets(p: &mut Painter, s: &Scene) {
    for b in &s.game.bullets {
        let pos = lerp(b.prev, b.pos, s.alpha);
        if !s.view.visible(pos, 20.) {
            continue;
        }
        let c = match b.style {
            0 => [1., 0.35, 0.75],
            1 => [1., 0.4, 0.55],
            _ => [1., 0.3, 0.3],
        };
        // Hostile fire always reads the same: dark rim, hot core, warning ring.
        let pulse = 0.5 + 0.5 * (s.time * 10. + b.age * 3.).sin();
        let dir = b.vel.normalize_or_zero();
        p.line_grad(
            true,
            pos - dir * b.radius * 3.,
            pos,
            b.radius * 1.4,
            rgba(c, 0.),
            rgba(c, 0.5),
        );
        p.glow_small(pos, b.radius * 3.2, rgba(c, 0.5));
        p.disc(
            false,
            pos,
            b.radius + 2.5,
            rgba([0.12, 0.0, 0.06], 0.95),
            12,
        );
        p.disc_grad(
            false,
            pos,
            b.radius,
            rgba([1., 0.97, 0.97], 1.),
            rgba(c, 1.),
            12,
        );
        p.ring(
            true,
            pos,
            b.radius + 3.,
            b.radius + 4.5 + pulse * 2.,
            rgba(c, 0.7),
            rgba(c, 0.),
            12,
        );
    }
}

fn rings(p: &mut Painter, s: &Scene) {
    for r in &s.fx.rings {
        if !r.live {
            continue;
        }
        let t = crate::gfx::ease::out_cubic(r.t());
        let radius = r.r0 + (r.r1 - r.r0) * t;
        let a = (1. - r.t()).powf(1.4);
        p.ring(
            true,
            r.pos,
            (radius - r.width).max(0.),
            radius,
            rgba(r.color, 0.),
            rgba(r.color, 0.7 * a),
            32,
        );
    }
}

fn rotate(v: Vec2, angle: f32) -> Vec2 {
    let (sin, cos) = angle.sin_cos();
    vec2(v.x * cos - v.y * sin, v.x * sin + v.y * cos)
}

/// Draw a lone pickup-style chest at a screen position (used by menus).
pub fn chest_icon(p: &mut Painter, at: Vec2, scale: f32, t: f32, open: f32) {
    let gold = [1., 0.82, 0.35];
    p.glow(at, 120. * scale, rgba(gold, 0.35 + 0.25 * open));
    for k in 0..10 {
        let a = k as f32 / 10. * std::f32::consts::TAU + t * 0.5;
        let d = vec2(a.cos(), a.sin());
        let len = (120. + 60. * open) * scale;
        p.tri(
            true,
            at,
            at + rotate(d, 0.1) * len,
            at + rotate(d, -0.1) * len,
            rgba(gold, 0.3 * (0.4 + open)),
            rgba(gold, 0.),
            rgba(gold, 0.),
        );
    }
    let s = scale;
    p.round_rect_v(
        false,
        at + vec2(-32., -10.) * s,
        vec2(64., 36.) * s,
        6. * s,
        rgba([0.6, 0.32, 0.16], 1.),
        rgba([0.38, 0.18, 0.1], 1.),
    );
    let lid = at + vec2(-34., -26. - open * 16.) * s;
    p.round_rect_v(
        false,
        lid,
        vec2(68., 20.) * s,
        8. * s,
        rgba([0.78, 0.45, 0.22], 1.),
        rgba([0.55, 0.3, 0.15], 1.),
    );
    p.rect(
        false,
        at + vec2(-34., -8.) * s,
        vec2(68., 5.) * s,
        rgba(gold, 1.),
    );
    p.rect(
        false,
        at + vec2(-6., -12.) * s,
        vec2(12., 16.) * s,
        rgba(gold, 1.),
    );
    if open > 0. {
        p.glow(
            at + vec2(0., -12.) * s,
            70. * s * open,
            rgba([1., 1., 0.8], 0.6 * open),
        );
    }
}
