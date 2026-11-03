//! Fixed-tick racket sport rules, independent of rendering and platform input.
//!
//! ```
//! use mooncourt::{Game, Phase};
//! use ferravex_arcade::{Input, Outcome};
//! let mut game = Game::new();
//! assert_eq!(game.phase(), Phase::Briefing);
//! game.step(&Input { primary: true, ..Input::default() });
//! assert_eq!(game.phase(), Phase::Serve);
//! game.step(&Input { primary: true, slot: Some(3), ..Input::default() });
//! assert_eq!(game.phase(), Phase::Rally);
//! assert_eq!(game.outcome(), Outcome::Playing);
//! ```
use crate::content::{OPPONENTS, Opponent};
use ferravex_arcade::{Cue, Input, Outcome, TICK_DT};
use ferravex_math::Vec2;
use ferravex_play::motion::approach_vec2;

/// Campaign and point phases. Briefings/training wait for a deliberate command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Briefing,
    Serve,
    Rally,
    Point,
    Training,
    Setback,
    Champion,
    Eliminated,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Shot {
    Drive,
    Lob,
    Smash,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Athlete {
    pub pos: Vec2,
    pub velocity: Vec2,
    pub stamina: f32,
    pub swing: u16,
    pub shot: Shot,
    pub cooldown: u16,
    pub flash: u16,
}
impl Athlete {
    fn new(side: usize) -> Self {
        Self {
            pos: Vec2::new(0., if side == 0 { 100. } else { -100. }),
            velocity: Vec2::ZERO,
            stamina: 100.,
            swing: 0,
            shot: Shot::Drive,
            cooldown: 0,
            flash: 0,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Ball {
    pub pos: Vec2,
    pub velocity: Vec2,
    pub height: f32,
    pub vertical: f32,
    pub target: Vec2,
    pub hitter: usize,
    pub bounced: bool,
    pub flight_ticks: u16,
}
impl Default for Ball {
    fn default() -> Self {
        Self {
            pos: Vec2::ZERO,
            velocity: Vec2::ZERO,
            height: 18.,
            vertical: 0.,
            target: Vec2::ZERO,
            hitter: 0,
            bounced: false,
            flight_ticks: 0,
        }
    }
}
/// A complete six-match tournament. Call [`Game::step`] once per executed 60 Hz tick.
#[derive(Clone, Debug, PartialEq)]
pub struct Game {
    pub(crate) phase: Phase,
    pub(crate) match_index: usize,
    pub(crate) athletes: [Athlete; 2],
    pub(crate) ball: Ball,
    pub(crate) points: [u16; 2],
    pub(crate) games: [u8; 2],
    pub(crate) server: usize,
    pub(crate) aim: usize,
    pub(crate) timer: u16,
    pub(crate) rally: u16,
    pub(crate) best_rally: u16,
    pub(crate) ai_delay: u16,
    pub(crate) ai_target: Vec2,
    pub(crate) training: [u8; 3],
    pub(crate) medals: [u8; 6],
    pub(crate) retries: u8,
    pub(crate) score: u32,
    pub(crate) match_score_start: u32,
    pub(crate) note: &'static str,
    pub(crate) tick: u64,
    pub(crate) total_returns: u32,
    pub(crate) point_winner: usize,
    pub(crate) perfect: bool,
}
impl Default for Game {
    fn default() -> Self {
        Self::new()
    }
}
impl Game {
    /// Start a fresh tournament, before the Tycho School briefing.
    pub fn new() -> Self {
        Self {
            phase: Phase::Briefing,
            match_index: 0,
            athletes: [Athlete::new(0), Athlete::new(1)],
            ball: Ball::default(),
            points: [0; 2],
            games: [0; 2],
            server: 0,
            aim: 1,
            timer: 0,
            rally: 0,
            best_rally: 0,
            ai_delay: 0,
            ai_target: Vec2::new(0., -100.),
            training: [0; 3],
            medals: [0; 6],
            retries: 3,
            score: 0,
            match_score_start: 0,
            note: "Move to the landing ring. Aim wide, then time your swing.",
            tick: 0,
            total_returns: 0,
            point_winner: 0,
            perfect: false,
        }
    }
    /// Current rules phase (the shared host separately owns pause/help).
    pub fn phase(&self) -> Phase {
        self.phase
    }
    /// One-based match number, including the completed final.
    pub fn match_number(&self) -> usize {
        self.match_index + 1
    }
    /// Current game totals, player first.
    pub fn games(&self) -> [u8; 2] {
        self.games
    }
    /// Current point counts, player first. Four with a two-point lead wins a game.
    pub fn points(&self) -> [u16; 2] {
        self.points
    }
    /// Current campaign score. Retrying restores the beginning-of-match score.
    pub fn score(&self) -> u32 {
        self.score
    }
    /// Terminal outcome; later commands cannot mutate a terminal tournament.
    pub fn outcome(&self) -> Outcome {
        match self.phase {
            Phase::Champion => Outcome::Won,
            Phase::Eliminated => Outcome::Lost,
            _ => Outcome::Playing,
        }
    }
    /// Human-readable tournament result for the shared ending screen.
    pub fn summary(&self) -> String {
        if self.phase == Phase::Champion {
            format!(
                "Earthrise champion! Six opponents defeated. {} medals, {} points. {} legal returns; longest rally {}. The Mooncourt cup is yours.",
                self.medals.iter().map(|&m| u32::from(m)).sum::<u32>(),
                self.score,
                self.total_returns,
                self.best_rally
            )
        } else {
            format!(
                "Tour ended at {} against {}. {} matches won, {} points. All wildcard entries used. A new tournament restores the full draw.",
                self.opponent().court,
                self.opponent().name,
                self.match_index,
                self.score
            )
        }
    }
    pub(crate) fn opponent(&self) -> &'static Opponent {
        &OPPONENTS[self.match_index]
    }
    pub(crate) fn point_text(&self, side: usize) -> &'static str {
        let a = self.points[side];
        let b = self.points[1 - side];
        if a >= 3 && b >= 3 {
            if a > b { "AD" } else { "40" }
        } else {
            match a {
                0 => "0",
                1 => "15",
                2 => "30",
                _ => "40",
            }
        }
    }
    pub(crate) fn speed(&self) -> f32 {
        185. + f32::from(self.training[0]) * 15.
    }
    pub(crate) fn reach(&self) -> f32 {
        31. + f32::from(self.training[2]) * 4.
    }
    fn setup_serve(&mut self) {
        self.phase = Phase::Serve;
        self.timer = 0;
        self.rally = 0;
        self.perfect = false;
        for side in 0..2 {
            let stamina = (self.athletes[side].stamina + 30.).min(100.);
            self.athletes[side] = Athlete::new(side);
            self.athletes[side].stamina = stamina;
        }
        let parity = (self.points[0] + self.points[1]) % 2;
        self.athletes[self.server].pos.x = if parity == 0 { -65. } else { 65. };
        self.ball = Ball {
            pos: self.athletes[self.server].pos,
            hitter: self.server,
            ..Ball::default()
        };
        self.note = if self.server == 0 {
            "Your serve. Choose 1/2/3 target; Space serves."
        } else {
            "Opponent serves shortly. Get ready to receive."
        };
    }
    fn new_match(&mut self) {
        self.points = [0; 2];
        self.games = [0; 2];
        self.server = self.match_index % 2;
        self.athletes = [Athlete::new(0), Athlete::new(1)];
        self.phase = Phase::Briefing;
        self.timer = 0;
        self.match_score_start = self.score;
    }
    /// Consume one ordinary command. There is no autoplay mode in the rules.
    pub fn step(&mut self, input: &Input) -> Cue {
        if self.outcome() != Outcome::Playing {
            return Cue::None;
        }
        match self.phase {
            Phase::Briefing => {
                if input.primary
                    || (input.click
                        && (180. ..=630.).contains(&input.pointer.x)
                        && (480. ..=513.).contains(&input.pointer.y))
                {
                    self.setup_serve();
                    return Cue::Action;
                }
                return Cue::None;
            }
            Phase::Training => {
                let choice = input.slot.or_else(|| {
                    (input.click
                        && (225. ..=463.).contains(&input.pointer.y)
                        && (123. ..=622.).contains(&input.pointer.x)
                        && (input.pointer.y - 225.) % 83. <= 72.)
                        .then_some(((input.pointer.y - 225.) / 83.) as usize + 1)
                });
                if let Some(choice) = choice.filter(|&c| (1..=3).contains(&c)) {
                    if self.training[choice - 1] >= 3 {
                        return Cue::None;
                    }
                    self.training[choice - 1] += 1;
                    self.match_index += 1;
                    self.new_match();
                    return Cue::Good;
                }
                if input.primary {
                    self.match_index += 1;
                    self.new_match();
                    return Cue::Action;
                }
                return Cue::None;
            }
            Phase::Setback => {
                if input.primary {
                    self.retries -= 1;
                    self.score = self.match_score_start;
                    self.new_match();
                    return Cue::Action;
                }
                return Cue::None;
            }
            _ => {}
        }
        self.tick += 1;
        if self.phase == Phase::Point {
            self.timer += 1;
            if self.timer >= 90 {
                return self.after_point();
            }
            return Cue::None;
        }
        if let Some(aim) = input.slot.filter(|&s| (1..=3).contains(&s)) {
            self.aim = aim - 1;
        }
        if input.click
            && (520. ..=558.).contains(&input.pointer.y)
            && (120. ..=590.).contains(&input.pointer.x)
        {
            self.aim = (((input.pointer.x - 120.) / 157.) as usize).min(2);
        }
        for side in 0..2 {
            let recovery = if side == 0 {
                13. + f32::from(self.training[1]) * 5.
            } else {
                13.
            };
            let athlete = &mut self.athletes[side];
            athlete.stamina = (athlete.stamina + recovery * TICK_DT).min(100.);
            athlete.cooldown = athlete.cooldown.saturating_sub(1);
            athlete.flash = athlete.flash.saturating_sub(1);
        }
        self.move_player(input);
        if self.phase == Phase::Serve {
            self.timer += 1;
            self.ball.pos = self.athletes[self.server].pos;
            if self.server == 0 && (input.primary || input.secondary || input.tertiary) {
                self.launch(0, Shot::Drive, true);
                self.phase = Phase::Rally;
                return Cue::Action;
            }
            if self.server == 1 && self.timer >= 90 {
                self.launch(1, Shot::Drive, true);
                self.phase = Phase::Rally;
                return Cue::Action;
            }
            return Cue::None;
        }
        self.move_opponent();
        let mut cue = Cue::None;
        if self.athletes[0].cooldown == 0 && (input.primary || input.secondary || input.tertiary) {
            let player = &mut self.athletes[0];
            player.shot = if input.tertiary {
                Shot::Smash
            } else if input.secondary {
                Shot::Lob
            } else {
                Shot::Drive
            };
            player.swing = 10;
            player.cooldown = 20;
            player.stamina = (player.stamina - 2.).max(0.);
            player.flash = 14;
            cue = Cue::Action;
        }
        if self.athletes[0].swing > 0 {
            if self.can_hit(0) {
                let shot = self.athletes[0].shot;
                self.launch(0, shot, false);
                cue = Cue::Good;
            }
            self.athletes[0].swing = self.athletes[0].swing.saturating_sub(1);
        }
        if self.ai_delay == 0 && self.can_hit(1) {
            let shot = self.ai_shot();
            self.launch(1, shot, false);
            cue = Cue::Action;
        }
        if let Some(winner) = self.fly_ball() {
            return self.award_point(winner);
        }
        cue
    }
    fn move_player(&mut self, input: &Input) {
        let mut direction = Vec2::new(
            f32::from(input.right) - f32::from(input.left),
            f32::from(input.down) - f32::from(input.up),
        );
        if direction.length_squared() > 1. {
            direction = direction.normalize();
        }
        let speed = self.speed()
            * if self.athletes[0].stamina < 12. {
                0.8
            } else {
                1.
            };
        let player = &mut self.athletes[0];
        player.velocity = approach_vec2(player.velocity, direction * speed, 1200., TICK_DT)
            .expect("bounded motion");
        player.pos += player.velocity * TICK_DT;
        player.pos.x = player.pos.x.clamp(-175., 175.);
        player.pos.y = player.pos.y.clamp(22., 125.);
        if self.phase == Phase::Serve && self.server == 0 {
            player.pos.y = 110.;
            player.velocity.y = 0.;
        }
    }
    fn move_opponent(&mut self) {
        if self.ai_delay > 0 {
            self.ai_delay -= 1;
        }
        if self.ball.hitter == 0 && self.ai_delay == 0 {
            self.ai_target = self.ball.target;
        }
        if self.ball.hitter == 1 {
            self.ai_target = Vec2::new(
                0.,
                if self.opponent().style == 4 {
                    -65.
                } else {
                    -100.
                },
            );
        }
        let rate = self.opponent().speed
            * if self.athletes[1].stamina < 12. {
                0.8
            } else {
                1.
            };
        let opponent = &mut self.athletes[1];
        let delta = self.ai_target - opponent.pos;
        let velocity = if delta.length() > 2. {
            delta.normalize() * rate
        } else {
            Vec2::ZERO
        };
        opponent.velocity =
            approach_vec2(opponent.velocity, velocity, 1000., TICK_DT).expect("bounded AI motion");
        opponent.pos += opponent.velocity * TICK_DT;
        opponent.pos.x = opponent.pos.x.clamp(-175., 175.);
        opponent.pos.y = opponent.pos.y.clamp(-125., -22.);
    }
    pub(crate) fn can_hit(&self, side: usize) -> bool {
        if self.ball.hitter == side
            || self.ball.flight_ticks < 8
            || self.ball.height > 43.
            || self.ball.height < 0.
        {
            return false;
        }
        if (side == 0 && self.ball.pos.y < 8.) || (side == 1 && self.ball.pos.y > -8.) {
            return false;
        }
        let reach = if side == 0 { self.reach() } else { 28. };
        (self.ball.pos - self.athletes[side].pos).length() <= reach
    }
    fn ai_shot(&self) -> Shot {
        match self.opponent().style {
            3 if self.rally % 3 != 1 => Shot::Lob,
            4 | 5 if self.ball.height >= 20. && self.athletes[1].stamina >= 30. => Shot::Smash,
            5 if self.rally % 4 == 3 => Shot::Lob,
            _ => Shot::Drive,
        }
    }
    fn launch(&mut self, side: usize, requested: Shot, serving: bool) {
        let mut shot = requested;
        if shot == Shot::Smash && (self.ball.height < 14. || self.athletes[side].stamina < 26.) {
            shot = Shot::Drive;
        }
        let cost = match shot {
            Shot::Drive => 7.,
            Shot::Lob => 4.,
            Shot::Smash => 26.,
        };
        self.athletes[side].stamina = (self.athletes[side].stamina - cost).max(0.);
        self.athletes[side].swing = 0;
        self.athletes[side].flash = 14;
        let aim = if side == 0 {
            self.aim
        } else {
            match self.opponent().style {
                0 => {
                    if (u32::from(self.points.iter().sum::<u16>()) + u32::from(self.rally))
                        .is_multiple_of(3)
                    {
                        0
                    } else {
                        2
                    }
                }
                2 => {
                    if self.rally.is_multiple_of(2) {
                        0
                    } else {
                        2
                    }
                }
                _ => {
                    if self.athletes[0].pos.x >= 0. {
                        0
                    } else {
                        2
                    }
                }
            }
        };
        let depth = if side == 1 && self.opponent().style == 4 && self.rally % 2 == 1 {
            52.
        } else {
            104.
        };
        let target = Vec2::new(
            [-146., 0., 146.][aim],
            if side == 0 { -depth } else { depth },
        );
        let duration = if serving {
            1.6
        } else {
            match shot {
                Shot::Drive => 1.25,
                Shot::Lob => 2.35,
                Shot::Smash => 0.86,
            }
        };
        self.perfect = !serving && (18. ..=34.).contains(&self.ball.height);
        let start_height = if serving {
            22.
        } else {
            self.ball.height.max(5.)
        };
        self.ball.height = start_height;
        self.ball.velocity = (target - self.ball.pos) / duration;
        self.ball.vertical =
            (self.opponent().gravity * 0.5 * duration * duration - start_height) / duration;
        self.ball.target = target;
        self.ball.hitter = side;
        self.ball.bounced = false;
        self.ball.flight_ticks = 0;
        self.rally = self.rally.saturating_add(1);
        self.best_rally = self.best_rally.max(self.rally);
        if side == 0 && !serving {
            self.total_returns = self.total_returns.saturating_add(1);
        }
        self.ai_delay = self.opponent().reaction;
        self.note = if serving {
            "Serve is live. One bounce allowed; a second loses the point."
        } else {
            match (side, shot) {
                (0, Shot::Smash) => "Your smash! Fast and costly; recover toward center.",
                (0, Shot::Lob) => "Your lob gives time to recover. Watch the landing ring.",
                (0, _) => "Clean drive. Change target to move your opponent.",
                (_, Shot::Smash) => "Incoming smash! Move toward the landing ring now.",
                (_, Shot::Lob) => "Incoming moonball. Wait for the ball to descend.",
                _ => "Incoming drive. Move, choose a target, then time your swing.",
            }
        };
    }
    fn fly_ball(&mut self) -> Option<usize> {
        let previous = self.ball.pos;
        let previous_height = self.ball.height;
        self.ball.flight_ticks = self.ball.flight_ticks.saturating_add(1);
        self.ball.pos += self.ball.velocity * TICK_DT;
        self.ball.height +=
            self.ball.vertical * TICK_DT - self.opponent().gravity * 0.5 * TICK_DT * TICK_DT;
        self.ball.vertical -= self.opponent().gravity * TICK_DT;
        if previous.y.signum() != self.ball.pos.y.signum() {
            let t = previous.y.abs() / (previous.y.abs() + self.ball.pos.y.abs());
            if previous_height + (self.ball.height - previous_height) * t < 12. {
                self.note = "Net fault. A low, close smash may not clear the net.";
                return Some(1 - self.ball.hitter);
            }
        }
        if self.ball.height <= 0. {
            if self.ball.bounced {
                self.note = "Second bounce. Point to the last player to return it.";
                return Some(self.ball.hitter);
            }
            let correct_side = if self.ball.hitter == 0 {
                self.ball.pos.y < 0.
            } else {
                self.ball.pos.y > 0.
            };
            if !correct_side || self.ball.pos.x.abs() > 180. || self.ball.pos.y.abs() > 125. {
                self.note = "Out. The shot landed beyond the singles lines.";
                return Some(1 - self.ball.hitter);
            }
            self.ball.bounced = true;
            self.ball.height = 0.;
            self.ball.vertical = 24.;
            self.ball.velocity *= 0.45;
        }
        if self.ball.pos.y.abs() > 190. || self.ball.pos.x.abs() > 235. {
            self.note = "Ball passed the receiver. Point to the last return.";
            return Some(self.ball.hitter);
        }
        None
    }
    fn award_point(&mut self, winner: usize) -> Cue {
        self.points[winner] += 1;
        // Compress repeated deuce without changing advantage, bounding the counters.
        if self.points[0] >= 4 && self.points[1] >= 4 {
            self.points[0] -= 1;
            self.points[1] -= 1;
        }
        self.point_winner = winner;
        self.timer = 0;
        self.phase = Phase::Point;
        if winner == 0 {
            self.score = self
                .score
                .saturating_add(50 + u32::from(self.rally.min(20)) * 2);
            Cue::Good
        } else {
            Cue::Bad
        }
    }
    fn after_point(&mut self) -> Cue {
        let winner = self.point_winner;
        if self.points[winner] >= 4 && self.points[winner] >= self.points[1 - winner] + 2 {
            self.games[winner] += 1;
            self.points = [0; 2];
            self.server = 1 - self.server;
            if winner == 0 {
                self.score += 200;
            }
            if self.games[winner] == 2 {
                if winner == 0 {
                    self.medals[self.match_index] = if self.games[1] == 0 { 3 } else { 2 };
                    self.score += 800 + u32::from(self.medals[self.match_index]) * 100;
                    self.phase = if self.match_index == 5 {
                        Phase::Champion
                    } else {
                        Phase::Training
                    };
                    return if self.phase == Phase::Champion {
                        Cue::Win
                    } else {
                        Cue::Good
                    };
                }
                self.phase = if self.retries == 0 {
                    Phase::Eliminated
                } else {
                    Phase::Setback
                };
                return Cue::Bad;
            }
        }
        self.setup_serve();
        Cue::Action
    }
    /// Generate ordinary movement, target and swing controls, without changing state.
    /// This sees the same landing marker that is drawn for a human player.
    pub fn pilot(&self, tick: u64) -> Input {
        match self.phase {
            Phase::Briefing | Phase::Setback => {
                return Input {
                    primary: tick.is_multiple_of(30),
                    ..Input::default()
                };
            }
            Phase::Training => {
                return Input {
                    slot: (tick.is_multiple_of(30)).then_some(if self.training[0] < 2 {
                        1
                    } else if self.training[2] < 2 {
                        3
                    } else {
                        2
                    }),
                    ..Input::default()
                };
            }
            Phase::Serve if self.server == 0 => {
                return Input {
                    primary: self.timer > 40,
                    slot: Some(if self.athletes[1].pos.x >= 0. { 1 } else { 3 }),
                    ..Input::default()
                };
            }
            _ => {}
        }
        if self.phase != Phase::Rally && self.phase != Phase::Serve {
            return Input::default();
        }
        let target = if self.phase == Phase::Serve {
            Vec2::new(0., 104.)
        } else if self.ball.hitter == 1 {
            Vec2::new(self.ball.target.x, (self.ball.target.y - 30.).max(25.))
        } else {
            Vec2::new(0., 102.)
        };
        let pos = self.athletes[0].pos;
        let mut input = Input {
            left: pos.x > target.x + 3.,
            right: pos.x < target.x - 3.,
            up: pos.y > target.y + 3.,
            down: pos.y < target.y - 3.,
            slot: Some(if self.athletes[1].pos.x >= 0. { 1 } else { 3 }),
            ..Input::default()
        };
        if self.can_hit(0) && self.athletes[0].cooldown == 0 {
            if self.ball.height >= 14. && self.athletes[0].stamina >= 28. {
                input.tertiary = true;
            } else {
                input.primary = true;
            }
        }
        input
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn primary() -> Input {
        Input {
            primary: true,
            ..Input::default()
        }
    }
    fn live() -> Game {
        let mut game = Game::new();
        game.step(&primary());
        game.step(&primary());
        game
    }
    fn incoming() -> Game {
        let mut game = live();
        game.ball = Ball {
            pos: Vec2::new(0., 80.),
            velocity: Vec2::new(0., 90.),
            height: 25.,
            vertical: -20.,
            target: Vec2::new(0., 104.),
            hitter: 1,
            bounced: false,
            flight_ticks: 60,
        };
        game.athletes[0].pos = game.ball.pos;
        game.athletes[0].stamina = 100.;
        game
    }
    #[test]
    fn briefing_and_invalid_choices_are_inert() {
        let mut game = Game::new();
        let original = game.clone();
        for slot in [None, Some(0), Some(5), Some(999)] {
            assert_eq!(
                game.step(&Input {
                    slot,
                    click: true,
                    pointer: Vec2::new(f32::NAN, 0.),
                    ..Input::default()
                }),
                Cue::None
            );
        }
        assert_eq!(game, original);
    }
    #[test]
    fn serves_require_press_edges_and_start_from_baseline() {
        let mut game = Game::new();
        game.step(&primary());
        for _ in 0..120 {
            game.step(&Input {
                primary_down: true,
                up: true,
                ..Input::default()
            });
        }
        assert_eq!(game.phase, Phase::Serve);
        assert_eq!(game.athletes[0].pos.y, 110.);
        assert_eq!(game.points, [0, 0]);
        game.step(&primary());
        assert_eq!(game.phase, Phase::Rally);
    }
    #[test]
    fn movement_has_acceleration_and_cannot_cross_net_or_lines() {
        let mut game = incoming();
        game.ball.hitter = 0;
        let input = Input {
            up: true,
            right: true,
            ..Input::default()
        };
        game.move_player(&input);
        assert!(game.athletes[0].velocity.length() <= 20.01);
        for _ in 0..600 {
            game.move_player(&input);
        }
        assert_eq!(game.athletes[0].pos, Vec2::new(175., 22.));
        assert!((game.athletes[0].velocity.length() - game.speed()).abs() < 0.01);
    }
    #[test]
    fn racket_requires_incoming_ball_height_and_actual_reach() {
        let mut game = incoming();
        assert!(game.can_hit(0));
        game.ball.hitter = 0;
        assert!(!game.can_hit(0));
        game.ball.hitter = 1;
        game.ball.height = 44.;
        assert!(!game.can_hit(0));
        game.ball.height = 25.;
        game.ball.flight_ticks = 7;
        assert!(!game.can_hit(0));
        game.ball.flight_ticks = 8;
        game.ball.pos.x = game.reach() + 1.;
        assert!(!game.can_hit(0));
        game.ball.pos.x = 0.;
        assert!(game.can_hit(0));
    }
    #[test]
    fn swing_is_bounded_and_holding_does_not_repeat_or_farm_returns() {
        let mut game = incoming();
        game.step(&primary());
        assert_eq!(game.total_returns, 1);
        assert_eq!(game.ball.hitter, 0);
        assert_eq!(game.athletes[0].swing, 0);
        for _ in 0..12 {
            game.step(&Input {
                primary_down: true,
                ..Input::default()
            });
        }
        assert_eq!(game.total_returns, 1);
        assert_eq!(game.score, 0);
        let mut missed = live();
        missed.athletes[0].pos = Vec2::new(-170., 125.);
        missed.step(&primary());
        for _ in 0..12 {
            missed.step(&Input::default());
        }
        assert_eq!(missed.athletes[0].swing, 0);
        assert_eq!(missed.total_returns, 0);
    }
    #[test]
    fn shot_types_have_distinct_flight_and_stamina_costs() {
        let base = incoming();
        let mut drive = base.clone();
        let mut lob = base.clone();
        let mut smash = base;
        drive.launch(0, Shot::Drive, false);
        lob.launch(0, Shot::Lob, false);
        smash.launch(0, Shot::Smash, false);
        assert_eq!(drive.athletes[0].stamina, 93.);
        assert_eq!(lob.athletes[0].stamina, 96.);
        assert_eq!(smash.athletes[0].stamina, 74.);
        assert!(smash.ball.velocity.length() > drive.ball.velocity.length());
        assert!(drive.ball.velocity.length() > lob.ball.velocity.length());
        assert!(lob.ball.vertical > drive.ball.vertical);
        let target = drive.ball.target;
        for _ in 0..75 {
            assert_eq!(drive.fly_ball(), None);
        }
        assert!((drive.ball.pos - target).length() < 0.1);
        assert!(drive.ball.height.abs() < 0.1);
    }
    #[test]
    fn low_or_tired_smashes_fall_back_to_drives_without_overspending() {
        for (height, stamina) in [(10., 100.), (25., 20.)] {
            let mut smash = incoming();
            smash.ball.height = height;
            smash.athletes[0].stamina = stamina;
            let mut drive = smash.clone();
            smash.launch(0, Shot::Smash, false);
            drive.launch(0, Shot::Drive, false);
            assert_eq!(smash.ball, drive.ball);
            assert_eq!(smash.athletes[0].stamina, stamina - 7.);
        }
    }
    #[test]
    fn net_out_and_second_bounce_award_the_correct_side() {
        let mut game = incoming();
        game.ball.pos = Vec2::new(0., 1.);
        game.ball.velocity = Vec2::new(0., -120.);
        game.ball.height = 5.;
        game.ball.vertical = 0.;
        game.ball.hitter = 0;
        assert_eq!(game.fly_ball(), Some(1));
        game.ball.pos = Vec2::new(181., -100.);
        game.ball.velocity = Vec2::ZERO;
        game.ball.height = 0.1;
        game.ball.vertical = -30.;
        assert_eq!(game.fly_ball(), Some(1));
        game.ball.pos = Vec2::new(0., -100.);
        game.ball.bounced = false;
        game.ball.height = 0.1;
        assert_eq!(game.fly_ball(), None);
        assert!(game.ball.bounced);
        let mut result = None;
        for _ in 0..90 {
            if let Some(w) = game.fly_ball() {
                result = Some(w);
                break;
            }
        }
        assert_eq!(result, Some(0));
    }
    #[test]
    fn deuce_advantage_and_service_change_follow_game_rules() {
        let mut game = live();
        game.points = [3, 3];
        game.award_point(0);
        assert_eq!(game.point_text(0), "AD");
        game.after_point();
        assert_eq!(game.games, [0, 0]);
        game.award_point(1);
        assert_eq!(game.points, [3, 3]);
        assert_eq!(game.point_text(1), "40");
        game.after_point();
        game.award_point(0);
        game.after_point();
        game.award_point(0);
        game.after_point();
        assert_eq!(game.games, [1, 0]);
        assert_eq!(game.points, [0, 0]);
        assert_eq!(game.server, 1);
    }
    #[test]
    fn two_games_win_match_and_training_clicks_match_cards() {
        let mut game = live();
        game.games = [1, 0];
        game.points = [3, 0];
        game.award_point(0);
        game.after_point();
        assert_eq!(game.phase, Phase::Training);
        assert_eq!(game.medals[0], 3);
        let before = game.clone();
        game.step(&Input {
            slot: Some(5),
            ..Input::default()
        });
        assert_eq!(game, before);
        game.step(&Input {
            click: true,
            pointer: Vec2::new(200., 340.),
            ..Input::default()
        });
        assert_eq!(game.training, [0, 1, 0]);
        assert_eq!(game.match_number(), 2);
        assert_eq!(game.phase, Phase::Briefing);
        let mut maxed = before;
        maxed.training[0] = 3;
        let before = maxed.clone();
        maxed.step(&Input {
            slot: Some(1),
            ..Input::default()
        });
        assert_eq!(maxed, before);
    }
    #[test]
    fn wildcard_rolls_back_score_and_four_lost_matches_end_a_fresh_run() {
        let mut game = Game::new();
        let mut observed_retries = 0;
        let mut last = game.retries;
        for _ in 0..80_000 {
            let input = if matches!(game.phase, Phase::Briefing | Phase::Setback)
                || game.phase == Phase::Serve && game.server == 0
            {
                primary()
            } else {
                Input::default()
            };
            game.step(&input);
            if game.retries != last {
                observed_retries += 1;
                last = game.retries;
                assert_eq!(game.score, game.match_score_start);
            }
            if game.outcome() != Outcome::Playing {
                break;
            }
        }
        assert_eq!(game.outcome(), Outcome::Lost);
        assert_eq!(observed_retries, 3);
        assert_eq!(game.retries, 0);
        let frozen = game.clone();
        for _ in 0..20 {
            game.step(&primary());
        }
        assert_eq!(game, frozen);
    }
    #[test]
    fn training_and_stamina_have_real_movement_effects() {
        let mut game = incoming();
        let old = (game.speed(), game.reach());
        game.training = [2, 1, 2];
        assert!(game.speed() > old.0 && game.reach() > old.1);
        game.athletes[0].stamina = 0.;
        game.ball.hitter = 0;
        game.step(&Input::default());
        assert!((game.athletes[0].stamina - 18. / 60.).abs() < 0.0001);
        assert_eq!(game.total_returns, 0);
    }
    #[test]
    fn complete_tournament_is_won_using_only_legal_inputs_and_replays_exactly() {
        let mut game = Game::new();
        let mut replay = Game::new();
        let mut visited = [false; 6];
        let mut win_tick = None;
        for tick in 0..150_000 {
            visited[game.match_index] = true;
            let input = game.pilot(tick);
            game.step(&input);
            replay.step(&input);
            assert_eq!(game, replay, "same-host replay diverged at tick {tick}");
            if game.outcome() != Outcome::Playing {
                win_tick = Some(tick);
                break;
            }
        }
        assert_eq!(
            game.outcome(),
            Outcome::Won,
            "{} / {:?} / {:?}",
            game.match_number(),
            game.games,
            game.points
        );
        assert!(visited.into_iter().all(|v| v));
        assert_eq!(game.medals, [3; 6]);
        assert_eq!(game.retries, 3);
        assert!(game.total_returns > 40);
        assert!(game.best_rally >= 4);
        assert!(game.score > 10_000);
        eprintln!(
            "Legal tournament victory at tick {}, score {}, returns {}, longest rally {}",
            win_tick.unwrap(),
            game.score,
            game.total_returns,
            game.best_rally
        );
        let frozen = game.clone();
        for _ in 0..50 {
            game.step(&Input {
                primary: true,
                secondary: true,
                tertiary: true,
                slot: Some(1),
                right: true,
                ..Input::default()
            });
        }
        assert_eq!(game, frozen);
    }
}
