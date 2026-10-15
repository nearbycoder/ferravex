//! Small allocation-free 2D collision queries for game logic, independent of rendering.
use ferravex_math::Vec2;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb2<V = Vec2> {
    pub min: V,
    pub max: V,
}

impl<V: Copy + Into<[f32; 2]> + From<[f32; 2]>> Aabb2<V> {
    pub fn from_min_size(min: V, size: V) -> Self {
        let a: [f32; 2] = min.into();
        let b: [f32; 2] = size.into();
        Self {
            min,
            max: [a[0] + b[0], a[1] + b[1]].into(),
        }
    }

    fn owned(self) -> Aabb2 {
        Aabb2 {
            min: Vec2::from(self.min.into()),
            max: Vec2::from(self.max.into()),
        }
    }
    fn valid(self) -> bool {
        let a = self.owned();
        a.min.is_finite() && a.max.is_finite() && a.max.cmpge(a.min).all()
    }

    /// Positive-area overlap: touching edges alone do not count as penetration.
    /// This distinction matters when a character rests on a platform.
    pub fn overlaps(self, other: Self) -> bool {
        let (this, other) = (self.owned(), other.owned());
        this.valid()
            && other.valid()
            && this.min.cmplt(this.max).all()
            && other.min.cmplt(other.max).all()
            && this.min.cmplt(other.max).all()
            && this.max.cmpgt(other.min).all()
    }

    /// First hit along a finite segment, as a fraction in [0, 1].
    /// Starts inside/on the boundary return zero; touching boundaries count as hits.
    pub fn cast_segment(self, start: V, end: V) -> Option<f32> {
        let this = self.owned();
        let (start, end) = (Vec2::from(start.into()), Vec2::from(end.into()));
        if !this.valid() || !start.is_finite() || !end.is_finite() {
            return None;
        }
        let delta = end - start;
        if !delta.is_finite() {
            return None;
        }
        let mut near: f32 = 0.0;
        let mut far: f32 = 1.0;
        for axis in 0..2 {
            if delta[axis] == 0.0 {
                if start[axis] < this.min[axis] || start[axis] > this.max[axis] {
                    return None;
                }
            } else {
                let a = (this.min[axis] - start[axis]) / delta[axis];
                let b = (this.max[axis] - start[axis]) / delta[axis];
                near = near.max(a.min(b));
                far = far.min(a.max(b));
                if near > far {
                    return None;
                }
            }
        }
        Some(near)
    }
}

/// Earliest segment/circle hit in [0, 1], including a stationary point inside.
/// Useful for fast projectiles that cross a target between simulation ticks.
pub fn segment_circle<V: Copy + Into<[f32; 2]>>(
    start: V,
    end: V,
    center: V,
    radius: f32,
) -> Option<f32> {
    let (start, end, center) = (
        Vec2::from(start.into()),
        Vec2::from(end.into()),
        Vec2::from(center.into()),
    );
    if !start.is_finite()
        || !end.is_finite()
        || !center.is_finite()
        || !radius.is_finite()
        || radius < 0.0
    {
        return None;
    }
    let delta = end - start;
    let relative = start - center;
    let c = relative.length_squared() - radius * radius;
    if c <= 0.0 {
        return Some(0.0);
    }
    let a = delta.length_squared();
    if a == 0.0 || !a.is_finite() || !c.is_finite() {
        return None;
    }
    let b = relative.dot(delta);
    let discriminant = b * b - a * c;
    if discriminant < 0.0 || !discriminant.is_finite() {
        return None;
    }
    let t = (-b - discriminant.sqrt()) / a;
    (0.0..=1.0).contains(&t).then_some(t)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resting_edges_do_not_push_a_platformer_sideways() {
        let floor = Aabb2::from_min_size(Vec2::new(0.0, 10.0), Vec2::new(100.0, 10.0));
        assert!(!floor.overlaps(Aabb2::from_min_size(Vec2::new(4.0, 0.0), Vec2::splat(10.0))));
        assert!(floor.overlaps(Aabb2::from_min_size(Vec2::new(4.0, 0.1), Vec2::splat(10.0))));
        assert!(!floor.overlaps(Aabb2::from_min_size(Vec2::new(4.0, 11.0), Vec2::ZERO)));
    }
    #[test]
    fn segment_queries_cover_tunneling_tangency_and_stationary_input() {
        let box_ = Aabb2::from_min_size(Vec2::splat(5.0), Vec2::splat(2.0));
        assert_eq!(
            box_.cast_segment(Vec2::new(0.0, 6.0), Vec2::new(10.0, 6.0)),
            Some(0.5)
        );
        assert_eq!(
            box_.cast_segment(Vec2::splat(6.0), Vec2::splat(6.0)),
            Some(0.0)
        );
        assert_eq!(box_.cast_segment(Vec2::ZERO, Vec2::X), None);
        assert_eq!(
            segment_circle(
                Vec2::ZERO,
                Vec2::new(100.0, 0.0),
                Vec2::new(50.0, 0.0),
                10.0
            ),
            Some(0.4)
        );
        assert_eq!(
            segment_circle(
                Vec2::ZERO,
                Vec2::new(100.0, 0.0),
                Vec2::new(50.0, 10.0),
                10.0
            ),
            Some(0.5)
        );
        assert_eq!(
            segment_circle(Vec2::ZERO, Vec2::ZERO, Vec2::ZERO, 1.0),
            Some(0.0)
        );
        assert_eq!(segment_circle(Vec2::ZERO, Vec2::ZERO, Vec2::X, 0.1), None);
        assert_eq!(segment_circle(Vec2::ZERO, Vec2::X, Vec2::X, f32::NAN), None);
    }
}
