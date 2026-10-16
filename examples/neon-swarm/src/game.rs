pub use ferravex_core::collision::segment_circle;
use ferravex_core::{Entity, World};
use ferravex_play::ability::{AbilityBank, AbilitySpec, ResourcePool};
use ferravex_play::action::{ActionPhase, ActionSpec, ActionTimeline};
use ferravex_play::particles::{
    Emitter, EmitterDesc, EmitterTransform, ParticleDesc, ParticleMotion, ParticlePool,
};
use ferravex_render::prelude::*;

pub const ARENA: Rect = Rect::new(72.0, 112.0, 1136.0, 604.0);
pub const COVER: [Rect; 4] = [
    Rect::new(350.0, 254.0, 96.0, 60.0),
    Rect::new(834.0, 254.0, 96.0, 60.0),
    Rect::new(350.0, 514.0, 96.0, 60.0),
    Rect::new(834.0, 514.0, 96.0, 60.0),
];
pub const PLAYER_RADIUS: f32 = 15.0;

#[derive(Clone, Copy)]
pub struct Position(pub Vec2);
#[derive(Clone, Copy)]
pub struct Enemy {
    pub ai_slot: usize,
    pub health: f32,
    pub radius: f32,
    pub speed: f32,
    pub heavy: bool,
    pub flash: f32,
    pub knockback: Vec2,
    pub attack: ActionTimeline,
}
pub fn attack_timeline(heavy: bool) -> ActionTimeline {
    let windup = if heavy { 0.38 } else { 0.16 };
    ActionTimeline::new(ActionSpec {
        windup,
        active: 0.08,
        recovery: if heavy { 1.0 } else { 0.65 },
        cancel_before: windup * 0.75,
        buffer_seconds: 0.1,
    })
    .unwrap()
}
#[derive(Clone, Copy)]
pub struct Bullet {
    pub velocity: Vec2,
    pub remaining: f32,
}
#[derive(Clone, Copy)]
pub struct Pickup {
    pub remaining: f32,
}
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Phase {
    Playing,
    Won,
    Lost,
}
#[derive(Clone, Copy, Default)]
pub struct Input {
    pub movement: Vec2,
    pub aim: Vec2,
    pub fire: bool,
    pub dash: bool,
}

pub struct Game {
    pub world: World,
    pub player: Vec2,
    pub facing: Vec2,
    tuning: ferravex_play::tuning::TuningProfile,
    velocity: Vec2,
    recoil: Vec2,
    pub health: i32,
    pub phase: Phase,
    pub wave: u32,
    pub kills: u32,
    pub score: ferravex_play::gameplay::combo::ScoreChain,
    pub shots: u32,
    pub progression: ferravex_play::gameplay::progression::Progression,
    weapon_upgrades: ferravex_play::gameplay::upgrades::Upgrades<1>,
    pub dash_cooldown: f32,
    pub dash_time: f32,
    pub hurt_time: f32,
    pub muzzle_time: f32,
    pub time: f32,
    pub wave_banner: f32,
    pub particles: ParticlePool,
    sparks: Emitter,
    ai: crate::ai::SwarmAi,
    abilities: AbilityBank<2>,
    resource: ResourcePool,
    director: ferravex_play::encounter::EncounterDirector,
    remaining: u32,
    spawn_index: u32,
    intermission: f32,
    dash_direction: Vec2,
}

