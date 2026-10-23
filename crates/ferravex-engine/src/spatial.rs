//! Bounded point buckets for engine interest, selection and proximity queries.
//! IDs are host-owned u64 values. Queries visit at most 4096 cells; an oversized
//! query returns an error instead of partial results. This is not a physics broadphase.
//! Use [`SpatialGrid::query_into`](crate::spatial::SpatialGrid::query_into) to reuse caller storage and report omitted matches.
//! [`SpatialGrid::query_radius_into`](crate::spatial::SpatialGrid::query_radius_into) and [`SpatialGrid::query_nearest_into`](crate::spatial::SpatialGrid::query_nearest_into)
//! return stored positions ranked by squared distance, then ID, without allocating.
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpatialError {
    /// Invalid coordinates/bounds/radius, or a radius whose square overflows.
    Invalid,
    /// Inserting a new ID would exceed the configured point capacity.
    Capacity,
    /// The query's bounding rectangle spans more than 4096 cells.
    QueryTooLarge,
}

/// Counts from a successful caller-buffer query on [`SpatialGrid`].
///
/// An exactly full buffer is complete when `written == total`; only omitted
/// matches count as truncation. A zero-length buffer still reports `total`.
///
/// ```
/// use ferravex_engine::spatial::SpatialGrid;
///
/// let mut grid = SpatialGrid::new(1.0, 2)?;
/// grid.insert(7, [0.0, 0.0])?;
/// let report = grid.query_into([0.0, 0.0], [1.0, 1.0], &mut [])?;
/// assert_eq!((report.written, report.total), (0, 1));
/// assert!(report.is_truncated());
/// # Ok::<(), ferravex_engine::spatial::SpatialError>(())
/// ```
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[must_use = "check is_truncated() to detect omitted spatial matches"]
pub struct SpatialQueryReport {
    /// Number of results written to the start of the caller's buffer, in query order.
    pub written: usize,
    /// Total matching points, including those omitted from the buffer.
    pub total: usize,
}

/// One stored point selected by a distance-ranked query.
///
/// Queries return finite, nonnegative squared distances, ordered ascending and
/// breaking equal-distance ties by ascending ID. Distances use `f64` arithmetic;
/// underflow can round distinct small distances to zero, which also ties by ID.
/// `Default` is useful for initializing output storage; only the reported written
/// prefix contains query results.
///
/// ```
/// use ferravex_engine::spatial::{SpatialGrid, SpatialMatch};
/// let mut grid = SpatialGrid::new(1.0, 1)?;
/// grid.insert(7, [3.0, 4.0])?;
/// let mut out = [SpatialMatch::default(); 1];
/// let report = grid.query_radius_into([0.0; 2], 5.0, &mut out)?;
/// assert_eq!(report.written, 1);
/// assert_eq!(out[0], SpatialMatch {
///     id: 7, point: [3.0, 4.0], distance_squared: 25.0,
/// });
/// # Ok::<(), ferravex_engine::spatial::SpatialError>(())
/// ```
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct SpatialMatch {
    /// Host-owned ID supplied to [`SpatialGrid::insert`].
    pub id: u64,
    /// Current stored point, without a duplicate caller-side position table.
    pub point: [f64; 2],
    /// Squared distance from the query center, in squared coordinate units.
    pub distance_squared: f64,
}

impl SpatialQueryReport {
    /// Whether matches were omitted by storage capacity or a nearest-query `k` limit.
    ///
    /// ```
    /// use ferravex_engine::spatial::SpatialQueryReport;
    ///
    /// assert!(!SpatialQueryReport { written: 2, total: 2 }.is_truncated());
    /// assert!(SpatialQueryReport { written: 2, total: 3 }.is_truncated());
    /// ```
    pub const fn is_truncated(self) -> bool {
        self.written < self.total
    }
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
    /// Returns every match. Use [`Self::query_into`] for bounded, reusable output.
    pub fn query(&self, min: [f64; 2], max: [f64; 2]) -> Result<Vec<u64>, SpatialError> {
        let mut out = Vec::new();
        self.visit_query(min, max, |id, _| out.push(id))?;
        out.sort_unstable();
        Ok(out)
    }

