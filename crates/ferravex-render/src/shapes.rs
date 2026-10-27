//! Anti-aliased immediate 2D shapes: rounded rectangles, circles, rings, arcs,
//! capsules, polylines, gradients, soft shadows and additive glows.
//!
//! Smooth edges come from geometry alone: each edge gets a thin fringe strip
//! whose outer vertices are transparent, about one device pixel wide. No texture,
//! shader or render-state change is needed, so these draws batch with ordinary
//! [`crate::draw_rectangle`] geometry and respect the current 2D camera. Every
//! call writes straight into the frame's existing vertex stream and performs no
//! heap allocation of its own.
//!
//! Vertex counts are bounded per call (see each function). Curves use 2–16
//! segments per quarter turn, chosen from the on-screen radius. Coordinates are
//! the current camera's logical units (pixels with the default camera).
//! Non-finite or degenerate input draws nothing.
//!
//! ```
//! use ferravex_render::{Color, Rect, vec2, shapes::{self, Fill, Shadow}};
//! let card = Rect::new(40., 40., 240., 120.);
//! shapes::drop_shadow(card, 12., Shadow::soft(Color::new(0., 0., 0., 0.45)));
//! shapes::fill_rounded_rect(card, 12., Fill::vertical(card,
//!     Color::from_hex(0x2b3550), Color::from_hex(0x1b2133)));
//! shapes::stroke_rounded_rect(card, 12., 1., Color::new(1., 1., 1., 0.12));
//! shapes::fill_circle(vec2(80., 100.), 16., Color::from_hex(0xf7b553));
//! shapes::glow_disc(vec2(80., 100.), 48., Color::new(1., 0.7, 0.3, 0.6));
//! ```
use crate::{Color, Rect, State, Vec2, Vec3, effects, vec2};
use std::f32::consts::{FRAC_PI_2, PI, TAU};

/// Paint for filled shapes. Gradients are evaluated per vertex and interpolated
/// by the GPU; linear gradients are exact on these convex fans, while radial
/// gradients are exact along circle spokes and approximate inside other shapes.
///
/// ```
/// use ferravex_render::{Color, Rect, vec2, shapes::Fill};
/// let fill = Fill::linear(vec2(0., 0.), vec2(10., 0.), Color::new(0., 0., 0., 1.),
///     Color::new(1., 1., 1., 1.));
/// assert_eq!(fill.at(vec2(5., 3.)).r, 0.5);
/// let solid: Fill = Color::new(1., 0., 0., 1.).into();
/// assert_eq!(solid.at(vec2(99., 99.)).r, 1.);
/// let bar = Fill::vertical(Rect::new(0., 10., 4., 20.), Color::new(0., 0., 0., 1.),
///     Color::new(1., 1., 1., 1.));
/// assert_eq!(bar.at(vec2(2., 30.)).g, 1.);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fill {
    /// One color everywhere.
    Solid(Color),
    /// Interpolates `start` at `from` to `end` at `to`, clamped beyond them.
    Linear {
        from: Vec2,
        to: Vec2,
        start: Color,
        end: Color,
    },
    /// Interpolates `inner` at `center` to `outer` at `radius`, clamped beyond.
    Radial {
        center: Vec2,
        radius: f32,
        inner: Color,
        outer: Color,
    },
}

impl From<Color> for Fill {
    fn from(color: Color) -> Self {
        Self::Solid(color)
    }
}

impl Fill {
    /// Linear gradient between two points.
    pub fn linear(from: Vec2, to: Vec2, start: Color, end: Color) -> Self {
        Self::Linear {
            from,
            to,
            start,
            end,
        }
    }
    /// Top-to-bottom gradient across `rect`.
    pub fn vertical(rect: Rect, top: Color, bottom: Color) -> Self {
        Self::linear(
            vec2(rect.x, rect.y),
            vec2(rect.x, rect.y + rect.h),
            top,
            bottom,
        )
    }
    /// Left-to-right gradient across `rect`.
    pub fn horizontal(rect: Rect, left: Color, right: Color) -> Self {
        Self::linear(
            vec2(rect.x, rect.y),
            vec2(rect.x + rect.w, rect.y),
            left,
            right,
        )
    }
    /// Radial gradient from `center` outward to `radius`.
    pub fn radial(center: Vec2, radius: f32, inner: Color, outer: Color) -> Self {
        Self::Radial {
            center,
            radius,
            inner,
            outer,
        }
    }
    /// Color at a point.
    pub fn at(&self, p: Vec2) -> Color {
        match *self {
            Self::Solid(c) => c,
            Self::Linear {
                from,
                to,
                start,
                end,
            } => {
                let axis = to - from;
                let len2 = axis.length_squared();
                let t = if len2 > 0. {
                    ((p - from).dot(axis) / len2).clamp(0., 1.)
                } else {
                    0.
                };
                mix(start, end, t)
            }
            Self::Radial {
                center,
                radius,
                inner,
                outer,
            } => {
                let t = if radius > 0. {
                    ((p - center).length() / radius).clamp(0., 1.)
                } else {
                    1.
                };
                mix(inner, outer, t)
            }
        }
    }
    fn is_finite(&self) -> bool {
        let c = |c: Color| c.r.is_finite() && c.g.is_finite() && c.b.is_finite() && c.a.is_finite();
        match *self {
            Self::Solid(a) => c(a),
            Self::Linear {
                from,
                to,
                start,
                end,
            } => from.is_finite() && to.is_finite() && c(start) && c(end),
            Self::Radial {
                center,
                radius,
                inner,
                outer,
            } => center.is_finite() && radius.is_finite() && c(inner) && c(outer),
        }
    }
}