impl Game {
    pub fn new() -> Self {
        Self::with_tuning(ferravex_play::tuning::TuningProfile::arena()).unwrap()
    }
    pub fn with_tuning(
        profile: ferravex_play::tuning::TuningProfile,
    ) -> Result<Self, ferravex_play::persistence::SaveError> {
        let mut tuning = ferravex_play::tuning::TuningProfile::arena();
        tuning.replace(profile)?;
        Ok(Self {
            world: World::new(),
            ai: crate::ai::SwarmAi::new(),
            player: vec2(640.0, 414.0),
            facing: vec2(1.0, 0.0),
            tuning,
            velocity: Vec2::ZERO,
            recoil: Vec2::ZERO,
            health: 100,
            phase: Phase::Playing,
            wave: 1,
            kills: 0,
            score: ferravex_play::gameplay::combo::ScoreChain::new(3.5, 3, 5).unwrap(),
            shots: 0,
            progression: ferravex_play::gameplay::progression::Progression::new(50, 30, 4).unwrap(),
            weapon_upgrades: ferravex_play::gameplay::upgrades::Upgrades::new([3]).unwrap(),
            dash_cooldown: 0.0,
            dash_time: 0.0,
            hurt_time: 0.0,
            muzzle_time: 0.0,
            time: 0.0,
            wave_banner: 2.7,
            particles: ParticlePool::new(512),
            sparks: Emitter::new(
                EmitterDesc {
                    particle: ParticleDesc {
                        lifetime: 0.45,
                        start_color: [1., 0.49, 0.34, 1.],
                        end_color: [1., 0.2, 0.08, 0.],
                        start_size: 3.8,
                        end_size: 1.,
                        ..Default::default()
                    },
                    motion: ParticleMotion {
                        drag: 4.,
                        angular_velocity: 5.,
                        ..Default::default()
                    },
                    velocity_jitter: [110., 110., 0.],
                    lifetime_jitter: 0.25,
                    size_jitter: 0.3,
                    rotation_jitter: std::f32::consts::PI,
                    rate: 0.,
                    ..Default::default()
                },
                0x4e454f4e,
            )
            .unwrap(),
            abilities: AbilityBank::new(
                1,
                &[
                    AbilitySpec {
                        charges: 1,
                        recharge_seconds: 0.12,
                        cost: 0.,
                    },
                    AbilitySpec {
                        charges: 1,
                        recharge_seconds: 1.7,
                        cost: 0.,
                    },
                ],
            )
            .unwrap(),
            resource: ResourcePool::new(0., 0., 0.).unwrap(),
            director: encounter(8, 1, 0.8),
            remaining: 8,
            spawn_index: 0,
            intermission: 1.6,
            dash_direction: Vec2::X,
        })
    }

    pub fn shot_damage(&self) -> f32 {
        24. + 4. * f32::from(self.weapon_upgrades.rank(0).unwrap())
    }

    pub fn enemy_count(&self) -> usize {
        self.world.query::<&Enemy>().iter().count()
    }

    pub fn remaining_enemies(&self) -> u32 {
        self.remaining + self.enemy_count() as u32
    }

    pub fn nearest_enemy(&self) -> Option<Vec2> {
        self.world
            .query::<(&Position, &Enemy)>()
            .iter()
            .map(|(p, _)| p.0)
            .min_by(|a, b| {
                a.distance_squared(self.player)
                    .total_cmp(&b.distance_squared(self.player))
            })
    }

    pub fn autoplay(&self) -> Input {
        let route = vec2(
            640.0 + (self.time * 0.5).cos() * 240.0,
            414.0 + (self.time * 0.5).sin() * 120.0,
        );
        let mut movement = (route - self.player).normalize_or_zero();
        for (p, _) in &mut self.world.query::<(&Position, &Enemy)>() {
            let delta = self.player - p.0;
            let distance = delta.length();
            if distance < 140.0 {
                movement += delta.normalize_or_zero() * (140.0 - distance) / 40.0;
            }
        }
        let target = self.nearest_enemy();
        Input {
            movement: movement.normalize_or_zero(),
            aim: target.unwrap_or(self.player + Vec2::X),
            fire: target.is_some(),
            dash: target.is_some_and(|p| p.distance(self.player) < 105.0),
        }
    }

