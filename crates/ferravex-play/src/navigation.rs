//! Deterministic weighted-grid A* and small 2D steering helpers.
use std::cmp::Ordering;
use std::collections::BinaryHeap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Cell {
    pub x: u32,
    pub y: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavigationError {
    InvalidDimensions,
    OutOfBounds,
    BlockedEndpoint,
    NoPath,
    InvalidCost,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Connectivity {
    Four,
    Eight,
}

pub struct Grid {
    width: u32,
    height: u32,
    costs: Vec<u16>, // zero is impassable; positive values multiply edge cost.
}
impl Grid {
    pub fn new(width: u32, height: u32) -> Result<Self, NavigationError> {
        let count = width
            .checked_mul(height)
            .filter(|n| *n > 0 && *n <= 4_000_000)
            .ok_or(NavigationError::InvalidDimensions)?;
        Ok(Self {
            width,
            height,
            costs: vec![1; count as usize],
        })
    }
    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    pub fn set_blocked(&mut self, cell: Cell, blocked: bool) -> Result<(), NavigationError> {
        let index = self.index(cell)?;
        self.costs[index] = if blocked { 0 } else { 1 };
        Ok(())
    }
    pub fn set_cost(&mut self, cell: Cell, cost: u16) -> Result<(), NavigationError> {
        if cost == 0 {
            return Err(NavigationError::InvalidCost);
        }
        let index = self.index(cell)?;
        self.costs[index] = cost;
        Ok(())
    }
    pub fn walkable(&self, cell: Cell) -> bool {
        self.index(cell).is_ok_and(|index| self.costs[index] != 0)
    }
    pub fn path(
        &self,
        start: Cell,
        end: Cell,
        connectivity: Connectivity,
    ) -> Result<Vec<Cell>, NavigationError> {
        let start_index = self.index(start)?;
        let end_index = self.index(end)?;
        if !self.walkable(start) || !self.walkable(end) {
            return Err(NavigationError::BlockedEndpoint);
        }
        let mut distance = vec![u64::MAX; self.costs.len()];
        let mut parent = vec![usize::MAX; self.costs.len()];
        let mut open = BinaryHeap::new();
        distance[start_index] = 0;
        open.push(Visit {
            index: start_index,
            cost: 0,
            estimate: heuristic(start, end, connectivity),
        });
        while let Some(visit) = open.pop() {
            if visit.cost != distance[visit.index] {
                continue;
            }
            if visit.index == end_index {
                let mut route = Vec::new();
                let mut index = end_index;
                loop {
                    route.push(self.cell(index));
                    if index == start_index {
                        break;
                    }
                    index = parent[index];
                }
                route.reverse();
                return Ok(route);
            }
            let current = self.cell(visit.index);
            for (dx, dy) in [
                (-1_i64, 0_i64),
                (0, -1),
                (1, 0),
                (0, 1),
                (-1, -1),
                (1, -1),
                (1, 1),
                (-1, 1),
            ] {
                let diagonal = dx != 0 && dy != 0;
                if diagonal && connectivity == Connectivity::Four {
                    continue;
                }
                let (x, y) = (i64::from(current.x) + dx, i64::from(current.y) + dy);
                if x < 0 || y < 0 || x >= i64::from(self.width) || y >= i64::from(self.height) {
                    continue;
                }
                let next = Cell {
                    x: x as u32,
                    y: y as u32,
                };
                if !self.walkable(next) {
                    continue;
                }
                if diagonal
                    && (!self.walkable(Cell {
                        x: next.x,
                        y: current.y,
                    }) || !self.walkable(Cell {
                        x: current.x,
                        y: next.y,
                    }))
                {
                    continue;
                }
                let index = self.index(next)?;
                let cost =
                    visit.cost + u64::from(self.costs[index]) * if diagonal { 14 } else { 10 };
                if cost < distance[index] {
                    distance[index] = cost;
                    parent[index] = visit.index;
                    open.push(Visit {
                        index,
                        cost,
                        estimate: cost + heuristic(next, end, connectivity),
                    });
                }
            }
        }
        Err(NavigationError::NoPath)
    }
    fn index(&self, cell: Cell) -> Result<usize, NavigationError> {
        if cell.x < self.width && cell.y < self.height {
            Ok((cell.y * self.width + cell.x) as usize)
        } else {
            Err(NavigationError::OutOfBounds)
        }
    }
    fn cell(&self, index: usize) -> Cell {
        Cell {
            x: index as u32 % self.width,
            y: index as u32 / self.width,
        }
    }
}
#[derive(Clone, Copy, Eq, PartialEq)]
struct Visit {
    index: usize,
    cost: u64,
    estimate: u64,
}
impl Ord for Visit {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .estimate
            .cmp(&self.estimate)
            .then_with(|| other.cost.cmp(&self.cost))
            .then_with(|| other.index.cmp(&self.index))
    }
}
impl PartialOrd for Visit {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
fn heuristic(a: Cell, b: Cell, connectivity: Connectivity) -> u64 {
    let dx = u64::from(a.x.abs_diff(b.x));
    let dy = u64::from(a.y.abs_diff(b.y));
    if connectivity == Connectivity::Four {
        (dx + dy) * 10
    } else {
        dx.min(dy) * 14 + dx.abs_diff(dy) * 10
    }
}
/// Desired velocity toward a target, slowing smoothly within the arrival radius.
/// Invalid or non-positive parameters produce a stopped agent rather than NaNs.
pub fn arrive(position: [f32; 2], target: [f32; 2], max_speed: f32, slow_radius: f32) -> [f32; 2] {
    if !position.into_iter().chain(target).all(f32::is_finite)
        || !max_speed.is_finite()
        || !slow_radius.is_finite()
        || max_speed <= 0.0
        || slow_radius <= 0.0
    {
        return [0.0; 2];
    }
    let d = [target[0] - position[0], target[1] - position[1]];
    let length = d[0].hypot(d[1]);
    if length <= f32::EPSILON || !length.is_finite() {
        return [0.0; 2];
    }
    let scale = max_speed * (length / slow_radius).min(1.0) / length;
    [d[0] * scale, d[1] * scale]
}
/// Repulsion from nearby peers, capped to max_speed; excludes coincident agents.
pub fn separation(position: [f32; 2], peers: &[[f32; 2]], radius: f32, max_speed: f32) -> [f32; 2] {
    if !radius.is_finite() || radius <= 0.0 || !max_speed.is_finite() || max_speed <= 0.0 {
        return [0.0; 2];
    }
    let mut sum = [0.0; 2];
    for peer in peers {
        let d = [position[0] - peer[0], position[1] - peer[1]];
        let length = d[0].hypot(d[1]);
        if length.is_finite() && length > 0.0001 && length < radius {
            let weight = (1.0 - length / radius) / length;
            sum[0] += d[0] * weight;
            sum[1] += d[1] * weight;
        }
    }
    arrive([0.0; 2], sum, max_speed, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn c(x: u32, y: u32) -> Cell {
        Cell { x, y }
    }
    #[test]
    fn detours_around_walls_and_respects_costs() {
        let mut grid = Grid::new(5, 3).unwrap();
        grid.set_blocked(c(2, 1), true).unwrap();
        let path = grid.path(c(0, 1), c(4, 1), Connectivity::Four).unwrap();
        assert_eq!(path.len(), 7);
        assert!(path.iter().all(|cell| grid.walkable(*cell)));
        grid.set_blocked(c(2, 1), false).unwrap();
        grid.set_cost(c(2, 1), 50).unwrap();
        assert!(
            !grid
                .path(c(0, 1), c(4, 1), Connectivity::Four)
                .unwrap()
                .contains(&c(2, 1))
        );
    }
    #[test]
    fn diagonal_moves_do_not_cut_blocked_corners() {
        let mut grid = Grid::new(2, 2).unwrap();
        grid.set_blocked(c(1, 0), true).unwrap();
        grid.set_blocked(c(0, 1), true).unwrap();
        assert_eq!(
            grid.path(c(0, 0), c(1, 1), Connectivity::Eight),
            Err(NavigationError::NoPath)
        );
        grid.set_blocked(c(1, 0), false).unwrap();
        assert_eq!(
            grid.path(c(0, 0), c(1, 1), Connectivity::Eight)
                .unwrap()
                .len(),
            3
        );
    }
    #[test]
    fn boundaries_empty_routes_and_invalid_dimensions() {
        assert!(matches!(
            Grid::new(0, 3),
            Err(NavigationError::InvalidDimensions)
        ));
        let grid = Grid::new(3, 3).unwrap();
        assert_eq!(
            grid.path(c(1, 1), c(1, 1), Connectivity::Four).unwrap(),
            vec![c(1, 1)]
        );
        assert_eq!(
            grid.path(c(5, 0), c(0, 0), Connectivity::Four),
            Err(NavigationError::OutOfBounds)
        );
        assert_eq!(
            grid.path(c(0, 0), c(2, 2), Connectivity::Eight)
                .unwrap()
                .len(),
            3
        );
    }
    #[test]
    fn steering_stops_at_target_and_slows_on_arrival() {
        assert_eq!(arrive([1.0, 0.0], [1.0, 0.0], 4.0, 2.0), [0.0; 2]);
        assert_eq!(arrive([0.0; 2], [1.0, 0.0], 4.0, 2.0), [2.0, 0.0]);
        assert_eq!(arrive([0.0; 2], [f32::NAN, 0.0], 4.0, 2.0), [0.0; 2]);
        assert!(separation([0.0; 2], &[[1.0, 0.0]], 2.0, 4.0)[0] < 0.0);
    }
}
