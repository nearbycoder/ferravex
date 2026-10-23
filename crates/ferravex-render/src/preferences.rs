//! Validated live presentation settings, separate from persistence and simulation.
//!
//! [`try_apply`] changes audio/graphics without saving; [`try_apply_with_feedback`] also
//! publishes a visual policy. Games opt in with [`sync_feedback`] before emitting
//! accepted events and before sampling presentation, including while a local menu
//! is open. Neither operation advances or pauses gameplay, networking or replay.
//! Motion/flash policy is session-local: the existing preferences save format is
//! unchanged. Hosts own persistence and resolution of system motion vs explicit
//! user choices. Renderer state, audio and this policy belong to the calling thread.
use crate::{
    audio,
    lighting::Quality,
    storage::{LoadStatus, PlatformStore, Slot, StorageError},
};
pub use ferravex_play::persistence::Preferences;
use ferravex_play::{
    feedback::{FeedbackController, FeedbackPolicy},
    ui::theme::{Spacing, ThemeError, ThemeMode, UiTheme},
};
use std::cell::Cell;

thread_local! {
    static FEEDBACK_POLICY: Cell<FeedbackPolicy> = Cell::new(FeedbackPolicy::default());
}

/// Version of the session-local UI snapshot and browser setter contract.
pub const UI_PREFERENCES_VERSION: u32 = 1;

/// Validated UI choices, independent of motion/flash, graphics and saved preferences.
/// Private fields prevent an invalid value from entering the live session.
/// Standard text is 1x; the player's existing Large choice maps to 1.5x.
/// Explicit contrast indices are 0 (Normal) and 1 (HighContrast), never "system".
/// No game is restyled until it opts into [`ui_preferences`] and [`Self::theme`].
///
/// ```
/// use ferravex_render::preferences::{self, UiPreferences};
/// use ferravex_play::ui::theme::ThemeMode;
/// let large = UiPreferences::from_snapshot(1, 1.5, 1)?;
/// assert_eq!((large.version(), large.text_scale(), large.contrast()), (1, 1.5, 1));
/// let changed = preferences::apply_ui_preferences(large);
/// assert!(changed.changed && changed.reflow_required);
/// let theme = preferences::ui_preferences().theme()?;
/// assert_eq!(theme.mode(), ThemeMode::HighContrast);
/// assert_eq!(theme.types().body, 27.);
/// assert_eq!(theme.spacing().control_height, 66.);
/// assert!(!preferences::apply_ui_preferences(large).changed);
/// preferences::apply_ui_preferences(UiPreferences::default()); // Session reset.
/// # Ok::<(), preferences::UiPreferenceError>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiPreferences {
    text_scale: f32,
    contrast: u32,
}
impl Default for UiPreferences {
    fn default() -> Self {
        Self {
            text_scale: 1.,
            contrast: 0,
        }
    }
}

/// Rejected snapshots leave every live channel unchanged.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UiPreferenceError {
    Version,
    TextScale,
    Contrast,
    Theme(ThemeError),
}
impl std::fmt::Display for UiPreferenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "UI preferences: {self:?}")
    }
}
impl std::error::Error for UiPreferenceError {}

impl UiPreferences {
    /// Validate every field before constructing a snapshot. Scale must be finite
    /// in `1..=2`; essential text is never shrunk to fit. See the type example.
    pub fn from_snapshot(
        version: u32,
        text_scale: f32,
        contrast: u32,
    ) -> Result<Self, UiPreferenceError> {
        if version != UI_PREFERENCES_VERSION {
            return Err(UiPreferenceError::Version);
        }
        if !text_scale.is_finite() || !(1. ..=2.).contains(&text_scale) {
            return Err(UiPreferenceError::TextScale);
        }
        if contrast > 1 {
            return Err(UiPreferenceError::Contrast);
        }
        let value = Self {
            text_scale,
            contrast,
        };
        value.theme()?; // Validate tokens, including palette/metrics, before adoption.
        Ok(value)
    }
    pub fn version(self) -> u32 {
        UI_PREFERENCES_VERSION
    }
    pub fn text_scale(self) -> f32 {
        self.text_scale
    }
    pub fn contrast(self) -> u32 {
        self.contrast
    }

