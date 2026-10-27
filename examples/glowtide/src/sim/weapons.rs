//! Eight weapons with distinct feel: homing, orbit, lob, sweep, chain, aura,
//! boomerang and ricochet. Hits go through [`Game::damage_foe`], which records
//! accepted damage per weapon for the run summary.
use super::content::*;
use super::{
    Event, Game, Lash, MAX_LASHES, MAX_SHOTS, MAX_ZAPS, MAX_ZONES, SKIP_BOUNDS, Shot, ShotKind,
    WeaponSlot, Zap, Zone,
};
use ferravex_render::prelude::{Vec2, vec2};

/// Weapon numbers before global modifiers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WStats {
    pub damage: f32,
    pub cooldown: f32,
    pub amount: u32,
    pub area: f32,
    pub speed: f32,
    pub duration: f32,
    pub pierce: u8,
    pub knock: f32,
    /// Weapon-specific extra: chain jumps, lash both-sides, aura slow, etc.
    pub extra: f32,
}

/// Authored per-level progression. Levels outside `1..=7` clamp.
pub fn weapon_stats(id: WeaponId, level: u8, evolved: bool) -> WStats {
    let level = level.clamp(1, WEAPON_MAX);
    let at = |l: u8| level >= l;
    let mut s = match id {
        WeaponId::WispBolt => WStats {
            damage: 12.,
            cooldown: 1.0,
            amount: 1,
            area: 1.,
            speed: 390.,
            duration: 1.6,
            pierce: 1,
            knock: 70.,
            extra: 0.,
        },
        WeaponId::HaloOrbit => WStats {
            damage: 12.,
            cooldown: 4.0,
            amount: 2,
            area: 1.,
            speed: 3.3,
            duration: 2.8,
            pierce: 0,
            knock: 150.,
            extra: 0.,
        },
        WeaponId::BubbleMortar => WStats {
            damage: 20.,
            cooldown: 1.9,
            amount: 1,
            area: 1.,
            speed: 0.55,
            duration: 0.,
            pierce: 0,
            knock: 190.,
            extra: 0.,
        },
        WeaponId::TideLash => WStats {
            damage: 18.,
            cooldown: 1.2,
            amount: 1,
            area: 1.,
            speed: 0.,
            duration: 0.,
            pierce: 0,
            knock: 160.,
            extra: 0.,
        },
        WeaponId::ChainSpark => WStats {
            damage: 15.,
            cooldown: 1.5,
            amount: 1,
            area: 1.,
            speed: 0.,
            duration: 0.,
            pierce: 0,
            knock: 40.,
            extra: 3.,
        },
        WeaponId::LanternAura => WStats {
            damage: 7.,
            cooldown: 0.5,
            amount: 0,
            area: 1.,
            speed: 0.,
            duration: 0.,
            pierce: 0,
            knock: 20.,
            extra: 0.25,
        },
        WeaponId::ShellBoomerang => WStats {
            damage: 18.,
            cooldown: 1.6,
            amount: 1,
            area: 1.,
            speed: 430.,
            duration: 220.,
            pierce: 0,
            knock: 100.,
            extra: 0.,
        },
        WeaponId::StarSkipper => WStats {
            damage: 13.,
            cooldown: 3.0,
            amount: 1,
            area: 1.,
            speed: 330.,
            duration: 2.6,
            pierce: 0,
            knock: 80.,
            extra: 0.,
        },
    };
    match id {
        WeaponId::WispBolt => {
            s.amount += u32::from(at(2)) + u32::from(at(5)) + u32::from(at(7));
            if at(3) {
                s.damage *= 1.3;
            }
            if at(4) {
                s.pierce += 1;
            }
            if at(6) {
                s.cooldown *= 0.8;
            }
            if at(7) {
                s.damage *= 1.4;
            }
            if evolved {
                s.damage *= 1.6;
                s.pierce = 99;
                s.speed = 660.;
                s.cooldown = 0.7;
                s.area = 1.35;
            }
        }
        WeaponId::HaloOrbit => {
            s.amount += u32::from(at(2)) + u32::from(at(4)) + u32::from(at(6));
            if at(3) {
                s.speed *= 1.25;
                s.damage *= 1.2;
            }
            if at(4) {
                s.area *= 1.15;
            }
            if at(5) {
                s.duration += 1.;
            }
            if at(7) {
                s.damage *= 1.4;
            }
            if evolved {
                s.amount += 1;
                s.damage *= 1.5;
                s.area *= 1.2;
                s.duration = f32::INFINITY;
            }
        }
        WeaponId::BubbleMortar => {
            s.amount += u32::from(at(2)) + u32::from(at(5)) + u32::from(at(7));
            if at(3) {
                s.area *= 1.25;
            }
            if at(4) {
                s.damage *= 1.4;
            }
            if at(6) {
                s.cooldown *= 0.8;
            }
            if at(7) {
                s.area *= 1.15;
            }
            if evolved {
                s.damage *= 1.4;
                s.extra = 1.;
            }
        }
        WeaponId::TideLash => {
            if at(2) {
                s.extra = 1.;
            }
            if at(3) {
                s.damage *= 1.35;
            }
            if at(4) {
                s.area *= 1.2;
            }
            if at(5) {
                s.cooldown *= 0.8;
            }
            if at(6) {
                s.damage *= 1.35;
            }
            if at(7) {
                s.area *= 1.2;
                s.damage *= 1.3;
            }
            if evolved {
                s.extra = 2.;
                s.damage *= 1.3;
            }
        }
        WeaponId::ChainSpark => {
            s.extra += 2. * f32::from(u8::from(at(2)) + u8::from(at(5)));
            if at(3) {
                s.damage *= 1.3;
            }
            s.amount += u32::from(at(4)) + u32::from(at(7));
            if at(6) {
                s.cooldown *= 0.8;
            }
            if at(7) {
                s.damage *= 1.4;
            }
            if evolved {
                s.amount *= 2;
                s.damage *= 1.3;
                s.duration = 0.5;
            }
        }
        WeaponId::LanternAura => {
            if at(2) {
                s.area *= 1.2;
            }
            if at(3) {
                s.damage *= 1.3;
            }
            if at(4) {
                s.cooldown *= 0.8;
            }
            if at(5) {
                s.area *= 1.2;
            }
            if at(6) {
                s.damage *= 1.4;
            }
            if at(7) {
                s.area *= 1.25;
                s.extra = 0.4;
            }
            if evolved {
                s.area *= 1.35;
                s.damage *= 1.5;
            }
        }
        WeaponId::ShellBoomerang => {
            s.amount += u32::from(at(2)) + u32::from(at(5)) + u32::from(at(7));
            if at(3) {
                s.damage *= 1.3;
            }
            if at(4) {
                s.duration *= 1.25;
            }
            if at(6) {
                s.damage *= 1.4;
            }
            if at(7) {
                s.area *= 1.3;
            }
            if evolved {
                s.amount = s.amount.max(3);
                s.damage *= 1.5;
                s.area *= 1.2;
                s.extra = 1.;
            }
        }
        WeaponId::StarSkipper => {
            s.amount += u32::from(at(2)) + u32::from(at(5)) + u32::from(at(7));
            if at(3) {
                s.duration += 1.;
            }
            if at(4) {
                s.damage *= 1.35;
            }
            if at(6) {
                s.speed *= 1.2;
                s.area *= 1.2;
            }
            if at(7) {
                s.damage *= 1.4;
            }
            if evolved {
                s.extra = 1.;
                s.damage *= 1.2;
            }
        }
    }
    s
}