/// Linear color interpolation (`t` is not clamped).
pub fn mix(a: Color, b: Color, t: f32) -> Color {
    Color::new(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
        a.a + (b.a - a.a) * t,
    )
}

fn fade(c: Color, alpha: f32) -> Color {
    Color::new(c.r, c.g, c.b, c.a * alpha)
}

/// Soft drop shadow parameters for [`drop_shadow`].
///
/// ```
/// use ferravex_render::{Color, vec2, shapes::Shadow};
/// let s = Shadow::soft(Color::new(0., 0., 0., 0.5)).offset(vec2(0., 6.)).blur(18.);
/// assert_eq!((s.blur, s.offset.y), (18., 6.));
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shadow {
    /// Shadow color at full coverage.
    pub color: Color,
    /// Displacement of the shadow from the caster.
    pub offset: Vec2,
    /// Width of the soft falloff, centered on the (spread) edge.
    pub blur: f32,
    /// Grows (or with negative values shrinks) the shadow before blurring.
    pub spread: f32,
}

impl Shadow {
    /// A gentle card shadow: 4 units down, 16 units of blur.
    pub fn soft(color: Color) -> Self {
        Self {
            color,
            offset: vec2(0., 4.),
            blur: 16.,
            spread: 0.,
        }
    }
    pub fn offset(mut self, offset: Vec2) -> Self {
        self.offset = offset;
        self
    }
    pub fn blur(mut self, blur: f32) -> Self {
        self.blur = blur;
        self
    }
    pub fn spread(mut self, spread: f32) -> Self {
        self.spread = spread;
        self
    }
}

/// How a [`polyline`] turns a corner.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Join {
    /// Sharp corners, falling back to [`Join::Bevel`] beyond 4× the half width.
    #[default]
    Miter,
    /// Corners are cut flat.
    Bevel,
    /// Corners are rounded.
    Round,
}

/// How a [`polyline`] or [`stroke_arc`] ends.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Cap {
    /// Ends exactly at the end point.
    #[default]
    Butt,
    /// Extends half the width past the end point.
    Square,
    /// Semicircular end.
    Round,
}

/// Line style for [`polyline`] and [`stroke_arc`].
///
/// ```
/// use ferravex_render::{Color, shapes::{Cap, Join, Stroke}};
/// let s = Stroke::new(3., Color::new(1., 1., 1., 1.)).join(Join::Round).cap(Cap::Round);
/// assert!(!s.closed);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stroke {
    pub width: f32,
    pub color: Color,
    pub join: Join,
    pub cap: Cap,
    /// Connect the last point back to the first.
    pub closed: bool,
}

impl Stroke {
    pub fn new(width: f32, color: Color) -> Self {
        Self {
            width,
            color,
            join: Join::Miter,
            cap: Cap::Butt,
            closed: false,
        }
    }
    pub fn join(mut self, join: Join) -> Self {
        self.join = join;
        self
    }
    pub fn cap(mut self, cap: Cap) -> Self {
        self.cap = cap;
        self
    }
    pub fn closed(mut self, closed: bool) -> Self {
        self.closed = closed;
        self
    }
}

// ---------------------------------------------------------------------------
// Vertex sink shared by shapes, styled text and the UI kit.

/// Appends immediate triangles to the frame stream with the same transform,
/// lighting and look rules as [`crate::draw_triangle`], recording one batch.
pub(crate) struct Sink<'a> {
    pub(crate) s: &'a mut State,
    start: u32,
}

impl<'a> Sink<'a> {
    pub(crate) fn new(s: &'a mut State) -> Self {
        let start = s.vertices.len() as u32;
        Self { s, start }
    }
    /// Logical units covered by one device pixel under the current camera.
    pub(crate) fn device_pixel(&self) -> f32 {
        Self::device_pixel_of(self.s)
    }
    #[inline]
    fn vertex(&mut self, p: Vec2, mut c: Color) {
        let p = p.extend(0.);
        if let Some(l) = self.s.lighting {
            c = l.shade_vertex(p, Vec3::Z, c);
        }
        if self.s.world_colors {
            c = self.s.look_override.unwrap_or(self.s.look).shade(c);
        }
        let v = self.s.matrix * p.extend(1.);
        self.s
            .vertices
            .push([v.x, v.y, v.z, v.w, c.r, c.g, c.b, c.a]);
    }
    #[inline]
    pub(crate) fn tri(&mut self, a: Vec2, ca: Color, b: Vec2, cb: Color, c: Vec2, cc: Color) {
        self.vertex(a, ca);
        self.vertex(b, cb);
        self.vertex(c, cc);
    }
    /// Axis-aligned rectangle with the triangle order of `draw_rectangle`.
    #[inline]
    pub(crate) fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, c: Color) {
        let a = vec2(x, y);
        let b = vec2(x + w, y);
        let d = vec2(x, y + h);
        let e = vec2(x + w, y + h);
        self.tri(a, c, b, c, d, c);
        self.tri(d, c, b, c, e, c);
    }
    /// Axis-aligned rectangle from its edges, with `rect`'s triangle order.
    #[inline]
    pub(crate) fn rect_edges(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, c: Color) {
        let a = vec2(x0, y0);
        let b = vec2(x1, y0);
        let d = vec2(x0, y1);
        let e = vec2(x1, y1);
        self.tri(a, c, b, c, d, c);
        self.tri(d, c, b, c, e, c);
    }
    /// Quad `p0 p1 p2 p3` in winding order.
    #[inline]
    fn quad(&mut self, p: [Vec2; 4], c: [Color; 4]) {
        self.tri(p[0], c[0], p[1], c[1], p[3], c[3]);
        self.tri(p[3], c[3], p[1], c[1], p[2], c[2]);
    }
}

