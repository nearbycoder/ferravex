//! The tide director (spawn curve, swarms, elites, bosses) and foe behavior.
use super::content::*;
use super::{
    Bullet, Event, Foe, Game, MAX_BULLETS, MAX_FOES, MAX_TELEGRAPHS, PLAYER_RADIUS, Phase,
    PickupKind, SPAWN_RADIUS, Shape, Telegraph,
};
use ferravex_play::gameplay::loot::RewardTable;
use ferravex_render::prelude::{Vec2, vec2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BossKind {
    MotherBrine,
    Gulpjaw,
}

impl BossKind {
    pub fn name(self) -> &'static str {
        match self {
            BossKind::MotherBrine => "Mother Brine",
            BossKind::Gulpjaw => "Old Gulpjaw",
        }
    }
    pub fn title(self) -> &'static str {
        match self {
            BossKind::MotherBrine => "Jelly Matriarch, Disappointed Parent",
            BossKind::Gulpjaw => "The Anglerking Who Ate The Moon",
        }
    }
    fn foe(self) -> FoeKind {
        match self {
            BossKind::MotherBrine => FoeKind::MotherBrine,
            BossKind::Gulpjaw => FoeKind::Gulpjaw,
        }
    }
    fn from_foe(kind: FoeKind) -> Option<Self> {
        match kind {
            FoeKind::MotherBrine => Some(BossKind::MotherBrine),
            FoeKind::Gulpjaw => Some(BossKind::Gulpjaw),
            _ => None,
        }
    }
}

