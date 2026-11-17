//! Procedural vector icons: a shared visual vocabulary that replaces words.
//!
//! Every icon is drawn from anti-aliased [`crate::shapes`] primitives inside a
//! square box, so it stays crisp at any size from small HUD glyphs (12 px) to
//! large menu art (128 px) and batches with ordinary immediate drawing. Icons
//! are two-tone: the main `color` paints the silhouette and a darker `detail`
//! tone paints inner marks (a coin's rim, a clock's hands), so they read on
//! both light and dark surfaces without needing a cut-out background.
//!
//! Coordinates are the current camera's logical units. Nothing allocates.
//! Non-finite input or a non-positive size draws nothing.
//!
//! ```
//! use ferravex_render::{Color, vec2, icons::{self, Icon, IconStyle}};
//! icons::draw(Icon::Heart, vec2(24., 24.), 24., Color::from_hex(0xe5484d));
//! icons::draw_styled(Icon::Coin, vec2(60., 24.), 24.,
//!     IconStyle::new(Color::from_hex(0xf7b553)).shadow(Color::new(0., 0., 0., 0.4)));
//! icons::badge(Icon::Pause, vec2(100., 24.), 32., Color::from_hex(0x1f2a3d),
//!     Color::from_hex(0xeef2f5));
//! assert_eq!(Icon::from_name("heart"), Some(Icon::Heart));
//! assert!(Icon::ALL.iter().all(|i| Icon::from_name(i.name()) == Some(*i)));
//! ```
use crate::shapes::{self, Cap, Join, Stroke};
use crate::{Color, Rect, Vec2, vec2};
use std::f32::consts::{FRAC_PI_2, PI, TAU};