impl Drop for Sink<'_> {
    fn drop(&mut self) {
        let count = self.s.vertices.len() as u32 - self.start;
        if count > 0 {
            let depth = self.s.depth;
            self.s.record_immediate(self.start, count, depth);
        }
    }
}

// ---------------------------------------------------------------------------
// Rounded-rectangle outlines: the shared core for rects, circles and capsules.

/// Segments per quarter turn for a radius in device pixels.
fn quarter_segments(radius_px: f32) -> usize {
    if radius_px.is_nan() || radius_px <= 0.5 {
        return 1;
    }
    ((radius_px.sqrt() * 1.5).ceil() as usize).clamp(2, 16)
}

/// A rounded rectangle centered at `center` with unit axis `ax` (its local +X).
#[derive(Clone, Copy)]
struct Outline {
    center: Vec2,
    ax: Vec2,
    half: Vec2,
    radius: f32,
    segments: usize,
}

impl Outline {
    fn new(rect: Rect, radius: f32, px: f32) -> Option<Self> {
        if !(rect.is_finite() && radius.is_finite() && rect.w > 0. && rect.h > 0.) {
            return None;
        }
        let half = vec2(rect.w, rect.h) * 0.5;
        let radius = radius.clamp(0., half.x.min(half.y));
        Some(Self {
            center: vec2(rect.x, rect.y) + half,
            ax: Vec2::X,
            half,
            radius,
            segments: quarter_segments(radius / px),
        })
    }
    fn points(&self) -> usize {
        4 * (self.segments + 1)
    }
    /// Point `i` of the outline grown by `offset` (negative shrinks).
    fn point(&self, i: usize, offset: f32) -> Vec2 {
        let half = (self.half + Vec2::splat(offset)).max(Vec2::ZERO);
        let radius = (self.radius + offset).clamp(0., half.x.min(half.y));
        let quarter = i / (self.segments + 1);
        let step = i % (self.segments + 1);
        let angle = (quarter as f32 + step as f32 / self.segments as f32) * FRAC_PI_2;
        let sign = match quarter {
            0 => vec2(1., 1.),
            1 => vec2(-1., 1.),
            2 => vec2(-1., -1.),
            _ => vec2(1., -1.),
        };
        let local = (half - Vec2::splat(radius)) * sign + vec2(angle.cos(), angle.sin()) * radius;
        let ay = self.ax.perp();
        self.center + self.ax * local.x + ay * local.y
    }
    /// Filled interior up to `offset`, as a fan from the center.
    fn fan(&self, sink: &mut Sink, offset: f32, fill: &Fill, alpha: f32) {
        let n = self.points();
        let center = fade(fill.at(self.center), alpha);
        let mut prev = self.point(n - 1, offset);
        let mut prev_c = fade(fill.at(prev), alpha);
        for i in 0..n {
            let p = self.point(i, offset);
            let c = fade(fill.at(p), alpha);
            sink.tri(self.center, center, prev, prev_c, p, c);
            prev = p;
            prev_c = c;
        }
    }
    /// Strip between two offset outlines with separate alpha multipliers.
    fn strip(&self, sink: &mut Sink, inner: (f32, f32), outer: (f32, f32), fill: &Fill) {
        let n = self.points();
        let at = |i: usize| {
            let a = self.point(i % n, inner.0);
            let b = self.point(i % n, outer.0);
            (a, fade(fill.at(a), inner.1), b, fade(fill.at(b), outer.1))
        };
        let mut prev = at(n - 1);
        for i in 0..n {
            let next = at(i);
            sink.quad(
                [prev.0, next.0, next.2, prev.2],
                [prev.1, next.1, next.3, prev.3],
            );
            prev = next;
        }
    }
}

fn filled(outline: Outline, fill: Fill, sink: &mut Sink, px: f32) {
    let f = px * 0.5;
    outline.fan(sink, -f, &fill, 1.);
    outline.strip(sink, (-f, 1.), (f, 0.), &fill);
}

fn stroked(outline: Outline, width: f32, color: Color, sink: &mut Sink, px: f32) {
    let f = px * 0.5;
    let half = (width * 0.5).max(f);
    let alpha = (width / px).min(1.);
    let fill = Fill::Solid(color);
    outline.strip(sink, (-half - f, 0.), (-half + f, alpha), &fill);
    outline.strip(sink, (-half + f, alpha), (half - f, alpha), &fill);
    outline.strip(sink, (half - f, alpha), (half + f, 0.), &fill);
}

fn with_sink<R>(draw: impl FnOnce(&mut Sink, f32) -> R) -> R {
    crate::edit(|s| {
        let mut sink = Sink::new(s);
        let px = sink.device_pixel();
        draw(&mut sink, px)
    })
}

