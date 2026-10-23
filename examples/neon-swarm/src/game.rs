pub use ferravex_core::collision::segment_circle;
use ferravex_core::{Entity, World};
use ferravex_play::ability::{AbilityAvailability, AbilityBank, AbilitySpec, ResourcePool};
use ferravex_play::action::{ActionPhase, ActionSpec, ActionTimeline};
use ferravex_play::encounter::AdmissionPreview;
use ferravex_play::feedback::{FeedbackEvent, FeedbackKind};
use ferravex_render::prelude::*;

pub const ARENA: Rect = Rect::new(72.0, 112.0, 1136.0, 604.0);
pub const COVER: [Rect; 4] = [
    Rect::new(350.0, 254.0, 96.0, 60.0),
    Rect::new(834.0, 254.0, 96.0, 60.0),
    Rect::new(350.0, 514.0, 96.0, 60.0),
    Rect::new(834.0, 514.0, 96.0, 60.0),
];
pub const PLAYER_RADIUS: f32 = 15.0;
const SPAWN_DISTANCE: f32 = 260.;
const ENTRY_POINTS: [Vec2; 8] = [
    vec2(100., 150.),
    vec2(640., 150.),
    vec2(1180., 150.),
    vec2(100., 414.),
    vec2(1180., 414.),
    vec2(100., 678.),
    vec2(640., 678.),
    vec2(1180., 678.),
];

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
    pub stagger: ferravex_play::gameplay::stagger::Stagger,
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
    /// A consumed input edge, including a press/release between simulation ticks.
    pub dash_pressed: bool,
}

#[derive(Clone, Copy)]
pub struct HitMarker {
    pub position: Vec2,
    pub remaining: f32,
    pub lethal: bool,
}

pub struct Game {
    pub world: World,
    pub player: Vec2,
    pub facing: Vec2,
    tuning: ferravex_play::tuning::TuningProfile,
    velocity: Vec2,
    recoil: Vec2,
    pub health: i32,
    pub shield: ferravex_play::gameplay::shield::Shield,
    pub phase: Phase,
    pub wave: u32,
    pub kills: u32,
    pub score: ferravex_play::gameplay::combo::ScoreChain,
    pub shots: u32,
    pub magazine: ferravex_play::gameplay::magazine::Magazine,
    pub progression: ferravex_play::gameplay::progression::Progression,
    weapon_upgrades: ferravex_play::gameplay::upgrades::Upgrades<1>,
    pub dash_cooldown: f32,
    pub dash_time: f32,
    pub hurt_time: f32,
    pub hit_marker: Option<HitMarker>,
    pub time: f32,
    pub wave_banner: f32,
    ai: crate::ai::SwarmAi,
    abilities: AbilityBank<2>,
    resource: ResourcePool,
    director: ferravex_play::encounter::EncounterDirector,
    remaining: u32,
    spawn_index: u32,
    spawns: ferravex_play::gameplay::spawn::SpawnSelector,
    intermission: f32,
    dash_direction: Vec2,
    dash_buffer: ferravex_play::gameplay::buffer::CommandBuffer<()>,
    dash_held: bool,
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
            shield: ferravex_play::gameplay::shield::Shield::new(20., 8., 4.).unwrap(),
            phase: Phase::Playing,
            wave: 1,
            kills: 0,
            score: ferravex_play::gameplay::combo::ScoreChain::new(3.5, 3, 5).unwrap(),
            shots: 0,
            magazine: ferravex_play::gameplay::magazine::Magazine::new(12, 12, 0.65).unwrap(),
            progression: ferravex_play::gameplay::progression::Progression::new(50, 30, 4).unwrap(),
            weapon_upgrades: ferravex_play::gameplay::upgrades::Upgrades::new([3]).unwrap(),
            dash_cooldown: 0.0,
            dash_time: 0.0,
            hurt_time: 0.0,
            hit_marker: None,
            time: 0.0,
            wave_banner: 2.7,
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
            spawns: ferravex_play::gameplay::spawn::SpawnSelector::new(SPAWN_DISTANCE).unwrap(),
            intermission: 1.6,
            dash_direction: Vec2::X,
            dash_buffer: ferravex_play::gameplay::buffer::CommandBuffer::new(0.15).unwrap(),
            dash_held: false,
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