    /// Derive validated UiTheme tokens in logical screen units. Both modes use
    /// their explicit authored preset; unknown contrast never selects a fallback.
    /// Type sizes, insets, gaps, control height and stroke enlarge together.
    /// Recompute measured text, flow/scroll bounds and painted/hit rectangles
    /// from this result before accepting input. Do not scale a camera/viewport
    /// or use font shrinking/ellipsis for essential labels. Existing fixed-style
    /// CardLayout does not consume these tokens; compose styled Ui controls instead.
    /// Custom software hosts may consume this pure theme through their own painter.
    /// See `tests/journey400_295.rs` for an executed live Ui/flow/touch recipe.
    pub fn theme(self) -> Result<UiTheme, UiPreferenceError> {
        let mode = match self.contrast {
            0 => ThemeMode::Normal,
            1 => ThemeMode::HighContrast,
            _ => return Err(UiPreferenceError::Contrast),
        };
        let base = UiTheme::preset(mode);
        let types = base
            .types()
            .try_scaled(self.text_scale)
            .map_err(UiPreferenceError::Theme)?;
        let spacing = base.spacing();
        UiTheme::try_new(
            mode,
            types,
            Spacing {
                inset: spacing.inset * self.text_scale,
                gap: spacing.gap * self.text_scale,
                compact_gap: spacing.compact_gap * self.text_scale,
                control_height: spacing.control_height * self.text_scale,
                stroke: spacing.stroke * self.text_scale,
            },
            base.palette(),
        )
        .map_err(UiPreferenceError::Theme)
    }
}

/// Result for the separate UI channel. A token change requires layout/paint/hit
/// reconstruction, including a contrast-only change (authored styles may differ).
/// This reports adoption by the session, not adoption by any game/renderer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UiApplyOutcome {
    pub changed: bool,
    pub reflow_required: bool,
}
thread_local! {
    static UI_PREFERENCES: Cell<UiPreferences> = Cell::new(UiPreferences::default());
}
/// Read the calling thread's session-local UI choice. See [`UiPreferences`].
pub fn ui_preferences() -> UiPreferences {
    UI_PREFERENCES.with(Cell::get)
}

/// Publish a validated UI choice without persistence, renderer rebuild or gameplay
/// mutation. Consumers rebuild their own layout from [`UiPreferences::theme`].
/// Use `UiPreferences::default()` for an explicit UI-only reset; see its example.
pub fn apply_ui_preferences(next: UiPreferences) -> UiApplyOutcome {
    let changed = UI_PREFERENCES.with(|value| value.replace(next) != next);
    UiApplyOutcome {
        changed,
        reflow_required: changed,
    }
}

/// Validate/apply the versioned browser contract. Numeric 1 accepts (also when
/// unchanged); 0 rejects without any side effects, including last-field failure.
/// This deliberately excludes motion/flash: old feedback consumers stay independent.
///
/// ```
/// use ferravex_render::preferences::*;
/// assert_eq!(apply_ui_snapshot_v1(1, 1.5, 1), 1);
/// let previous = ui_preferences();
/// assert_eq!(apply_ui_snapshot_v1(1, 2., 2), 0);
/// assert_eq!(ui_preferences(), previous);
/// assert_eq!(apply_ui_snapshot_v1(1, 1., 0), 1);
/// ```
pub fn apply_ui_snapshot_v1(version: u32, text_scale: f32, contrast: u32) -> u32 {
    let Ok(next) = UiPreferences::from_snapshot(version, text_scale, contrast) else {
        return 0;
    };
    apply_ui_preferences(next);
    1
}

/// Result of applying a complete validated preference snapshot.
///
/// Change flags compare actual runtime settings, including mixer changes made
/// outside this module. `graphics_changed` includes underlying user choices even
/// when a URL/environment override hides them. `quality` and `look` report the
/// effective result with those overrides still in force. No flag implies a save,
/// renderer rebuild, or that a game has called [`sync_feedback`].
///
/// ```
/// use ferravex_render::preferences::{self, Preferences};
/// let next = Preferences { master: 0.25, quality: Some(2), ..Default::default() };
/// let result = preferences::try_apply(next).unwrap();
/// assert_eq!(result.quality, ferravex_render::lighting::Quality::High);
/// assert!(!preferences::try_apply(next).unwrap().changed());
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ApplyOutcome {
    pub audio_changed: bool,
    pub graphics_changed: bool,
    pub feedback_changed: bool,
    pub quality: Quality,
    pub look: crate::Look,
}
impl ApplyOutcome {
    /// Whether any underlying audio, graphics or shared feedback setting changed.
    ///
    /// ```
    /// use ferravex_render::preferences;
    /// let result = preferences::try_apply(preferences::current()).unwrap();
    /// assert!(!result.changed());
    /// ```
    pub fn changed(self) -> bool {
        self.audio_changed || self.graphics_changed || self.feedback_changed
    }
}