/// Filled, anti-aliased rounded rectangle. `radius` is clamped to half the
/// shorter side; zero gives crisp-but-smooth square corners.
/// Emits at most 612 vertices.
///
/// ```
/// use ferravex_render::{Color, Rect, shapes};
/// shapes::fill_rounded_rect(Rect::new(10., 10., 120., 40.), 8., Color::from_hex(0x1f2a3d));
/// ```
pub fn fill_rounded_rect(rect: Rect, radius: f32, fill: impl Into<Fill>) {
    let fill = fill.into();
    if !fill.is_finite() {
        return;
    }
    with_sink(|sink, px| {
        if let Some(outline) = Outline::new(rect, radius, px) {
            filled(outline, fill, sink, px);
        }
    })
}

/// Anti-aliased rounded-rectangle border of `width`, centered on the edge of
/// `rect`. Strokes thinner than a device pixel fade instead of vanishing.
/// Emits at most 1,224 vertices.
///
/// ```
/// use ferravex_render::{Color, Rect, shapes};
/// shapes::stroke_rounded_rect(Rect::new(10., 10., 120., 40.), 8., 2., Color::from_hex(0xf7b553));
/// ```
pub fn stroke_rounded_rect(rect: Rect, radius: f32, width: f32, color: Color) {
    if !(width.is_finite() && width > 0. && Fill::Solid(color).is_finite()) {
        return;
    }
    with_sink(|sink, px| {
        if let Some(outline) = Outline::new(rect, radius, px) {
            stroked(outline, width, color, sink, px);
        }
    })
}

fn circle_rect(center: Vec2, radius: f32) -> Rect {
    Rect::new(
        center.x - radius,
        center.y - radius,
        radius * 2.,
        radius * 2.,
    )
}

/// Filled, anti-aliased circle (up to 64 segments). Emits at most 612 vertices.
///
/// ```
/// use ferravex_render::{Color, vec2, shapes::{self, Fill}};
/// let c = vec2(50., 50.);
/// shapes::fill_circle(c, 20., Fill::radial(c, 20., Color::new(1., 1., 1., 1.),
///     Color::new(0.2, 0.4, 1., 1.)));
/// ```
pub fn fill_circle(center: Vec2, radius: f32, fill: impl Into<Fill>) {
    fill_rounded_rect(circle_rect(center, radius), radius, fill)
}

/// Anti-aliased ring of `width` centered on `radius`. Emits at most 1,224 vertices.
///
/// ```
/// use ferravex_render::{Color, vec2, shapes};
/// shapes::stroke_circle(vec2(50., 50.), 20., 3., Color::new(1., 1., 1., 0.8));
/// ```
pub fn stroke_circle(center: Vec2, radius: f32, width: f32, color: Color) {
    stroke_rounded_rect(circle_rect(center, radius), radius, width, color)
}

/// Filled, anti-aliased capsule (a segment with round ends) of `radius`.
/// Coincident end points draw a circle. Emits at most 612 vertices.
///
/// ```
/// use ferravex_render::{Color, vec2, shapes};
/// shapes::fill_capsule(vec2(20., 20.), vec2(120., 20.), 6., Color::from_hex(0x5bd5d8));
/// ```
pub fn fill_capsule(a: Vec2, b: Vec2, radius: f32, fill: impl Into<Fill>) {
    let fill = fill.into();
    if !(a.is_finite() && b.is_finite() && radius.is_finite() && radius > 0. && fill.is_finite()) {
        return;
    }
    let axis = (b - a).normalize_or(Vec2::X);
    let length = (b - a).length();
    with_sink(|sink, px| {
        let outline = Outline {
            center: (a + b) * 0.5,
            ax: axis,
            half: vec2(length * 0.5 + radius, radius),
            radius,
            segments: quarter_segments(radius / px),
        };
        filled(outline, fill, sink, px);
    })
}

/// Soft shadow under a rounded rectangle. The falloff follows a smoothstep
/// across `blur`, approximating a Gaussian with four feathered rings.
/// Emits at most 1,836 vertices.
///
/// ```
/// use ferravex_render::{Color, Rect, shapes::{self, Shadow}};
/// shapes::drop_shadow(Rect::new(20., 20., 200., 90.), 10., Shadow::soft(Color::new(0., 0., 0., 0.5)));
/// ```
pub fn drop_shadow(rect: Rect, radius: f32, shadow: Shadow) {
    let Shadow {
        color,
        offset,
        blur,
        spread,
    } = shadow;
    if !(offset.is_finite() && blur.is_finite() && spread.is_finite()) {
        return;
    }
    let rect = Rect::new(
        rect.x + offset.x - spread,
        rect.y + offset.y - spread,
        rect.w + spread * 2.,
        rect.h + spread * 2.,
    );
    if !Fill::Solid(color).is_finite() {
        return;
    }
    with_sink(|sink, px| {
        let blur = blur.max(px);
        let Some(mut outline) = Outline::new(rect, radius + spread.max(0.), px) else {
            return;
        };
        outline.segments = quarter_segments((outline.radius + blur * 0.5) / px);
        let fill = Fill::Solid(color);
        let half = blur * 0.5;
        // Smoothstep alpha at five evenly spaced offsets.
        const T: [f32; 5] = [0., 0.25, 0.5, 0.75, 1.];
        let alpha = |t: f32| 1. - t * t * (3. - 2. * t);
        outline.fan(sink, -half, &fill, 1.);
        for k in 0..4 {
            let a = -half + T[k] * blur;
            let b = -half + T[k + 1] * blur;
            outline.strip(sink, (a, alpha(T[k])), (b, alpha(T[k + 1])), &fill);
        }
    })
}