/// Scheduled beats on the logical ten-minute clock.
const EVENTS: [(f32, u8); 8] = [
    (1.5, 0), // mothling migration
    (2.5, 1), // driftlet bloom ring
    (3.5, 2), // eel pack
    (4.9, 3), // mother brine
    (6.5, 4), // crab parade
    (7.5, 5), // bloom ring with grumps
    (8.5, 0), // second migration
    (9.4, 6), // old gulpjaw
];
const ELITES: [f32; 8] = [1.0, 2.0, 3.0, 4.0, 6.0, 7.0, 8.0, 8.9];

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Director {
    pub spawn_acc: f32,
    pub events_done: u16,
    pub elites_done: u16,
    pub tide: u8,
    pub elite_names: u8,
    pub boss_alive: bool,
    /// Per-boss pattern state, keyed by boss kind index.
    pub boss: [BossBrain; 2],
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BossBrain {
    pub pattern: u8,
    pub timer: f32,
    pub phase: u8,
    pub step: u16,
    pub angle: f32,
    pub charge: Vec2,
    pub casting: bool,
}

impl Game {
    /// Target living-foe count on the logical clock (minutes).
    pub fn target_population(minute: f32) -> f32 {
        (22. + 42. * minute + 2.6 * minute * minute).min(560.)
    }

    fn hp_scale(&self) -> f32 {
        let m = self.minute();
        1. + 0.16 * m + 0.035 * m * m
    }

    pub(crate) fn spawn_foe(&mut self, kind: FoeKind, pos: Vec2, scale: f32, elite: u8) -> bool {
        if self.foes.len() >= MAX_FOES {
            return false;
        }
        let info = kind.info();
        let minute = self.minute();
        let mut hp = info.hp * scale;
        let mut radius = info.radius;
        let mut damage = info.damage * (1. + 0.06 * minute);
        let mut xp = info.xp;
        if elite > 0 {
            hp *= 9.;
            radius *= 1.55;
            damage *= 1.4;
            xp *= 6;
        }
        let seed = self.rand();
        let uid = self.next_uid;
        self.next_uid = self.next_uid.wrapping_add(1).max(1);
        self.foes.push(Foe {
            pos,
            prev: pos,
            vel: Vec2::ZERO,
            knock: Vec2::ZERO,
            kind,
            hp,
            max_hp: hp,
            radius,
            speed: info.speed * (1. + 0.04 * minute) * (0.9 + 0.2 * seed),
            damage,
            uid,
            elite,
            state: 0,
            timer: seed * 2.,
            aim: Vec2::ZERO,
            hit_cd: [0.; WEAPONS],
            flash: 0.,
            slow: 0.,
            stun: 0.,
            seed,
            dead: false,
            xp,
            last_weapon: 0,
        });
        true
    }

    fn ring_point(&mut self, radius: f32) -> Vec2 {
        // Bias spawns toward where the player is heading so the horde meets them.
        let mut dir = self.rand_dir();
        let heading = self.player.vel.normalize_or_zero();
        if heading != Vec2::ZERO && self.rand() < 0.35 {
            dir = (dir + heading * 1.2).normalize_or_zero();
        }
        self.player.pos + dir * radius
    }

    fn pick_kind(&mut self) -> FoeKind {
        let m = self.minute();
        let weights: [u32; 6] = if m < 1. {
            [100, 0, 0, 0, 0, 0]
        } else if m < 2. {
            [70, 30, 0, 0, 0, 0]
        } else if m < 4. {
            [45, 25, 20, 10, 0, 0]
        } else if m < 6. {
            [30, 20, 20, 15, 15, 0]
        } else if m < 8. {
            [25, 20, 15, 15, 12, 13]
        } else {
            [20, 25, 15, 15, 12, 13]
        };
        let table = RewardTable::new(weights).expect("authored spawn weights");
        [
            FoeKind::Driftlet,
            FoeKind::Mothling,
            FoeKind::Grumpfish,
            FoeKind::Sneel,
            FoeKind::Urchin,
            FoeKind::Crabbo,
        ][table.roll(self.rng())]
    }

    /// Director: population curve, scheduled swarms, elites and bosses.
    pub(crate) fn direct(&mut self, dt: f32) {
        let minute = self.minute();
        let tide = self.tide() as u8;
        if tide != self.director.tide {
            self.director.tide = tide;
            self.emit(Event::TideShift { tide });
        }
        for (k, &(at, beat)) in EVENTS.iter().enumerate() {
            if minute >= at && self.director.events_done & (1 << k) == 0 {
                self.director.events_done |= 1 << k;
                self.beat(beat);
            }
            // Bosses get a short warning beforehand.
            let warn = at - 0.12;
            if beat >= 3 && matches!(beat, 3 | 6) && minute >= warn && minute < at {
                let flag = 1 << (k + 8);
                if self.director.events_done & flag == 0 {
                    self.director.events_done |= flag;
                    let boss = if beat == 3 {
                        BossKind::MotherBrine
                    } else {
                        BossKind::Gulpjaw
                    };
                    self.emit(Event::BossWarning { boss });
                }
            }
        }
        for (k, &at) in ELITES.iter().enumerate() {
            if minute >= at && self.director.elites_done & (1 << k) == 0 {
                self.director.elites_done |= 1 << k;
                let kind = self.pick_kind();
                let pos = self.ring_point(SPAWN_RADIUS * 0.9);
                let name = self.director.elite_names % ELITE_NAMES.len() as u8;
                self.director.elite_names = self.director.elite_names.wrapping_add(1);
                let scale = self.hp_scale();
                if self.spawn_foe(kind, pos, scale, name + 1) {
                    self.emit(Event::EliteSpawn { name, kind });
                }
            }
        }
        self.director.boss_alive = self.foes.iter().any(|f| f.kind.is_boss());
        let mut target = Self::target_population(minute);
        if self.director.boss_alive {
            target *= 0.45;
        }
        let living = self.foes.len() as f32;
        let rate = ((target - living) * 2.2).clamp(2., 90.);
        if living < target {
            self.director.spawn_acc += rate * dt;
        }
        let scale = self.hp_scale();
        while self.director.spawn_acc >= 1. {
            self.director.spawn_acc -= 1.;
            let kind = self.pick_kind();
            let r = SPAWN_RADIUS + self.rand() * 60.;
            let pos = self.ring_point(r);
            if kind == FoeKind::Mothling {
                // Mothlings travel in little clouds.
                for k in 0..4 {
                    let off = vec2((k as f32 * 2.1).cos(), (k as f32 * 2.1).sin()) * 18.;
                    self.spawn_foe(kind, pos + off, scale, 0);
                }
                self.director.spawn_acc -= 2.;
            } else {
                self.spawn_foe(kind, pos, scale, 0);
            }
        }
    }

    fn beat(&mut self, beat: u8) {
        let scale = self.hp_scale();
        let player = self.player.pos;
        match beat {
            0 => {
                self.emit(Event::SwarmWarning { kind: 0 });
                let side = self.rand_dir();
                let across = vec2(-side.y, side.x);
                for k in 0..44 {
                    let lane = (k % 11) as f32 - 5.;
                    let depth = (k / 11) as f32;
                    let pos = player + side * (SPAWN_RADIUS + depth * 40.) + across * lane * 42.;
                    if self.spawn_foe(FoeKind::Mothling, pos, scale, 0) {
                        let f = self.foes.last_mut().unwrap();
                        f.state = 9; // migrating: fly straight across
                        f.aim = -side;
                    }
                }
            }
            1 | 5 => {
                self.emit(Event::SwarmWarning { kind: 1 });
                let count = if beat == 1 { 36 } else { 48 };
                for k in 0..count {
                    let a = k as f32 / count as f32 * std::f32::consts::TAU;
                    let pos = player + vec2(a.cos(), a.sin()) * 430.;
                    let kind = if beat == 5 && k % 3 == 0 {
                        FoeKind::Grumpfish
                    } else {
                        FoeKind::Driftlet
                    };
                    self.spawn_foe(kind, pos, scale, 0);
                }
            }
            2 => {
                self.emit(Event::SwarmWarning { kind: 2 });
                let side = self.rand_dir();
                for k in 0..10 {
                    let off = vec2(-side.y, side.x) * (k as f32 - 4.5) * 50.;
                    self.spawn_foe(FoeKind::Sneel, player + side * SPAWN_RADIUS + off, scale, 0);
                }
            }
            4 => {
                self.emit(Event::SwarmWarning { kind: 3 });
                let side = self.rand_dir();
                for k in 0..12 {
                    let off = vec2(-side.y, side.x) * (k as f32 - 5.5) * 60.;
                    self.spawn_foe(
                        FoeKind::Crabbo,
                        player + side * SPAWN_RADIUS + off,
                        scale,
                        0,
                    );
                }
            }
            3 => self.spawn_boss(BossKind::MotherBrine),
            6 => self.spawn_boss(BossKind::Gulpjaw),
            _ => {}
        }
    }

    pub(crate) fn spawn_boss(&mut self, boss: BossKind) {
        let pos = self.ring_point(520.);
        // Quick tides reach their bosses with half the build time.
        let scale = (1. + 0.1 * self.minute()) * if self.config.quick { 0.6 } else { 1. };
        // Bosses always fit: retire the farthest ordinary foe if full.
        if self.foes.len() >= MAX_FOES
            && let Some(far) = (0..self.foes.len())
                .filter(|&i| !self.foes[i].kind.is_boss())
                .max_by(|&a, &b| {
                    let pa = (self.foes[a].pos - self.player.pos).length_squared();
                    let pb = (self.foes[b].pos - self.player.pos).length_squared();
                    pa.total_cmp(&pb)
                })
        {
            self.foes.swap_remove(far);
        }
        if self.spawn_foe(boss.foe(), pos, scale, 0) {
            self.director.boss[boss as usize] = BossBrain {
                timer: 2.0,
                ..BossBrain::default()
            };
            self.emit(Event::BossSpawn { boss });
        }
    }

    pub(crate) fn boss_defeated(&mut self, f: Foe) {
        let Some(boss) = BossKind::from_foe(f.kind) else {
            return;
        };
        self.emit(Event::BossDefeated { boss, pos: f.pos });
        self.bullets.clear();
        self.telegraphs.clear();
        for k in 0..24 {
            let a = k as f32 * 0.9;
            let off = vec2(a.cos(), a.sin()) * (20. + k as f32 * 5.);
            self.drop_mote(f.pos + off, 25);
        }
        match boss {
            BossKind::MotherBrine => {
                self.stats.brine_down = true;
                self.drop_pickup(f.pos, PickupKind::Chest(2));
            }
            BossKind::Gulpjaw => {
                self.stats.gulpjaw_down = true;
                self.stats.gulpjaw_left = Some(0.);
                self.phase = Phase::Won;
                self.emit(Event::Victory);
            }
        }
    }

    pub(crate) fn fire_bullet(
        &mut self,
        pos: Vec2,
        vel: Vec2,
        radius: f32,
        damage: f32,
        style: u8,
    ) {
        if self.bullets.len() < MAX_BULLETS {
            self.bullets.push(Bullet {
                pos,
                prev: pos,
                vel,
                radius,
                damage,
                life: 4.2,
                age: 0.,
                style,
            });
        }
    }

    pub(crate) fn update_foes(&mut self, dt: f32) {
        let player = self.player.pos;
        let n = self.foes.len();
        let minute = self.minute();
        for i in 0..n {
            let f = &mut self.foes[i];
            f.flash = (f.flash - dt).max(0.);
            f.stun = (f.stun - dt).max(0.);
            let slow = 1. - f.slow.min(0.6);
            f.slow = (f.slow - dt * 0.8).max(0.);
            f.timer += dt;
            let to = player - f.pos;
            let dist = to.length().max(1e-3);
            let dir = to / dist;
            let mut desired = dir * f.speed;
            match f.kind {
                FoeKind::Driftlet => {
                    // A lazy pulse: jellies squeeze, then glide.
                    let pulse = 0.65 + 0.55 * (f.timer * 3.2 + f.seed * 9.).sin().max(0.);
                    desired *= pulse;
                }
                FoeKind::Mothling => {
                    if f.state == 9 {
                        desired = f.aim * f.speed * 1.5;
                        if f.timer > 14. {
                            f.state = 0;
                        }
                    } else {
                        let side = vec2(-dir.y, dir.x);
                        desired += side * (f.timer * 5. + f.seed * 6.).sin() * f.speed * 0.8;
                    }
                }
                FoeKind::Grumpfish => {
                    // Puffs up when close: bigger, slower, grumpier.
                    let base = f.kind.info().radius * if f.elite > 0 { 1.55 } else { 1. };
                    let target = if dist < 110. { base * 1.45 } else { base };
                    f.radius += (target - f.radius) * (1. - (-6. * dt).exp());
                    if dist < 110. {
                        desired *= 0.5;
                    }
                }
                FoeKind::Sneel => match f.state {
                    0 => {
                        if dist < 230. && f.timer > 1.5 {
                            f.state = 1;
                            f.timer = 0.;
                            f.aim = dir;
                        }
                    }
                    1 => {
                        // Telegraph: freeze, glare and aim.
                        desired = Vec2::ZERO;
                        f.aim = (f.aim + (dir - f.aim) * (3. * dt)).normalize_or_zero();
                        if f.timer > 0.75 {
                            f.state = 2;
                            f.timer = 0.;
                        }
                    }
                    _ => {
                        desired = f.aim * 430.;
                        if f.timer > 0.45 {
                            f.state = 0;
                            f.timer = 0.;
                        }
                    }
                },
                FoeKind::Urchin => {
                    if dist < 280. {
                        desired = -dir * f.speed * 0.3;
                        if f.state == 0 && f.timer > 3.4 {
                            f.state = 1;
                            f.timer = 0.;
                            f.aim = dir;
                        }
                    }
                    if f.state == 1 {
                        desired = Vec2::ZERO;
                    }
                }
                FoeKind::Crabbo => {
                    // Sideways scuttle, as is traditional.
                    let side = vec2(-dir.y, dir.x);
                    desired += side * (f.timer * 1.3 + f.seed * 5.).sin() * f.speed * 0.6;
                }
                FoeKind::MotherBrine | FoeKind::Gulpjaw => {
                    desired = Vec2::ZERO; // bosses steer in update_bosses
                }
            }
            if f.stun > 0. {
                desired = Vec2::ZERO;
            }
            f.vel = desired * slow;
            f.pos += (f.vel + f.knock) * dt;
            f.knock *= (-9. * dt).exp();
            // Recycle stragglers ahead of the player instead of letting them trail forever.
            if dist > 1250. && f.elite == 0 && !f.kind.is_boss() && f.state != 9 {
                f.pos = player + dir * 760.;
                f.prev = f.pos;
            }
        }
        // Urchin volleys happen after movement so bullets spawn from final positions.
        for i in 0..n {
            let f = self.foes[i];
            if f.kind == FoeKind::Urchin && f.state == 1 && f.timer > 0.7 {
                self.foes[i].state = 0;
                self.foes[i].timer = 0.;
                let dmg = 7. * (1. + 0.06 * minute);
                for k in -1..=1 {
                    let dir = rotate(f.aim, k as f32 * 0.28);
                    self.fire_bullet(f.pos + dir * f.radius, dir * 135., 6., dmg, 0);
                }
            }
        }
        self.separate();
        // Contact damage.
        let mut touched: Option<f32> = None;
        self.grid.visit(player, PLAYER_RADIUS + 80., |j| {
            if let Some(f) = self.foes.get(j) {
                let rr = PLAYER_RADIUS + f.radius * 0.85;
                if !f.dead && (f.pos - player).length_squared() < rr * rr {
                    touched = Some(touched.map_or(f.damage, |d: f32| d.max(f.damage)));
                }
            }
        });
        if let Some(damage) = touched {
            self.hurt_player(damage);
        }
    }

    /// Soft-body separation so the horde reads as a mass instead of one blob.
    fn separate(&mut self) {
        let n = self.foes.len();
        let Game { grid, foes, .. } = self;
        for i in 0..n {
            let pi = foes[i].pos;
            let ri = foes[i].radius;
            let wi = 1. - foes[i].kind.info().sturdiness * 0.9;
            let mut push = Vec2::ZERO;
            let mut contacts = 0;
            grid.visit_until(pi, ri + 70., |j| {
                if j <= i || j >= n {
                    return true;
                }
                let d = foes[j].pos - pi;
                let rr = ri + foes[j].radius;
                let d2 = d.length_squared();
                if d2 < rr * rr && d2 > 1e-6 {
                    let dist = d2.sqrt();
                    let overlap = (rr - dist) * 0.5;
                    let dir = d / dist;
                    let wj = 1. - foes[j].kind.info().sturdiness * 0.9;
                    foes[j].pos += dir * overlap * wj;
                    push -= dir * overlap * wi;
                    contacts += 1;
                }
                contacts < 6
            });
            foes[i].pos += push;
        }
    }

    pub(crate) fn update_bosses(&mut self, dt: f32) {
        let Some(i) = self.foes.iter().position(|f| f.kind.is_boss() && !f.dead) else {
            return;
        };
        let f = self.foes[i];
        let Some(kind) = BossKind::from_foe(f.kind) else {
            return;
        };
        let mut brain = self.director.boss[kind as usize];
        let player = self.player.pos;
        let to = player - f.pos;
        let dist = to.length().max(1e-3);
        let dir = to / dist;
        let health = f.hp / f.max_hp;
        let phase = match kind {
            BossKind::MotherBrine => u8::from(health < 0.5),
            BossKind::Gulpjaw => u8::from(health < 0.6) + u8::from(health < 0.25),
        };
        if phase > brain.phase {
            brain.phase = phase;
            self.emit(Event::BossPhase { boss: kind, phase });
        }
        brain.timer -= dt;
        let mut vel;
        match kind {
            BossKind::MotherBrine => {
                // Drift to a comfortable menace distance and orbit.
                let want = 250.;
                let side = vec2(-dir.y, dir.x);
                vel = dir * (dist - want).clamp(-60., 60.) + side * 30.;
                if brain.casting {
                    vel *= 0.3;
                }
                let speed_up = if phase > 0 { 0.75 } else { 1. };
                if brain.timer <= 0. {
                    if !brain.casting {
                        brain.casting = true;
                        brain.timer = 0.9 * speed_up;
                        self.emit(Event::BossCast {
                            boss: kind,
                            pattern: brain.pattern,
                        });
                        if brain.pattern == 2 {
                            let count = if phase > 0 { 5 } else { 3 };
                            for k in 0..count {
                                let lead = self.player.vel * 0.55;
                                let jitter = if k == 0 {
                                    Vec2::ZERO
                                } else {
                                    self.rand_dir() * self.rand_range(60., 150.)
                                };
                                if self.telegraphs.len() < MAX_TELEGRAPHS {
                                    self.telegraphs.push(Telegraph {
                                        shape: Shape::Circle {
                                            pos: player + lead + jitter,
                                            radius: 68.,
                                        },
                                        age: 0.,
                                        delay: 1.15,
                                        damage: 24.,
                                        style: 0,
                                    });
                                }
                            }
                        }
                    } else {
                        brain.casting = false;
                        match brain.pattern {
                            0 => {
                                let count = if phase > 0 { 12 } else { 8 };
                                let scale = self.hp_scale();
                                for k in 0..count {
                                    let a = k as f32 / count as f32 * std::f32::consts::TAU;
                                    let pos = f.pos + vec2(a.cos(), a.sin()) * (f.radius + 20.);
                                    self.spawn_foe(FoeKind::Driftlet, pos, scale, 0);
                                }
                            }
                            1 => {
                                let count = if phase > 0 { 28 } else { 20 };
                                let offset = self.rand() * 6.;
                                for ring in 0..2 {
                                    for k in 0..count {
                                        let a = (k as f32 + ring as f32 * 0.5) / count as f32
                                            * std::f32::consts::TAU
                                            + offset;
                                        let d = vec2(a.cos(), a.sin());
                                        let speed = 105. + ring as f32 * 45.;
                                        self.fire_bullet(
                                            f.pos + d * f.radius,
                                            d * speed,
                                            8.,
                                            14.,
                                            1,
                                        );
                                    }
                                }
                            }
                            _ => {}
                        }
                        brain.pattern = (brain.pattern + 1) % 3;
                        brain.timer = 2.6 * speed_up;
                    }
                }
            }
            BossKind::Gulpjaw => {
                let speed_up: f32 = match phase {
                    0 => 1.,
                    1 => 0.8,
                    _ => 0.62,
                };
                let side = vec2(-dir.y, dir.x);
                vel = dir * (dist - 300.).clamp(-70., 70.) + side * 40.;
                match (brain.casting, brain.pattern) {
                    (true, 0) => {
                        // Lure: drag the player in while moths flutter out.
                        vel *= 0.2;
                        self.player.pull = -dir * 105.;
                        brain.step += 1;
                        if brain.step.is_multiple_of(30) {
                            let scale = self.hp_scale();
                            let pos = f.pos + self.rand_dir() * (f.radius + 10.);
                            self.spawn_foe(FoeKind::Mothling, pos, scale, 0);
                        }
                    }
                    (true, 1) | (true, 3) => {
                        if brain.step == 0 {
                            vel = Vec2::ZERO; // windup while the lane telegraph fills
                        } else {
                            vel = brain.charge * if phase >= 2 { 720. } else { 620. };
                        }
                    }
                    (true, 2) => {
                        vel *= 0.4;
                        brain.step += 1;
                        let every = if phase >= 1 { 3 } else { 4 };
                        if brain.step.is_multiple_of(every) {
                            let arms = if phase >= 1 { 3 } else { 2 };
                            brain.angle += 0.23;
                            for k in 0..arms {
                                let a =
                                    brain.angle + k as f32 * std::f32::consts::TAU / arms as f32;
                                let d = vec2(a.cos(), a.sin());
                                self.fire_bullet(f.pos + d * f.radius * 0.8, d * 150., 8., 16., 2);
                            }
                        }
                    }
                    _ => {}
                }
                if brain.timer <= 0. {
                    if !brain.casting {
                        brain.casting = true;
                        brain.step = 0;
                        self.emit(Event::BossCast {
                            boss: kind,
                            pattern: brain.pattern,
                        });
                        match brain.pattern {
                            0 => brain.timer = 2.4,
                            1 | 3 => {
                                brain.charge = dir;
                                brain.timer = 1.0 * speed_up.max(0.8);
                                if self.telegraphs.len() < MAX_TELEGRAPHS {
                                    self.telegraphs.push(Telegraph {
                                        shape: Shape::Lane {
                                            from: f.pos,
                                            dir,
                                            length: 1000.,
                                            width: f.radius * 2.,
                                        },
                                        age: 0.,
                                        delay: brain.timer,
                                        damage: 0.,
                                        style: 1,
                                    });
                                }
                            }
                            _ => brain.timer = 3.0,
                        }
                    } else if matches!(brain.pattern, 1 | 3) && brain.step == 0 {
                        // Telegraph done: CHARGE.
                        brain.step = 1;
                        brain.timer = 1.15;
                        self.emit(Event::BossCast {
                            boss: kind,
                            pattern: 10,
                        });
                    } else {
                        brain.casting = false;
                        brain.pattern = (brain.pattern + 1) % 4;
                        brain.timer = 1.6 * speed_up;
                        if phase >= 2 {
                            // Enraged: a parting nova after every pattern.
                            for k in 0..24 {
                                let a = k as f32 / 24. * std::f32::consts::TAU + brain.angle;
                                let d = vec2(a.cos(), a.sin());
                                self.fire_bullet(f.pos + d * f.radius, d * 120., 8., 16., 2);
                            }
                        }
                    }
                }
            }
        }
        let foe = &mut self.foes[i];
        foe.vel = vel;
        foe.pos += vel * dt;
        foe.aim = dir;
        self.director.boss[kind as usize] = brain;
    }
}

fn rotate(v: Vec2, angle: f32) -> Vec2 {
    let (sin, cos) = angle.sin_cos();
    vec2(v.x * cos - v.y * sin, v.x * sin + v.y * cos)
}
