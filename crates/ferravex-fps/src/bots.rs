//! Offline practice bots use shared sensing/decisions/navigation. They submit
//! ordinary InputFrames to Match; collision, ammo, damage and respawn stay there.
use crate::{Arena, FIXED_DT, GameEvent, MAX_PLAYERS, Match, forward};
use ferravex_math::{Vec3, vec3};
use ferravex_net::{BUTTON_FIRE, BUTTON_RELOAD, InputFrame};
use ferravex_play::{ai::*, navigation::*};
const WIDTH: u32 = 48;
const HEIGHT: u32 = 56;
fn point(c: Cell) -> Vec3 {
    vec3(c.x as f32 - 23.5, 0., c.y as f32 - 27.5)
}
struct ArenaSight<'a>(&'a Arena);
impl VisibilityQuery for ArenaSight<'_> {
    fn clear(&mut self, from: Vec3, to: Vec3) -> bool {
        let d = to - from;
        let length = d.length();
        length <= 1e-6
            || self
                .0
                .raycast(from, d / length, length)
                .is_none_or(|hit| hit >= length - 1e-4)
    }
}
struct Brain {
    memory: Memory,
    tactics: EnemyController,
    intent: Option<EnemyIntent>,
    route: Route3,
    follower: WaypointFollower,
    replan: f32,
    patrol: usize,
    alive: bool,
    radio: u64,
    tactical_route: bool,
    cover_goal: Option<(Vec3, f32)>,
}
pub struct PracticeBots {
    brains: Vec<Brain>,
    grid: Grid,
    planner: PathPlanner,
    schedule: RoundRobin,
    noises: NoiseBuffer,
    squad: SquadBoard<16>,
    cover_schedule: RoundRobin,
    pub work: WorkCounts,
}
impl PracticeBots {
    pub fn new(arena: &Arena) -> Self {
        let mut grid = Grid::new(WIDTH, HEIGHT).unwrap();
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let c = Cell { x, y };
                let p = point(c);
                let blocked = arena.solids.iter().any(|s| {
                    p.x >= s.min().x - 0.45
                        && p.x <= s.max().x + 0.45
                        && p.z >= s.min().z - 0.45
                        && p.z <= s.max().z + 0.45
                });
                grid.set_blocked(c, blocked).unwrap();
            }
        }
        let brains = (0..MAX_PLAYERS)
            .map(|i| Brain {
                memory: Memory::new(MemorySettings {
                    capacity: 1,
                    sight_seconds: 2.,
                    hearing_seconds: 3.,
                })
                .unwrap(),
                tactics: EnemyController::new(EnemyConfig::ranged(), i as u64).unwrap(),
                intent: None,
                route: Route3::new(256).unwrap(),
                follower: WaypointFollower::default(),
                replan: 0.,
                patrol: 0,
                alive: false,
                radio: 0,
                tactical_route: false,
                cover_goal: None,
            })
            .collect();
        let mut cover = [Vec3::ZERO; 16];
        for (i, s) in arena
            .solids
            .iter()
            .filter(|s| s.half_extents.x < 10. && s.half_extents.z < 10.)
            .take(4)
            .enumerate()
        {
            for (j, d) in [Vec3::X, -Vec3::X, Vec3::Z, -Vec3::Z]
                .into_iter()
                .enumerate()
            {
                cover[i * 4 + j] = s.center
                    + d * (if j < 2 {
                        s.half_extents.x
                    } else {
                        s.half_extents.z
                    } + 1.1);
                cover[i * 4 + j].y = 1.62;
            }
        }
        Self {
            brains,
            grid,
            planner: PathPlanner::new((WIDTH * HEIGHT) as usize, MAX_PLAYERS).unwrap(),
            schedule: RoundRobin::default(),
            noises: NoiseBuffer::new(8).unwrap(),
            squad: SquadBoard::new(cover).unwrap(),
            cover_schedule: RoundRobin::default(),
            work: WorkCounts::default(),
        }
    }
    pub fn events(&mut self, events: &[GameEvent]) {
        for event in events {
            if let GameEvent::Shot {
                player: 0, origin, ..
            } = event
            {
                self.noises
                    .push(
                        Noise {
                            source: 0,
                            position: *origin,
                            groups: 1,
                            radius: 45.,
                        },
                        0.2,
                    )
                    .unwrap();
            }
        }
    }
    pub fn commands(
        &mut self,
        game: &Match,
        local: InputFrame,
        commands: &mut Vec<(u8, InputFrame)>,
    ) {
        commands.clear();
        commands.push((
            0,
            InputFrame {
                sequence: game.tick,
                ..local
            },
        ));
        self.noises.advance(FIXED_DT).unwrap();
        self.squad.advance(FIXED_DT).unwrap();
        let mut budget = WorkBudget::new(64, 16, 128);
        let target = game.players.iter().find(|p| p.id == 0);
        for bot in game.players.iter().filter(|p| p.id != 0) {
            let b = &mut self.brains[bot.id as usize];
            b.memory.advance(FIXED_DT).unwrap();
            b.replan = (b.replan - FIXED_DT).max(0.);
            if let Some((_, time)) = &mut b.cover_goal {
                *time -= FIXED_DT;
                if *time <= 0. {
                    b.cover_goal = None;
                }
            }
            if b.alive != bot.is_alive() {
                self.squad.release(u64::from(bot.id));
                b.tactical_route = false;
                b.cover_goal = None;
                b.memory.clear();
                b.route.clear();
                b.follower.reset();
                b.tactics.reset();
                b.intent = None;
                b.replan = 0.;
                b.alive = bot.is_alive();
                self.planner.cancel(u64::from(bot.id));
            }
            if target.is_none_or(|p| !p.is_alive()) {
                b.memory.forget(0);
                self.squad.forget();
            }
        }
        let count = game.players.len().saturating_sub(1);
        for index in self.schedule.next(count, 3) {
            let bot = &game.players[index + 1];
            if bot.id == 0 || !bot.is_alive() {
                continue;
            }
            let b = &mut self.brains[bot.id as usize];
            if let Some(target) = target.filter(|p| p.is_alive()) {
                let candidate = Candidate {
                    id: 0,
                    position: target.eye_position(),
                    groups: 1,
                };
                if Sight::new(70., 130_f32.to_radians(), 1)
                    .unwrap()
                    .test(
                        Observer {
                            position: bot.eye_position(),
                            forward: forward(bot.yaw, bot.pitch),
                        },
                        candidate,
                        &mut budget,
                        &mut ArenaSight(&game.arena),
                    )
                    .unwrap()
                    == Detection::Detected
                {
                    b.memory.observe(candidate, Sense::Sight).unwrap();
                    self.squad.report(candidate).unwrap();
                }
                for noise in self.noises.iter() {
                    if Hearing::new(45., 1, 0.5)
                        .unwrap()
                        .test(
                            bot.eye_position(),
                            noise,
                            &mut budget,
                            Some(&mut ArenaSight(&game.arena)),
                        )
                        .unwrap()
                        == Detection::Detected
                    {
                        b.memory
                            .observe(
                                Candidate {
                                    id: 0,
                                    position: noise.position,
                                    groups: 1,
                                },
                                Sense::Hearing,
                            )
                            .unwrap();
                    }
                }
            }
        }
        let cover_bot = self
            .cover_schedule
            .next(count, 1)
            .next()
            .map(|i| game.players[i + 1].id);
        for bot in game.players.iter().filter(|p| p.id != 0) {
            let b = &mut self.brains[bot.id as usize];
            if let Some(radio) = self.squad.contact().filter(|c| c.revision != b.radio) {
                if bot.is_alive() {
                    b.memory.observe(radio.candidate, Sense::Hearing).unwrap();
                }
                b.radio = radio.revision;
            }
            let memory = b.memory.best(1);
            if !bot.is_alive() {
                commands.push((
                    bot.id,
                    InputFrame {
                        sequence: game.tick,
                        ..Default::default()
                    },
                ));
                continue;
            }
            let patrol = if game.arena.spawns.is_empty() {
                bot.position
            } else {
                game.arena.spawns
                    [(usize::from(bot.id) + 1 + b.patrol * 3) % game.arena.spawns.len()]
            };
            let intent = b
                .tactics
                .tick(
                    EnemyInput {
                        position: bot.eye_position(),
                        forward: forward(bot.yaw, bot.pitch),
                        up: Vec3::Y,
                        patrol_goal: patrol,
                        target: memory.copied(),
                        health_fraction: f32::from(bot.health) / 100.,
                        weapon_ready: bot.ammo > 0 && bot.reload_ticks == 0,
                        alive: bot.is_alive(),
                    },
                    FIXED_DT,
                )
                .unwrap();
            b.intent = Some(intent);
            let mut goal = intent.goal;
            b.tactical_route = false;
            if let Some(track) = memory {
                let role = SquadRole::for_member(u64::from(bot.id));
                if cover_bot == Some(bot.id)
                    && (matches!(intent.state, EnemyState::Retreat | EnemyState::Reload)
                        || role == SquadRole::Support)
                {
                    if let Some(p) = self
                        .squad
                        .reserve(
                            u64::from(bot.id),
                            bot.eye_position(),
                            track.position,
                            &mut budget,
                            &mut ArenaSight(&game.arena),
                            |p| nearest(&self.grid, p).is_some(),
                        )
                        .unwrap()
                    {
                        b.cover_goal = Some((p, 1.2));
                        goal = p;
                        b.tactical_route = true;
                        b.replan = 0.;
                    }
                } else if matches!(role, SquadRole::FlankLeft | SquadRole::FlankRight) {
                    goal = role
                        .goal(track.position, bot.position, Vec3::Y, 4.)
                        .unwrap();
                    b.tactical_route = true;
                }
                if let Some((p, _)) = b.cover_goal {
                    goal = p;
                    b.tactical_route = true;
                }
            } else {
                b.cover_goal = None;
            }

            if intent.replan {
                b.replan = 0.;
                b.route.clear();
                b.follower.reset();
            }
            if memory.is_none() && bot.position.distance(patrol) < 1.5 {
                b.patrol = b.patrol.wrapping_add(1);
                b.replan = 0.;
            }
            if b.replan == 0.
                && let (Some(start), Some(end)) =
                    (nearest(&self.grid, bot.position), nearest(&self.grid, goal))
            {
                let status = self.planner.request(PathRequest {
                    agent: u64::from(bot.id),
                    start,
                    end,
                    connectivity: Connectivity::Eight,
                });
                if status != RequestStatus::Full {
                    b.replan = 1.2;
                }
            }
        }
        self.planner.step(&self.grid, &mut budget);
        if let Some((request, result)) = self.planner.result() {
            if let Some(b) = self.brains.get_mut(request.agent as usize)
                && b.alive
                && let Ok(route) = result
                && route.len() <= 256
            {
                let mut points = [Vec3::ZERO; 256];
                for (out, c) in points.iter_mut().zip(route.iter().rev()) {
                    *out = point(*c);
                }
                b.route.set(&points[..route.len()]).unwrap();
                b.follower.reset();
            }
            self.planner.acknowledge();
        }
        for bot in game.players.iter().filter(|p| p.id != 0 && p.is_alive()) {
            let b = &mut self.brains[bot.id as usize];
            let memory = b.memory.best(1);
            let visible = memory.is_some_and(|t| t.visible);
            let waypoint = b.follower.target(&b.route, bot.position, 0.6, 8).unwrap();
            let intent = b.intent.unwrap();
            let direction = if visible && !b.tactical_route {
                intent.movement
            } else {
                waypoint.map_or(Vec3::ZERO, |p| (p - bot.position).normalize_or_zero()) * 0.65
            };
            let aim = if visible {
                intent.aim
            } else {
                turn_towards(forward(bot.yaw, bot.pitch), direction, 4. * FIXED_DT)
            };
            let yaw = aim.x.atan2(-aim.z);
            let pitch = aim.y.clamp(-1., 1.).asin();
            let right = vec3(yaw.cos(), 0., yaw.sin());
            let ahead = forward(yaw, 0.);
            let buttons = if intent.reload {
                BUTTON_RELOAD
            } else if intent.attack {
                BUTTON_FIRE
            } else {
                0
            };
            commands.push((
                bot.id,
                InputFrame {
                    sequence: game.tick,
                    move_x: direction.dot(right),
                    move_z: direction.dot(ahead),
                    yaw,
                    pitch,
                    buttons,
                },
            ));
        }
        self.work = budget.counts();
    }
    #[cfg(test)]
    fn memory(&self, id: usize) -> Option<Track> {
        self.brains[id].memory.best(1).copied()
    }
}
fn nearest(grid: &Grid, p: Vec3) -> Option<Cell> {
    let x = ((p.x + 24.).floor() as i32).clamp(0, WIDTH as i32 - 1);
    let y = ((p.z + 28.).floor() as i32).clamp(0, HEIGHT as i32 - 1);
    for radius in 0_i32..=2 {
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx.abs().max(dy.abs()) != radius {
                    continue;
                }
                let (x, y) = (x + dx, y + dy);
                if x >= 0 && y >= 0 && x < WIDTH as i32 && y < HEIGHT as i32 {
                    let c = Cell {
                        x: x as u32,
                        y: y as u32,
                    };
                    if grid.walkable(c) {
                        return Some(c);
                    }
                }
            }
        }
    }
    None
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::Obstacle;
    #[test]
    fn hidden_silent_players_are_not_known_and_sound_never_grants_fire() {
        let mut game = Match::new(Arena {
            solids: vec![Obstacle {
                center: vec3(0., 2., 0.),
                half_extents: vec3(1., 2., 8.),
            }],
            spawns: vec![vec3(5., 0., 0.), vec3(-5., 0., 0.)],
        });
        game.spawn_player(0);
        game.spawn_player(1);
        game.players[1].yaw = std::f32::consts::FRAC_PI_2;
        let mut bots = PracticeBots::new(&game.arena);
        let mut commands = Vec::with_capacity(MAX_PLAYERS);
        for _ in 0..400 {
            bots.commands(&game, InputFrame::default(), &mut commands);
            assert!(bots.memory(1).is_none());
            assert!(commands.iter().all(|(_, i)| i.buttons & BUTTON_FIRE == 0));
            game.tick += 1;
        }
        bots.events(&[GameEvent::Shot {
            player: 0,
            origin: game.players[0].eye_position(),
            end: Vec3::ZERO,
        }]);
        bots.commands(&game, InputFrame::default(), &mut commands);
        let track = bots.memory(1).unwrap();
        assert!(!track.visible);
        assert_eq!(track.sense, Sense::Hearing);
        assert_eq!(commands[1].1.buttons & BUTTON_FIRE, 0);
    }
    #[test]
    fn squad_radio_shares_position_without_granting_visibility() {
        let mut game = Match::new(Arena {
            solids: vec![Obstacle {
                center: vec3(0., 2., 0.),
                half_extents: vec3(1., 2., 8.),
            }],
            spawns: vec![vec3(5., 0., 0.), vec3(5., 0., 8.), vec3(-5., 0., 0.)],
        });
        for id in 0..3 {
            game.spawn_player(id);
        }
        game.players[1].position = vec3(5., 0., 8.);
        game.players[1].yaw = 0.;
        game.players[2].position = vec3(-5., 0., 0.);
        game.players[2].yaw = std::f32::consts::FRAC_PI_2;
        let mut bots = PracticeBots::new(&game.arena);
        let mut commands = Vec::new();
        bots.commands(&game, InputFrame::default(), &mut commands);
        assert!(bots.memory(1).unwrap().visible);
        let radio = bots.memory(2).unwrap();
        assert!(!radio.visible);
        assert_eq!(radio.sense, Sense::Hearing);
        assert_eq!(
            commands.iter().find(|(id, _)| *id == 2).unwrap().1.buttons & BUTTON_FIRE,
            0
        );
    }
    #[test]
    fn visible_bots_submit_normal_accepted_shots_with_bounded_work() {
        let mut game = Match::new(Arena {
            solids: vec![],
            spawns: vec![vec3(0., 0., -8.), Vec3::ZERO],
        });
        game.spawn_player(0);
        game.spawn_player(1);
        game.tick = 400;
        let mut bots = PracticeBots::new(&game.arena);
        let mut commands = Vec::with_capacity(MAX_PLAYERS);
        let mut shots = 0;
        for _ in 0..600 {
            bots.commands(&game, InputFrame::default(), &mut commands);
            assert!(
                bots.work.queries <= 16
                    && bots.work.candidates <= 64
                    && bots.work.path_steps <= 128
            );
            for (_, input) in &commands {
                assert!(input.yaw.is_finite() && input.pitch.is_finite());
            }
            let events = game.step(&commands);
            shots += events
                .iter()
                .filter(|e| matches!(e, GameEvent::Shot { player: 1, .. }))
                .count();
            bots.events(&events);
        }
        assert!(shots > 0, "no authority-accepted bot shots");
    }
}