/// Snapshot actual audio and underlying graphics settings, excluding temporary
/// URL/environment look/quality overrides and queued, unapplied detail edits.
/// This is not the saved user snapshot: direct mixer/renderer calls are visible.
///
/// ```
/// use ferravex_render::{audio, preferences};
/// audio::set_master_gain(0.4);
/// assert_eq!(preferences::current().master, 0.4);
/// ```
pub fn current() -> Preferences {
    crate::read(|s| Preferences {
        master: audio::master_gain(),
        buses: audio::bus_gains(),
        muted: audio::muted(),
        quality: s.preferred_quality.map(|q| q as u8),
        look: s.look as u8,
        resolution: s.resolution,
        shadows: s.shadow_detail,
        lights: s.light_detail,
        adaptive: s.performance.budget.state().enabled,
    })
}

/// Current session-local visual policy. Hosts can inspect it to suppress their
/// own animation/particles; already emitted external effects cannot be recalled.
///
/// ```
/// assert_eq!(ferravex_render::preferences::feedback_policy().intensity(), 1.);
/// ```
pub fn feedback_policy() -> FeedbackPolicy {
    FEEDBACK_POLICY.with(Cell::get)
}

/// Publish an already validated visual policy; returns whether it changed.
/// Does not write preferences, change audio/graphics, or touch a simulation clock.
/// Controllers receive it on their next [`sync_feedback`] call.
///
/// ```
/// use ferravex_play::feedback::FeedbackPolicy;
/// use ferravex_render::preferences;
/// let policy = FeedbackPolicy::new(true, false, 0.5).unwrap();
/// assert!(preferences::set_feedback_policy(policy));
/// assert_eq!(preferences::feedback_policy(), policy);
/// assert!(!preferences::set_feedback_policy(policy));
/// ```
pub fn set_feedback_policy(policy: FeedbackPolicy) -> bool {
    FEEDBACK_POLICY.with(|value| value.replace(policy) != policy)
}

// Browser flags are checked together before publishing. The host owns these
// accessibility choices; gameplay-owned intensity remains unchanged.
#[cfg(any(target_arch = "wasm32", test))]
pub(crate) fn apply_feedback_flags(motion: u32, flash: u32) -> u32 {
    if motion > 1 || flash > 1 {
        return 0;
    }
    let intensity = feedback_policy().intensity();
    let policy = FeedbackPolicy::new(motion != 0, flash != 0, intensity)
        .expect("existing validated feedback intensity");
    set_feedback_policy(policy);
    1
}

/// Apply the shared visual policy to an actual feedback consumer, returning
/// whether its policy changed. Call before accepted-event emission and frame
/// sampling, even with no simulation tick or while local menus are open.
///
/// Uses [`FeedbackController::set_policy`]: disabled channels clear pending
/// shake, emphasis, hit-stop or flash immediately; re-enabling cannot revive
/// those effects. Sound, rumble, event IDs and controller configuration survive.
/// This opts the controller into the entire shared policy, including intensity.
///
/// ```
/// use ferravex_math::Vec3;
/// use ferravex_play::feedback::{FeedbackController, FeedbackEvent, FeedbackPolicy};
/// use ferravex_render::preferences;
/// let mut feedback = FeedbackController::<8>::new(Default::default(), 1).unwrap();
/// feedback.emit(1, FeedbackEvent::hurt(Vec3::ZERO, 1.).unwrap()).unwrap();
/// preferences::set_feedback_policy(FeedbackPolicy::new(true, true, 1.).unwrap());
/// assert!(preferences::sync_feedback(&mut feedback));
/// assert_eq!(feedback.impact().flash, 0.);
/// assert_eq!(feedback.impact().emphasis, 0.);
/// assert!(!preferences::sync_feedback(&mut feedback));
/// ```
pub fn sync_feedback<const N: usize>(feedback: &mut FeedbackController<N>) -> bool {
    let policy = feedback_policy();
    if feedback.policy() == policy {
        return false;
    }
    feedback.set_policy(policy);
    true
}

