use super::{Cell, Connectivity, Grid, NavigationError, heuristic};
use crate::ai::WorkBudget;
pub const MAX_SEARCH_CELLS: usize = 4_000_000;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchStatus {
    Idle,
    Searching,
    Tracing,
    Found,
    NoPath,
    Stale,
}
#[derive(Clone, Copy, Debug)]
pub struct SearchProgress {
    pub status: SearchStatus,
    pub work: u32,
}
/// Indexed binary heap: at most one open entry per cell. Scratch, heap and route
/// storage are allocated once; decreases do not insert duplicate heap entries.
pub struct SearchScratch {
    stamps: Vec<u32>,
    distance: Vec<u64>,
    parent: Vec<usize>,
    positions: Vec<usize>,
    open: Vec<usize>,
    route: Vec<Cell>,
    epoch: u32,
    status: SearchStatus,
    width: u32,
    height: u32,
    revision: u64,
    identity: u64,
    start: usize,
    end: usize,
    trace: usize,
    connectivity: Connectivity,
}
impl SearchScratch {
    pub fn new(capacity: usize) -> Result<Self, NavigationError> {
        if capacity == 0 || capacity > MAX_SEARCH_CELLS {
            return Err(NavigationError::Capacity);
        }
        Ok(Self {
            stamps: vec![0; capacity],
            distance: vec![0; capacity],
            parent: vec![usize::MAX; capacity],
            positions: vec![usize::MAX; capacity],
            open: Vec::with_capacity(capacity),
            route: Vec::with_capacity(capacity),
            epoch: 0,
            status: SearchStatus::Idle,
            width: 0,
            height: 0,
            revision: 0,
            identity: 0,
            start: 0,
            end: 0,
            trace: 0,
            connectivity: Connectivity::Four,
        })
    }
    pub fn capacity(&self) -> usize {
        self.stamps.len()
    }
    pub fn status(&self) -> SearchStatus {
        self.status
    }
    /// Invalid requests leave an existing search unchanged. A valid request resets
    /// it without allocation. Generation wrap clears stamps once in 2^32 starts.
    pub fn begin(
        &mut self,
        grid: &Grid,
        start: Cell,
        end: Cell,
        connectivity: Connectivity,
    ) -> Result<(), NavigationError> {
        let start = grid.index(start)?;
        let end = grid.index(end)?;
        if grid.costs.len() > self.capacity() {
            return Err(NavigationError::Capacity);
        }
        if grid.costs[start] == 0 || grid.costs[end] == 0 {
            return Err(NavigationError::BlockedEndpoint);
        }
        self.epoch = self.epoch.wrapping_add(1);
        if self.epoch == 0 {
            self.stamps.fill(0);
            self.epoch = 1;
        }
        self.open.clear();
        self.route.clear();
        self.width = grid.width;
        self.height = grid.height;
        self.revision = grid.revision;
        self.identity = grid.identity;
        self.start = start;
        self.end = end;
        self.trace = end;
        self.connectivity = connectivity;
        self.status = SearchStatus::Searching;
        self.stamps[start] = self.epoch;
        self.distance[start] = 0;
        self.parent[start] = usize::MAX;
        self.positions[start] = 0;
        self.open.push(start);
        Ok(())
    }
    fn cell(&self, index: usize) -> Cell {
        Cell {
            x: index as u32 % self.width,
            y: index as u32 / self.width,
        }
    }
    fn less(&self, a: usize, b: usize) -> bool {
        let end = self.cell(self.end);
        let ea = self.distance[a] + heuristic(self.cell(a), end, self.connectivity);
        let eb = self.distance[b] + heuristic(self.cell(b), end, self.connectivity);
        (ea, self.distance[a], a) < (eb, self.distance[b], b)
    }
    fn swap(&mut self, a: usize, b: usize) {
        self.open.swap(a, b);
        self.positions[self.open[a]] = a;
        self.positions[self.open[b]] = b;
    }
    fn up(&mut self, mut index: usize) {
        while index > 0 {
            let parent = (index - 1) / 2;
            if !self.less(self.open[index], self.open[parent]) {
                break;
            }
            self.swap(index, parent);
            index = parent;
        }
    }
    fn pop(&mut self) -> Option<usize> {
        let first = *self.open.first()?;
        let last = self.open.pop().unwrap();
        self.positions[first] = usize::MAX;
        if !self.open.is_empty() {
            self.open[0] = last;
            self.positions[last] = 0;
            let mut i = 0;
            loop {
                let left = i * 2 + 1;
                if left >= self.open.len() {
                    break;
                }
                let right = left + 1;
                let best =
                    if right < self.open.len() && self.less(self.open[right], self.open[left]) {
                        right
                    } else {
                        left
                    };
                if !self.less(self.open[best], self.open[i]) {
                    break;
                }
                self.swap(i, best);
                i = best;
            }
        }
        Some(first)
    }
    /// One work unit pops/expands one cell (up to eight fixed neighbors), or
    /// reconstructs one parent. Both phases honor the same tick allowance.
    pub fn step(&mut self, grid: &Grid, max_work: u32) -> SearchProgress {
        if matches!(
            self.status,
            SearchStatus::Searching | SearchStatus::Tracing | SearchStatus::Found
        ) && (grid.identity != self.identity
            || grid.width != self.width
            || grid.height != self.height
            || grid.revision != self.revision)
        {
            self.status = SearchStatus::Stale;
            self.route.clear();
        }
        let mut work = 0;
        while work < max_work {
            if self.status == SearchStatus::Tracing {
                work += 1;
                self.route.push(self.cell(self.trace));
                if self.trace == self.start {
                    self.status = SearchStatus::Found;
                    break;
                }
                self.trace = self.parent[self.trace];
                continue;
            }
            if self.status != SearchStatus::Searching {
                break;
            }
            let Some(current) = self.pop() else {
                self.status = SearchStatus::NoPath;
                break;
            };
            work += 1;
            if current == self.end {
                self.trace = current;
                self.status = SearchStatus::Tracing;
                continue;
            }
            let c = self.cell(current);
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
                if diagonal && self.connectivity == Connectivity::Four {
                    continue;
                }
                let x = i64::from(c.x) + dx;
                let y = i64::from(c.y) + dy;
                if x < 0 || y < 0 || x >= i64::from(grid.width) || y >= i64::from(grid.height) {
                    continue;
                }
                let next = Cell {
                    x: x as u32,
                    y: y as u32,
                };
                let index = (next.y * grid.width + next.x) as usize;
                if grid.costs[index] == 0
                    || diagonal
                        && (!grid.walkable(Cell { x: next.x, y: c.y })
                            || !grid.walkable(Cell { x: c.x, y: next.y }))
                {
                    continue;
                }
                let cost = self.distance[current]
                    + u64::from(grid.costs[index]) * if diagonal { 14 } else { 10 };
                if self.stamps[index] != self.epoch || cost < self.distance[index] {
                    let was_open =
                        self.stamps[index] == self.epoch && self.positions[index] != usize::MAX;
                    self.stamps[index] = self.epoch;
                    self.distance[index] = cost;
                    self.parent[index] = current;
                    if !was_open {
                        self.positions[index] = self.open.len();
                        self.open.push(index);
                    }
                    self.up(self.positions[index]);
                }
            }
        }
        SearchProgress {
            status: self.status,
            work,
        }
    }
    pub fn step_budgeted(&mut self, grid: &Grid, budget: &mut WorkBudget) -> SearchProgress {
        let result = self.step(grid, budget.remaining_path_steps());
        assert!(budget.spend_path_steps(result.work));
        result
    }
    /// Goal-to-start order avoids an unbudgeted reversal at completion.
    pub fn reverse_path(&self) -> Option<&[Cell]> {
        (self.status == SearchStatus::Found).then_some(&self.route)
    }
    /// Copying is explicit host work, bounded by the supplied output and path size.
    /// Failure leaves output unchanged.
    pub fn copy_path(&self, output: &mut [Cell]) -> Result<usize, NavigationError> {
        let route = self.reverse_path().ok_or(NavigationError::NoPath)?;
        if route.len() > output.len() {
            return Err(NavigationError::Capacity);
        }
        for (out, cell) in output.iter_mut().zip(route.iter().rev()) {
            *out = *cell;
        }
        Ok(route.len())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PathRequest {
    pub agent: u64,
    pub start: Cell,
    pub end: Cell,
    pub connectivity: Connectivity,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestStatus {
    Queued,
    Updated,
    AlreadyActive,
    Full,
}
/// Bounded FIFO planner. A ready result owns the scratch route until acknowledge;
/// polling never discards it. Agent IDs/generations belong to the host.
pub struct PathPlanner {
    scratch: SearchScratch,
    queue: Vec<PathRequest>,
    capacity: usize,
    active: Option<PathRequest>,
    ready: bool,
    dropped: u64,
    error: Option<NavigationError>,
}
impl PathPlanner {
    pub fn new(cells: usize, requests: usize) -> Result<Self, NavigationError> {
        if requests == 0 || requests > 1024 {
            return Err(NavigationError::Capacity);
        }
        Ok(Self {
            scratch: SearchScratch::new(cells)?,
            queue: Vec::with_capacity(requests),
            capacity: requests,
            active: None,
            ready: false,
            dropped: 0,
            error: None,
        })
    }
    pub fn request(&mut self, request: PathRequest) -> RequestStatus {
        if self.active == Some(request) {
            return RequestStatus::AlreadyActive;
        }
        if let Some(existing) = self.queue.iter_mut().find(|r| r.agent == request.agent) {
            *existing = request;
            return RequestStatus::Updated;
        }
        if self.queue.len() == self.capacity {
            self.dropped = self.dropped.saturating_add(1);
            return RequestStatus::Full;
        }
        self.queue.push(request);
        RequestStatus::Queued
    }
    pub fn step(&mut self, grid: &Grid, budget: &mut WorkBudget) -> SearchProgress {
        if self.ready {
            return if self.error.is_some() {
                SearchProgress {
                    status: SearchStatus::NoPath,
                    work: 0,
                }
            } else {
                self.scratch.step(grid, 0)
            };
        }
        if self.active.is_none() {
            if self.queue.is_empty() || budget.remaining_path_steps() == 0 {
                return SearchProgress {
                    status: SearchStatus::Idle,
                    work: 0,
                };
            }
            let request = self.queue.remove(0);
            self.active = Some(request);
            if let Err(error) =
                self.scratch
                    .begin(grid, request.start, request.end, request.connectivity)
            {
                self.error = Some(error);
                self.ready = true;
                return SearchProgress {
                    status: SearchStatus::NoPath,
                    work: 0,
                };
            }
        }
        let progress = self.scratch.step_budgeted(grid, budget);
        self.ready = matches!(
            progress.status,
            SearchStatus::Found | SearchStatus::NoPath | SearchStatus::Stale
        );
        progress
    }
    pub fn result(&self) -> Option<(PathRequest, Result<&[Cell], NavigationError>)> {
        if !self.ready {
            return None;
        }
        Some((
            self.active.unwrap(),
            if let Some(error) = self.error {
                Err(error)
            } else {
                self.scratch
                    .reverse_path()
                    .ok_or(if self.scratch.status() == SearchStatus::Stale {
                        NavigationError::InvalidInput
                    } else {
                        NavigationError::NoPath
                    })
            },
        ))
    }
    pub fn acknowledge(&mut self) {
        if self.ready {
            self.active = None;
            self.ready = false;
            self.error = None;
        }
    }
    pub fn cancel(&mut self, agent: u64) {
        self.queue.retain(|r| r.agent != agent);
        if self.active.is_some_and(|r| r.agent == agent) {
            self.active = None;
            self.ready = false;
            self.error = None;
        }
    }
    pub fn clear(&mut self) {
        self.queue.clear();
        self.active = None;
        self.ready = false;
        self.error = None;
    }
    pub fn dropped(&self) -> u64 {
        self.dropped
    }
    pub fn pending(&self) -> usize {
        self.queue.len() + usize::from(self.active.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scratch_epoch_wrap_clears_old_stamps_and_capacity_is_bounded() {
        let grid = Grid::new(4, 4).unwrap();
        let mut scratch = SearchScratch::new(16).unwrap();
        scratch.epoch = u32::MAX;
        scratch.stamps.fill(1);
        scratch
            .begin(
                &grid,
                Cell { x: 0, y: 0 },
                Cell { x: 3, y: 3 },
                Connectivity::Eight,
            )
            .unwrap();
        assert_eq!(scratch.step(&grid, 100).status, SearchStatus::Found);
        assert!(scratch.open.len() <= 16);
        assert!(scratch.route.len() <= 16);
        let old = scratch.reverse_path().unwrap().to_vec();
        assert_eq!(
            scratch.begin(
                &grid,
                Cell { x: 8, y: 8 },
                Cell { x: 0, y: 0 },
                Connectivity::Four
            ),
            Err(NavigationError::OutOfBounds)
        );
        assert_eq!(scratch.reverse_path().unwrap(), old);
    }
}