/// Orbit radius for Halo Orbit at a given area multiplier.
pub fn orbit_radius(area: f32) -> f32 {
    74. * area
}

/// Aura radius for Lantern Aura at a given area multiplier.
pub fn aura_radius(area: f32) -> f32 {
    62. * area
}

/// Lash reach/width for Tide Lash.
pub fn lash_size(area: f32) -> (f32, f32) {
    (128. * area, 50. * area)
}

impl Game {
    /// Up to `out.len()` nearest living foes within `radius`, nearest first.
    pub(crate) fn nearest_into(&self, at: Vec2, radius: f32, out: &mut [(u16, f32)]) -> usize {
        let mut n = 0;
        let r2 = radius * radius;
        self.grid.visit(at, radius, |j| {
            let Some(f) = self.foes.get(j) else {
                return;
            };
            if f.dead {
                return;
            }
            let d2 = (f.pos - at).length_squared();
            if d2 > r2 {
                return;
            }
            // Insertion into a tiny sorted buffer.
            let mut k = n;
            if k == out.len() {
                if out.is_empty() || d2 >= out[k - 1].1 {
                    return;
                }
                k -= 1;
            } else {
                n += 1;
            }
            while k > 0 && out[k - 1].1 > d2 {
                out[k] = out[k - 1];
                k -= 1;
            }
            out[k] = (j as u16, d2);
        });
        n
    }

