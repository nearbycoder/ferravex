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
impl Shot {
    /// Stamina a shot spends on contact.
    pub(crate) fn cost(self) -> f32 {
        match self {
            Shot::Drive => 7.,
            Shot::Lob => 4.,
            Shot::Smash => 26.,
        }
    }
}
/// Player movement speed (units per second) at a footwork training level.
pub(crate) fn speed_at(level: u8) -> f32 {
    185. + f32::from(level) * 15.
}
/// Stamina recovered per second at an endurance training level.
pub(crate) fn recovery_at(level: u8) -> f32 {
    13. + f32::from(level) * 5.
}
/// Racket reach (court units) at a racket training level.
pub(crate) fn reach_at(level: u8) -> f32 {
    31. + f32::from(level) * 4.
}
/// Stamina every swing attempt spends.
pub(crate) const SWING_COST: f32 = 2.;
/// A smash needs this much stamina and a ball at least [`SMASH_HEIGHT`] high.
pub(crate) const SMASH_STAMINA: f32 = 26.;
pub(crate) const SMASH_HEIGHT: f32 = 14.;
/// The racket cannot reach a ball higher than this.
pub(crate) const RACKET_HEIGHT: f32 = 43.;
/// Below this stamina an athlete moves at 80% speed.
pub(crate) const TIRED: f32 = 12.;
/// Why a rally ended, kept for presentation (the scoring decision is made by
/// [`Game::step`] before this is recorded).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum End {
    /// The ball crossed the net too low.
    Net,
    /// The first bounce landed outside the receiver's singles court.
    Out,
    /// The ball bounced twice.
    Bounce,
    /// The ball passed the receiver after its legal bounce.
    Passed,
}
/// Why a requested smash became a drive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Fallback {
    /// Less than 26 stamina remained.
    Tired,
    /// The ball was below smash height.
    Low,
}
/// One happening that the latest [`Game::step`] accepted, in court units.
/// Presentation reads these for effects; the rules never do.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Event {
    /// The briefing was accepted and the first serve is set.
    Enter,
    /// The aimed lane (0 left, 1 center, 2 right) changed.
    Aim(usize),
    /// The player started a swing.
    Swing(Shot),
    /// The player's swing window closed without contact.
    Whiff,
    /// A racket struck the ball at `at`, `height` above the ground.
    Hit {
        side: usize,
        shot: Shot,
        at: Vec2,
        height: f32,
        serve: bool,
        sweet: bool,
        fallback: Option<Fallback>,
    },
    /// The first, legal bounce.
    Bounce(Vec2),
    /// The rally ended at `at`.
    Dead { end: End, at: Vec2 },
    /// A point was awarded; `ace` marks a serve the receiver never touched.
    Point { winner: usize, ace: bool },
    /// A game was decided.
    Game(usize),
    /// A match was decided, with the medals it earned the player.
    Match { winner: usize, medals: u8 },
    /// A training level was bought (0 footwork, 1 endurance, 2 reach).
    Train(usize),
    /// A training choice was refused because that track is complete.
    Maxed(usize),
    /// Training was skipped.
    Skip,
    /// A wildcard restarted the lost match.
    Retry,
}
/// Presentation-only record of the latest step. It is cleared at the start of
/// every step and never read by the rules, so it is excluded from equality:
/// replays and inertness checks compare rules state only.
#[derive(Clone, Debug, Default)]
pub(crate) struct Feed(Vec<Event>);
impl PartialEq for Feed {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}
impl Feed {
    fn push(&mut self, event: Event) {
        // A tick accepts only a handful of events; the bound keeps it so.
        if self.0.len() < 16 {
            self.0.push(event);
        }
    }
    pub(crate) fn events(&self) -> &[Event] {
        &self.0
    }
}
/// Logical-canvas click regions shared with the view: left, top, right, bottom.
pub(crate) const AIM_STRIP: [f32; 4] = [120., 520., 590., 558.];
/// The rival's half of the court surface; clicking a third aims there.
pub(crate) const FAR_COURT: [f32; 4] = [67.4, 195.5, 632.6, 343.];
/// The card's main button: enter the court, or retry with a wildcard.
pub(crate) const CARD_BUTTON: [f32; 4] = [180., 480., 630., 513.];
/// The training card's skip button.
pub(crate) const SKIP_BUTTON: [f32; 4] = [452., 478., 622., 512.];
pub(crate) fn inside(area: [f32; 4], p: Vec2) -> bool {
    (area[0]..=area[2]).contains(&p.x) && (area[1]..=area[3]).contains(&p.y)
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
    pub(crate) tick: u64,
    pub(crate) total_returns: u32,
    pub(crate) point_winner: usize,
    pub(crate) perfect: bool,
    /// Kind of the ball's current flight (a serve flies as a drive).
    pub(crate) ball_shot: Shot,
    /// How the latest rally ended, shown during the point pause.
    pub(crate) last_end: Option<End>,
    /// Tournament statistics for the ending.
    pub(crate) points_won: u32,
    pub(crate) aces: u32,
    pub(crate) feed: Feed,
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
            tick: 0,
            total_returns: 0,
            point_winner: 0,
            perfect: false,
            ball_shot: Shot::Drive,
            last_end: None,
            points_won: 0,
            aces: 0,
            feed: Feed(Vec::with_capacity(16)),
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
    /// One-sentence tournament result for status readers and the ending.
    pub fn summary(&self) -> String {
        if self.phase == Phase::Champion {
            format!(
                "Earthrise champion with {} medals, {} aces and a longest rally of {}.",
                self.medal_total(),
                self.aces,
                self.best_rally
            )
        } else {
            format!(
                "Tour ended against {} after {} of 6 matches won.",
                self.opponent().name,
                self.matches_won()
            )
        }
    }
    pub(crate) fn opponent(&self) -> &'static Opponent {
        &OPPONENTS[self.match_index]
    }
    /// Medals earned so far: three for a 2-0 win, two for 2-1.
    pub(crate) fn medal_total(&self) -> u32 {
        self.medals.iter().map(|&m| u32::from(m)).sum()
    }
    /// Matches won so far.
    pub(crate) fn matches_won(&self) -> usize {
        self.medals.iter().filter(|&&m| m > 0).count()
    }
    /// Whether the latest awarded point decided its game (shown during the
    /// pause, before [`Game::step`] advances the game count).
    pub(crate) fn game_point_won(&self) -> bool {
        let w = self.point_winner;
        self.phase == Phase::Point
            && self.points[w] >= 4
            && self.points[w] >= self.points[1 - w] + 2
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
        speed_at(self.training[0])
    }
    pub(crate) fn recovery(&self) -> f32 {
        recovery_at(self.training[1])
    }
    pub(crate) fn reach(&self) -> f32 {
        reach_at(self.training[2])
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
        self.ball_shot = Shot::Drive;
        self.last_end = None;
    }
    /// Aim at `lane` (0..=2), recording a change for presentation.
    fn set_aim(&mut self, lane: usize) {
        if lane != self.aim {
            self.feed.push(Event::Aim(lane));
        }
        self.aim = lane;
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
        self.feed.0.clear();
        if self.outcome() != Outcome::Playing {
            return Cue::None;
        }
        let click = |area| input.click && inside(area, input.pointer);
        match self.phase {
            Phase::Briefing => {
                if input.primary || click(CARD_BUTTON) {
                    self.setup_serve();
                    self.feed.push(Event::Enter);
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
                        self.feed.push(Event::Maxed(choice - 1));
                        return Cue::None;
                    }
                    self.training[choice - 1] += 1;
                    self.match_index += 1;
                    self.new_match();
                    self.feed.push(Event::Train(choice - 1));
                    return Cue::Good;
                }
                // The skip button is the pointer twin of Space.
                if input.primary || click(SKIP_BUTTON) {
                    self.match_index += 1;
                    self.new_match();
                    self.feed.push(Event::Skip);
                    return Cue::Action;
                }
                return Cue::None;
            }
            Phase::Setback => {
                // The retry button is the pointer twin of Space.
                if input.primary || click(CARD_BUTTON) {
                    self.retries -= 1;
                    self.score = self.match_score_start;
                    self.new_match();
                    self.feed.push(Event::Retry);
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
            self.set_aim(aim - 1);
        }
        if click(AIM_STRIP) {
            self.set_aim((((input.pointer.x - AIM_STRIP[0]) / 157.) as usize).min(2));
        }
        // Clicking a third of the rival's court aims there, like 1/2/3.
        if click(FAR_COURT) {
            let third = (FAR_COURT[2] - FAR_COURT[0]) / 3.;
            self.set_aim((((input.pointer.x - FAR_COURT[0]) / third) as usize).min(2));
        }
        for side in 0..2 {
            let recovery = if side == 0 {
                self.recovery()
            } else {
                recovery_at(0)
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
            player.stamina = (player.stamina - SWING_COST).max(0.);
            player.flash = 14;
            let shot = player.shot;
            self.feed.push(Event::Swing(shot));
            cue = Cue::Action;
        }
        if self.athletes[0].swing > 0 {
            if self.can_hit(0) {
                let shot = self.athletes[0].shot;
                self.launch(0, shot, false);
                cue = Cue::Good;
            } else if self.athletes[0].swing == 1 {
                self.feed.push(Event::Whiff);
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
            * if self.athletes[0].stamina < TIRED {
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
            * if self.athletes[1].stamina < TIRED {
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
            || self.ball.height > RACKET_HEIGHT
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
        let mut fallback = None;
        if shot == Shot::Smash
            && (self.ball.height < SMASH_HEIGHT || self.athletes[side].stamina < SMASH_STAMINA)
        {
            shot = Shot::Drive;
            fallback = Some(if self.athletes[side].stamina < SMASH_STAMINA {
                Fallback::Tired
            } else {
                Fallback::Low
            });
        }
        let contact = (self.ball.pos, self.ball.height);
        self.athletes[side].stamina = (self.athletes[side].stamina - shot.cost()).max(0.);
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
        self.ball_shot = shot;
        self.feed.push(Event::Hit {
            side,
            shot,
            at: contact.0,
            height: contact.1,
            serve: serving,
            sweet: self.perfect,
            fallback: if side == 0 { fallback } else { None },
        });
    }
    /// Record how and where the rally ended, then return its winner.
    fn dead(&mut self, end: End, at: Vec2, winner: usize) -> Option<usize> {
        self.last_end = Some(end);
        self.feed.push(Event::Dead { end, at });
        Some(winner)
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
                let at = previous + (self.ball.pos - previous) * t;
                return self.dead(End::Net, at, 1 - self.ball.hitter);
            }
        }
        if self.ball.height <= 0. {
            if self.ball.bounced {
                return self.dead(End::Bounce, self.ball.pos, self.ball.hitter);
            }
            let correct_side = if self.ball.hitter == 0 {
                self.ball.pos.y < 0.
            } else {
                self.ball.pos.y > 0.
            };
            if !correct_side || self.ball.pos.x.abs() > 180. || self.ball.pos.y.abs() > 125. {
                return self.dead(End::Out, self.ball.pos, 1 - self.ball.hitter);
            }
            self.ball.bounced = true;
            self.ball.height = 0.;
            self.ball.vertical = 24.;
            self.ball.velocity *= 0.45;
            self.feed.push(Event::Bounce(self.ball.pos));
        }
        if self.ball.pos.y.abs() > 190. || self.ball.pos.x.abs() > 235. {
            return self.dead(End::Passed, self.ball.pos, self.ball.hitter);
        }
        None
    }
    fn award_point(&mut self, winner: usize) -> Cue {
        // Only the serve was struck, and the server won: an ace.
        let ace = self.rally == 1 && winner == self.server;
        self.points[winner] += 1;
        // Compress repeated deuce without changing advantage, bounding the counters.
        if self.points[0] >= 4 && self.points[1] >= 4 {
            self.points[0] -= 1;
            self.points[1] -= 1;
        }
        self.point_winner = winner;
        self.timer = 0;
        self.phase = Phase::Point;
        self.feed.push(Event::Point { winner, ace });
        if winner == 0 {
            self.points_won = self.points_won.saturating_add(1);
            self.aces = self.aces.saturating_add(u32::from(ace));
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
            self.feed.push(Event::Game(winner));
            if winner == 0 {
                self.score += 200;
            }
            if self.games[winner] == 2 {
                if winner == 0 {
                    self.medals[self.match_index] = if self.games[1] == 0 { 3 } else { 2 };
                    self.score += 800 + u32::from(self.medals[self.match_index]) * 100;
                    self.feed.push(Event::Match {
                        winner,
                        medals: self.medals[self.match_index],
                    });
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
                self.feed.push(Event::Match { winner, medals: 0 });
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
