use crate::{PinballFeedback, Table};
use ferravex_render::{input::PlayerInput, preferences};

const BUMPER_SECONDS: f32 = 0.18;
const SCORE_SECONDS: f32 = 0.9;

/// Only cosmetic state lives here. In particular, the table's bumper timer is
/// a scoring debounce and must never be cleared to honor a flash preference.
#[derive(Default)]
pub(super) struct Presentation {
    seen_hits: [u64; 3],
    seen_trail_tick: u32,
    flashes: [f32; 3],
    rings: [f32; 3],
    pub trail_len: usize,
    pub motion_scale: f32,
    flash_scale: f32,
    pub score_points: u32,
    score_seconds: f32,
    pub launch_seconds: f32,
}

impl Presentation {
    /// Run on every presentation frame BEFORE accepting events, even with no
    /// simulation ticks, paused or in Controls. The shared policy is the only
    /// preference store; no game-specific export or persistence is needed.
    pub fn begin_frame(&mut self) {
        let policy = preferences::feedback_policy();
        self.apply_preferences(
            policy.reduced_motion(),
            policy.reduced_flash(),
            policy.intensity(),
        );
    }

    fn apply_preferences(&mut self, reduced_motion: bool, reduced_flash: bool, intensity: f32) {
        self.motion_scale = if reduced_motion { 0. } else { intensity };
        self.flash_scale = if reduced_flash { 0. } else { intensity };
        if self.motion_scale == 0. {
            self.rings.fill(0.);
            self.trail_len = 0;
        }
        if self.flash_scale == 0. {
            self.flashes.fill(0.);
        }
    }

    pub fn reset(&mut self) {
        *self = Self::default();
        self.begin_frame();
    }

    pub fn finish_frame(&mut self, dt: f32, table: &Table, events: PinballFeedback) {
        let dt = if dt.is_finite() {
            dt.clamp(0., 0.25)
        } else {
            0.
        };
        self.score_seconds = (self.score_seconds - dt).max(0.);
        self.launch_seconds = (self.launch_seconds - dt).max(0.);
        if self.score_seconds == 0. {
            self.score_points = 0;
        }
        if events.points > 0 {
            self.score_points = self.score_points.saturating_add(events.points);
            self.score_seconds = SCORE_SECONDS;
        }
        if events.launches > 0 {
            self.launch_seconds = 0.65;
        }
        if events.drains > 0 {
            self.launch_seconds = 0.;
        }
        for i in 0..3 {
            self.flashes[i] = (self.flashes[i] - dt).max(0.);
            self.rings[i] = (self.rings[i] - dt).max(0.);
            if table.bumper_hits[i] != self.seen_hits[i] {
                if self.flash_scale > 0. {
                    self.flashes[i] = BUMPER_SECONDS;
                }
                if self.motion_scale > 0. {
                    self.rings[i] = BUMPER_SECONDS;
                }
            }
        }
        // Consume serials even while effects are suppressed. Enabling a channel
        // cannot replay contacts accepted under the previous preference.
        self.seen_hits = table.bumper_hits;

        // Reuse the fixed simulation trail, exposing only samples produced after
        // motion was enabled. Never reveal pre-toggle points on a zero-tick frame.
        let ticks = table.trail_tick.wrapping_sub(self.seen_trail_tick);
        let samples = (u64::from(ticks) + u64::from(self.seen_trail_tick % 3)) / 3;
        self.seen_trail_tick = table.trail_tick;
        if self.motion_scale == 0. || table.waiting || table.over() || events.drains > 0 {
            self.trail_len = 0;
        } else {
            self.trail_len = (self.trail_len + samples.min(12) as usize).min(12);
        }
    }

    pub fn flash(&self, bumper: usize) -> f32 {
        self.flashes[bumper] / BUMPER_SECONDS * self.flash_scale
    }

    pub fn ring(&self, bumper: usize) -> f32 {
        self.rings[bumper] / BUMPER_SECONDS * self.motion_scale
    }
}

