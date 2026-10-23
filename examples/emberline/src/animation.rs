//! Procedural sprite poses indexed by the shared timed sprite player. Rendering
//! keeps the original hand-drawn shapes; no texture download or atlas is needed.
use super::Game;
use ferravex_play::animation::{
    AnimationEvent, EventBuffer, LoopMode, SpriteClip, SpriteFrame, SpritePlayer,
};
const FOOTSTEP: u32 = 1;
pub(super) struct EmberAnimation {
    players: [SpritePlayer; 5],
    state: usize,
    events: EventBuffer<4>,
    facing: f32,
    grounded: bool,
    motion_enabled: bool,
    landing_visible: bool,
}
impl EmberAnimation {
    pub fn new() -> Self {
        let player = |frames: &[(u32, f32)], mode, events| {
            SpritePlayer::new(
                SpriteClip::new(
                    frames
                        .iter()
                        .map(|(index, duration)| SpriteFrame {
                            index: *index,
                            duration: *duration,
                        })
                        .collect(),
                )
                .unwrap(),
                mode,
                events,
            )
            .unwrap()
        };
        Self {
            players: [
                player(&[(0, 0.8), (1, 0.8)], LoopMode::Repeat, vec![]),
                player(
                    &[(2, 0.115), (3, 0.115), (4, 0.115), (5, 0.115)],
                    LoopMode::Repeat,
                    vec![
                        AnimationEvent {
                            time: 0.115,
                            id: FOOTSTEP,
                        },
                        AnimationEvent {
                            time: 0.345,
                            id: FOOTSTEP,
                        },
                    ],
                ),
                player(&[(8, 1.)], LoopMode::Clamp, vec![]),
                player(&[(9, 1.)], LoopMode::Clamp, vec![]),
                player(&[(10, 0.08), (11, 0.08)], LoopMode::Clamp, vec![]),
            ],
            state: 0,
            events: EventBuffer::default(),
            facing: 1.,
            grounded: true,
            motion_enabled: true,
            landing_visible: true,
        }
    }
    pub fn reset(&mut self) {
        for p in &mut self.players {
            p.timeline_mut().restart();
        }
        self.state = 0;
        self.events.clear();
        self.facing = 1.;
        self.grounded = true;
        self.landing_visible = self.motion_enabled;
    }
    pub fn set_motion_enabled(&mut self, enabled: bool) {
        self.motion_enabled = enabled;
        if !enabled {
            self.landing_visible = false;
        }
    }
    pub fn update(&mut self, game: &Game, dt: f32) {
        let velocity = game.velocity();
        if velocity.x.abs() > 1. {
            self.facing = velocity.x.signum();
        }
        let next = if !game.motor.grounded {
            if velocity.y < 0. { 2 } else { 3 }
        } else if !self.grounded || self.state == 4 && !self.players[4].timeline().finished() {
            4
        } else if velocity.x.abs() > 10. && !game.won() {
            1
        } else {
            0
        };
        self.grounded = game.motor.grounded;
        if next != self.state {
            self.players[next].timeline_mut().restart();
            self.state = next;
            if next == 4 {
                self.landing_visible = self.motion_enabled;
            }
        }
        self.players[self.state]
            .timeline_mut()
            .set_speed(if self.state == 1 {
                (velocity.x.abs() / super::SPEED).clamp(0., 2.)
            } else {
                1.
            })
            .unwrap();
        self.players[self.state]
            .advance(dt, &mut self.events)
            .unwrap();
    }
    pub fn frame(&self) -> u32 {
        if (!self.motion_enabled && !matches!(self.state, 2 | 3))
            || self.state == 4 && !self.landing_visible
        {
            0
        } else {
            self.players[self.state].frame()
        }
    }
    pub fn facing(&self) -> f32 {
        self.facing
    }
    pub fn footsteps(&self) -> usize {
        self.events
            .hits()
            .iter()
            .filter(|e| e.id == FOOTSTEP)
            .count()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reduced_motion_keeps_footstep_timing_and_discards_old_landing_poses() {
        let mut game = Game::new(true);
        let mut regular = EmberAnimation::new();
        let mut reduced = EmberAnimation::new();
        reduced.set_motion_enabled(false);
        game.motor.grounded = true;
        game.motor.velocity.x = -super::super::SPEED;
        let mut footsteps = 0;
        for _ in 0..120 {
            regular.update(&game, 1. / 120.);
            reduced.update(&game, 1. / 120.);
            assert_eq!(regular.footsteps(), reduced.footsteps());
            footsteps += reduced.footsteps();
            assert_eq!(reduced.frame(), 0);
        }
        assert_eq!(footsteps, 4);
        assert_eq!(reduced.facing(), -1.);
        game.motor.grounded = false;
        game.motor.velocity.y = -20.;
        reduced.update(&game, 1. / 120.);
        assert_eq!(
            reduced.frame(),
            9,
            "essential airborne pose remains readable"
        );
        game.motor.grounded = true;
        game.motor.velocity.x = 0.;
        reduced.update(&game, 1. / 120.);
        assert_eq!(reduced.frame(), 0);
        reduced.set_motion_enabled(true);
        assert_eq!(
            reduced.frame(),
            0,
            "hidden landing cannot revive on re-enable"
        );
        reduced.reset();
        assert_eq!(reduced.frame(), 0);
        game.motor.grounded = false;
        reduced.update(&game, 1. / 120.);
        game.motor.grounded = true;
        reduced.update(&game, 1. / 120.);
        assert_eq!(
            reduced.frame(),
            10,
            "a new accepted landing can animate again"
        );
    }

    #[test]
    fn sprite_states_run_air_land_pause_and_retain_facing() {
        let mut game = Game::new(true);
        let mut anim = EmberAnimation::new();
        game.motor.grounded = true;
        game.motor.velocity.x = -super::super::SPEED;
        let mut steps = 0;
        for _ in 0..60 {
            anim.update(&game, 1. / 60.);
            steps += anim.footsteps();
        }
        assert_eq!(steps, 4);
        assert_eq!(anim.facing(), -1.);
        assert!((2..=5).contains(&anim.frame()));
        let time = anim.players[1].timeline().time();
        anim.update(&game, 0.);
        assert_eq!(anim.players[1].timeline().time(), time);
        assert_eq!(anim.footsteps(), 0);
        game.motor.grounded = false;
        game.motor.velocity.y = 20.;
        anim.update(&game, 1. / 60.);
        assert_eq!(anim.frame(), 8);
        game.motor.velocity.y = -20.;
        anim.update(&game, 1. / 60.);
        assert_eq!(anim.frame(), 9);
        game.motor.grounded = true;
        game.motor.velocity.x = 0.;
        anim.update(&game, 1. / 60.);
        assert_eq!(anim.frame(), 10);
        for _ in 0..20 {
            anim.update(&game, 1. / 60.);
        }
        assert!(anim.frame() <= 1);
        assert_eq!(anim.facing(), -1.);
        anim.reset();
        assert_eq!(anim.frame(), 0);
        assert_eq!(anim.facing(), 1.);
    }
}
