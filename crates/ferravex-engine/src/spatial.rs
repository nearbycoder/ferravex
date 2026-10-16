//! Bounded point buckets for engine interest, selection and proximity queries.
//! IDs are host-owned u64 values. Queries visit at most 4096 cells; an oversized
//! query returns an error instead of partial results. This is not a physics broadphase.
use std::collections::{BTreeMap, BTreeSet};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpatialError {
    Invalid,
    Capacity,
    QueryTooLarge,
}
type Cell = (i32, i32);
pub struct SpatialGrid {
    cell_size: f64,
    capacity: usize,
    points: BTreeMap<u64, ([f64; 2], Cell)>,
    cells: BTreeMap<Cell, BTreeSet<u64>>,
}
impl SpatialGrid {
    pub fn new(cell_size: f64, capacity: usize) -> Result<Self, SpatialError> {
        if !cell_size.is_finite() || cell_size <= 0. {
            return Err(SpatialError::Invalid);
        }
        Ok(Self {
            cell_size,
            capacity,
            points: BTreeMap::new(),
            cells: BTreeMap::new(),
        })
    }
    fn cell(&self, point: [f64; 2]) -> Result<Cell, SpatialError> {
        let axis = |x: f64| {
            let v = (x / self.cell_size).floor();
            if !x.is_finite()
                || !v.is_finite()
                || v < f64::from(i32::MIN)
                || v > f64::from(i32::MAX)
            {
                Err(SpatialError::Invalid)
            } else {
                Ok(v as i32)
            }
        };
        Ok((axis(point[0])?, axis(point[1])?))
    }
    /// Move or insert atomically. Invalid positions preserve any previous location.
    pub fn insert(&mut self, id: u64, point: [f64; 2]) -> Result<(), SpatialError> {
        let cell = self.cell(point)?;
        if !self.points.contains_key(&id) && self.points.len() >= self.capacity {
            return Err(SpatialError::Capacity);
        }
        self.remove(id);
        self.points.insert(id, (point, cell));
        self.cells.entry(cell).or_default().insert(id);
        Ok(())
    }
    pub fn remove(&mut self, id: u64) -> bool {
        let Some((_, cell)) = self.points.remove(&id) else {
            return false;
        };
        let bucket = self.cells.get_mut(&cell).unwrap();
        bucket.remove(&id);
        if bucket.is_empty() {
            self.cells.remove(&cell);
        }
        true
    }
    /// Inclusive axis-aligned point query with stable ascending IDs and exact filtering.
    pub fn query(&self, min: [f64; 2], max: [f64; 2]) -> Result<Vec<u64>, SpatialError> {
        if min[0] > max[0] || min[1] > max[1] {
            return Err(SpatialError::Invalid);
        }
        let a = self.cell(min)?;
        let b = self.cell(max)?;
        let width = i64::from(b.0) - i64::from(a.0) + 1;
        let height = i64::from(b.1) - i64::from(a.1) + 1;
        if width > 4096 || height > 4096 || width * height > 4096 {
            return Err(SpatialError::QueryTooLarge);
        }
        let mut out = Vec::new();
        for x in a.0..=b.0 {
            for y in a.1..=b.1 {
                if let Some(bucket) = self.cells.get(&(x, y)) {
                    for id in bucket {
                        let p = self.points[id].0;
                        if p[0] >= min[0] && p[0] <= max[0] && p[1] >= min[1] && p[1] <= max[1] {
                            out.push(*id);
                        }
                    }
                }
            }
        }
        out.sort_unstable();
        Ok(out)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn negative_cells_move_exact_filter_and_limits() {
        let mut g = SpatialGrid::new(1., 3).unwrap();
        g.insert(3, [-0.5, 0.]).unwrap();
        g.insert(1, [0.9, 0.9]).unwrap();
        g.insert(2, [0.1, 0.1]).unwrap();
        assert_eq!(g.query([-1., -1.], [0.2, 0.2]).unwrap(), [2, 3]);
        assert_eq!(g.insert(4, [0., 0.]), Err(SpatialError::Capacity));
        assert_eq!(g.insert(2, [f64::NAN, 0.]), Err(SpatialError::Invalid));
        assert_eq!(g.query([0., 0.], [0.2, 0.2]).unwrap(), [2]);
        g.insert(2, [2., 2.]).unwrap();
        assert_eq!(g.query([0., 0.], [1., 1.]).unwrap(), [1]);
        assert!(g.remove(3));
        assert!(!g.remove(3));
        assert_eq!(
            g.query([-100., -100.], [100., 100.]),
            Err(SpatialError::QueryTooLarge)
        );
        assert!(g.query([1., 1.], [0., 0.]).is_err());
    }
}
