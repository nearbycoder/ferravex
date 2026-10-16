//! Presentation budgets and local playtest traces. Never change simulation dt,
//! authoritative state, input sampling or saved user fidelity preferences.
use std::fmt::Write;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PerformanceError {
    Invalid,
    Capacity,
}
#[derive(Clone, Copy, Debug)]
pub struct BudgetSample {
    pub frame_ms: f32,
    pub cpu_ms: f32,
    pub gpu_ms: Option<f32>,
    pub paused: bool,
}
impl BudgetSample {
    pub fn valid(self) -> bool {
        [Some(self.frame_ms), Some(self.cpu_ms), self.gpu_ms]
            .into_iter()
            .flatten()
            .all(|v| v.is_finite() && (0. ..=10000.).contains(&v))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bottleneck {
    Measuring,
    Cpu,
    Gpu,
    Stable,
    Paused,
}
#[derive(Clone, Copy, Debug)]
pub struct BudgetDecision {
    pub scale: f32,
    pub level: u8,
    pub changed: bool,
    pub bottleneck: Bottleneck,
}
#[derive(Clone, Copy, Debug)]
pub struct RenderBudget {
    target: f32,
    enabled: bool,
    level: u8,
    slow: u16,
    fast: u16,
    cooldown: u16,
    gpu: Option<f32>,
}
impl Default for RenderBudget {
    fn default() -> Self {
        Self::new(60.).unwrap()
    }
}
impl RenderBudget {
    pub fn new(target_fps: f32) -> Result<Self, PerformanceError> {
        if !target_fps.is_finite() || !(15. ..=240.).contains(&target_fps) {
            return Err(PerformanceError::Invalid);
        }
        Ok(Self {
            target: 1000. / target_fps,
            enabled: false,
            level: 0,
            slow: 0,
            fast: 0,
            cooldown: 0,
            gpu: None,
        })
    }
    pub fn set_enabled(&mut self, enabled: bool) {
        if self.enabled != enabled {
            self.enabled = enabled;
            self.level = 0;
            self.slow = 0;
            self.fast = 0;
            self.cooldown = 0;
            self.gpu = None;
        }
    }
    pub fn level(&self) -> u8 {
        self.level
    }
    pub fn scale(&self) -> f32 {
        1. - f32::from(self.level) * 0.125
    }
    /// Only confirmed, valid GPU measurements can lower fidelity. High CPU load,
    /// unavailable timers and background gaps never masquerade as GPU pressure.
    pub fn observe(&mut self, s: BudgetSample) -> Result<BudgetDecision, PerformanceError> {
        if !s.valid() {
            return Err(PerformanceError::Invalid);
        }
        let old = self.level;
        let bottleneck = if s.paused || s.frame_ms > 250. {
            self.slow = 0;
            self.fast = 0;
            self.gpu = None;
            Bottleneck::Paused
        } else if s.cpu_ms > self.target * 0.85 {
            self.slow = 0;
            self.fast = 0;
            self.gpu = None;
            Bottleneck::Cpu
        } else if let Some(gpu) = s.gpu_ms {
            let mean = self.gpu.map_or(gpu, |old| old + (gpu - old) * 0.12);
            self.gpu = Some(mean);
            self.cooldown = self.cooldown.saturating_sub(1);
            if mean > self.target * 1.1 {
                self.slow = self.slow.saturating_add(1);
                self.fast = 0;
                if self.enabled && self.cooldown == 0 && self.slow >= 30 && self.level < 4 {
                    self.level += 1;
                    self.cooldown = 120;
                    self.slow = 0;
                }
                Bottleneck::Gpu
            } else {
                self.slow = 0;
                if mean < self.target * 0.65 {
                    self.fast = self.fast.saturating_add(1);
                } else {
                    self.fast = 0;
                }
                if self.enabled && self.cooldown == 0 && self.fast >= 180 && self.level > 0 {
                    self.level -= 1;
                    self.cooldown = 120;
                    self.fast = 0;
                }
                Bottleneck::Stable
            }
        } else {
            self.slow = 0;
            self.fast = 0;
            self.gpu = None;
            Bottleneck::Measuring
        };
        Ok(BudgetDecision {
            scale: self.scale(),
            level: self.level,
            changed: old != self.level,
            bottleneck,
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticKind {
    Frame,
    Spike,
    Fidelity,
    Fall,
    Objective,
    Death,
    Stuck,
    Replay,
    Completed,
}
impl DiagnosticKind {
    fn label(self) -> &'static str {
        match self {
            Self::Frame => "frame",
            Self::Spike => "spike",
            Self::Fidelity => "fidelity",
            Self::Fall => "fall",
            Self::Objective => "objective",
            Self::Death => "death",
            Self::Stuck => "stuck",
            Self::Replay => "replay",
            Self::Completed => "completed",
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct PlaytestEvent {
    pub frame: u64,
    pub tick: Option<u64>,
    pub replay: Option<u64>,
    pub kind: DiagnosticKind,
    pub value: f32,
    pub frame_ms: f32,
    pub cpu_ms: f32,
    pub gpu_ms: Option<f32>,
}
pub struct PlaytestTrace<const N: usize> {
    events: [Option<PlaytestEvent>; N],
    next: usize,
    len: usize,
    overwritten: u64,
}
impl<const N: usize> PlaytestTrace<N> {
    pub fn new() -> Result<Self, PerformanceError> {
        if N == 0 || N > 4096 {
            return Err(PerformanceError::Capacity);
        }
        Ok(Self {
            events: [None; N],
            next: 0,
            len: 0,
            overwritten: 0,
        })
    }
    pub fn record(&mut self, event: PlaytestEvent) -> Result<(), PerformanceError> {
        if !event.value.is_finite()
            || !(BudgetSample {
                frame_ms: event.frame_ms,
                cpu_ms: event.cpu_ms,
                gpu_ms: event.gpu_ms,
                paused: false,
            })
            .valid()
        {
            return Err(PerformanceError::Invalid);
        }
        if self.len == N {
            self.overwritten = self.overwritten.saturating_add(1);
        }
        self.events[self.next] = Some(event);
        self.next = (self.next + 1) % N;
        self.len = (self.len + 1).min(N);
        Ok(())
    }
    pub fn events(&self) -> impl Iterator<Item = &PlaytestEvent> {
        (0..self.len).map(|i| {
            self.events[(self.next + N - self.len + i) % N]
                .as_ref()
                .unwrap()
        })
    }
    pub fn overwritten(&self) -> u64 {
        self.overwritten
    }
    /// Explicit export may grow the caller's string. Recording itself never allocates.
    pub fn write_csv(&self, out: &mut String) {
        out.clear();
        out.push_str("frame,tick,replay,kind,value,frame_ms,cpu_ms,gpu_ms\n");
        for e in self.events() {
            let _ = write!(out, "{},", e.frame);
            if let Some(t) = e.tick {
                let _ = write!(out, "{t}");
            }
            out.push(',');
            if let Some(r) = e.replay {
                let _ = write!(out, "{r}");
            }
            let _ = write!(
                out,
                ",{},{:.4},{:.4},{:.4},",
                e.kind.label(),
                e.value,
                e.frame_ms,
                e.cpu_ms
            );
            if let Some(g) = e.gpu_ms {
                let _ = write!(out, "{g:.4}");
            }
            out.push('\n');
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gpu_hysteresis_caps_and_manual_restore() {
        let mut b = RenderBudget::default();
        b.set_enabled(true);
        let slow = BudgetSample {
            frame_ms: 35.,
            cpu_ms: 2.,
            gpu_ms: Some(30.),
            paused: false,
        };
        for _ in 0..1000 {
            b.observe(slow).unwrap();
        }
        assert_eq!(b.level(), 4);
        assert_eq!(b.scale(), 0.5);
        for _ in 0..2000 {
            b.observe(BudgetSample {
                gpu_ms: Some(2.),
                ..slow
            })
            .unwrap();
        }
        assert_eq!(b.level(), 0);
        for _ in 0..1000 {
            b.observe(BudgetSample {
                cpu_ms: 30.,
                ..slow
            })
            .unwrap();
        }
        assert_eq!(b.level(), 0);
        for _ in 0..1000 {
            b.observe(BudgetSample {
                gpu_ms: None,
                ..slow
            })
            .unwrap();
        }
        assert_eq!(b.level(), 0);
        b.set_enabled(false);
        assert_eq!(b.scale(), 1.);
    }
    #[test]
    fn pause_and_invalid_data_do_not_degrade() {
        let mut b = RenderBudget::default();
        b.set_enabled(true);
        for _ in 0..1000 {
            b.observe(BudgetSample {
                frame_ms: 1000.,
                cpu_ms: 0.,
                gpu_ms: Some(30.),
                paused: false,
            })
            .unwrap();
        }
        assert_eq!(b.level(), 0);
        assert!(
            b.observe(BudgetSample {
                frame_ms: f32::NAN,
                cpu_ms: 0.,
                gpu_ms: None,
                paused: false
            })
            .is_err()
        );
    }
    #[test]
    fn ring_exports_chronologically_with_replay_context() {
        let mut t = PlaytestTrace::<2>::new().unwrap();
        for frame in 0..3 {
            t.record(PlaytestEvent {
                frame,
                tick: Some(frame * 2),
                replay: Some(77),
                kind: DiagnosticKind::Fall,
                value: 1.,
                frame_ms: 16.,
                cpu_ms: 2.,
                gpu_ms: None,
            })
            .unwrap();
        }
        assert_eq!(t.overwritten(), 1);
        let mut csv = String::new();
        t.write_csv(&mut csv);
        assert!(csv.contains("1,2,77,fall"));
        assert!(!csv.contains("0,0,77,fall"));
    }
}
