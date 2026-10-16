use ferravex_engine::{
    dynamics::{BodyDesc, BodyHandle, PhysicsWorld, QueryFilter, Shape},
    play::particles::{ParticleDesc, ParticlePool},
};

pub const WIDTH: u32 = 640;
pub const HEIGHT: u32 = 720;
pub const LEFT: f32 = 38.0;
pub const RIGHT: f32 = 602.0;
pub const TOP: f32 = 106.0;
pub const PADDLE_Y: f32 = 636.0;
pub const BALL_RADIUS: f32 = 6.0;
const PADDLE_HALF: f32 = 43.0;
const MAX_IMPACTS: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Phase {
    Ready = 0,
    Playing = 1,
    Transition = 2,
    Won = 3,
    Lost = 4,
}
#[derive(Clone, Copy, Default, Debug)]
pub struct Input {
    pub axis: f32,
    pub launch: bool,
    pub restart: bool,
}
pub struct Block {
    pub body: BodyHandle,
    pub center: [f32; 2],
    pub half: [f32; 2],
    pub health: u8,
    pub original_health: u8,
    pub flash: f32,
    pub row: usize,
}
pub struct Drop {
    pub position: [f32; 2],
}

pub struct Game {
    world: PhysicsWorld<2>,
    paddle_body: BodyHandle,
    pub paddle_x: f32,
    pub paddle_half: f32,
    pub ball: [f32; 2],
    pub velocity: [f32; 2],
    pub blocks: Vec<Block>,
    pub particles: ParticlePool,
    pub drops: Vec<Drop>,
    pub trail: Vec<[f32; 2]>,
    pub phase: Phase,
    pub lives: u32,
    pub sector: u32,
    pub score: u32,
    pub chain: ferravex_engine::gameplay::combo::ScoreChain,
    pub time: f32,
    pub wide_time: f32,
    pub hits: u32,
    pub clears: u32,
    pub ccd_impacts: u64,
    transition: f32,
    launch_count: u32,
    reward_rng: ferravex_engine::gameplay::loot::RewardRng,
    drop_pity: ferravex_engine::gameplay::pity::PityCounter,
    trail_clock: f32,
    paddle_velocity: f32,
}
impl Default for Game {
    fn default() -> Self {
        Self::new()
    }
}
impl Game {
    pub fn new() -> Self {
        let mut world = PhysicsWorld::<2>::new([0.0; 2]);
        for (center, half) in [
            ([LEFT - 10.0, 380.0], [10.0, 274.0]),
            ([RIGHT + 10.0, 380.0], [10.0, 274.0]),
            ([320.0, TOP - 10.0], [282.0, 10.0]),
        ] {
            world
                .insert(BodyDesc::fixed(Shape::cuboid(half), center))
                .expect("valid wall");
        }
        let paddle_body = world
            .insert(BodyDesc::fixed(
                Shape::cuboid([PADDLE_HALF, 8.0]),
                [320.0, PADDLE_Y],
            ))
            .expect("valid paddle");
        let mut game = Self {
            world,
            paddle_body,
            paddle_x: 320.0,
            paddle_half: PADDLE_HALF,
            ball: [320.0, PADDLE_Y - 16.0],
            velocity: [0.0; 2],
            blocks: Vec::with_capacity(35),
            particles: ParticlePool::new(512),
            drops: Vec::with_capacity(8),
            trail: Vec::with_capacity(18),
            phase: Phase::Ready,
            lives: 3,
            sector: 1,
            score: 0,
            chain: ferravex_engine::gameplay::combo::ScoreChain::new(4., 3, 4).unwrap(),
            time: 0.0,
            wide_time: 0.0,
            hits: 0,
            clears: 0,
            ccd_impacts: 0,
            transition: 0.0,
            launch_count: 0,
            reward_rng: ferravex_engine::gameplay::loot::RewardRng::new(0x4f52424954),
            drop_pity: ferravex_engine::gameplay::pity::PityCounter::new(6).unwrap(),
            trail_clock: 0.0,
            paddle_velocity: 0.,
        };
        game.build_sector();
        game
    }
    pub fn remaining(&self) -> usize {
        self.blocks.iter().filter(|block| block.health > 0).count()
    }
    pub fn autoplay(&self) -> Input {
        let mut landing = self.ball[0];
        if self.velocity[1] > 0.0 {
            let flight =
                ((PADDLE_Y - 8.0 - BALL_RADIUS - self.ball[1]) / self.velocity[1]).max(0.0);
            landing = reflect_x(self.ball[0] + self.velocity[0] * flight);
        }
        let target = self.blocks.iter().filter(|b| b.health > 0).min_by(|a, b| {
            let da = (a.center[0] - landing).abs() + (a.center[1] - self.ball[1]).abs() * 0.05;
            let db = (b.center[0] - landing).abs() + (b.center[1] - self.ball[1]).abs() * 0.05;
            da.total_cmp(&db)
        });
        let offset = if let Some(target) = target {
            let dx = target.center[0] - landing;
            let dy = PADDLE_Y - target.center[1];
            (dx / dx.hypot(dy) / 0.86).clamp(-0.8, 0.8) * self.paddle_half
        } else {
            0.0
        };
        let desired = if self.velocity[1] > 0.0 {
            landing - offset
        } else {
            self.ball[0]
        };
        Input {
            axis: ((desired - self.paddle_x) / 10.0).clamp(-1.0, 1.0),
            launch: self.phase == Phase::Ready,
            restart: false,
        }
    }
    pub fn tick(&mut self, input: Input, dt: f32) {
        if !dt.is_finite() || !(0.0..=0.1).contains(&dt) {
            return;
        }
        if input.restart || input.launch && matches!(self.phase, Phase::Won | Phase::Lost) {
            *self = Self::new();
            return;
        }
        self.particles.update(dt).expect("validated delta");
        for block in &mut self.blocks {
            block.flash = (block.flash - dt).max(0.0);
        }
        if matches!(self.phase, Phase::Won | Phase::Lost) {
            return;
        }
        self.time += dt;
        self.chain.advance(dt).unwrap();
        if self.phase == Phase::Transition {
            self.transition -= dt;
            if self.transition <= 0.0 {
                self.sector += 1;
                self.build_sector();
                self.phase = Phase::Ready;
            }
            return;
        }
        if self.wide_time > 0.0 {
            self.wide_time = (self.wide_time - dt).max(0.0);
            if self.wide_time == 0.0 {
                self.resize_paddle(PADDLE_HALF);
            }
        }
        let axis = if input.axis.is_finite() {
            input.axis.clamp(-1.0, 1.0)
        } else {
            0.0
        };
        let previous_paddle = self.paddle_x;
        self.paddle_x = (self.paddle_x + axis * 590.0 * dt).clamp(
            LEFT + self.paddle_half + 1.0,
            RIGHT - self.paddle_half - 1.0,
        );
        self.paddle_velocity = if dt > 0. {
            (self.paddle_x - previous_paddle) / dt
        } else {
            0.
        };
        self.world
            .body_mut(self.paddle_body)
            .expect("paddle alive")
            .set_position([self.paddle_x, PADDLE_Y])
            .expect("finite paddle");
        if self.phase == Phase::Ready {
            self.ball = [self.paddle_x, PADDLE_Y - 8.0 - BALL_RADIUS - 1.0];
            if input.launch {
                self.phase = Phase::Playing;
                self.launch_count += 1;
                let angle = 0.24 + (self.launch_count % 3) as f32 * 0.09;
                let speed = self.speed();
                self.velocity = [angle.sin() * speed, -angle.cos() * speed];
            } else {
                return;
            }
        }
        self.move_ball(dt);
        self.update_drops(dt);
        self.trail_clock += dt;
        if self.trail_clock >= 1.0 / 90.0 {
            self.trail_clock = 0.0;
            if self.trail.len() == 18 {
                self.trail.remove(0);
            }
            self.trail.push(self.ball);
        }
        if self.ball[1] > 677.0 {
            self.lives -= 1;
            self.chain.break_chain();
            self.trail.clear();
            self.velocity = [0.0; 2];
            self.drops.clear();
            self.wide_time = 0.0;
            self.resize_paddle(PADDLE_HALF);
            self.phase = if self.lives == 0 {
                Phase::Lost
            } else {
                Phase::Ready
            };
        } else if self.remaining() == 0 {
            self.phase = if self.sector == 2 {
                Phase::Won
            } else {
                Phase::Transition
            };
            self.transition = 1.4;
            self.velocity = [0.0; 2];
            self.trail.clear();
            self.drops.clear();
        }
    }
    fn speed(&self) -> f32 {
        (300.0 + self.sector as f32 * 25.0 + self.clears as f32 * 1.1).min(430.0)
    }
    fn resize_paddle(&mut self, half: f32) {
        self.world.remove(self.paddle_body);
        self.paddle_half = half;
        self.paddle_x = self.paddle_x.clamp(LEFT + half + 1.0, RIGHT - half - 1.0);
        self.paddle_body = self
            .world
            .insert(BodyDesc::fixed(
                Shape::cuboid([half, 8.0]),
                [self.paddle_x, PADDLE_Y],
            ))
            .expect("valid paddle");
    }
    fn build_sector(&mut self) {
        for block in &self.blocks {
            self.world.remove(block.body);
        }
        self.blocks.clear();
        self.trail.clear();
        self.drops.clear();
        self.wide_time = 0.0;
        self.resize_paddle(PADDLE_HALF);
        self.paddle_x = 320.0;
        for row in 0..5 {
            for col in 0..7 {
                let center = [80.0 + col as f32 * 80.0, 165.0 + row as f32 * 40.0];
                let half = [35.0, 14.0];
                let body = self
                    .world
                    .insert(BodyDesc::fixed(Shape::cuboid(half), center))
                    .expect("valid block");
                let health = if self.sector == 2 && (row + col) % 3 == 0 {
                    2
                } else {
                    1
                };
                self.blocks.push(Block {
                    body,
                    center,
                    half,
                    health,
                    original_health: health,
                    flash: 0.0,
                    row,
                });
            }
        }
        self.ball = [self.paddle_x, PADDLE_Y - 15.0];
    }
    fn move_ball(&mut self, dt: f32) {
        let mut remaining = dt;
        // Game-specific reflection is driven by the engine's exact continuous
        // circle casts against every wall, paddle, and destructible block.
        for _ in 0..MAX_IMPACTS {
            let speed = self.velocity[0].hypot(self.velocity[1]);
            if speed < 0.001 || remaining <= 1e-6 {
                break;
            }
            let direction = [self.velocity[0] / speed, self.velocity[1] / speed];
            let hit = self.world.cast_ball(
                self.ball,
                BALL_RADIUS,
                direction,
                speed * remaining,
                QueryFilter {
                    only_static: true,
                    ..QueryFilter::default()
                },
            );
            let Some(hit) = hit else {
                self.ball[0] += self.velocity[0] * remaining;
                self.ball[1] += self.velocity[1] * remaining;
                break;
            };
            self.ccd_impacts += 1;
            self.ball = [
                hit.point[0] + hit.normal[0] * 0.025,
                hit.point[1] + hit.normal[1] * 0.025,
            ];
            remaining = (remaining - hit.distance / speed).max(0.0);
            if hit.body == self.paddle_body && hit.normal[1] < -0.1 && self.velocity[1] > 0.0 {
                let offset = ((self.ball[0] - self.paddle_x) / self.paddle_half).clamp(-1.0, 1.0);
                let horizontal = if offset.abs() < 0.08 {
                    if self.velocity[0] < 0.0 { -0.08 } else { 0.08 }
                } else {
                    offset
                } * 0.86;
                // The paddle's actual movement adds a little English. Angle and
                // total speed remain bounded so a save never creates a flat orbit.
                let horizontal =
                    (horizontal + self.paddle_velocity / 590. * 0.11).clamp(-0.90, 0.90);
                let next_speed = self.speed();
                self.velocity = [
                    horizontal * next_speed,
                    -(1.0 - horizontal * horizontal).sqrt() * next_speed,
                ];
                self.chain.break_chain();
                self.emit(self.ball, [0.35, 0.85, 0.79, 1.0], 5);
            } else {
                let dot = self.velocity[0] * hit.normal[0] + self.velocity[1] * hit.normal[1];
                if dot < 0.0 {
                    self.velocity[0] -= 2.0 * dot * hit.normal[0];
                    self.velocity[1] -= 2.0 * dot * hit.normal[1];
                    // Avoid an unplayable near-horizontal loop between side walls.
                    if self.velocity[1].abs() < speed * 0.18 {
                        let sign = if self.velocity[1] < 0. { -1. } else { 1. };
                        self.velocity[1] = sign * speed * 0.18;
                        self.velocity[0] = self.velocity[0].signum()
                            * (speed * speed - self.velocity[1].powi(2)).sqrt();
                    }
                }
                if let Some(index) = self
                    .blocks
                    .iter()
                    .position(|b| b.body == hit.body && b.health > 0)
                {
                    self.hit_block(index);
                }
            }
        }
    }
    fn hit_block(&mut self, index: usize) {
        let block = &mut self.blocks[index];
        block.health -= 1;
        block.flash = 0.13;
        let position = block.center;
        let destroyed = block.health == 0;
        self.hits += 1;
        let award = self.chain.award(if destroyed { 100 } else { 25 });
        self.score = self.score.saturating_add(award as u32);
        if destroyed {
            self.world.remove(block.body);
            self.clears += 1;
            let natural = ferravex_engine::gameplay::loot::RewardTable::new([3, 1])
                .unwrap()
                .roll(&mut self.reward_rng)
                == 1;
            if self.drops.len() < 8 && self.drop_pity.attempt(natural) {
                self.drops.push(Drop { position });
            }
        }
        self.emit(
            position,
            if destroyed {
                [0.88, 0.68, 0.40, 1.0]
            } else {
                [0.82, 0.93, 0.92, 1.0]
            },
            if destroyed { 16 } else { 6 },
        );
    }
    fn emit(&mut self, position: [f32; 2], color: [f32; 4], count: usize) {
        for i in 0..count {
            let angle = i as f32 / count as f32 * std::f32::consts::TAU + self.time;
            let speed = 35.0 + (i % 5) as f32 * 16.0;
            let _ = self.particles.spawn(ParticleDesc {
                position: [position[0], position[1], 0.0],
                velocity: [angle.cos() * speed, angle.sin() * speed, 0.0],
                acceleration: [0.0, 65.0, 0.0],
                lifetime: 0.28 + (i % 4) as f32 * 0.08,
                start_color: color,
                end_color: [color[0], color[1], color[2], 0.0],
                start_size: 3.0,
                end_size: 1.0,
            });
        }
    }
    fn update_drops(&mut self, dt: f32) {
        let mut caught = false;
        self.drops.retain_mut(|drop| {
            drop.position[1] += 130.0 * dt;
            if drop.position[1] <= PADDLE_Y + 12. {
                let motion = ferravex_engine::gameplay::pickup::Magnet {
                    radius: 100.,
                    collect_radius: 0.,
                    speed: 180.,
                }
                .step(
                    ferravex_engine::math::Vec2::from_array(drop.position),
                    ferravex_engine::math::Vec2::new(self.paddle_x, PADDLE_Y),
                    dt,
                )
                .unwrap();
                drop.position = motion.position.to_array();
            }
            if (drop.position[1] - PADDLE_Y).abs() < 12.0
                && (drop.position[0] - self.paddle_x).abs() < self.paddle_half + 8.0
            {
                caught = true;
                return false;
            }
            drop.position[1] < 680.0
        });
        if caught {
            self.resize_paddle(67.0);
            self.wide_time = 9.0;
            self.score += 50;
        }
    }
}
fn reflect_x(x: f32) -> f32 {
    let min = LEFT + BALL_RADIUS;
    let max = RIGHT - BALL_RADIUS;
    let width = max - min;
    let phase = (x - min).rem_euclid(width * 2.0);
    min + if phase > width {
        width * 2.0 - phase
    } else {
        phase
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moving_paddle_adds_english_and_shallow_wall_bounces_remain_playable() {
        for axis in [-1., 1.] {
            let mut game = Game::new();
            game.phase = Phase::Playing;
            game.ball = [320. + axis * 590. / 120., PADDLE_Y - 8. - BALL_RADIUS - 1.];
            game.velocity = [0., 325.];
            game.tick(
                Input {
                    axis,
                    ..Default::default()
                },
                1. / 120.,
            );
            assert!(game.velocity[1] < 0. && game.velocity[0] * axis > 0.);
            assert!((game.velocity[0].hypot(game.velocity[1]) - game.speed()).abs() < 0.01);
        }
        let mut game = Game::new();
        game.phase = Phase::Playing;
        game.ball = [RIGHT - BALL_RADIUS - 0.5, 500.];
        game.velocity = [325., 0.01];
        game.tick(Input::default(), 1. / 120.);
        assert!(game.velocity[0] < 0. && game.velocity[1].abs() >= 325. * 0.179);
    }

    #[test]
    fn swept_ball_destroys_thin_block_at_high_speed_without_tunneling() {
        let mut g = Game::new();
        g.phase = Phase::Playing;
        g.ball = [80.0, 380.0];
        g.velocity = [0.0, -12000.0];
        let before = g.remaining();
        g.tick(Input::default(), 1.0 / 120.0);
        assert!(g.remaining() < before);
        assert!(g.velocity[1] > 0.0);
        assert!(g.ccd_impacts > 0);
    }
    #[test]
    fn autoplay_clears_both_sectors_through_normal_controls() {
        let mut g = Game::new();
        for _ in 0..120 * 600 {
            let input = g.autoplay();
            g.tick(input, 1.0 / 120.0);
            if matches!(g.phase, Phase::Won | Phase::Lost) {
                break;
            }
        }
        assert_eq!(
            g.phase,
            Phase::Won,
            "sector {}, remaining {}, lives {}, time {}",
            g.sector,
            g.remaining(),
            g.lives,
            g.time
        );
        assert_eq!(g.clears, 70);
        assert!(g.ccd_impacts > 70);
        assert!(g.score >= 7000);
    }
    #[test]
    fn misses_consume_lives_and_restart_restores_world() {
        let mut g = Game::new();
        for expected in [2, 1, 0] {
            g.phase = Phase::Playing;
            g.ball = [100.0, 676.0];
            g.velocity = [0.0, 325.0];
            g.tick(Input::default(), 1.0 / 120.0);
            assert_eq!(g.lives, expected);
        }
        assert_eq!(g.phase, Phase::Lost);
        g.tick(
            Input {
                restart: true,
                ..Input::default()
            },
            1.0 / 120.0,
        );
        assert_eq!(g.phase, Phase::Ready);
        assert_eq!(g.lives, 3);
        assert_eq!(g.remaining(), 35);
        assert_eq!(g.score, 0);
    }
    #[test]
    fn pickups_expand_paddle_and_expire_without_invalid_handles() {
        let mut g = Game::new();
        g.phase = Phase::Playing;
        g.ball = [320.0, 500.0];
        g.velocity = [0.0, -325.0];
        g.drops.push(Drop {
            position: [g.paddle_x, PADDLE_Y - 2.0],
        });
        g.tick(Input::default(), 1.0 / 120.0);
        assert_eq!(g.paddle_half, 67.0);
        assert!(g.world.body(g.paddle_body).is_some());
        g.wide_time = 0.001;
        g.tick(Input::default(), 1.0 / 120.0);
        assert_eq!(g.paddle_half, PADDLE_HALF);
    }
}

#[cfg(test)]
mod chain_loop_tests {
    use super::*;
    #[test]
    fn successive_panels_in_a_volley_build_bonus_points() {
        let mut g = Game::new();
        for i in 0..4 {
            g.hit_block(i);
        }
        assert_eq!(g.score, 500);
        assert_eq!(g.chain.multiplier(), 2);
        g.chain.advance(5.).unwrap();
        g.hit_block(4);
        assert_eq!(g.score, 600);
        assert_eq!(Game::new().score, 0);
    }
}

#[cfg(test)]
mod reward_loop_tests {
    use super::*;
    #[test]
    fn actual_panel_destruction_never_leaves_more_than_five_unrewarded_clears() {
        let mut g = Game::new();
        let mut drought = 0;
        let mut drops = 0;
        for index in 0..35 {
            g.hit_block(index);
            if g.drops.is_empty() {
                drought += 1;
                assert!(drought < 6);
            } else {
                drought = 0;
                drops += 1;
                g.drops.clear();
            }
        }
        assert!(drops >= 5);
        let mut replay = Game::new();
        for index in 0..35 {
            replay.hit_block(index);
            replay.drops.clear();
        }
        assert_eq!(g.drop_pity.misses(), replay.drop_pity.misses());
    }
}

#[cfg(test)]
mod drop_magnet_tests {
    use super::*;
    #[test]
    fn nearby_capsules_follow_the_paddle_but_distant_ones_stay_on_course() {
        let mut g = Game::new();
        g.drops.push(Drop {
            position: [g.paddle_x + 65., PADDLE_Y - 60.],
        });
        let mut caught = false;
        for _ in 0..120 {
            g.update_drops(1. / 120.);
            caught |= g.wide_time > 0.;
        }
        assert!(caught);
        assert_eq!(g.paddle_half, 67.);
        g.drops.push(Drop {
            position: [LEFT + 10., 400.],
        });
        let x = g.drops[0].position[0];
        g.update_drops(0.1);
        assert_eq!(g.drops[0].position[0], x);
    }
}