    pub fn update(&mut self, input: Input, dt: f32) {
        self.particles.update(dt).expect("validated game delta");
        if self.phase != Phase::Playing {
            return;
        }
        self.time += dt;
        self.score.advance(dt).unwrap();
        self.abilities.advance(dt).unwrap();
        self.resource.advance(dt).unwrap();
        self.dash_cooldown = self.abilities.remaining(1).unwrap();
        self.dash_time = (self.dash_time - dt).max(0.0);
        self.hurt_time = (self.hurt_time - dt).max(0.0);
        self.muzzle_time = (self.muzzle_time - dt).max(0.0);
        self.wave_banner = (self.wave_banner - dt).max(0.0);
        let direction = (input.aim - self.player).normalize_or_zero();
        if direction.length_squared() > 0.0 {
            self.facing = direction;
        }
        if input.dash && self.abilities.activate(1, &mut self.resource, true).is_ok() {
            self.dash_direction = if input.movement.length_squared() > 0.01 {
                input.movement.normalize()
            } else {
                self.facing
            };
            self.dash_time = 0.18;
            self.dash_cooldown = self.abilities.remaining(1).unwrap();
        }
        let velocity = if self.dash_time > 0.0 {
            self.dash_direction * 700.0
        } else {
            let desired = input.movement.clamp_length_max(1.0) * self.tuning.motion.max_speed;
            self.velocity = self.velocity.lerp(
                desired,
                ferravex_play::motion::response(self.tuning.motion.response, dt),
            );
            self.velocity
        };
        self.recoil *= (-15. * dt).exp();
        let previous_player = self.player;
        self.player = move_body(self.player, (velocity + self.recoil) * dt, PLAYER_RADIUS);
        if self.dash_time == 0. && dt > 0. {
            self.velocity = ((self.player - previous_player) / dt - self.recoil)
                .clamp_length_max(self.tuning.motion.max_speed);
        }
        let shot = input.fire && self.abilities.activate(0, &mut self.resource, true).is_ok();
        if shot {
            self.world.spawn((
                Position(self.player + self.facing * 24.0),
                Bullet {
                    velocity: self.facing * 900.0,
                    remaining: 1.4,
                },
            ));
            self.muzzle_time = 0.055;
            self.shots += 1;
        }
        let alive = self.enemy_count();
        let pressure =
            ((1. - self.health as f32 / 100.) * 0.55 + alive as f32 / 12. * 0.55).clamp(0., 1.);
        self.director.advance(dt, pressure).unwrap();
        if self.remaining > 0 && self.director.admit(1, alive) {
            self.spawn_enemy();
        }
        self.ai.update(&self.world, self.player, shot, dt);
        for (mut p, mut enemy) in &mut self.world.query::<(&mut Position, &mut Enemy)>() {
            enemy.flash = (enemy.flash - dt).max(0.0);
            let planted = match enemy.attack.phase() {
                ActionPhase::Windup => 0.2,
                ActionPhase::Recovery => 0.6,
                _ => 1.,
            };
            let velocity = self.ai.velocity(enemy.ai_slot, p.0, enemy.speed * planted);
            enemy.knockback *= (-10. * dt).exp();
            p.0 = move_body(p.0, (velocity + enemy.knockback) * dt, enemy.radius);
            let within_range = p.0.distance(self.player) < enemy.radius + PLAYER_RADIUS;
            let attack = enemy.attack.step(dt, within_range).unwrap();
            if within_range && attack.activated() && self.hurt_time == 0.0 && self.dash_time == 0.0
            {
                let damage =
                    ((if enemy.heavy { 24. } else { 14. }) * self.tuning.difficulty).round() as i32;
                self.health = (self.health - damage.max(1)).max(0);
                self.hurt_time = 0.75;
                self.score.break_chain();
                self.recoil = (self.player - p.0).try_normalize().unwrap_or(-self.facing) * 150.;
            }
        }
        self.update_bullets(dt);
        let mut despawn = Vec::new();
        for (id, mut p, mut pickup) in
            &mut self.world.query::<(Entity, &mut Position, &mut Pickup)>()
        {
            pickup.remaining -= dt;
            let attraction = ferravex_play::gameplay::pickup::Magnet {
                radius: 130.,
                collect_radius: 28.,
                speed: 220.,
            }
            .step(p.0, self.player, dt)
            .unwrap();
            // Cover remains authoritative for attraction and collection.
            if !COVER
                .iter()
                .any(|wall| segment_rect(p.0, self.player, *wall).is_some())
            {
                p.0 = attraction.position;
            }
            if p.0.distance(self.player) <= 28.0 && self.health < 100 {
                self.health = (self.health + 22).min(100);
                despawn.push(id);
            } else if pickup.remaining <= 0.0 {
                despawn.push(id);
            }
        }
        for id in despawn {
            let _ = self.world.despawn(id);
        }
        if self.health == 0 {
            self.phase = Phase::Lost;
        } else if self.remaining == 0 && self.enemy_count() == 0 {
            self.intermission -= dt;
            if self.intermission <= 0.0 {
                if self.wave == 3 {
                    self.phase = Phase::Won;
                } else {
                    self.wave += 1;
                    self.remaining = 4 + self.wave * 5;
                    self.intermission = 1.6;
                    self.director = encounter(self.remaining, self.wave, 0.7);
                    self.wave_banner = 2.4;
                }
            }
        }
    }