/// Caller-owned, bounded labels from the current binding/device. A huge custom
/// binding list directs players to Controls rather than showing obsolete keys.
pub(super) fn hint<'a>(input: &PlayerInput, action: &str, buffer: &'a mut [u8]) -> &'a str {
    input.format_hint(action, buffer).unwrap_or("SEE CONTROLS")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BUMPERS, Controls, PinballGame};
    use ferravex_physics::d2::Vector;
    use ferravex_render::input::{ActionKind, Binding, InputCode};

    const DT: f32 = 1. / 120.;

    fn hit(table: &mut Table, bumper: usize) -> PinballFeedback {
        let before = (table.score, table.hits);
        table.waiting = false;
        let [x, y] = BUMPERS[bumper];
        table.world.bodies[table.ball].set_translation(Vector::new(x, y - 0.78), true);
        table.world.bodies[table.ball].set_linvel(Vector::new(0., 2.), true);
        table.step(DT, Controls::default());
        PinballFeedback {
            points: table.score - before.0,
            hits: table.hits - before.1,
            ..Default::default()
        }
    }

    #[test]
    fn independent_channels_clear_on_zero_tick_frames_without_reviving_effects() {
        let mut table = Table::new();
        let mut view = Presentation::default();
        view.begin_frame(); // Actual shared-policy read before the first contact.
        let events = hit(&mut table, 0);
        assert_eq!(events.points, 100);
        view.finish_frame(DT, &table, events);
        assert!(view.flash(0) > 0. && view.ring(0) > 0.);
        let simulation_timer = table.bumper_flash[0];

        // The same reducer used by begin_frame; native renderer does not
        // reexport a named FeedbackPolicy constructor. Host ABI testing belongs
        // to the parent's fresh Wasm/browser pass.
        view.apply_preferences(false, true, 1.);
        view.finish_frame(0., &table, PinballFeedback::default());
        assert_eq!(view.flash(0), 0.);
        assert!(view.ring(0) > 0.);
        view.apply_preferences(false, false, 1.);
        view.finish_frame(0., &table, PinballFeedback::default());
        assert_eq!(view.flash(0), 0., "enabling flash must not revive a hit");
        view.apply_preferences(true, false, 1.);
        assert_eq!(view.ring(0), 0.);
        view.apply_preferences(false, false, 1.);
        view.finish_frame(0., &table, PinballFeedback::default());
        assert_eq!(view.ring(0), 0., "enabling motion must not revive a ring");
        assert_eq!(table.bumper_flash[0], simulation_timer);
        assert_eq!(view.score_points, 100, "textual score feedback survives");

        let events = hit(&mut table, 1);
        view.finish_frame(DT, &table, events);
        assert!(
            view.flash(1) > 0. && view.ring(1) > 0.,
            "fresh hits still work"
        );
        view.apply_preferences(true, false, 0.5);
        assert_eq!(view.ring(1), 0.);
        assert_eq!(
            view.flash(1),
            0.5,
            "motion and intensity do not disable flash"
        );
        view.apply_preferences(false, false, 0.);
        view.apply_preferences(false, false, 1.);
        assert_eq!(view.flash(1), 0.);
    }

    #[test]
    fn suppressed_hits_do_not_reappear_or_change_contact_debounce() {
        let mut table = Table::new();
        let mut control = Table::new();
        let mut view = Presentation::default();
        view.apply_preferences(true, true, 1.);
        for repeat in 0..3 {
            let event = hit(&mut table, 0);
            assert_eq!(event, hit(&mut control, 0));
            view.finish_frame(DT, &table, event);
            assert_eq!(view.flash(0), 0.);
            assert_eq!(view.ring(0), 0.);
            // Leave the contact for a tick, then return inside the debounce on
            // the second iteration. Wait out the debounce before the third.
            for _ in 0..if repeat == 0 { 1 } else { 24 } {
                for t in [&mut table, &mut control] {
                    t.world.bodies[t.ball].set_translation(Vector::new(4.4, 2.), true);
                    t.world.bodies[t.ball].set_linvel(Vector::ZERO, true);
                    t.step(DT, Controls::default());
                }
                view.finish_frame(DT, &table, PinballFeedback::default());
            }
            assert_eq!(table.score, if repeat < 2 { 100 } else { 200 });
        }
        view.apply_preferences(false, false, 1.);
        view.finish_frame(0., &table, PinballFeedback::default());
        assert_eq!(view.flash(0), 0.);
        assert_eq!(view.ring(0), 0.);
        assert_eq!(view.trail_len, 0);
        assert_eq!(table.position(), control.position());
        assert_eq!(table.bumper_flash, control.bumper_flash);
        assert_eq!(table.score, control.score);
    }

    #[test]
    fn reenabling_motion_only_exposes_new_trail_samples_and_reset_clears_notices() {
        let mut game = PinballGame::new(true);
        let mut view = Presentation::default();
        view.begin_frame();
        let events = game.advance(12. / 120., Controls::default());
        view.finish_frame(0.1, game.table(), events);
        assert_eq!(view.trail_len, 4);
        assert!(view.launch_seconds > 0.);
        view.apply_preferences(true, false, 1.);
        assert_eq!(view.trail_len, 0);
        let events = game.advance(12. / 120., Controls::default());
        view.finish_frame(0.1, game.table(), events);
        view.apply_preferences(false, false, 1.);
        view.finish_frame(0., game.table(), PinballFeedback::default());
        assert_eq!(view.trail_len, 0);
        let events = game.advance(3. / 120., Controls::default());
        view.finish_frame(0.025, game.table(), events);
        assert_eq!(view.trail_len, 1);
        game.set_paused(true);
        let events = game.advance(60., Controls::default());
        view.finish_frame(0.1, game.table(), events);
        assert_eq!(
            view.trail_len, 1,
            "paused presentation cannot invent trail samples"
        );
        game.restart();
        view.reset();
        assert_eq!(view.trail_len, 0);
        assert_eq!(view.score_points, 0);
        assert_eq!(view.launch_seconds, 0.);
        let events = game.advance(0., Controls::default());
        view.finish_frame(0., game.table(), events);
        assert_eq!(view.trail_len, 0);
    }

    #[test]
    fn one_score_notice_combines_contacts_and_expires_without_simulation_ticks() {
        let mut table = Table::new();
        let mut view = Presentation::default();
        view.begin_frame();
        for bumper in [0, 1] {
            let events = hit(&mut table, bumper);
            assert_eq!(events.points, 100);
            view.finish_frame(DT, &table, events);
        }
        assert_eq!(view.score_points, 200);
        let time = table.time;
        view.finish_frame(f32::NAN, &table, PinballFeedback::default());
        assert_eq!(view.score_points, 200);
        for _ in 0..4 {
            view.finish_frame(0.25, &table, PinballFeedback::default());
        }
        assert_eq!(view.score_points, 0);
        assert_eq!(view.flash(0), 0.);
        assert_eq!(view.ring(1), 0.);
        assert_eq!(table.score, 200);
        assert_eq!(table.time, time);
    }

    #[test]
    fn hints_follow_remaps_and_report_unbound_or_oversized_lists_honestly() {
        let mut input = PlayerInput::transient("pinball-hint-test");
        input
            .action(
                "launch",
                "Launch",
                ActionKind::Button,
                vec![Binding::digital(InputCode::Key(32))],
            )
            .unwrap();
        let mut text = [0; 128];
        assert_eq!(hint(&input, "launch", &mut text), "Space");
        input
            .rebind("launch", vec![Binding::digital(InputCode::Key(106))])
            .unwrap();
        assert_eq!(hint(&input, "launch", &mut text), "J");
        input.rebind("launch", vec![]).unwrap();
        assert_eq!(hint(&input, "launch", &mut text), "Unbound");
        input
            .rebind("launch", vec![Binding::digital(InputCode::Key(106)); 64])
            .unwrap();
        assert_eq!(hint(&input, "launch", &mut text), "SEE CONTROLS");
    }
}
