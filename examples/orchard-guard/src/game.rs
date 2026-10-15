use ferravex_render::prelude::*;

pub const WAVES: u8 = 4;
pub const MAX_PESTS: usize = 32;
pub const MAX_SHOTS: usize = 96;
pub const PATH: [Vec2; 10] = [
    vec2(-30., 180.),
    vec2(260., 180.),
    vec2(260., 390.),
    vec2(500., 390.),
    vec2(500., 210.),
    vec2(770., 210.),
    vec2(770., 490.),
    vec2(1070., 490.),
    vec2(1070., 330.),
    vec2(1240., 330.),
];
pub const PLOTS: [Vec2; 8] = [
    vec2(160., 280.),
    vec2(360., 280.),
    vec2(400., 490.),
    vec2(600., 310.),
    vec2(670., 420.),
    vec2(880., 330.),
    vec2(975., 410.),
    vec2(970., 580.),
];
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
    pub towers: [Option<Tower>; 8],
    pub pests: Vec<Pest>,
    pub shots: Vec<Shot>,
    pub money: u32,
    pub lives: u8,
    pub wave: u8,
    pub active: bool,
    pub remaining: u8,
    pub kills: u32,
    pub escaped: u32,
    pub outcome: Outcome,
    pub selected: usize,
    pub time: f32,
    spawn_timer: f32,
    next_id: u32,
}
impl Game {
    pub fn new() -> Self {
        Self {
            towers: [None; 8],
            pests: Vec::with_capacity(MAX_PESTS),
            shots: Vec::with_capacity(MAX_SHOTS),
            money: 200,
            lives: 12,
            wave: 0,
            active: false,
            remaining: 0,
            kills: 0,
            escaped: 0,
            outcome: Outcome::Playing,
            selected: 0,
            time: 0.,
            spawn_timer: 0.,
            next_id: 0,
        }
    }
    pub fn build(&mut self, plot: usize, kind: Kind) -> bool {
        if self.outcome != Outcome::Playing
            || plot >= PLOTS.len()
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
        if self.active || self.wave == WAVES || self.outcome != Outcome::Playing {
            return false;
        }
        self.wave += 1;
        self.remaining = 6 + self.wave * 4;
        self.active = true;
        self.spawn_timer = 0.;
        true
    }
    /// Uses the same economy/actions as human input. No currency, health or combat cheats.
    pub fn autoplay(&mut self) {
        for (plot, kind) in [
            (1, Kind::Seed),
            (3, Kind::Frost),
            (0, Kind::Seed),
            (4, Kind::Seed),
            (5, Kind::Seed),
            (6, Kind::Frost),
            (2, Kind::Seed),
            (7, Kind::Seed),
        ] {
            if self.towers[plot].is_none() {
                self.build(plot, kind);
                break;
            }
        }
        if self.towers.iter().flatten().count() >= 4 {
            for plot in [1, 3, 4, 0, 5, 6, 2, 7] {
                if self.towers[plot].is_some_and(|t| t.level < 2) {
                    self.upgrade(plot);
                    break;
                }
            }
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
        if self.active && self.remaining > 0 {
            self.spawn_timer -= dt;
            if self.spawn_timer <= 0. && self.pests.len() < MAX_PESTS {
                self.next_id += 1;
                let fast = self.next_id.is_multiple_of(4);
                let hp = (30. + self.wave as f32 * 16.) * if fast { 0.7 } else { 1. };
                self.pests.push(Pest {
                    id: self.next_id,
                    pos: PATH[0],
                    segment: 0,
                    distance: 0.,
                    hp,
                    max_hp: hp,
                    slow: 0.,
                    fast,
                });
                self.remaining -= 1;
                self.spawn_timer += 0.95 - self.wave as f32 * 0.08;
            }
        }
        for pest in &mut self.pests {
            pest.slow = (pest.slow - dt).max(0.);
            let mut travel =
                (if pest.fast { 105. } else { 66. }) * if pest.slow > 0. { 0.45 } else { 1. } * dt;
            pest.distance += travel;
            while travel > 0. && pest.segment < PATH.len() - 1 {
                let delta = PATH[pest.segment + 1] - pest.pos;
                let length = delta.length();
                if travel >= length {
                    pest.pos = PATH[pest.segment + 1];
                    travel -= length;
                    pest.segment += 1;
                } else {
                    pest.pos += delta / length * travel;
                    travel = 0.;
                }
            }
        }
        self.pests.retain(|p| {
            if p.segment == PATH.len() - 1 {
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
            let target = self
                .pests
                .iter()
                .filter(|p| {
                    p.hp > 0. && p.pos.distance_squared(PLOTS[plot]) <= tower.range().powi(2)
                })
                .max_by(|a, b| a.distance.total_cmp(&b.distance));
            if let Some(pest) = target {
                tower.aim = (pest.pos - PLOTS[plot]).normalize_or_zero();
                self.shots.push(Shot {
                    pos: PLOTS[plot] + tower.aim * 18.,
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
                if shot.kind == Kind::Frost {
                    target.slow = 1.65;
                }
                false
            } else {
                shot.pos += delta.normalize_or_zero() * travel;
                true
            }
        });
        self.pests.retain(|p| {
            if p.hp <= 0. {
                self.money += 13;
                self.kills += 1;
                false
            } else {
                true
            }
        });
        if self.active && self.remaining == 0 && self.pests.is_empty() {
            self.active = false;
            self.shots.clear();
            self.money += 45;
            if self.wave == WAVES {
                self.outcome = Outcome::Won;
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
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
        assert_eq!(game.wave, WAVES);
        assert_eq!(game.kills + game.escaped, 64);
        assert!(game.lives > 0);
        assert!(game.towers.iter().flatten().any(|t| t.kind == Kind::Frost));
        assert!(game.towers.iter().flatten().any(|t| t.level > 0));
        assert!(!game.launch());
    }
}
