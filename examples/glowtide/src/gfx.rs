//! Centralized batched drawing: every world/HUD shape and every glow goes
//! through [`Painter`], which records into two reusable triangle streams and
//! submits each as **one** `effects::draw_triangles` batch:
//!
//! - the *alpha* stream for solid shapes (shares the ordinary 2D batch), and
//! - the *glow* stream, additively blended, for light, halos and sparks.
//!
//! This is the seam for the upcoming post-processing pipeline: today glow is
//! faked with additive radial falloff; with bloom, [`Painter::flush_glow`]
//! becomes the emissive pass and the soft falloff radii can shrink.
#![allow(clippy::too_many_arguments)] // Drawing helpers take explicit geometry.
use ferravex_render::effects::{EffectBlend, EffectStyle, draw_triangles};
use ferravex_render::effects::{RibbonTriangle, RibbonVertex};
use ferravex_render::prelude::{Color, Vec2, vec2};

pub type Rgba = [f32; 4];

const TABLE: usize = 48;

pub fn rgba(c: [f32; 3], a: f32) -> Rgba {
    [c[0], c[1], c[2], a]
}

pub fn mix3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

pub fn scale3(a: [f32; 3], k: f32) -> [f32; 3] {
    [a[0] * k, a[1] * k, a[2] * k]
}

pub fn with_alpha(c: Rgba, a: f32) -> Rgba {
    [c[0], c[1], c[2], c[3] * a]
}

pub fn color(c: Rgba) -> Color {
    Color::new(c[0], c[1], c[2], c[3])
}

