use ferravex_play::attributes::{EffectSet, EffectSpec, EffectStatus};
use ferravex_play::{
    ai::{Candidate, Observer, Sight, TargetSelector, Unoccluded, WorkBudget},
    navigation::{PathCursor, Route3},
};
use ferravex_render::prelude::*;

use crate::level::Level;
use std::sync::Arc;

/// A rejected action leaves the orchard and its economy unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ActionError {
    Finished,
    InvalidPlot,
    Occupied,
    Empty,
    FullyGrown,
    Funds { missing: u32 },
    WaveActive,
    NoMoreWaves,
    Paused,
}
impl std::fmt::Display for ActionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Finished => f.write_str("Harvest finished. Plant again to start fresh."),
            Self::InvalidPlot => {
                f.write_str("Choose a round plot; paths and trees cannot hold towers.")
            }
            Self::Occupied => {
                f.write_str("This plot has a tower. Choose Upgrade or select an empty plot.")
            }
            Self::Empty => f.write_str("Build a tower here before upgrading."),
            Self::FullyGrown => f.write_str("Fully grown: level 3 is the maximum."),
            Self::Funds { missing } => write!(
                f,
                "Need {missing} more coins. Stop pests or clear a wave to earn them."
            ),
            Self::WaveActive => {
                f.write_str("Clear the pests already on their way before sending another wave.")
            }
            Self::NoMoreWaves => f.write_str("All authored waves have been sent."),
            Self::Paused => f.write_str("Resume the orchard before building or sending a wave."),
        }
    }
}
pub const MAX_PESTS: usize = 32;
pub const MAX_SHOTS: usize = 96;
/// Bounded per-frame event log. Presentation drains it; unattended tests may not.
pub const MAX_FEED: usize = 128;
/// Every eighth projectile fired anywhere in the orchard is a lucky shot that
/// deals double damage. It is derived from the saved `fired` counter, so
/// restored checkpoints continue the same cadence without a save change.
pub const LUCKY_EVERY: u32 = 8;
/// The secret royal pest: the final beetle of wave three wears a tiny crown.
pub const ROYAL_ID: u32 = 42;