    fn push_shot(&mut self, shot: Shot) {
        if self.shots.len() < MAX_SHOTS {
            self.shots.push(shot);
        }
    }

    pub(crate) fn update_weapons(&mut self, dt: f32) {
        for f in &mut self.foes {
            for cd in &mut f.hit_cd {
                *cd -= dt;
            }
        }
        for s in 0..self.weapons.len() {
            let Some(mut slot) = self.weapons[s] else {
                continue;
            };
            let stats = weapon_stats(slot.id, slot.level, slot.evolved);
            self.tick_weapon(&mut slot, stats, dt);
            self.weapons[s] = Some(slot);
        }
    }

    fn tick_weapon(&mut self, slot: &mut WeaponSlot, s: WStats, dt: f32) {
        let mods = self.mods;
        let amount = s.amount + if s.amount > 0 { mods.amount } else { 0 };
        let damage = s.damage;
        let area = s.area * mods.area;
        let cooldown = (s.cooldown * mods.haste).max(0.12);
        let player = self.player.pos;
        match slot.id {
            WeaponId::HaloOrbit => {
                slot.angle += s.speed * dt;
                if slot.active > 0. || s.duration.is_infinite() {
                    if s.duration.is_finite() {
                        slot.active -= dt;
                        if slot.active <= 0. {
                            slot.cooldown = cooldown;
                        }
                    }
                    let radius = orbit_radius(area);
                    for k in 0..amount {
                        let a = slot.angle + k as f32 * std::f32::consts::TAU / amount as f32;
                        let orb = player + vec2(a.cos(), a.sin()) * radius;
                        self.contact_area(
                            orb,
                            13. * area,
                            WeaponId::HaloOrbit,
                            damage,
                            s.knock,
                            0.45,
                        );
                    }
                } else {
                    slot.cooldown -= dt;
                    if slot.cooldown <= 0. {
                        slot.active = s.duration;
                        self.emit_minor(Event::Fire {
                            weapon: slot.id,
                            pos: player,
                        });
                    }
                }
                return;
            }
            WeaponId::LanternAura => {
                let radius = aura_radius(area);
                let mut scratch = std::mem::take(&mut self.scratch);
                scratch.clear();
                self.grid.visit(player, radius + 70., |j| {
                    if scratch.len() < scratch.capacity() {
                        scratch.push(j as u16);
                    }
                });
                for &j in &scratch {
                    let j = usize::from(j);
                    let Some(f) = self.foes.get_mut(j) else {
                        continue;
                    };
                    let d = f.pos - player;
                    let rr = radius + f.radius;
                    if f.dead || d.length_squared() > rr * rr {
                        continue;
                    }
                    f.slow = f.slow.max(s.extra);
                    if f.hit_cd[WeaponId::LanternAura as usize] <= 0. {
                        f.hit_cd[WeaponId::LanternAura as usize] = cooldown;
                        self.damage_foe(j, damage, WeaponId::LanternAura, d, s.knock);
                    }
                }
                self.scratch = scratch;
                return;
            }
            _ => {}
        }
        // Volley weapons: a cooldown followed by a short burst of `amount` shots.
        if slot.burst > 0 {
            slot.burst_timer -= dt;
            if slot.burst_timer <= 0. {
                slot.burst -= 1;
                slot.burst_timer = 0.07;
                let index = amount.saturating_sub(u32::from(slot.burst) + 1);
                self.fire_one(slot, s, damage, area, index, amount);
            }
            return;
        }
        slot.cooldown -= dt;
        if slot.cooldown > 0. {
            return;
        }
        slot.cooldown = cooldown;
        match slot.id {
            WeaponId::TideLash => {
                let (reach, width) = lash_size(area);
                let full = s.extra >= 2.;
                let mut near = [(0u16, 0f32); 1];
                let dir = if self.nearest_into(player, reach + 120., &mut near) > 0 {
                    (self.foes[usize::from(near[0].0)].pos - player).normalize_or_zero()
                } else {
                    self.player.facing
                };
                self.lash(dir, reach, width, damage, s.knock, full);
                if s.extra >= 1. && !full {
                    self.lash(-dir, reach, width, damage, s.knock, false);
                }
                slot.side = slot.side.wrapping_add(1);
                self.emit_minor(Event::Fire {
                    weapon: slot.id,
                    pos: player,
                });
            }
            WeaponId::ChainSpark => {
                let mut starts = [(0u16, 0f32); 8];
                let n = self.nearest_into(player, 360., &mut starts);
                if n == 0 {
                    slot.cooldown = 0.2;
                    return;
                }
                for k in 0..(amount as usize).min(8) {
                    let first = starts[k % n].0;
                    self.chain(first, s.extra as u32, damage, s.duration, slot.evolved);
                }
                self.emit_minor(Event::Fire {
                    weapon: slot.id,
                    pos: player,
                });
            }
            _ => {
                if amount == 0 {
                    return;
                }
                // Fire the first projectile now; the rest trail at 70 ms.
                self.fire_one(slot, s, damage, area, 0, amount);
                slot.burst = (amount - 1).min(16) as u8;
                slot.burst_timer = 0.07;
                self.emit_minor(Event::Fire {
                    weapon: slot.id,
                    pos: player,
                });
            }
        }
    }

