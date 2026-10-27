//! The shared first-party FPS bot adapter is also available to other games.
pub use ferravex_fps::bots::PracticeBots;
use ferravex_fps::{BUTTON_FIRE, GameEvent, InputFrame, Match};

/// Ticks the crew holds fire after the match starts or you respawn.
pub(super) const GRACE_TICKS: u32 = 128;
/// Once a bot decides to shoot, it keeps the trigger down this many ticks.
pub(super) const BURST_TICKS: u8 = 22;

/// Practice difficulty: the crew are dockworkers, not snipers. The shared bots
/// fire rare single taps aimed at the eyes, so most deaths were sudden
/// cross-map headshots. Here a bot that decides to shoot fires a short burst
/// aimed at the chest with a little growing sway: readable, survivable
/// pressure you can hear, see and dodge. They also hold fire briefly after
/// the match starts or you respawn.
///
/// This edits only the bots' practice commands before the authority step, so
/// it is deterministic and captured in replays exactly like any other input.
/// Network authority, tactical rules and the player's own command are untouched.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Tuning {
    grace_until: u32,
    burst: [u8; ferravex_fps::MAX_PLAYERS],
}

impl Tuning {
    pub fn new() -> Self {
        Self {
            grace_until: GRACE_TICKS,
            burst: [0; ferravex_fps::MAX_PLAYERS],
        }
    }

    /// Adjust bot commands for the tick about to run from `game`.
    pub fn adjust(&mut self, game: &Match, commands: &mut [(u8, InputFrame)]) {
        let tick = game.tick;
        let target_alive = game.player(0).is_some_and(|p| p.is_alive());
        for (id, input) in commands.iter_mut().filter(|(id, _)| *id != 0) {
            let Some(burst) = self.burst.get_mut(usize::from(*id)) else {
                continue;
            };
            let alive = game.player(*id).is_some_and(|p| p.is_alive() && p.ammo > 0);
            if !alive || !target_alive || tick < self.grace_until {
                *burst = 0;
                input.buttons &= !BUTTON_FIRE;
                continue;
            }
            if input.buttons & BUTTON_FIRE != 0 && *burst == 0 {
                *burst = BURST_TICKS;
            }
            if *burst == 0 {
                continue;
            }
            *burst -= 1;
            input.buttons |= BUTTON_FIRE;
            // Deterministic sway from the tick, bot id and burst age.
            let kick = f32::from(BURST_TICKS - *burst) / f32::from(BURST_TICKS);
            let phase = tick as f32 * 0.37 + f32::from(*id) * 1.7;
            input.pitch -= 0.03 + 0.012 * (phase * 0.6).sin() - 0.02 * kick;
            input.yaw += (0.008 + 0.02 * kick) * phase.sin();
        }
    }

    /// Follow accepted events; a respawn of the player restarts the grace.
    pub fn events(&mut self, tick: u32, events: &[GameEvent]) {
        if events
            .iter()
            .any(|e| matches!(e, GameEvent::Respawned { player: 0 }))
        {
            self.grace_until = tick + GRACE_TICKS;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_crew_holds_fire_during_grace_bursts_after_and_never_touches_the_player() {
        let mut game = crate::practice();
        let mut tuning = Tuning::new();
        let fire = InputFrame {
            buttons: BUTTON_FIRE,
            yaw: 1.,
            pitch: 0.1,
            ..Default::default()
        };
        let mut commands = [(0, fire), (2, fire)];
        tuning.adjust(&game, &mut commands);
        assert_eq!(commands[0].1, fire, "your command is never edited");
        assert_eq!(
            commands[1].1.buttons, 0,
            "no shots during the opening grace"
        );
        game.tick = GRACE_TICKS;
        let mut commands = [(0, fire), (2, fire)];
        tuning.adjust(&game, &mut commands);
        assert_eq!(commands[1].1.buttons, BUTTON_FIRE);
        assert!(commands[1].1.pitch < fire.pitch, "aims below the head");
        let idle = InputFrame::default();
        for _ in 1..BURST_TICKS {
            let mut held = [(2, idle)];
            tuning.adjust(&game, &mut held);
            assert_eq!(
                held[0].1.buttons, BUTTON_FIRE,
                "the burst holds the trigger"
            );
        }
        let mut done = [(2, idle)];
        tuning.adjust(&game, &mut done);
        assert_eq!(done[0].1.buttons, 0, "bursts end");
        tuning.events(500, &[GameEvent::Respawned { player: 3 }]);
        game.tick = 501;
        let mut again = [(2, fire)];
        tuning.adjust(&game, &mut again);
        assert_eq!(
            again[0].1.buttons, BUTTON_FIRE,
            "bot respawns grant no grace"
        );
        tuning.events(500, &[GameEvent::Respawned { player: 0 }]);
        let mut again = [(2, fire)];
        tuning.adjust(&game, &mut again);
        assert_eq!(again[0].1.buttons, 0);
    }

    #[test]
    fn tuned_practice_is_deterministic_and_gives_a_fresh_spawn_a_chance() {
        let run = || {
            let mut game = crate::practice();
            let mut bots = PracticeBots::new(&game.arena);
            let mut tuning = Tuning::new();
            let mut commands = Vec::new();
            let mut first_death = None;
            for _ in 0..1500 {
                bots.commands(&game, InputFrame::default(), &mut commands);
                tuning.adjust(&game, &mut commands);
                let events = game.step(&commands);
                bots.events(&events);
                tuning.events(game.tick, &events);
                if first_death.is_none()
                    && events
                        .iter()
                        .any(|e| matches!(e, GameEvent::Killed { victim: 0, .. }))
                {
                    first_death = Some(game.tick);
                }
            }
            (first_death, game.state_hash().unwrap())
        };
        let (death, hash) = run();
        assert_eq!(run(), (death, hash));
        assert!(death.is_none_or(|t| t > GRACE_TICKS), "{death:?}");
    }

    #[test]
    fn noisy_players_still_draw_fire_soon_after_the_grace() {
        let mut game = crate::practice();
        let mut bots = PracticeBots::new(&game.arena);
        let mut tuning = Tuning::new();
        let mut commands = Vec::new();
        let mut first = None;
        while first.is_none() && game.tick < 4 * ferravex_fps::TICK_RATE {
            let mut input = InputFrame::default();
            if (3..22).contains(&game.tick) {
                input.buttons = BUTTON_FIRE;
            }
            bots.commands(&game, input, &mut commands);
            tuning.adjust(&game, &mut commands);
            let events = game.step(&commands);
            bots.events(&events);
            tuning.events(game.tick, &events);
            if events
                .iter()
                .any(|e| matches!(e, GameEvent::Shot { player, .. } if *player != 0))
            {
                first = Some(game.tick);
            }
        }
        let first = first.expect("the crew answers gunfire within four seconds");
        assert!(first >= GRACE_TICKS, "{first}");
    }
}
