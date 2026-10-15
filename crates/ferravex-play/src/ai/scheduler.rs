/// Round-robin slots over the host's stable dense agent array. Deletion/reordering
/// is allowed between calls. Every remaining index is visited within ceil(n/k)
/// calls while population and allowance stay fixed. No per-call allocation.
#[derive(Clone, Copy, Debug, Default)]
pub struct RoundRobin {
    cursor: usize,
}
impl RoundRobin {
    pub fn reset(&mut self) {
        self.cursor = 0;
    }
    pub fn next(&mut self, population: usize, allowance: usize) -> Slots {
        let count = population.min(allowance);
        let start = if population == 0 {
            0
        } else {
            self.cursor % population
        };
        self.cursor = if population == 0 {
            0
        } else {
            if count >= population - start {
                count - (population - start)
            } else {
                start + count
            }
        };
        Slots {
            start,
            count,
            population,
            emitted: 0,
        }
    }
}
pub struct Slots {
    start: usize,
    count: usize,
    population: usize,
    emitted: usize,
}
impl Iterator for Slots {
    type Item = usize;
    fn next(&mut self) -> Option<usize> {
        if self.emitted == self.count {
            return None;
        }
        // Avoid overflow for arbitrary usize populations.
        let tail = self.population - self.start;
        let value = if self.emitted < tail {
            self.start + self.emitted
        } else {
            self.emitted - tail
        };
        self.emitted += 1;
        Some(value)
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let n = self.count - self.emitted;
        (n, Some(n))
    }
}
impl ExactSizeIterator for Slots {}