    fn fire_one(
        &mut self,
        slot: &WeaponSlot,
        s: WStats,
        damage: f32,
        area: f32,
        index: u32,
        amount: u32,
    ) {
        let player = self.player.pos;
        match slot.id {
            WeaponId::WispBolt => {
                let mut targets = [(0u16, 0f32); 8];
                let n = self.nearest_into(player, 520., &mut targets);
                let (dir, homing) = if n > 0 {
                    let pick = targets[(index as usize) % n].0 as usize;
                    let f = self.foes[pick];
                    ((f.pos - player).normalize_or_zero(), f.uid)
                } else {
                    (self.player.facing, 0)
                };
                let spread = (index as f32 - (amount as f32 - 1.) * 0.5) * 0.12;
                let dir = rotate(dir, spread);
                let kind = if slot.evolved {
                    ShotKind::Lance
                } else {
                    ShotKind::Wisp
                };
                self.push_shot(Shot {
                    pos: player,
                    prev: player,
                    vel: dir * s.speed,
                    origin: player,
                    target: player,
                    kind,
                    weapon: slot.id,
                    damage,
                    radius: 7. * area,
                    life: s.duration,
                    age: 0.,
                    pierce: s.pierce,
                    knock: s.knock,
                    spin: 0.,
                    hits: [0; 6],
                    hit_n: 0,
                    homing,
                    returning: false,
                    dead: false,
                });
            }
            WeaponId::BubbleMortar => {
                let mut targets = [(0u16, 0f32); 8];
                let n = self.nearest_into(player, 420., &mut targets);
                let target = if n > 0 {
                    let pick = (self.rand() * n as f32) as usize % n;
                    self.foes[targets[pick].0 as usize].pos
                } else {
                    player + self.rand_dir() * self.rand_range(120., 260.)
                };
                self.push_shot(Shot {
                    pos: player,
                    prev: player,
                    vel: (target - player) / s.speed,
                    origin: player,
                    target,
                    kind: ShotKind::Bubble,
                    weapon: slot.id,
                    damage,
                    radius: 50. * area,
                    life: s.speed,
                    age: 0.,
                    pierce: 0,
                    knock: s.knock,
                    spin: if slot.evolved { 1. } else { 0. },
                    hits: [0; 6],
                    hit_n: 0,
                    homing: 0,
                    returning: false,
                    dead: false,
                });
            }
            WeaponId::ShellBoomerang => {
                let mut targets = [(0u16, 0f32); 4];
                let n = self.nearest_into(player, 400., &mut targets);
                let base = if n > 0 {
                    (self.foes[targets[0].0 as usize].pos - player).normalize_or_zero()
                } else {
                    self.player.facing
                };
                let spread = if amount > 1 {
                    std::f32::consts::TAU * index as f32 / amount as f32
                } else {
                    0.
                };
                let dir = rotate(base, spread);
                self.push_shot(Shot {
                    pos: player,
                    prev: player,
                    vel: dir * s.speed,
                    origin: player,
                    target: player + dir * s.duration,
                    kind: ShotKind::Shell,
                    weapon: slot.id,
                    damage,
                    radius: 11. * area,
                    life: 4.,
                    age: 0.,
                    pierce: 0,
                    knock: s.knock,
                    spin: if slot.evolved { 1. } else { 0. },
                    hits: [0; 6],
                    hit_n: 0,
                    homing: 0,
                    returning: false,
                    dead: false,
                });
            }
            WeaponId::StarSkipper => {
                let a = self.rand() * std::f32::consts::TAU;
                let dir = vec2(a.cos(), a.sin());
                self.push_shot(Shot {
                    pos: player,
                    prev: player,
                    vel: dir * s.speed,
                    origin: player,
                    target: player,
                    kind: ShotKind::Star,
                    weapon: slot.id,
                    damage,
                    radius: 12. * area,
                    life: s.duration,
                    age: 0.,
                    pierce: 0,
                    knock: s.knock,
                    spin: if slot.evolved { 1. } else { 0. },
                    hits: [0; 6],
                    hit_n: 0,
                    homing: 0,
                    returning: false,
                    dead: false,
                });
            }
            _ => {}
        }
    }