// ---------------------------------------------------------------------------
// Polylines and arcs.

/// One cross-section of a stroke: its center and four points from the left
/// fringe to the right.
#[derive(Clone, Copy)]
struct Section(Vec2, [Vec2; 4]);

struct Strokes<'s, 'a> {
    sink: &'s mut Sink<'a>,
    color: Color,
    alpha: f32,
    /// Half width of the opaque core and of the fringe.
    core: f32,
    fringe: f32,
    prev: Option<Section>,
}

impl Strokes<'_, '_> {
    fn section(&mut self, p: Vec2, n: Vec2, scale: f32) {
        let outer = n * ((self.core + self.fringe) * scale);
        let inner = n * ((self.core - self.fringe).max(0.) * scale);
        let next = Section(p, [p + outer, p + inner, p - inner, p - outer]);
        if let Some(Section(center, prev)) = self.prev {
            let clear = fade(self.color, 0.);
            let solid = fade(self.color, self.alpha);
            let colors = [clear, solid, solid, clear];
            let q = next.1;
            for k in [0, 2] {
                self.sink.quad(
                    [prev[k], q[k], q[k + 1], prev[k + 1]],
                    [colors[k], colors[k], colors[k + 1], colors[k + 1]],
                );
            }
            if center == p {
                // A pivot (join or cap sweep): two wedges instead of a bowtie.
                self.sink.tri(p, solid, prev[1], solid, q[1], solid);
                self.sink.tri(p, solid, q[2], solid, prev[2], solid);
            } else {
                self.sink.quad([prev[1], q[1], q[2], prev[2]], [solid; 4]);
            }
        }
        self.prev = Some(next);
    }
    fn break_strip(&mut self) {
        self.prev = None;
    }
    /// Sections around `p` sweeping the normal from `from` by `angle` radians.
    fn sweep(&mut self, p: Vec2, from: Vec2, angle: f32, steps: usize) {
        for i in 0..=steps {
            let a = angle * i as f32 / steps as f32;
            let (sin, cos) = a.sin_cos();
            let n = vec2(from.x * cos - from.y * sin, from.x * sin + from.y * cos);
            self.section(p, n, 1.);
        }
    }
    fn cap(&mut self, p: Vec2, dir: Vec2, cap: Cap, start: bool, steps: usize) {
        let n = dir.perp();
        match cap {
            Cap::Butt => self.section(p, n, 1.),
            Cap::Square => {
                let shift = dir * self.core * if start { -1. } else { 1. };
                self.section(p + shift, n, 1.);
                if !start {
                    return;
                }
                self.section(p, n, 1.);
            }
            Cap::Round => {
                if start {
                    self.sweep(p, -n, PI, steps * 2);
                    self.break_strip();
                    self.section(p, n, 1.);
                } else {
                    self.section(p, n, 1.);
                    self.break_strip();
                    self.sweep(p, n, PI, steps * 2);
                }
            }
        }
    }
    fn join(&mut self, p: Vec2, d_in: Vec2, d_out: Vec2, join: Join, steps: usize) {
        let n_in = d_in.perp();
        let n_out = d_out.perp();
        let cross = d_in.x * d_out.y - d_in.y * d_out.x;
        if cross.abs() < 1e-4 && d_in.dot(d_out) > 0. {
            self.section(p, n_in, 1.);
            return;
        }
        let miter = (n_in + n_out).normalize_or_zero();
        let cos = miter.dot(n_in);
        if join == Join::Miter && cos > 0.25 {
            self.section(p, miter, 1. / cos);
            return;
        }
        let angle = cross.atan2(d_in.dot(d_out));
        let steps = if join == Join::Round {
            ((angle.abs() / FRAC_PI_2) * steps as f32).ceil().max(1.) as usize
        } else {
            1
        };
        self.sweep(p, n_in, angle, steps);
    }
}

fn stroke_setup(px: f32, stroke: &Stroke) -> Option<(f32, f32, f32)> {
    if !(stroke.width.is_finite() && stroke.width > 0. && Fill::Solid(stroke.color).is_finite()) {
        return None;
    }
    let fringe = px * 0.5;
    let core = (stroke.width * 0.5).max(fringe);
    let alpha = (stroke.width / px).min(1.);
    Some((core, fringe, alpha))
}

