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

/// Why the last accepted observation or explicit controller reset held or
/// changed presentation quality. Read through [`RenderBudget::state`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BudgetReason {
    /// Adaptation is disabled; the requested user fidelity is retained.
    Disabled,
    /// An explicit reset restored full scale and discarded measurement history.
    Reset,
    /// Warm-up was started, or this sample seeded the filter without adding evidence.
    WarmingUp,
    /// A local pause discarded measurement history, retaining the current scale.
    Paused,
    /// A frame longer than 250 ms discarded measurement history.
    BackgroundGap,
    /// CPU time exceeded 85% of the target frame budget.
    CpuBound,
    /// No GPU measurement was supplied.
    GpuUnavailable,
    /// A previous quality change is still settling.
    Cooldown,
    /// Current and smoothed GPU times indicate overload; more evidence is needed.
    ConfirmingOverload,
    /// Current and smoothed GPU times indicate headroom; more evidence is needed.
    ConfirmingRecovery,
    /// Neither overload nor recovery has both current and smoothed evidence.
    Monitoring,
    /// GPU pressure persists at the minimum allowed presentation scale.
    AtMinimumScale,
    /// GPU headroom is available at full presentation scale.
    AtFullScale,
    /// Sustained GPU pressure lowered presentation quality by one level.
    LoweredQuality,
    /// Sustained GPU headroom raised presentation quality by one level.
    RaisedQuality,
}

/// An allocation-free snapshot after the last accepted sample or reset.
/// Invalid observations leave every field unchanged. Counters count eligible
/// GPU observations, not simulation ticks, render frames without timings, or
/// elapsed time. See [`RenderBudget::observe`] for thresholds and reset rules.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BudgetState {
    /// Whether automatic quality changes are enabled.
    pub enabled: bool,
    /// Filter-seeding samples still needed, bounded by eight.
    pub warmup_remaining: u16,
    /// Overload evidence, bounded by 30; a nonqualifying sample subtracts two.
    pub overload_score: u16,
    /// Consecutive recovery evidence, bounded by 180.
    pub recovery_samples: u16,
    /// Eligible samples still needed before another change, bounded by 120.
    pub cooldown_remaining: u16,
    /// Exponentially smoothed GPU milliseconds, or `None` after history resets.
    pub smoothed_gpu_ms: Option<f32>,
    /// Reason for the last accepted observation, toggle or explicit reset.
    pub reason: BudgetReason,
}

const WARMUP_SAMPLES: u16 = 8;
const OVERLOAD_SCORE: u16 = 30;
const RECOVERY_SAMPLES: u16 = 180;
const COOLDOWN_SAMPLES: u16 = 120;