    fn lash(&mut self, dir: Vec2, reach: f32, width: f32, damage: f32, knock: f32, full: bool) {
        let player = self.player.pos;
        if self.lashes.len() < MAX_LASHES {
            self.lashes.push(Lash {
                dir,
                reach,
                width,
                age: 0.,
                full,
            });
        }
        let mut scratch = std::mem::take(&mut self.scratch);
        scratch.clear();
        self.grid.visit(player, reach + 70., |j| {
            if scratch.len() < scratch.capacity() {
                scratch.push(j as u16);
            }
        });
        let mut healed = 0;
        for &j in &scratch {
            let j = usize::from(j);
            let Some(f) = self.foes.get(j) else {
                continue;
            };
            if f.dead {
                continue;
            }
            let d = f.pos - player;
            let hit = if full {
                d.length_squared() < (reach + f.radius).powi(2)
            } else {
                let along = d.dot(dir);
                let side = d.dot(vec2(-dir.y, dir.x)).abs();
                along > -f.radius && along < reach + f.radius && side < width * 0.5 + f.radius
            };
            if hit {
                self.damage_foe(j, damage, WeaponId::TideLash, d, knock);
                if full && healed < 6 {
                    healed += 1;
                    self.heal(0.5);
                }
            }
        }
        self.scratch = scratch;
    }

