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
        } else if velocity.x.abs() > 10. && !game.won {
            1
        } else {
            0
        };
        self.grounded = game.motor.grounded;
        if next != self.state {
            self.players[next].timeline_mut().restart();
            self.state = next;
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
        self.players[self.state].frame()
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
    fn sprite_states_run_air_land_pause_and_retain_facing() {
        let mut game = Game::new();
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
