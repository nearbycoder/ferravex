use super::{Cell, Connectivity, Grid, NavigationError, heuristic};
use crate::ai::WorkBudget;
pub const MAX_SEARCH_CELLS: usize = 4_000_000;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchStatus {
    /// No search is running, including after explicit cancellation.
    Idle,
    /// Cell expansion is pending; another work allowance can resume it.
    Searching,
    /// The goal was reached, but budgeted path reconstruction is pending.
    Tracing,
    Found,
    /// The reachable frontier was exhausted without reaching the goal.
    /// A planner also uses this for rejected requests; consult [`PathPlanner::result`].
    NoPath,
    /// The supplied grid no longer matches the searched grid snapshot.
    Stale,
}
#[derive(Clone, Copy, Debug)]
pub struct SearchProgress {
    pub status: SearchStatus,
    /// Work consumed by this call, including path reconstruction.
    pub work: u32,
}
impl SearchProgress {
    /// Whether a running search yielded because its work allowance ran out.
    ///
    /// This includes a zero allowance while searching or tracing. A terminal
    /// outcome such as [`SearchStatus::NoPath`] takes precedence even when the
    /// call consumed the exact allowance. An idle planner with queued requests
    /// has not started a search and returns `false`.
    ///
    /// ```
    /// use ferravex_play::navigation::{Cell, Connectivity, Grid, SearchScratch, SearchStatus};
    /// let mut grid = Grid::new(3, 1).unwrap();
    /// let mut search = SearchScratch::new(3).unwrap();
    /// search.begin(&grid, Cell { x: 0, y: 0 }, Cell { x: 2, y: 0 }, Connectivity::Four).unwrap();
    /// assert!(search.step(&grid, 1).budget_exhausted());
    /// assert_eq!(search.step(&grid, 5).status, SearchStatus::Found);
    /// grid.set_blocked(Cell { x: 1, y: 0 }, true).unwrap();
    /// search.restart(&grid).unwrap();
    /// let progress = search.step(&grid, 1);
    /// assert_eq!(progress.status, SearchStatus::NoPath);
    /// assert!(!progress.budget_exhausted());
    /// ```
    pub fn budget_exhausted(&self) -> bool {
        matches!(self.status, SearchStatus::Searching | SearchStatus::Tracing)
    }
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
    total_work: u64,
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
            total_work: 0,
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
    /// Work consumed since the last successful [`Self::begin`] or [`Self::restart`].
    ///
    /// Counts both expanded cells and traced parents. Cancellation, stale-grid
    /// detection and rejected requests preserve this count; a fresh search
    /// resets it. This is observed work, not an estimate of work remaining.
    ///
    /// ```
    /// use ferravex_play::navigation::{Cell, Connectivity, Grid, SearchScratch};
    /// let grid = Grid::new(2, 1).unwrap();
    /// let mut search = SearchScratch::new(2).unwrap();
    /// search.begin(&grid, Cell { x: 0, y: 0 }, Cell { x: 1, y: 0 }, Connectivity::Four).unwrap();
    /// search.step(&grid, 1);
    /// search.step(&grid, 1);
    /// assert_eq!(search.total_work(), 2);
    /// search.step(&grid, 2); // Two cells of path reconstruction.
    /// assert_eq!(search.total_work(), 4);
    /// ```
    pub fn total_work(&self) -> u64 {
        self.total_work
    }
    /// Discard pending work and any path, returning whether the status changed.
    ///
    /// Sets the status to [`SearchStatus::Idle`] without allocating or spending
    /// search work. Repeated cancellation returns `false`. The last request and
    /// its [`Self::total_work`] remain available for inspection and restart.
    ///
    /// ```
    /// use ferravex_play::navigation::{Cell, Connectivity, Grid, SearchScratch, SearchStatus};
    /// let grid = Grid::new(2, 1).unwrap();
    /// let mut search = SearchScratch::new(2).unwrap();
    /// search.begin(&grid, Cell { x: 0, y: 0 }, Cell { x: 1, y: 0 }, Connectivity::Four).unwrap();
    /// search.step(&grid, 1);
    /// assert!(search.cancel());
    /// assert!(!search.cancel());
    /// assert_eq!(search.step(&grid, 10).status, SearchStatus::Idle);
    /// assert_eq!(search.total_work(), 1);
    /// assert!(search.reverse_path().is_none());
    /// ```
    pub fn cancel(&mut self) -> bool {
        let changed = self.status != SearchStatus::Idle;
        self.open.clear();
        self.route.clear();
        self.status = SearchStatus::Idle;
        changed
    }
    /// Restart the last successful request against the supplied grid snapshot.
    ///
    /// Reuses the original cell coordinates and connectivity, even if the grid
    /// identity or dimensions changed. This also works after cancellation or a
    /// terminal outcome. Returns [`NavigationError::InvalidInput`] if no request
    /// has ever begun; other errors are the same as [`Self::begin`]. Every error
    /// leaves the existing search and its work count unchanged. Like `begin`,
    /// this reuses storage and only clears stamps on generation wrap.
    ///
    /// ```
    /// use ferravex_play::navigation::{Cell, Connectivity, Grid, NavigationError, SearchScratch};
    /// let mut grid = Grid::new(3, 1).unwrap();
    /// let mut search = SearchScratch::new(3).unwrap();
    /// assert_eq!(search.restart(&grid), Err(NavigationError::InvalidInput));
    /// search.begin(&grid, Cell { x: 0, y: 0 }, Cell { x: 2, y: 0 }, Connectivity::Four).unwrap();
    /// search.step(&grid, 1);
    /// search.cancel();
    /// grid.set_cost(Cell { x: 1, y: 0 }, 2).unwrap();
    /// search.restart(&grid).unwrap();
    /// assert_eq!(search.total_work(), 0);
    /// assert!(search.step(&grid, 1).budget_exhausted());
    /// ```
    pub fn restart(&mut self, grid: &Grid) -> Result<(), NavigationError> {
        if self.epoch == 0 {
            return Err(NavigationError::InvalidInput);
        }
        self.begin(
            grid,
            self.cell(self.start),
            self.cell(self.end),
            self.connectivity,
        )
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
        self.total_work = 0;
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
    /// An empty frontier reports `NoPath` in the same call as its last expansion,
    /// even when that expansion uses the final work unit. A zero allowance does
    /// no expansion or tracing, but still invalidates results from another grid
    /// snapshot, including an earlier `NoPath` result.
    pub fn step(&mut self, grid: &Grid, max_work: u32) -> SearchProgress {
        if matches!(
            self.status,
            SearchStatus::Searching
                | SearchStatus::Tracing
                | SearchStatus::Found
                | SearchStatus::NoPath
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
        if self.status == SearchStatus::Searching && self.open.is_empty() {
            self.status = SearchStatus::NoPath;
        }
        self.total_work += u64::from(work);
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

    fn c(x: u32, y: u32) -> Cell {
        Cell { x, y }
    }

    #[test]
    fn exhaustion_distinguishes_searching_tracing_and_exact_completion() {
        let grid = Grid::new(2, 1).unwrap();
        let mut scratch = SearchScratch::new(2).unwrap();
        assert!(!scratch.step(&grid, 0).budget_exhausted());
        scratch
            .begin(&grid, c(0, 0), c(1, 0), Connectivity::Four)
            .unwrap();
        let zero = scratch.step(&grid, 0);
        assert!(zero.budget_exhausted());
        assert_eq!(zero.work, 0);
        assert_eq!(scratch.total_work(), 0);

        let mut budget = WorkBudget::new(0, 0, 2);
        let progress = scratch.step_budgeted(&grid, &mut budget);
        assert_eq!(progress.status, SearchStatus::Tracing);
        assert!(progress.budget_exhausted());
        assert_eq!(progress.work, 2);
        assert_eq!(budget.remaining_path_steps(), 0);
        assert_eq!(budget.counts().path_steps, 2);
        assert!(scratch.reverse_path().is_none());
        assert_eq!(scratch.step_budgeted(&grid, &mut budget).work, 0);
        assert_eq!(scratch.total_work(), 2);

        let mut budget = WorkBudget::new(0, 0, 1);
        assert!(scratch.step_budgeted(&grid, &mut budget).budget_exhausted());
        assert!(scratch.reverse_path().is_none());
        let mut budget = WorkBudget::new(0, 0, 1);
        let progress = scratch.step_budgeted(&grid, &mut budget);
        assert_eq!(progress.status, SearchStatus::Found);
        assert!(!progress.budget_exhausted());
        assert_eq!(progress.work, 1);
        assert_eq!(scratch.total_work(), 4);
        assert_eq!(scratch.reverse_path(), Some([c(1, 0), c(0, 0)].as_slice()));
        assert_eq!(scratch.step(&grid, u32::MAX).work, 0);
        assert_eq!(scratch.total_work(), 4);
    }

    #[test]
    fn same_cell_request_still_budgets_reconstruction() {
        let grid = Grid::new(1, 1).unwrap();
        let mut scratch = SearchScratch::new(1).unwrap();
        scratch
            .begin(&grid, c(0, 0), c(0, 0), Connectivity::Eight)
            .unwrap();
        assert!(scratch.step(&grid, 1).budget_exhausted());
        assert_eq!(scratch.status(), SearchStatus::Tracing);
        assert!(scratch.step(&grid, 0).budget_exhausted());
        assert_eq!(scratch.step(&grid, 1).status, SearchStatus::Found);
        assert_eq!(scratch.total_work(), 2);
        assert_eq!(scratch.reverse_path(), Some([c(0, 0)].as_slice()));
    }

    #[test]
    fn unreachable_is_reported_on_the_last_allowed_expansion() {
        let mut grid = Grid::new(4, 1).unwrap();
        grid.set_blocked(c(2, 0), true).unwrap();
        let mut scratch = SearchScratch::new(4).unwrap();
        scratch
            .begin(&grid, c(0, 0), c(3, 0), Connectivity::Four)
            .unwrap();
        assert!(scratch.step(&grid, 1).budget_exhausted());
        let mut budget = WorkBudget::new(0, 0, 1);
        let progress = scratch.step_budgeted(&grid, &mut budget);
        assert_eq!(progress.status, SearchStatus::NoPath);
        assert!(!progress.budget_exhausted());
        assert_eq!(progress.work, 1);
        assert_eq!(budget.counts().path_steps, 1);
        assert_eq!(scratch.total_work(), 2);
        assert_eq!(scratch.step(&grid, 0).status, SearchStatus::NoPath);
        assert_eq!(scratch.step(&grid, 100).work, 0);

        scratch.restart(&grid).unwrap();
        let mut budget = WorkBudget::new(0, 0, 100);
        assert_eq!(scratch.step_budgeted(&grid, &mut budget).work, 2);
        assert_eq!(budget.remaining_path_steps(), 98);
        assert_eq!(scratch.total_work(), 2);
    }

    #[test]
    fn cancel_is_idempotent_in_every_phase_and_retains_restart_request() {
        for status in [
            SearchStatus::Searching,
            SearchStatus::Tracing,
            SearchStatus::Found,
            SearchStatus::NoPath,
            SearchStatus::Stale,
        ] {
            let mut grid = Grid::new(3, 1).unwrap();
            let mut scratch = SearchScratch::new(3).unwrap();
            if status == SearchStatus::NoPath {
                grid.set_blocked(c(1, 0), true).unwrap();
            }
            scratch
                .begin(&grid, c(0, 0), c(2, 0), Connectivity::Four)
                .unwrap();
            match status {
                SearchStatus::Searching | SearchStatus::NoPath => {
                    scratch.step(&grid, 1);
                }
                SearchStatus::Tracing => {
                    scratch.step(&grid, 4);
                }
                SearchStatus::Found => {
                    scratch.step(&grid, 6);
                }
                SearchStatus::Stale => {
                    scratch.step(&grid, 1);
                    grid.set_cost(c(1, 0), 2).unwrap();
                    scratch.step(&grid, 0);
                }
                SearchStatus::Idle => unreachable!(),
            }
            assert_eq!(scratch.status(), status);
            let work = scratch.total_work();
            assert!(scratch.cancel());
            assert!(!scratch.cancel());
            assert!(scratch.reverse_path().is_none());
            let mut output = [c(99, 99); 3];
            assert_eq!(scratch.copy_path(&mut output), Err(NavigationError::NoPath));
            assert_eq!(output, [c(99, 99); 3]);
            grid.set_blocked(c(1, 0), false).unwrap();
            let mut budget = WorkBudget::new(0, 0, 100);
            let progress = scratch.step_budgeted(&grid, &mut budget);
            assert_eq!(progress.status, SearchStatus::Idle);
            assert!(!progress.budget_exhausted());
            assert_eq!(progress.work, 0);
            assert_eq!(scratch.total_work(), work);
            assert_eq!(budget.remaining_path_steps(), 100);

            scratch.restart(&grid).unwrap();
            assert_eq!(scratch.total_work(), 0);
            assert_eq!(scratch.step(&grid, 6).status, SearchStatus::Found);
            assert_eq!(scratch.total_work(), 6);
            assert_eq!(scratch.copy_path(&mut output), Ok(3));
            assert_eq!(output, [c(0, 0), c(1, 0), c(2, 0)]);
        }
    }

    #[test]
    fn restart_uses_cell_coordinates_when_grid_dimensions_change() {
        let original = Grid::new(4, 2).unwrap();
        let replacement = Grid::new(3, 3).unwrap();
        let mut scratch = SearchScratch::new(9).unwrap();
        assert!(!scratch.cancel());
        assert_eq!(
            scratch.restart(&original),
            Err(NavigationError::InvalidInput)
        );
        assert_eq!(
            scratch.begin(&original, c(4, 0), c(0, 0), Connectivity::Four),
            Err(NavigationError::OutOfBounds)
        );
        assert_eq!(
            scratch.restart(&original),
            Err(NavigationError::InvalidInput)
        );
        scratch
            .begin(&original, c(1, 1), c(2, 1), Connectivity::Eight)
            .unwrap();
        scratch.step(&original, 1);
        scratch.restart(&replacement).unwrap();
        assert_eq!(scratch.step(&replacement, 4).status, SearchStatus::Found);
        assert_eq!(scratch.reverse_path(), Some([c(2, 1), c(1, 1)].as_slice()));
    }

    #[test]
    fn rejected_restart_preserves_progress_and_completed_route() {
        let grid = Grid::new(3, 2).unwrap();
        let too_small = Grid::new(2, 2).unwrap();
        let too_large = Grid::new(4, 2).unwrap();
        let mut blocked = Grid::new(3, 2).unwrap();
        blocked.set_blocked(c(2, 1), true).unwrap();
        for allowance in [1, 100] {
            let mut scratch = SearchScratch::new(6).unwrap();
            scratch
                .begin(&grid, c(0, 0), c(2, 1), Connectivity::Eight)
                .unwrap();
            scratch.step(&grid, allowance);
            let status = scratch.status();
            let work = scratch.total_work();
            let route = scratch.reverse_path().map(<[Cell]>::to_vec);
            for (replacement, error) in [
                (&too_small, NavigationError::OutOfBounds),
                (&too_large, NavigationError::Capacity),
                (&blocked, NavigationError::BlockedEndpoint),
            ] {
                assert_eq!(scratch.restart(replacement), Err(error));
                assert_eq!(scratch.status(), status);
                assert_eq!(scratch.total_work(), work);
                assert_eq!(scratch.reverse_path(), route.as_deref());
            }
            assert_eq!(scratch.step(&grid, 100).status, SearchStatus::Found);
            let mut output = [c(99, 99); 6];
            let count = scratch.copy_path(&mut output).unwrap();
            assert_eq!(
                output[..count],
                grid.path(c(0, 0), c(2, 1), Connectivity::Eight).unwrap()
            );
            scratch.restart(&grid).unwrap();
            assert_eq!(scratch.total_work(), 0);
            assert_eq!(scratch.status(), SearchStatus::Searching);
        }
    }

    #[test]
    fn unreachable_results_become_stale_without_spending_work() {
        let mut grid = Grid::new(3, 1).unwrap();
        grid.set_blocked(c(1, 0), true).unwrap();
        let replacement = Grid::new(3, 1).unwrap();
        let mut scratch = SearchScratch::new(3).unwrap();
        scratch
            .begin(&grid, c(0, 0), c(2, 0), Connectivity::Four)
            .unwrap();
        assert_eq!(scratch.step(&grid, 1).status, SearchStatus::NoPath);
        let progress = scratch.step(&replacement, 0);
        assert_eq!(progress.status, SearchStatus::Stale);
        assert!(!progress.budget_exhausted());
        assert_eq!(progress.work, 0);
        assert_eq!(scratch.total_work(), 1);

        scratch.restart(&grid).unwrap();
        assert_eq!(scratch.step(&grid, 1).status, SearchStatus::NoPath);
        grid.set_blocked(c(1, 0), false).unwrap();
        let mut budget = WorkBudget::new(0, 0, 10);
        assert_eq!(
            scratch.step_budgeted(&grid, &mut budget).status,
            SearchStatus::Stale
        );
        assert_eq!(budget.remaining_path_steps(), 10);
        assert_eq!(scratch.total_work(), 1);
        scratch.restart(&grid).unwrap();
        assert_eq!(scratch.step(&grid, 6).status, SearchStatus::Found);
    }

    #[test]
    fn restarts_and_budget_splits_preserve_tie_breaking_and_total_work() {
        let mut grid = Grid::new(5, 3).unwrap();
        grid.set_blocked(c(2, 1), true).unwrap();
        grid.set_cost(c(1, 1), 10).unwrap();
        let mut scratch = SearchScratch::new(15).unwrap();
        for connectivity in [Connectivity::Four, Connectivity::Eight] {
            let expected = grid.path(c(0, 1), c(4, 1), connectivity).unwrap();
            // Equal-cost detours choose the lower cell index (the upper row).
            assert_eq!(
                expected[1],
                c(u32::from(connectivity == Connectivity::Eight), 0)
            );
            scratch
                .begin(&grid, c(0, 1), c(4, 1), connectivity)
                .unwrap();
            assert_eq!(scratch.step(&grid, 100).status, SearchStatus::Found);
            let expected_work = scratch.total_work();
            for allowance in [1, 2, 3, 7, 100] {
                scratch.restart(&grid).unwrap();
                let mut consumed = 0;
                for _ in 0..100 {
                    assert_eq!(scratch.step(&grid, 0).work, 0);
                    let mut budget = WorkBudget::new(0, 0, allowance);
                    let progress = scratch.step_budgeted(&grid, &mut budget);
                    assert!(progress.work <= allowance);
                    assert_eq!(progress.work, budget.counts().path_steps);
                    consumed += u64::from(progress.work);
                    assert_eq!(scratch.total_work(), consumed);
                    if !progress.budget_exhausted() {
                        break;
                    }
                }
                assert_eq!(scratch.status(), SearchStatus::Found);
                assert_eq!(scratch.total_work(), expected_work);
                let mut output = [c(99, 99); 15];
                let count = scratch.copy_path(&mut output).unwrap();
                assert_eq!(output[..count], expected);
            }
        }
    }

    #[test]
    fn planner_delivers_unreachable_at_exact_budget_and_invalidates_it_on_edit() {
        let mut grid = Grid::new(3, 1).unwrap();
        grid.set_blocked(c(1, 0), true).unwrap();
        let mut planner = PathPlanner::new(3, 1).unwrap();
        let request = PathRequest {
            agent: 1,
            start: c(0, 0),
            end: c(2, 0),
            connectivity: Connectivity::Four,
        };
        assert_eq!(planner.request(request), RequestStatus::Queued);
        let mut zero = WorkBudget::new(0, 0, 0);
        assert_eq!(planner.step(&grid, &mut zero).status, SearchStatus::Idle);
        assert!(planner.result().is_none());
        let mut budget = WorkBudget::new(0, 0, 1);
        let progress = planner.step(&grid, &mut budget);
        assert_eq!(progress.status, SearchStatus::NoPath);
        assert!(!progress.budget_exhausted());
        assert_eq!(
            planner.result(),
            Some((request, Err(NavigationError::NoPath)))
        );
        grid.set_blocked(c(1, 0), false).unwrap();
        assert_eq!(planner.step(&grid, &mut zero).status, SearchStatus::Stale);
        assert_eq!(
            planner.result(),
            Some((request, Err(NavigationError::InvalidInput)))
        );
        planner.acknowledge();
        assert_eq!(planner.request(request), RequestStatus::Queued);
        let mut budget = WorkBudget::new(0, 0, 6);
        assert_eq!(planner.step(&grid, &mut budget).status, SearchStatus::Found);
        assert_eq!(
            planner.result().unwrap().1.unwrap(),
            [c(2, 0), c(1, 0), c(0, 0)]
        );
    }

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
