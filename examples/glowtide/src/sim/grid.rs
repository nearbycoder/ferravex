//! Dense, allocation-free spatial hash for hundreds of foes.
//!
//! The grid is a fixed window of `COLS × ROWS` cells re-centred on the player
//! every tick and rebuilt in O(n) as intrusive linked lists (`heads`/`next`).
//! Foes outside the window clamp into border cells, so queries remain correct
//! (just slower) for stragglers. All storage is allocated once in [`Grid::new`].
use ferravex_render::prelude::{Vec2, vec2};

pub const CELL: f32 = 40.;
pub const COLS: usize = 96;
pub const ROWS: usize = 96;
const EMPTY: u16 = u16::MAX;

pub struct Grid {
    origin: Vec2,
    heads: Box<[u16]>,
    next: Box<[u16]>,
}

impl Grid {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity < EMPTY as usize, "grid indices are u16");
        Self {
            origin: Vec2::ZERO,
            heads: vec![EMPTY; COLS * ROWS].into_boxed_slice(),
            next: vec![EMPTY; capacity].into_boxed_slice(),
        }
    }

    #[inline]
    fn cell_of(&self, p: Vec2) -> (usize, usize) {
        let local = (p - self.origin) * (1. / CELL);
        let cx = (local.x.max(0.) as usize).min(COLS - 1);
        let cy = (local.y.max(0.) as usize).min(ROWS - 1);
        (cx, cy)
    }

    /// Rebuild around `center` from positions; index `i` refers to `points[i]`.
    pub fn rebuild(&mut self, center: Vec2, points: impl Iterator<Item = Vec2>) {
        self.origin = center - vec2(COLS as f32, ROWS as f32) * (CELL * 0.5);
        self.heads.fill(EMPTY);
        for (i, p) in points.enumerate() {
            if i >= self.next.len() {
                break;
            }
            let (cx, cy) = self.cell_of(p);
            let cell = cy * COLS + cx;
            self.next[i] = self.heads[cell];
            self.heads[cell] = i as u16;
        }
    }

    /// Visit every stored index whose cell overlaps the circle's bounding box.
    /// Callers still test exact distances.
    #[inline]
    pub fn visit(&self, center: Vec2, radius: f32, mut f: impl FnMut(usize)) {
        let (x0, y0) = self.cell_of(center - Vec2::splat(radius));
        let (x1, y1) = self.cell_of(center + Vec2::splat(radius));
        for cy in y0..=y1 {
            let row = cy * COLS;
            for cx in x0..=x1 {
                let mut i = self.heads[row + cx];
                while i != EMPTY {
                    f(i as usize);
                    i = self.next[i as usize];
                }
            }
        }
    }

    /// Like [`Self::visit`] but stops once `f` returns false.
    #[inline]
    pub fn visit_until(&self, center: Vec2, radius: f32, mut f: impl FnMut(usize) -> bool) {
        let (x0, y0) = self.cell_of(center - Vec2::splat(radius));
        let (x1, y1) = self.cell_of(center + Vec2::splat(radius));
        for cy in y0..=y1 {
            let row = cy * COLS;
            for cx in x0..=x1 {
                let mut i = self.heads[row + cx];
                while i != EMPTY {
                    if !f(i as usize) {
                        return;
                    }
                    i = self.next[i as usize];
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visits_neighbors_and_clamps_far_points() {
        let mut grid = Grid::new(8);
        let points = [
            vec2(0., 0.),
            vec2(30., 0.),
            vec2(500., 500.),
            vec2(1e5, -1e5),
        ];
        grid.rebuild(Vec2::ZERO, points.iter().copied());
        let mut seen = Vec::new();
        grid.visit(Vec2::ZERO, 45., |i| seen.push(i));
        seen.sort();
        assert_eq!(seen, vec![0, 1]);
        // Far points land in a border cell and are still found when queried there.
        let mut far = Vec::new();
        grid.visit(vec2(1e5, -1e5), 10., |i| far.push(i));
        assert!(far.contains(&3));
        let mut first = None;
        grid.visit_until(Vec2::ZERO, 45., |i| {
            first = Some(i);
            false
        });
        assert!(first.is_some());
    }
}