    /// The existing intermission clock, visible only after all wave credits and
    /// live enemies are exhausted. A quiet gap between spawns is not a clear.
    pub fn wave_clear_remaining(&self) -> Option<f32> {
        (self.phase == Phase::Playing && self.remaining_enemies() == 0)
            .then_some(self.intermission.max(0.))
    }

    /// Discard queued commands when the host pauses or loses gameplay focus.
    pub fn clear_input_buffer(&mut self) {
        self.dash_buffer.clear();
        self.dash_held = false;
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
            ..Input::default()
        }
    }

    #[cfg(test)]
    pub fn update(&mut self, input: Input, dt: f32) {
        self.update_with_feedback(input, dt, |_| {});
    }

    /// Emit presentation events at accepted gameplay actions, including shield
    /// damage and nonlethal bullet impacts. Presentation never owns game rules.
    pub fn update_with_feedback(
        &mut self,
        input: Input,
        dt: f32,
        mut emit: impl FnMut(FeedbackEvent),
    ) {
        if self.phase != Phase::Playing {
            return;
        }
        if let Some(marker) = &mut self.hit_marker {
            marker.remaining -= dt;
            if marker.remaining <= 0. {
                self.hit_marker = None;
            }
        }
        self.time += dt;
        self.score.advance(dt).unwrap();
        self.shield.advance(dt).unwrap();
        self.magazine.advance(dt).unwrap();
        if self.magazine.loaded() == 0 && !self.magazine.reloading() {
            // Energy cells replenish automatically; reload still costs quiet time.
            if self.magazine.reserve() == 0 {
                self.magazine.add_reserve(12).unwrap();
            }
            self.magazine.reload();
        }
        self.abilities.advance(dt).unwrap();
        self.resource.advance(dt).unwrap();
        self.dash_cooldown = self.abilities.remaining(1).unwrap();
        self.dash_time = (self.dash_time - dt).max(0.0);
        self.hurt_time = (self.hurt_time - dt).max(0.0);
        self.wave_banner = (self.wave_banner - dt).max(0.0);
        let direction = (input.aim - self.player).normalize_or_zero();
        if direction.length_squared() > 0.0 {
            self.facing = direction;
        }
        self.dash_buffer.advance(dt).unwrap();
        if input.dash_pressed || input.dash && !self.dash_held {
            self.dash_buffer.push(());
        }
        self.dash_held = input.dash;
        let dash_ready = self.abilities.charges(1) == Some(1);
        if self.dash_buffer.take_if(dash_ready).is_some()
            && self.abilities.activate(1, &mut self.resource, true).is_ok()
        {
            self.dash_direction = if input.movement.length_squared() > 0.01 {
                input.movement.normalize()
            } else {
                self.facing
            };
            self.dash_time = 0.18;
            self.dash_cooldown = self.abilities.remaining(1).unwrap();
            emit(FeedbackEvent {
                kind: FeedbackKind::Dash,
                intensity: 1.,
                position: self.player.extend(0.),
            });
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
        let shot = input.fire
            && self.magazine.loaded() > 0
            && !self.magazine.reloading()
            && self.abilities.activate(0, &mut self.resource, true).is_ok()
            && self.magazine.fire();
        if shot {
            self.world.spawn((
                Position(self.player + self.facing * 24.0),
                Bullet {
                    velocity: self.facing * 900.0,
                    remaining: 1.4,
                },
            ));
            self.shots += 1;
            emit(FeedbackEvent {
                kind: FeedbackKind::Fire,
                intensity: 1.,
                position: self.player.extend(0.),
            });
        }
        let alive = self.enemy_count();
        let pressure =
            ((1. - self.health as f32 / 100.) * 0.55 + alive as f32 / 12. * 0.55).clamp(0., 1.);
        self.director.advance(dt, pressure).unwrap();
        if self.remaining > 0
            && let Some(position) = self
                .spawn_location()
                .filter(|_| self.director.admit(1, alive))
        {
            self.spawn_enemy(position);
        }
        self.ai.update(&self.world, self.player, shot, dt);
        for (mut p, mut enemy) in &mut self.world.query::<(&mut Position, &mut Enemy)>() {
            enemy.flash = (enemy.flash - dt).max(0.0);
            enemy.stagger.advance(dt).unwrap();
            let planted = if enemy.stagger.active() {
                0.
            } else {
                match enemy.attack.phase() {
                    ActionPhase::Windup => 0.2,
                    ActionPhase::Recovery => 0.6,
                    _ => 1.,
                }
            };
            let velocity = self.ai.velocity(enemy.ai_slot, p.0, enemy.speed * planted);
            enemy.knockback *= (-10. * dt).exp();
            p.0 = move_body(p.0, (velocity + enemy.knockback) * dt, enemy.radius);
            let within_range = p.0.distance(self.player) < enemy.radius + PLAYER_RADIUS;
            let can_attack = within_range && !enemy.stagger.active();
            let attack = enemy.attack.step(dt, can_attack).unwrap();
            if within_range && attack.activated() && self.hurt_time == 0.0 && self.dash_time == 0.0
            {
                let damage =
                    ((if enemy.heavy { 24. } else { 14. }) * self.tuning.difficulty).round() as i32;
                let unabsorbed = self.shield.absorb(damage.max(1) as f32).unwrap().ceil() as i32;
                self.health = (self.health - unabsorbed).max(0);
                self.hurt_time = 0.75;
                self.score.break_chain();
                self.recoil = (self.player - p.0).try_normalize().unwrap_or(-self.facing) * 150.;
                emit(FeedbackEvent {
                    kind: FeedbackKind::Hurt,
                    intensity: if unabsorbed == 0 { 0.55 } else { 1. },
                    position: self.player.extend(0.),
                });
            }
        }
        self.update_bullets(dt, &mut emit);
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
            if p.0.distance(self.player) <= 28.0 && (1..100).contains(&self.health) {
                self.health = (self.health + 22).min(100);
                despawn.push(id);
                emit(FeedbackEvent {
                    kind: FeedbackKind::Pickup,
                    intensity: 1.,
                    position: p.0.extend(0.),
                });
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

    /// Advisory inspection only: a moving player, occupancy or pressure can
    /// change the next admission. Copy the selector so HUD reads cannot rotate
    /// tie breaks, spend credits or reserve a position.
    pub fn spawn_preview(&self) -> (AdmissionPreview, Option<Vec2>) {
        let mut selector = self.spawns;
        (
            self.director.preview(1, self.enemy_count()),
            self.choose_spawn(&mut selector),
        )
    }

    /// Eligible zones, without predicting which tie-break will win on a later
    /// tick. Showing the zones avoids a rapidly alternating single warning when
    /// the unchanged simulation rotates between equally distant candidates.
    pub fn entry_candidates(&self) -> impl Iterator<Item = Vec2> + '_ {
        ENTRY_POINTS.into_iter().filter(|p| self.entry_safe(*p))
    }

    fn entry_safe(&self, position: Vec2) -> bool {
        position.distance_squared(self.player) >= SPAWN_DISTANCE * SPAWN_DISTANCE
            && self
                .world
                .query::<(&Position, &Enemy)>()
                .iter()
                .all(|(p, e)| p.0.distance(position) > e.radius + 30.)
    }

    /// The same charge/resource admission used by an actual dash activation.
    pub fn dash_availability(&self) -> AbilityAvailability {
        self.abilities
            .availability(1, &self.resource, self.phase == Phase::Playing)
            .expect("registered dash ability")
    }

    fn spawn_location(&mut self) -> Option<Vec2> {
        let mut selector = self.spawns;
        let position = self.choose_spawn(&mut selector);
        self.spawns = selector;
        position
    }

    fn choose_spawn(
        &self,
        selector: &mut ferravex_play::gameplay::spawn::SpawnSelector,
    ) -> Option<Vec2> {
        let selected = selector
            .choose(&ENTRY_POINTS, self.player, |i| {
                self.entry_safe(ENTRY_POINTS[i])
            })
            .unwrap();
        selected.map(|i| ENTRY_POINTS[i])
    }

    fn spawn_enemy(&mut self, p: Vec2) {
        let index = self.spawn_index;
        self.spawn_index += 1;
        self.remaining -= 1;
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
                stagger: ferravex_play::gameplay::stagger::Stagger::new(40., 10., 0.8).unwrap(),
                flash: 0.16,
            },
        ));
    }

    fn update_bullets(&mut self, dt: f32, emit: &mut impl FnMut(FeedbackEvent)) {
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
                if enemy.heavy {
                    if enemy.stagger.hit(24.).unwrap() {
                        enemy.attack.cancel(true);
                    }
                } else {
                    enemy.attack.cancel(false);
                }
                enemy.flash = 0.09;
                self.hit_marker = Some(HitMarker {
                    position: point,
                    remaining: 0.16,
                    lethal: enemy.health <= 0.,
                });
                emit(FeedbackEvent {
                    kind: FeedbackKind::Hit,
                    intensity: if enemy.health <= 0. { 1. } else { 0.65 },
                    position: point.extend(0.),
                });
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
            if let Ok(enemy) = self.world.get::<&Enemy>(id) {
                self.ai.reset_agent(enemy.ai_slot);
            }
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
                stagger: ferravex_play::gameplay::stagger::Stagger::new(40., 10., 0.8).unwrap(),
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
        game.update_bullets(0.1, &mut |_| {});
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
        assert_eq!(game.health, 96);
        assert_eq!(game.shield.value(), 0.);
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
                stagger: ferravex_play::gameplay::stagger::Stagger::new(40., 10., 0.8).unwrap(),
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
        game.update_bullets(0.3, &mut |_| panic!("cover rejected the hit"));
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
                stagger: ferravex_play::gameplay::stagger::Stagger::new(40., 10., 0.8).unwrap(),
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
                stagger: ferravex_play::gameplay::stagger::Stagger::new(40., 10., 0.8).unwrap(),
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

#[cfg(test)]
mod safe_spawn_tests {
    use super::*;

    #[test]
    fn repeated_spawn_previews_do_not_rotate_or_spend_admissions() {
        let mut observed = Game::new();
        let mut baseline = Game::new();
        assert_eq!(
            observed.spawn_preview().0.next_available_in(),
            Some(f64::from(0.8_f32))
        );
        for _ in 0..360 {
            let before = observed.spawn_preview();
            for _ in 0..5 {
                assert_eq!(observed.spawn_preview(), before);
            }
            observed.update(Input::default(), 1. / 120.);
            baseline.update(Input::default(), 1. / 120.);
            assert_eq!(observed.director.credits(), baseline.director.credits());
            assert_eq!(observed.remaining, baseline.remaining);
            let positions = |game: &Game| {
                game.world
                    .query::<(&Position, &Enemy)>()
                    .iter()
                    .map(|(p, e)| (p.0, e.health, e.attack.phase()))
                    .collect::<Vec<_>>()
            };
            assert_eq!(positions(&observed), positions(&baseline));
        }
    }

    #[test]
    fn unsafe_entry_and_cap_are_not_misreported_as_guaranteed_spawns() {
        let mut game = Game::new();
        // Block every perimeter candidate, independently of the director timer.
        for _ in 0..8 {
            let (_, position) = game.spawn_preview();
            game.spawn_enemy(position.unwrap());
        }
        assert!(game.spawn_preview().1.is_none());
        game.director = encounter(5, 1, 0.);
        let preview = game.spawn_preview();
        assert!(preview.0.is_ready());
        assert!(
            preview.1.is_none(),
            "admission readiness alone is not a safe position"
        );
        assert_eq!(game.director.credits(), 5);
        game.director = ferravex_play::encounter::EncounterDirector::new(
            ferravex_play::encounter::EncounterConfig {
                credits: 5,
                max_alive: 8,
                interval: 0.5,
                first_delay: 0.,
                recovery_seconds: 1.,
            },
        )
        .unwrap();
        assert_eq!(game.spawn_preview().0.next_available_in(), None);
        assert!(matches!(
            game.spawn_preview().0,
            AdmissionPreview::AliveLimit { alive: 8, limit: 8 }
        ));
    }

    #[test]
    fn dash_hint_reads_the_actual_admission_without_charging_or_spending() {
        let mut game = Game::new();
        assert!(game.dash_availability().check().is_ok());
        assert_eq!(game.dash_availability().charges, 1);
        game.update(
            Input {
                dash_pressed: true,
                ..Default::default()
            },
            1. / 120.,
        );
        let denied = game.dash_availability();
        assert!(denied.check().is_err());
        assert_eq!(denied.cooldown_remaining, game.dash_cooldown);
        for _ in 0..50 {
            assert_eq!(game.dash_availability(), denied);
        }
        for _ in 0..205 {
            game.update(Input::default(), 1. / 120.);
        }
        assert!(game.dash_availability().check().is_ok());
        game.phase = Phase::Lost;
        assert!(game.dash_availability().check().is_err());
    }

    #[test]
    fn actual_admitted_enemies_spawn_at_a_safe_distance_without_losing_credits() {
        let mut g = Game::new();
        g.player = vec2(100., 150.);
        for _ in 0..120 {
            g.update(Input::default(), 1. / 120.);
            if g.enemy_count() > 0 {
                break;
            }
        }
        assert_eq!(g.enemy_count(), 1);
        assert_eq!(g.remaining, 7);
        let p = g
            .world
            .query::<(&Position, &Enemy)>()
            .iter()
            .next()
            .unwrap()
            .0
            .0;
        assert!(p.distance(g.player) > 250.);
    }
}

#[cfg(test)]
mod stagger_loop_tests {
    use super::*;
    #[test]
    fn concentrated_shots_stagger_heavy_enemies_and_recovery_reenables_them() {
        let mut g = Game::new();
        g.wave = 2;
        g.remaining = 1;
        g.spawn_index = 0;
        let point = g.player + Vec2::X * 80.;
        g.spawn_enemy(point);
        for _ in 0..2 {
            g.world.spawn((
                Position(g.player),
                Bullet {
                    velocity: Vec2::X * 900.,
                    remaining: 1.,
                },
            ));
            g.update_bullets(0.1, &mut |_| {});
        }
        let id = g.world.query::<(Entity, &Enemy)>().iter().next().unwrap().0;
        let e = g.world.get::<&Enemy>(id).unwrap();
        assert!(e.stagger.active());
        assert!(e.health > 0.);
        drop(e);
        {
            let mut e = g.world.get::<&mut Enemy>(id).unwrap();
            e.stagger.advance(1.).unwrap();
            assert!(!e.stagger.active());
        }
    }
}

#[cfg(test)]
mod shield_loop_tests {
    use super::*;
    #[test]
    fn movement_time_recovers_shields_after_the_quiet_window() {
        let mut g = Game::new();
        g.remaining = 0;
        g.intermission = 100.;
        assert_eq!(g.shield.absorb(25.).unwrap(), 5.);
        for _ in 0..360 {
            g.update(Input::default(), 1. / 120.);
        }
        assert_eq!(g.shield.value(), 0.);
        for _ in 0..480 {
            g.update(Input::default(), 1. / 120.);
        }
        assert_eq!(g.shield.value(), 20.);
        assert_eq!(g.health, 100);
        assert_eq!(Game::new().shield.value(), 20.);
    }
}

#[cfg(test)]
mod magazine_loop_tests {
    use super::*;
    #[test]
    fn continuous_fire_cycles_energy_cells_without_spending_denied_shots() {
        let mut g = Game::new();
        g.remaining = 0;
        g.intermission = 100.;
        let input = Input {
            fire: true,
            aim: g.player + Vec2::X,
            ..Input::default()
        };
        let mut observed_reload = false;
        for _ in 0..120 * 5 {
            let before = g.shots;
            let reloading = g.magazine.reloading();
            g.update(input, 1. / 120.);
            if reloading && g.magazine.reloading() {
                assert_eq!(g.shots, before);
            }
            observed_reload |= g.magazine.reloading();
        }
        assert!(observed_reload);
        assert!(g.shots > 24);
        assert!(g.magazine.loaded() <= 12);
    }
}

#[cfg(test)]
mod dash_buffer_tests {
    use super::*;
    #[test]
    fn early_dash_taps_execute_at_readiness_and_holds_do_not_repeat() {
        let mut g = Game::new();
        g.remaining = 0;
        g.intermission = 100.;
        let dash = Input {
            dash: true,
            aim: g.player + Vec2::X,
            ..Input::default()
        };
        g.update(dash, 1. / 120.);
        for _ in 0..190 {
            g.update(Input::default(), 1. / 120.);
        }
        g.update(dash, 1. / 120.); // Tap just before the 1.7-second charge returns.
        g.update(Input::default(), 1. / 120.);
        for _ in 0..20 {
            g.update(Input::default(), 1. / 120.);
        }
        assert!(g.dash_cooldown > 1.5);
        for _ in 0..500 {
            g.update(dash, 1. / 120.);
        }
        assert_eq!(g.dash_cooldown, 0., "a held button cannot repeatedly dash");
    }
}

#[cfg(test)]
mod migration_tests {
    use super::*;

    fn stationary_target(game: &mut Game, position: Vec2, heavy: bool) -> Entity {
        game.wave = if heavy { 2 } else { 1 };
        game.spawn_enemy(position);
        game.remaining = 0;
        let id = game
            .world
            .query::<(Entity, &Enemy)>()
            .iter()
            .next()
            .unwrap()
            .0;
        game.world.get::<&mut Enemy>(id).unwrap().speed = 0.;
        id
    }

    fn bullet(game: &mut Game) {
        game.world.spawn((
            Position(game.player),
            Bullet {
                velocity: Vec2::X * 900.,
                remaining: 1.,
            },
        ));
    }

    #[test]
    fn wave_clear_requires_no_pending_or_live_drones_and_preserves_the_break() {
        let mut game = Game::new();
        assert!(
            game.wave_clear_remaining().is_none(),
            "initial spawn delay is not a clear"
        );
        let position = game.player + Vec2::X * 80.;
        let enemy = stationary_target(&mut game, position, false);
        assert!(
            game.wave_clear_remaining().is_none(),
            "a live drone keeps the wave active"
        );
        game.world.despawn(enemy).unwrap();
        assert_eq!(game.wave_clear_remaining(), Some(1.6));
        for _ in 0..191 {
            game.update(Input::default(), 1. / 120.);
            assert_eq!(game.wave, 1, "the clear banner must have its full interval");
            assert_eq!(game.remaining_enemies(), 0);
        }
        for _ in 0..3 {
            if game.wave == 1 {
                game.update(Input::default(), 1. / 120.);
            }
        }
        assert_eq!((game.wave, game.remaining_enemies()), (2, 14));
        assert!(game.wave_clear_remaining().is_none());
        assert_eq!(game.wave_banner, 2.4);
        assert_eq!(
            game.enemy_count(),
            0,
            "the new wave still honors spawn admission delay"
        );
        game.wave = 3;
        game.remaining = 0;
        game.intermission = 0.01;
        game.update(Input::default(), 1. / 120.);
        assert_eq!(game.phase, Phase::Playing);
        game.update(Input::default(), 1. / 120.);
        assert_eq!(game.phase, Phase::Won);
        assert!(game.wave_clear_remaining().is_none());
    }

    #[test]
    fn impacts_report_nonlethal_and_lethal_hits_without_double_counting_a_dead_target() {
        let mut game = Game::new();
        let position = game.player + Vec2::X * 80.;
        let enemy = stationary_target(&mut game, position, true);
        bullet(&mut game);
        let mut events = Vec::new();
        game.update_bullets(0.1, &mut |event| events.push(event));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, FeedbackKind::Hit);
        assert_eq!(game.world.get::<&Enemy>(enemy).unwrap().health, 48.);
        let marker = game.hit_marker.unwrap();
        assert!(!marker.lethal);
        assert_eq!(marker.position.extend(0.), events[0].position);
        assert_eq!(game.kills, 0);

        events.clear();
        for _ in 0..3 {
            bullet(&mut game);
        }
        game.update_bullets(0.1, &mut |event| events.push(event));
        assert_eq!(
            events.len(),
            2,
            "the third impact reached an already dead target"
        );
        assert!(events.iter().all(|event| event.kind == FeedbackKind::Hit));
        assert!(game.hit_marker.unwrap().lethal);
        assert_eq!(
            (game.enemy_count(), game.kills, game.score.total()),
            (0, 1, 100)
        );
        game.update_with_feedback(Input::default(), 0.2, |_| panic!("no new impact"));
        assert!(
            game.hit_marker.is_none(),
            "the marker expires in simulation time"
        );
    }

    #[test]
    fn shield_only_hit_and_same_tick_healing_both_emit_feedback() {
        let mut game = Game::new();
        let position = game.player;
        let enemy = stationary_target(&mut game, position, false);
        game.world
            .get::<&mut Enemy>(enemy)
            .unwrap()
            .attack
            .step(0.155, true)
            .unwrap();
        game.health = 90;
        game.world
            .spawn((Position(game.player), Pickup { remaining: 5. }));
        let mut events = Vec::new();
        game.update_with_feedback(Input::default(), 1. / 120., |event| events.push(event));
        assert_eq!((game.health, game.shield.value()), (100, 6.));
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].kind, FeedbackKind::Hurt);
        assert_eq!(events[0].intensity, 0.55);
        assert_eq!(events[1].kind, FeedbackKind::Pickup);
        game.update_with_feedback(Input::default(), 1. / 120., |_| {
            panic!("continuing an attack or an already consumed pickup is not a new event")
        });
    }

    #[test]
    fn lethal_contact_cannot_be_undone_by_a_pickup_or_the_last_kill() {
        let mut game = Game::new();
        let position = game.player;
        let enemy = stationary_target(&mut game, position, false);
        game.world
            .get::<&mut Enemy>(enemy)
            .unwrap()
            .attack
            .step(0.155, true)
            .unwrap();
        game.wave = 3;
        game.health = 5;
        game.shield.absorb(20.).unwrap();
        game.world
            .spawn((Position(game.player), Pickup { remaining: 5. }));
        bullet(&mut game);
        let mut events = Vec::new();
        game.update_with_feedback(Input::default(), 1. / 120., |event| events.push(event));
        assert_eq!((game.health, game.enemy_count()), (0, 0));
        assert_eq!(game.phase, Phase::Lost);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].kind, FeedbackKind::Hurt);
        assert_eq!(events[1].kind, FeedbackKind::Hit);
        assert_eq!(game.world.query::<&Pickup>().iter().count(), 1);
        let final_state = (game.time, game.player, game.shots, game.world.len());
        game.update_with_feedback(
            Input {
                fire: true,
                dash_pressed: true,
                movement: Vec2::X,
                ..Input::default()
            },
            0.1,
            |_| panic!("terminal games emit no combat feedback"),
        );
        assert_eq!(
            (game.time, game.player, game.shots, game.world.len()),
            final_state
        );
        assert_eq!(game.phase, Phase::Lost);
    }

    #[test]
    fn rejected_fire_and_dash_requests_do_not_emit_acceptance_feedback() {
        let mut game = Game::new();
        let input = Input {
            fire: true,
            dash: true,
            ..Input::default()
        };
        let mut events = Vec::new();
        game.update_with_feedback(input, 1. / 120., |event| events.push(event.kind));
        assert_eq!(events, [FeedbackKind::Dash, FeedbackKind::Fire]);
        game.update_with_feedback(input, 1. / 120., |_| {
            panic!("cooldowns reject both requests")
        });
    }
}