    /// Query into a caller-provided buffer without allocating.
    ///
    /// Uses the same inclusive bounds, exact point filtering and ascending ID
    /// order as [`Self::query`]. Writes the smallest matching IDs to
    /// `out[..report.written]`, leaving the rest untouched. `out.len()` limits
    /// output, not query work: all matching points in the visited cells are
    /// counted in `report.total`, even when `out` is empty. Check
    /// [`SpatialQueryReport::is_truncated`] before treating the output as complete.
    ///
    /// Returns [`SpatialError::Invalid`] for reversed bounds, nonfinite coordinates
    /// or coordinates outside the grid's cell range, and
    /// [`SpatialError::QueryTooLarge`] for bounds spanning more than 4096 cells.
    /// On error, `out` is unchanged, including when it has zero length.
    ///
    /// The buffer doubles as a bounded max-heap during traversal, so retaining
    /// each match takes at most logarithmic work in the buffer length; only the
    /// written prefix is sorted afterward. No scratch allocation is needed.
    ///
    /// ```
    /// use ferravex_engine::spatial::SpatialGrid;
    ///
    /// let mut grid = SpatialGrid::new(1.0, 3)?;
    /// grid.insert(30, [-1.0, 0.0])?;
    /// grid.insert(10, [1.0, 0.0])?;
    /// grid.insert(20, [0.0, 0.0])?;
    /// let mut ids = [0; 2];
    /// let report = grid.query_into([-1.0, 0.0], [1.0, 0.0], &mut ids)?;
    /// assert_eq!(&ids[..report.written], &[10, 20]);
    /// assert_eq!(report.total, 3);
    /// assert!(report.is_truncated());
    /// # Ok::<(), ferravex_engine::spatial::SpatialError>(())
    /// ```
    pub fn query_into(
        &self,
        min: [f64; 2],
        max: [f64; 2],
        out: &mut [u64],
    ) -> Result<SpatialQueryReport, SpatialError> {
        let mut report = SpatialQueryReport::default();
        self.visit_query(min, max, |id, _| {
            report.total += 1;
            retain_smallest(out, &mut report.written, id, u64::cmp);
        })?;
        out[..report.written].sort_unstable();
        Ok(report)
    }

    /// Inclusive circular point query, ranked by squared distance then ID.
    ///
    /// Equivalent to [`Self::query_nearest_into`] with `k = out.len()`. All points
    /// inside the circle are counted, even with empty output. Retains the closest
    /// `out.len()` matches, writes only `out[..report.written]`, and leaves the tail
    /// untouched. Inspect [`SpatialQueryReport::is_truncated`] for omitted points.
    /// See [`Self::query_nearest_into`] for numeric, work and error bounds.
    ///
    /// ```
    /// use ferravex_engine::spatial::{SpatialGrid, SpatialMatch};
    /// let mut grid = SpatialGrid::new(1.0, 3)?;
    /// grid.insert(1, [2.0, 2.0])?; // In the bounding square, outside the circle.
    /// grid.insert(2, [2.0, 0.0])?; // Exactly on the inclusive boundary.
    /// grid.insert(9, [0.0, 0.0])?;
    /// let mut out = [SpatialMatch::default(); 3];
    /// let report = grid.query_radius_into([0.0; 2], 2.0, &mut out)?;
    /// assert_eq!((report.written, report.total), (2, 2));
    /// assert!(!report.is_truncated());
    /// assert_eq!((out[0].id, out[1].id), (9, 2));
    /// assert_eq!(out[2], SpatialMatch::default());
    /// # Ok::<(), ferravex_engine::spatial::SpatialError>(())
    /// ```
    pub fn query_radius_into(
        &self,
        center: [f64; 2],
        radius: f64,
        out: &mut [SpatialMatch],
    ) -> Result<SpatialQueryReport, SpatialError> {
        self.query_nearest_into(center, radius, out.len(), out)
    }