/// Anti-aliased polyline through caller-owned `points` with joins and caps.
///
/// Consecutive duplicate points are skipped. Bevel and round joins overlap the
/// body on the inside of a turn, so translucent strokes may darken there.
/// Emits at most `18 × (sections)` vertices, where each point contributes one
/// section for miters, two for bevels and up to 17 for round joins/caps.
///
/// ```
/// use ferravex_render::{Color, vec2, shapes::{self, Cap, Join, Stroke}};
/// let path = [vec2(10., 80.), vec2(60., 20.), vec2(110., 70.), vec2(160., 30.)];
/// shapes::polyline(&path, Stroke::new(4., Color::from_hex(0x5bd5d8))
///     .join(Join::Round).cap(Cap::Round));
/// ```
pub fn polyline(points: &[Vec2], stroke: Stroke) {
    if points.len() < 2 || !points.iter().all(|p| p.is_finite()) {
        return;
    }
    with_sink(|sink, px| {
        let Some((core, fringe, alpha)) = stroke_setup(px, &stroke) else {
            return;
        };
        let steps = quarter_segments(core / px).min(8);
        let eps = px * 1e-3;
        // Distinct points via index filtering: no buffer is needed.
        let n = points.len();
        let next_distinct =
            |from: usize| (from + 1..n).find(|&j| (points[j] - points[from]).length() > eps);
        let mut strokes = Strokes {
            sink,
            color: stroke.color,
            alpha,
            core,
            fringe,
            prev: None,
        };
        let closed = stroke.closed && n > 2;
        // Collect the distinct path lazily: walk indices i0 -> i1 -> ...
        let Some(second) = next_distinct(0) else {
            return;
        };
        let last = {
            let mut i = 0;
            while let Some(j) = next_distinct(i) {
                i = j;
            }
            i
        };
        let closed = closed && (points[last] - points[0]).length() > eps;
        let first_dir = (points[second] - points[0]).normalize_or_zero();
        if closed {
            let closing = (points[0] - points[last]).normalize_or_zero();
            strokes.join(points[0], closing, first_dir, stroke.join, steps);
        } else {
            strokes.cap(points[0], first_dir, stroke.cap, true, steps);
        }
        let mut current = 0;
        let mut dir = first_dir;
        while let Some(next) = next_distinct(current) {
            let d_in = (points[next] - points[current]).normalize_or_zero();
            if current != 0 {
                strokes.join(points[current], dir, d_in, stroke.join, steps);
            }
            dir = d_in;
            current = next;
        }
        if closed {
            let closing = (points[0] - points[last]).normalize_or_zero();
            strokes.join(points[last], dir, closing, stroke.join, steps);
            strokes.join(points[0], closing, first_dir, stroke.join, steps);
        } else {
            strokes.cap(points[last], dir, stroke.cap, false, steps);
        }
    })
}

/// Anti-aliased arc of `radius` from `start` sweeping `sweep` radians (positive
/// is clockwise on screen with Y down). Useful for radial meters and cooldowns.
/// A sweep of a full turn or more draws a closed ring. Emits at most 1,782 vertices.
///
/// ```
/// use std::f32::consts::{FRAC_PI_2, TAU};
/// use ferravex_render::{Color, vec2, shapes::{self, Cap, Stroke}};
/// // A 70% cooldown ring starting at 12 o'clock.
/// shapes::stroke_arc(vec2(40., 40.), 18., -FRAC_PI_2, 0.7 * TAU,
///     Stroke::new(5., Color::from_hex(0xf7b553)).cap(Cap::Round));
/// ```
pub fn stroke_arc(center: Vec2, radius: f32, start: f32, sweep: f32, stroke: Stroke) {
    if !(center.is_finite()
        && radius.is_finite()
        && radius > 0.
        && start.is_finite()
        && sweep.is_finite())
        || sweep == 0.
    {
        return;
    }
    if sweep.abs() >= TAU {
        return stroke_circle(center, radius, stroke.width, stroke.color);
    }
    with_sink(|sink, px| {
        let Some((core, fringe, alpha)) = stroke_setup(px, &stroke) else {
            return;
        };
        let cap_steps = quarter_segments(core / px).min(8);
        let steps = ((quarter_segments(radius / px) * 4) as f32 * sweep.abs() / TAU)
            .ceil()
            .clamp(1., 64.) as usize;
        let mut strokes = Strokes {
            sink,
            color: stroke.color,
            alpha,
            core,
            fringe,
            prev: None,
        };
        let at = |a: f32| vec2(a.cos(), a.sin());
        let tangent = |a: f32| at(a).perp() * sweep.signum();
        strokes.cap(
            center + at(start) * radius,
            tangent(start),
            stroke.cap,
            true,
            cap_steps,
        );
        for i in 1..steps {
            let a = start + sweep * i as f32 / steps as f32;
            strokes.section(center + at(a) * radius, at(a) * -sweep.signum(), 1.);
        }
        let end = start + sweep;
        strokes.cap(
            center + at(end) * radius,
            tangent(end),
            stroke.cap,
            false,
            cap_steps,
        );
    })
}

// ---------------------------------------------------------------------------
// Additive glows.

fn glow_segments(radius: f32, px: f32) -> usize {
    (quarter_segments(radius / px) * 4).clamp(12, 48)
}

/// Draws additive triangles from `count` lazily generated triangles.
fn additive(count: usize, triangle: impl Fn(usize) -> [(Vec2, Color); 3]) {
    let style = effects::EffectStyle {
        blend: effects::EffectBlend::Additive,
        depth_test: None,
        depth_write: false,
    };
    effects::draw_triangles(
        (0..count).map(|i| {
            let t = triangle(i);
            effects::RibbonTriangle {
                vertices: t.map(|(p, c)| effects::RibbonVertex {
                    position: [p.x, p.y, 0.],
                    color: [c.r, c.g, c.b, c.a],
                    uv: [0., 0.],
                }),
            }
        }),
        style,
    );
}

