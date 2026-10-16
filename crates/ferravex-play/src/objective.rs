//! Authoritative, bounded puzzle/quest DAGs. Feed ordered accepted game events;
//! this module does not validate world distance, visibility or inventory itself.
use crate::persistence::{Document, SaveData, SaveError, Value};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectiveError {
    Invalid,
    Cycle,
    Snapshot,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ObjectiveKind {
    Event { id: u16, count: u16 },
    Timer { seconds: f32 },
    Gate,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObjectiveSpec {
    pub id: u16,
    pub all: u64,
    pub any: u64,
    pub exclusive_group: u8,
    pub kind: ObjectiveKind,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectiveState {
    Locked,
    Ready,
    Complete,
    Skipped,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ObjectiveFrame {
    pub completed: u64,
    pub skipped: u64,
}
#[derive(Clone, Debug)]
pub struct ObjectiveGraph<const N: usize> {
    specs: [ObjectiveSpec; N],
    order: [usize; N],
    saved: ObjectiveSnapshot<N>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ObjectiveSnapshot<const N: usize> {
    fingerprint: u64,
    done: u64,
    skipped: u64,
    last: u64,
    counts: [u16; N],
    elapsed: [f32; N],
}
fn mask<const N: usize>() -> u64 {
    if N == 64 { u64::MAX } else { (1_u64 << N) - 1 }
}
impl<const N: usize> ObjectiveGraph<N> {
    pub fn new(rules: u64, specs: [ObjectiveSpec; N]) -> Result<Self, ObjectiveError> {
        if N == 0 || N > 64 {
            return Err(ObjectiveError::Invalid);
        }
        let mut hash = 0xcbf29ce484222325_u64;
        let mut feed = |bytes: &[u8]| {
            for b in bytes {
                hash = (hash ^ u64::from(*b)).wrapping_mul(0x100000001b3);
            }
        };
        feed(&rules.to_le_bytes());
        for (i, s) in specs.iter().enumerate() {
            if (s.all | s.any) & !mask::<N>() != 0
                || (s.all | s.any) & (1 << i) != 0
                || specs[..i].iter().any(|a| a.id == s.id)
            {
                return Err(ObjectiveError::Invalid);
            }
            feed(&s.id.to_le_bytes());
            feed(&s.all.to_le_bytes());
            feed(&s.any.to_le_bytes());
            feed(&[s.exclusive_group]);
            match s.kind {
                ObjectiveKind::Event { id, count } if count > 0 => {
                    feed(&[0]);
                    feed(&id.to_le_bytes());
                    feed(&count.to_le_bytes());
                }
                ObjectiveKind::Timer { seconds }
                    if seconds.is_finite() && (0.001..=3600.).contains(&seconds) =>
                {
                    feed(&[1]);
                    feed(&seconds.to_le_bytes());
                }
                ObjectiveKind::Gate => feed(&[2]),
                _ => return Err(ObjectiveError::Invalid),
            }
        }
        let mut placed = 0;
        let mut order = [0; N];
        for out in &mut order {
            let i = (0..N)
                .find(|&i| placed & (1 << i) == 0 && (specs[i].all | specs[i].any) & !placed == 0)
                .ok_or(ObjectiveError::Cycle)?;
            *out = i;
            placed |= 1 << i;
        }
        let mut graph = Self {
            specs,
            order,
            saved: ObjectiveSnapshot {
                fingerprint: hash,
                done: 0,
                skipped: 0,
                last: 0,
                counts: [0; N],
                elapsed: [0.; N],
            },
        };
        graph.settle();
        Ok(graph)
    }
    fn requirements(&self, i: usize) -> bool {
        let s = self.specs[i];
        self.saved.done & s.all == s.all && (s.any == 0 || self.saved.done & s.any != 0)
    }
    fn winner(&self, group: u8) -> bool {
        group != 0
            && self
                .specs
                .iter()
                .enumerate()
                .any(|(i, s)| s.exclusive_group == group && self.saved.done & (1 << i) != 0)
    }
    fn settle(&mut self) {
        let mut chosen = [0_u64; 4];
        for (i, s) in self.specs.iter().enumerate() {
            if self.saved.done & (1 << i) != 0 && s.exclusive_group != 0 {
                let g = usize::from(s.exclusive_group);
                chosen[g / 64] |= 1 << (g % 64);
            }
        }
        for i in self.order {
            let bit = 1 << i;
            if self.saved.done & bit != 0 {
                continue;
            }
            let g = usize::from(self.specs[i].exclusive_group);
            if g != 0 && chosen[g / 64] & (1 << (g % 64)) != 0 {
                self.saved.skipped |= bit;
            } else if self.saved.skipped & bit == 0
                && self.requirements(i)
                && self.specs[i].kind == ObjectiveKind::Gate
            {
                self.saved.done |= bit;
                if g != 0 {
                    chosen[g / 64] |= 1 << (g % 64);
                }
            }
        }
    }
    pub fn state(&self, i: usize) -> Option<ObjectiveState> {
        if i >= N {
            return None;
        }
        let bit = 1 << i;
        Some(if self.saved.done & bit != 0 {
            ObjectiveState::Complete
        } else if self.saved.skipped & bit != 0 {
            ObjectiveState::Skipped
        } else if self.requirements(i) {
            ObjectiveState::Ready
        } else {
            ObjectiveState::Locked
        })
    }
    pub fn complete(&self, i: usize) -> bool {
        self.state(i) == Some(ObjectiveState::Complete)
    }
    fn frame(&self, old: (u64, u64)) -> ObjectiveFrame {
        ObjectiveFrame {
            completed: self.saved.done & !old.0,
            skipped: self.saved.skipped & !old.1,
        }
    }
    /// Serials must be strictly increasing within the authoritative event stream.
    /// Older/duplicate deliveries are ignored, including after restoring a save.
    pub fn event(
        &mut self,
        id: u16,
        amount: u16,
        serial: u64,
    ) -> Result<ObjectiveFrame, ObjectiveError> {
        if amount == 0 || serial == 0 {
            return Err(ObjectiveError::Invalid);
        }
        if serial <= self.saved.last {
            return Ok(ObjectiveFrame::default());
        }
        let old = (self.saved.done, self.saved.skipped);
        let ready =
            std::array::from_fn::<_, N, _>(|i| self.state(i) == Some(ObjectiveState::Ready));
        self.saved.last = serial;
        for (i, was_ready) in ready.into_iter().enumerate() {
            if was_ready
                && self.saved.skipped & (1 << i) == 0
                && let ObjectiveKind::Event { id: event, count } = self.specs[i].kind
                && event == id
            {
                self.saved.counts[i] = self.saved.counts[i].saturating_add(amount).min(count);
                if self.saved.counts[i] == count {
                    self.saved.done |= 1 << i;
                    self.settle();
                }
            }
        }
        Ok(self.frame(old))
    }
    /// Timers newly unlocked inside this step begin at its end boundary. Parallel
    /// branch timers already active are resolved by earliest expiry, then slot.
    pub fn advance(&mut self, dt: f32) -> Result<ObjectiveFrame, ObjectiveError> {
        if !dt.is_finite() || !(0. ..=60.).contains(&dt) {
            return Err(ObjectiveError::Invalid);
        }
        let old = (self.saved.done, self.saved.skipped);
        let mut deadlines = [f32::INFINITY; N];
        for (i, deadline) in deadlines.iter_mut().enumerate() {
            if self.state(i) == Some(ObjectiveState::Ready)
                && let ObjectiveKind::Timer { seconds } = self.specs[i].kind
            {
                *deadline = seconds - self.saved.elapsed[i];
                self.saved.elapsed[i] = (self.saved.elapsed[i] + dt).min(seconds);
            }
        }
        let mut order = std::array::from_fn::<_, N, _>(|i| i);
        order.sort_unstable_by(|a, b| deadlines[*a].total_cmp(&deadlines[*b]).then(a.cmp(b)));
        for i in order {
            if deadlines[i] <= dt && self.saved.skipped & (1 << i) == 0 {
                self.saved.done |= 1 << i;
                self.settle();
            }
        }
        Ok(self.frame(old))
    }
    pub fn last_event_serial(&self) -> u64 {
        self.saved.last
    }
    pub fn snapshot(&self) -> ObjectiveSnapshot<N> {
        self.saved.clone()
    }
    pub fn restore(&mut self, saved: ObjectiveSnapshot<N>) -> Result<(), ObjectiveError> {
        if saved.fingerprint != self.saved.fingerprint
            || (saved.done | saved.skipped) & !mask::<N>() != 0
            || saved.done & saved.skipped != 0
        {
            return Err(ObjectiveError::Snapshot);
        }
        let mut next = self.clone();
        next.saved = saved;
        for i in 0..N {
            let done = next.saved.done & (1 << i) != 0;
            let skipped = next.saved.skipped & (1 << i) != 0;
            if skipped && !next.winner(next.specs[i].exclusive_group) {
                return Err(ObjectiveError::Snapshot);
            }
            let count = next.saved.counts[i];
            let elapsed = next.saved.elapsed[i];
            let valid = match next.specs[i].kind {
                ObjectiveKind::Event {
                    count: required, ..
                } => elapsed == 0. && count <= required && (skipped || done == (count == required)),
                ObjectiveKind::Timer { seconds } => {
                    count == 0
                        && elapsed.is_finite()
                        && (0. ..=seconds).contains(&elapsed)
                        && (skipped || done == (elapsed == seconds))
                }
                ObjectiveKind::Gate => count == 0 && elapsed == 0.,
            };
            if !valid || (done || count > 0 || elapsed > 0.) && !next.requirements(i) {
                return Err(ObjectiveError::Snapshot);
            }
            let group = next.specs[i].exclusive_group;
            if done
                && group != 0
                && (0..i).any(|j| {
                    next.specs[j].exclusive_group == group && next.saved.done & (1 << j) != 0
                })
            {
                return Err(ObjectiveError::Snapshot);
            }
        }
        let old = (next.saved.done, next.saved.skipped);
        next.settle();
        if old != (next.saved.done, next.saved.skipped) {
            return Err(ObjectiveError::Snapshot);
        }
        *self = next;
        Ok(())
    }
}
impl<const N: usize> SaveData for ObjectiveSnapshot<N> {
    const SCHEMA: u64 = 0x46584f424a454354;
    const VERSION: u32 = 1;
    const MAX_BYTES: usize = 2048;
    fn write(&self, d: &mut Document) -> Result<(), SaveError> {
        if N == 0 || N > 64 {
            return Err(SaveError::Capacity);
        }
        for (key, v) in [
            ("rules", self.fingerprint),
            ("done", self.done),
            ("skipped", self.skipped),
            ("serial", self.last),
        ] {
            d.set(key, Value::U64(v))?;
        }
        let mut b = Vec::with_capacity(N * 6);
        for i in 0..N {
            b.extend_from_slice(&self.counts[i].to_le_bytes());
            b.extend_from_slice(&self.elapsed[i].to_le_bytes());
        }
        d.set("progress", Value::Bytes(b))
    }
    fn read(d: &Document) -> Result<Self, SaveError> {
        let b = d.bytes("progress")?;
        if N == 0 || N > 64 || b.len() != N * 6 {
            return Err(SaveError::InvalidData);
        }
        let mut result = Self {
            fingerprint: d.u64("rules")?,
            done: d.u64("done")?,
            skipped: d.u64("skipped")?,
            last: d.u64("serial")?,
            counts: [0; N],
            elapsed: [0.; N],
        };
        for (i, b) in b.as_chunks::<6>().0.iter().enumerate() {
            result.counts[i] = u16::from_le_bytes(b[..2].try_into().unwrap());
            result.elapsed[i] = f32::from_le_bytes(b[2..].try_into().unwrap());
        }
        Ok(result)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn node(id: u16, all: u64, kind: ObjectiveKind) -> ObjectiveSpec {
        ObjectiveSpec {
            id,
            all,
            any: 0,
            exclusive_group: 0,
            kind,
        }
    }
    #[test]
    fn dependencies_serials_save_and_restore() {
        let specs = [
            node(1, 0, ObjectiveKind::Event { id: 5, count: 2 }),
            node(2, 1, ObjectiveKind::Event { id: 7, count: 1 }),
            node(3, 2, ObjectiveKind::Gate),
        ];
        let mut g = ObjectiveGraph::new(1, specs).unwrap();
        g.event(7, 1, 1).unwrap();
        assert!(!g.complete(1));
        g.event(5, 1, 2).unwrap();
        let s = g.snapshot();
        let bytes = s.encode(4).unwrap();
        g.event(5, 1, 3).unwrap();
        g.restore(ObjectiveSnapshot::decode(&bytes).unwrap().0)
            .unwrap();
        g.event(5, 1, 2).unwrap();
        assert!(!g.complete(0));
        g.event(5, 1, 3).unwrap();
        g.event(7, 1, 4).unwrap();
        assert!(g.complete(2));
        assert!(
            ObjectiveGraph::new(2, specs)
                .unwrap()
                .restore(g.snapshot())
                .is_err()
        );
    }
    #[test]
    fn earliest_branch_timer_wins_and_cycles_fail() {
        let mut a = node(1, 0, ObjectiveKind::Timer { seconds: 0.2 });
        a.exclusive_group = 1;
        let mut b = node(2, 0, ObjectiveKind::Timer { seconds: 0.1 });
        b.exclusive_group = 1;
        let mut g = ObjectiveGraph::new(1, [a, b]).unwrap();
        g.advance(0.25).unwrap();
        assert_eq!(g.state(0), Some(ObjectiveState::Skipped));
        assert!(g.complete(1));
        let mut restored = ObjectiveGraph::new(1, [a, b]).unwrap();
        assert!(restored.restore(g.snapshot()).is_ok());
        let mut cycle = [a, b];
        cycle[0].all = 2;
        cycle[1].all = 1;
        assert!(matches!(
            ObjectiveGraph::new(1, cycle),
            Err(ObjectiveError::Cycle)
        ));
    }
}
