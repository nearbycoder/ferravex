//! Fixed-clock action timelines. Gameplay drives animation, never the reverse.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionError {
    Invalid,
    Capacity,
    Exhausted,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionPhase {
    Idle,
    Windup,
    Active,
    Recovery,
}
#[derive(Clone, Copy, Debug)]
pub struct ActionSpec {
    pub windup: f32,
    pub active: f32,
    pub recovery: f32,
    pub cancel_before: f32,
    pub buffer_seconds: f32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionEdge {
    Started,
    Activated,
    Recovered,
    Finished,
    Cancelled,
}
#[derive(Clone, Copy, Debug)]
pub struct ActionEvent {
    pub edge: ActionEdge,
    pub serial: u64,
    pub offset: f32,
}
#[derive(Clone, Copy, Debug)]
pub struct ActionFrame {
    events: [Option<ActionEvent>; 8],
}
impl ActionFrame {
    pub fn events(&self) -> impl Iterator<Item = ActionEvent> + '_ {
        self.events.iter().flatten().copied()
    }
    pub fn activated(&self) -> bool {
        self.events().any(|e| e.edge == ActionEdge::Activated)
    }
    fn push(&mut self, edge: ActionEdge, serial: u64, offset: f32) {
        let slot = self
            .events
            .iter_mut()
            .find(|v| v.is_none())
            .expect("at most two three-phase actions per step");
        *slot = Some(ActionEvent {
            edge,
            serial,
            offset,
        });
    }
}
#[derive(Clone, Copy, Debug)]
pub struct ActionTimeline {
    spec: ActionSpec,
    phase: ActionPhase,
    elapsed: f64,
    serial: u64,
    buffered: bool,
}
impl ActionTimeline {
    pub fn new(spec: ActionSpec) -> Result<Self, ActionError> {
        if ![
            spec.windup,
            spec.active,
            spec.recovery,
            spec.cancel_before,
            spec.buffer_seconds,
        ]
        .iter()
        .all(|v| v.is_finite() && (0. ..=10.).contains(v))
            || spec.active < 0.0001
            || spec.cancel_before > spec.windup
            || spec.buffer_seconds > spec.recovery.min(0.5)
        {
            return Err(ActionError::Invalid);
        }
        Ok(Self {
            spec,
            phase: ActionPhase::Idle,
            elapsed: 0.,
            serial: 0,
            buffered: false,
        })
    }
    pub fn phase(&self) -> ActionPhase {
        self.phase
    }
    pub fn serial(&self) -> u64 {
        self.serial
    }
    fn duration(&self) -> f64 {
        f64::from(match self.phase {
            ActionPhase::Idle => 0.,
            ActionPhase::Windup => self.spec.windup,
            ActionPhase::Active => self.spec.active,
            ActionPhase::Recovery => self.spec.recovery,
        })
    }
    pub fn progress(&self) -> f32 {
        if self.duration() == 0. {
            0.
        } else {
            (self.elapsed / self.duration()).clamp(0., 1.) as f32
        }
    }
    /// External interruption can obey the authored windup cutoff or force cancellation
    /// on death/stun. Successful cancellation clears queued follow-up actions.
    pub fn cancel(&mut self, force: bool) -> bool {
        if self.phase == ActionPhase::Idle {
            return false;
        }
        if !force
            && (self.phase != ActionPhase::Windup
                || self.elapsed > f64::from(self.spec.cancel_before))
        {
            return false;
        }
        self.phase = ActionPhase::Idle;
        self.elapsed = 0.;
        self.buffered = false;
        true
    }
    fn start(&mut self) -> Result<(), ActionError> {
        self.serial = self.serial.checked_add(1).ok_or(ActionError::Exhausted)?;
        self.phase = ActionPhase::Windup;
        self.elapsed = 0.;
        self.buffered = false;
        Ok(())
    }
    /// At most eight transition reports: an existing action and one buffered action.
    /// Offsets let an authoritative host schedule queries at the proper substep.
    pub fn step(&mut self, dt: f32, trigger: bool) -> Result<ActionFrame, ActionError> {
        if !dt.is_finite() || !(0. ..=0.25).contains(&dt) {
            return Err(ActionError::Invalid);
        }
        let mut frame = ActionFrame { events: [None; 8] };
        if trigger && self.phase == ActionPhase::Idle {
            self.start()?;
            frame.push(ActionEdge::Started, self.serial, 0.);
        } else if trigger
            && self.phase == ActionPhase::Recovery
            && self.duration() - self.elapsed <= f64::from(self.spec.buffer_seconds)
        {
            self.buffered = true;
        }
        let mut left = f64::from(dt);
        let mut offset = 0.;
        for _ in 0..8 {
            if self.phase == ActionPhase::Idle {
                break;
            }
            let remaining = (self.duration() - self.elapsed).max(0.);
            if left + 1e-9 < remaining {
                self.elapsed += left;
                break;
            }
            left = (left - remaining).max(0.);
            offset += remaining;
            self.elapsed = 0.;
            match self.phase {
                ActionPhase::Windup => {
                    self.phase = ActionPhase::Active;
                    frame.push(ActionEdge::Activated, self.serial, offset as f32);
                }
                ActionPhase::Active => {
                    self.phase = ActionPhase::Recovery;
                    frame.push(ActionEdge::Recovered, self.serial, offset as f32);
                }
                ActionPhase::Recovery => {
                    self.phase = ActionPhase::Idle;
                    frame.push(ActionEdge::Finished, self.serial, offset as f32);
                    if self.buffered {
                        self.start()?;
                        frame.push(ActionEdge::Started, self.serial, offset as f32);
                    } else {
                        break;
                    }
                }
                ActionPhase::Idle => break,
            }
        }
        Ok(frame)
    }
}
/// Prevent repeated damage to one generational target during a single action.
#[derive(Clone, Debug)]
pub struct HitWindow<const N: usize> {
    serial: u64,
    open: bool,
    targets: [u64; N],
    len: usize,
}
impl<const N: usize> HitWindow<N> {
    pub fn new() -> Result<Self, ActionError> {
        if N == 0 || N > 128 {
            return Err(ActionError::Capacity);
        }
        Ok(Self {
            serial: 0,
            open: false,
            targets: [0; N],
            len: 0,
        })
    }
    pub fn begin(&mut self, serial: u64) -> bool {
        if serial <= self.serial {
            return false;
        }
        self.serial = serial;
        self.open = true;
        self.len = 0;
        true
    }
    pub fn close(&mut self) {
        self.open = false;
    }
    pub fn accept(&mut self, serial: u64, target: u64) -> Result<bool, ActionError> {
        if target == 0 {
            return Err(ActionError::Invalid);
        }
        if !self.open || serial != self.serial || self.targets[..self.len].contains(&target) {
            return Ok(false);
        }
        if self.len == N {
            return Err(ActionError::Capacity);
        }
        self.targets[self.len] = target;
        self.len += 1;
        Ok(true)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn timeline() -> ActionTimeline {
        ActionTimeline::new(ActionSpec {
            windup: 0.125,
            active: 0.0625,
            recovery: 0.25,
            cancel_before: 0.0625,
            buffer_seconds: 0.125,
        })
        .unwrap()
    }
    #[test]
    fn ordered_edges_offsets_and_partition_independence() {
        let mut t = timeline();
        assert!(!t.step(0.0625, true).unwrap().activated());
        let frame = t.step(0.125, false).unwrap();
        let edges: Vec<_> = frame.events().map(|e| e.edge).collect();
        assert_eq!(edges, [ActionEdge::Activated, ActionEdge::Recovered]);
        assert_eq!(t.phase(), ActionPhase::Recovery);
        let mut a = timeline();
        let mut b = timeline();
        let mut count = 0;
        for i in 0..7 {
            count += b.step(0.0625, i == 0).unwrap().events().count();
        }
        let count_a = a.step(0.25, true).unwrap().events().count()
            + a.step(0.1875, false).unwrap().events().count();
        assert_eq!(count, count_a);
        assert_eq!(a.phase(), b.phase());
        assert_eq!(a.phase(), ActionPhase::Idle);
    }
    #[test]
    fn cancellation_cutoff_and_buffer_do_not_skip_windup() {
        let mut t = timeline();
        t.step(0.03125, true).unwrap();
        assert!(t.cancel(false));
        t.step(0.1, true).unwrap();
        assert!(!t.cancel(false));
        assert!(t.cancel(true));
        t = timeline();
        t.step(0.25, true).unwrap();
        t.step(0.0625, true).unwrap();
        let frame = t.step(0.125, true).unwrap();
        assert!(frame.events().any(|e| e.edge == ActionEdge::Started));
        assert_eq!(t.phase(), ActionPhase::Windup);
        assert_eq!(t.serial(), 2);
    }
    #[test]
    fn target_windows_dedup_serial_and_capacity() {
        let mut w = HitWindow::<1>::new().unwrap();
        assert!(w.begin(1));
        assert!(w.accept(1, 9).unwrap());
        assert!(!w.begin(1));
        assert!(!w.accept(1, 9).unwrap());
        assert_eq!(w.accept(1, 10), Err(ActionError::Capacity));
        w.close();
        assert!(!w.begin(1));
        assert!(w.begin(2));
        assert!(w.accept(2, 9).unwrap());
    }
    #[test]
    fn ability_reservation_commits_only_when_active_and_refunds_cancel() {
        use crate::ability::*;
        let mut bank = AbilityBank::<1>::new(
            1,
            &[AbilitySpec {
                charges: 1,
                recharge_seconds: 1.,
                cost: 5.,
            }],
        )
        .unwrap();
        let mut pool = ResourcePool::new(10., 10., 0.).unwrap();
        let token = bank.begin(0, &mut pool, true).unwrap();
        let mut t = timeline();
        t.step(0.03125, true).unwrap();
        assert!(t.cancel(false));
        bank.cancel(token, &mut pool).unwrap();
        assert_eq!(pool.value(), 10.);
        let token = bank.begin(0, &mut pool, true).unwrap();
        let frame = t.step(0.125, true).unwrap();
        assert!(frame.activated());
        bank.commit(token).unwrap();
        assert_eq!(pool.value(), 5.);
        assert!(bank.commit(token).is_err());
    }
}
