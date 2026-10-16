//! Shared runtime adapter for optional presentation adaptation and local reports.
use crate::{edit, read};
pub use ferravex_play::performance::*;
pub(crate) struct Runtime {
    pub budget: RenderBudget,
    trace: PlaytestTrace<256>,
    csv: String,
    tick: Option<u64>,
    replay: Option<u64>,
}
impl Default for Runtime {
    fn default() -> Self {
        Self {
            budget: RenderBudget::default(),
            trace: PlaytestTrace::new().unwrap(),
            csv: String::new(),
            tick: None,
            replay: None,
        }
    }
}
pub fn context(tick: Option<u64>, replay: Option<u64>) {
    edit(|s| {
        s.performance.tick = tick;
        s.performance.replay = replay;
    });
}
pub fn event(kind: DiagnosticKind, value: f32) -> bool {
    edit(|s| {
        s.performance
            .trace
            .record(PlaytestEvent {
                frame: s.frame,
                tick: s.performance.tick,
                replay: s.performance.replay,
                kind,
                value,
                frame_ms: 0.,
                cpu_ms: 0.,
                gpu_ms: None,
            })
            .is_ok()
    })
}
pub fn scale() -> f32 {
    read(|s| {
        if s.presentation.adaptive {
            s.performance.budget.scale()
        } else {
            1.
        }
    })
}
pub fn sample(frame_ms: f32, cpu_ms: f32, gpu_ms: Option<f32>) -> bool {
    edit(|s| {
        let sample = BudgetSample {
            frame_ms,
            cpu_ms,
            gpu_ms,
            paused: s.controls_open || s.details_open || !s.input.focused,
        };
        if !sample.valid() {
            return false;
        }
        s.performance.budget.set_enabled(s.presentation.adaptive);
        let decision = s.performance.budget.observe(sample).unwrap();
        if decision.changed || s.frame.is_multiple_of(60) || (!sample.paused && frame_ms > 42.) {
            let kind = if decision.changed {
                DiagnosticKind::Fidelity
            } else if frame_ms > 42. {
                DiagnosticKind::Spike
            } else {
                DiagnosticKind::Frame
            };
            let _ = s.performance.trace.record(PlaytestEvent {
                frame: s.frame,
                tick: s.performance.tick,
                replay: s.performance.replay,
                kind,
                value: decision.scale,
                frame_ms,
                cpu_ms,
                gpu_ms,
            });
        }
        if decision.changed {
            s.refresh_environment();
        }
        true
    })
}
pub fn export_csv() -> String {
    edit(|s| {
        s.performance.trace.write_csv(&mut s.performance.csv);
        s.performance.csv.clone()
    })
}
#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::*;
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_adaptive_scale() -> f32 {
        scale()
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_performance_sample(frame_ms: f32, cpu_ms: f32, gpu_ms: f32) -> u32 {
        u32::from(sample(
            frame_ms,
            cpu_ms,
            if gpu_ms < 0. { None } else { Some(gpu_ms) },
        ))
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_diagnostics_export() -> *const u8 {
        edit(|s| {
            s.performance.trace.write_csv(&mut s.performance.csv);
            s.performance.csv.as_ptr()
        })
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_diagnostics_len() -> usize {
        read(|s| s.performance.csv.len())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn adaptation_and_reports_preserve_user_choices() {
        crate::edit(|s| {
            *s = crate::State::default();
            s.input.focused = true;
            s.sun = Some(Default::default());
        });
        crate::preferences::apply(crate::preferences::Preferences {
            adaptive: true,
            resolution: 5,
            shadows: 3,
            ..Default::default()
        });
        let preferences = crate::read(|s| s.presentation);
        assert_eq!(crate::read(|s| s.retained.environment.shadow_size), 1024);
        for i in 0..600 {
            crate::edit(|s| s.frame = i);
            assert!(sample(33., 2., Some(30.)));
        }
        assert_eq!(scale(), 0.5);
        assert_eq!(crate::read(|s| s.presentation), preferences);
        assert_eq!(crate::read(|s| s.retained.environment.shadow_size), 0);
        context(Some(42), Some(99));
        assert!(event(DiagnosticKind::Replay, 1.));
        assert!(export_csv().contains("42,99,replay"));
        crate::preferences::apply(crate::preferences::Preferences {
            adaptive: false,
            ..preferences
        });
        assert_eq!(scale(), 1.);
        assert_eq!(crate::read(|s| s.performance.budget.level()), 0);
        assert_eq!(crate::read(|s| s.retained.environment.shadow_size), 1024);
        crate::preferences::apply(preferences);
        assert_eq!(scale(), 1., "reenabling starts from manual fidelity");
    }
}