macro_rules! icons {
    ($($(#[$doc:meta])* $variant:ident => $name:literal,)*) => {
        /// A named pictogram. See [`Icon::ALL`] for the complete set.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum Icon {
            $($(#[$doc])* $variant,)*
        }
        impl Icon {
            /// Every icon, in declaration order (useful for specimen sheets).
            pub const ALL: &'static [Icon] = &[$(Icon::$variant,)*];
            /// Stable lowercase identifier, such as `"heart"`.
            pub const fn name(self) -> &'static str {
                match self {
                    $(Icon::$variant => $name,)*
                }
            }
            /// The icon whose [`Icon::name`] equals `name`.
            pub fn from_name(name: &str) -> Option<Icon> {
                match name {
                    $($name => Some(Icon::$variant),)*
                    _ => None,
                }
            }
        }
    };
}

icons! {
    // Interface and menus.
    /// Right-pointing triangle: start, resume, run.
    Play => "play",
    /// Two bars: pause.
    Pause => "pause",
    /// Circular arrow: restart, retry.
    Restart => "restart",
    /// Counter-clockwise hook arrow: undo, take back.
    Undo => "undo",
    /// Tick mark: done, correct, accepted.
    Check => "check",
    /// Diagonal cross: wrong, close, rejected.
    Cross => "cross",
    Plus => "plus",
    Minus => "minus",
    ArrowUp => "arrow_up",
    ArrowDown => "arrow_down",
    ArrowLeft => "arrow_left",
    ArrowRight => "arrow_right",
    /// Question mark: help, unknown.
    Question => "question",
    /// Lowercase i: information, details.
    Info => "info",
    /// Exclamation in a triangle: warning, danger.
    Alert => "alert",
    /// Speaker with waves.
    SoundOn => "sound_on",
    /// Speaker with a cross.
    SoundOff => "sound_off",
    /// Cog: settings, machinery.
    Gear => "gear",
    /// Game controller: controls, input.
    Gamepad => "gamepad",
    /// Mouse arrow cursor.
    Pointer => "pointer",
    /// Raised hand / tap gesture.
    Hand => "hand",
    Eye => "eye",
    /// Eye with a slash: hidden, unseen.
    EyeOff => "eye_off",
    Lock => "lock",
    Unlock => "unlock",
    Key => "key",
    Home => "home",
    Trophy => "trophy",
    Crown => "crown",
    Flag => "flag",
    /// Concentric rings: aim, goal.
    Target => "target",
    /// Map marker teardrop.
    Pin => "pin",
    /// Folded map.
    Map => "map",
    /// Magnifying glass: search, inspect.
    Search => "search",
    Bell => "bell",
    // Stats and resources.
    Star => "star",
    Heart => "heart",
    Skull => "skull",
    Clock => "clock",
    Hourglass => "hourglass",
    Calendar => "calendar",
    /// Lightning bolt: energy, power, speed.
    Bolt => "bolt",
    Battery => "battery",
    Shield => "shield",
    Sword => "sword",
    /// Round bomb with a fuse; also a cannonball.
    Bomb => "bomb",
    Flame => "flame",
    /// Water droplet.
    Drop => "drop",
    Snowflake => "snowflake",
    Sun => "sun",
    Moon => "moon",
    Cloud => "cloud",
    Rain => "rain",
    Wind => "wind",
    /// Two water waves.
    Wave => "wave",
    Leaf => "leaf",
    /// Seedling with two leaves.
    Sprout => "sprout",
    Tree => "tree",
    Mountain => "mountain",
    Coin => "coin",
    /// Faceted gemstone: treasure, crystal, amber.
    Gem => "gem",
    /// Tied sack: inventory, goods.
    Bag => "bag",
    /// Wooden crate: cargo, supplies, parcels.
    Crate => "crate",
    Scroll => "scroll",
    Book => "book",
    /// Light bulb: idea, light, research.
    Bulb => "bulb",
    Hammer => "hammer",
    Wrench => "wrench",
    Pickaxe => "pickaxe",
    Anchor => "anchor",
    /// Sailing ship.
    Ship => "ship",
    Rocket => "rocket",
    /// Ringed planet.
    Planet => "planet",
    Train => "train",
    Person => "person",
    People => "people",
    Paw => "paw",
    Bone => "bone",
    /// Round flask: potion, remedy.
    Potion => "potion",
    Fish => "fish",
    /// Eighth note.
    Music => "music",
    /// Speech bubble.
    Chat => "chat",
    /// Cooking pot.
    Pot => "pot",
    Thermometer => "thermometer",
    /// Fuel can.
    Fuel => "fuel",
    /// Two chevrons: fast forward, speed up, skip ahead.
    FastForward => "fast_forward",
    /// Horseshoe magnet: attract, collect, crane.
    Magnet => "magnet",
    /// Beetle: pests, enemies, bugs.
    Bug => "bug",
    /// Two interlocking links: chains, combos, links.
    Chain => "chain",
    /// Four-way arrows: move, steer, drag.
    Move => "move",
    /// Two fanned playing cards: deck, hand, draw pile.
    Cards => "cards",
    /// A ball with a highlight: golf, sports, projectiles.
    Ball => "ball",
}

/// Colors for one icon draw.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IconStyle {
    /// Silhouette color.
    pub color: Color,
    /// Inner marks; [`IconStyle::new`] derives a darker shade of `color`.
    pub detail: Color,
    /// Optional soft drop shadow drawn under the silhouette.
    pub shadow: Option<Color>,
}

impl IconStyle {
    /// Two-tone style whose detail tone is `color` mixed 45% toward black.
    pub fn new(color: Color) -> Self {
        let dark = shapes::mix(color, Color::new(0., 0., 0., color.a), 0.45);
        Self {
            color,
            detail: Color::new(dark.r, dark.g, dark.b, color.a),
            shadow: None,
        }
    }
    /// Replace the inner-mark color.
    pub fn detail(mut self, detail: Color) -> Self {
        self.detail = detail;
        self
    }
    /// Add a soft drop shadow in `color`.
    pub fn shadow(mut self, color: Color) -> Self {
        self.shadow = Some(color);
        self
    }
}

/// Draw `icon` centered at `center`, fitted to a `size` × `size` box.
pub fn draw(icon: Icon, center: Vec2, size: f32, color: Color) {
    draw_styled(icon, center, size, IconStyle::new(color));
}

/// Draw `icon` with explicit detail and shadow colors.
pub fn draw_styled(icon: Icon, center: Vec2, size: f32, style: IconStyle) {
    if !(center.is_finite() && size.is_finite() && size > 0.) {
        return;
    }
    if let Some(shadow) = style.shadow {
        let offset = vec2(0., (size * 0.06).max(1.));
        let pen = Pen::new(center + offset, size);
        paint(icon, &pen, shadow, shadow);
    }
    let pen = Pen::new(center, size);
    paint(icon, &pen, style.color, style.detail);
}

/// A filled circle of `diameter` with `icon` centered on it at 58% size.
pub fn badge(icon: Icon, center: Vec2, diameter: f32, background: Color, foreground: Color) {
    if !(center.is_finite() && diameter.is_finite() && diameter > 0.) {
        return;
    }
    shapes::fill_circle(center, diameter * 0.5, background);
    let detail = shapes::mix(foreground, background, 0.55);
    draw_styled(
        icon,
        center,
        diameter * 0.58,
        IconStyle::new(foreground).detail(Color::new(detail.r, detail.g, detail.b, foreground.a)),
    );
}

// ---------------------------------------------------------------------------
// Drawing kit. Icons are authored in a unit box `[-1, 1]²` with Y down and
// angles in radians, clockwise on screen from +X (as `shapes::stroke_arc`).

/// Degrees to radians, for readable authoring.
const fn deg(degrees: f32) -> f32 {
    degrees * (PI / 180.)
}

/// Most points in one outline, fillet arcs included.
const MAX_POINTS: usize = 160;

/// Maps the unit box `[-1, 1]²` (Y down) onto the destination square, with an
/// optional rotation for sub-drawings such as tools lying on the diagonal.
#[derive(Clone, Copy)]
struct Pen {
    center: Vec2,
    half: f32,
    /// `(cos, sin)` of `angle`, the clockwise rotation of unit-box points.
    turn: Vec2,
    angle: f32,
}

impl Pen {
    fn new(center: Vec2, size: f32) -> Self {
        Self {
            center,
            half: size * 0.5,
            turn: Vec2::X,
            angle: 0.,
        }
    }
    /// A pen whose unit box is centered at `(x, y)`, scaled by `scale` and
    /// turned `angle` radians clockwise.
    fn sub(&self, x: f32, y: f32, scale: f32, angle: f32) -> Self {
        let angle = self.angle + angle;
        Self {
            center: self.p(x, y),
            half: self.half * scale,
            turn: if angle == 0. {
                Vec2::X
            } else {
                Vec2::from_angle(angle)
            },
            angle,
        }
    }
    /// Unit-box point to logical units.
    fn p(&self, x: f32, y: f32) -> Vec2 {
        self.center + vec2(x, y).rotate(self.turn) * self.half
    }
    /// Unit length to logical units.
    fn l(&self, length: f32) -> f32 {
        length * self.half
    }
    fn circle(&self, x: f32, y: f32, r: f32, c: Color) {
        shapes::fill_circle(self.p(x, y), self.l(r), c);
    }
    fn ring(&self, x: f32, y: f32, r: f32, width: f32, c: Color) {
        shapes::stroke_circle(self.p(x, y), self.l(r), self.l(width), c);
    }
    /// Rounded rectangle from its top-left corner; turned pens fill it as a
    /// rounded outline instead.
    fn rect(&self, x: f32, y: f32, w: f32, h: f32, radius: f32, c: Color) {
        if self.angle == 0. {
            let a = self.p(x, y);
            shapes::fill_rounded_rect(Rect::new(a.x, a.y, self.l(w), self.l(h)), self.l(radius), c);
        } else {
            let r = radius.min(w * 0.5).min(h * 0.5);
            let corners = [(x, y, r), (x + w, y, r), (x + w, y + h, r), (x, y + h, r)];
            self.fill(&Outline::corners(&corners), c);
        }
    }
    /// Rounded-rectangle border of `width` centered on the edge (unturned pens).
    fn frame(&self, (x, y, w, h): (f32, f32, f32, f32), radius: f32, width: f32, c: Color) {
        let a = self.p(x, y);
        shapes::stroke_rounded_rect(
            Rect::new(a.x, a.y, self.l(w), self.l(h)),
            self.l(radius),
            self.l(width),
            c,
        );
    }
    fn capsule(&self, a: (f32, f32), b: (f32, f32), r: f32, c: Color) {
        shapes::fill_capsule(self.p(a.0, a.1), self.p(b.0, b.1), self.l(r), c);
    }
    /// Convex polygon through unit-box points.
    fn poly(&self, points: &[(f32, f32)], c: Color) {
        self.fill(&Outline::of(points), c);
    }
    /// Convex outline.
    fn fill(&self, outline: &Outline, c: Color) {
        let (points, n) = self.place(outline);
        #[cfg(test)]
        tests::assert_convex(&points[..n]);
        if n <= shapes::MAX_POLYGON_POINTS {
            shapes::fill_polygon(&points[..n], c);
        } else if n > 0 {
            let hub = points[..n].iter().copied().sum::<Vec2>() / n as f32;
            fill_mesh(&points[..n], Some(hub), c);
        }
    }
    /// Concave outline that is star-shaped about `hub` (every edge faces it),
    /// filled as one piece.
    fn fan(&self, hub: (f32, f32), outline: &Outline, c: Color) {
        let (points, n) = self.place(outline);
        let hub = self.p(hub.0, hub.1);
        #[cfg(test)]
        tests::assert_star_shaped(&points[..n], hub);
        fill_mesh(&points[..n], Some(hub), c);
    }
    /// Round-capped stroke along a smooth open outline (sampled arcs and
    /// curves). Mitred joins are exact for gentle turns and, unlike round
    /// joins, never overlap the body, so translucent curves stay even.
    fn stroke(&self, outline: &Outline, width: f32, c: Color) {
        let (points, n) = self.place(outline);
        let stroke = Stroke::new(self.l(width), c)
            .cap(Cap::Round)
            .join(Join::Miter);
        shapes::polyline(&points[..n], stroke);
    }
    /// Mitre-joined stroke around a smooth closed outline.
    fn stroke_closed(&self, outline: &Outline, width: f32, c: Color) {
        let (points, n) = self.place(outline);
        let stroke = Stroke::new(self.l(width), c).join(Join::Miter).closed(true);
        shapes::polyline(&points[..n], stroke);
    }
    /// Round-capped, round-joined stroke through a few unit-box points, for
    /// marks with real corners (clock hands, facets).
    fn line(&self, points: &[(f32, f32)], width: f32, c: Color) {
        let (points, n) = self.place(&Outline::of(points));
        let stroke = Stroke::new(self.l(width), c)
            .cap(Cap::Round)
            .join(Join::Round);
        shapes::polyline(&points[..n], stroke);
    }
    /// Round-capped arc of `r` around `(x, y)` from `start` sweeping `sweep`.
    fn arc(&self, (x, y, r): (f32, f32, f32), start: f32, sweep: f32, width: f32, c: Color) {
        let stroke = Stroke::new(self.l(width), c).cap(Cap::Round);
        shapes::stroke_arc(self.p(x, y), self.l(r), start + self.angle, sweep, stroke);
    }
    fn sector(&self, (x, y, r): (f32, f32, f32), start: f32, sweep: f32, c: Color) {
        shapes::fill_sector(self.p(x, y), self.l(r), start + self.angle, sweep, c);
    }
    /// The part of circle `outer` outside circle `inner` (each `(x, y, r)`),
    /// filled as one ladder mesh between the two arcs.
    fn crescent(&self, outer: (f32, f32, f32), inner: (f32, f32, f32), c: Color) {
        let (co, ro) = (vec2(outer.0, outer.1), outer.2);
        let (ci, ri) = (vec2(inner.0, inner.1), inner.2);
        let span = ci - co;
        let dist = span.length();
        if !(dist > 1e-4 && dist < ro + ri && dist > (ro - ri).abs()) {
            return;
        }
        let axis = span / dist;
        let along = (ro * ro - ri * ri + dist * dist) / (2. * dist);
        let lift = axis.perp() * (ro * ro - along * along).max(0.).sqrt();
        let (tip_a, tip_b) = (co + axis * along + lift, co + axis * along - lift);
        // The arc of `center` from `tip_a` to `tip_b` passing through `via`.
        let span_through = |center: Vec2, via: Vec2| {
            let from = (tip_a - center).to_angle();
            let ahead = ((tip_b - center).to_angle() - from).rem_euclid(TAU);
            let via = ((via - center).to_angle() - from).rem_euclid(TAU);
            (from, if via < ahead { ahead } else { ahead - TAU })
        };
        let (o_from, o_sweep) = span_through(co, co - axis * ro);
        let (i_from, i_sweep) = span_through(ci, ci - axis * ri);
        // Out along the outer arc from tip to tip, back along the inner one;
        // point `k` and point `2 * steps - k` share a parameter (a rung).
        let steps = arc_steps(ro, o_sweep).clamp(8, MAX_POINTS / 2);
        let mut points = [Vec2::ZERO; MAX_POINTS];
        for k in 0..=steps {
            let t = k as f32 / steps as f32;
            let o = co + Vec2::from_angle(o_from + o_sweep * t) * ro;
            points[k] = self.p(o.x, o.y);
            if k > 0 && k < steps {
                let i = ci + Vec2::from_angle(i_from + i_sweep * t) * ri;
                points[2 * steps - k] = self.p(i.x, i.y);
            }
        }
        #[cfg(test)]
        tests::assert_ladder(&points[..2 * steps]);
        fill_mesh(&points[..2 * steps], None, c);
    }
    /// Outline points (fillets expanded) in logical units.
    fn place(&self, outline: &Outline) -> ([Vec2; MAX_POINTS], usize) {
        let mut out = [Vec2::ZERO; MAX_POINTS];
        let mut n = 0;
        outline.trace(|x, y| {
            #[cfg(test)]
            assert!(n < MAX_POINTS, "outline overflows {MAX_POINTS} points");
            if n < MAX_POINTS {
                out[n] = self.p(x, y);
                n += 1;
            }
        });
        (out, n)
    }
}

/// A stack-built outline in unit-box coordinates. Each corner may carry a
/// radius, which [`Outline::trace`] replaces with a tangent fillet arc.
#[derive(Clone, Copy)]
struct Outline {
    corners: [(f32, f32, f32); MAX_POINTS],
    len: usize,
}

impl Outline {
    fn new() -> Self {
        Self {
            corners: [(0., 0., 0.); MAX_POINTS],
            len: 0,
        }
    }
    fn of(points: &[(f32, f32)]) -> Self {
        let mut o = Self::new();
        for &(x, y) in points {
            o.at(x, y);
        }
        o
    }
    /// Corners `(x, y, radius)`; a zero radius keeps the corner sharp.
    fn corners(corners: &[(f32, f32, f32)]) -> Self {
        let mut o = Self::new();
        for &(x, y, r) in corners {
            o.round(x, y, r);
        }
        o
    }
    fn at(&mut self, x: f32, y: f32) -> &mut Self {
        self.round(x, y, 0.)
    }
    /// A corner rounded with radius `r`. Repeated points are skipped.
    fn round(&mut self, x: f32, y: f32, r: f32) -> &mut Self {
        let repeat = self.len > 0 && {
            let (px, py, _) = self.corners[self.len - 1];
            (px - x).abs() < 1e-5 && (py - y).abs() < 1e-5
        };
        #[cfg(test)]
        assert!(
            repeat || self.len < MAX_POINTS,
            "outline overflows {MAX_POINTS} corners"
        );
        if !repeat && self.len < MAX_POINTS {
            self.corners[self.len] = (x, y, r);
            self.len += 1;
        }
        self
    }
    /// Points along an arc of `r` around `(cx, cy)` from `start` sweeping
    /// `sweep` radians clockwise, both ends included.
    fn arc(&mut self, (cx, cy, r): (f32, f32, f32), start: f32, sweep: f32) -> &mut Self {
        let steps = arc_steps(r, sweep);
        for i in 0..=steps {
            let a = start + sweep * i as f32 / steps as f32;
            self.at(cx + r * a.cos(), cy + r * a.sin());
        }
        self
    }
    /// Quadratic Bézier from the last point through control `q` to `(x, y)`.
    fn curve(&mut self, q: (f32, f32), to: (f32, f32), steps: usize) -> &mut Self {
        let Some(&(sx, sy, _)) = self.corners[..self.len].last() else {
            return self.at(to.0, to.1);
        };
        // A quadratic is the cubic with controls two thirds of the way to `q`.
        let lerp = |a: f32, b: f32| a + (b - a) * (2. / 3.);
        let q1 = (lerp(sx, q.0), lerp(sy, q.1));
        let q2 = (lerp(to.0, q.0), lerp(to.1, q.1));
        self.cubic(q1, q2, to, steps)
    }
    /// Cubic Bézier from the last point through controls `q1`, `q2` to `to`.
    fn cubic(&mut self, q1: (f32, f32), q2: (f32, f32), to: (f32, f32), steps: usize) -> &mut Self {
        let Some(&(sx, sy, _)) = self.corners[..self.len].last() else {
            return self.at(to.0, to.1);
        };
        for i in 1..=steps {
            let t = i as f32 / steps as f32;
            let u = 1. - t;
            let w = [u * u * u, 3. * u * u * t, 3. * u * t * t, t * t * t];
            self.at(
                w[0] * sx + w[1] * q1.0 + w[2] * q2.0 + w[3] * to.0,
                w[0] * sy + w[1] * q1.1 + w[2] * q2.1 + w[3] * to.1,
            );
        }
        self
    }
    /// Calls `f` for every final point: sharp corners as given and rounded
    /// corners as fillet arcs. A last point repeating the first is dropped.
    fn trace(&self, mut f: impl FnMut(f32, f32)) {
        let mut n = self.len;
        if n > 2 {
            let (a, b) = (self.corners[0], self.corners[n - 1]);
            if (a.0 - b.0).abs() < 1e-5 && (a.1 - b.1).abs() < 1e-5 {
                n -= 1;
            }
        }
        for i in 0..n {
            let (x, y, r) = self.corners[i];
            if r > 0. && n > 2 {
                let (px, py, _) = self.corners[(i + n - 1) % n];
                let (nx, ny, _) = self.corners[(i + 1) % n];
                fillet(vec2(px, py), vec2(x, y), vec2(nx, ny), r, &mut f);
            } else {
                f(x, y);
            }
        }
    }
}

/// The signed turn from angle `from` to angle `to` the short way, in `(-π, π]`.
fn short_sweep(from: f32, to: f32) -> f32 {
    let turn = (to - from).rem_euclid(TAU);
    if turn > PI { turn - TAU } else { turn }
}

/// Segments for an arc of radius `r` (unit box): under a fifth of a pixel of
/// chord error on a 128-pixel icon.
fn arc_steps(r: f32, sweep: f32) -> usize {
    let step = (0.14 / r.abs().max(1e-3).sqrt()).clamp(0.08, 0.7);
    ((sweep.abs() / step).ceil() as usize).clamp(1, 48)
}

/// Emits the arc rounding corner `p` (between `prev` and `next`) with radius
/// `r`, shrunk so it uses at most half of either edge. Works for convex and
/// reflex corners alike.
fn fillet(prev: Vec2, p: Vec2, next: Vec2, r: f32, f: &mut impl FnMut(f32, f32)) {
    let (a, b) = (prev - p, next - p);
    let (u, v) = (a.normalize_or_zero(), b.normalize_or_zero());
    let theta = u.dot(v).clamp(-1., 1.).acos();
    if !(theta > 1e-3 && theta < PI - 1e-3) {
        return f(p.x, p.y);
    }
    let half = theta * 0.5;
    let reach = (r / half.tan()).min(0.5 * a.length().min(b.length()));
    let r = reach * half.tan();
    let center = p + (u + v).normalize_or_zero() * (r / half.sin());
    let start = (p + u * reach - center).to_angle();
    let sweep = short_sweep(start, (p + v * reach - center).to_angle());
    let steps = arc_steps(r, sweep);
    for i in 0..=steps {
        let angle = start + sweep * i as f32 / steps as f32;
        f(center.x + r * angle.cos(), center.y + r * angle.sin());
    }
}

/// Fills the closed outline `points` (logical units) as one mesh, so concave
/// pictograms stay a single piece: no seams, and a translucent icon is not
/// darkened where convex parts would overlap. With a `hub` the interior is a
/// fan from it, for outlines star-shaped about it (stars, hearts, gears,
/// arrows). Without one the outline is a ladder of even length `n`: rungs
/// join point `k` to point `n - k` (crescents). Edges get the same half-pixel
/// fringe and mitre limit as [`shapes::fill_polygon`].
fn fill_mesh(points: &[Vec2], hub: Option<Vec2>, color: Color) {
    let n = points.len();
    if n < 3
        || (hub.is_none() && !n.is_multiple_of(2))
        || !hub.is_none_or(|h| h.is_finite())
        || !points.iter().all(|p| p.is_finite())
    {
        return;
    }
    let twice_area: f32 = (0..n)
        .map(|i| points[i].perp_dot(points[(i + 1) % n]))
        .sum();
    if !(twice_area.is_finite() && twice_area.abs() > 1e-9) {
        return;
    }
    let outward = if twice_area > 0. { -1. } else { 1. };
    let clear = Color::new(color.r, color.g, color.b, 0.);
    crate::edit(|s| {
        let mut sink = shapes::Sink::new(s);
        let f = sink.device_pixel() * 0.5;
        // Inner (opaque) and outer (clear) fringe points of corner `i`.
        let fringe = |i: usize| {
            let p = points[i];
            let n_in = (p - points[(i + n - 1) % n]).normalize_or_zero().perp() * outward;
            let n_out = (points[(i + 1) % n] - p).normalize_or_zero().perp() * outward;
            let miter = (n_in + n_out).normalize_or(n_out);
            let reach = f / miter.dot(n_out).abs().max(0.25);
            (p - miter * reach, p + miter * reach)
        };
        sink.reserve(n * 9);
        let (mut inner, mut outer) = fringe(n - 1);
        for i in 0..n {
            let (next_inner, next_outer) = fringe(i);
            if let Some(hub) = hub {
                sink.tri(hub, color, inner, color, next_inner, color);
            }
            sink.tri(inner, color, next_inner, color, outer, clear);
            sink.tri(outer, clear, next_inner, color, next_outer, clear);
            (inner, outer) = (next_inner, next_outer);
        }
        if hub.is_none() {
            // Ladder interior: a quad between consecutive rungs.
            for k in 0..n / 2 {
                let (a, b) = (fringe(k).0, fringe(k + 1).0);
                let (e, g) = (fringe(n - k - 1).0, fringe((n - k) % n).0);
                sink.tri(a, color, b, color, e, color);
                sink.tri(a, color, e, color, g, color);
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Reusable outlines.

/// Star-shaped outline of `n` round-ended arms of half-width `half` reaching
/// `len` from the origin (the arm's cap center): a plus, a cross, an asterisk.
fn arms(n: usize, first: f32, len: f32, half: f32) -> Outline {
    let mut o = Outline::new();
    let step = TAU / n as f32;
    let reach = half / (step * 0.5).sin();
    for k in 0..n {
        let a = first + step * k as f32;
        let between = a - step * 0.5;
        o.at(reach * between.cos(), reach * between.sin());
        o.arc((len * a.cos(), len * a.sin(), half), a - FRAC_PI_2, PI);
    }
    o
}

/// Five-point star about `(cx, cy)` with outer radius `r`, inner radius
/// `r * inner`, and rounded tips and notches.
fn star_outline(cx: f32, cy: f32, r: f32, inner: f32, (tip, notch): (f32, f32)) -> Outline {
    let mut o = Outline::new();
    for k in 0..10 {
        let a = -FRAC_PI_2 + k as f32 * PI / 5.;
        let (radius, round) = if k % 2 == 0 {
            (r, tip)
        } else {
            (r * inner, notch)
        };
        o.round(cx + radius * a.cos(), cy + radius * a.sin(), round);
    }
    o
}

/// Four-point sparkle about `(cx, cy)`.
fn sparkle(cx: f32, cy: f32, r: f32) -> Outline {
    let mut o = Outline::new();
    for k in 0..8 {
        let a = k as f32 * PI / 4.;
        let radius = if k % 2 == 0 { r } else { r * 0.34 };
        o.round(cx + radius * a.cos(), cy + radius * a.sin(), 0.);
    }
    o
}

/// Circle `(cx, cy, r)` drawn out to a point at `(tx, ty)` along its two
/// tangents: drops, pins and flames. Convex.
fn teardrop((cx, cy, r): (f32, f32, f32), (tx, ty): (f32, f32), tip_round: f32) -> Outline {
    let to_tip = vec2(tx - cx, ty - cy);
    let base = to_tip.to_angle();
    let off = (r / to_tip.length()).clamp(-1., 1.).acos();
    let mut o = Outline::new();
    o.round(tx, ty, tip_round);
    o.arc((cx, cy, r), base + off, TAU - 2. * off);
    o
}

/// Convex lens between tips `a` and `b`, each side an arc bulging `bulge`
/// (at most half the tip distance) from the chord.
fn lens(a: (f32, f32), b: (f32, f32), bulge: f32) -> Outline {
    let (a, b) = (vec2(a.0, a.1), vec2(b.0, b.1));
    let half = (b - a).length() * 0.5;
    let normal = ((b - a) / (2. * half)).perp();
    let radius = (half * half + bulge * bulge) / (2. * bulge);
    let mut o = Outline::new();
    for (side, from, to) in [(1., a, b), (-1., b, a)] {
        let center = (a + b) * 0.5 - normal * (side * (radius - bulge));
        let start = (from - center).to_angle();
        let sweep = short_sweep(start, (to - center).to_angle());
        o.arc((center.x, center.y, radius), start, sweep);
    }
    o
}

/// A leaf from `base` to `tip`, about `bulge` wide on each side: two cubic
/// curves, fullest nearer the base and meeting at a sharp (about 55°) tip.
/// The control polygons turn one way only (`0.4 * 1.86 < 0.88 * 0.92`), so
/// the outline is convex for any length and width.
fn leaf_outline(base: (f32, f32), tip: (f32, f32), bulge: f32) -> Outline {
    let (b, t) = (vec2(base.0, base.1), vec2(tip.0, tip.1));
    let (axis, normal) = (t - b, (t - b).normalize_or(Vec2::X).perp());
    let at = |along: f32, out: f32| {
        let q = b + axis * along + normal * (bulge * out);
        (q.x, q.y)
    };
    let mut o = Outline::new();
    o.at(b.x, b.y)
        .cubic(at(0.12, 1.86), at(0.6, 0.92), (t.x, t.y), 14)
        .cubic(at(0.6, -0.92), at(0.12, -1.86), (b.x, b.y), 14);
    o
}

/// Ellipse with radii `rx`, `ry` turned `tilt` radians clockwise.
fn ellipse(cx: f32, cy: f32, (rx, ry): (f32, f32), tilt: f32) -> Outline {
    let mut o = Outline::new();
    let turn = Vec2::from_angle(tilt);
    for k in 0..28 {
        let t = k as f32 * TAU / 28.;
        let p = vec2(rx * t.cos(), ry * t.sin()).rotate(turn);
        o.at(cx + p.x, cy + p.y);
    }
    o
}

/// Intersection of line `a`–`b` with line `c`–`d`.
fn meet(a: (f32, f32), b: (f32, f32), c: (f32, f32), d: (f32, f32)) -> (f32, f32) {
    let (r, s) = (vec2(b.0 - a.0, b.1 - a.1), vec2(d.0 - c.0, d.1 - c.1));
    let t = vec2(c.0 - a.0, c.1 - a.1).perp_dot(s) / r.perp_dot(s);
    (a.0 + r.x * t, a.1 + r.y * t)
}

/// A filled arrowhead whose base is centered at angle `at` on the circle
/// `(cx, cy, r)`, pointing along the circle (`dir` +1 is clockwise).
fn arrowhead(
    p: &Pen,
    (cx, cy, r): (f32, f32, f32),
    at: f32,
    dir: f32,
    (len, half): (f32, f32),
    c: Color,
) {
    let center = vec2(cx, cy);
    let base = center + Vec2::from_angle(at) * r;
    // Aim at where the circle is one head length ahead, following the curve.
    let ahead = center + Vec2::from_angle(at + dir * len / r) * r;
    let forward = (ahead - base).normalize_or(Vec2::X);
    let (tip, side) = (base + forward * len, forward.perp() * half);
    p.fill(
        &Outline::corners(&[
            (tip.x, tip.y, 0.05),
            (base.x + side.x, base.y + side.y, 0.04),
            (base.x - side.x, base.y - side.y, 0.04),
        ]),
        c,
    );
}

// ---------------------------------------------------------------------------
// The pictograms.

/// Paint one icon with the silhouette `c` and inner-mark tone `d`.
fn paint(icon: Icon, p: &Pen, c: Color, d: Color) {
    match icon {
        Icon::Play => play(p, c),
        Icon::Pause => pause(p, c),
        Icon::Restart => restart(p, c),
        Icon::Undo => undo(p, c),
        Icon::Check => check(p, c),
        Icon::Cross => p.fan((0., 0.), &arms(4, deg(45.), 0.88, 0.15), c),
        Icon::Plus => p.fan((0., 0.), &arms(4, 0., 0.66, 0.16), c),
        Icon::Minus => p.capsule((-0.66, 0.), (0.66, 0.), 0.16, c),
        Icon::ArrowUp => arrow(p, -FRAC_PI_2, c),
        Icon::ArrowDown => arrow(p, FRAC_PI_2, c),
        Icon::ArrowLeft => arrow(p, PI, c),
        Icon::ArrowRight => arrow(p, 0., c),
        Icon::Question => question(p, c),
        Icon::Info => info(p, c),
        Icon::Alert => alert(p, c, d),
        Icon::SoundOn => sound(p, c, true),
        Icon::SoundOff => sound(p, c, false),
        Icon::Gear => gear(p, c, d),
        Icon::Gamepad => gamepad(p, c, d),
        Icon::Pointer => pointer(p, c),
        Icon::Hand => hand(p, c),
        Icon::Eye => eye(p, c, d, false),
        Icon::EyeOff => eye(p, c, d, true),
        Icon::Lock => lock(p, c, d, false),
        Icon::Unlock => lock(p, c, d, true),
        Icon::Key => key(p, c, d),
        Icon::Home => home(p, c, d),
        Icon::Trophy => trophy(p, c, d),
        Icon::Crown => crown(p, c, d),
        Icon::Flag => flag(p, c),
        Icon::Target => target(p, c),
        Icon::Pin => pin(p, c, d),
        Icon::Map => map(p, c, d),
        Icon::Search => search(p, c),
        Icon::Bell => bell(p, c),
        Icon::Star => p.fan(
            (0., 0.08),
            &star_outline(0., 0.08, 1.02, 0.5, (0.1, 0.04)),
            c,
        ),
        Icon::Heart => heart(p, c),
        Icon::Skull => skull(p, c, d),
        Icon::Clock => clock(p, c, d),
        Icon::Hourglass => hourglass(p, c, d),
        Icon::Calendar => calendar(p, c, d),
        Icon::Bolt => bolt(p, c),
        Icon::Battery => battery(p, c, d),
        Icon::Shield => shield(p, c, d),
        Icon::Sword => sword(p, c, d),
        Icon::Bomb => bomb(p, c),
        Icon::Flame => flame(p, c, d),
        Icon::Drop => water_drop(p, c, d),
        Icon::Snowflake => snowflake(p, c),
        Icon::Sun => sun(p, c),
        Icon::Moon => p.crescent((-0.04, 0.04, 0.84), (0.42, -0.32, 0.68), c),
        Icon::Cloud => cloud(p, c),
        Icon::Rain => rain(p, c),
        Icon::Wind => wind(p, c),
        Icon::Wave => wave(p, c),
        Icon::Leaf => leaf(p, c, d),
        Icon::Sprout => sprout(p, c, d),
        Icon::Tree => tree(p, c),
        Icon::Mountain => mountain(p, c, d),
        Icon::Coin => coin(p, c, d),
        Icon::Gem => gem(p, c, d),
        Icon::Bag => bag(p, c, d),
        Icon::Crate => wooden_crate(p, c, d),
        Icon::Scroll => scroll(p, c, d),
        Icon::Book => book(p, c, d),
        Icon::Bulb => bulb(p, c, d),
        Icon::Hammer => hammer(p, c, d),
        Icon::Wrench => wrench(p, c),
        Icon::Pickaxe => pickaxe(p, c, d),
        Icon::Anchor => anchor(p, c),
        Icon::Ship => ship(p, c, d),
        Icon::Rocket => rocket(p, c, d),
        Icon::Planet => planet(p, c, d),
        Icon::Train => train(p, c, d),
        Icon::Person => person(p, c),
        Icon::People => people(p, c, d),
        Icon::Paw => paw(p, c),
        Icon::Bone => bone(p, c),
        Icon::Potion => potion(p, c, d),
        Icon::Fish => fish(p, c, d),
        Icon::Music => music(p, c),
        Icon::Chat => chat(p, c, d),
        Icon::Pot => pot(p, c, d),
        Icon::Thermometer => thermometer(p, c, d),
        Icon::Fuel => fuel(p, c, d),
        Icon::FastForward => fast_forward(p, c),
        Icon::Magnet => magnet(p, c, d),
        Icon::Bug => bug(p, c, d),
        Icon::Chain => chain(p, c),
        Icon::Move => four_way(p, c),
        Icon::Cards => cards(p, c, d),
        Icon::Ball => ball(p, c, d),
    }
}

fn cards(p: &Pen, c: Color, d: Color) {
    // The back card leans left; the front card leans right and carries a pip.
    let card = |s: &Pen| {
        s.fill(
            &Outline::corners(&[
                (-0.42, -0.62, 0.12),
                (0.42, -0.62, 0.12),
                (0.42, 0.62, 0.12),
                (-0.42, 0.62, 0.12),
            ]),
            c,
        );
    };
    let back = p.sub(-0.2, 0.04, 1., deg(-14.));
    card(&back);
    back.stroke_closed(
        &Outline::corners(&[
            (-0.28, -0.48, 0.06),
            (0.28, -0.48, 0.06),
            (0.28, 0.48, 0.06),
            (-0.28, 0.48, 0.06),
        ]),
        0.08,
        d,
    );
    let front = p.sub(0.22, 0.02, 1., deg(10.));
    // A dark rim separates the overlapping cards without a cut-out.
    front.fill(
        &Outline::corners(&[
            (-0.5, -0.7, 0.16),
            (0.5, -0.7, 0.16),
            (0.5, 0.7, 0.16),
            (-0.5, 0.7, 0.16),
        ]),
        d,
    );
    card(&front);
    front.fill(
        &Outline::corners(&[
            (0., -0.26, 0.04),
            (0.2, 0., 0.04),
            (0., 0.26, 0.04),
            (-0.2, 0., 0.04),
        ]),
        d,
    );
}

fn ball(p: &Pen, c: Color, d: Color) {
    p.circle(0., 0., 0.84, c);
    // Dimples in the detail tone, and a soft highlight.
    for (x, y) in [
        (0.28, 0.22),
        (-0.06, 0.42),
        (0.42, -0.12),
        (-0.3, 0.12),
        (0.12, -0.02),
    ] {
        p.circle(x, y, 0.08, d);
    }
    p.arc(
        (0., 0., 0.58),
        deg(200.),
        deg(60.),
        0.12,
        Color::new(1., 1., 1., c.a * 0.55),
    );
}

// Additions requested by the games: speed, magnets, pests, links, movement.

fn fast_forward(p: &Pen, c: Color) {
    for x in [-0.9, -0.02] {
        p.fill(
            &Outline::corners(&[(x, -0.74, 0.12), (x + 0.9, 0., 0.12), (x, 0.74, 0.12)]),
            c,
        );
    }
}

fn magnet(p: &Pen, c: Color, d: Color) {
    // An upright horseshoe: a thick bottom arc and two legs with light poles.
    p.arc((0., 0.08, 0.52), 0., PI, 0.36, c);
    p.rect(-0.7, -0.62, 0.36, 0.72, 0.03, c);
    p.rect(0.34, -0.62, 0.36, 0.72, 0.03, c);
    p.rect(-0.7, -0.9, 0.36, 0.34, 0.07, d);
    p.rect(0.34, -0.9, 0.36, 0.34, 0.07, d);
}

fn bug(p: &Pen, c: Color, d: Color) {
    // Legs first so the shell covers their roots.
    for (y, reach) in [(-0.16, -0.36), (0.16, 0.), (0.46, 0.36)] {
        p.line(
            &[(-0.3, y), (-0.66, y + reach * 0.45), (-0.84, y + reach)],
            0.11,
            c,
        );
        p.line(
            &[(0.3, y), (0.66, y + reach * 0.45), (0.84, y + reach)],
            0.11,
            c,
        );
    }
    p.line(&[(-0.12, -0.62), (-0.3, -0.86)], 0.09, c);
    p.line(&[(0.12, -0.62), (0.3, -0.86)], 0.09, c);
    p.circle(0., -0.5, 0.24, c);
    p.fill(&ellipse(0., 0.18, (0.48, 0.6), 0.), c);
    // Wing split and two spots in the detail tone.
    p.line(&[(0., -0.3), (0., 0.7)], 0.07, d);
    p.circle(-0.22, 0.1, 0.09, d);
    p.circle(0.24, 0.38, 0.09, d);
}

fn chain(p: &Pen, c: Color) {
    // Rounded links as outlines, so they follow the pen's rotation.
    let s = p.sub(0., 0., 1., deg(-45.));
    for x in [-0.94, -0.12] {
        let link = Outline::corners(&[
            (x, -0.28, 0.26),
            (x + 1.06, -0.28, 0.26),
            (x + 1.06, 0.28, 0.26),
            (x, 0.28, 0.26),
        ]);
        s.stroke_closed(&link, 0.19, c);
    }
}

fn four_way(p: &Pen, c: Color) {
    p.capsule((-0.56, 0.), (0.56, 0.), 0.11, c);
    p.capsule((0., -0.56), (0., 0.56), 0.11, c);
    for angle in [0., FRAC_PI_2, PI, -FRAC_PI_2] {
        p.sub(0., 0., 1., angle).fill(
            &Outline::corners(&[(0.92, 0., 0.06), (0.5, -0.32, 0.05), (0.5, 0.32, 0.05)]),
            c,
        );
    }
}

// Interface and menus.

fn play(p: &Pen, c: Color) {
    p.fill(
        &Outline::corners(&[(-0.62, -0.9, 0.15), (0.92, 0., 0.15), (-0.62, 0.9, 0.15)]),
        c,
    );
}

fn pause(p: &Pen, c: Color) {
    p.rect(-0.66, -0.8, 0.46, 1.6, 0.14, c);
    p.rect(0.2, -0.8, 0.46, 1.6, 0.14, c);
}

fn restart(p: &Pen, c: Color) {
    let circle = (0., 0.06, 0.6);
    let (start, sweep) = (deg(-28.), deg(284.));
    p.arc(circle, start, sweep, 0.24, c);
    arrowhead(p, circle, start + sweep, 1., (0.44, 0.32), c);
}

fn undo(p: &Pen, c: Color) {
    let (y, r, cx) = (-0.3, 0.4, 0.38);
    let mut hook = Outline::new();
    hook.at(-0.4, y)
        .arc((cx, y + r, r), -FRAC_PI_2, PI)
        .at(-0.12, y + 2. * r);
    p.stroke(&hook, 0.22, c);
    p.fill(
        &Outline::corners(&[
            (-0.9, y, 0.05),
            (-0.34, y - 0.4, 0.04),
            (-0.34, y + 0.4, 0.04),
        ]),
        c,
    );
}

fn check(p: &Pen, c: Color) {
    let (a, corner, b, half) = (vec2(-0.68, 0.04), vec2(-0.2, 0.52), vec2(0.72, -0.48), 0.14);
    let (u, v) = ((a - corner).normalize(), (b - corner).normalize());
    // Unit normals toward the inside of the V for each arm.
    let inner_a = u.perp() * u.perp().dot(v).signum();
    let inner_b = v.perp() * v.perp().dot(u).signum();
    let edge = |from: Vec2, dir: Vec2, normal: Vec2, side: f32| {
        let p = from + normal * (half * side);
        ((p.x, p.y), (p.x + dir.x, p.y + dir.y))
    };
    let (ia, ib) = (edge(corner, u, inner_a, 1.), edge(corner, v, inner_b, 1.));
    let (oa, ob) = (edge(corner, u, inner_a, -1.), edge(corner, v, inner_b, -1.));
    let inner = meet(ia.0, ia.1, ib.0, ib.1);
    let outer = meet(oa.0, oa.1, ob.0, ob.1);
    // Semicircular cap at `end` from the inner side round to the outer side.
    let cap = |o: &mut Outline, end: Vec2, dir: Vec2, normal: Vec2| {
        let start = normal.to_angle();
        let ahead = (dir.to_angle() - start).rem_euclid(TAU);
        o.arc(
            (end.x, end.y, half),
            start,
            if ahead < PI { PI } else { -PI },
        );
    };
    let mut o = Outline::new();
    o.at(inner.0, inner.1);
    cap(&mut o, a, u, inner_a);
    o.round(outer.0, outer.1, half);
    cap(&mut o, b, v, -inner_b);
    p.fan((corner.x, corner.y), &o, c);
}

fn arrow(p: &Pen, angle: f32, c: Color) {
    let shape = Outline::corners(&[
        (-0.82, -0.17, 0.08),
        (0.02, -0.17, 0.03),
        (0.02, -0.68, 0.08),
        (0.86, 0., 0.1),
        (0.02, 0.68, 0.08),
        (0.02, 0.17, 0.03),
        (-0.82, 0.17, 0.08),
    ]);
    p.sub(0., 0., 1., angle).fan((0.2, 0.), &shape, c);
}

fn question(p: &Pen, c: Color) {
    let mut hook = Outline::new();
    hook.arc((0., -0.34, 0.36), deg(192.), deg(218.))
        .at(0.06, 0.02)
        .at(0.02, 0.12)
        .at(0.02, 0.24);
    p.stroke(&hook, 0.27, c);
    p.circle(0.02, 0.66, 0.17, c);
}

fn info(p: &Pen, c: Color) {
    p.circle(0.04, -0.62, 0.18, c);
    let stem = Outline::corners(&[
        (-0.36, -0.26, 0.06),
        (0.2, -0.26, 0.06),
        (0.2, 0.6, 0.),
        (-0.14, 0.6, 0.),
        (-0.14, 0., 0.03),
        (-0.36, 0., 0.06),
    ]);
    p.fan((0.03, -0.13), &stem, c);
    p.rect(-0.38, 0.52, 0.82, 0.28, 0.08, c);
}

fn alert(p: &Pen, c: Color, d: Color) {
    p.fill(
        &Outline::corners(&[(0., -0.88, 0.16), (0.94, 0.8, 0.16), (-0.94, 0.8, 0.16)]),
        c,
    );
    p.capsule((0., -0.28), (0., 0.2), 0.11, d);
    p.circle(0., 0.5, 0.12, d);
}

fn sound(p: &Pen, c: Color, on: bool) {
    let x = -0.4;
    let speaker = Outline::corners(&[
        (x - 0.44, -0.27, 0.08),
        (x, -0.27, 0.02),
        (x + 0.44, -0.7, 0.08),
        (x + 0.44, 0.7, 0.08),
        (x, 0.27, 0.02),
        (x - 0.44, 0.27, 0.08),
    ]);
    p.fan((x - 0.04, 0.), &speaker, c);
    if on {
        p.arc((0.02, 0., 0.42), deg(-44.), deg(88.), 0.17, c);
        p.arc((0.02, 0., 0.72), deg(-46.), deg(92.), 0.17, c);
    } else {
        p.sub(0.5, 0., 1., 0.)
            .fan((0., 0.), &arms(4, deg(45.), 0.3, 0.09), c);
    }
}

fn gear(p: &Pen, c: Color, d: Color) {
    let (teeth, body, tip) = (8, 0.62, 0.9);
    let step = TAU / teeth as f32;
    let (base_half, tip_half) = (deg(13.), deg(8.5));
    let mut o = Outline::new();
    for k in 0..teeth {
        let a = step * k as f32;
        let at = |r: f32, t: f32| (r * t.cos(), r * t.sin());
        for (r, t, round) in [
            (body, a - base_half, 0.03),
            (tip, a - tip_half, 0.06),
            (tip, a + tip_half, 0.06),
            (body, a + base_half, 0.03),
            (body, a + step * 0.5, 0.),
        ] {
            let (x, y) = at(r, t);
            o.round(x, y, round);
        }
    }
    p.fan((0., 0.), &o, c);
    p.circle(0., 0., 0.26, d);
}

fn gamepad(p: &Pen, c: Color, d: Color) {
    p.rect(-0.88, -0.6, 1.76, 0.76, 0.32, c);
    p.capsule((-0.52, -0.1), (-0.64, 0.4), 0.26, c);
    p.capsule((0.52, -0.1), (0.64, 0.4), 0.26, c);
    p.sub(-0.46, -0.22, 1., 0.)
        .fan((0., 0.), &arms(4, 0., 0.14, 0.075), d);
    p.circle(0.34, -0.12, 0.1, d);
    p.circle(0.56, -0.32, 0.1, d);
}

fn pointer(p: &Pen, c: Color) {
    let cursor = Outline::corners(&[
        (-0.52, -0.9, 0.06),
        (0.56, 0.18, 0.06),
        (0.06, 0.2, 0.02),
        (0.34, 0.76, 0.06),
        (0.12, 0.88, 0.06),
        (-0.16, 0.32, 0.02),
        (-0.52, 0.66, 0.06),
    ]);
    p.fan((-0.16, 0.12), &cursor, c);
}

fn hand(p: &Pen, c: Color) {
    let p = &p.sub(0.12, 0., 1., 0.);
    p.rect(-0.46, -0.02, 0.98, 0.9, 0.3, c);
    p.capsule((0.04, -0.06), (0.04, 0.2), 0.15, c);
    p.capsule((0.32, 0.), (0.32, 0.2), 0.14, c);
    p.capsule((-0.26, -0.5), (-0.26, 0.2), 0.16, c);
    p.capsule((-0.66, 0.1), (-0.42, 0.42), 0.13, c);
    p.arc((-0.26, -0.5, 0.32), deg(200.), deg(140.), 0.12, c);
}

fn eye(p: &Pen, c: Color, d: Color, off: bool) {
    p.stroke_closed(&lens((-0.82, 0.), (0.82, 0.), 0.54), 0.17, c);
    p.circle(0., 0., 0.3, c);
    if off {
        // The slash, set off from the iris by a detail band clipped to it.
        p.fill(&disc_band(0.3, Vec2::from_angle(deg(45.)), 0.17), d);
        p.capsule((-0.7, -0.7), (0.7, 0.7), 0.09, c);
    } else {
        p.circle(0., 0., 0.12, d);
    }
}

/// The part of the origin-centered disc of radius `r` lying within `half` of
/// the line through its center along `dir`: a convex band across the disc.
fn disc_band(r: f32, dir: Vec2, half: f32) -> Outline {
    let u = dir.normalize_or(Vec2::X);
    let (along, across) = (u * (r * r - half * half).max(0.).sqrt(), u.perp() * half);
    let mut o = Outline::new();
    for side in [1., -1.] {
        // Along one edge of the band, then round the rim to the other edge.
        let (from, to) = ((across + along) * side, (across - along) * side);
        let end = (-across - along) * side;
        let start = to.to_angle();
        o.at(from.x, from.y).at(to.x, to.y).arc(
            (0., 0., r),
            start,
            short_sweep(start, end.to_angle()),
        );
    }
    o
}

fn lock(p: &Pen, c: Color, d: Color, open: bool) {
    let p = &p.sub(0., -0.03, 1., 0.);
    let r = 0.3;
    let top = if open { -0.48 } else { -0.36 };
    let mut shackle = Outline::new();
    shackle
        .at(-r, 0.)
        .at(-r, top)
        .arc((0., top, r), PI, PI)
        .at(r, if open { top + 0.12 } else { 0. });
    p.stroke(&shackle, 0.2, c);
    p.rect(-0.66, -0.06, 1.32, 0.94, 0.16, c);
    p.circle(0., 0.36, 0.13, d);
    p.capsule((0., 0.36), (0., 0.62), 0.07, d);
}

fn key(p: &Pen, c: Color, d: Color) {
    let k = p.sub(0.03, 0.03, 1., deg(45.));
    k.circle(-0.66, 0., 0.4, c);
    k.rect(-0.4, -0.13, 1.32, 0.26, 0.06, c);
    k.rect(0.34, -0.06, 0.17, 0.36, 0.04, c);
    k.rect(0.62, -0.06, 0.2, 0.42, 0.04, c);
    k.circle(-0.74, 0., 0.15, d);
}

fn home(p: &Pen, c: Color, d: Color) {
    p.rect(0.36, -0.76, 0.22, 0.4, 0.04, c);
    let house = Outline::corners(&[
        (0., -0.86, 0.1),
        (0.92, -0.02, 0.08),
        (0.66, -0.02, 0.),
        (0.66, 0.86, 0.1),
        (-0.66, 0.86, 0.1),
        (-0.66, -0.02, 0.),
        (-0.92, -0.02, 0.08),
    ]);
    p.fan((0., -0.12), &house, c);
    p.rect(-0.18, 0.32, 0.36, 0.54, 0.08, d);
}

fn trophy(p: &Pen, c: Color, d: Color) {
    let mut cup = Outline::new();
    cup.round(-0.56, -0.86, 0.06)
        .round(0.56, -0.86, 0.06)
        .arc((0., -0.34, 0.56), 0., PI);
    p.fill(&cup, c);
    p.arc((-0.56, -0.54, 0.22), FRAC_PI_2, PI, 0.15, c);
    p.arc((0.56, -0.54, 0.22), -FRAC_PI_2, PI, 0.15, c);
    p.rect(-0.1, 0.12, 0.2, 0.42, 0., c);
    p.rect(-0.44, 0.5, 0.88, 0.36, 0.08, c);
    p.fan(
        (0., -0.48),
        &star_outline(0., -0.48, 0.27, 0.5, (0.03, 0.)),
        d,
    );
}

fn crown(p: &Pen, c: Color, d: Color) {
    let peaks = [(-0.8, -0.44), (0., -0.66), (0.8, -0.44)];
    let body = Outline::corners(&[
        (-0.76, 0.74, 0.08),
        (peaks[0].0, peaks[0].1, 0.04),
        (-0.4, 0., 0.03),
        (peaks[1].0, peaks[1].1, 0.04),
        (0.4, 0., 0.03),
        (peaks[2].0, peaks[2].1, 0.04),
        (0.76, 0.74, 0.08),
    ]);
    p.fan((0., 0.6), &body, c);
    // Balls sit on the peak corners, overlapping the rounded tips.
    for (x, y) in peaks {
        p.circle(x, y - 0.015, 0.11, c);
    }
    for x in [-0.4, 0., 0.4] {
        p.circle(x, 0.46, 0.1, d);
    }
}

fn flag(p: &Pen, c: Color) {
    p.capsule((-0.66, -0.76), (-0.66, 0.8), 0.09, c);
    p.circle(-0.66, -0.8, 0.11, c);
    let mut cloth = Outline::new();
    let wave = |t: f32| 0.09 * (t * TAU).sin();
    for i in 0..=10 {
        let t = i as f32 / 10.;
        cloth.at(-0.6 + 1.46 * t, -0.74 + wave(t));
    }
    for i in (0..=10).rev() {
        let t = i as f32 / 10.;
        cloth.at(-0.6 + 1.46 * t, wave(t));
    }
    p.fan((0.13, -0.37), &cloth, c);
}

fn target(p: &Pen, c: Color) {
    p.ring(0., 0., 0.76, 0.2, c);
    p.ring(0., 0., 0.4, 0.2, c);
    p.circle(0., 0., 0.14, c);
}

fn pin(p: &Pen, c: Color, d: Color) {
    p.fill(&teardrop((0., -0.26, 0.6), (0., 0.92), 0.06), c);
    p.circle(0., -0.26, 0.24, d);
}

fn map(p: &Pen, c: Color, d: Color) {
    let sheet = Outline::corners(&[
        (-0.88, -0.66, 0.06),
        (-0.3, -0.86, 0.02),
        (0.3, -0.66, 0.02),
        (0.88, -0.86, 0.06),
        (0.88, 0.66, 0.06),
        (0.3, 0.86, 0.02),
        (-0.3, 0.66, 0.02),
        (-0.88, 0.86, 0.06),
    ]);
    p.fan((0., 0.), &sheet, c);
    p.line(&[(-0.3, -0.74), (-0.3, 0.54)], 0.13, d);
    p.line(&[(0.3, -0.54), (0.3, 0.74)], 0.13, d);
}

fn search(p: &Pen, c: Color) {
    p.ring(-0.16, -0.16, 0.48, 0.22, c);
    p.capsule((0.22, 0.22), (0.66, 0.66), 0.16, c);
}

fn bell(p: &Pen, c: Color) {
    let mut body = Outline::new();
    body.round(-0.86, 0.58, 0.08)
        .round(-0.86, 0.38, 0.08)
        .round(-0.68, 0.38, 0.04)
        .arc((0., -0.22, 0.5), PI, PI)
        .round(0.68, 0.38, 0.04)
        .round(0.86, 0.38, 0.08)
        .round(0.86, 0.58, 0.08);
    p.fan((0., 0.48), &body, c);
    p.circle(0., 0.74, 0.14, c);
    p.circle(0., -0.76, 0.1, c);
}

// Stats and resources.

fn heart(p: &Pen, c: Color) {
    let (a, cy, r): (f32, f32, f32) = (0.42, -0.3, 0.46);
    let tip = vec2(0., 0.84);
    let notch = -(r * r - a * a).sqrt();
    // Angle of the notch seen from the left lobe and of the left lobe's
    // outer tangent toward the tip; the right lobe mirrors both.
    let notch_angle = notch.atan2(a);
    let to_tip = tip - vec2(-a, cy);
    let tangent = to_tip.to_angle() + (r / to_tip.length()).acos();
    let mut o = Outline::new();
    o.arc((-a, cy, r), notch_angle, tangent - TAU - notch_angle)
        .round(tip.x, tip.y, 0.1)
        .arc((a, cy, r), PI - tangent, tangent - TAU - notch_angle);
    p.fan((0., 0.), &o, c);
}

fn skull(p: &Pen, c: Color, d: Color) {
    let (cy, r, jaw): (f32, f32, f32) = (-0.16, 0.72, 0.42);
    let drop = (r * r - jaw * jaw).sqrt();
    let left = drop.atan2(-jaw);
    let mut head = Outline::new();
    head.arc((0., cy, r), left, TAU - (2. * left - PI))
        .round(jaw, 0.86, 0.16)
        .round(-jaw, 0.86, 0.16);
    p.fan((0., 0.1), &head, c);
    p.circle(-0.28, -0.08, 0.2, d);
    p.circle(0.28, -0.08, 0.2, d);
    p.fill(
        &Outline::corners(&[(0., 0.16, 0.03), (0.11, 0.36, 0.03), (-0.11, 0.36, 0.03)]),
        d,
    );
    p.line(&[(-0.13, 0.62), (-0.13, 0.8)], 0.1, d);
    p.line(&[(0.13, 0.62), (0.13, 0.8)], 0.1, d);
}

fn clock(p: &Pen, c: Color, d: Color) {
    p.circle(0., 0., 0.88, c);
    p.line(&[(0., -0.54), (0., 0.), (0.36, 0.22)], 0.17, d);
}

fn hourglass(p: &Pen, c: Color, d: Color) {
    p.rect(-0.66, -0.9, 1.32, 0.2, 0.08, c);
    p.rect(-0.66, 0.7, 1.32, 0.2, 0.08, c);
    let glass = Outline::corners(&[
        (-0.52, -0.74, 0.),
        (0.52, -0.74, 0.),
        (0.52, -0.5, 0.2),
        (0.1, -0.04, 0.06),
        (0.1, 0.04, 0.06),
        (0.52, 0.5, 0.2),
        (0.52, 0.74, 0.),
        (-0.52, 0.74, 0.),
        (-0.52, 0.5, 0.2),
        (-0.1, 0.04, 0.06),
        (-0.1, -0.04, 0.06),
        (-0.52, -0.5, 0.2),
    ]);
    p.fan((0., 0.), &glass, c);
    p.poly(
        &[(-0.28, -0.36), (0.28, -0.36), (0.06, -0.12), (-0.06, -0.12)],
        d,
    );
    p.fill(
        &Outline::corners(&[(-0.38, 0.66, 0.), (0.38, 0.66, 0.), (0., 0.32, 0.1)]),
        d,
    );
}

fn calendar(p: &Pen, c: Color, d: Color) {
    p.rect(-0.82, -0.64, 1.64, 1.5, 0.18, c);
    p.capsule((-0.42, -0.8), (-0.42, -0.5), 0.1, c);
    p.capsule((0.42, -0.8), (0.42, -0.5), 0.1, c);
    p.rect(-0.68, -0.3, 1.36, 0.12, 0.06, d);
    for row in 0..2 {
        for col in 0..3 {
            let (x, y) = (-0.52 + col as f32 * 0.4, -0.04 + row as f32 * 0.4);
            p.rect(x, y, 0.24, 0.24, 0.05, d);
        }
    }
}

fn bolt(p: &Pen, c: Color) {
    let o = Outline::corners(&[
        (0.08, -0.92, 0.05),
        (0.64, -0.92, 0.05),
        (0.2, -0.16, 0.02),
        (0.66, -0.16, 0.05),
        (-0.34, 0.94, 0.04),
        (-0.16, 0.14, 0.02),
        (-0.64, 0.14, 0.05),
    ]);
    p.fan((0., 0.), &o, c);
}

fn battery(p: &Pen, c: Color, d: Color) {
    p.rect(-0.88, -0.46, 1.6, 0.92, 0.16, c);
    p.rect(0.66, -0.2, 0.24, 0.4, 0.08, c);
    for i in 0..3 {
        p.rect(-0.7 + i as f32 * 0.34, -0.28, 0.24, 0.56, 0.05, d);
    }
}

/// Heater-shield outline (or only its right half) with half-width `w`.
fn shield_outline(
    w: f32,
    (top, peak, side, tip): (f32, f32, f32, f32),
    right_half: bool,
) -> Outline {
    let mut o = Outline::new();
    if right_half {
        o.at(0., peak);
    } else {
        o.round(-w, top, 0.14).round(0., peak, 0.1);
    }
    o.round(w, top, 0.14)
        .at(w, side)
        .curve((w, side + (tip - side) * 0.6), (0., tip), 8);
    if !right_half {
        o.curve((-w, side + (tip - side) * 0.6), (-w, side), 8);
    }
    o
}

fn shield(p: &Pen, c: Color, d: Color) {
    p.fill(&shield_outline(0.8, (-0.66, -0.86, -0.08, 0.9), false), c);
    p.fill(&shield_outline(0.56, (-0.49, -0.64, -0.04, 0.61), true), d);
}

fn sword(p: &Pen, c: Color, d: Color) {
    let s = p.sub(0., 0., 1., deg(45.));
    s.fill(
        &Outline::corners(&[
            (-0.17, 0.3, 0.),
            (-0.17, -0.82, 0.),
            (0., -1.16, 0.04),
            (0.17, -0.82, 0.),
            (0.17, 0.3, 0.),
        ]),
        c,
    );
    s.poly(&[(0., 0.3), (0., -1.04), (0.13, -0.8), (0.13, 0.3)], d);
    s.capsule((-0.5, 0.36), (0.5, 0.36), 0.13, c);
    s.rect(-0.1, 0.38, 0.2, 0.52, 0.04, c);
    s.circle(0., 1.0, 0.17, c);
}

fn bomb(p: &Pen, c: Color) {
    let p = &p.sub(-0.06, 0.03, 1., 0.);
    p.circle(-0.1, 0.16, 0.68, c);
    p.sub(0.34, -0.28, 1., deg(45.))
        .rect(-0.2, -0.2, 0.4, 0.3, 0.06, c);
    let mut fuse = Outline::new();
    fuse.at(0.44, -0.42).curve((0.48, -0.66), (0.7, -0.7), 6);
    p.stroke(&fuse, 0.13, c);
    p.fan((0.72, -0.72), &sparkle(0.72, -0.72, 0.2), c);
}

fn flame(p: &Pen, c: Color, d: Color) {
    p.fill(&teardrop((-0.38, 0.4, 0.3), (-0.62, -0.26), 0.04), c);
    p.fill(&teardrop((0.4, 0.44, 0.28), (0.66, -0.1), 0.04), c);
    p.fill(&teardrop((0., 0.3, 0.58), (0.1, -0.92), 0.05), c);
    p.fill(&teardrop((0.02, 0.5, 0.3), (0.06, -0.1), 0.04), d);
}

fn water_drop(p: &Pen, c: Color, d: Color) {
    p.fill(&teardrop((0., 0.28, 0.6), (0., -0.92), 0.04), c);
    p.arc((0., 0.28, 0.38), deg(105.), deg(60.), 0.13, d);
}

fn snowflake(p: &Pen, c: Color) {
    p.fan((0., 0.), &arms(6, -FRAC_PI_2, 0.78, 0.09), c);
    for k in 0..6 {
        let u = Vec2::from_angle(-FRAC_PI_2 + k as f32 * TAU / 6.);
        let base = u * 0.44;
        let (l, r) = (
            base + u * 0.24 + u.perp() * 0.22,
            base + u * 0.24 - u.perp() * 0.22,
        );
        p.line(&[(l.x, l.y), (base.x, base.y), (r.x, r.y)], 0.16, c);
    }
}

fn sun(p: &Pen, c: Color) {
    p.circle(0., 0., 0.42, c);
    for k in 0..8 {
        let u = Vec2::from_angle(k as f32 * TAU / 8.);
        p.capsule((u.x * 0.64, u.y * 0.64), (u.x * 0.8, u.y * 0.8), 0.095, c);
    }
}

/// The upper crossing point of circles `a` and `b`, each `(x, y, r)`.
fn upper_crossing(a: (f32, f32, f32), b: (f32, f32, f32)) -> Vec2 {
    let (ca, cb) = (vec2(a.0, a.1), vec2(b.0, b.1));
    let dist = (cb - ca).length();
    let axis = (cb - ca) / dist;
    let along = (a.2 * a.2 - b.2 * b.2 + dist * dist) / (2. * dist);
    let lift = axis.perp() * (a.2 * a.2 - along * along).max(0.).sqrt();
    let (p, q) = (ca + axis * along + lift, ca + axis * along - lift);
    if p.y < q.y { p } else { q }
}

/// A cloud: a flat-bottomed run of bumps traced as one outline.
fn cloud(p: &Pen, c: Color) {
    // End caps of the flat base, then the bumps from right to left.
    let circles = [
        (0.56, 0.3, 0.3),
        (0.14, -0.12, 0.5),
        (-0.36, 0.1, 0.36),
        (-0.56, 0.3, 0.3),
    ];
    let mut o = Outline::new();
    o.at(-0.56, 0.6);
    // Counter-clockwise along each circle, from where the previous one hands
    // over to where the next takes over; the last runs down to the base.
    let mut from = FRAC_PI_2;
    for (i, &(x, y, r)) in circles.iter().enumerate() {
        let (to, next_from) = match circles.get(i + 1) {
            Some(&(nx, ny, nr)) => {
                let crossing = upper_crossing((x, y, r), (nx, ny, nr));
                (
                    (crossing - vec2(x, y)).to_angle(),
                    (crossing - vec2(nx, ny)).to_angle(),
                )
            }
            None => (FRAC_PI_2, FRAC_PI_2),
        };
        o.arc((x, y, r), from, -(from - to).rem_euclid(TAU));
        from = next_from;
    }
    p.fan((0., 0.3), &o, c);
}

fn rain(p: &Pen, c: Color) {
    cloud(&p.sub(0., -0.24, 0.8, 0.), c);
    for x in [-0.42, 0., 0.42] {
        p.capsule((x + 0.08, 0.44), (x - 0.04, 0.72), 0.08, c);
    }
}

fn wind(p: &Pen, c: Color) {
    let w = 0.17;
    let mut top = Outline::new();
    top.at(-0.74, -0.3)
        .at(0.38, -0.3)
        .arc((0.38, -0.52, 0.22), FRAC_PI_2, deg(-250.));
    p.stroke(&top, w, c);
    p.line(&[(-0.74, 0.04), (0.72, 0.04)], w, c);
    let mut bottom = Outline::new();
    bottom
        .at(-0.58, 0.38)
        .at(0.42, 0.38)
        .arc((0.42, 0.58, 0.2), -FRAC_PI_2, deg(250.));
    p.stroke(&bottom, w, c);
}

fn wave(p: &Pen, c: Color) {
    for y in [-0.28, 0.3] {
        let mut o = Outline::new();
        for i in 0..=16 {
            let t = i as f32 / 16.;
            o.at(-0.82 + 1.64 * t, y - 0.15 * (t * TAU * 2.).sin());
        }
        p.stroke(&o, 0.18, c);
    }
}

fn leaf(p: &Pen, c: Color, d: Color) {
    let s = p.sub(0.06, -0.06, 1., deg(-45.));
    s.capsule((-0.9, 0.), (-1.12, 0.), 0.07, c);
    s.fill(&leaf_outline((-0.92, 0.), (1., 0.), 0.42), c);
    s.line(&[(-0.74, 0.), (0.7, 0.)], 0.11, d);
    for x in [-0.3, 0.16] {
        s.line(&[(x, 0.), (x + 0.24, -0.22)], 0.09, d);
        s.line(&[(x, 0.), (x + 0.24, 0.22)], 0.09, d);
    }
}

fn sprout(p: &Pen, c: Color, d: Color) {
    p.capsule((0., 0.8), (0., -0.12), 0.09, c);
    p.capsule((-0.5, 0.8), (0.5, 0.8), 0.08, c);
    p.fill(&leaf_outline((0., -0.08), (-0.84, -0.46), 0.24), c);
    p.fill(&leaf_outline((0., -0.2), (0.86, -0.8), 0.3), c);
    p.line(&[(-0.1, -0.12), (-0.66, -0.4)], 0.09, d);
    p.line(&[(0.1, -0.26), (0.66, -0.68)], 0.09, d);
}

fn tree(p: &Pen, c: Color) {
    p.rect(-0.13, 0.46, 0.26, 0.44, 0.04, c);
    for (top, bottom, half) in [(-0.3, 0.56, 0.82), (-0.62, 0.14, 0.64), (-0.92, -0.3, 0.46)] {
        p.fill(
            &Outline::corners(&[(0., top, 0.06), (half, bottom, 0.08), (-half, bottom, 0.08)]),
            c,
        );
    }
}

fn mountain(p: &Pen, c: Color, d: Color) {
    let (back, front) = ((0.42, -0.5), (-0.2, -0.84));
    let valley = meet(front, (0.6, 0.72), back, (-0.08, 0.72));
    let range = Outline::corners(&[
        (-0.92, 0.72, 0.06),
        (front.0, front.1, 0.08),
        (valley.0, valley.1, 0.03),
        (back.0, back.1, 0.08),
        (0.92, 0.72, 0.06),
    ]);
    p.fan((0.1, 0.55), &range, c);
    let cap = Outline::corners(&[
        (front.0, front.1, 0.08),
        (0.025, -0.4, 0.),
        (-0.06, -0.3, 0.02),
        (-0.18, -0.42, 0.02),
        (-0.3, -0.3, 0.02),
        (-0.403, -0.4, 0.),
    ]);
    p.fan((-0.19, -0.55), &cap, d);
}

fn coin(p: &Pen, c: Color, d: Color) {
    p.circle(0., 0., 0.86, c);
    p.ring(0., 0., 0.65, 0.12, d);
    p.fan(
        (0., 0.03),
        &star_outline(0., 0.03, 0.4, 0.5, (0.05, 0.02)),
        d,
    );
}

fn gem(p: &Pen, c: Color, d: Color) {
    let p = &p.sub(0., -0.05, 1., 0.);
    p.fill(
        &Outline::corners(&[
            (-0.48, -0.66, 0.06),
            (0.48, -0.66, 0.06),
            (0.92, -0.18, 0.06),
            (0., 0.86, 0.08),
            (-0.92, -0.18, 0.06),
        ]),
        c,
    );
    p.line(&[(-0.72, -0.18), (0.72, -0.18)], 0.1, d);
    p.line(
        &[
            (-0.24, -0.6),
            (-0.4, -0.18),
            (0., 0.66),
            (0.4, -0.18),
            (0.24, -0.6),
        ],
        0.1,
        d,
    );
}

fn bag(p: &Pen, c: Color, d: Color) {
    let p = &p.sub(0., -0.03, 1., 0.);
    p.circle(0., 0.28, 0.6, c);
    p.poly(
        &[(-0.24, -0.44), (0.24, -0.44), (0.52, 0.04), (-0.52, 0.04)],
        c,
    );
    p.fill(
        &Outline::corners(&[
            (-0.24, -0.44, 0.),
            (0.24, -0.44, 0.),
            (0.46, -0.8, 0.1),
            (-0.46, -0.8, 0.1),
        ]),
        c,
    );
    p.capsule((-0.26, -0.44), (0.26, -0.44), 0.075, d);
}

fn wooden_crate(p: &Pen, c: Color, d: Color) {
    p.rect(-0.82, -0.82, 1.64, 1.64, 0.12, c);
    p.frame((-0.62, -0.62, 1.24, 1.24), 0.04, 0.14, d);
    p.line(&[(-0.5, -0.5), (0.5, 0.5)], 0.14, d);
}

fn scroll(p: &Pen, c: Color, d: Color) {
    p.rect(-0.58, -0.68, 1.16, 1.36, 0.04, c);
    p.capsule((-0.76, -0.7), (0.76, -0.7), 0.16, c);
    p.capsule((-0.76, 0.7), (0.76, 0.7), 0.16, c);
    for y in [-0.26, 0., 0.26] {
        p.capsule((-0.34, y), (0.34, y), 0.065, d);
    }
}

fn book(p: &Pen, c: Color, d: Color) {
    p.rect(-0.66, -0.86, 1.36, 1.72, 0.12, c);
    p.capsule((-0.4, -0.74), (-0.4, 0.74), 0.07, d);
    p.capsule((-0.24, 0.6), (0.56, 0.6), 0.08, d);
    p.rect(-0.14, -0.58, 0.58, 0.2, 0.05, d);
}

fn bulb(p: &Pen, c: Color, d: Color) {
    let (cy, r, neck) = (-0.26, 0.6, (0.3, 0.38));
    let to_neck = vec2(neck.0, neck.1 - cy);
    let tangent = to_neck.to_angle() - (r / to_neck.length()).acos();
    let mut glass = Outline::new();
    glass
        .at(neck.0, neck.1)
        .arc((0., cy, r), tangent, -(TAU - (PI - 2. * tangent)))
        .at(-neck.0, neck.1);
    p.fill(&glass, c);
    p.rect(-0.3, 0.36, 0.6, 0.34, 0.06, c);
    p.rect(-0.15, 0.66, 0.3, 0.22, 0.08, c);
    p.line(&[(-0.22, 0.47), (0.22, 0.47)], 0.1, d);
    p.line(&[(-0.22, 0.6), (0.22, 0.6)], 0.1, d);
}

// Tools and travel.

fn hammer(p: &Pen, c: Color, d: Color) {
    let s = p.sub(0., 0., 1., deg(45.));
    s.capsule((0., -0.3), (0., 1.06), 0.13, c);
    s.rect(-0.62, -0.62, 1.24, 0.44, 0.08, c);
    s.capsule((0., 0.64), (0., 1.06), 0.13, d);
}

fn wrench(p: &Pen, c: Color) {
    p.capsule((-0.62, 0.62), (0.12, -0.12), 0.16, c);
    p.arc((0.34, -0.34, 0.28), deg(20.), deg(230.), 0.34, c);
}

fn pickaxe(p: &Pen, c: Color, d: Color) {
    let s = p.sub(0.06, -0.06, 1., deg(45.));
    s.capsule((0., -0.5), (0., 1.06), 0.12, c);
    s.crescent((0., 0.05, 0.75), (0., 0.707, 1.127), c);
    s.rect(-0.16, -0.62, 0.32, 0.28, 0.05, d);
}

fn anchor(p: &Pen, c: Color) {
    p.ring(0., -0.66, 0.15, 0.14, c);
    p.capsule((0., -0.46), (0., 0.74), 0.1, c);
    p.capsule((-0.38, -0.24), (0.38, -0.24), 0.09, c);
    let arms = (0., 0.12, 0.64);
    p.arc(arms, deg(22.), deg(136.), 0.18, c);
    arrowhead(p, arms, deg(22.), -1., (0.32, 0.23), c);
    arrowhead(p, arms, deg(158.), 1., (0.32, 0.23), c);
}

fn ship(p: &Pen, c: Color, d: Color) {
    p.fill(
        &Outline::corners(&[
            (-0.9, 0.34, 0.04),
            (0.9, 0.34, 0.04),
            (0.62, 0.82, 0.14),
            (-0.62, 0.82, 0.14),
        ]),
        c,
    );
    p.rect(-0.07, -0.9, 0.14, 1.3, 0.05, c);
    p.fill(
        &Outline::corners(&[(0.16, -0.84, 0.04), (0.76, 0.2, 0.05), (0.16, 0.2, 0.04)]),
        c,
    );
    p.fill(
        &Outline::corners(&[(-0.16, -0.6, 0.04), (-0.16, 0.2, 0.04), (-0.64, 0.2, 0.05)]),
        c,
    );
    p.line(&[(-0.52, 0.58), (0.52, 0.58)], 0.1, d);
}

fn rocket(p: &Pen, c: Color, d: Color) {
    let s = p.sub(-0.02, 0.02, 0.98, deg(45.));
    let (w, top, bottom, tip): (f32, f32, f32, f32) = (0.3, -0.3, 0.5, -1.1);
    // Ogive nose: each side is an arc tangent to the body, meeting at the tip.
    let radius = (w * w + (top - tip) * (top - tip)) / (2. * w);
    let reach = (top - tip).atan2(radius - w);
    let mut body = Outline::new();
    body.round(-w, bottom, 0.06)
        .round(w, bottom, 0.06)
        .arc((w - radius, top, radius), 0., -reach)
        .arc((radius - w, top, radius), -(PI - reach), -reach);
    s.fill(&body, c);
    for side in [-1., 1.] {
        s.fill(
            &Outline::corners(&[
                (side * 0.26, 0., 0.),
                (side * 0.58, 0.38, 0.06),
                (side * 0.58, 0.66, 0.06),
                (side * 0.26, 0.48, 0.),
            ]),
            c,
        );
    }
    s.fill(&teardrop((0., 0.72, 0.16), (0., 1.08), 0.03), c);
    s.circle(0., -0.36, 0.15, d);
}

fn planet(p: &Pen, c: Color, d: Color) {
    let (a, b, tilt) = (0.88, 0.3, Vec2::from_angle(deg(-20.)));
    let ring = |from: f32, to: f32| {
        let mut o = Outline::new();
        for i in 0..=14 {
            let t = from + (to - from) * i as f32 / 14.;
            let q = vec2(a * t.cos(), b * t.sin()).rotate(tilt);
            o.at(q.x, q.y);
        }
        o
    };
    p.stroke(&ring(PI, TAU), 0.13, c);
    p.circle(0., 0., 0.52, c);
    p.stroke(&ring(deg(52.), deg(128.)), 0.27, d);
    p.stroke(&ring(0., PI), 0.13, c);
}

fn train(p: &Pen, c: Color, d: Color) {
    p.rect(-0.62, -0.88, 1.24, 1.38, 0.26, c);
    p.rect(-0.42, -0.64, 0.84, 0.46, 0.08, d);
    p.circle(-0.3, 0.2, 0.1, d);
    p.circle(0.3, 0.2, 0.1, d);
    p.capsule((-0.3, 0.5), (-0.54, 0.82), 0.09, c);
    p.capsule((0.3, 0.5), (0.54, 0.82), 0.09, c);
}

// People and creatures.

fn person(p: &Pen, c: Color) {
    p.circle(0., -0.46, 0.36, c);
    p.fill(
        &Outline::corners(&[
            (-0.74, 0.88, 0.08),
            (-0.74, 0.04, 0.4),
            (0.74, 0.04, 0.4),
            (0.74, 0.88, 0.08),
        ]),
        c,
    );
}

fn people(p: &Pen, c: Color, d: Color) {
    let p = &p.sub(0., -0.04, 1., 0.);
    person(&p.sub(0.3, -0.12, 0.76, 0.), d);
    person(&p.sub(-0.18, 0.08, 0.86, 0.), c);
}

fn paw(p: &Pen, c: Color) {
    p.fill(
        &Outline::corners(&[(-0.54, 0.74, 0.28), (0., 0.06, 0.3), (0.54, 0.74, 0.28)]),
        c,
    );
    for (x, y, tilt) in [
        (-0.68, -0.04, -20.),
        (-0.26, -0.46, -8.),
        (0.26, -0.46, 8.),
        (0.68, -0.04, 20.),
    ] {
        p.fill(&ellipse(x, y, (0.17, 0.23), deg(tilt)), c);
    }
}

fn bone(p: &Pen, c: Color) {
    p.capsule((-0.48, 0.48), (0.48, -0.48), 0.17, c);
    for (x, y) in [(0.65, -0.35), (0.35, -0.65), (-0.65, 0.35), (-0.35, 0.65)] {
        p.circle(x, y, 0.22, c);
    }
}

fn potion(p: &Pen, c: Color, d: Color) {
    p.circle(0., 0.26, 0.62, c);
    p.rect(-0.21, -0.68, 0.42, 0.52, 0.04, c);
    p.rect(-0.31, -0.88, 0.62, 0.24, 0.08, c);
    let (cy, r, surface): (f32, f32, f32) = (0.26, 0.46, 0.14);
    let s = ((surface - cy) / r).asin();
    let mut liquid = Outline::new();
    liquid.arc((0., cy, r), s, PI - 2. * s);
    p.fill(&liquid, d);
    p.circle(0.16, 0.42, 0.08, c);
    p.circle(-0.12, 0.56, 0.06, c);
}

fn fish(p: &Pen, c: Color, d: Color) {
    p.fill(&teardrop((0.28, 0., 0.58), (-0.46, 0.), 0.04), c);
    let tail = Outline::corners(&[
        (-0.34, 0., 0.),
        (-0.88, -0.52, 0.08),
        (-0.7, 0., 0.04),
        (-0.88, 0.52, 0.08),
    ]);
    p.fan((-0.58, 0.), &tail, c);
    p.circle(0.52, -0.14, 0.11, d);
    p.arc((-0.22, 0., 0.46), deg(-38.), deg(76.), 0.1, d);
}

fn music(p: &Pen, c: Color) {
    p.fill(&ellipse(-0.3, 0.56, (0.33, 0.25), deg(-24.)), c);
    p.rect(-0.04, -0.86, 0.18, 1.42, 0.05, c);
    p.line(
        &[(0.08, -0.8), (0.36, -0.6), (0.58, -0.32), (0.52, -0.02)],
        0.18,
        c,
    );
}

fn chat(p: &Pen, c: Color, d: Color) {
    p.rect(-0.88, -0.78, 1.76, 1.2, 0.34, c);
    p.fill(
        &Outline::corners(&[(-0.52, 0.26, 0.), (-0.08, 0.26, 0.), (-0.62, 0.84, 0.06)]),
        c,
    );
    for x in [-0.4, 0., 0.4] {
        p.circle(x, -0.18, 0.11, d);
    }
}

fn pot(p: &Pen, c: Color, d: Color) {
    p.capsule((-0.8, 0.16), (-0.6, 0.16), 0.09, c);
    p.capsule((0.6, 0.16), (0.8, 0.16), 0.09, c);
    p.rect(-0.66, -0.12, 1.32, 0.98, 0.22, c);
    p.rect(-0.74, -0.28, 1.48, 0.2, 0.08, c);
    p.sector((0., -0.28, 0.5), PI, PI, c);
    p.circle(0., -0.78, 0.11, c);
    p.line(&[(-0.46, -0.28), (0.46, -0.28)], 0.1, d);
}

fn thermometer(p: &Pen, c: Color, d: Color) {
    let x = -0.16;
    p.capsule((x, -0.7), (x, 0.3), 0.21, c);
    p.circle(x, 0.52, 0.35, c);
    p.capsule((x, -0.3), (x, 0.4), 0.085, d);
    p.circle(x, 0.52, 0.2, d);
    for (y, len) in [(-0.56, 0.28), (-0.3, 0.2), (-0.04, 0.28)] {
        p.capsule((x + 0.4, y), (x + 0.4 + len, y), 0.07, c);
    }
}

fn fuel(p: &Pen, c: Color, d: Color) {
    let p = &p.sub(0.08, 0., 1., 0.);
    p.frame((-0.04, -0.82, 0.6, 0.4), 0.12, 0.15, c);
    p.capsule((-0.5, -0.36), (-0.68, -0.56), 0.15, c);
    p.fill(
        &Outline::corners(&[
            (-0.68, -0.08, 0.06),
            (-0.24, -0.52, 0.06),
            (0.64, -0.52, 0.12),
            (0.64, 0.88, 0.12),
            (-0.68, 0.88, 0.12),
        ]),
        c,
    );
    p.line(&[(-0.36, 0.0), (0.32, 0.64)], 0.14, d);
    p.line(&[(0.32, 0.0), (-0.36, 0.64)], 0.14, d);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shapes::tests::capture;

    /// Every polygon handed to `fill_polygon` must be convex: a fan from its
    /// centroid only covers convex outlines exactly.
    pub(super) fn assert_convex(points: &[Vec2]) {
        let n = points.len();
        let mut sign = 0.;
        for i in 0..n {
            let (a, b, e) = (points[i], points[(i + 1) % n], points[(i + 2) % n]);
            let (u, v) = (b - a, e - b);
            let turn = u.perp_dot(v) / (u.length() * v.length()).max(1e-12);
            if turn.abs() < 2e-3 {
                continue;
            }
            if sign == 0. {
                sign = turn.signum();
            }
            assert!(turn * sign > 0., "concave corner at {b:?} in {points:?}");
        }
    }

    /// A ladder outline must split into convex quads between its rungs.
    pub(super) fn assert_ladder(points: &[Vec2]) {
        let n = points.len();
        assert!(n >= 4 && n.is_multiple_of(2), "ladder of {n} points");
        for k in 0..n / 2 {
            let quad = [
                points[k],
                points[k + 1],
                points[n - k - 1],
                points[(n - k) % n],
            ];
            let mut corners = [Vec2::ZERO; 4];
            let mut len = 0;
            for p in quad {
                if len == 0 || (p - corners[len - 1]).length() > 1e-4 {
                    corners[len] = p;
                    len += 1;
                }
            }
            if len > 3 && (corners[len - 1] - corners[0]).length() <= 1e-4 {
                len -= 1;
            }
            assert_convex(&corners[..len]);
        }
    }

    /// A fanned outline must face its hub along every edge.
    pub(super) fn assert_star_shaped(points: &[Vec2], hub: Vec2) {
        let n = points.len();
        let area: f32 = (0..n)
            .map(|i| points[i].perp_dot(points[(i + 1) % n]))
            .sum();
        for i in 0..n {
            let (a, b) = (points[i], points[(i + 1) % n]);
            let side =
                (b - a).perp_dot(hub - a) / ((b - a).length() * (hub - a).length()).max(1e-12);
            assert!(
                side * area.signum() > -1e-3,
                "hub {hub:?} is behind edge {a:?}-{b:?}"
            );
        }
    }

    #[test]
    fn names_round_trip_and_are_unique() {
        let mut names: Vec<_> = Icon::ALL.iter().map(|i| i.name()).collect();
        assert!(names.len() >= 80);
        for icon in Icon::ALL {
            assert_eq!(Icon::from_name(icon.name()), Some(*icon));
        }
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), Icon::ALL.len());
        assert_eq!(Icon::from_name("not-an-icon"), None);
    }

    #[test]
    fn every_icon_draws_finite_geometry_inside_its_box() {
        for &icon in Icon::ALL {
            let v = capture(200, 200, || {
                draw(icon, vec2(100., 100.), 64., Color::new(1., 1., 1., 1.))
            });
            assert!(!v.is_empty(), "{} drew nothing", icon.name());
            assert!(v.len() <= 12_000, "{}: {} vertices", icon.name(), v.len());
            for vertex in &v {
                assert!(vertex.iter().all(|x| x.is_finite()), "{}", icon.name());
                // Clip space to pixels; allow the fringe and round caps.
                let x = (vertex[0] + 1.) * 100.;
                let y = (1. - vertex[1]) * 100.;
                assert!(
                    (64. ..=136.).contains(&x) && (64. ..=136.).contains(&y),
                    "{} leaves its box at ({x}, {y})",
                    icon.name()
                );
            }
        }
    }

    #[test]
    fn invalid_input_draws_nothing() {
        let v = capture(100, 100, || {
            draw(
                Icon::Star,
                vec2(f32::NAN, 0.),
                20.,
                Color::new(1., 1., 1., 1.),
            );
            draw(Icon::Star, vec2(10., 10.), 0., Color::new(1., 1., 1., 1.));
            draw(Icon::Star, vec2(10., 10.), -4., Color::new(1., 1., 1., 1.));
            badge(
                Icon::Star,
                vec2(10., 10.),
                f32::INFINITY,
                Color::new(0., 0., 0., 1.),
                Color::new(1., 1., 1., 1.),
            );
        });
        assert!(v.is_empty());
    }

    #[test]
    fn every_icon_stays_in_its_box_at_hud_and_menu_sizes() {
        for &icon in Icon::ALL {
            for size in [12., 24., 128.] {
                let v = capture(400, 400, || {
                    let style = IconStyle::new(Color::new(0.9, 0.6, 0.2, 0.7))
                        .shadow(Color::new(0., 0., 0., 0.3));
                    draw_styled(icon, vec2(200., 200.), size, style);
                });
                assert!(!v.is_empty(), "{} drew nothing at {size}", icon.name());
                // Geometry stays within 95% of the half size; allow the
                // shadow's drop and the fringe (up to two pixels at mitres).
                let (reach, drop) = (size * 0.5 * 0.95 + 2.5, (size * 0.06).max(1.));
                for p in &v {
                    let (x, y) = ((p[0] + 1.) * 200. - 200., (1. - p[1]) * 200. - 200.);
                    assert!(
                        x.abs() <= reach && y >= -reach && y <= reach + drop,
                        "{} at {size} px reaches ({x}, {y})",
                        icon.name()
                    );
                }
            }
            let v = capture(400, 400, || {
                badge(
                    icon,
                    vec2(200., 200.),
                    40.,
                    Color::new(0.1, 0.2, 0.3, 1.),
                    Color::new(1., 1., 1., 1.),
                )
            });
            assert!(
                v.iter().flatten().all(|x| x.is_finite()),
                "{} badge",
                icon.name()
            );
        }
    }

    /// Detail-free icons drawn translucent: no pixel is covered by two opaque
    /// interior triangles, so a faded heart, star or arrow stays even.
    #[test]
    fn single_piece_icons_cover_each_pixel_once() {
        let single = [
            Icon::Play,
            Icon::Pause,
            Icon::Check,
            Icon::Cross,
            Icon::Plus,
            Icon::Minus,
            Icon::ArrowUp,
            Icon::ArrowDown,
            Icon::ArrowLeft,
            Icon::ArrowRight,
            Icon::Pointer,
            Icon::Target,
            Icon::Star,
            Icon::Heart,
            Icon::Bolt,
            Icon::Sun,
            Icon::Moon,
            Icon::Cloud,
        ];
        for icon in single {
            let alpha = 0.5;
            let v = capture(200, 200, || {
                draw(icon, vec2(100., 100.), 64., Color::new(1., 1., 1., alpha))
            });
            let mut hits = vec![0u8; 200 * 200];
            for t in v.chunks_exact(3) {
                if !t.iter().all(|p| (p[7] - alpha).abs() < 1e-6) {
                    continue; // fringe
                }
                let q = [0, 1, 2].map(|k| vec2((t[k][0] + 1.) * 100., (1. - t[k][1]) * 100.));
                let side = |a: Vec2, b: Vec2, p: Vec2| (b - a).perp_dot(p - a);
                let (lo, hi) = (q[0].min(q[1]).min(q[2]), q[0].max(q[1]).max(q[2]));
                for y in lo.y.floor().max(0.) as usize..(hi.y.ceil() as usize).min(200) {
                    for x in lo.x.floor().max(0.) as usize..(hi.x.ceil() as usize).min(200) {
                        // Off-grid samples avoid landing on shared edges.
                        let p = vec2(x as f32 + 0.37, y as f32 + 0.61);
                        let s = [
                            side(q[0], q[1], p),
                            side(q[1], q[2], p),
                            side(q[2], q[0], p),
                        ];
                        if s.iter().all(|&s| s > 0.) || s.iter().all(|&s| s < 0.) {
                            hits[y * 200 + x] += 1;
                        }
                    }
                }
            }
            let twice = hits.iter().filter(|&&h| h > 1).count();
            assert_eq!(twice, 0, "{} covers {twice} pixels twice", icon.name());
        }
    }

    #[test]
    fn rounded_corners_are_tangent_fillets() {
        let square = Outline::corners(&[
            (-1., -1., 0.5),
            (1., -1., 0.5),
            (1., 1., 0.5),
            (-1., 1., 0.5),
        ]);
        let (mut lo, mut hi, mut nearest) = (Vec2::splat(9.), Vec2::splat(-9.), f32::MAX);
        square.trace(|x, y| {
            let p = vec2(x, y);
            (lo, hi) = (lo.min(p), hi.max(p));
            nearest = nearest.min((p - vec2(1., 1.)).length());
            // Every point lies on the rounded square: an edge or a fillet arc.
            let corner = p.abs() - Vec2::splat(0.5);
            let on_arc = corner.x >= 0. && corner.y >= 0.;
            assert!(!on_arc || (corner.length() - 0.5).abs() < 1e-4, "{p:?}");
        });
        assert!((lo - Vec2::splat(-1.)).length() < 1e-5 && (hi - Vec2::ONE).length() < 1e-5);
        // The arc passes (√2 - 1) / 2 from the sharp corner it replaces.
        assert!((0.2071..0.23).contains(&nearest), "{nearest}");
        // A closing point that repeats the first is dropped.
        let mut count = 0;
        Outline::of(&[(0., 0.), (1., 0.), (1., 1.), (0., 0.)]).trace(|_, _| count += 1);
        assert_eq!(count, 3);
    }

    #[test]
    #[should_panic(expected = "concave corner")]
    fn convexity_check_rejects_concave_outlines() {
        let dart = [vec2(0., 0.), vec2(2., 1.), vec2(0., 2.), vec2(0.6, 1.)];
        assert_convex(&dart);
    }
}