/// Accepted simulation events, recorded for presentation (juice, audio, quips).
/// Recording never feeds back into combat, timing or the economy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Feed {
    Fired {
        plot: usize,
        lucky: bool,
    },
    Hit {
        at: Vec2,
        kind: Kind,
        lucky: bool,
    },
    Stopped {
        at: Vec2,
        id: u32,
        fast: bool,
        lucky: bool,
    },
    Escaped {
        id: u32,
    },
    Planted {
        plot: usize,
        kind: Kind,
    },
    Grown {
        plot: usize,
        level: u8,
    },
    Launched {
        wave: u8,
    },
    Cleared {
        wave: u8,
        perfect: bool,
        reward: u32,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Seed,
    Frost,
}
impl Kind {
    pub fn price(self) -> u32 {
        match self {
            Self::Seed => 60,
            Self::Frost => 80,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Seed => "SEED SLING",
            Self::Frost => "FROST HIVE",
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Tower {
    pub kind: Kind,
    pub level: u8,
    pub cooldown: f32,
    pub aim: Vec2,
}
impl Tower {
    pub fn planted(kind: Kind) -> Self {
        Self {
            kind,
            level: 0,
            cooldown: 0.,
            aim: vec2(0., -1.),
        }
    }
    pub fn damage(self) -> f32 {
        match self.kind {
            Kind::Seed => 22. + self.level as f32 * 14.,
            Kind::Frost => 8. + self.level as f32 * 6.,
        }
    }
    pub fn reload(self) -> f32 {
        (match self.kind {
            Kind::Seed => 0.62,
            Kind::Frost => 0.85,
        }) - self.level as f32 * 0.08
    }
    pub fn range(self) -> f32 {
        (match self.kind {
            Kind::Seed => 158.,
            Kind::Frost => 170.,
        }) + self.level as f32 * 18.
    }
    pub fn upgrade_cost(self) -> u32 {
        65 + self.level as u32 * 35
    }
}
#[derive(Clone, Debug)]
pub struct Pest {
    pub id: u32,
    pub pos: Vec2,
    pub segment: usize,
    pub distance: f32,
    pub hp: f32,
    pub max_hp: f32,
    pub fast: bool,
    /// Accepted direct/splash impacts, consumed only by presentation.
    pub impacts: u32,
    /// Whether the latest direct hit was a lucky shot (credited on the kill).
    pub lucky: bool,
    cursor: PathCursor,
    effects: EffectSet<4>,
}
impl Pest {
    pub fn slow(&self) -> Option<EffectStatus> {
        self.effects.status(1)
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct WavePreview {
    pub number: u8,
    pub spec: crate::level::Wave,
    pub fast_count: u32,
}
#[derive(Clone, Debug)]
pub struct Shot {
    pub pos: Vec2,
    pub target: u32,
    pub damage: f32,
    pub kind: Kind,
    /// A lucky shot already includes its doubled damage.
    pub lucky: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Playing,
    Won,
    Lost,
}
pub struct Game {
    pub level: Arc<Level>,
    pub towers: Vec<Option<Tower>>,
    pub pests: Vec<Pest>,
    pub shots: Vec<Shot>,
    pub money: u32,
    pub lives: u8,
    pub wave: u8,
    pub active: bool,
    pub remaining: u8,
    pub kills: u32,
    pub fired: u32,
    pub escaped: u32,
    pub outcome: Outcome,
    pub selected: usize,
    pub time: f32,
    /// Lucky shots that connected this run.
    pub lucky_hits: u32,
    /// Waves cleared without losing a heart.
    pub perfect_waves: u8,
    /// The royal pest was stopped this run.
    pub royal_stopped: bool,
    /// Presentation event log; see [`Feed`].
    pub feed: Vec<Feed>,
    leaks_at_launch: u32,
    spawn_timer: f32,
    next_id: u32,
    route: Route3,
    targets: [TargetSelector; 8],
}
impl Game {
    #[cfg(test)]
    pub fn new() -> Self {
        Self::with_level(Arc::new(Level::parse(crate::level::SOURCE).unwrap()))
    }
    pub fn with_level(level: Arc<Level>) -> Self {
        let mut route = Route3::new(level.path.len()).unwrap();
        route
            .set(&level.path.iter().map(|p| p.extend(0.)).collect::<Vec<_>>())
            .unwrap();
        Self {
            towers: vec![None; level.plots.len()],
            pests: Vec::with_capacity(MAX_PESTS),
            shots: Vec::with_capacity(MAX_SHOTS),
            money: level.money,
            lives: level.lives,
            level,
            wave: 0,
            active: false,
            remaining: 0,
            kills: 0,
            fired: 0,
            escaped: 0,
            outcome: Outcome::Playing,
            selected: 0,
            time: 0.,
            lucky_hits: 0,
            perfect_waves: 0,
            royal_stopped: false,
            feed: Vec::with_capacity(MAX_FEED),
            leaks_at_launch: 0,
            spawn_timer: 0.,
            next_id: 0,
            route,
            targets: [TargetSelector::new(0.).unwrap(); 8],
        }
    }
    /// Record a presentation event. Frequent combat events leave headroom for
    /// rarer milestones; once full, new events are dropped, never reallocated.
    fn emit(&mut self, event: Feed) {
        let limit = if matches!(event, Feed::Fired { .. } | Feed::Hit { .. }) {
            MAX_FEED - 32
        } else {
            MAX_FEED
        };
        if self.feed.len() < limit {
            self.feed.push(event);
        }
    }
    /// Read authored arrivals without starting the wave or spending spawn IDs.
    pub(super) fn next_wave(&self) -> Option<WavePreview> {
        if self.active || self.outcome != Outcome::Playing {
            return None;
        }
        let spec = *self.level.waves.get(usize::from(self.wave))?;
        Some(WavePreview {
            number: self.wave + 1,
            spec,
            fast_count: (self.next_id + u32::from(spec.count)) / spec.fast_every
                - self.next_id / spec.fast_every,
        })
    }
    /// A living, in-range target for the selected tower; never a stale ID.
    pub(super) fn selected_target(&self) -> Option<&Pest> {
        let tower = self.towers.get(self.selected)?.as_ref()?;
        let id = self.targets.get(self.selected)?.current()?;
        self.pests.iter().find(|p| {
            u64::from(p.id) == id
                && p.hp > 0.
                && p.pos.distance_squared(self.level.plots[self.selected]) <= tower.range().powi(2)
        })
    }
    /// Check a purchase without charging coins; shared by the HUD and mutation.
    pub(super) fn build_status(&self, plot: usize, kind: Kind) -> Result<(), ActionError> {
        self.playing_status()?;
        match self.towers.get(plot) {
            None => return Err(ActionError::InvalidPlot),
            Some(Some(_)) => return Err(ActionError::Occupied),
            Some(None) => {}
        }
        self.funds_status(kind.price())
    }
    fn playing_status(&self) -> Result<(), ActionError> {
        if self.outcome == Outcome::Playing {
            Ok(())
        } else {
            Err(ActionError::Finished)
        }
    }
    fn funds_status(&self, cost: u32) -> Result<(), ActionError> {
        if self.money >= cost {
            Ok(())
        } else {
            Err(ActionError::Funds {
                missing: cost - self.money,
            })
        }
    }
    pub fn build(&mut self, plot: usize, kind: Kind) -> bool {
        if self.build_status(plot, kind).is_err() {
            return false;
        }
        self.money -= kind.price();
        self.towers[plot] = Some(Tower::planted(kind));
        self.emit(Feed::Planted { plot, kind });
        true
    }
    /// Check the selected tower's next level without changing it.
    pub(super) fn upgrade_status(&self, plot: usize) -> Result<(), ActionError> {
        self.playing_status()?;
        let tower = match self.towers.get(plot) {
            None => return Err(ActionError::InvalidPlot),
            Some(None) => return Err(ActionError::Empty),
            Some(Some(tower)) => tower,
        };
        if tower.level >= 2 {
            return Err(ActionError::FullyGrown);
        }
        self.funds_status(tower.upgrade_cost())
    }
    pub fn upgrade(&mut self, plot: usize) -> bool {
        if self.upgrade_status(plot).is_err() {
            return false;
        }
        let tower = self.towers[plot].as_mut().unwrap();
        self.money -= tower.upgrade_cost();
        tower.level += 1;
        let level = tower.level;
        self.emit(Feed::Grown { plot, level });
        true
    }
    pub(super) fn wave_status(&self) -> Result<(), ActionError> {
        self.playing_status()?;
        if self.active {
            return Err(ActionError::WaveActive);
        }
        if usize::from(self.wave) >= self.level.waves.len() {
            return Err(ActionError::NoMoreWaves);
        }
        Ok(())
    }
    pub fn launch(&mut self) -> bool {
        if self.wave_status().is_err() {
            return false;
        }
        self.wave += 1;
        self.remaining = self.level.waves[usize::from(self.wave) - 1].count;
        self.active = true;
        self.spawn_timer = 0.;
        self.leaks_at_launch = self.escaped;
        self.emit(Feed::Launched { wave: self.wave });
        true
    }
    /// Reconstruct spawn identity at a safe between-wave checkpoint.
    /// Perfect waves are not saved; a leak-free checkpoint implies every
    /// completed wave was perfect, otherwise counting resumes from here.
    pub(super) fn restore_spawn_sequence(&mut self) {
        self.next_id = self.kills + self.escaped;
        self.perfect_waves = if self.escaped == 0 { self.wave } else { 0 };
    }
    /// Uses the same economy/actions as human input. No currency, health or combat cheats.
    pub fn autoplay(&mut self) {
        if let Some((plot, kind)) = self
            .level
            .autoplay
            .iter()
            .copied()
            .find(|(plot, _)| self.towers[*plot].is_none())
        {
            self.build(plot, kind);
        }
        if self.towers.iter().flatten().count() >= 4.min(self.towers.len())
            && let Some(plot) = self
                .level
                .upgrade_order
                .iter()
                .copied()
                .find(|plot| self.towers[*plot].is_some_and(|t| t.level < 2))
        {
            self.upgrade(plot);
        }
        if !self.active {
            self.launch();
        }
    }
    pub fn step(&mut self, dt: f32) {
        if self.outcome != Outcome::Playing {
            return;
        }
        self.time += dt;
        let wave = self.level.waves[usize::from(self.wave.saturating_sub(1))];
        if self.active && self.remaining > 0 {
            self.spawn_timer -= dt;
            if self.spawn_timer <= 0. && self.pests.len() < MAX_PESTS {
                self.next_id += 1;
                let fast = self.next_id.is_multiple_of(wave.fast_every);
                let hp = wave.hp * if fast { wave.fast_factor } else { 1. };
                self.pests.push(Pest {
                    id: self.next_id,
                    pos: self.level.path[0],
                    segment: 0,
                    distance: 0.,
                    hp,
                    max_hp: hp,
                    impacts: 0,
                    lucky: false,
                    fast,
                    cursor: PathCursor::default(),
                    effects: EffectSet::new().unwrap(),
                });
                self.remaining -= 1;
                self.spawn_timer += wave.interval;
            }
        }
        for pest in &mut self.pests {
            pest.effects.advance(dt).unwrap();
            let speed = if pest.fast {
                wave.fast_speed
            } else {
                wave.speed
            };
            let speed = pest.effects.value(0, speed, 0., 10000.).unwrap();
            let sample = pest
                .cursor
                .advance(&self.route, speed, dt)
                .unwrap()
                .unwrap();
            pest.pos = sample.position.truncate();
            pest.segment = sample.segment;
            pest.distance = sample.distance as f32;
        }
        let mut index = 0;
        while index < self.pests.len() {
            if self.pests[index].segment == self.level.path.len() - 1 {
                let pest = self.pests.remove(index);
                self.lives = self.lives.saturating_sub(1);
                self.escaped += 1;
                self.emit(Feed::Escaped { id: pest.id });
            } else {
                index += 1;
            }
        }
        if self.lives == 0 {
            self.outcome = Outcome::Lost;
            self.shots.clear();
            return;
        }
        let mut fired = [None; 8];
        for (plot, tower) in self.towers.iter_mut().enumerate() {
            let Some(tower) = tower else {
                continue;
            };
            tower.cooldown = (tower.cooldown - dt).max(0.);
            let selected = self.targets[plot]
                .select_visible(
                    self.pests.iter().filter(|p| p.hp > 0.).map(|p| Candidate {
                        id: u64::from(p.id),
                        position: p.pos.extend(0.),
                        groups: 1,
                    }),
                    Observer {
                        position: self.level.plots[plot].extend(0.),
                        forward: Vec3::Y,
                    },
                    Sight::new(tower.range(), std::f32::consts::TAU, 1).unwrap(),
                    &mut WorkBudget::new(MAX_PESTS as u32, MAX_PESTS as u32, 0),
                    &mut Unoccluded,
                    |candidate| {
                        self.pests
                            .iter()
                            .find(|p| u64::from(p.id) == candidate.id)
                            .unwrap()
                            .distance
                    },
                )
                .unwrap();
            if let Some(pest) = selected
                .target
                .and_then(|c| self.pests.iter().find(|p| u64::from(p.id) == c.id))
            {
                tower.aim = (pest.pos - self.level.plots[plot]).normalize_or_zero();
                if tower.cooldown > 0. || self.shots.len() == MAX_SHOTS {
                    continue;
                }
                self.fired = self.fired.saturating_add(1);
                let lucky = self.fired.is_multiple_of(LUCKY_EVERY);
                self.shots.push(Shot {
                    pos: self.level.plots[plot] + tower.aim * 18.,
                    target: pest.id,
                    damage: tower.damage() * if lucky { 2. } else { 1. },
                    kind: tower.kind,
                    lucky,
                });
                tower.cooldown = tower.reload();
                if let Some(slot) = fired.get_mut(plot) {
                    *slot = Some(lucky);
                }
            }
        }
        for (plot, lucky) in fired.into_iter().enumerate() {
            if let Some(lucky) = lucky {
                self.emit(Feed::Fired { plot, lucky });
            }
        }
        let mut hits = [None; MAX_SHOTS];
        let mut hit_count = 0;
        let mut frost_impacts = [Vec2::ZERO; MAX_SHOTS];
        let mut frost_count = 0;
        self.shots.retain_mut(|shot| {
            let Some(target) = self
                .pests
                .iter_mut()
                .find(|p| p.id == shot.target && p.hp > 0.)
            else {
                return false;
            };
            let delta = target.pos - shot.pos;
            let travel = 400. * dt;
            if delta.length_squared() <= (travel + 9.).powi(2) {
                target.hp -= shot.damage;
                target.impacts = target.impacts.saturating_add(1);
                target.lucky = shot.lucky;
                hits[hit_count] = Some((target.pos, shot.kind, shot.lucky));
                hit_count += 1;
                if shot.kind == Kind::Frost {
                    target
                        .effects
                        .apply(EffectSpec::modifier(1, 0, 0.45, 1.65), 0)
                        .unwrap();
                    frost_impacts[frost_count] = target.pos;
                    frost_count += 1;
                } else {
                    // Small, bounded hit-stop gives seed hits weight without
                    // teleporting pests backward or invalidating route progress.
                    target
                        .effects
                        .apply(EffectSpec::modifier(2, 0, 0.35, 0.10), 0)
                        .unwrap();
                }
                false
            } else {
                shot.pos += delta.normalize_or_zero() * travel;
                true
            }
        });
        for (at, kind, lucky) in hits[..hit_count].iter().flatten().copied() {
            self.lucky_hits += u32::from(lucky);
            self.emit(Feed::Hit { at, kind, lucky });
        }
        for center in &frost_impacts[..frost_count] {
            for pest in &mut self.pests {
                if pest.hp > 0. && pest.pos.distance_squared(*center) < 46. * 46. {
                    pest.effects
                        .apply(EffectSpec::modifier(1, 0, 0.45, 1.15), 0)
                        .unwrap();
                    pest.impacts = pest.impacts.saturating_add(1);
                }
            }
        }
        let mut index = 0;
        while index < self.pests.len() {
            if self.pests[index].hp <= 0. {
                let pest = self.pests.remove(index);
                self.money += self.level.kill_reward;
                self.kills += 1;
                self.royal_stopped |= pest.id == ROYAL_ID;
                self.emit(Feed::Stopped {
                    at: pest.pos,
                    id: pest.id,
                    fast: pest.fast,
                    lucky: pest.lucky,
                });
            } else {
                index += 1;
            }
        }
        if self.active && self.remaining == 0 && self.pests.is_empty() {
            self.active = false;
            self.shots.clear();
            for target in &mut self.targets {
                target.reset();
            }
            self.money += wave.reward;
            let perfect = self.escaped == self.leaks_at_launch;
            self.perfect_waves += u8::from(perfect);
            self.emit(Feed::Cleared {
                wave: self.wave,
                perfect,
                reward: wave.reward,
            });
            if usize::from(self.wave) == self.level.waves.len() {
                self.outcome = Outcome::Won;
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wave_preview_matches_global_runner_cadence_without_launching() {
        let mut game = Game::new();
        for index in 0..game.level.waves.len() {
            game.wave = index as u8;
            game.kills = game.level.waves[..index]
                .iter()
                .map(|w| u32::from(w.count))
                .sum();
            game.restore_spawn_sequence();
            let before = (game.next_id, game.money, game.time);
            let preview = game.next_wave().unwrap();
            assert_eq!(preview.number, index as u8 + 1);
            for _ in 0..3 {
                assert_eq!(game.next_wave().unwrap().fast_count, preview.fast_count);
            }
            assert_eq!((game.next_id, game.money, game.time), before);
            assert!(game.launch());
            assert!(game.next_wave().is_none());
            for _ in 0..3000 {
                game.step(1. / 60.);
                if game.remaining == 0 {
                    break;
                }
            }
            assert_eq!(game.remaining, 0);
            assert_eq!(game.pests.len(), usize::from(preview.spec.count));
            assert_eq!(
                game.pests.iter().filter(|p| p.fast).count() as u32,
                preview.fast_count
            );
            game.pests.clear();
            game.active = false;
        }
        game.outcome = Outcome::Won;
        assert!(game.next_wave().is_none());
    }

    #[test]
    fn target_identity_survives_reordering_and_rejects_dead_or_out_of_range_pests() {
        let mut game = Game::new();
        assert!(game.build(0, Kind::Seed));
        game.launch();
        game.step(0.);
        game.pests[0]
            .cursor
            .advance(&game.route, 600., 0.25)
            .unwrap();
        let mut second = game.pests[0].clone();
        second.id = 2;
        second.cursor.advance(&game.route, 300., 0.25).unwrap();
        game.pests.push(second);
        game.step(0.);
        assert_eq!(
            game.selected_target().unwrap().id,
            2,
            "closest to the exit, not nearest tower"
        );
        let shot = &game.shots[0];
        assert_eq!(shot.target, 2);
        assert_eq!(shot.damage, game.towers[0].unwrap().damage());
        assert_eq!(
            game.towers[0].unwrap().cooldown,
            game.towers[0].unwrap().reload()
        );
        game.pests.swap(0, 1);
        game.step(0.);
        assert_eq!(game.selected_target().unwrap().id, 2);
        game.pests[0].hp = 0.;
        assert!(game.selected_target().is_none());
        game.step(0.);
        assert_eq!(game.selected_target().unwrap().id, 1);
        assert!(
            game.shots.is_empty(),
            "dead targets retire their in-flight projectiles"
        );
        game.pests[0].cursor = PathCursor::default();
        game.step(0.);
        assert!(game.selected_target().is_none());
    }

    #[test]
    fn displayed_stats_match_each_levels_projectiles_and_reload() {
        for kind in [Kind::Seed, Kind::Frost] {
            let mut game = Game::new();
            game.money = 1000;
            game.build(0, kind);
            game.launch();
            game.step(0.);
            game.pests[0].hp = 10000.;
            game.pests[0]
                .cursor
                .advance(&game.route, 600., 0.25)
                .unwrap();
            for level in 0..3 {
                game.towers[0].as_mut().unwrap().cooldown = 0.;
                game.step(0.);
                let tower = game.towers[0].unwrap();
                assert_eq!(tower.level, level);
                assert_eq!(game.shots.last().unwrap().damage, tower.damage());
                assert_eq!(tower.cooldown, tower.reload());
                if level < 2 {
                    let before = game.money;
                    assert!(game.upgrade(0));
                    assert_eq!(game.money, before - tower.upgrade_cost());
                    assert!(game.towers[0].unwrap().damage() > tower.damage());
                    assert_eq!(game.towers[0].unwrap().range(), tower.range() + 18.);
                    assert!(game.towers[0].unwrap().reload() < tower.reload());
                }
            }
        }
    }

    #[test]
    fn rejected_actions_explain_why_without_changing_the_economy() {
        let mut game = Game::new();
        assert_eq!(
            game.build_status(99, Kind::Seed),
            Err(ActionError::InvalidPlot)
        );
        assert_eq!(game.upgrade_status(0), Err(ActionError::Empty));
        assert!(game.build(0, Kind::Seed));
        assert_eq!(
            game.build_status(0, Kind::Frost),
            Err(ActionError::Occupied)
        );
        assert!(game.upgrade(0));
        assert_eq!(
            game.upgrade_status(0),
            Err(ActionError::Funds { missing: 25 })
        );
        assert_eq!(
            game.build_status(1, Kind::Frost),
            Err(ActionError::Funds { missing: 5 })
        );
        for _ in 0..10 {
            assert!(!game.upgrade(0));
        }
        assert_eq!(game.money, 75);
        game.money = 100;
        assert!(game.upgrade(0));
        assert_eq!(game.upgrade_status(0), Err(ActionError::FullyGrown));
        assert!(game.launch());
        assert_eq!(game.wave_status(), Err(ActionError::WaveActive));
        game.outcome = Outcome::Lost;
        assert_eq!(game.upgrade_status(0), Err(ActionError::Finished));
        assert_eq!(game.build_status(1, Kind::Seed), Err(ActionError::Finished));
        assert_eq!(game.wave_status(), Err(ActionError::Finished));
    }

    #[test]
    fn clearing_a_wave_pays_its_reward_once_and_clears_projectiles() {
        let mut game = Game::new();
        game.autoplay();
        for _ in 0..7200 {
            game.step(1. / 60.);
            if !game.active {
                break;
            }
        }
        assert_eq!(game.outcome, Outcome::Playing);
        assert!(!game.active);
        assert_eq!(game.wave, 1);
        assert!(game.shots.is_empty() && game.pests.is_empty());
        let expected = game.level.money - Kind::Seed.price()
            + game.kills * game.level.kill_reward
            + game.level.waves[0].reward;
        assert_eq!(game.money, expected);
        for _ in 0..120 {
            game.step(1. / 60.);
        }
        assert_eq!(
            game.money, expected,
            "idle ticks must not pay the wave again"
        );
    }

    #[test]
    fn frost_splashes_slow_nearby_pests_without_duplicate_damage() {
        let mut game = Game::new();
        game.launch();
        game.step(0.01);
        game.spawn_timer = 0.;
        game.step(0.01);
        assert_eq!(game.pests.len(), 2);
        let target = &game.pests[0];
        let before = target.hp;
        game.shots.push(Shot {
            pos: target.pos,
            target: target.id,
            damage: 8.,
            kind: Kind::Frost,
            lucky: false,
        });
        let neighbor_hp = game.pests[1].hp;
        game.step(1. / 120.);
        assert_eq!(game.pests[0].hp, before - 8.);
        assert_eq!(game.pests[1].hp, neighbor_hp);
        assert!(
            game.pests
                .iter()
                .all(|p| p.slow().is_some_and(|s| s.remaining_seconds > 1.) && p.impacts > 0)
        );
    }

    #[test]
    fn purchases_are_atomic_and_cannot_overspend() {
        let mut game = Game::new();
        assert!(game.build(0, Kind::Seed));
        assert_eq!(game.money, 140);
        assert!(!game.build(0, Kind::Frost));
        assert_eq!(game.money, 140);
        assert!(game.upgrade(0));
        assert_eq!(game.money, 75);
        assert!(!game.upgrade(0));
        assert_eq!(game.towers[0].unwrap().level, 1);
        assert!(!game.build(99, Kind::Seed));
        assert!(!game.upgrade(99));
    }
    #[test]
    fn unattended_orchard_loses_and_cannot_restart_wave() {
        let mut game = Game::new();
        assert!(game.launch());
        assert!(!game.launch());
        for _ in 0..18000 {
            if !game.active {
                game.launch();
            }
            game.step(1. / 60.);
        }
        assert_eq!(game.outcome, Outcome::Lost);
        assert_eq!(game.lives, 0);
        assert!(!game.launch());
        assert!(!game.build(0, Kind::Seed));
    }
    #[test]
    fn real_autoplay_economy_wins_all_waves_with_bounded_entities() {
        let mut game = Game::new();
        for _ in 0..24000 {
            game.autoplay();
            game.step(1. / 60.);
            assert!(game.pests.len() <= MAX_PESTS);
            assert!(game.shots.len() <= MAX_SHOTS);
            if game.outcome != Outcome::Playing {
                break;
            }
        }
        assert_eq!(game.outcome, Outcome::Won);
        assert_eq!(usize::from(game.wave), game.level.waves.len());
        assert_eq!(game.kills + game.escaped, 64);
        assert!(game.lives > 0);
        assert!(game.towers.iter().flatten().any(|t| t.kind == Kind::Frost));
        assert!(game.towers.iter().flatten().any(|t| t.level > 0));
        assert!(!game.launch());
    }

    #[test]
    fn lucky_shots_follow_the_saved_fired_cadence_with_double_damage() {
        let mut game = Game::new();
        game.money = 1000;
        assert!(game.build(0, Kind::Seed));
        game.launch();
        game.step(0.);
        game.pests[0].hp = 1.0e6;
        game.pests[0].max_hp = 1.0e6;
        game.pests[0]
            .cursor
            .advance(&game.route, 600., 0.25)
            .unwrap();
        let base = game.towers[0].unwrap().damage();
        for shot in 1..=(LUCKY_EVERY * 2) {
            game.towers[0].as_mut().unwrap().cooldown = 0.;
            game.step(0.);
            let last = game.shots.last().unwrap();
            let lucky = shot % LUCKY_EVERY == 0;
            assert_eq!(game.fired, shot);
            assert_eq!(last.lucky, lucky, "shot {shot}");
            assert_eq!(last.damage, if lucky { base * 2. } else { base });
        }
        // Land every projectile: lucky hits are counted exactly once.
        for _ in 0..240 {
            game.towers[0].as_mut().unwrap().cooldown = 1.;
            game.step(1. / 60.);
        }
        assert_eq!(game.lucky_hits, 2);
        assert!(
            game.feed
                .iter()
                .any(|e| matches!(e, Feed::Hit { lucky: true, .. }))
        );
    }

    #[test]
    fn feed_records_accepted_milestones_once_and_stays_bounded() {
        let mut game = Game::new();
        let mut undrained = Game::new();
        let (mut launched, mut cleared, mut perfect, mut stopped, mut escaped) = (0, 0, 0, 0, 0);
        let (mut planted, mut grown, mut royal) = (0, 0, false);
        for _ in 0..24000 {
            game.autoplay();
            game.step(1. / 60.);
            undrained.autoplay();
            undrained.step(1. / 60.);
            assert!(undrained.feed.len() <= MAX_FEED);
            assert!(undrained.feed.capacity() <= MAX_FEED, "never reallocates");
            for event in game.feed.drain(..) {
                match event {
                    Feed::Launched { .. } => launched += 1,
                    Feed::Cleared { perfect: p, .. } => {
                        cleared += 1;
                        perfect += u8::from(p);
                    }
                    Feed::Stopped { id, .. } => {
                        stopped += 1;
                        royal |= id == ROYAL_ID;
                    }
                    Feed::Escaped { .. } => escaped += 1,
                    Feed::Planted { .. } => planted += 1,
                    Feed::Grown { .. } => grown += 1,
                    Feed::Fired { .. } | Feed::Hit { .. } => {}
                }
            }
            if game.outcome != Outcome::Playing {
                break;
            }
        }
        assert_eq!(game.outcome, Outcome::Won);
        assert_eq!((launched, cleared), (4, 4));
        assert_eq!((stopped, escaped), (game.kills, game.escaped));
        assert_eq!(perfect, game.perfect_waves);
        assert_eq!(royal, game.royal_stopped);
        assert_eq!(planted, game.towers.iter().flatten().count());
        assert_eq!(
            grown,
            game.towers
                .iter()
                .flatten()
                .map(|t| usize::from(t.level))
                .sum::<usize>()
        );
        // Presentation bookkeeping never changes the simulation itself.
        assert_eq!(
            (undrained.money, undrained.kills, undrained.time),
            (game.money, game.kills, game.time)
        );
    }

    #[test]
    fn a_leak_spoils_only_its_own_wave_and_restores_count_perfect_waves() {
        let mut game = Game::new();
        assert!(game.launch());
        for _ in 0..7200 {
            game.step(1. / 60.);
            if !game.active {
                break;
            }
        }
        assert!(game.escaped > 0);
        assert_eq!(game.perfect_waves, 0);
        assert!(game.feed.contains(&Feed::Cleared {
            wave: 1,
            perfect: false,
            reward: game.level.waves[0].reward,
        }));
        let mut clean = Game::new();
        clean.wave = 2;
        clean.kills = 24;
        clean.restore_spawn_sequence();
        assert_eq!(clean.perfect_waves, 2);
        game.restore_spawn_sequence();
        assert_eq!(game.perfect_waves, 0);
    }
}