/// Apply audio, graphics and visual policy together. Invalid preferences leave
/// every setting, including the shared feedback policy, unchanged. Policy inputs
/// are validated by [`FeedbackPolicy::new`]. No persistence or controller polling
/// occurs; use [`sync_feedback`] in the presentation owner.
///
/// ```
/// use ferravex_play::feedback::FeedbackPolicy;
/// use ferravex_render::preferences::{self, Preferences};
/// let policy = FeedbackPolicy::new(true, true, 1.).unwrap();
/// let result = preferences::try_apply_with_feedback(Preferences::default(), policy).unwrap();
/// assert!(result.feedback_changed);
/// assert!(!preferences::try_apply_with_feedback(Preferences::default(), policy).unwrap().changed());
/// let invalid = Preferences { look: 255, ..Default::default() };
/// assert!(preferences::try_apply_with_feedback(invalid, FeedbackPolicy::default()).is_err());
/// assert_eq!(preferences::feedback_policy(), policy);
/// ```
pub fn try_apply_with_feedback(
    next: Preferences,
    policy: FeedbackPolicy,
) -> Result<ApplyOutcome, StorageError> {
    let mut outcome = try_apply(next)?;
    outcome.feedback_changed = set_feedback_policy(policy);
    Ok(outcome)
}

// Preserve the internal entry point used by loaded, validated preferences and
// existing renderer callers. Validation still precedes every side effect.
pub(crate) fn apply(p: Preferences) {
    try_apply(p).expect("validated internal preferences");
}