    fn spawn_enemy(&mut self) {
        let index = self.spawn_index;
        self.spawn_index += 1;
        self.remaining -= 1;
        let along = 0.15 + ((index * 37 % 70) as f32) / 100.0;
        let p = match index % 4 {
            0 => vec2(ARENA.x + 26.0, ARENA.y + along * ARENA.h),
            1 => vec2(ARENA.x + along * ARENA.w, ARENA.y + 26.0),
            2 => vec2(ARENA.right() - 26.0, ARENA.y + along * ARENA.h),
            _ => vec2(ARENA.x + along * ARENA.w, ARENA.bottom() - 26.0),
        };
        let heavy = self.wave > 1 && index.is_multiple_of(4);
        let ai_slot = index as usize % 64;
        self.ai.reset_agent(ai_slot);
        self.world.spawn((
            Position(p),
            Enemy {
                knockback: Vec2::ZERO,
                ai_slot,
                health: if heavy { 72.0 } else { 24.0 },
                radius: if heavy { 22.0 } else { 14.0 },
                speed: if heavy {
                    68.0
                } else {
                    89.0 + self.wave as f32 * 8.0
                },
                heavy,
                attack: attack_timeline(heavy),
                flash: 0.16,
            },
        ));
    }

    fn update_bullets(&mut self, dt: f32) {
        let enemies: Vec<(Entity, Vec2, f32)> = self
            .world
            .query::<(Entity, &Position, &Enemy)>()
            .iter()
            .map(|(id, p, e)| (id, p.0, e.radius))
            .collect();
        let mut impacts = Vec::new();
        let mut despawn = Vec::new();
        for (id, mut pos, mut bullet) in
            &mut self.world.query::<(Entity, &mut Position, &mut Bullet)>()
        {
            let old = pos.0;
            pos.0 += bullet.velocity * dt;
            bullet.remaining -= dt;
            let blocked = COVER
                .iter()
                .filter_map(|r| segment_rect(old, pos.0, *r))
                .min_by(f32::total_cmp);
            let hit = enemies
                .iter()
                .filter_map(|(enemy, p, radius)| {
                    segment_circle(old, pos.0, *p, radius + 3.0).map(|t| (*enemy, t))
                })
                .min_by(|a, b| a.1.total_cmp(&b.1));
            if let Some((enemy, t)) = hit.filter(|(_, t)| blocked.is_none_or(|wall| *t <= wall)) {
                impacts.push((enemy, old.lerp(pos.0, t)));
                despawn.push(id);
            } else if blocked.is_some() || bullet.remaining <= 0.0 || !ARENA.contains(pos.0) {
                despawn.push(id);
            }
        }
        for id in despawn {
            let _ = self.world.despawn(id);
        }
        let mut dead = Vec::new();
        let damage = self.shot_damage();
        for (id, point) in impacts {
            let target_position = self.world.get::<&Position>(id).ok().map(|p| p.0);
            if let Ok(mut enemy) = self.world.get::<&mut Enemy>(id) {
                if enemy.health <= 0.0 {
                    continue;
                }
                enemy.health -= damage;
                enemy.attack.cancel(false);
                enemy.flash = 0.09;
                if let Some(position) = target_position {
                    let direction = (position - self.player).normalize_or_zero();
                    enemy.knockback = (enemy.knockback
                        + direction * if enemy.heavy { 75. } else { 180. })
                    .clamp_length_max(240.);
                }
                if enemy.health <= 0.0 {
                    dead.push((id, point));
                }
            }
        }
        for (id, point) in dead {
            let _ = self.world.despawn(id);
            self.kills += 1;
            self.score.award(100);
            for _ in 0..self.progression.award(10) {
                self.weapon_upgrades.apply(0).unwrap();
            }
            if self.kills.is_multiple_of(5) {
                self.world
                    .spawn((Position(point), Pickup { remaining: 16.0 }));
            }
            self.sparks
                .burst(
                    9,
                    EmitterTransform::at([point.x, point.y, 0.]),
                    &mut self.particles,
                )
                .expect("finite hit position");
        }
    }
}