/// Additive soft glow disc for particles, pickups and highlights. Brightness
/// falls off as `(1 - r/radius)²`; `color.a` scales intensity. Additive light
/// brightens what is underneath and never darkens. Emits at most 1,296 vertices
/// through [`crate::effects::draw_triangles`] (its per-frame budget applies).
///
/// ```
/// use ferravex_render::{Color, vec2, shapes};
/// shapes::glow_disc(vec2(64., 64.), 40., Color::new(1., 0.6, 0.2, 0.8));
/// ```
pub fn glow_disc(center: Vec2, radius: f32, color: Color) {
    if !(center.is_finite() && radius.is_finite() && radius > 0. && Fill::Solid(color).is_finite())
    {
        return;
    }
    const RINGS: [f32; 6] = [0., 0.1, 0.25, 0.45, 0.7, 1.];
    let px = crate::read(Sink::device_pixel_of);
    let n = glow_segments(radius, px);
    let point = |ring: usize, i: usize| {
        let a = (i % n) as f32 * TAU / n as f32;
        let t = RINGS[ring];
        (
            center + vec2(a.cos(), a.sin()) * radius * t,
            fade(color, (1. - t) * (1. - t)),
        )
    };
    // One fan to the first ring, then two triangles per segment per ring gap.
    additive(n + (RINGS.len() - 2) * n * 2, |k| {
        if k < n {
            return [point(0, 0), point(1, k), point(1, k + 1)];
        }
        let k = k - n;
        let ring = 1 + k / (2 * n);
        let i = (k % (2 * n)) / 2;
        if k.is_multiple_of(2) {
            [point(ring, i), point(ring + 1, i), point(ring + 1, i + 1)]
        } else {
            [point(ring, i), point(ring + 1, i + 1), point(ring, i + 1)]
        }
    });
}

/// Additive soft glow ring (shockwaves, selection halos, portals) of total
/// `width` centered on `radius`, brightest on the center line.
/// Emits at most 1,728 vertices through [`crate::effects::draw_triangles`].
///
/// ```
/// use ferravex_render::{Color, vec2, shapes};
/// shapes::glow_ring(vec2(64., 64.), 30., 12., Color::new(0.4, 0.8, 1., 0.9));
/// ```
pub fn glow_ring(center: Vec2, radius: f32, width: f32, color: Color) {
    if !(center.is_finite()
        && radius.is_finite()
        && radius > 0.
        && width.is_finite()
        && width > 0.
        && Fill::Solid(color).is_finite())
    {
        return;
    }
    const BANDS: [f32; 7] = [-1., -0.55, -0.2, 0., 0.2, 0.55, 1.];
    let px = crate::read(Sink::device_pixel_of);
    let n = glow_segments(radius + width * 0.5, px);
    let point = |band: usize, i: usize| {
        let a = (i % n) as f32 * TAU / n as f32;
        let u = BANDS[band];
        let r = (radius + u * width * 0.5).max(0.);
        let falloff = 1. - u.abs();
        (
            center + vec2(a.cos(), a.sin()) * r,
            fade(color, falloff * falloff),
        )
    };
    additive((BANDS.len() - 1) * n * 2, |k| {
        let band = k / (2 * n);
        let i = (k % (2 * n)) / 2;
        if k.is_multiple_of(2) {
            [point(band, i), point(band + 1, i), point(band + 1, i + 1)]
        } else {
            [point(band, i), point(band + 1, i + 1), point(band, i + 1)]
        }
    });
}