    fn chain(&mut self, first: u16, jumps: u32, damage: f32, stun: f32, evolved: bool) {
        let mut visited = [0u32; 16];
        let mut current = usize::from(first);
        let mut from = self.player.pos;
        for step in 0..=jumps.min(15) as usize {
            let Some(f) = self.foes.get(current) else {
                break;
            };
            if f.dead {
                break;
            }
            let (pos, uid) = (f.pos, f.uid);
            visited[step] = uid;
            let count = step + 1;
            if self.zaps.len() < MAX_ZAPS {
                let seed = (self.rand() * 65535.) as u32;
                self.zaps.push(Zap {
                    a: from,
                    b: pos,
                    age: 0.,
                    seed,
                    evolved,
                });
            }
            self.damage_foe(current, damage, WeaponId::ChainSpark, pos - from, 40.);
            if stun > 0.
                && let Some(f) = self.foes.get_mut(current)
            {
                f.stun = f.stun.max(stun);
            }
            from = pos;
            // Next: nearest unvisited living foe within 150.
            let mut best = None;
            let mut best_d2 = 150. * 150.;
            self.grid.visit(pos, 150., |j| {
                let Some(f) = self.foes.get(j) else {
                    return;
                };
                if f.dead || visited[..count].contains(&f.uid) {
                    return;
                }
                let d2 = (f.pos - pos).length_squared();
                if d2 < best_d2 {
                    best_d2 = d2;
                    best = Some(j);
                }
            });
            match best {
                Some(j) => current = j,
                None => break,
            }
        }
    }

    /// Damage foes touching a circle, respecting each foe's per-weapon cooldown.
    pub(crate) fn contact_area(
        &mut self,
        at: Vec2,
        radius: f32,
        weapon: WeaponId,
        damage: f32,
        knock: f32,
        rehit: f32,
    ) -> u32 {
        let mut hits = 0;
        let mut scratch = std::mem::take(&mut self.scratch);
        scratch.clear();
        self.grid.visit(at, radius + 70., |j| {
            if scratch.len() < scratch.capacity() {
                scratch.push(j as u16);
            }
        });
        for &j in &scratch {
            let j = usize::from(j);
            let Some(f) = self.foes.get_mut(j) else {
                continue;
            };
            let d = f.pos - at;
            let rr = radius + f.radius;
            if f.dead || f.hit_cd[weapon as usize] > 0. || d.length_squared() > rr * rr {
                continue;
            }
            f.hit_cd[weapon as usize] = rehit;
            hits += 1;
            self.damage_foe(j, damage, weapon, d, knock);
        }
        self.scratch = scratch;
        hits
    }