pub fn move_body(mut p: Vec2, delta: Vec2, radius: f32) -> Vec2 {
    // Axis resolution allows sliding along cover. Fixed 120 Hz limits movement to <6 px.
    for axis in 0..2 {
        p[axis] += delta[axis];
        for obstacle in COVER {
            let nearest = vec2(
                p.x.clamp(obstacle.x, obstacle.right()),
                p.y.clamp(obstacle.y, obstacle.bottom()),
            );
            let offset = p - nearest;
            if offset.length_squared() < radius * radius {
                if offset.length_squared() > 0.0001 {
                    p += offset.normalize() * (radius - offset.length());
                } else if delta[axis] != 0.0 {
                    p[axis] = if delta[axis] > 0.0 {
                        obstacle.point()[axis] - radius
                    } else {
                        (obstacle.point() + obstacle.size())[axis] + radius
                    };
                }
            }
        }
    }
    p.x = p.x.clamp(ARENA.x + radius, ARENA.right() - radius);
    p.y = p.y.clamp(ARENA.y + radius, ARENA.bottom() - radius);
    p
}

fn segment_rect(start: Vec2, end: Vec2, r: Rect) -> Option<f32> {
    ferravex_core::collision::Aabb2::from_min_size(r.point(), r.size()).cast_segment(start, end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surviving_hits_push_heavy_enemies_and_contact_pushes_the_player() {
        let mut game = Game::new();
        game.remaining = 0;
        let enemy = game.world.spawn((
            Position(game.player + Vec2::X * 80.),
            Enemy {
                ai_slot: 0,
                health: 72.,
                radius: 22.,
                speed: 0.,
                heavy: true,
                attack: attack_timeline(true),
                flash: 0.,
                knockback: Vec2::ZERO,
            },
        ));
        game.world.spawn((
            Position(game.player),
            Bullet {
                velocity: Vec2::X * 900.,
                remaining: 1.,
            },
        ));
        game.update_bullets(0.1);
        let actor = game.world.get::<&Enemy>(enemy).unwrap();
        assert_eq!(actor.health, 48.);
        assert!(actor.knockback.x > 50.);
        drop(actor);
        game.world.get::<&mut Position>(enemy).unwrap().0 = game.player;
        game.world.get::<&mut Enemy>(enemy).unwrap().knockback = Vec2::ZERO;
        game.update(Input::default(), 1. / 120.);
        assert_eq!(game.health, 100, "contact must first telegraph its attack");
        assert_eq!(
            game.world.get::<&Enemy>(enemy).unwrap().attack.phase(),
            ActionPhase::Windup
        );
        for _ in 0..60 {
            game.update(Input::default(), 1. / 120.);
            if game.health < 100 {
                break;
            }
        }
        assert_eq!(game.health, 76);
        assert!(game.recoil.length() > 100.);
    }

    #[test]
    fn swept_shots_hit_between_frames_and_cover_stops_them() {
        assert_eq!(
            segment_circle(vec2(0.0, 0.0), vec2(100.0, 0.0), vec2(50.0, 0.0), 10.0),
            Some(0.4)
        );
        assert_eq!(
            segment_circle(Vec2::ZERO, vec2(100.0, 0.0), vec2(50.0, 20.0), 10.0),
            None
        );
        let mut game = Game::new();
        game.remaining = 0;
        game.world.spawn((
            Position(vec2(500.0, 284.0)),
            Enemy {
                ai_slot: 0,
                knockback: Vec2::ZERO,
                health: 24.0,
                radius: 14.0,
                speed: 0.0,
                heavy: false,
                attack: attack_timeline(false),
                flash: 0.0,
            },
        ));
        game.world.spawn((
            Position(vec2(320.0, 284.0)),
            Bullet {
                velocity: vec2(900.0, 0.0),
                remaining: 1.0,
            },
        ));
        game.update_bullets(0.3);
        assert_eq!(game.enemy_count(), 1);
        assert_eq!(game.world.query::<&Bullet>().iter().count(), 0);
    }
    #[test]
    fn dash_avoids_contact_and_cover_blocks_movement() {
        let p = move_body(vec2(330.0, 284.0), vec2(20.0, 0.0), PLAYER_RADIUS);
        assert!(p.x <= 335.01);
        let mut game = Game::new();
        game.world.spawn((
            Position(game.player),
            Enemy {
                ai_slot: 0,
                knockback: Vec2::ZERO,
                health: 24.0,
                radius: 14.0,
                speed: 0.0,
                heavy: false,
                attack: attack_timeline(false),
                flash: 0.0,
            },
        ));
        game.update(
            Input {
                dash: true,
                aim: game.player + Vec2::X,
                ..Input::default()
            },
            1.0 / 120.0,
        );
        assert_eq!(game.health, 100);
        assert!(game.dash_cooldown > 1.0);
    }
    #[test]
    fn idle_player_loses_to_ordinary_spawned_enemies() {
        let mut game = Game::new();
        for _ in 0..120 * 120 {
            game.update(Input::default(), 1. / 120.);
            if game.phase != Phase::Playing {
                break;
            }
        }
        assert_eq!(
            game.phase,
            Phase::Lost,
            "health {}, player {:?}, enemies {:?}",
            game.health,
            game.player,
            game.world
                .query::<(&Position, &Enemy)>()
                .iter()
                .map(|(p, e)| (p.0, e.attack.phase()))
                .collect::<Vec<_>>()
        );
    }
    #[test]
    fn waves_progress_to_victory_using_normal_inputs() {
        let mut game = Game::new();
        for _ in 0..120 * 100 {
            let input = game.autoplay();
            game.update(input, 1.0 / 120.0);
            if game.phase != Phase::Playing {
                break;
            }
        }
        assert_eq!(
            game.phase,
            Phase::Won,
            "wave {}, hp {}, remaining {}",
            game.wave,
            game.health,
            game.remaining_enemies()
        );
        assert_eq!(game.kills, 41);
        assert!(game.shots > game.kills);
        let reset = Game::new();
        assert_eq!((reset.health, reset.wave, reset.kills), (100, 1, 0));
        assert_eq!(reset.world.len(), 0);
    }
    #[test]
    fn pickups_heal_and_zero_health_loses() {
        let mut game = Game::new();
        game.health = 40;
        game.world
            .spawn((Position(game.player), Pickup { remaining: 5.0 }));
        game.update(Input::default(), 1.0 / 120.0);
        assert_eq!(game.health, 62);
        assert_eq!(game.world.query::<&Pickup>().iter().count(), 0);
        game.health = 0;
        game.update(Input::default(), 1.0 / 120.0);
        assert_eq!(game.phase, Phase::Lost);
    }
}

#[cfg(test)]
mod telegraph_tests {
    use super::*;
    #[test]
    fn stepping_out_of_a_real_contact_windup_avoids_damage() {
        let mut game = Game::new();
        game.remaining = 0;
        game.world.spawn((
            Position(game.player),
            Enemy {
                ai_slot: 0,
                health: 72.,
                radius: 22.,
                speed: 0.,
                heavy: true,
                attack: attack_timeline(true),
                flash: 0.,
                knockback: Vec2::ZERO,
            },
        ));
        game.update(Input::default(), 1. / 120.);
        assert_eq!(game.health, 100);
        game.player += Vec2::X * 100.;
        for _ in 0..60 {
            game.update(Input::default(), 1. / 120.);
        }
        assert_eq!(
            game.health, 100,
            "a missed attack must not damage a departed target"
        );
    }
}

fn encounter(credits: u32, wave: u32, delay: f32) -> ferravex_play::encounter::EncounterDirector {
    use ferravex_play::encounter::*;
    EncounterDirector::new(EncounterConfig {
        credits,
        max_alive: 12,
        interval: 0.68 - wave as f32 * 0.06,
        first_delay: delay,
        recovery_seconds: 2.,
    })
    .unwrap()
}

#[cfg(test)]
mod score_loop_tests {
    use super::*;
    #[test]
    fn confirmed_kills_build_score_and_restart_clears_the_run() {
        let mut game = Game::new();
        for _ in 0..120 * 100 {
            game.update(game.autoplay(), 1. / 120.);
            if game.phase != Phase::Playing {
                break;
            }
        }
        assert_eq!(game.phase, Phase::Won);
        assert!(game.score.total() >= u64::from(game.kills) * 100);
        assert!(game.score.best() > 3);
        assert_eq!(Game::new().score.total(), 0);
    }
}

#[cfg(test)]
mod progression_loop_tests {
    use super::*;
    #[test]
    fn ordinary_kills_earn_capped_damage_upgrades() {
        let mut game = Game::new();
        assert_eq!(game.shot_damage(), 24.);
        for _ in 0..120 * 100 {
            game.update(game.autoplay(), 1. / 120.);
            if game.phase != Phase::Playing {
                break;
            }
        }
        assert_eq!(game.phase, Phase::Won);
        assert_eq!(game.progression.level(), 4);
        assert_eq!(game.shot_damage(), 36.);
        assert_eq!(Game::new().progression.level(), 1);
    }
}

#[cfg(test)]
mod magnet_loop_tests {
    use super::*;
    #[test]
    fn injured_players_attract_health_but_cover_blocks_the_magnet() {
        let mut g = Game::new();
        g.health = 50;
        g.remaining = 0;
        let pickup = g.world.spawn((
            Position(g.player + Vec2::X * 100.),
            Pickup { remaining: 5. },
        ));
        for _ in 0..120 {
            g.update(Input::default(), 1. / 120.);
        }
        assert!(g.world.get::<&Pickup>(pickup).is_err());
        assert_eq!(g.health, 72);
        let mut g = Game::new();
        g.health = 50;
        g.remaining = 0;
        g.player = vec2(330., 284.);
        let position = vec2(460., 284.);
        let pickup = g
            .world
            .spawn((Position(position), Pickup { remaining: 5. }));
        for _ in 0..60 {
            g.update(Input::default(), 1. / 120.);
        }
        assert_eq!(g.world.get::<&Position>(pickup).unwrap().0, position);
        assert_eq!(g.health, 50);
    }
}