impl Sink<'_> {
    pub(crate) fn device_pixel_of(s: &State) -> f32 {
        let m = s.matrix;
        let x = vec2(m.x_axis.x * s.w as f32, m.x_axis.y * s.h as f32) * 0.5;
        let ppu = x.length();
        if ppu.is_finite() && ppu > 1e-6 {
            1. / ppu
        } else {
            1.
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::input::host_frame;

    /// Runs `draw` in a frame and returns its vertices (clip space, RGBA).
    pub(crate) fn capture(w: u32, h: u32, draw: impl FnOnce()) -> Vec<[f32; 8]> {
        host_frame(1. / 60., w, h, draw);
        crate::read(|s| s.vertices.clone())
    }

    fn screen(v: &[f32; 8], w: f32, h: f32) -> Vec2 {
        vec2((v[0] + 1.) * w * 0.5, (1. - v[1]) * h * 0.5)
    }

    fn bounds(vertices: &[[f32; 8]], w: f32, h: f32, opaque: bool) -> [f32; 4] {
        let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
        for v in vertices.iter().filter(|v| !opaque || v[7] > 0.999) {
            let p = screen(v, w, h);
            b = [b[0].min(p.x), b[1].min(p.y), b[2].max(p.x), b[3].max(p.y)];
        }
        b
    }

    #[test]
    fn rounded_rect_core_and_fringe_straddle_the_edge_by_half_a_pixel() {
        let v = capture(400, 300, || {
            fill_rounded_rect(
                Rect::new(10., 20., 100., 50.),
                8.,
                Color::new(1., 0., 0., 1.),
            )
        });
        assert!(v.len() <= 612 && v.len() % 3 == 0, "{}", v.len());
        let outer = bounds(&v, 400., 300., false);
        let core = bounds(&v, 400., 300., true);
        for (o, e) in outer.iter().zip([9.5, 19.5, 110.5, 70.5]) {
            assert!((o - e).abs() < 1e-3, "{outer:?}");
        }
        for (c, e) in core.iter().zip([10.5, 20.5, 109.5, 69.5]) {
            assert!((c - e).abs() < 1e-3, "{core:?}");
        }
        // Fringe vertices are transparent; core vertices are opaque.
        assert!(v.iter().all(|v| v[7] == 0. || v[7] == 1.));
    }

    #[test]
    fn shapes_respect_documented_vertex_bounds_and_reject_bad_input() {
        type Case = (&'static str, usize, Box<dyn Fn()>);
        let cases: [Case; 8] = [
            (
                "rect",
                612,
                Box::new(|| {
                    fill_rounded_rect(
                        Rect::new(0., 0., 900., 900.),
                        450.,
                        Color::new(1., 1., 1., 1.),
                    )
                }),
            ),
            (
                "stroke",
                1224,
                Box::new(|| {
                    stroke_rounded_rect(
                        Rect::new(0., 0., 900., 900.),
                        450.,
                        3.,
                        Color::new(1., 1., 1., 1.),
                    )
                }),
            ),
            (
                "shadow",
                1836,
                Box::new(|| {
                    drop_shadow(
                        Rect::new(0., 0., 900., 900.),
                        400.,
                        Shadow::soft(Color::new(0., 0., 0., 1.)).blur(200.),
                    )
                }),
            ),
            (
                "capsule",
                612,
                Box::new(|| {
                    fill_capsule(
                        vec2(0., 0.),
                        vec2(900., 300.),
                        400.,
                        Color::new(1., 1., 1., 1.),
                    )
                }),
            ),
            (
                "arc",
                1782,
                Box::new(|| {
                    stroke_arc(
                        vec2(400., 400.),
                        390.,
                        0.,
                        6.2,
                        Stroke::new(80., Color::new(1., 1., 1., 1.)).cap(Cap::Round),
                    )
                }),
            ),
            (
                "glow",
                1296,
                Box::new(|| glow_disc(vec2(0., 0.), 2000., Color::new(1., 1., 1., 1.))),
            ),
            (
                "ring",
                1728,
                Box::new(|| glow_ring(vec2(0., 0.), 2000., 50., Color::new(1., 1., 1., 1.))),
            ),
            (
                "nan",
                0,
                Box::new(|| {
                    fill_rounded_rect(
                        Rect::new(f32::NAN, 0., 1., 1.),
                        0.,
                        Color::new(1., 1., 1., 1.),
                    );
                    fill_circle(vec2(0., 0.), -1., Color::new(1., 1., 1., 1.));
                    polyline(&[vec2(0., 0.)], Stroke::new(2., Color::new(1., 1., 1., 1.)));
                    polyline(
                        &[vec2(1., 1.), vec2(1., 1.)],
                        Stroke::new(2., Color::new(1., 1., 1., 1.)),
                    );
                    stroke_circle(vec2(0., 0.), 5., f32::INFINITY, Color::new(1., 1., 1., 1.));
                    glow_disc(vec2(0., 0.), 5., Color::new(f32::NAN, 1., 1., 1.));
                }),
            ),
        ];
        for (name, bound, draw) in cases {
            let v = capture(1280, 800, draw);
            assert!(v.len() <= bound, "{name}: {} > {bound}", v.len());
            assert!(v.iter().flatten().all(|x| x.is_finite()), "{name}");
            if bound > 0 {
                assert!(!v.is_empty(), "{name} drew nothing");
            }
        }
    }

    #[test]
    fn polyline_joins_cover_corners_and_are_finite() {
        let path = [
            vec2(10., 10.),
            vec2(100., 10.),
            vec2(100., 100.),
            vec2(10., 100.),
        ];
        for join in [Join::Miter, Join::Bevel, Join::Round] {
            for cap in [Cap::Butt, Cap::Square, Cap::Round] {
                let v = capture(200, 200, || {
                    polyline(
                        &path,
                        Stroke::new(6., Color::new(1., 1., 1., 1.))
                            .join(join)
                            .cap(cap),
                    )
                });
                assert!(!v.is_empty() && v.len() % 3 == 0);
                assert!(v.iter().flatten().all(|x| x.is_finite()));
                let outer = bounds(&v, 200., 200., false);
                // Outer corner reaches the miter point for miters, never beyond it.
                assert!(outer[2] <= 103.5 + 1e-3, "{join:?} {cap:?} {outer:?}");
                if join == Join::Miter {
                    assert!((outer[2] - 103.5).abs() < 1e-3);
                }
            }
        }
        let closed = capture(200, 200, || {
            polyline(
                &path,
                Stroke::new(2., Color::new(1., 1., 1., 1.)).closed(true),
            )
        });
        let b = bounds(&closed, 200., 200., false);
        assert!(
            (b[0] - 8.5).abs() < 1e-3 && (b[1] - 8.5).abs() < 1e-3,
            "{b:?}"
        );
    }

    #[test]
    fn gradients_interpolate_per_vertex_and_glow_is_additive() {
        let rect = Rect::new(0., 0., 100., 100.);
        let v = capture(100, 100, || {
            fill_rounded_rect(
                rect,
                0.,
                Fill::vertical(rect, Color::new(0., 0., 0., 1.), Color::new(1., 1., 1., 1.)),
            )
        });
        for v in &v {
            let p = screen(v, 100., 100.);
            assert!((v[4] - (p.y / 100.).clamp(0., 1.)).abs() < 1e-4);
        }
        host_frame(1. / 60., 100, 100, || {
            glow_disc(vec2(50., 50.), 30., Color::new(1., 1., 1., 1.))
        });
        let (effects, commands) =
            crate::read(|s| (s.retained.effects.clone(), s.retained.commands.len()));
        assert_eq!(effects.len(), 1);
        assert_eq!(effects[0].flags & 4, 4, "additive blend flag");
        assert!(commands >= 1);
    }
}