    /// Select at most `k` nearest points within an inclusive, bounded circle.
    ///
    /// Every candidate in the circle's bounding cells is examined before the
    /// final selection is returned; a farther, lower ID cannot hide a closer
    /// point. Results are ordered by ascending squared distance, then ascending
    /// ID. Writes `min(k, out.len(), total)` results, preserving the unwritten
    /// tail. `total` counts **all** circle matches, including those omitted by
    /// either `k` or storage capacity. `k == 0` or empty output is count-only,
    /// with the same validation and traversal as a query that writes results.
    ///
    /// The caller buffer serves as a bounded max-heap; no query allocation or
    /// whole-grid scan occurs. Work is bounded by 4096 bounding cells, their
    /// occupants (at most the grid's point capacity), logarithmic heap work per
    /// match, and an in-place sort of the written prefix. A small `k` does not
    /// reduce candidate traversal. Choose cell size and radius together.
    ///
    /// # Numeric limits and errors
    ///
    /// Returns [`SpatialError::Invalid`] for nonfinite centers, negative/nonfinite
    /// radius, an overflowing `radius * radius`, or bounding coordinates
    /// `center +/- radius` that are nonfinite or outside the existing i32 cell
    /// range. The bounding square is never silently clipped. Returns
    /// [`SpatialError::QueryTooLarge`] if that square spans more than 4096 cells,
    /// even for an empty grid, `k == 0`, or empty output. All errors leave the
    /// entire caller buffer unchanged.
    ///
    /// Circle filtering compares `dx * dx + dy * dy <= radius * radius` without
    /// an epsilon. Overflowing candidate distances are outside a valid radius
    /// and are omitted. When the radius square is subnormal or underflows, the
    /// comparison instead uses radius-normalized deltas to avoid admitting the
    /// square's corners. A zero radius matches only equal stored coordinates
    /// (including signed zero). Returned squared distances still use ordinary
    /// `f64` arithmetic: underflow/rounding ties are resolved by ID. These are
    /// floating-point geometric tests, not exact real arithmetic.
    ///
    /// # Tower-target recipe
    ///
    /// Store eligible targets, update their positions on executed simulation
    /// ticks, and remove expired IDs. This recipe selects from stored point data
    /// without duplicating positions. The accepted target ID can then be kept
    /// by a projectile independently of later queries. Faction, visibility and
    /// game-specific eligibility belong to the caller; filtering a truncated
    /// prefix afterward may miss an eligible target farther away.
    ///
    /// ```
    /// use ferravex_engine::spatial::{SpatialError, SpatialGrid, SpatialMatch};
    ///
    /// fn main() -> Result<(), SpatialError> {
    ///     let mut targets = SpatialGrid::new(4.0, 64)?;
    ///     targets.insert(1, [6.0, 0.0])?;
    ///     targets.insert(90, [1.0, 1.0])?;
    ///     targets.insert(91, [2.0, 0.0])?;
    ///     targets.insert(90, [1.0, 0.0])?; // Move on an executed tick.
    ///     targets.remove(91); // Despawn before selecting a target.
    ///
    ///     let tower = [0.0, 0.0];
    ///     let mut nearest = [SpatialMatch::default(); 1];
    ///     let report = targets.query_nearest_into(tower, 8.0, 1, &mut nearest)?;
    ///     assert_eq!(report.total, 2);
    ///     assert!(report.is_truncated()); // One intentionally omitted target.
    ///     if let Some(target) = nearest[..report.written].first() {
    ///         let accepted_target_id = target.id;
    ///         let aim_delta = [target.point[0] - tower[0], target.point[1] - tower[1]];
    ///         assert_eq!((accepted_target_id, aim_delta), (90, [1.0, 0.0]));
    ///         assert_eq!(target.distance_squared, 1.0);
    ///     }
    ///     Ok(())
    /// }
    /// ```
    pub fn query_nearest_into(
        &self,
        center: [f64; 2],
        radius: f64,
        k: usize,
        out: &mut [SpatialMatch],
    ) -> Result<SpatialQueryReport, SpatialError> {
        let radius_squared = radius * radius;
        if !radius.is_finite() || radius < 0. || !radius_squared.is_finite() {
            return Err(SpatialError::Invalid);
        }
        let min = [center[0] - radius, center[1] - radius];
        let max = [center[0] + radius, center[1] + radius];
        let limit = k.min(out.len());
        let out = &mut out[..limit];
        let mut report = SpatialQueryReport::default();
        self.visit_query(min, max, |id, point| {
            let dx = point[0] - center[0];
            let dy = point[1] - center[1];
            let distance_squared = dx * dx + dy * dy;
            let inside = if radius == 0. {
                point == center
            } else if radius_squared < f64::MIN_POSITIVE {
                let x = dx / radius;
                let y = dy / radius;
                x * x + y * y <= 1.
            } else {
                distance_squared <= radius_squared
            };
            if inside {
                report.total += 1;
                retain_smallest(
                    out,
                    &mut report.written,
                    SpatialMatch {
                        id,
                        point,
                        distance_squared,
                    },
                    compare_matches,
                );
            }
        })?;
        out[..report.written].sort_unstable_by(compare_matches);
        Ok(report)
    }

