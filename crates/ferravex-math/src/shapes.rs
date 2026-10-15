use crate::{Vec2, Vec4, vec2};

/// Axis-aligned 2D rectangle. Width and height should be nonnegative.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}
impl Rect {
    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0, 0.0);
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }
    pub const fn point(self) -> Vec2 {
        vec2(self.x, self.y)
    }
    pub const fn size(self) -> Vec2 {
        vec2(self.w, self.h)
    }
    pub fn center(self) -> Vec2 {
        self.point() + self.size() * 0.5
    }
    pub const fn left(self) -> f32 {
        self.x
    }
    pub const fn right(self) -> f32 {
        self.x + self.w
    }
    pub const fn top(self) -> f32 {
        self.y
    }
    pub const fn bottom(self) -> f32 {
        self.y + self.h
    }
    pub fn is_finite(self) -> bool {
        [self.x, self.y, self.w, self.h]
            .into_iter()
            .all(f32::is_finite)
    }
    pub fn contains(self, p: Vec2) -> bool {
        self.is_finite()
            && self.w >= 0.0
            && self.h >= 0.0
            && p.x >= self.x
            && p.x <= self.right()
            && p.y >= self.y
            && p.y <= self.bottom()
    }
    /// Positive-area overlap. Touching edges are not solid penetration.
    pub fn overlaps(self, other: &Self) -> bool {
        self.w > 0.0
            && self.h > 0.0
            && other.w > 0.0
            && other.h > 0.0
            && self.x < other.right()
            && self.right() > other.x
            && self.y < other.bottom()
            && self.bottom() > other.y
    }
    pub fn intersect(self, other: Self) -> Option<Self> {
        if !self.overlaps(&other) {
            return None;
        }
        let point = self.point().max(other.point());
        let end = vec2(self.right(), self.bottom()).min(vec2(other.right(), other.bottom()));
        Some(Self::new(
            point.x,
            point.y,
            end.x - point.x,
            end.y - point.y,
        ))
    }
    pub fn combine_with(self, other: Self) -> Self {
        let p = self.point().min(other.point());
        let end = vec2(self.right(), self.bottom()).max(vec2(other.right(), other.bottom()));
        Self::new(p.x, p.y, end.x - p.x, end.y - p.y)
    }
    pub fn move_to(&mut self, p: Vec2) {
        self.x = p.x;
        self.y = p.y;
    }
    pub fn offset(self, delta: Vec2) -> Self {
        Self::new(self.x + delta.x, self.y + delta.y, self.w, self.h)
    }
    pub fn normalized(self) -> Self {
        Self::new(
            self.x.min(self.right()),
            self.y.min(self.bottom()),
            self.w.abs(),
            self.h.abs(),
        )
    }
}

/// Unpremultiplied RGBA floating-point channels. No implicit gamma conversion.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}
impl Color {
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }
    pub const fn from_rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self::new(
            r as f32 / 255.0,
            g as f32 / 255.0,
            b as f32 / 255.0,
            a as f32 / 255.0,
        )
    }
    /// Opaque 0xRRGGBB color.
    pub const fn from_hex(rgb: u32) -> Self {
        Self::from_rgba((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255)
    }
    pub const fn to_vec(self) -> Vec4 {
        Vec4::new(self.r, self.g, self.b, self.a)
    }
    pub const fn to_array(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }
    pub fn to_rgba(self) -> [u8; 4] {
        self.to_array()
            .map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8)
    }
    pub const fn with_alpha(self, alpha: f32) -> Self {
        Self { a: alpha, ..self }
    }
    pub fn lerp(self, other: Self, t: f32) -> Self {
        Self::from(self.to_vec().lerp(other.to_vec(), t))
    }
    pub fn premultiplied(self) -> Self {
        Self::new(self.r * self.a, self.g * self.a, self.b * self.a, self.a)
    }
}
impl From<Vec4> for Color {
    fn from(v: Vec4) -> Self {
        Self::new(v.x, v.y, v.z, v.w)
    }
}
impl From<[f32; 4]> for Color {
    fn from(v: [f32; 4]) -> Self {
        Self::new(v[0], v[1], v[2], v[3])
    }
}
impl From<Color> for [f32; 4] {
    fn from(v: Color) -> Self {
        v.to_array()
    }
}
impl From<Color> for Vec4 {
    fn from(v: Color) -> Self {
        v.to_vec()
    }
}
