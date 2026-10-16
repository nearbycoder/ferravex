use ferravex_play::attributes::{EffectSet, EffectSpec};
use ferravex_play::{
    ai::{Candidate, Observer, Sight, Unoccluded, WorkBudget, select_target},
    navigation::{PathCursor, Route3},
};
use ferravex_render::prelude::*;

use crate::level::Level;
use std::sync::Arc;
pub const MAX_PESTS: usize = 32;
pub const MAX_SHOTS: usize = 96;
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
    pub slow: f32,
    pub fast: bool,
    pub stagger: f32,
    pub flash: f32,
    cursor: PathCursor,
    effects: EffectSet<4>,
}
#[derive(Clone, Debug)]
pub struct Shot {
    pub pos: Vec2,
    pub target: u32,
    pub damage: f32,
    pub kind: Kind,
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
    spawn_timer: f32,
    next_id: u32,
    route: Route3,
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
            spawn_timer: 0.,
            next_id: 0,
            route,
        }
    }
    pub fn build(&mut self, plot: usize, kind: Kind) -> bool {
        if self.outcome != Outcome::Playing
            || plot >= self.level.plots.len()
            || self.towers[plot].is_some()
            || self.money < kind.price()
        {
            return false;
        }
        self.money -= kind.price();
        self.towers[plot] = Some(Tower {
            kind,
            level: 0,
            cooldown: 0.,
            aim: vec2(0., -1.),
        });
        true
    }
    pub fn upgrade(&mut self, plot: usize) -> bool {
        let Some(Some(tower)) = self.towers.get_mut(plot) else {
            return false;
        };
        if self.outcome != Outcome::Playing || tower.level >= 2 || self.money < tower.upgrade_cost()
        {
            return false;
        }
        self.money -= tower.upgrade_cost();
        tower.level += 1;
        true
    }
    pub fn launch(&mut self) -> bool {
        if self.active
            || usize::from(self.wave) == self.level.waves.len()
            || self.outcome != Outcome::Playing
        {
            return false;
        }
        self.wave += 1;
        self.remaining = self.level.waves[usize::from(self.wave) - 1].count;
        self.active = true;
        self.spawn_timer = 0.;
        true
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
                    slow: 0.,
                    stagger: 0.,
                    flash: 0.,
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
            pest.slow = pest.effects.remaining(1);
            pest.stagger = pest.effects.remaining(2);
            pest.flash = (pest.flash - dt).max(0.);
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
        self.pests.retain(|p| {
            if p.segment == self.level.path.len() - 1 {
                self.lives = self.lives.saturating_sub(1);
                self.escaped += 1;
                false
            } else {
                true
            }
        });
        if self.lives == 0 {
            self.outcome = Outcome::Lost;
            self.shots.clear();
            return;
        }
        for (plot, tower) in self.towers.iter_mut().enumerate() {
            let Some(tower) = tower else {
                continue;
            };
            tower.cooldown = (tower.cooldown - dt).max(0.);
            if tower.cooldown > 0. || self.shots.len() == MAX_SHOTS {
                continue;
            }
            let selected = select_target(
                self.pests
                    .iter()
                    .enumerate()
                    .filter(|(_, p)| p.hp > 0.)
                    .map(|(i, p)| Candidate {
                        id: i as u64,
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
                |candidate| self.pests[candidate.id as usize].distance,
            )
            .unwrap();
            if let Some(pest) = selected.target.map(|c| &self.pests[c.id as usize]) {
                tower.aim = (pest.pos - self.level.plots[plot]).normalize_or_zero();
                self.fired = self.fired.saturating_add(1);
                self.shots.push(Shot {
                    pos: self.level.plots[plot] + tower.aim * 18.,
                    target: pest.id,
                    damage: match tower.kind {
                        Kind::Seed => 22. + tower.level as f32 * 14.,
                        Kind::Frost => 8. + tower.level as f32 * 6.,
                    },
                    kind: tower.kind,
                });
                tower.cooldown = match tower.kind {
                    Kind::Seed => 0.62,
                    Kind::Frost => 0.85,
                } - tower.level as f32 * 0.08;
            }
        }
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
                target.flash = 0.12;
                if shot.kind == Kind::Frost {
                    target
                        .effects
                        .apply(EffectSpec::modifier(1, 0, 0.45, 1.65), 0)
                        .unwrap();
                    target.slow = target.effects.remaining(1);
                    frost_impacts[frost_count] = target.pos;
                    frost_count += 1;
                } else {
                    // Small, bounded hit-stop gives seed hits weight without
                    // teleporting pests backward or invalidating route progress.
                    target
                        .effects
                        .apply(EffectSpec::modifier(2, 0, 0.35, 0.10), 0)
                        .unwrap();
                    target.stagger = target.effects.remaining(2);
                }
                false
            } else {
                shot.pos += delta.normalize_or_zero() * travel;
                true
            }
        });
        for center in &frost_impacts[..frost_count] {
            for pest in &mut self.pests {
                if pest.hp > 0. && pest.pos.distance_squared(*center) < 46. * 46. {
                    pest.effects
                        .apply(EffectSpec::modifier(1, 0, 0.45, 1.15), 0)
                        .unwrap();
                    pest.slow = pest.effects.remaining(1);
                    pest.flash = pest.flash.max(0.08);
                }
            }
        }
        self.pests.retain(|p| {
            if p.hp <= 0. {
                self.money += self.level.kill_reward;
                self.kills += 1;
                false
            } else {
                true
            }
        });
        if self.active && self.remaining == 0 && self.pests.is_empty() {
            self.active = false;
            self.shots.clear();
            self.money += wave.reward;
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
        });
        let neighbor_hp = game.pests[1].hp;
        game.step(1. / 120.);
        assert_eq!(game.pests[0].hp, before - 8.);
        assert_eq!(game.pests[1].hp, neighbor_hp);
        assert!(game.pests.iter().all(|p| p.slow > 1. && p.flash > 0.));
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
}
