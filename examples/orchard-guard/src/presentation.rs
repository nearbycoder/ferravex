//! Cosmetic state sampled every presentation frame, including local menus.
//! Accepted impact counters come from combat; preferences never mutate the game.
use crate::game::{Game, MAX_PESTS};
use ferravex_play::feedback::FeedbackPolicy;
use ferravex_render::preferences;

#[derive(Clone, Copy, Default)]
struct Flash {
    id: u32,
    impacts: u32,
    remaining: f32,
}

pub(super) struct Presentation {
    flashes: [Flash; MAX_PESTS],
    policy: FeedbackPolicy,
    phase: f32,
}

impl Default for Presentation {
    fn default() -> Self {
        Self {
            flashes: [Flash::default(); MAX_PESTS],
            policy: FeedbackPolicy::default(),
            phase: 0.,
        }
    }
}

impl Presentation {
    /// Sync before drawing and accepted feedback, even when no tick can run.
    pub fn frame(&mut self, game: &Game, elapsed: f32) {
        self.policy = preferences::feedback_policy();
        let dt = if elapsed.is_finite() {
            elapsed.clamp(0., 0.25)
        } else {
            0.
        };
        self.phase = if self.policy.reduced_motion() || self.policy.intensity() == 0. {
            0.
        } else {
            (game.time * 24.) % std::f32::consts::TAU
        };
        let previous = self.flashes;
        self.flashes.fill(Flash::default());
        for (slot, pest) in self.flashes.iter_mut().zip(&game.pests) {
            let old = previous
                .iter()
                .find(|flash| flash.id == pest.id)
                .copied()
                .unwrap_or_default();
            let remaining = if self.policy.reduced_flash() || self.policy.intensity() == 0. {
                0.
            } else if pest.impacts > old.impacts {
                0.12
            } else {
                (old.remaining - dt).max(0.)
            };
            // Remember suppressed impacts too: restoring a preference cannot replay them.
            *slot = Flash {
                id: pest.id,
                impacts: pest.impacts,
                remaining,
            };
        }
    }

    pub fn flash(&self, id: u32) -> f32 {
        self.flashes
            .iter()
            .find(|flash| flash.id == id)
            .map_or(0., |flash| flash.remaining / 0.12 * self.policy.intensity())
    }

    pub fn wiggle(&self, id: u32) -> f32 {
        if self.policy.reduced_motion() {
            0.
        } else {
            (self.phase + id as f32).sin() * 3. * self.policy.intensity()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct RestorePolicy(FeedbackPolicy);
    impl Drop for RestorePolicy {
        fn drop(&mut self) {
            preferences::set_feedback_policy(self.0);
        }
    }

    #[test]
    fn independent_preferences_clear_existing_and_suppressed_hits_on_zero_tick_frames() {
        let _restore = RestorePolicy(preferences::feedback_policy());
        preferences::set_feedback_policy(FeedbackPolicy::default());
        let mut game = Game::new();
        game.launch();
        game.step(1. / 60.);
        let id = game.pests[0].id;
        let mut presentation = Presentation::default();
        game.pests[0].impacts += 1;
        presentation.frame(&game, 0.);
        assert_eq!(presentation.flash(id), 1.);
        let frozen = (game.time, game.pests[0].pos, game.money, game.remaining);

        preferences::set_feedback_policy(FeedbackPolicy::new(true, false, 1.).unwrap());
        presentation.frame(&game, 0.);
        assert_eq!(presentation.wiggle(id), 0.);
        assert_eq!(
            presentation.flash(id),
            1.,
            "motion does not imply reduced flash"
        );
        preferences::set_feedback_policy(FeedbackPolicy::new(false, true, 1.).unwrap());
        presentation.frame(&game, 0.);
        assert_eq!(presentation.flash(id), 0.);
        assert_ne!(presentation.wiggle(id), 0.);
        game.pests[0].impacts += 1;
        presentation.frame(&game, 0.);
        preferences::set_feedback_policy(FeedbackPolicy::default());
        presentation.frame(&game, 0.);
        assert_eq!(
            presentation.flash(id),
            0.,
            "old hits cannot revive on resume"
        );
        game.pests[0].impacts += 1;
        presentation.frame(&game, 0.);
        assert_eq!(presentation.flash(id), 1., "new accepted hits still work");
        presentation.frame(&game, 0.13);
        assert_eq!(
            presentation.flash(id),
            0.,
            "flash fades without a simulation tick"
        );
        assert_eq!(
            (game.time, game.pests[0].pos, game.money, game.remaining),
            frozen
        );
    }

    #[test]
    fn zero_intensity_and_removed_entities_cannot_restore_a_flash() {
        let _restore = RestorePolicy(preferences::feedback_policy());
        let mut game = Game::new();
        game.launch();
        game.step(1. / 60.);
        game.pests[0].impacts = 1;
        let id = game.pests[0].id;
        preferences::set_feedback_policy(FeedbackPolicy::new(false, false, 0.).unwrap());
        let mut presentation = Presentation::default();
        presentation.frame(&game, f32::NAN);
        preferences::set_feedback_policy(FeedbackPolicy::default());
        presentation.frame(&game, 0.);
        assert_eq!(presentation.flash(id), 0.);
        game.pests[0].impacts += 1;
        presentation.frame(&game, 0.);
        assert_eq!(presentation.flash(id), 1.);
        game.pests.clear();
        presentation.frame(&game, 0.);
        assert_eq!(presentation.flash(id), 0.);
    }
}
