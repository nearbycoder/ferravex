//! A deliberately small set of common engine entry points.
//!
//! Import with `use ferravex::prelude::*;`. These are the original types, so they
//! interoperate with the full [`crate::core`], [`crate::math`], [`crate::input`]
//! and [`crate::render`] APIs without conversions.
//!
//! - [`GameSession`] coordinates start, pause, restart and fixed simulation time.
//!   [`SessionConfig`] validates reusable settings; setup and inspector errors
//!   stay at [`crate::core::FixedClockError`] and [`crate::session::SessionStepError`].
//! - [`TickBatch`] yields [`TickContext`] values with each scheduled update's ID
//!   and delta; scheduled IDs are distinct from executed-update counts.
//! - [`Vec2`], [`Vec3`], [`Mat4`] and the other math types are shared by all engines.
//! - [`ActionMap`] handles platform-independent actions; [`PlayerInput`] connects
//!   those actions to the renderer's platform input and controls UI.
//! - [`Camera2D`], [`Camera3D`] and [`Conf`] configure the renderer. Drawing,
//!   frame scheduling and low-level input polling stay qualified as `render::…`.
//!
//! There is no generic `Camera`, `Mesh` or `Action` alias: choose the appropriate
//! subsystem explicitly when reaching beyond these entry points.
//! In particular, use [`crate::app::AppFrameReport`] for lifecycle diagnostics,
//! [`crate::time::SimulationTimer`] for timers and [`crate::feedback`] for
//! accepted-event presentation. These systems do not expand wildcard imports.
//!
//! ```
//! use ferravex::prelude::*;
//!
//! let mut actions = ActionMap::default();
//! let right = actions.action(vec![Binding::digital(
//!     InputCode::Key(KeyCode::Right as u16),
//! )]);
//! let config = SessionConfig::try_new(60.0, 8, true)?;
//! let mut session = GameSession::from_config(config);
//! actions.feed(InputCode::Key(KeyCode::Right as u16), 1.0);
//! let mut position = Vec2::ZERO;
//! let batch: TickBatch = session.advance_batch(1.0 / 60.0);
//! for TickContext { tick, dt, .. } in batch.ticks() {
//!     // Consume edges per simulation tick, including catch-up ticks.
//!     let action = actions.consume_tick(right);
//!     position += vec2(action.value, 0.0) * 120.0 * dt;
//!     assert_eq!(tick, 1);
//! }
//! assert!((position.x - 2.0).abs() < 0.0001);
//! # Ok::<(), ferravex::core::FixedClockError>(())
//! ```

pub use crate::input::{ActionKind, ActionMap, Binding, InputCode};
pub use crate::math::{Color, Mat4, Quat, Rect, Vec2, Vec3, vec2, vec3};
pub use crate::render;
pub use crate::render::input::PlayerInput;
pub use crate::render::{Camera2D, Camera3D, Conf, KeyCode, MouseButton};
pub use crate::{
    Entity, FixedClock, FrameStep, GameSession, SessionConfig, SessionState, TickBatch,
    TickContext, World,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_config_reexport_keeps_core_identity_and_error_types() {
        let config: crate::core::session::SessionConfig =
            SessionConfig::try_new(64.0, 4, false).unwrap();
        let mut session = GameSession::from_config(config);
        let independent = GameSession::from_config(config);
        assert!(session.start());
        assert!(session.set_paused(true));
        let frame: FrameStep = session.step_paused().unwrap();
        assert_eq!(frame.steps, 1);
        assert_eq!(session.tick(), 1);
        assert_eq!(independent.tick(), 0);
        assert_eq!(independent.state(), SessionState::Ready);
        assert_eq!(
            SessionConfig::try_new(60.0, 0, false),
            Err(crate::core::FixedClockError::ZeroCatchUpBudget)
        );
        session.finish(true);
        assert_eq!(
            session.step_paused().unwrap_err(),
            crate::session::SessionStepError::NotPaused {
                state: SessionState::Won
            }
        );
    }

    #[test]
    fn session_exports_share_core_state_and_do_not_catch_up_after_pause() {
        let mut session: crate::core::session::GameSession = GameSession::new(60.0, 4, false);
        assert_eq!(session.state(), SessionState::Ready);
        assert_eq!(session.advance(10.0).steps, 0);
        assert!(session.start());
        assert_eq!(session.advance(1.0 / 60.0).steps, 1);
        assert!(session.set_paused(true));
        assert_eq!(session.advance(10.0).steps, 0);
        assert!(session.set_paused(false));
        let frame: FrameStep = session.advance(0.5 / 60.0);
        assert_eq!(frame.steps, 0);
        assert!((frame.alpha - 0.5).abs() < 0.0001);
        assert_eq!(session.tick(), 1);
    }

    #[test]
    fn input_edges_survive_zero_step_frames_and_are_consumed_once() {
        let mut input: crate::input::ActionMap = ActionMap::default();
        let code = InputCode::Key(KeyCode::Space as u16);
        let jump = input.action(vec![Binding::digital(code)]);
        let mut session = GameSession::new(60.0, 4, true);
        input.feed(code, 1.0);
        input.feed(code, 0.0);
        assert_eq!(session.advance(0.5 / 60.0).steps, 0);

        let frame = session.advance(2.0 / 60.0);
        assert_eq!(frame.steps, 2);
        let mut accepted = 0;
        for _ in 0..frame.steps {
            accepted += u32::from(input.consume_tick(jump).pressed);
        }
        assert_eq!(accepted, 1);
        assert!(!input.consume_tick(jump).pressed);
    }

    #[test]
    fn math_exports_work_directly_with_renderer_cameras() {
        let camera = Camera2D::from_display_rect(Rect::new(0.0, 0.0, 640.0, 360.0));
        let matrix: Mat4 = render::Camera::matrix(&camera, 640.0, 360.0);
        let origin: Vec3 = matrix.project_point3(Vec3::ZERO);
        assert!(origin.abs_diff_eq(vec3(-1.0, 1.0, 0.0), 0.0001));
        assert!(
            matrix
                .project_point3(vec3(640.0, 360.0, 0.0))
                .abs_diff_eq(vec3(1.0, -1.0, 0.0), 0.0001)
        );
    }
}