    // Validate the whole query before visiting matches, so callers can write
    // directly into their output without rolling back on errors.
    fn visit_query(
        &self,
        min: [f64; 2],
        max: [f64; 2],
        mut visit: impl FnMut(u64, [f64; 2]),
    ) -> Result<(), SpatialError> {
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
        for x in a.0..=b.0 {
            for y in a.1..=b.1 {
                if let Some(bucket) = self.cells.get(&(x, y)) {
                    for id in bucket {
                        let p = self.points[id].0;
                        if p[0] >= min[0] && p[0] <= max[0] && p[1] >= min[1] && p[1] <= max[1] {
                            visit(*id, p);
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

fn compare_matches(a: &SpatialMatch, b: &SpatialMatch) -> Ordering {
    a.distance_squared
        .total_cmp(&b.distance_squared)
        .then_with(|| a.id.cmp(&b.id))
}

// The output doubles as a max-heap of the smallest values seen so far.
fn retain_smallest<T: Copy>(
    heap: &mut [T],
    written: &mut usize,
    value: T,
    compare: fn(&T, &T) -> Ordering,
) {
    if *written < heap.len() {
        let mut child = *written;
        heap[child] = value;
        *written += 1;
        while child > 0 {
            let parent = (child - 1) / 2;
            if !compare(&heap[parent], &heap[child]).is_lt() {
                break;
            }
            heap.swap(parent, child);
            child = parent;
        }
    } else if !heap.is_empty() && compare(&value, &heap[0]).is_lt() {
        heap[0] = value;
        sift_down(heap, compare);
    }
}

// Restore a max-heap after replacing its root with a smaller value.
fn sift_down<T>(heap: &mut [T], compare: fn(&T, &T) -> Ordering) {
    let mut root = 0;
    loop {
        let mut child = root * 2 + 1;
        if child >= heap.len() {
            return;
        }
        if child + 1 < heap.len() && compare(&heap[child + 1], &heap[child]).is_gt() {
            child += 1;
        }
        if !compare(&heap[root], &heap[child]).is_lt() {
            return;
        }
        heap.swap(root, child);
        root = child;
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

    #[test]
    fn caller_buffer_reports_truncation_exact_fit_and_untouched_tail() {
        let mut grid = SpatialGrid::new(1., 3).unwrap();
        grid.insert(30, [-1., 0.]).unwrap();
        grid.insert(20, [0., 0.]).unwrap();
        grid.insert(10, [1., 0.]).unwrap();
        let min = [-1., 0.];
        let max = [1., 0.];

        let report = grid.query_into(min, max, &mut []).unwrap();
        assert_eq!(
            report,
            SpatialQueryReport {
                written: 0,
                total: 3
            }
        );
        assert!(report.is_truncated());

        let mut short = [99; 2];
        let report = grid.query_into(min, max, &mut short).unwrap();
        assert_eq!(
            report,
            SpatialQueryReport {
                written: 2,
                total: 3
            }
        );
        assert!(report.is_truncated());
        assert_eq!(short, [10, 20]);

        let mut exact = [99; 3];
        let report = grid.query_into(min, max, &mut exact).unwrap();
        assert_eq!(
            report,
            SpatialQueryReport {
                written: 3,
                total: 3
            }
        );
        assert!(!report.is_truncated());
        assert_eq!(exact, [10, 20, 30]);

        let mut large = [99; 5];
        let report = grid.query_into(min, max, &mut large).unwrap();
        assert_eq!(
            report,
            SpatialQueryReport {
                written: 3,
                total: 3
            }
        );
        assert!(!report.is_truncated());
        assert_eq!(large, [10, 20, 30, 99, 99]);

        let report = grid.query_into([2., 2.], [2., 2.], &mut large).unwrap();
        assert_eq!(report, SpatialQueryReport::default());
        assert!(!report.is_truncated());
        assert_eq!(large, [10, 20, 30, 99, 99]);
    }

    #[test]
    fn caller_buffer_matches_sorted_prefix_for_every_capacity_and_insert_order() {
        let mut points: Vec<_> = (0..97)
            .map(|i| {
                (
                    ((i * 37) % 97) as u64,
                    [
                        f64::from((i * 7) % 19) - 9. + 0.25,
                        f64::from((i * 13) % 23) - 11. + 0.75,
                    ],
                )
            })
            .collect();
        points.push((u64::MAX, [0., 0.]));
        let mut forward = SpatialGrid::new(1., points.len()).unwrap();
        let mut reverse = SpatialGrid::new(1., points.len()).unwrap();
        for &(id, point) in &points {
            forward.insert(id, point).unwrap();
        }
        for &(id, point) in points.iter().rev() {
            reverse.insert(id, point).unwrap();
        }
        for (min, max) in [
            ([-10., -12.], [10., 12.]),
            ([-6.5, -8.5], [5.5, 8.5]),
            ([-1.5, -1.5], [1.5, 1.5]),
            ([0., 0.], [0., 0.]),
        ] {
            let mut expected: Vec<_> = points
                .iter()
                .filter(|(_, p)| {
                    p[0] >= min[0] && p[0] <= max[0] && p[1] >= min[1] && p[1] <= max[1]
                })
                .map(|(id, _)| *id)
                .collect();
            expected.sort_unstable();
            for grid in [&forward, &reverse] {
                assert_eq!(grid.query(min, max).unwrap(), expected);
                for capacity in 0..=points.len() + 1 {
                    let mut out = vec![123; capacity];
                    let report = grid.query_into(min, max, &mut out).unwrap();
                    let written = capacity.min(expected.len());
                    assert_eq!(report.written, written);
                    assert_eq!(report.total, expected.len());
                    assert_eq!(report.is_truncated(), capacity < expected.len());
                    assert_eq!(&out[..written], &expected[..written]);
                    assert!(out[written..].iter().all(|id| *id == 123));
                }
            }
        }
    }

    #[test]
    fn caller_buffer_preserves_inclusive_filtering_moves_and_removals() {
        let mut grid = SpatialGrid::new(1., 4).unwrap();
        grid.insert(4, [-0.9, -0.9]).unwrap();
        grid.insert(3, [-0.5, -0.5]).unwrap();
        grid.insert(2, [0.5, 0.5]).unwrap();
        grid.insert(1, [0.9, 0.9]).unwrap();
        let mut out = [99; 1];
        let report = grid.query_into([-0.5; 2], [0.5; 2], &mut out).unwrap();
        assert_eq!(
            report,
            SpatialQueryReport {
                written: 1,
                total: 2
            }
        );
        assert_eq!(out, [2]);

        assert_eq!(grid.insert(2, [f64::NAN, 0.]), Err(SpatialError::Invalid));
        assert_eq!(grid.insert(5, [0., 0.]), Err(SpatialError::Capacity));
        assert_eq!(
            grid.query_into([-0.5; 2], [0.5; 2], &mut out).unwrap(),
            report
        );
        assert_eq!(out, [2]);

        grid.insert(2, [2., 2.]).unwrap();
        grid.insert(1, [-0.25, 0.25]).unwrap();
        assert!(grid.remove(3));
        let report = grid.query_into([-0.5; 2], [0.5; 2], &mut out).unwrap();
        assert_eq!(
            report,
            SpatialQueryReport {
                written: 1,
                total: 1
            }
        );
        assert!(!report.is_truncated());
        assert_eq!(out, [1]);
    }

    #[test]
    fn caller_buffer_validation_is_atomic_even_with_empty_output() {
        let mut grid = SpatialGrid::new(1., 1).unwrap();
        grid.insert(7, [0., 0.]).unwrap();
        for axis in 0..2 {
            for invalid in [
                f64::NAN,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::MAX,
                f64::from(i32::MAX) + 1.,
                f64::from(i32::MIN) - 1.,
            ] {
                let mut point = [0.; 2];
                point[axis] = invalid;
                for (min, max) in [(point, [0.; 2]), ([0.; 2], point)] {
                    let mut out = [99, 98];
                    assert_eq!(grid.query(min, max), Err(SpatialError::Invalid));
                    assert_eq!(
                        grid.query_into(min, max, &mut out),
                        Err(SpatialError::Invalid)
                    );
                    assert_eq!(out, [99, 98]);
                    assert_eq!(
                        grid.query_into(min, max, &mut []),
                        Err(SpatialError::Invalid)
                    );
                }
            }
            let mut reversed = [0.; 2];
            reversed[axis] = 1.;
            let mut out = [99, 98];
            assert_eq!(
                grid.query_into(reversed, [0.; 2], &mut out),
                Err(SpatialError::Invalid)
            );
            assert_eq!(out, [99, 98]);
        }

        let tiny = SpatialGrid::new(f64::MIN_POSITIVE, 0).unwrap();
        let mut out = [99];
        assert_eq!(
            tiny.query_into([0.; 2], [f64::MAX; 2], &mut out),
            Err(SpatialError::Invalid)
        );
        assert_eq!(out, [99]);
    }

    #[test]
    fn caller_buffer_keeps_cell_work_limit_and_cell_coordinate_extremes() {
        let mut grid = SpatialGrid::new(1., 3).unwrap();
        grid.insert(1, [0., 0.]).unwrap();
        grid.insert(2, [f64::from(i32::MIN); 2]).unwrap();
        grid.insert(3, [f64::from(i32::MAX); 2]).unwrap();
        for (min, max, expected) in [
            ([0., 0.], [4095., 0.], 1),
            ([0., 0.], [0., 4095.], 1),
            ([0., 0.], [63., 63.], 1),
            ([f64::from(i32::MIN); 2], [f64::from(i32::MIN); 2], 2),
            ([f64::from(i32::MAX); 2], [f64::from(i32::MAX); 2], 3),
        ] {
            let mut out = [99];
            let report = grid.query_into(min, max, &mut out).unwrap();
            assert_eq!(
                report,
                SpatialQueryReport {
                    written: 1,
                    total: 1
                }
            );
            assert_eq!(out, [expected]);
            assert_eq!(grid.query(min, max).unwrap(), [expected]);
        }
        for max in [[4096., 0.], [0., 4096.], [64., 63.]] {
            let mut out = [99];
            assert_eq!(
                grid.query_into([0.; 2], max, &mut out),
                Err(SpatialError::QueryTooLarge)
            );
            assert_eq!(out, [99]);
            assert_eq!(
                grid.query_into([0.; 2], max, &mut []),
                Err(SpatialError::QueryTooLarge)
            );
            assert_eq!(grid.query([0.; 2], max), Err(SpatialError::QueryTooLarge));
        }
    }

    #[test]
    fn caller_buffer_empty_grid_is_complete() {
        let grid = SpatialGrid::new(1., 0).unwrap();
        let mut out = [99];
        for buffer in [&mut out[..], &mut [][..]] {
            let report = grid.query_into([0.; 2], [0.; 2], buffer).unwrap();
            assert_eq!(report, SpatialQueryReport::default());
            assert!(!report.is_truncated());
        }
        assert_eq!(out, [99]);
    }

    #[test]
    fn proximity_circle_boundaries_colocated_ties_and_stored_points() {
        let mut grid = SpatialGrid::new(1., 8).unwrap();
        for (id, point) in [
            (1, [3., 4.]),
            (2, [5., 0.]),
            (3, [-5., 0.]),
            (4, [0., -5.]),
            (5, [5., 5.]), // Bounding-square corner is outside the circle.
            (6, [5.000000000000001, 0.]),
            (u64::MAX, [0., -0.]),
            (0, [-0., 0.]),
        ] {
            grid.insert(id, point).unwrap();
        }
        let sentinel = SpatialMatch {
            id: 123,
            point: [99.; 2],
            distance_squared: -1.,
        };
        let mut out = [sentinel; 8];
        let report = grid.query_radius_into([0.; 2], 5., &mut out).unwrap();
        assert_eq!(
            report,
            SpatialQueryReport {
                written: 6,
                total: 6
            }
        );
        assert!(!report.is_truncated());
        assert_eq!(
            out[..6].iter().map(|m| m.id).collect::<Vec<_>>(),
            [0, u64::MAX, 1, 2, 3, 4]
        );
        assert_eq!(out[2].point, [3., 4.]);
        assert_eq!(out[2].distance_squared, 25.);
        assert_eq!(out[6..], [sentinel; 2]);

        for radius in [0., -0.] {
            let report = grid.query_radius_into([0.; 2], radius, &mut out).unwrap();
            assert_eq!(
                report,
                SpatialQueryReport {
                    written: 2,
                    total: 2
                }
            );
            assert_eq!((out[0].id, out[1].id), (0, u64::MAX));
            assert_eq!(out[0].distance_squared.to_bits(), 0_f64.to_bits());
        }
    }

    #[test]
    fn nearest_searches_later_cells_and_ids_before_selecting() {
        let mut grid = SpatialGrid::new(1., 4).unwrap();
        grid.insert(1, [-4., 0.]).unwrap(); // First cell, farthest point.
        grid.insert(2, [0.9, 0.]).unwrap(); // Earlier ID, same cell as closest.
        grid.insert(90, [0.1, 0.]).unwrap();
        grid.insert(91, [1., 0.]).unwrap();
        let mut out = [SpatialMatch::default(); 3];
        for (k, written, ids) in [(1, 1, &[90][..]), (2, 2, &[90, 2][..])] {
            out.fill(SpatialMatch::default());
            let report = grid.query_nearest_into([0.; 2], 4., k, &mut out).unwrap();
            assert_eq!(report, SpatialQueryReport { written, total: 4 });
            assert!(report.is_truncated());
            assert_eq!(out[..written].iter().map(|m| m.id).collect::<Vec<_>>(), ids);
            assert!(out[written..].iter().all(|m| *m == SpatialMatch::default()));
        }
    }

    #[test]
    fn proximity_all_capacities_k_values_and_insert_orders_match_reference() {
        let points: Vec<_> = (0..73)
            .map(|i| {
                (
                    ((i * 29) % 73) as u64,
                    [f64::from((i * 7) % 17) - 8., f64::from((i * 11) % 19) - 9.],
                )
            })
            .collect();
        let mut forward = SpatialGrid::new(1., points.len()).unwrap();
        let mut reverse = SpatialGrid::new(1., points.len()).unwrap();
        for &(id, point) in &points {
            forward.insert(id, point).unwrap();
        }
        for &(id, point) in points.iter().rev() {
            reverse.insert(id, point).unwrap();
        }
        let sentinel = SpatialMatch {
            id: 999,
            ..SpatialMatch::default()
        };
        for (center, radius) in [([0.; 2], 10.), ([-2., 3.], 5.), ([0.; 2], 0.)] {
            let mut expected: Vec<_> = points
                .iter()
                .filter_map(|&(id, point)| {
                    let dx = point[0] - center[0];
                    let dy = point[1] - center[1];
                    let distance_squared = dx * dx + dy * dy;
                    (distance_squared <= radius * radius).then_some(SpatialMatch {
                        id,
                        point,
                        distance_squared,
                    })
                })
                .collect();
            expected.sort_unstable_by(|a, b| {
                a.distance_squared
                    .partial_cmp(&b.distance_squared)
                    .unwrap()
                    .then(a.id.cmp(&b.id))
            });
            for grid in [&forward, &reverse] {
                for capacity in 0..=points.len() + 1 {
                    let mut out = vec![sentinel; capacity];
                    for k in [0, 1, 2, 7, expected.len(), points.len() + 1, usize::MAX] {
                        out.fill(sentinel);
                        let report = grid
                            .query_nearest_into(center, radius, k, &mut out)
                            .unwrap();
                        let written = k.min(capacity).min(expected.len());
                        assert_eq!(
                            report,
                            SpatialQueryReport {
                                written,
                                total: expected.len()
                            }
                        );
                        assert_eq!(report.is_truncated(), written < expected.len());
                        assert_eq!(out[..written], expected[..written]);
                        assert!(out[written..].iter().all(|m| *m == sentinel));
                    }
                    out.fill(sentinel);
                    let report = grid.query_radius_into(center, radius, &mut out).unwrap();
                    let written = capacity.min(expected.len());
                    assert_eq!(
                        report,
                        SpatialQueryReport {
                            written,
                            total: expected.len()
                        }
                    );
                    assert_eq!(out[..written], expected[..written]);
                    assert!(out[written..].iter().all(|m| *m == sentinel));
                }
            }
        }
    }

    #[test]
    fn proximity_moves_removals_rejected_updates_and_reuse_do_not_leave_results() {
        let mut grid = SpatialGrid::new(1., 2).unwrap();
        grid.insert(10, [0., 0.]).unwrap();
        grid.insert(20, [1., 0.]).unwrap();
        let mut out = [SpatialMatch::default(); 2];
        assert_eq!(
            grid.query_radius_into([0.; 2], 2., &mut out).unwrap().total,
            2
        );
        let before = out;
        assert_eq!(grid.insert(30, [0.; 2]), Err(SpatialError::Capacity));
        assert_eq!(grid.insert(10, [f64::NAN, 0.]), Err(SpatialError::Invalid));
        assert_eq!(
            grid.query_radius_into([0.; 2], 2., &mut out).unwrap().total,
            2
        );
        assert_eq!(out, before);

        grid.insert(10, [3., 0.]).unwrap(); // Existing IDs can move at capacity.
        grid.insert(20, [-0.5, 0.]).unwrap();
        let report = grid.query_nearest_into([0.; 2], 2., 1, &mut out).unwrap();
        assert_eq!(
            report,
            SpatialQueryReport {
                written: 1,
                total: 1
            }
        );
        assert_eq!(
            out[0],
            SpatialMatch {
                id: 20,
                point: [-0.5, 0.],
                distance_squared: 0.25
            }
        );
        assert_eq!(out[1], before[1]);
        assert!(grid.remove(10));
        assert!(grid.remove(20));
        assert!(!grid.remove(20));
        let old_output = out;
        assert_eq!(
            grid.query_radius_into([0.; 2], 2., &mut out).unwrap(),
            SpatialQueryReport::default()
        );
        assert_eq!(out, old_output); // A zero written prefix does not clear caller storage.
        grid.insert(20, [0.25, 0.]).unwrap();
        let report = grid.query_radius_into([0.; 2], 2., &mut out).unwrap();
        assert_eq!(
            report,
            SpatialQueryReport {
                written: 1,
                total: 1
            }
        );
        assert_eq!(out[0].distance_squared, 0.0625);
    }

    #[test]
    fn proximity_errors_are_atomic_even_for_zero_k_and_empty_output() {
        let mut grid = SpatialGrid::new(1., 1).unwrap();
        grid.insert(7, [0.; 2]).unwrap();
        let mut cases = vec![];
        for radius in [
            -1.,
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::MAX,
            1e155,
        ] {
            cases.push(([0.; 2], radius));
        }
        for axis in 0..2 {
            for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, f64::MAX] {
                let mut center = [0.; 2];
                center[axis] = invalid;
                cases.push((center, 0.));
            }
        }
        cases.push(([f64::from(i32::MAX); 2], 1.));
        cases.push(([f64::from(i32::MIN); 2], 1.));
        for (center, radius) in cases {
            let sentinel = SpatialMatch {
                id: 99,
                ..SpatialMatch::default()
            };
            let mut out = [sentinel; 2];
            for k in [0, 1, usize::MAX] {
                assert_eq!(
                    grid.query_nearest_into(center, radius, k, &mut out),
                    Err(SpatialError::Invalid)
                );
                assert_eq!(out, [sentinel; 2]);
                assert_eq!(
                    grid.query_nearest_into(center, radius, k, &mut []),
                    Err(SpatialError::Invalid)
                );
            }
            assert_eq!(
                grid.query_radius_into(center, radius, &mut out),
                Err(SpatialError::Invalid)
            );
            assert_eq!(out, [sentinel; 2]);
        }
        let tiny = SpatialGrid::new(f64::MIN_POSITIVE, 0).unwrap();
        assert_eq!(
            tiny.query_radius_into([0.; 2], 1., &mut []),
            Err(SpatialError::Invalid)
        );
    }

    #[test]
    fn proximity_bounding_cell_limit_is_validated_before_output() {
        let mut grid = SpatialGrid::new(1., 1).unwrap();
        grid.insert(7, [31.5; 2]).unwrap();
        let mut out = [SpatialMatch::default(); 1];
        // [0, 63] on both axes is exactly 4096 cells.
        assert_eq!(
            grid.query_radius_into([31.5; 2], 31.5, &mut out)
                .unwrap()
                .total,
            1
        );
        let before = out;
        let empty = SpatialGrid::new(1., 0).unwrap();
        for grid in [&grid, &empty] {
            for radius in [32., 100., 1e9] {
                for k in [0, 1, usize::MAX] {
                    assert_eq!(
                        grid.query_nearest_into([31.5; 2], radius, k, &mut out),
                        Err(SpatialError::QueryTooLarge)
                    );
                    assert_eq!(out, before);
                    assert_eq!(
                        grid.query_nearest_into([31.5; 2], radius, k, &mut []),
                        Err(SpatialError::QueryTooLarge)
                    );
                }
                assert_eq!(
                    grid.query_radius_into([31.5; 2], radius, &mut out),
                    Err(SpatialError::QueryTooLarge)
                );
                assert_eq!(out, before);
            }
        }
    }

    #[test]
    fn proximity_extreme_coordinates_and_overflowing_corner_distance() {
        let mut grid = SpatialGrid::new(1., 2).unwrap();
        for (id, coordinate) in [(1, f64::from(i32::MIN)), (2, f64::from(i32::MAX))] {
            grid.insert(id, [coordinate; 2]).unwrap();
            let mut out = [SpatialMatch::default(); 1];
            let report = grid
                .query_radius_into([coordinate; 2], 0., &mut out)
                .unwrap();
            assert_eq!(
                report,
                SpatialQueryReport {
                    written: 1,
                    total: 1
                }
            );
            assert_eq!(
                out[0],
                SpatialMatch {
                    id,
                    point: [coordinate; 2],
                    distance_squared: 0.
                }
            );
        }
        let mut huge = SpatialGrid::new(f64::MAX, 1).unwrap();
        huge.insert(8, [f64::MAX; 2]).unwrap();
        let mut out = [SpatialMatch::default(); 3];
        assert_eq!(
            huge.query_radius_into([f64::MAX; 2], 0., &mut out)
                .unwrap()
                .total,
            1
        );
        assert_eq!(out[0].point, [f64::MAX; 2]);

        let radius = 1.3e154;
        let mut large = SpatialGrid::new(radius, 3).unwrap();
        large.insert(1, [radius; 2]).unwrap(); // dx² + dy² overflows, outside circle.
        large.insert(2, [-radius, 0.]).unwrap();
        large.insert(3, [0., 0.]).unwrap();
        let report = large.query_radius_into([0.; 2], radius, &mut out).unwrap();
        assert_eq!(
            report,
            SpatialQueryReport {
                written: 2,
                total: 2
            }
        );
        assert_eq!((out[0].id, out[1].id), (3, 2));
        assert_eq!(out[1].distance_squared, radius * radius);
        assert!(
            out[..report.written]
                .iter()
                .all(|m| m.distance_squared.is_finite())
        );
    }

    #[test]
    fn proximity_tiny_radii_filter_circle_before_squared_distance_underflow_ties() {
        for radius in [1e-160, 1e-200, f64::from_bits(1)] {
            let mut grid = SpatialGrid::new(radius, 4).unwrap();
            grid.insert(1, [radius; 2]).unwrap(); // Must not become a match when r² == 0.
            grid.insert(4, [radius, 0.]).unwrap();
            grid.insert(3, [0., -radius]).unwrap();
            grid.insert(2, [0., 0.]).unwrap();
            let mut out = [SpatialMatch::default(); 4];
            let report = grid.query_radius_into([0.; 2], radius, &mut out).unwrap();
            assert_eq!(
                report,
                SpatialQueryReport {
                    written: 3,
                    total: 3
                }
            );
            assert_eq!(out[..3].iter().map(|m| m.id).collect::<Vec<_>>(), [2, 3, 4]);
            assert_eq!(out[1].distance_squared, radius * radius);
            let report = grid.query_radius_into([0.; 2], 0., &mut out).unwrap();
            assert_eq!(
                report,
                SpatialQueryReport {
                    written: 1,
                    total: 1
                }
            );
            assert_eq!(out[0].id, 2);
        }
    }

    #[test]
    fn proximity_empty_grid_and_count_only_are_complete_or_truncated_by_total() {
        let mut grid = SpatialGrid::new(1., 0).unwrap();
        let mut out = [SpatialMatch::default(); 2];
        assert_eq!(
            grid.query_radius_into([0.; 2], 1., &mut out).unwrap(),
            SpatialQueryReport::default()
        );
        assert_eq!(
            grid.query_nearest_into([0.; 2], 1., 0, &mut []).unwrap(),
            SpatialQueryReport::default()
        );
        assert_eq!(grid.insert(1, [0.; 2]), Err(SpatialError::Capacity));
        grid = SpatialGrid::new(1., 1).unwrap();
        grid.insert(1, [0.; 2]).unwrap();
        for (k, buffer) in [(0, &mut out[..]), (1, &mut [][..])] {
            let report = grid.query_nearest_into([0.; 2], 1., k, buffer).unwrap();
            assert_eq!(
                report,
                SpatialQueryReport {
                    written: 0,
                    total: 1
                }
            );
            assert!(report.is_truncated());
        }
        assert_eq!(out, [SpatialMatch::default(); 2]);
    }
}
