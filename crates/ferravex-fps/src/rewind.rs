//! Bounded server-authoritative pose rewind. The arena is always queried now.
use super::*;
mod checkpoint;
use std::collections::VecDeque;

/// 250 ms at 64 Hz. Client claims cannot extend the authority's rewind window.
pub const MAX_REWIND_TICKS: u32 = 16;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShotTime {
    pub player: u8,
    pub tick: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RewindError {
    UnknownPlayer,
    ShooterDead,
    InvalidAim,
    FutureTick,
    TooOld,
    HistoryUnavailable,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShotTrace {
    pub origin: Vec3,
    pub end: Vec3,
    pub victim: Option<u8>,
    pub headshot: bool,
}
#[derive(Debug, Default)]
pub struct RewindStep {
    pub events: Vec<GameEvent>,
    /// Invalid shot times suppress firing while movement and weapon timers advance.
    pub rejected_shots: Vec<(u8, RewindError)>,
}
#[derive(Clone, Copy)]
struct Pose {
    position: Vec3,
    alive: bool,
    life: u64,
}
struct Frame {
    tick: u32,
    players: [Option<Pose>; MAX_PLAYERS],
}
pub(crate) struct RewindHistory {
    frames: VecDeque<Frame>,
    lives: [u64; MAX_PLAYERS],
}
impl Default for RewindHistory {
    fn default() -> Self {
        Self {
            frames: VecDeque::with_capacity(MAX_REWIND_TICKS as usize + 1),
            lives: [0; MAX_PLAYERS],
        }
    }
}
impl RewindHistory {
    pub(crate) fn clear_frames(&mut self) {
        self.frames.clear();
    }
    pub(crate) fn life(&self, id: u8) -> u64 {
        self.lives.get(id as usize).copied().unwrap_or(0)
    }
    pub(crate) fn new_life(&mut self, id: u8) {
        if let Some(life) = self.lives.get_mut(id as usize) {
            // Exhaustion is unreachable at game timescales; clear history rather
            // than ever aliasing an earlier generation if it does happen.
            if *life == u64::MAX {
                self.frames.clear();
                *life = 0;
            }
            *life += 1;
        }
    }
    pub(crate) fn capture(&mut self, tick: u32, players: &[Player]) {
        if self.frames.back().is_some_and(|f| f.tick == tick) {
            return;
        }
        if self.frames.len() == MAX_REWIND_TICKS as usize + 1 {
            self.frames.pop_front();
        }
        let mut poses = [None; MAX_PLAYERS];
        for player in players {
            if let Some(pose) = poses.get_mut(player.id as usize) {
                *pose = Some(Pose {
                    position: player.position,
                    alive: player.is_alive(),
                    life: self.lives[player.id as usize],
                });
            }
        }
        self.frames.push_back(Frame {
            tick,
            players: poses,
        });
    }
    pub(crate) fn validate(&self, now: u32, tick: u32) -> Result<(), RewindError> {
        if ferravex_net::sequence_newer(tick, now) || now.wrapping_sub(tick) == 1 << 31 {
            return Err(RewindError::FutureTick);
        }
        if now.wrapping_sub(tick) > MAX_REWIND_TICKS {
            return Err(RewindError::TooOld);
        }
        if !self.frames.iter().any(|f| f.tick == tick) {
            return Err(RewindError::HistoryUnavailable);
        }
        Ok(())
    }
    fn pose(&self, tick: u32, player: &Player) -> Option<Vec3> {
        let frame = self.frames.iter().find(|f| f.tick == tick)?;
        let pose = frame.players.get(player.id as usize)?.as_ref()?;
        (pose.alive && pose.life == self.lives[player.id as usize]).then_some(pose.position)
    }
}
impl Match {
    /// Read-only authority query using the shooter's current eye and historical
    /// target poses. Current arena solids occlude old poses. Removed players,
    /// dead targets and previous lives of a respawned/reused slot cannot be hit.
    /// This function alone never spends ammunition or applies damage.
    pub fn rewind_hit_query(
        &self,
        attacker: u8,
        shot_tick: u32,
        yaw: f32,
        pitch: f32,
    ) -> Result<ShotTrace, RewindError> {
        let player = self.player(attacker).ok_or(RewindError::UnknownPlayer)?;
        if !player.is_alive() {
            return Err(RewindError::ShooterDead);
        }
        if !(InputFrame {
            yaw,
            pitch,
            ..InputFrame::default()
        })
        .is_valid()
        {
            return Err(RewindError::InvalidAim);
        }
        self.rewind.validate(self.tick, shot_tick)?;
        Ok(self.trace_shot(
            attacker,
            player.eye_position(),
            forward(yaw, pitch),
            Some(shot_tick),
        ))
    }
    pub(crate) fn trace_shot(
        &self,
        attacker: u8,
        origin: Vec3,
        direction: Vec3,
        shot_tick: Option<u32>,
    ) -> ShotTrace {
        let mut distance = self
            .arena
            .raycast(origin, direction, MAX_SHOT_DISTANCE)
            .unwrap_or(MAX_SHOT_DISTANCE);
        let mut victim = None;
        let mut headshot = false;
        for target in &self.players {
            if target.id == attacker || !target.is_alive() {
                continue;
            }
            let position = if let Some(tick) = shot_tick {
                let Some(position) = self.rewind.pose(tick, target) else {
                    continue;
                };
                position
            } else {
                target.position
            };
            let min = position - Vec3::new(PLAYER_RADIUS, 0., PLAYER_RADIUS);
            let max = position + Vec3::new(PLAYER_RADIUS, PLAYER_HEIGHT, PLAYER_RADIUS);
            if let Some(hit) = ray_aabb(origin, direction, min, max, distance)
                && hit < distance
            {
                distance = hit;
                victim = Some(target.id);
                headshot = (origin + direction * hit).y - position.y > 1.35;
            }
        }
        ShotTrace {
            origin,
            end: origin + direction * distance,
            victim,
            headshot,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn game() -> Match {
        let mut game = Match::new(Arena {
            solids: vec![],
            spawns: vec![Vec3::ZERO, Vec3::new(0., 0., -5.)],
        });
        game.spawn_player(0);
        game.spawn_player(1);
        game
    }
    fn move_victim(game: &mut Match) -> u32 {
        game.step(&[]);
        let old_tick = game.tick;
        for _ in 0..10 {
            game.step(&[(
                1,
                InputFrame {
                    move_x: 1.,
                    ..InputFrame::default()
                },
            )]);
        }
        old_tick
    }
    #[test]
    fn rewind_hits_moving_victim_and_applies_authoritative_damage() {
        let mut game = game();
        let old_tick = move_victim(&mut game);
        assert!(game.player(1).unwrap().position.x > PLAYER_RADIUS);
        assert_eq!(
            game.rewind_hit_query(0, game.tick, 0., 0.).unwrap().victim,
            None
        );
        assert_eq!(
            game.rewind_hit_query(0, old_tick, 0., 0.).unwrap().victim,
            Some(1)
        );
        let result = game.step_with_rewind(
            &[(
                0,
                InputFrame {
                    buttons: BUTTON_FIRE,
                    ..InputFrame::default()
                },
            )],
            &[ShotTime {
                player: 0,
                tick: old_tick,
            }],
        );
        assert!(result.rejected_shots.is_empty());
        assert!(result.events.iter().any(|e| matches!(
            e,
            GameEvent::Killed {
                attacker: 0,
                victim: 1
            }
        )));
        assert_eq!(game.player(1).unwrap().health, 0);
        assert_eq!(game.player(0).unwrap().ammo, MAGAZINE_SIZE - 1);
    }
    #[test]
    fn future_and_expired_times_cannot_fire_but_movement_still_advances() {
        let mut game = game();
        for _ in 0..MAX_REWIND_TICKS + 3 {
            game.step(&[]);
        }
        assert_eq!(
            game.rewind_hit_query(0, 0, 0., 0.),
            Err(RewindError::TooOld)
        );
        assert_eq!(
            game.rewind_hit_query(0, game.tick + 1, 0., 0.),
            Err(RewindError::FutureTick)
        );
        let future = game.tick + 1;
        let result = game.step_with_rewind(
            &[(
                0,
                InputFrame {
                    move_x: 1.,
                    buttons: BUTTON_FIRE,
                    ..InputFrame::default()
                },
            )],
            &[ShotTime {
                player: 0,
                tick: future,
            }],
        );
        assert_eq!(result.rejected_shots, vec![(0, RewindError::FutureTick)]);
        assert!(result.events.is_empty());
        assert_eq!(game.player(0).unwrap().ammo, MAGAZINE_SIZE);
        assert!(game.player(0).unwrap().position.x > 0.);
        assert_eq!(game.rewind.frames.len(), MAX_REWIND_TICKS as usize + 1);
    }
    #[test]
    fn current_world_occludes_historical_target_pose() {
        let mut game = game();
        let old_tick = move_victim(&mut game);
        game.arena.solids.push(Obstacle {
            center: Vec3::new(0., 1., -2.),
            half_extents: Vec3::new(0.5, 2., 0.2),
        });
        let trace = game.rewind_hit_query(0, old_tick, 0., 0.).unwrap();
        assert_eq!(trace.victim, None);
        assert!((trace.end.z + 1.8).abs() < 0.001);
        let result = game.step_with_rewind(
            &[(
                0,
                InputFrame {
                    buttons: BUTTON_FIRE,
                    ..InputFrame::default()
                },
            )],
            &[ShotTime {
                player: 0,
                tick: old_tick,
            }],
        );
        assert!(
            result
                .events
                .iter()
                .all(|e| !matches!(e, GameEvent::Hit { .. }))
        );
        assert_eq!(game.player(1).unwrap().health, 100);
    }
    #[test]
    fn rewind_never_hits_old_lives_or_reused_player_slots() {
        let mut game = game();
        game.step(&[]);
        let old = game.tick;
        game.player_mut(1).unwrap().health = 0;
        game.player_mut(1).unwrap().respawn_ticks = 1;
        game.step(&[]);
        assert_eq!(game.rewind_hit_query(0, old, 0., 0.).unwrap().victim, None);
        let before_reuse = game.tick;
        game.remove_player(1);
        game.spawn_player(1);
        game.step(&[]);
        // Recorded tick poses stay immutable across removal and slot reuse.
        assert_eq!(game.rewind_hit_query(0, old, 0., 0.).unwrap().victim, None);
        assert_eq!(
            game.rewind_hit_query(0, before_reuse, 0., 0.)
                .unwrap()
                .victim,
            None
        );
    }
    #[test]
    fn wraparound_tick_window_remains_bounded_and_queries_do_not_mutate() {
        let mut game = Match::new(Arena {
            solids: vec![],
            spawns: vec![Vec3::ZERO, Vec3::new(0., 0., -5.)],
        });
        game.tick = u32::MAX - 3;
        game.spawn_player(0);
        game.spawn_player(1);
        let before_wrap = game.tick;
        for _ in 0..6 {
            game.step(&[]);
        }
        let players = game.players.clone();
        assert_eq!(
            game.rewind_hit_query(0, before_wrap, 0., 0.)
                .unwrap()
                .victim,
            Some(1)
        );
        assert_eq!(
            game.rewind_hit_query(0, game.tick + 1, 0., 0.),
            Err(RewindError::FutureTick)
        );
        assert_eq!(
            game.rewind_hit_query(0, before_wrap, f32::NAN, 0.),
            Err(RewindError::InvalidAim)
        );
        assert_eq!(game.players, players);
    }
}