pub(crate) struct Session {
    slot: Slot<Preferences>,
    current: Preferences,
    error: Option<StorageError>,
}
impl Session {
    pub fn new(owner: &str, transient: bool) -> Self {
        let backend = if transient {
            PlatformStore::transient()
        } else {
            PlatformStore::new()
        };
        let slot = Slot::with_backend(owner, "preferences", Preferences::default(), backend)
            .expect("validated preferences owner");
        let current = *slot.value();
        let error = if let LoadStatus::Rejected(e) = slot.status() {
            Some(e)
        } else {
            None
        };
        let session = Self {
            slot,
            current,
            error,
        };
        if !transient {
            apply(current);
        }
        session
    }
    pub fn current(&self) -> Preferences {
        self.current
    }
    pub fn error(&self) -> Option<StorageError> {
        self.error
    }
    pub fn set(&mut self, next: Preferences) -> Result<(), StorageError> {
        // Reconcile runtime drift even when the saved choice has not changed.
        // Validation happens before touching either runtime or session state.
        try_apply(next)?;
        if next == self.current {
            return self.error.map_or(Ok(()), Err);
        }
        self.current = next;
        self.save()
    }
    pub fn save(&mut self) -> Result<(), StorageError> {
        let result = self.slot.commit(self.current);
        self.error = result.err();
        result
    }
    pub fn reset(&mut self) -> Result<(), StorageError> {
        let next = Preferences::default();
        try_apply(next)?;
        self.current = next;
        let result = self.slot.reset(self.current);
        self.error = result.err();
        result
    }
    /// External sound buttons may change the mixer between frames. Detect that
    /// change in memory and write once; failed writes are not retried per frame.
    pub fn sync_audio(&mut self) {
        let next = Preferences {
            master: audio::master_gain(),
            muted: audio::muted(),
            buses: audio::bus_gains(),
            ..self.current
        };
        if next != self.current {
            self.current = next;
            let _ = self.save();
        }
    }
}
/// Validate and apply a complete audio/graphics snapshot without persisting it.
/// Invalid fields return [`StorageError::Invalid`] before any change. Unchanged
/// settings avoid mixer setters and graphics refresh; runtime drift is repaired.
/// Existing visual policy and explicit URL/environment overrides are preserved.
///
/// ```
/// use ferravex_render::preferences::{self, Preferences};
/// let previous = preferences::current();
/// let invalid = Preferences { master: 0.2, lights: 5, ..previous };
/// assert!(preferences::try_apply(invalid).is_err());
/// assert_eq!(preferences::current(), previous);
/// let result = preferences::try_apply(Preferences { master: 0.2, ..previous }).unwrap();
/// assert!(result.audio_changed);
/// assert!(!result.graphics_changed);
/// ```
pub fn try_apply(p: Preferences) -> Result<ApplyOutcome, StorageError> {
    let diff = current().diff(&p)?;
    let audio_changed =
        diff.master.is_some() || diff.muted.is_some() || diff.buses.iter().any(Option::is_some);
    let graphics_changed = diff.quality.is_some()
        || diff.look.is_some()
        || diff.resolution.is_some()
        || diff.shadows.is_some()
        || diff.lights.is_some()
        || diff.adaptive.is_some();
    // Everything below is infallible for the validated complete snapshot.
    crate::edit(|s| {
        s.presentation = p;
        s.performance.budget.set_enabled(p.adaptive);
        if graphics_changed {
            s.resolution = p.resolution;
            s.shadow_detail = p.shadows;
            s.light_detail = p.lights;
            s.look = crate::Look::from_index(p.look as u32).expect("validated look");
            s.preferred_quality = p.quality.map(|q| match q {
                0 => Quality::Low,
                1 => Quality::Medium,
                _ => Quality::High,
            });
            s.quality = s
                .quality_override
                .or(s.preferred_quality)
                .unwrap_or(s.authored_quality);
            s.refresh_environment();
        }
    });
    if let Some(gain) = diff.master {
        audio::set_master_gain(gain);
    }
    if let Some(muted) = diff.muted {
        audio::set_muted(muted);
    }
    for (bus, gain) in [
        audio::Bus::Effects,
        audio::Bus::Ambience,
        audio::Bus::Music,
        audio::Bus::Ui,
    ]
    .into_iter()
    .zip(diff.buses)
    {
        if let Some(gain) = gain {
            audio::set_bus_gain(bus, gain);
        }
    }
    Ok(ApplyOutcome {
        audio_changed,
        graphics_changed,
        feedback_changed: false,
        quality: crate::quality(),
        look: crate::look(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferravex_math::Vec3;
    use ferravex_play::{feedback::FeedbackEvent, persistence::SaveError};

    fn reset_runtime() {
        crate::edit(|s| *s = crate::State::default());
        audio::reset();
        audio::set_master_gain(0.7);
        audio::set_muted(false);
        set_feedback_policy(FeedbackPolicy::default());
    }

    #[test]
    fn ui_snapshot_preserves_world_input_overrides_save_and_feedback_channels() {
        reset_runtime();
        crate::edit(|s| {
            s.matrix = crate::Mat4::from_translation(crate::vec3(3., 4., 5.));
            s.eye = Some(crate::vec3(1., 2., 3.));
            s.w = 390;
            s.h = 640;
            s.dpi = 2.;
            s.time = 91.;
            s.dt = 0.125;
            s.mouse = crate::vec2(12., 34.);
            s.delta = crate::vec2(0.25, -0.75);
            s.frame = 23;
            s.input_enabled = true;
            s.controls_open = true;
            s.keys.insert(32);
            s.pressed.insert(32);
            s.look_override = Some(crate::Look::Ink);
            s.quality_override = Some(Quality::Low);
        });
        let state = || {
            crate::read(|s| {
                (
                    (s.matrix, s.eye, s.w, s.h, s.dpi, s.time, s.dt, s.frame),
                    (
                        s.mouse,
                        s.delta,
                        s.input_enabled,
                        s.controls_open,
                        s.keys.clone(),
                        s.pressed.clone(),
                    ),
                    (s.look_override, s.quality_override, s.presentation),
                )
            })
        };
        let previous = state();
        let mut session = Session::new("ui295-preserve", true);
        let saved = session.current();
        let generation = session.slot.generation();
        for motion in 0..=1 {
            for flash in 0..=1 {
                assert_eq!(apply_feedback_flags(motion, flash), 1);
                let feedback = feedback_policy();
                for (scale, contrast) in [(1., 0), (1.5, 1), (2., 0)] {
                    assert_eq!(apply_ui_snapshot_v1(1, scale, contrast), 1);
                    assert_eq!(state(), previous);
                    assert_eq!(feedback_policy(), feedback);
                }
                let ui = ui_preferences();
                assert_eq!(apply_ui_snapshot_v1(1, 1.5, 2), 0);
                assert_eq!(state(), previous);
                assert_eq!(ui_preferences(), ui);
                assert_eq!(feedback_policy(), feedback);
            }
        }
        session.sync_audio();
        assert_eq!(session.current(), saved);
        assert_eq!(session.slot.generation(), generation);
        apply_ui_preferences(UiPreferences::default());
        assert_eq!(state(), previous);
    }

    #[test]
    fn browser_feedback_flags_are_atomic_and_preserve_intensity_and_preferences() {
        reset_runtime();
        let previous = current();
        set_feedback_policy(FeedbackPolicy::new(false, false, 0.375).unwrap());
        for motion in 0..=1 {
            for flash in 0..=1 {
                assert_eq!(apply_feedback_flags(motion, flash), 1);
                let expected = FeedbackPolicy::new(motion != 0, flash != 0, 0.375).unwrap();
                assert_eq!(feedback_policy(), expected);
                assert_eq!(
                    apply_feedback_flags(motion, flash),
                    1,
                    "unchanged is accepted"
                );
                for invalid in [2, u32::MAX] {
                    assert_eq!(apply_feedback_flags(invalid, flash), 0);
                    assert_eq!(apply_feedback_flags(motion, invalid), 0);
                    assert_eq!(feedback_policy(), expected);
                }
                assert_eq!(
                    current(),
                    previous,
                    "visual flags must not alter audio or graphics choices"
                );
            }
        }
    }

    #[test]
    fn invalid_snapshots_leave_all_runtime_channels_and_session_unchanged() {
        reset_runtime();
        let mut session = Session::new("preferences-invalid", true);
        let previous = Preferences {
            master: 0.3,
            buses: [0.2, 0.3, 0.4, 0.5],
            look: 1,
            quality: Some(1),
            resolution: 2,
            shadows: 2,
            lights: 2,
            adaptive: true,
            ..Default::default()
        };
        session.set(previous).unwrap();
        let policy = FeedbackPolicy::new(true, false, 0.5).unwrap();
        set_feedback_policy(policy);
        let generation = session.slot.generation();
        // Include otherwise-valid changes to every channel before corrupting one.
        let candidate = Preferences {
            master: 0.8,
            buses: [0.6; 4],
            muted: true,
            look: 3,
            quality: Some(2),
            resolution: 5,
            shadows: 3,
            lights: 4,
            adaptive: false,
        };
        let mut invalid = Vec::new();
        for gain in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.01, 1.01] {
            invalid.push(Preferences {
                master: gain,
                ..candidate
            });
            for bus in 0..4 {
                let mut p = candidate;
                p.buses[bus] = gain;
                invalid.push(p);
            }
        }
        invalid.extend([
            Preferences {
                look: 4,
                ..candidate
            },
            Preferences {
                quality: Some(3),
                ..candidate
            },
            Preferences {
                resolution: 6,
                ..candidate
            },
            Preferences {
                shadows: 4,
                ..candidate
            },
            Preferences {
                lights: 5,
                ..candidate
            },
        ]);
        for next in invalid {
            assert_eq!(
                try_apply_with_feedback(next, FeedbackPolicy::default()),
                Err(StorageError::Invalid(SaveError::InvalidData))
            );
            assert_eq!(
                session.set(next),
                Err(StorageError::Invalid(SaveError::InvalidData))
            );
            assert_eq!(current(), previous);
            assert_eq!(feedback_policy(), policy);
            assert_eq!(crate::read(|s| s.presentation), previous);
            assert_eq!(session.current(), previous);
            assert_eq!(session.slot.generation(), generation);
            assert_eq!(crate::quality(), Quality::Medium);
            assert_eq!(crate::look(), crate::Look::Warm);
        }
    }

    #[test]
    fn outcomes_separate_audio_graphics_and_feedback_and_reconcile_real_state() {
        reset_runtime();
        let base = Preferences::default();
        assert!(!try_apply(base).unwrap().changed());
        for next in [
            Preferences { master: 0., ..base },
            Preferences {
                buses: [0., 1., 1., 1.],
                ..base
            },
            Preferences {
                buses: [1., 0., 1., 1.],
                ..base
            },
            Preferences {
                buses: [1., 1., 0., 1.],
                ..base
            },
            Preferences {
                buses: [1., 1., 1., 0.],
                ..base
            },
            Preferences {
                muted: true,
                ..base
            },
        ] {
            try_apply(base).unwrap();
            let outcome = try_apply(next).unwrap();
            assert!(outcome.audio_changed && outcome.changed());
            assert!(!outcome.graphics_changed && !outcome.feedback_changed);
            assert_eq!(current(), next);
            assert!(!try_apply(next).unwrap().changed());
        }
        for next in [
            Preferences {
                quality: Some(2),
                ..base
            },
            Preferences { look: 3, ..base },
            Preferences {
                resolution: 5,
                ..base
            },
            Preferences { shadows: 3, ..base },
            Preferences { lights: 4, ..base },
            Preferences {
                adaptive: true,
                ..base
            },
        ] {
            try_apply(base).unwrap();
            let outcome = try_apply(next).unwrap();
            assert!(outcome.graphics_changed && outcome.changed());
            assert!(!outcome.audio_changed && !outcome.feedback_changed);
            assert_eq!(current(), next);
            assert!(!try_apply(next).unwrap().changed());
        }
        let policy = FeedbackPolicy::new(false, true, 1.).unwrap();
        let next = current();
        let outcome = try_apply_with_feedback(next, policy).unwrap();
        assert!(outcome.feedback_changed && outcome.changed());
        assert!(!outcome.audio_changed && !outcome.graphics_changed);
        assert!(!try_apply_with_feedback(next, policy).unwrap().changed());

        // The Wasm details queue updates presentation before runtime application.
        // Compare the real fields so this cannot conceal unapplied graphics.
        let pending = Preferences {
            look: 2,
            adaptive: false,
            resolution: 4,
            ..next
        };
        crate::edit(|s| s.presentation = pending);
        assert_ne!(current(), pending);
        assert!(try_apply(pending).unwrap().graphics_changed);
        assert_eq!(current(), pending);
        audio::set_master_gain(0.1);
        assert!(try_apply(pending).unwrap().audio_changed);
        assert_eq!(audio::master_gain(), pending.master);
        let zero = Preferences {
            master: 0.,
            ..pending
        };
        try_apply(zero).unwrap();
        assert!(
            !try_apply(Preferences {
                master: -0.,
                ..zero
            })
            .unwrap()
            .changed()
        );
    }

    #[test]
    fn session_reapplies_equal_choices_without_writing_or_erasing_policy() {
        reset_runtime();
        let mut session = Session::new("preferences-reapply", true);
        let p = Preferences {
            master: 0.4,
            buses: [0.2; 4],
            quality: Some(2),
            look: 3,
            ..Default::default()
        };
        session.set(p).unwrap();
        let generation = session.slot.generation();
        let policy = FeedbackPolicy::new(true, true, 0.5).unwrap();
        set_feedback_policy(policy);
        audio::reset();
        audio::set_master_gain(1.);
        crate::set_look(crate::Look::Warm);
        crate::set_quality_preference(None);
        session.set(p).unwrap();
        assert_eq!(current(), p);
        assert_eq!(feedback_policy(), policy);
        assert_eq!(session.slot.generation(), generation);
        session.error = Some(StorageError::Unavailable);
        assert_eq!(session.set(p), Err(StorageError::Unavailable));
        assert_eq!(session.slot.generation(), generation);
        session.save().unwrap();
        assert_eq!(session.error(), None);
        session.reset().unwrap();
        assert_eq!(feedback_policy(), policy);
    }

    #[test]
    fn effective_overrides_hide_but_do_not_destroy_user_choices() {
        reset_runtime();
        crate::set_quality(Quality::Medium);
        crate::quality_override(Quality::Low);
        crate::edit(|s| s.look_override = Some(crate::Look::Ink));
        let p = Preferences {
            quality: Some(2),
            look: 1,
            ..Default::default()
        };
        let outcome = try_apply(p).unwrap();
        assert!(outcome.graphics_changed);
        assert_eq!(outcome.quality, Quality::Low);
        assert_eq!(outcome.look, crate::Look::Ink);
        assert_eq!(current(), p);
        assert_eq!(crate::read(|s| s.presentation), p);
        let cleared = Preferences {
            quality: None,
            look: 2,
            ..p
        };
        let outcome = try_apply(cleared).unwrap();
        assert!(outcome.graphics_changed);
        assert_eq!(outcome.quality, Quality::Low);
        assert_eq!(outcome.look, crate::Look::Ink);
        crate::edit(|s| {
            s.quality_override = None;
            s.look_override = None;
        });
        crate::set_quality(Quality::Medium);
        assert_eq!(crate::quality(), Quality::Medium);
        assert_eq!(crate::look(), crate::Look::Cool);
        assert_eq!(current(), cleared);
    }

    #[test]
    fn live_feedback_consumer_clears_pending_effects_without_replaying_events() {
        reset_runtime();
        let event = FeedbackEvent::hurt(Vec3::ZERO, 1.).unwrap();
        for motion in [false, true] {
            for flash in [false, true] {
                set_feedback_policy(FeedbackPolicy::default());
                let mut feedback = FeedbackController::<8>::new(
                    ferravex_play::feedback::FeedbackConfig {
                        single_player_hitstop: true,
                        ..Default::default()
                    },
                    7,
                )
                .unwrap();
                let response = feedback.emit(1, event).unwrap().unwrap();
                assert!(feedback.impact().emphasis > 0.);
                assert!(feedback.impact().flash > 0.);
                let policy = FeedbackPolicy::new(motion, flash, 1.).unwrap();
                set_feedback_policy(policy);
                assert_eq!(sync_feedback(&mut feedback), motion || flash);
                assert!(!sync_feedback(&mut feedback));
                assert_eq!(feedback.impact().emphasis == 0., motion);
                assert_eq!(feedback.impact().flash == 0., flash);
                assert!(feedback.emit(1, event).unwrap().is_none());
                let next = feedback.emit(2, event).unwrap().unwrap();
                assert_eq!(next.cue, response.cue);
                assert_eq!(next.gain, response.gain);
                assert_eq!(next.particles == 0, motion);
                let frame = feedback.advance(0.).unwrap();
                assert_eq!(frame.hitstop_seconds == 0., motion);
                if motion {
                    assert_eq!(frame.shake.translation, Vec3::ZERO);
                }
                assert!(frame.rumble[0] > 0.);
                set_feedback_policy(FeedbackPolicy::default());
                sync_feedback(&mut feedback);
                if motion {
                    assert_eq!(feedback.impact().emphasis, 0.);
                }
                if flash {
                    assert_eq!(feedback.impact().flash, 0.);
                }
                assert!(feedback.emit(2, event).unwrap().is_none());
            }
        }
    }

    #[test]
    fn session_changes_persist_only_once_and_explicit_quality_overrides_win() {
        reset_runtime();
        let mut session = Session::new("preferences-test", true);
        let p = Preferences {
            master: 0.25,
            buses: [0.2, 0.3, 0.4, 0.5],
            muted: true,
            quality: Some(2),
            look: 1,
            ..Default::default()
        };
        session.set(p).unwrap();
        assert_eq!(audio::master_gain(), 0.25);
        assert_eq!(audio::bus_gains(), p.buses);
        assert!(audio::muted());
        assert_eq!(crate::quality(), Quality::High);
        let stamp = session.slot.generation();
        for _ in 0..1000 {
            session.sync_audio();
        }
        assert_eq!(session.slot.generation(), stamp);
        crate::set_quality(Quality::Low);
        assert_eq!(crate::quality(), Quality::High);
        crate::quality_override(Quality::Medium);
        session
            .set(Preferences {
                quality: Some(0),
                ..p
            })
            .unwrap();
        assert_eq!(crate::quality(), Quality::Medium);
        audio::set_muted(false);
        session.sync_audio();
        assert!(!session.current().muted);
        assert_eq!(session.slot.generation(), stamp + 2);
        let previous = session.current();
        assert!(
            session
                .set(Preferences {
                    master: f32::NAN,
                    ..previous
                })
                .is_err()
        );
        assert_eq!(session.current(), previous);
        session.reset().unwrap();
        assert_eq!(session.current(), Preferences::default());
        assert_eq!(crate::quality(), Quality::Medium);
    }
}
