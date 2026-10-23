//! Authoritative tactical round/economy rules. The host supplies validated movement
//! and hit results; this module validates teams, phase, money, bomb possession,
//! interaction distance, alive state, and uninterrupted plant/defuse durations.
//!
//! Menus can read [`TacticalMatch::summary`] without consuming events or advancing
//! time. Continue authoritative fixed ticks while a local menu is open; call
//! [`TacticalMatch::reset`] only when the authority starts a fresh match.
use std::collections::VecDeque;

pub type PlayerId = u16;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Team {
    Attack,
    Defend,
}
impl Team {
    fn index(self) -> usize {
        match self {
            Self::Attack => 0,
            Self::Defend => 1,
        }
    }
    fn other(self) -> Self {
        match self {
            Self::Attack => Self::Defend,
            Self::Defend => Self::Attack,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Weapon {
    Pistol,
    Rifle,
    Sniper,
}
impl Weapon {
    pub fn price(self) -> u32 {
        match self {
            Self::Pistol => 200,
            Self::Rifle => 2700,
            Self::Sniper => 4750,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Buy,
    Live,
    Planted,
    PostRound,
    Finished,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WinReason {
    Forfeit,
    Elimination,
    TimeExpired,
    Explosion,
    Defused,
}
/// The most recently accepted round result, retained independently of events.
///
/// See [`TacticalMatch::summary`] for an example. A forfeit records the current
/// round and ends the match immediately, including during buy or intermission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RoundOutcome {
    /// One-based round in which the result was accepted.
    pub round: u32,
    pub winner: Team,
    pub reason: WinReason,
}
/// An allocation-free snapshot for round HUDs and menus.
///
/// Created by [`TacticalMatch::summary`]; reading it never drains feedback events
/// or changes the authoritative phase. Team arrays use attack, then defend order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RoundSummary {
    pub round: u32,
    pub phase: Phase,
    pub score: [u16; 2],
    /// Time to the current phase's deadline, or zero when finished.
    /// An earlier plant/defuse completion can change the phase first.
    pub remaining_seconds: f32,
    /// Remaining phase seconds rounded up for display; `None` when finished.
    pub countdown_seconds: Option<u32>,
    /// Latest result, retained through subsequent rounds until replaced or reset.
    /// `None` before the first result of a fresh match.
    pub last_outcome: Option<RoundOutcome>,
    /// Set only after entering [`Phase::Finished`], including by forfeit.
    /// A match-clinching intermission still has no match winner here.
    pub match_winner: Option<Team>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Bomb {
    Carried(PlayerId),
    Dropped([f32; 3]),
    Planted { site: usize, position: [f32; 3] },
}
#[derive(Clone, Copy, Debug)]
pub struct Site {
    pub center: [f32; 3],
    pub radius: f32,
}
#[derive(Clone, Copy, Debug)]
pub struct PlayerSetup {
    pub id: PlayerId,
    pub team: Team,
    pub spawn: [f32; 3],
}
#[derive(Clone, Debug)]
pub struct Player {
    pub id: PlayerId,
    pub team: Team,
    pub position: [f32; 3],
    pub health: u16,
    pub money: u32,
    pub weapon: Weapon,
    pub armor: u16,
    pub defuse_kit: bool,
    spawn: [f32; 3],
}
impl Player {
    pub fn alive(&self) -> bool {
        self.health > 0
    }
}
#[derive(Clone, Debug)]
pub struct Config {
    pub buy_seconds: f32,
    pub round_seconds: f32,
    pub bomb_seconds: f32,
    pub plant_seconds: f32,
    pub defuse_seconds: f32,
    pub kit_defuse_seconds: f32,
    pub intermission_seconds: f32,
    pub rounds_to_win: u16,
    pub starting_money: u32,
    pub sites: Vec<Site>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            buy_seconds: 10.0,
            round_seconds: 115.0,
            bomb_seconds: 40.0,
            plant_seconds: 3.0,
            defuse_seconds: 10.0,
            kit_defuse_seconds: 5.0,
            intermission_seconds: 3.0,
            rounds_to_win: 13,
            starting_money: 800,
            sites: vec![
                Site {
                    center: [10.0, 0.0, 10.0],
                    radius: 4.0,
                },
                Site {
                    center: [-10.0, 0.0, 10.0],
                    radius: 4.0,
                },
            ],
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    BuyWeapon(Weapon),
    BuyArmor,
    BuyDefuseKit,
    Plant(usize),
    Defuse,
    Cancel,
    DropBomb,
    PickUpBomb,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuleError {
    InvalidConfig,
    InvalidRoster,
    UnknownPlayer,
    WrongPhase,
    Dead,
    WrongTeam,
    InsufficientFunds,
    AlreadyOwned,
    BombUnavailable,
    OutOfRange,
    Busy,
    InvalidInput,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    RoundStarted(u32),
    Killed {
        attacker: PlayerId,
        victim: PlayerId,
    },
    Planted {
        player: PlayerId,
        site: usize,
    },
    RoundWon {
        team: Team,
        reason: WinReason,
    },
    MatchWon(Team),
}
#[derive(Clone, Copy)]
enum Interaction {
    Plant(usize),
    Defuse,
}
#[derive(Clone, Copy)]
struct Channel {
    player: PlayerId,
    action: Interaction,
    elapsed: f32,
    required: f32,
    origin: [f32; 3],
}

pub struct TacticalMatch {
    config: Config,
    players: Vec<Player>,
    phase: Phase,
    remaining: f32,
    bomb: Bomb,
    channel: Option<Channel>,
    score: [u16; 2],
    loss_streak: [u8; 2],
    round: u32,
    last_outcome: Option<RoundOutcome>,
    events: VecDeque<Event>,
}
impl TacticalMatch {
    pub fn new(config: Config, roster: Vec<PlayerSetup>) -> Result<Self, RuleError> {
        if ![
            config.buy_seconds,
            config.round_seconds,
            config.bomb_seconds,
            config.plant_seconds,
            config.defuse_seconds,
            config.kit_defuse_seconds,
            config.intermission_seconds,
        ]
        .into_iter()
        .all(|n| n.is_finite() && n > 0.0 && n <= 3600.0)
            || config.rounds_to_win == 0
            || config.starting_money > 16000
            || config.sites.is_empty()
            || config.sites.len() > 32
            || config
                .sites
                .iter()
                .any(|site| !finite(site.center) || !site.radius.is_finite() || site.radius <= 0.0)
        {
            return Err(RuleError::InvalidConfig);
        }
        if roster.len() < 2
            || roster.len() > 64
            || !roster.iter().any(|p| p.team == Team::Attack)
            || !roster.iter().any(|p| p.team == Team::Defend)
            || roster
                .iter()
                .enumerate()
                .any(|(i, p)| !finite(p.spawn) || roster[..i].iter().any(|old| old.id == p.id))
        {
            return Err(RuleError::InvalidRoster);
        }
        let carrier = roster
            .iter()
            .find(|p| p.team == Team::Attack)
            .expect("validated attackers")
            .id;
        let players = roster
            .into_iter()
            .map(|p| Player {
                id: p.id,
                team: p.team,
                position: p.spawn,
                spawn: p.spawn,
                health: 100,
                money: config.starting_money,
                weapon: Weapon::Pistol,
                armor: 0,
                defuse_kit: false,
            })
            .collect();
        let remaining = config.buy_seconds;
        let mut game = Self {
            config,
            players,
            phase: Phase::Buy,
            remaining,
            bomb: Bomb::Carried(carrier),
            channel: None,
            score: [0; 2],
            loss_streak: [0; 2],
            round: 1,
            last_outcome: None,
            events: VecDeque::with_capacity(128),
        };
        game.start_round();
        Ok(game)
    }
    /// Start a fresh match with the same validated config, roster and spawns.
    ///
    /// Restores round one/buy time, starting money, health, pistol loadouts and
    /// initial bomb ownership. Clears scores, loss streaks, interactions, outcome
    /// history and queued events, then queues exactly one [`Event::RoundStarted`].
    /// Repeated calls produce the same state and reuse existing roster/event
    /// storage. Disconnected players are restored too; use a new match if the
    /// roster has changed.
    ///
    /// This is an authority operation, not a local menu pause. The caller owns
    /// resetting its clock, pending input, feedback and external game entities.
    ///
    /// ```
    /// use ferravex_play::rounds::{Config, Event, Phase, PlayerSetup, TacticalMatch, Team};
    /// let mut game = TacticalMatch::new(Config::default(), vec![
    ///     PlayerSetup { id: 1, team: Team::Attack, spawn: [0.0; 3] },
    ///     PlayerSetup { id: 2, team: Team::Defend, spawn: [1.0, 0.0, 0.0] },
    /// ]).unwrap();
    /// game.forfeit(Team::Attack);
    /// game.reset();
    /// game.reset(); // No stale win events or duplicate start notifications.
    /// assert_eq!(game.summary().phase, Phase::Buy);
    /// assert_eq!(game.summary().last_outcome, None);
    /// assert_eq!(game.pop_event(), Some(Event::RoundStarted(1)));
    /// assert_eq!(game.pop_event(), None);
    /// ```
    pub fn reset(&mut self) {
        self.score = [0; 2];
        self.loss_streak = [0; 2];
        self.round = 1;
        self.last_outcome = None;
        self.events.clear();
        for player in &mut self.players {
            player.money = self.config.starting_money;
            player.weapon = Weapon::Pistol;
            player.armor = 0;
            player.defuse_kit = false;
        }
        self.start_round();
    }
    /// Read round, countdown and outcome information without consuming events.
    ///
    /// The result is a detached value. Countdown rounding is presentation only;
    /// phase transitions still occur exclusively through the rules' mutations.
    /// The latest outcome survives event draining and the start of another round.
    ///
    /// ```
    /// use ferravex_play::rounds::{
    ///     Config, Phase, PlayerSetup, RoundOutcome, TacticalMatch, Team, WinReason,
    /// };
    /// let mut game = TacticalMatch::new(Config::default(), vec![
    ///     PlayerSetup { id: 1, team: Team::Attack, spawn: [0.0; 3] },
    ///     PlayerSetup { id: 2, team: Team::Defend, spawn: [1.0, 0.0, 0.0] },
    /// ]).unwrap();
    /// game.tick(0.25).unwrap();
    /// let menu = game.summary();
    /// assert_eq!(menu.remaining_seconds, 9.75);
    /// assert_eq!(menu.countdown_seconds, Some(10));
    /// game.forfeit(Team::Attack);
    /// while game.pop_event().is_some() {}
    /// let result = game.summary();
    /// assert_eq!(result.phase, Phase::Finished);
    /// assert_eq!(result.countdown_seconds, None);
    /// assert_eq!(result.match_winner, Some(Team::Defend));
    /// assert_eq!(result.last_outcome, Some(RoundOutcome {
    ///     round: 1, winner: Team::Defend, reason: WinReason::Forfeit,
    /// }));
    /// assert_eq!(menu.phase, Phase::Buy); // Earlier snapshots remain unchanged.
    /// ```
    pub fn summary(&self) -> RoundSummary {
        let (countdown_seconds, match_winner) = match self.phase {
            Phase::Finished => (None, self.last_outcome.map(|outcome| outcome.winner)),
            _ => (Some(self.remaining.ceil() as u32), None),
        };
        RoundSummary {
            round: self.round,
            phase: self.phase,
            score: self.score,
            remaining_seconds: self.remaining,
            countdown_seconds,
            last_outcome: self.last_outcome,
            match_winner,
        }
    }
    /// Remove a disconnected player's live contribution without inventing a killer.
    /// Stable roster slots remain dead until a later authority roster reset.
    pub fn disconnect(&mut self, id: PlayerId) -> Result<(), RuleError> {
        let i = self.index(id)?;
        self.players[i].health = 0;
        if self.bomb == Bomb::Carried(id) {
            self.bomb = Bomb::Dropped(self.players[i].position);
        }
        if self.channel.is_some_and(|c| c.player == id) {
            self.channel = None;
        }
        if matches!(self.phase, Phase::Live | Phase::Planted) {
            self.check_elimination();
        }
        Ok(())
    }
    /// Authority-only disconnect forfeit. A client action cannot invoke this.
    /// Ends the match immediately so disconnected roster slots cannot resurrect
    /// on the next round and prevent elimination indefinitely.
    pub fn forfeit(&mut self, team: Team) {
        if self.phase == Phase::Finished {
            return;
        }
        for player in &mut self.players {
            if player.team == team {
                player.health = 0;
            }
        }
        if let Bomb::Carried(id) = self.bomb
            && let Some(player) = self.player(id)
            && !player.alive()
        {
            self.bomb = Bomb::Dropped(player.position);
        }
        let winner = team.other();
        self.channel = None;
        self.score[winner.index()] = self.config.rounds_to_win;
        self.phase = Phase::Finished;
        self.remaining = 0.;
        self.last_outcome = Some(RoundOutcome {
            round: self.round,
            winner,
            reason: WinReason::Forfeit,
        });
        self.emit(Event::RoundWon {
            team: winner,
            reason: WinReason::Forfeit,
        });
        self.emit(Event::MatchWon(winner));
    }
    pub fn phase(&self) -> Phase {
        self.phase
    }
    pub fn remaining(&self) -> f32 {
        self.remaining
    }
    pub fn round(&self) -> u32 {
        self.round
    }
    pub fn score(&self) -> [u16; 2] {
        self.score
    }
    pub fn bomb(&self) -> Bomb {
        self.bomb
    }
    pub fn players(&self) -> &[Player] {
        &self.players
    }
    pub fn player(&self, id: PlayerId) -> Option<&Player> {
        self.players.iter().find(|p| p.id == id)
    }
    pub fn interaction_progress(&self) -> Option<(PlayerId, f32)> {
        self.channel
            .map(|c| (c.player, (c.elapsed / c.required).clamp(0.0, 1.0)))
    }
    pub fn pop_event(&mut self) -> Option<Event> {
        self.events.pop_front()
    }
    /// Integrate movement on the authoritative host, then provide its accepted position.
    /// Moving >15 cm from interaction origin interrupts plant/defuse.
    pub fn set_position(&mut self, id: PlayerId, position: [f32; 3]) -> Result<(), RuleError> {
        if !finite(position) {
            return Err(RuleError::InvalidInput);
        }
        let index = self.index(id)?;
        if !self.players[index].alive() {
            return Err(RuleError::Dead);
        }
        self.players[index].position = position;
        if self
            .channel
            .is_some_and(|c| c.player == id && distance(c.origin, position) > 0.15)
        {
            self.channel = None;
        }
        Ok(())
    }
    pub fn act(&mut self, id: PlayerId, action: Action) -> Result<(), RuleError> {
        let index = self.index(id)?;
        let player = &self.players[index];
        if !player.alive() {
            return Err(RuleError::Dead);
        }
        match action {
            Action::BuyWeapon(weapon) => {
                if self.phase != Phase::Buy {
                    return Err(RuleError::WrongPhase);
                }
                if player.weapon == weapon {
                    return Err(RuleError::AlreadyOwned);
                }
                self.spend(index, weapon.price())?;
                self.players[index].weapon = weapon;
            }
            Action::BuyArmor => {
                if self.phase != Phase::Buy {
                    return Err(RuleError::WrongPhase);
                }
                if player.armor == 100 {
                    return Err(RuleError::AlreadyOwned);
                }
                self.spend(index, 650)?;
                self.players[index].armor = 100;
            }
            Action::BuyDefuseKit => {
                if self.phase != Phase::Buy {
                    return Err(RuleError::WrongPhase);
                }
                if player.team != Team::Defend {
                    return Err(RuleError::WrongTeam);
                }
                if player.defuse_kit {
                    return Err(RuleError::AlreadyOwned);
                }
                self.spend(index, 400)?;
                self.players[index].defuse_kit = true;
            }
            Action::Cancel => {
                if self.channel.is_some_and(|c| c.player == id) {
                    self.channel = None;
                }
            }
            Action::Plant(site_index) => {
                if self.phase != Phase::Live {
                    return Err(RuleError::WrongPhase);
                }
                if player.team != Team::Attack {
                    return Err(RuleError::WrongTeam);
                }
                if self.bomb != Bomb::Carried(id) {
                    return Err(RuleError::BombUnavailable);
                }
                let site = self
                    .config
                    .sites
                    .get(site_index)
                    .ok_or(RuleError::OutOfRange)?;
                if distance(site.center, player.position) > site.radius {
                    return Err(RuleError::OutOfRange);
                }
                if self.channel.is_some() {
                    return Err(RuleError::Busy);
                }
                self.channel = Some(Channel {
                    player: id,
                    action: Interaction::Plant(site_index),
                    elapsed: 0.0,
                    required: self.config.plant_seconds,
                    origin: player.position,
                });
            }
            Action::Defuse => {
                if self.phase != Phase::Planted {
                    return Err(RuleError::WrongPhase);
                }
                if player.team != Team::Defend {
                    return Err(RuleError::WrongTeam);
                }
                let Bomb::Planted { position, .. } = self.bomb else {
                    return Err(RuleError::BombUnavailable);
                };
                if distance(position, player.position) > 2.5 {
                    return Err(RuleError::OutOfRange);
                }
                if self.channel.is_some() {
                    return Err(RuleError::Busy);
                }
                self.channel = Some(Channel {
                    player: id,
                    action: Interaction::Defuse,
                    elapsed: 0.0,
                    required: if player.defuse_kit {
                        self.config.kit_defuse_seconds
                    } else {
                        self.config.defuse_seconds
                    },
                    origin: player.position,
                });
            }
            Action::DropBomb => {
                if self.phase != Phase::Live {
                    return Err(RuleError::WrongPhase);
                }
                if self.bomb != Bomb::Carried(id) {
                    return Err(RuleError::BombUnavailable);
                }
                self.bomb = Bomb::Dropped(player.position);
                self.channel = None;
            }
            Action::PickUpBomb => {
                if self.phase != Phase::Live {
                    return Err(RuleError::WrongPhase);
                }
                if player.team != Team::Attack {
                    return Err(RuleError::WrongTeam);
                }
                let Bomb::Dropped(position) = self.bomb else {
                    return Err(RuleError::BombUnavailable);
                };
                if distance(player.position, position) > 1.5 {
                    return Err(RuleError::OutOfRange);
                }
                self.bomb = Bomb::Carried(id);
            }
        }
        Ok(())
    }
    /// Apply a server-confirmed hit. Clients must never supply unvalidated hit results.
    pub fn damage(
        &mut self,
        attacker: PlayerId,
        victim: PlayerId,
        amount: u16,
    ) -> Result<(), RuleError> {
        if !matches!(self.phase, Phase::Live | Phase::Planted) {
            return Err(RuleError::WrongPhase);
        }
        let a = self.index(attacker)?;
        let v = self.index(victim)?;
        if !self.players[a].alive() || !self.players[v].alive() {
            return Err(RuleError::Dead);
        }
        if self.players[a].team == self.players[v].team {
            return Err(RuleError::WrongTeam);
        }
        let absorbed = self.players[v].armor.min(amount / 2);
        self.players[v].armor -= absorbed;
        self.players[v].health = self.players[v].health.saturating_sub(amount - absorbed);
        if !self.players[v].alive() {
            self.players[a].money = (self.players[a].money + 300).min(16000);
            if self.bomb == Bomb::Carried(victim) {
                self.bomb = Bomb::Dropped(self.players[v].position);
            }
            if self.channel.is_some_and(|c| c.player == victim) {
                self.channel = None;
            }
            self.emit(Event::Killed { attacker, victim });
            self.check_elimination();
        }
        Ok(())
    }
    /// Splits elapsed time at every phase/interaction deadline. Large steps produce
    /// the same rule transitions as small steps. Bomb expiry wins exact defuse ties.
    /// Zero time and valid ticks after match completion are no-ops; invalid deltas
    /// are rejected even after completion.
    pub fn tick(&mut self, mut dt: f32) -> Result<(), RuleError> {
        if !dt.is_finite() || !(0.0..=3600.0).contains(&dt) {
            return Err(RuleError::InvalidInput);
        }
        while dt > 0.0 && self.phase != Phase::Finished {
            let channel_left = self
                .channel
                .map_or(f32::INFINITY, |c| (c.required - c.elapsed).max(0.0));
            let step = dt.min(self.remaining).min(channel_left);
            self.remaining = (self.remaining - step).max(0.0);
            if let Some(channel) = &mut self.channel {
                channel.elapsed += step;
            }
            dt = (dt - step).max(0.0);
            if self.phase == Phase::Planted && self.remaining <= 1e-6 {
                self.finish_round(Team::Attack, WinReason::Explosion);
                continue;
            }
            if self.channel.is_some_and(|c| c.elapsed + 1e-6 >= c.required) {
                self.finish_channel();
                continue;
            }
            if self.remaining <= 1e-6 {
                match self.phase {
                    Phase::Buy => {
                        self.phase = Phase::Live;
                        self.remaining = self.config.round_seconds;
                    }
                    Phase::Live => self.finish_round(Team::Defend, WinReason::TimeExpired),
                    Phase::PostRound => self.next_round(),
                    Phase::Planted | Phase::Finished => {
                        unreachable!("handled before phase transitions")
                    }
                }
            }
        }
        Ok(())
    }
    fn index(&self, id: PlayerId) -> Result<usize, RuleError> {
        self.players
            .iter()
            .position(|p| p.id == id)
            .ok_or(RuleError::UnknownPlayer)
    }
    fn spend(&mut self, index: usize, price: u32) -> Result<(), RuleError> {
        let player = &mut self.players[index];
        if player.money < price {
            return Err(RuleError::InsufficientFunds);
        }
        player.money -= price;
        Ok(())
    }
    fn finish_channel(&mut self) {
        let channel = self.channel.take().expect("completed channel");
        match channel.action {
            Interaction::Plant(site) => {
                let index = self.index(channel.player).expect("roster stable");
                self.bomb = Bomb::Planted {
                    site,
                    position: channel.origin,
                };
                self.phase = Phase::Planted;
                self.remaining = self.config.bomb_seconds;
                self.players[index].money = (self.players[index].money + 300).min(16000);
                self.emit(Event::Planted {
                    player: channel.player,
                    site,
                });
            }
            Interaction::Defuse => self.finish_round(Team::Defend, WinReason::Defused),
        }
    }
    fn check_elimination(&mut self) {
        let attacks = self
            .players
            .iter()
            .any(|p| p.team == Team::Attack && p.alive());
        let defends = self
            .players
            .iter()
            .any(|p| p.team == Team::Defend && p.alive());
        if !defends {
            self.finish_round(Team::Attack, WinReason::Elimination);
        } else if !attacks && self.phase != Phase::Planted {
            self.finish_round(Team::Defend, WinReason::Elimination);
        }
    }
    fn finish_round(&mut self, winner: Team, reason: WinReason) {
        if !matches!(self.phase, Phase::Live | Phase::Planted) {
            return;
        }
        self.phase = Phase::PostRound;
        self.remaining = self.config.intermission_seconds;
        self.channel = None;
        self.last_outcome = Some(RoundOutcome {
            round: self.round,
            winner,
            reason,
        });
        self.score[winner.index()] += 1;
        self.loss_streak[winner.index()] = 0;
        let loser = winner.other();
        self.loss_streak[loser.index()] = (self.loss_streak[loser.index()] + 1).min(5);
        let loss_reward = 1400 + u32::from(self.loss_streak[loser.index()] - 1) * 500;
        for player in &mut self.players {
            player.money = (player.money
                + if player.team == winner {
                    3250
                } else {
                    loss_reward
                })
            .min(16000);
        }
        self.emit(Event::RoundWon {
            team: winner,
            reason,
        });
    }
    fn next_round(&mut self) {
        let winner = self.last_outcome.expect("post round has outcome").winner;
        if self.score[winner.index()] >= self.config.rounds_to_win {
            self.phase = Phase::Finished;
            self.remaining = 0.0;
            self.emit(Event::MatchWon(winner));
            return;
        }
        self.round += 1;
        self.start_round();
    }
    fn start_round(&mut self) {
        self.phase = Phase::Buy;
        self.remaining = self.config.buy_seconds;
        self.channel = None;
        for player in &mut self.players {
            if !player.alive() {
                player.weapon = Weapon::Pistol;
                player.armor = 0;
                player.defuse_kit = false;
            }
            player.health = 100;
            player.position = player.spawn;
        }
        self.bomb = Bomb::Carried(
            self.players
                .iter()
                .find(|p| p.team == Team::Attack)
                .expect("roster has attackers")
                .id,
        );
        self.emit(Event::RoundStarted(self.round));
    }
    fn emit(&mut self, event: Event) {
        if self.events.len() == 128 {
            self.events.pop_front();
        }
        self.events.push_back(event);
    }
}
fn finite(v: [f32; 3]) -> bool {
    v.into_iter().all(f32::is_finite)
}
fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((f64::from(a[0]) - f64::from(b[0])).powi(2)
        + (f64::from(a[1]) - f64::from(b[1])).powi(2)
        + (f64::from(a[2]) - f64::from(b[2])).powi(2))
    .sqrt() as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    fn game() -> TacticalMatch {
        TacticalMatch::new(
            Config {
                buy_seconds: 1.0,
                round_seconds: 10.0,
                bomb_seconds: 8.0,
                plant_seconds: 2.0,
                defuse_seconds: 4.0,
                kit_defuse_seconds: 2.0,
                intermission_seconds: 1.0,
                rounds_to_win: 2,
                starting_money: 4000,
                sites: vec![Site {
                    center: [0.0; 3],
                    radius: 3.0,
                }],
            },
            vec![
                PlayerSetup {
                    id: 1,
                    team: Team::Attack,
                    spawn: [0.0; 3],
                },
                PlayerSetup {
                    id: 2,
                    team: Team::Defend,
                    spawn: [1.0, 0.0, 0.0],
                },
            ],
        )
        .unwrap()
    }
    #[test]
    fn purchases_validate_team_phase_money_and_ownership() {
        let mut g = game();
        g.act(1, Action::BuyWeapon(Weapon::Rifle)).unwrap();
        assert_eq!(g.player(1).unwrap().money, 1300);
        assert_eq!(
            g.act(1, Action::BuyWeapon(Weapon::Sniper)),
            Err(RuleError::InsufficientFunds)
        );
        assert_eq!(g.act(1, Action::BuyDefuseKit), Err(RuleError::WrongTeam));
        g.act(2, Action::BuyDefuseKit).unwrap();
        assert_eq!(g.act(2, Action::BuyDefuseKit), Err(RuleError::AlreadyOwned));
        g.tick(1.0).unwrap();
        assert_eq!(g.act(1, Action::BuyArmor), Err(RuleError::WrongPhase));
    }
    #[test]
    fn plant_defuse_requires_uninterrupted_distance_checked_channels() {
        let mut g = game();
        g.act(2, Action::BuyDefuseKit).unwrap();
        g.tick(1.0).unwrap();
        g.act(1, Action::Plant(0)).unwrap();
        g.tick(1.0).unwrap();
        g.set_position(1, [0.2, 0.0, 0.0]).unwrap();
        assert!(g.interaction_progress().is_none());
        g.act(1, Action::Plant(0)).unwrap();
        g.tick(2.0).unwrap();
        assert_eq!(g.phase(), Phase::Planted);
        g.act(2, Action::Defuse).unwrap();
        g.tick(2.0).unwrap();
        assert_eq!(g.phase(), Phase::PostRound);
        assert_eq!(g.score(), [0, 1]);
        assert_eq!(g.player(1).unwrap().money, 5700); // plant bonus + first loss
    }
    #[test]
    fn planted_bomb_survives_attacker_elimination_and_expires() {
        let mut g = game();
        g.tick(1.0).unwrap();
        g.act(1, Action::Plant(0)).unwrap();
        g.tick(2.0).unwrap();
        g.damage(2, 1, 100).unwrap();
        assert_eq!(g.phase(), Phase::Planted);
        g.tick(8.0).unwrap();
        assert_eq!(g.score(), [1, 0]);
    }
    #[test]
    fn elimination_economy_respawn_and_match_end() {
        let mut g = game();
        g.tick(1.0).unwrap();
        g.damage(1, 2, 100).unwrap();
        assert_eq!(g.player(1).unwrap().money, 7550);
        assert_eq!(g.player(2).unwrap().money, 5400);
        g.tick(1.0).unwrap();
        assert_eq!(g.round(), 2);
        assert!(g.player(2).unwrap().alive());
        g.tick(1.0).unwrap();
        g.damage(1, 2, 100).unwrap();
        g.tick(1.0).unwrap();
        assert_eq!(g.phase(), Phase::Finished);
        assert_eq!(g.score(), [2, 0]);
        assert_eq!(g.player(2).unwrap().money, 7300); // escalating loss bonus
    }
    #[test]
    fn time_partitioning_and_exact_defuse_deadline_are_defined() {
        let mut a = game();
        let mut b = game();
        a.tick(12.0).unwrap();
        for _ in 0..120 {
            b.tick(0.1).unwrap();
        }
        assert_eq!(a.phase(), b.phase());
        assert_eq!(a.score(), b.score());
        assert_eq!(a.round(), b.round());
        let mut g = game();
        g.tick(1.0).unwrap();
        g.act(1, Action::Plant(0)).unwrap();
        g.tick(2.0).unwrap();
        g.tick(4.0).unwrap();
        g.act(2, Action::Defuse).unwrap();
        g.tick(4.0).unwrap();
        assert_eq!(g.score(), [1, 0]); // explosion wins exact tie
    }
    #[test]
    fn dropped_bomb_rejects_defender_and_remote_pickup() {
        let mut g = game();
        g.tick(1.0).unwrap();
        g.act(1, Action::DropBomb).unwrap();
        assert_eq!(g.act(2, Action::PickUpBomb), Err(RuleError::WrongTeam));
        g.set_position(1, [5.0, 0.0, 0.0]).unwrap();
        assert_eq!(g.act(1, Action::PickUpBomb), Err(RuleError::OutOfRange));
        g.set_position(1, [0.0; 3]).unwrap();
        g.act(1, Action::PickUpBomb).unwrap();
        assert_eq!(g.bomb(), Bomb::Carried(1));
    }

    #[test]
    fn summary_rounds_countdown_up_without_consuming_events_or_tracking_later_ticks() {
        let mut g = game();
        let initial = g.summary();
        assert_eq!(
            initial,
            RoundSummary {
                round: 1,
                phase: Phase::Buy,
                score: [0, 0],
                remaining_seconds: 1.0,
                countdown_seconds: Some(1),
                last_outcome: None,
                match_winner: None,
            }
        );
        g.tick(0.75).unwrap();
        assert_eq!(g.summary().remaining_seconds, 0.25);
        assert_eq!(g.summary().countdown_seconds, Some(1));
        assert_eq!(g.summary(), g.summary());
        assert_eq!(g.pop_event(), Some(Event::RoundStarted(1)));
        assert_eq!(g.pop_event(), None);
        g.tick(0.25).unwrap();
        assert_eq!(g.summary().phase, Phase::Live);
        assert_eq!(g.summary().countdown_seconds, Some(10));
        g.tick(0.25).unwrap();
        assert_eq!(g.summary().remaining_seconds, 9.75);
        assert_eq!(g.summary().countdown_seconds, Some(10));
        assert_eq!(initial.phase, Phase::Buy);
        assert_eq!(initial.remaining_seconds, 1.0);
    }

    #[test]
    fn outcomes_survive_event_draining_and_are_replaced_on_the_next_result() {
        let mut g = game();
        g.tick(11.0).unwrap();
        let first = RoundOutcome {
            round: 1,
            winner: Team::Defend,
            reason: WinReason::TimeExpired,
        };
        assert_eq!(g.summary().phase, Phase::PostRound);
        assert_eq!(g.summary().countdown_seconds, Some(1));
        assert_eq!(g.summary().last_outcome, Some(first));
        assert_eq!(g.summary().match_winner, None);
        let events: Vec<_> = std::iter::from_fn(|| g.pop_event()).collect();
        assert_eq!(
            events,
            [
                Event::RoundStarted(1),
                Event::RoundWon {
                    team: first.winner,
                    reason: first.reason,
                },
            ]
        );
        g.tick(1.0).unwrap();
        assert_eq!(g.summary().round, 2);
        assert_eq!(g.summary().phase, Phase::Buy);
        assert_eq!(g.summary().last_outcome, Some(first));
        g.tick(1.0).unwrap();
        g.damage(1, 2, 100).unwrap();
        assert_eq!(
            g.summary().last_outcome,
            Some(RoundOutcome {
                round: 2,
                winner: Team::Attack,
                reason: WinReason::Elimination,
            })
        );
        assert_eq!(g.summary().score, [1, 1]);
        assert_eq!(g.damage(1, 2, 100), Err(RuleError::WrongPhase));
        assert_eq!(
            std::iter::from_fn(|| g.pop_event())
                .filter(|event| matches!(event, Event::RoundWon { .. }))
                .count(),
            1
        );
    }

    #[test]
    fn bomb_outcome_summaries_agree_with_accepted_events_and_exact_ties() {
        for (delay, winner, reason) in [
            (0.0, Team::Defend, WinReason::Defused),
            (4.0, Team::Attack, WinReason::Explosion),
        ] {
            let mut g = game();
            g.tick(1.0).unwrap();
            g.act(1, Action::Plant(0)).unwrap();
            g.tick(2.0).unwrap();
            assert_eq!(g.summary().phase, Phase::Planted);
            assert_eq!(g.summary().countdown_seconds, Some(8));
            assert_eq!(g.summary().last_outcome, None);
            g.tick(delay).unwrap();
            g.act(2, Action::Defuse).unwrap();
            g.tick(4.0).unwrap();
            let summary = g.summary();
            assert_eq!(summary.phase, Phase::PostRound);
            assert_eq!(summary.countdown_seconds, Some(1));
            assert_eq!(
                summary.last_outcome,
                Some(RoundOutcome {
                    round: 1,
                    winner,
                    reason,
                })
            );
            let wins: Vec<_> = std::iter::from_fn(|| g.pop_event())
                .filter(|event| matches!(event, Event::RoundWon { .. }))
                .collect();
            assert_eq!(
                wins,
                [Event::RoundWon {
                    team: winner,
                    reason,
                }]
            );
            assert_eq!(g.summary(), summary);
        }
    }

    #[test]
    fn match_winner_waits_for_intermission_and_terminal_ticks_emit_nothing() {
        let mut g = game();
        g.tick(23.0).unwrap();
        assert_eq!(g.summary().phase, Phase::PostRound);
        assert_eq!(g.summary().score, [0, 2]);
        assert_eq!(g.summary().match_winner, None);
        g.tick(1.0).unwrap();
        let finished = g.summary();
        assert_eq!(finished.phase, Phase::Finished);
        assert_eq!(finished.round, 2);
        assert_eq!(finished.remaining_seconds, 0.0);
        assert_eq!(finished.countdown_seconds, None);
        assert_eq!(finished.match_winner, Some(Team::Defend));
        assert_eq!(
            finished.last_outcome,
            Some(RoundOutcome {
                round: 2,
                winner: Team::Defend,
                reason: WinReason::TimeExpired,
            })
        );
        let events: Vec<_> = std::iter::from_fn(|| g.pop_event()).collect();
        assert_eq!(
            events,
            [
                Event::RoundStarted(1),
                Event::RoundWon {
                    team: Team::Defend,
                    reason: WinReason::TimeExpired,
                },
                Event::RoundStarted(2),
                Event::RoundWon {
                    team: Team::Defend,
                    reason: WinReason::TimeExpired,
                },
                Event::MatchWon(Team::Defend),
            ]
        );
        for _ in 0..3 {
            g.tick(0.0).unwrap();
            g.tick(3600.0).unwrap();
            g.forfeit(Team::Attack);
            g.forfeit(Team::Defend);
            assert_eq!(g.summary(), finished);
            assert_eq!(g.pop_event(), None);
        }
    }

    fn game_in_phase(phase: Phase) -> TacticalMatch {
        let mut g = game();
        g.act(1, Action::BuyWeapon(Weapon::Rifle)).unwrap();
        g.act(2, Action::BuyArmor).unwrap();
        g.act(2, Action::BuyDefuseKit).unwrap();
        if phase != Phase::Buy {
            g.tick(1.0).unwrap();
            g.act(1, Action::Plant(0)).unwrap();
            g.tick(0.5).unwrap();
            if phase != Phase::Live {
                g.tick(1.5).unwrap();
                g.act(2, Action::Defuse).unwrap();
                g.tick(0.5).unwrap();
                if phase != Phase::Planted {
                    g.tick(1.5).unwrap();
                    if phase == Phase::Finished {
                        g.tick(13.0).unwrap();
                    }
                }
            }
        }
        assert_eq!(g.phase(), phase);
        g
    }

    #[test]
    fn zero_and_invalid_ticks_preserve_every_phase_and_pending_interactions() {
        for phase in [
            Phase::Buy,
            Phase::Live,
            Phase::Planted,
            Phase::PostRound,
            Phase::Finished,
        ] {
            let mut g = game_in_phase(phase);
            let summary = g.summary();
            let progress = g.interaction_progress();
            let events = g.events.clone();
            for dt in [0.0, -0.0, -1.0, f32::NAN, f32::INFINITY, 3601.0] {
                let result = g.tick(dt);
                assert_eq!(
                    result,
                    if dt == 0.0 {
                        Ok(())
                    } else {
                        Err(RuleError::InvalidInput)
                    }
                );
                assert_eq!(g.summary(), summary);
                assert_eq!(g.interaction_progress(), progress);
                assert_eq!(g.events, events);
            }
        }
    }

    #[test]
    fn forfeit_summaries_are_immediate_and_repeated_forfeits_are_no_ops() {
        for phase in [Phase::Buy, Phase::Live, Phase::Planted, Phase::PostRound] {
            let mut g = game_in_phase(phase);
            while g.pop_event().is_some() {}
            g.forfeit(Team::Attack);
            let summary = g.summary();
            assert_eq!(summary.phase, Phase::Finished);
            assert_eq!(summary.remaining_seconds, 0.0);
            assert_eq!(summary.countdown_seconds, None);
            assert_eq!(summary.score[1], 2);
            assert_eq!(summary.match_winner, Some(Team::Defend));
            assert_eq!(
                summary.last_outcome,
                Some(RoundOutcome {
                    round: 1,
                    winner: Team::Defend,
                    reason: WinReason::Forfeit,
                })
            );
            assert_eq!(g.interaction_progress(), None);
            g.forfeit(Team::Attack);
            g.forfeit(Team::Defend);
            assert_eq!(g.summary(), summary);
            assert_eq!(
                g.pop_event(),
                Some(Event::RoundWon {
                    team: Team::Defend,
                    reason: WinReason::Forfeit,
                })
            );
            assert_eq!(g.pop_event(), Some(Event::MatchWon(Team::Defend)));
            assert_eq!(g.pop_event(), None);
        }
    }

    #[test]
    fn forfeit_winner_is_authoritative_even_when_final_scores_tie() {
        let mut g = game();
        g.tick(23.0).unwrap();
        // A clinching defender intermission is still eligible for a forfeit.
        g.forfeit(Team::Defend);
        assert_eq!(g.summary().score, [2, 2]);
        assert_eq!(g.summary().match_winner, Some(Team::Attack));
        assert_eq!(g.summary().last_outcome.unwrap().reason, WinReason::Forfeit);
    }

    #[test]
    fn reset_restores_fresh_state_from_every_phase_and_reuses_storage() {
        for phase in [
            Phase::Buy,
            Phase::Live,
            Phase::Planted,
            Phase::PostRound,
            Phase::Finished,
        ] {
            let mut g = game_in_phase(phase);
            let fresh = game();
            let players_ptr = g.players.as_ptr();
            let players_capacity = g.players.capacity();
            let events_capacity = g.events.capacity();
            g.set_position(1, [5.0, 0.0, 0.0]).unwrap();
            if matches!(phase, Phase::Live | Phase::Planted) {
                g.damage(1, 2, 10).unwrap();
            } else {
                g.disconnect(2).unwrap();
            }
            assert_eq!(g.phase(), phase);
            for _ in 0..2 {
                g.reset();
                assert_eq!(g.summary(), fresh.summary());
                assert_eq!(g.interaction_progress(), None);
                assert_eq!(g.bomb(), fresh.bomb());
                assert_eq!(g.loss_streak, [0, 0]);
                assert_eq!(g.players.as_ptr(), players_ptr);
                assert_eq!(g.players.capacity(), players_capacity);
                assert_eq!(g.events.capacity(), events_capacity);
                for (player, expected) in g.players().iter().zip(fresh.players()) {
                    assert_eq!(player.id, expected.id);
                    assert_eq!(player.team, expected.team);
                    assert_eq!(player.spawn, expected.spawn);
                    assert_eq!(player.position, expected.position);
                    assert_eq!(player.health, expected.health);
                    assert_eq!(player.money, expected.money);
                    assert_eq!(player.weapon, expected.weapon);
                    assert_eq!(player.armor, expected.armor);
                    assert_eq!(player.defuse_kit, expected.defuse_kit);
                }
                assert_eq!(g.pop_event(), Some(Event::RoundStarted(1)));
                assert_eq!(g.pop_event(), None);
            }
            g.tick(11.0).unwrap();
            assert_eq!(g.score(), [0, 1]);
            assert_eq!(g.player(1).unwrap().money, 5400); // First-loss reward again.
        }
    }

    #[test]
    fn reset_during_an_interaction_clears_channels_and_stale_events() {
        for phase in [Phase::Live, Phase::Planted] {
            let mut g = game_in_phase(phase);
            assert!(g.interaction_progress().is_some());
            g.reset();
            g.reset();
            assert_eq!(g.interaction_progress(), None);
            assert_eq!(g.pop_event(), Some(Event::RoundStarted(1)));
            assert_eq!(g.pop_event(), None);
            g.tick(3.0).unwrap();
            assert_eq!(g.phase(), Phase::Live);
            assert_eq!(g.remaining(), 8.0);
            assert_eq!(g.bomb(), Bomb::Carried(1));
            assert_eq!(g.player(1).unwrap().money, 4000);
            assert_eq!(g.pop_event(), None);
        }
    }

    #[test]
    fn large_and_partitioned_ticks_have_identical_summaries_and_events() {
        let mut large = game();
        let mut partitioned = game();
        large.tick(24.0).unwrap();
        for _ in 0..96 {
            partitioned.tick(0.25).unwrap();
        }
        assert_eq!(large.summary(), partitioned.summary());
        assert_eq!(large.events, partitioned.events);
        assert_eq!(large.summary().phase, Phase::Finished);
    }

    #[test]
    fn next_round_preserves_survivor_equipment_and_resets_dead_loadouts() {
        let mut g = game();
        g.act(1, Action::BuyWeapon(Weapon::Rifle)).unwrap();
        g.act(1, Action::BuyArmor).unwrap();
        g.act(2, Action::BuyWeapon(Weapon::Rifle)).unwrap();
        g.act(2, Action::BuyArmor).unwrap();
        g.act(2, Action::BuyDefuseKit).unwrap();
        g.tick(1.0).unwrap();
        g.set_position(1, [2.0, 0.0, 0.0]).unwrap();
        g.damage(1, 2, 200).unwrap();
        g.tick(1.0).unwrap();
        let survivor = g.player(1).unwrap();
        assert_eq!(survivor.weapon, Weapon::Rifle);
        assert_eq!(survivor.armor, 100);
        assert_eq!(survivor.position, survivor.spawn);
        let respawned = g.player(2).unwrap();
        assert_eq!(respawned.weapon, Weapon::Pistol);
        assert_eq!(respawned.armor, 0);
        assert!(!respawned.defuse_kit);
        assert_eq!(respawned.health, 100);
        assert_eq!(respawned.position, respawned.spawn);
        assert_eq!(g.bomb(), Bomb::Carried(1));
        assert_eq!(g.summary().round, 2);
    }
}