#[derive(Clone, Copy, Debug)]
pub struct RenderBudget {
    target: f32,
    enabled: bool,
    level: u8,
    slow: u16,
    fast: u16,
    cooldown: u16,
    gpu: Option<f32>,
    warmup: u16,
    reason: BudgetReason,
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
            warmup: WARMUP_SAMPLES,
            reason: BudgetReason::Disabled,
        })
    }
    /// Changing this setting restores full scale and starts a fresh warm-up.
    /// Repeating the current setting leaves history and the last reason intact.
    pub fn set_enabled(&mut self, enabled: bool) {
        if self.enabled != enabled {
            self.enabled = enabled;
            self.reset();
            self.reason = if enabled {
                BudgetReason::WarmingUp
            } else {
                BudgetReason::Disabled
            };
        }
    }

    /// Restore full presentation scale and begin a new measurement warm-up.
    /// Keeps the target FPS and enabled setting. Use after a scene/restart or
    /// timing-source change; it does not touch simulation or user preferences.
    ///
    /// ```
    /// use ferravex_play::performance::{BudgetReason, BudgetSample, RenderBudget};
    ///
    /// let mut budget = RenderBudget::new(60.)?;
    /// budget.set_enabled(true);
    /// let sample = BudgetSample {
    ///     frame_ms: 35., cpu_ms: 2., gpu_ms: Some(30.), paused: false,
    /// };
    /// for _ in 0..38 { // Eight warm-up samples, then 30 overload observations.
    ///     budget.observe(sample)?;
    /// }
    /// assert_eq!(budget.level(), 1);
    /// assert_eq!(budget.state().reason, BudgetReason::LoweredQuality);
    /// budget.reset();
    /// assert_eq!(budget.scale(), 1.);
    /// assert!(budget.state().enabled);
    /// assert_eq!(budget.state().reason, BudgetReason::Reset);
    /// assert_eq!(budget.state().warmup_remaining, 8);
    /// # Ok::<(), ferravex_play::performance::PerformanceError>(())
    /// ```
    pub fn reset(&mut self) {
        self.level = 0;
        self.cooldown = 0;
        self.clear_measurements();
        self.reason = BudgetReason::Reset;
    }

    /// Inspect warm-up, hysteresis and the last reason without consuming data.
    /// [`BudgetDecision`] and the existing observation API remain unchanged.
    ///
    /// ```
    /// use ferravex_play::performance::{BudgetReason, BudgetSample, RenderBudget};
    ///
    /// let mut budget = RenderBudget::default();
    /// budget.set_enabled(true);
    /// let decision = budget.observe(BudgetSample {
    ///     frame_ms: 16., cpu_ms: 2., gpu_ms: Some(8.), paused: false,
    /// })?;
    /// assert!(!decision.changed);
    /// let state = budget.state();
    /// assert_eq!(state.reason, BudgetReason::WarmingUp);
    /// assert_eq!(state.warmup_remaining, 7);
    /// assert_eq!(state.smoothed_gpu_ms, Some(8.));
    /// # Ok::<(), ferravex_play::performance::PerformanceError>(())
    /// ```
    pub fn state(&self) -> BudgetState {
        BudgetState {
            enabled: self.enabled,
            warmup_remaining: self.warmup,
            overload_score: self.slow,
            recovery_samples: self.fast,
            cooldown_remaining: self.cooldown,
            smoothed_gpu_ms: self.gpu,
            reason: self.reason,
        }
    }

    fn clear_measurements(&mut self) {
        self.slow = 0;
        self.fast = 0;
        self.gpu = None;
        self.warmup = WARMUP_SAMPLES;
    }

    pub fn level(&self) -> u8 {
        self.level
    }
    pub fn scale(&self) -> f32 {
        1. - f32::from(self.level) * 0.125
    }
    /// Only confirmed, valid GPU measurements can lower fidelity. High CPU load,
    /// unavailable timers and background gaps never masquerade as GPU pressure.
    ///
    /// The first eight consecutive eligible samples seed the exponential filter
    /// (weight 0.12). Subsequent samples add one overload point only when both
    /// current and smoothed GPU times exceed 110% of the target frame budget;
    /// otherwise two points decay. Thirty points lower quality one level.
    /// Recovery needs 180 consecutive samples with both times below 65%.
    /// Each change starts a 120-eligible-sample cooldown; evidence may accumulate
    /// during it. Scales remain bounded to 1.0 through 0.5 in steps of 0.125.
    ///
    /// Paused, CPU-bound, missing-GPU and over-250-ms frames discard filter and
    /// evidence history and require fresh warm-up, retaining scale and cooldown.
    /// They do not advance the cooldown. Invalid data returns an error without
    /// mutation. [`Self::state`] explains the decision; no clocks are consulted.
    pub fn observe(&mut self, s: BudgetSample) -> Result<BudgetDecision, PerformanceError> {
        if !s.valid() {
            return Err(PerformanceError::Invalid);
        }
        let old = self.level;
        let bottleneck = if s.paused || s.frame_ms > 250. {
            self.clear_measurements();
            self.reason = if s.paused {
                BudgetReason::Paused
            } else {
                BudgetReason::BackgroundGap
            };
            Bottleneck::Paused
        } else if s.cpu_ms > self.target * 0.85 {
            self.clear_measurements();
            self.reason = BudgetReason::CpuBound;
            Bottleneck::Cpu
        } else if let Some(gpu) = s.gpu_ms {
            self.observe_gpu(gpu)
        } else {
            self.clear_measurements();
            self.reason = BudgetReason::GpuUnavailable;
            Bottleneck::Measuring
        };
        Ok(BudgetDecision {
            scale: self.scale(),
            level: self.level,
            changed: old != self.level,
            bottleneck,
        })
    }

    fn observe_gpu(&mut self, gpu: f32) -> Bottleneck {
        let mean = self.gpu.map_or(gpu, |old| old + (gpu - old) * 0.12);
        self.gpu = Some(mean);
        self.cooldown = self.cooldown.saturating_sub(1);
        let bottleneck = if mean > self.target * 1.1 {
            Bottleneck::Gpu
        } else {
            Bottleneck::Stable
        };
        if self.warmup > 0 {
            self.warmup -= 1;
            self.reason = if self.enabled {
                BudgetReason::WarmingUp
            } else {
                BudgetReason::Disabled
            };
            return bottleneck;
        }

        let overloaded = gpu > self.target * 1.1 && mean > self.target * 1.1;
        let recovering = gpu < self.target * 0.65 && mean < self.target * 0.65;
        self.slow = if overloaded {
            (self.slow + 1).min(OVERLOAD_SCORE)
        } else {
            self.slow.saturating_sub(2)
        };
        self.fast = if recovering {
            (self.fast + 1).min(RECOVERY_SAMPLES)
        } else {
            0
        };
        self.reason = if !self.enabled {
            BudgetReason::Disabled
        } else if self.cooldown > 0 {
            BudgetReason::Cooldown
        } else if overloaded && self.level == 4 {
            BudgetReason::AtMinimumScale
        } else if self.slow == OVERLOAD_SCORE {
            self.level += 1;
            self.quality_changed();
            BudgetReason::LoweredQuality
        } else if recovering && self.level == 0 {
            BudgetReason::AtFullScale
        } else if self.fast == RECOVERY_SAMPLES {
            self.level -= 1;
            self.quality_changed();
            BudgetReason::RaisedQuality
        } else if overloaded {
            BudgetReason::ConfirmingOverload
        } else if recovering {
            BudgetReason::ConfirmingRecovery
        } else {
            BudgetReason::Monitoring
        };
        bottleneck
    }

    fn quality_changed(&mut self) {
        self.cooldown = COOLDOWN_SAMPLES;
        self.slow = 0;
        self.fast = 0;
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

    fn gpu_sample(gpu_ms: f32) -> BudgetSample {
        BudgetSample {
            frame_ms: 35.,
            cpu_ms: 2.,
            gpu_ms: Some(gpu_ms),
            paused: false,
        }
    }

    #[test]
    fn transient_spikes_do_not_lower_quality() {
        let mut budget = RenderBudget::default();
        budget.set_enabled(true);
        // Exercise a cold spike, then a short burst after measurements settle.
        for (count, gpu_ms) in [(1, 10000.), (240, 2.), (3, 10000.), (240, 2.)] {
            for sample in 0..count {
                let decision = budget.observe(gpu_sample(gpu_ms)).unwrap();
                assert_eq!(decision.level, 0, "GPU {gpu_ms} ms, sample {sample}");
                assert!(!decision.changed);
            }
        }
    }

    #[test]
    fn periodic_spikes_and_threshold_jitter_stay_at_full_scale() {
        let mut budget = RenderBudget::default();
        budget.set_enabled(true);
        for frame in 0..2000 {
            let gpu_ms = if frame % 30 == 0 { 10000. } else { 16. };
            assert!(!budget.observe(gpu_sample(gpu_ms)).unwrap().changed);
            assert!(budget.state().overload_score <= 1);
        }
        budget.reset();
        for frame in 0..2000 {
            // Both sides of the 18.33-ms overload boundary, with no net pressure.
            let gpu_ms = if frame % 2 == 0 { 18.5 } else { 18.2 };
            assert!(!budget.observe(gpu_sample(gpu_ms)).unwrap().changed);
        }
        assert_eq!(budget.level(), 0);
    }

    #[test]
    fn sustained_overload_and_recovery_have_a_deterministic_trace() {
        let mut budget = RenderBudget::default();
        budget.set_enabled(true);
        let mut changes = Vec::new();
        for frame in 1..=1230 {
            let gpu_ms = if frame <= 430 { 30. } else { 2. };
            let decision = budget.observe(gpu_sample(gpu_ms)).unwrap();
            let state = budget.state();
            assert!(state.overload_score <= 30);
            assert!(state.recovery_samples <= 180);
            assert!(state.cooldown_remaining <= 120);
            assert!((0.5..=1.).contains(&decision.scale));
            if decision.changed {
                changes.push((frame, decision.level, state.reason));
                assert_eq!(state.cooldown_remaining, 120);
                assert_eq!(state.overload_score, 0);
                assert_eq!(state.recovery_samples, 0);
            }
        }
        assert_eq!(
            changes,
            [
                (38, 1, BudgetReason::LoweredQuality),
                (158, 2, BudgetReason::LoweredQuality),
                (278, 3, BudgetReason::LoweredQuality),
                (398, 4, BudgetReason::LoweredQuality),
                (619, 3, BudgetReason::RaisedQuality),
                (799, 2, BudgetReason::RaisedQuality),
                (979, 1, BudgetReason::RaisedQuality),
                (1159, 0, BudgetReason::RaisedQuality),
            ]
        );
    }

    #[test]
    fn brief_healthy_dips_do_not_hide_sustained_overload() {
        let mut budget = RenderBudget::default();
        budget.set_enabled(true);
        for _ in 0..8 {
            budget.observe(gpu_sample(30.)).unwrap();
        }
        let mut changes = Vec::new();
        for sample in 1..=600 {
            // One healthy sample in ten decays pressure instead of erasing it.
            let gpu_ms = if sample % 10 == 0 { 16. } else { 30. };
            if budget.observe(gpu_sample(gpu_ms)).unwrap().changed {
                changes.push((sample, budget.level()));
            }
        }
        assert_eq!(changes, [(39, 1), (159, 2), (279, 3), (399, 4)]);
        assert_eq!(budget.state().reason, BudgetReason::Monitoring);
        budget.observe(gpu_sample(30.)).unwrap();
        assert_eq!(budget.state().reason, BudgetReason::AtMinimumScale);
    }

    #[test]
    fn warmup_reset_and_toggles_are_observable() {
        let mut budget = RenderBudget::new(120.).unwrap();
        assert_eq!(budget.state().reason, BudgetReason::Disabled);
        assert!(!budget.state().enabled);
        budget.set_enabled(true);
        for remaining in (0..8).rev() {
            assert!(!budget.observe(gpu_sample(10.)).unwrap().changed);
            let state = budget.state();
            assert_eq!(state.reason, BudgetReason::WarmingUp);
            assert_eq!(state.warmup_remaining, remaining);
            assert_eq!(state.overload_score, 0);
            budget.set_enabled(true);
            assert_eq!(budget.state(), state, "same setting must not reset history");
        }
        budget.observe(gpu_sample(10.)).unwrap();
        assert_eq!(budget.state().reason, BudgetReason::ConfirmingOverload);
        assert_eq!(budget.state().overload_score, 1);
        for _ in 0..29 {
            budget.observe(gpu_sample(10.)).unwrap();
        }
        assert_eq!(budget.level(), 1);
        budget.reset();
        assert_eq!(budget.level(), 0);
        assert_eq!(budget.scale(), 1.);
        assert_eq!(
            budget.state(),
            BudgetState {
                enabled: true,
                warmup_remaining: 8,
                overload_score: 0,
                recovery_samples: 0,
                cooldown_remaining: 0,
                smoothed_gpu_ms: None,
                reason: BudgetReason::Reset,
            }
        );
        for _ in 0..38 {
            budget.observe(gpu_sample(10.)).unwrap();
        }
        assert_eq!(budget.level(), 1, "reset must preserve the 120-FPS target");
        budget.set_enabled(false);
        assert_eq!(budget.scale(), 1.);
        assert_eq!(budget.state().warmup_remaining, 8);
        assert_eq!(budget.state().cooldown_remaining, 0);
        for _ in 0..1000 {
            assert!(!budget.observe(gpu_sample(30.)).unwrap().changed);
            assert_eq!(budget.state().reason, BudgetReason::Disabled);
        }
        budget.reset();
        assert!(
            !budget.state().enabled,
            "reset must preserve the disabled setting"
        );
        budget.set_enabled(true);
        assert_eq!(budget.state().warmup_remaining, 8);
        assert_eq!(budget.state().smoothed_gpu_ms, None);
    }

    #[test]
    fn ineligible_samples_restart_warmup_and_freeze_cooldown() {
        let mut baseline = RenderBudget::default();
        baseline.set_enabled(true);
        for _ in 0..38 {
            baseline.observe(gpu_sample(30.)).unwrap();
        }
        for (sample, reason, bottleneck) in [
            (
                BudgetSample {
                    paused: true,
                    ..gpu_sample(30.)
                },
                BudgetReason::Paused,
                Bottleneck::Paused,
            ),
            (
                BudgetSample {
                    frame_ms: 251.,
                    ..gpu_sample(30.)
                },
                BudgetReason::BackgroundGap,
                Bottleneck::Paused,
            ),
            (
                BudgetSample {
                    cpu_ms: 30.,
                    ..gpu_sample(30.)
                },
                BudgetReason::CpuBound,
                Bottleneck::Cpu,
            ),
            (
                BudgetSample {
                    gpu_ms: None,
                    ..gpu_sample(30.)
                },
                BudgetReason::GpuUnavailable,
                Bottleneck::Measuring,
            ),
        ] {
            let mut budget = baseline;
            for _ in 0..200 {
                let decision = budget.observe(sample).unwrap();
                assert_eq!(decision.bottleneck, bottleneck);
                assert_eq!(decision.level, 1);
                assert!(!decision.changed);
                let state = budget.state();
                assert_eq!(state.reason, reason);
                assert_eq!(state.warmup_remaining, 8);
                assert_eq!(state.cooldown_remaining, 120);
                assert_eq!(state.overload_score, 0);
                assert_eq!(state.recovery_samples, 0);
                assert_eq!(state.smoothed_gpu_ms, None);
            }
            budget.observe(gpu_sample(30.)).unwrap();
            assert_eq!(budget.state().reason, BudgetReason::WarmingUp);
            assert_eq!(budget.state().warmup_remaining, 7);
            assert_eq!(budget.state().cooldown_remaining, 119);
            assert_eq!(budget.state().overload_score, 0);
        }
    }

    #[test]
    fn invalid_observations_are_atomic() {
        let mut budget = RenderBudget::default();
        budget.set_enabled(true);
        for _ in 0..40 {
            budget.observe(gpu_sample(30.)).unwrap();
        }
        let state = budget.state();
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -1., 10001.] {
            for sample in [
                BudgetSample {
                    frame_ms: invalid,
                    ..gpu_sample(30.)
                },
                BudgetSample {
                    cpu_ms: invalid,
                    ..gpu_sample(30.)
                },
                gpu_sample(invalid),
                BudgetSample {
                    paused: true,
                    ..gpu_sample(invalid)
                },
            ] {
                assert!(matches!(
                    budget.observe(sample),
                    Err(PerformanceError::Invalid)
                ));
                assert_eq!(budget.state(), state);
                assert_eq!(budget.level(), 1);
            }
        }
    }

    #[test]
    fn recovery_requires_current_headroom_and_explains_holds() {
        let mut budget = RenderBudget::default();
        budget.set_enabled(true);
        for _ in 0..38 {
            budget.observe(gpu_sample(30.)).unwrap();
        }
        budget.observe(gpu_sample(2.)).unwrap();
        assert_eq!(budget.state().reason, BudgetReason::Cooldown);
        for _ in 0..120 {
            budget.observe(gpu_sample(2.)).unwrap();
        }
        assert_eq!(budget.state().reason, BudgetReason::ConfirmingRecovery);
        for _ in 0..600 {
            // A healthy mean alone must not restore quality during these spikes.
            for _ in 0..2 {
                assert!(!budget.observe(gpu_sample(2.)).unwrap().changed);
            }
            assert!(!budget.observe(gpu_sample(20.)).unwrap().changed);
            assert_eq!(budget.state().recovery_samples, 0);
        }
        assert_eq!(budget.level(), 1);
        for _ in 0..180 {
            budget.observe(gpu_sample(2.)).unwrap();
        }
        assert_eq!(budget.level(), 0);
        assert_eq!(budget.state().reason, BudgetReason::RaisedQuality);
        for _ in 0..120 {
            budget.observe(gpu_sample(2.)).unwrap();
        }
        assert_eq!(budget.state().reason, BudgetReason::AtFullScale);
    }

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