/// Easing helpers (presentation only). Centralized for the coming tween kit.
pub mod ease {
    pub fn out_cubic(t: f32) -> f32 {
        let t = t.clamp(0., 1.);
        1. - (1. - t).powi(3)
    }
    pub fn in_out_sine(t: f32) -> f32 {
        let t = t.clamp(0., 1.);
        -((std::f32::consts::PI * t).cos() - 1.) * 0.5
    }
    pub fn out_back(t: f32) -> f32 {
        let t = t.clamp(0., 1.);
        let c1 = 1.70158;
        let c3 = c1 + 1.;
        1. + c3 * (t - 1.).powi(3) + c1 * (t - 1.).powi(2)
    }
    pub fn out_elastic(t: f32) -> f32 {
        let t = t.clamp(0., 1.);
        if t == 0. || t == 1. {
            return t;
        }
        let c4 = std::f32::consts::TAU / 3.;
        2f32.powf(-10. * t) * ((t * 10. - 0.75) * c4).sin() + 1.
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PaintStats {
    pub triangles: u32,
    pub batches: u32,
}

pub struct Painter {
    alpha: Vec<RibbonTriangle>,
    glow: Vec<RibbonTriangle>,
    unit: [Vec2; TABLE],
    pub stats: PaintStats,
    /// Level-of-detail divisor for crowded frames (1 = full detail).
    pub lod: usize,
}

#[inline]
fn vert(p: Vec2, c: Rgba) -> RibbonVertex {
    RibbonVertex {
        position: [p.x, p.y, 0.],
        color: c,
        uv: [0.; 2],
    }
}

impl Painter {
    pub fn new() -> Self {
        let mut unit = [Vec2::ZERO; TABLE];
        for (i, u) in unit.iter_mut().enumerate() {
            let a = i as f32 / TABLE as f32 * std::f32::consts::TAU;
            *u = vec2(a.cos(), a.sin());
        }
        Self {
            alpha: Vec::with_capacity(32768),
            glow: Vec::with_capacity(24576),
            unit,
            stats: PaintStats::default(),
            lod: 1,
        }
    }

    pub fn begin_frame(&mut self) {
        self.stats = PaintStats::default();
    }

    #[inline]
    fn buf(&mut self, glow: bool) -> &mut Vec<RibbonTriangle> {
        if glow {
            &mut self.glow
        } else {
            &mut self.alpha
        }
    }

    #[inline]
    pub fn tri(&mut self, glow: bool, a: Vec2, b: Vec2, c: Vec2, ca: Rgba, cb: Rgba, cc: Rgba) {
        self.buf(glow).push(RibbonTriangle {
            vertices: [vert(a, ca), vert(b, cb), vert(c, cc)],
        });
    }

    /// Quad `a b c d` in winding order with per-corner colors.
    #[inline]
    pub fn quad(&mut self, glow: bool, p: [Vec2; 4], c: [Rgba; 4]) {
        self.tri(glow, p[0], p[1], p[2], c[0], c[1], c[2]);
        self.tri(glow, p[0], p[2], p[3], c[0], c[2], c[3]);
    }

    pub fn rect(&mut self, glow: bool, min: Vec2, size: Vec2, c: Rgba) {
        let max = min + size;
        self.quad(
            glow,
            [min, vec2(max.x, min.y), max, vec2(min.x, max.y)],
            [c; 4],
        );
    }

    pub fn rect_v(&mut self, glow: bool, min: Vec2, size: Vec2, top: Rgba, bottom: Rgba) {
        let max = min + size;
        self.quad(
            glow,
            [min, vec2(max.x, min.y), max, vec2(min.x, max.y)],
            [top, top, bottom, bottom],
        );
    }

    pub fn rect_h(&mut self, glow: bool, min: Vec2, size: Vec2, left: Rgba, right: Rgba) {
        let max = min + size;
        self.quad(
            glow,
            [min, vec2(max.x, min.y), max, vec2(min.x, max.y)],
            [left, right, right, left],
        );
    }

    fn step(&self, segs: usize) -> usize {
        let segs = (segs / self.lod.max(1)).max(6);
        let segs = [6, 8, 12, 16, 24, 48]
            .into_iter()
            .find(|s| *s >= segs)
            .unwrap_or(48);
        TABLE / segs
    }

    /// Filled disc with center/rim colors (a radial gradient).
    pub fn disc_grad(
        &mut self,
        glow: bool,
        center: Vec2,
        r: f32,
        inner: Rgba,
        outer: Rgba,
        segs: usize,
    ) {
        let step = self.step(segs);
        let mut i = 0;
        while i < TABLE {
            let j = (i + step) % TABLE;
            let a = center + self.unit[i] * r;
            let b = center + self.unit[j] * r;
            self.tri(glow, center, a, b, inner, outer, outer);
            i += step;
        }
    }

    pub fn disc(&mut self, glow: bool, center: Vec2, r: f32, c: Rgba, segs: usize) {
        self.disc_grad(glow, center, r, c, c, segs);
    }

    /// Ellipse rotated by `angle`, with a radial gradient.
    #[allow(clippy::too_many_arguments)]
    pub fn ellipse(
        &mut self,
        glow: bool,
        center: Vec2,
        rx: f32,
        ry: f32,
        angle: f32,
        inner: Rgba,
        outer: Rgba,
        segs: usize,
    ) {
        let (s, c) = angle.sin_cos();
        let step = self.step(segs);
        let mut i = 0;
        while i < TABLE {
            let j = (i + step) % TABLE;
            let map = |u: Vec2| {
                let p = vec2(u.x * rx, u.y * ry);
                center + vec2(p.x * c - p.y * s, p.x * s + p.y * c)
            };
            let (a, b) = (map(self.unit[i]), map(self.unit[j]));
            self.tri(glow, center, a, b, inner, outer, outer);
            i += step;
        }
    }

    /// Annulus from `r0` to `r1` with inner/outer colors.
    #[allow(clippy::too_many_arguments)]
    pub fn ring(
        &mut self,
        glow: bool,
        center: Vec2,
        r0: f32,
        r1: f32,
        c0: Rgba,
        c1: Rgba,
        segs: usize,
    ) {
        let step = self.step(segs);
        let mut i = 0;
        while i < TABLE {
            let j = (i + step) % TABLE;
            let (ui, uj) = (self.unit[i], self.unit[j]);
            self.quad(
                glow,
                [
                    center + ui * r0,
                    center + ui * r1,
                    center + uj * r1,
                    center + uj * r0,
                ],
                [c0, c1, c1, c0],
            );
            i += step;
        }
    }

    /// Partial annulus between angles `a0..a1` (radians).
    #[allow(clippy::too_many_arguments)]
    pub fn arc(
        &mut self,
        glow: bool,
        center: Vec2,
        r0: f32,
        r1: f32,
        a0: f32,
        a1: f32,
        c0: Rgba,
        c1: Rgba,
        segs: usize,
    ) {
        let n = segs.max(2);
        let mut prev = vec2(a0.cos(), a0.sin());
        for k in 1..=n {
            let a = a0 + (a1 - a0) * k as f32 / n as f32;
            let u = vec2(a.cos(), a.sin());
            self.quad(
                glow,
                [
                    center + prev * r0,
                    center + prev * r1,
                    center + u * r1,
                    center + u * r0,
                ],
                [c0, c1, c1, c0],
            );
            prev = u;
        }
    }

    /// Soft additive light: bright core falling smoothly to nothing.
    /// Three-stage falloff approximates an inverse-square halo without banding
    /// polygons on large radii.
    pub fn glow(&mut self, center: Vec2, r: f32, c: Rgba) {
        if c[3] <= 0.002 || r <= 0.5 {
            return;
        }
        if r < 40. {
            return self.glow_small(center, r, c);
        }
        let segs = if r > 260. {
            48
        } else if r > 90. {
            24
        } else {
            16
        };
        let c1 = with_alpha(c, 0.42);
        let c2 = with_alpha(c, 0.12);
        let zero = with_alpha(c, 0.);
        self.disc_grad(true, center, r * 0.16, c, c1, segs);
        self.ring(true, center, r * 0.16, r * 0.45, c1, c2, segs);
        self.ring(true, center, r * 0.45, r, c2, zero, segs);
    }

    /// Cheaper two-stage halo for the many small lights (foes, shots).
    pub fn glow_small(&mut self, center: Vec2, r: f32, c: Rgba) {
        if c[3] <= 0.002 || r <= 0.5 {
            return;
        }
        let mid = with_alpha(c, 0.3);
        self.disc_grad(true, center, r * 0.32, c, mid, 12);
        self.ring(true, center, r * 0.32, r, mid, with_alpha(c, 0.), 12);
    }

    /// Single-fan glint for very numerous lights (XP motes).
    pub fn glint(&mut self, center: Vec2, r: f32, c: Rgba) {
        self.disc_grad(true, center, r, c, with_alpha(c, 0.), 8);
    }

    pub fn line(&mut self, glow: bool, a: Vec2, b: Vec2, w: f32, c: Rgba) {
        let d = (b - a).normalize_or_zero();
        let n = vec2(-d.y, d.x) * w * 0.5;
        self.quad(glow, [a + n, b + n, b - n, a - n], [c; 4]);
    }

    pub fn line_grad(&mut self, glow: bool, a: Vec2, b: Vec2, w: f32, ca: Rgba, cb: Rgba) {
        let d = (b - a).normalize_or_zero();
        let n = vec2(-d.y, d.x) * w * 0.5;
        self.quad(glow, [a + n, b + n, b - n, a - n], [ca, cb, cb, ca]);
    }

    /// Additive line with a bright core and soft edges.
    pub fn soft_line(&mut self, a: Vec2, b: Vec2, w: f32, c: Rgba) {
        let d = (b - a).normalize_or_zero();
        let n = vec2(-d.y, d.x);
        let core = n * w * 0.18;
        let edge = n * w * 0.5;
        let zero = with_alpha(c, 0.);
        self.quad(true, [a + core, b + core, b - core, a - core], [c; 4]);
        self.quad(
            true,
            [a + edge, b + edge, b + core, a + core],
            [zero, zero, c, c],
        );
        self.quad(
            true,
            [a - core, b - core, b - edge, a - edge],
            [c, c, zero, zero],
        );
    }

    /// Regular polygon (e.g. stars when alternating radii via `star`).
    pub fn star(
        &mut self,
        glow: bool,
        center: Vec2,
        r_out: f32,
        r_in: f32,
        points: usize,
        rot: f32,
        inner: Rgba,
        outer: Rgba,
    ) {
        let n = points.max(3) * 2;
        for k in 0..n {
            let a0 = rot + k as f32 / n as f32 * std::f32::consts::TAU;
            let a1 = rot + (k + 1) as f32 / n as f32 * std::f32::consts::TAU;
            let r0 = if k % 2 == 0 { r_out } else { r_in };
            let r1 = if k % 2 == 0 { r_in } else { r_out };
            self.tri(
                glow,
                center,
                center + vec2(a0.cos(), a0.sin()) * r0,
                center + vec2(a1.cos(), a1.sin()) * r1,
                inner,
                outer,
                outer,
            );
        }
    }

    /// Rounded rectangle with a vertical gradient.
    pub fn round_rect_v(
        &mut self,
        glow: bool,
        min: Vec2,
        size: Vec2,
        radius: f32,
        top: Rgba,
        bottom: Rgba,
    ) {
        let r = radius.min(size.x * 0.5).min(size.y * 0.5).max(0.);
        let lerp = |y: f32| {
            let t = ((y - min.y) / size.y.max(1e-3)).clamp(0., 1.);
            [
                top[0] + (bottom[0] - top[0]) * t,
                top[1] + (bottom[1] - top[1]) * t,
                top[2] + (bottom[2] - top[2]) * t,
                top[3] + (bottom[3] - top[3]) * t,
            ]
        };
        // Center column and side columns.
        let inner_top = min.y + r;
        let inner_bottom = min.y + size.y - r;
        let (ct, cb) = (lerp(min.y), lerp(min.y + size.y));
        self.quad(
            glow,
            [
                vec2(min.x + r, min.y),
                vec2(min.x + size.x - r, min.y),
                vec2(min.x + size.x - r, min.y + size.y),
                vec2(min.x + r, min.y + size.y),
            ],
            [ct, ct, cb, cb],
        );
        let (it, ib) = (lerp(inner_top), lerp(inner_bottom));
        for x in [min.x, min.x + size.x - r] {
            self.quad(
                glow,
                [
                    vec2(x, inner_top),
                    vec2(x + r, inner_top),
                    vec2(x + r, inner_bottom),
                    vec2(x, inner_bottom),
                ],
                [it, it, ib, ib],
            );
        }
        if r <= 0.5 {
            return;
        }
        let corners = [
            (vec2(min.x + r, min.y + r), std::f32::consts::PI),
            (
                vec2(min.x + size.x - r, min.y + r),
                std::f32::consts::PI * 1.5,
            ),
            (vec2(min.x + size.x - r, min.y + size.y - r), 0.),
            (
                vec2(min.x + r, min.y + size.y - r),
                std::f32::consts::FRAC_PI_2,
            ),
        ];
        let n = if r > 10. { 6 } else { 4 };
        for (c, start) in corners {
            let cc = lerp(c.y);
            for k in 0..n {
                let a0 = start + k as f32 / n as f32 * std::f32::consts::FRAC_PI_2;
                let a1 = start + (k + 1) as f32 / n as f32 * std::f32::consts::FRAC_PI_2;
                let p0 = c + vec2(a0.cos(), a0.sin()) * r;
                let p1 = c + vec2(a1.cos(), a1.sin()) * r;
                self.tri(glow, c, p0, p1, cc, lerp(p0.y), lerp(p1.y));
            }
        }
    }

    /// Outline of a rounded rectangle, from `inset` inside the edge to `outset` outside.
    #[allow(clippy::too_many_arguments)]
    pub fn round_rect_band(
        &mut self,
        glow: bool,
        min: Vec2,
        size: Vec2,
        radius: f32,
        inset: f32,
        outset: f32,
        c_in: Rgba,
        c_out: Rgba,
    ) {
        let r = radius.min(size.x * 0.5).min(size.y * 0.5).max(0.);
        let centers = [
            (vec2(min.x + r, min.y + r), std::f32::consts::PI),
            (
                vec2(min.x + size.x - r, min.y + r),
                std::f32::consts::PI * 1.5,
            ),
            (vec2(min.x + size.x - r, min.y + size.y - r), 0.),
            (
                vec2(min.x + r, min.y + size.y - r),
                std::f32::consts::FRAC_PI_2,
            ),
        ];
        let r0 = (r - inset).max(0.);
        let r1 = r + outset;
        let n = 5;
        let mut first: Option<(Vec2, Vec2)> = None;
        let mut last: Option<(Vec2, Vec2)> = None;
        for (c, start) in centers {
            for k in 0..=n {
                let a = start + k as f32 / n as f32 * std::f32::consts::FRAC_PI_2;
                let u = vec2(a.cos(), a.sin());
                let pair = (c + u * r0, c + u * r1);
                if let Some(prev) = last {
                    self.quad(
                        glow,
                        [prev.0, prev.1, pair.1, pair.0],
                        [c_in, c_out, c_out, c_in],
                    );
                } else {
                    first = Some(pair);
                }
                last = Some(pair);
            }
        }
        if let (Some(a), Some(b)) = (last, first) {
            self.quad(glow, [a.0, a.1, b.1, b.0], [c_in, c_out, c_out, c_in]);
        }
    }

    /// Additive halo around a rounded rectangle.
    pub fn glow_rect(&mut self, min: Vec2, size: Vec2, radius: f32, spread: f32, c: Rgba) {
        self.round_rect_band(true, min, size, radius, 0., spread, c, with_alpha(c, 0.));
    }

    pub fn flush_alpha(&mut self) {
        if self.alpha.is_empty() {
            return;
        }
        let report = draw_triangles(self.alpha.drain(..), EffectStyle::default());
        self.stats.triangles += report.triangles;
        self.stats.batches += 1;
    }

    pub fn flush_glow(&mut self) {
        if self.glow.is_empty() {
            return;
        }
        let report = draw_triangles(
            self.glow.drain(..),
            EffectStyle {
                blend: EffectBlend::Additive,
                depth_test: Some(false),
                depth_write: false,
            },
        );
        self.stats.triangles += report.triangles;
        self.stats.batches += 1;
    }

    /// Alpha shapes first, then their light on top.
    pub fn flush(&mut self) {
        self.flush_alpha();
        self.flush_glow();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn painter_reuses_storage_and_counts_glow_geometry() {
        let mut p = Painter::new();
        let cap = (p.alpha.capacity(), p.glow.capacity());
        p.glow(Vec2::ZERO, 40., [1., 1., 1., 1.]);
        p.round_rect_v(false, Vec2::ZERO, vec2(100., 40.), 8., [1.; 4], [1.; 4]);
        assert!(p.glow.len() >= 16 * 3);
        assert!(!p.alpha.is_empty());
        p.alpha.clear();
        p.glow.clear();
        assert_eq!((p.alpha.capacity(), p.glow.capacity()), cap);
        assert!((ease::out_back(1.) - 1.).abs() < 1e-5);
        assert!(ease::out_cubic(0.) == 0.);
    }
}