    pub(crate) fn update_shots(&mut self, dt: f32) {
        let player = self.player.pos;
        for i in 0..self.shots.len() {
            let mut s = self.shots[i];
            if s.dead {
                continue;
            }
            s.age += dt;
            match s.kind {
                ShotKind::Wisp | ShotKind::Lance => {
                    if s.homing != 0 && s.kind == ShotKind::Wisp {
                        if let Some(f) = self.foes.iter().find(|f| f.uid == s.homing && !f.dead) {
                            let want = (f.pos - s.pos).normalize_or_zero();
                            let speed = s.vel.length();
                            let cur = s.vel / speed.max(1e-3);
                            let turn = (cur + (want - cur) * (7. * dt)).normalize_or_zero();
                            s.vel = turn * speed;
                        } else {
                            s.homing = 0;
                        }
                    }
                    s.pos += s.vel * dt;
                    self.shot_hits(&mut s, true);
                }
                ShotKind::Bubble => {
                    let t = (s.age / s.life).min(1.);
                    s.pos = s.origin + (s.target - s.origin) * t;
                    if s.age >= s.life {
                        // Pop!
                        let radius = s.radius;
                        let mut scratch = std::mem::take(&mut self.scratch);
                        scratch.clear();
                        self.grid.visit(s.target, radius + 70., |j| {
                            if scratch.len() < scratch.capacity() {
                                scratch.push(j as u16);
                            }
                        });
                        for &j in &scratch {
                            let j = usize::from(j);
                            let Some(f) = self.foes.get(j) else {
                                continue;
                            };
                            let d = f.pos - s.target;
                            if !f.dead && d.length_squared() < (radius + f.radius).powi(2) {
                                self.damage_foe(j, s.damage, s.weapon, d, s.knock);
                            }
                        }
                        self.scratch = scratch;
                        let evolved = s.spin > 0.;
                        self.emit(Event::Burst {
                            pos: s.target,
                            radius,
                            evolved,
                        });
                        if evolved && self.zones.len() < MAX_ZONES {
                            self.zones.push(Zone {
                                pos: s.target,
                                radius: radius * 1.25,
                                life: 2.4,
                                age: 0.,
                                damage: s.damage * 0.3,
                            });
                        }
                        s.dead = true;
                    }
                }
                ShotKind::Shell => {
                    let speed = s.vel.length().max(1.);
                    if !s.returning {
                        let out = (s.target - s.origin).normalize_or_zero();
                        let travelled = (s.pos - s.origin).dot(out);
                        let total = (s.target - s.origin).length();
                        // Ease out as it reaches the far point.
                        let slow = (1. - travelled / total.max(1.)).clamp(0.25, 1.);
                        let mut v = out * 430. * slow.sqrt();
                        if s.spin > 0. {
                            v = rotate(v, (s.age * 3.).sin() * 0.6);
                        }
                        s.vel = v;
                        if travelled >= total - 4. {
                            s.returning = true;
                            s.hit_n = 0;
                        }
                    } else {
                        let back = (player - s.pos).normalize_or_zero();
                        s.vel = back * (speed + 900. * dt).min(620.);
                        if (player - s.pos).length_squared() < 18. * 18. {
                            s.dead = true;
                        }
                    }
                    s.pos += s.vel * dt;
                    s.spin += dt;
                    let radius = s.radius;
                    self.contact_area(s.pos, radius, s.weapon, s.damage, s.knock, 0.3);
                }
                ShotKind::Star => {
                    s.pos += s.vel * dt;
                    let rel = s.pos - player;
                    let mut bounced = false;
                    if rel.x.abs() > SKIP_BOUNDS.x {
                        s.vel.x = -s.vel.x.abs() * rel.x.signum();
                        bounced = true;
                    }
                    if rel.y.abs() > SKIP_BOUNDS.y {
                        s.vel.y = -s.vel.y.abs() * rel.y.signum();
                        bounced = true;
                    }
                    if bounced && s.spin > 0. {
                        for k in 0..4 {
                            let a = k as f32 * std::f32::consts::FRAC_PI_2 + s.age * 2.;
                            let dir = vec2(a.cos(), a.sin());
                            let shard = Shot {
                                pos: s.pos,
                                prev: s.pos,
                                vel: dir * 260.,
                                kind: ShotKind::Stardust,
                                damage: s.damage * 0.5,
                                radius: 7.,
                                life: 0.6,
                                age: 0.,
                                spin: 0.,
                                ..s
                            };
                            self.push_shot(shard);
                        }
                    }
                    let radius = s.radius;
                    self.contact_area(s.pos, radius, s.weapon, s.damage, s.knock, 0.35);
                }
                ShotKind::Stardust => {
                    s.pos += s.vel * dt;
                    s.vel *= 1. - 2. * dt;
                    self.shot_hits(&mut s, false);
                }
            }
            self.shots[i] = s;
        }
    }

    /// Piercing projectile contact: each foe at most once per shot.
    fn shot_hits(&mut self, s: &mut Shot, consume: bool) {
        let mut hit: Option<usize> = None;
        let pos = s.pos;
        let radius = s.radius;
        let hits = s.hits;
        let n = usize::from(s.hit_n).min(6);
        self.grid.visit_until(pos, radius + 70., |j| {
            let Some(f) = self.foes.get(j) else {
                return true;
            };
            if f.dead || hits[..n].contains(&f.uid) {
                return true;
            }
            let rr = radius + f.radius;
            if (f.pos - pos).length_squared() < rr * rr {
                hit = Some(j);
                return false;
            }
            true
        });
        if let Some(j) = hit {
            let uid = self.foes[j].uid;
            s.hits[usize::from(s.hit_n) % 6] = uid;
            s.hit_n = s.hit_n.saturating_add(1);
            self.damage_foe(j, s.damage, s.weapon, s.vel, s.knock);
            if consume {
                if s.pierce <= 1 {
                    s.dead = true;
                } else if s.pierce < 99 {
                    s.pierce -= 1;
                }
            } else {
                s.dead = true;
            }
        }
    }
}

fn rotate(v: Vec2, angle: f32) -> Vec2 {
    let (sin, cos) = angle.sin_cos();
    vec2(v.x * cos - v.y * sin, v.x * sin + v.y * cos)
}
