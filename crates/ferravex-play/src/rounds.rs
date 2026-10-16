//! Authoritative tactical round/economy rules. The host supplies validated movement
//! and hit results; this module validates teams, phase, money, bomb possession,
//! interaction distance, alive state, and uninterrupted plant/defuse durations.
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
    last_winner: Option<Team>,
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
            last_winner: None,
            events: VecDeque::with_capacity(128),
        };
        game.emit(Event::RoundStarted(1));
        Ok(game)
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
        self.last_winner = Some(winner);
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
        self.last_winner = Some(winner);
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
        let winner = self.last_winner.expect("post round has winner");
        if self.score[winner.index()] >= self.config.rounds_to_win {
            self.phase = Phase::Finished;
            self.remaining = 0.0;
            self.emit(Event::MatchWon(winner));
            return;
        }
        self.round += 1;
        self.phase = Phase::Buy;
        self.remaining = self.config.buy_seconds;
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
}
