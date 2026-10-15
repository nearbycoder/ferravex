//! Cover-aware, budgeted swarm brains; movement/collision still belongs to Game.
use crate::game::{ARENA, COVER};
use crate::game::{Enemy, Position};
use ferravex_core::World;
use ferravex_play::{ai::*, navigation::*};
use ferravex_render::prelude::*;
const CAP: usize = 64;
const WIDTH: u32 = 32;
const HEIGHT: u32 = 18;
const CELL: f32 = 36.;
const ORIGIN: Vec2 = Vec2::new(64., 104.);
struct CoverSight;
impl VisibilityQuery for CoverSight {
    fn clear(&mut self, from: Vec3, to: Vec3) -> bool {
        COVER.iter().all(|r| {
            ferravex_core::collision::Aabb2::from_min_size(r.point(), r.size())
                .cast_segment(from.truncate(), to.truncate())
                .is_none()
        })
    }
}
struct Brain {
    memory: Memory,
    state: DecisionState,
    route: Route3,
    follower: WaypointFollower,
    replan: f32,
}
pub struct SwarmAi {
    brains: Vec<Brain>,
    graph: DecisionGraph,
    grid: Grid,
    planner: PathPlanner,
    schedule: RoundRobin,
    noises: NoiseBuffer,
    peers: [Option<(usize, Vec2)>; CAP],
    pub work: WorkCounts,
}
impl SwarmAi {
    pub fn new() -> Self {
        let graph = DecisionGraph::new(
            3,
            0,
            &[
                DecisionEdge {
                    from: None,
                    to: 2,
                    all: 9,
                    none: 0,
                    after: 0.,
                },
                DecisionEdge {
                    from: None,
                    to: 1,
                    all: 1,
                    none: 8,
                    after: 0.,
                },
                DecisionEdge {
                    from: None,
                    to: 0,
                    all: 0,
                    none: 1,
                    after: 0.,
                },
            ],
        )
        .unwrap();
        let mut grid = Grid::new(WIDTH, HEIGHT).unwrap();
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let cell = Cell { x, y };
                let p = point(cell);
                let blocked = p.x < ARENA.x + 22.
                    || p.x > ARENA.right() - 22.
                    || p.y < ARENA.y + 22.
                    || p.y > ARENA.bottom() - 22.
                    || COVER
                        .iter()
                        .any(|r| Rect::new(r.x - 25., r.y - 25., r.w + 50., r.h + 50.).contains(p));
                grid.set_blocked(cell, blocked).unwrap();
            }
        }
        let brains = (0..CAP)
            .map(|_| Brain {
                memory: Memory::new(MemorySettings {
                    capacity: 1,
                    sight_seconds: 2.,
                    hearing_seconds: 2.5,
                })
                .unwrap(),
                state: graph.initial(),
                route: Route3::new(128).unwrap(),
                follower: WaypointFollower::default(),
                replan: 0.,
            })
            .collect();
        Self {
            brains,
            graph,
            grid,
            planner: PathPlanner::new((WIDTH * HEIGHT) as usize, 16).unwrap(),
            schedule: RoundRobin::default(),
            noises: NoiseBuffer::new(4).unwrap(),
            peers: [None; CAP],
            work: WorkCounts::default(),
        }
    }
    pub fn reset_agent(&mut self, slot: usize) {
        let b = &mut self.brains[slot];
        b.memory.clear();
        b.route.clear();
        b.follower.reset();
        b.state.reset(&self.graph);
        b.replan = 0.;
        self.planner.cancel(slot as u64);
    }
    pub fn update(&mut self, world: &World, player: Vec2, shot: bool, dt: f32) {
        self.noises.advance(dt).unwrap();
        if shot {
            self.noises
                .push(
                    Noise {
                        source: 1000,
                        position: player.extend(0.),
                        groups: 1,
                        radius: 800.,
                    },
                    0.12,
                )
                .unwrap();
        }
        self.peers.fill(None);
        let mut count = 0;
        for (p, e) in &mut world.query::<(&Position, &Enemy)>() {
            if count < CAP {
                self.peers[count] = Some((e.ai_slot, p.0));
                count += 1;
            }
        }
        let mut budget = WorkBudget::new(128, 24, 192);
        for (slot, _) in self.peers[..count].iter().flatten() {
            let b = &mut self.brains[*slot];
            b.memory.advance(dt).unwrap();
            b.replan = (b.replan - dt).max(0.);
        }
        for index in self.schedule.next(count, 4) {
            let (slot, p) = self.peers[index].unwrap();
            let b = &mut self.brains[slot];
            let target = Candidate {
                id: 1000,
                position: player.extend(0.),
                groups: 1,
            };
            if Sight::new(500., std::f32::consts::TAU, 1)
                .unwrap()
                .test(
                    Observer {
                        position: p.extend(0.),
                        forward: Vec3::X,
                    },
                    target,
                    &mut budget,
                    &mut CoverSight,
                )
                .unwrap()
                == Detection::Detected
            {
                b.memory.observe(target, Sense::Sight).unwrap();
            }
            for noise in self.noises.iter() {
                if Hearing::new(800., 1, 0.65)
                    .unwrap()
                    .test(p.extend(0.), noise, &mut budget, Some(&mut CoverSight))
                    .unwrap()
                    == Detection::Detected
                {
                    b.memory
                        .observe(
                            Candidate {
                                id: noise.source,
                                position: noise.position,
                                groups: noise.groups,
                            },
                            Sense::Hearing,
                        )
                        .unwrap();
                }
            }
            let goal = b
                .memory
                .best(1)
                .map_or(vec2(640., 414.), |t| t.position.truncate());
            if b.replan == 0.
                && let (Some(start), Some(end)) =
                    (nearest(&self.grid, p), nearest(&self.grid, goal))
            {
                let status = self.planner.request(PathRequest {
                    agent: slot as u64,
                    start,
                    end,
                    connectivity: Connectivity::Eight,
                });
                if status != RequestStatus::Full {
                    b.replan = 0.7;
                }
            }
        }
        for (slot, p) in self.peers[..count].iter().flatten() {
            let b = &mut self.brains[*slot];
            let memory = b.memory.best(1);
            let flags = if let Some(t) = memory {
                1 | if t.visible && t.position.truncate().distance(*p) < 48. {
                    8
                } else {
                    0
                }
            } else {
                0
            };
            b.state.update(&self.graph, flags, dt).unwrap();
        }
        self.planner.step(&self.grid, &mut budget);
        if let Some((request, result)) = self.planner.result() {
            if self.peers[..count]
                .iter()
                .flatten()
                .any(|(s, _)| *s == request.agent as usize)
            {
                let b = &mut self.brains[request.agent as usize];
                if let Ok(route) = result
                    && route.len() <= 128
                {
                    let mut points = [Vec3::ZERO; 128];
                    for (out, cell) in points.iter_mut().zip(route.iter().rev()) {
                        *out = point(*cell).extend(0.);
                    }
                    b.route.set(&points[..route.len()]).unwrap();
                    b.follower.reset();
                }
            }
            self.planner.acknowledge();
        }
        self.work = budget.counts();
    }
    pub fn velocity(&mut self, slot: usize, position: Vec2, speed: f32) -> Vec2 {
        let b = &mut self.brains[slot];
        let track = b.memory.best(1);
        let goal = if b.state.state() == 0 {
            vec2(640., 414.)
        } else {
            track.map_or(position, |t| t.position.truncate())
        };
        let direct = track.is_some_and(|t| t.visible);
        let target = if direct {
            goal
        } else {
            b.follower
                .target(&b.route, position.extend(0.), 12., 8)
                .unwrap()
                .map_or(goal, |p| p.truncate())
        };
        let toward = (target - position).normalize_or_zero();
        // Scan the fixed snapshot, retaining at most eight nearest peers.
        let mut peers = [[0.; 2]; 8];
        let mut distances = [f32::INFINITY; 8];
        let mut count = 0;
        for (peer_slot, peer) in self.peers.iter().flatten() {
            if *peer_slot == slot {
                continue;
            }
            let d = peer.distance_squared(position);
            if d >= 48. * 48. {
                continue;
            }
            if count < 8 {
                peers[count] = peer.to_array();
                distances[count] = d;
                count += 1;
            } else {
                let worst = (0..8)
                    .max_by(|a, b| distances[*a].total_cmp(&distances[*b]))
                    .unwrap();
                if d < distances[worst] {
                    peers[worst] = peer.to_array();
                    distances[worst] = d;
                }
            }
        }
        let repel = Vec2::from_array(separation(position.to_array(), &peers[..count], 48., 0.5));
        (toward + repel).normalize_or_zero() * speed
    }
}
fn point(cell: Cell) -> Vec2 {
    ORIGIN + vec2(cell.x as f32 + 0.5, cell.y as f32 + 0.5) * CELL
}
fn nearest(grid: &Grid, p: Vec2) -> Option<Cell> {
    let raw = (p - ORIGIN) / CELL;
    let x = (raw.x.floor() as i32).clamp(0, WIDTH as i32 - 1);
    let y = (raw.y.floor() as i32).clamp(0, HEIGHT as i32 - 1);
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
    #[test]
    fn heard_targets_route_around_cover_without_penetrating_it() {
        let mut world = World::new();
        let actor = world.spawn((
            Position(vec2(300., 284.)),
            Enemy {
                ai_slot: 0,
                health: 24.,
                radius: 14.,
                speed: 100.,
                heavy: false,
                flash: 0.,
            },
        ));
        let player = vec2(500., 284.);
        let mut ai = SwarmAi::new();
        ai.reset_agent(0);
        let mut acquired = false;
        for tick in 0..1000 {
            ai.update(&world, player, tick == 0, 1. / 120.);
            assert!(
                ai.work.queries <= 24 && ai.work.candidates <= 128 && ai.work.path_steps <= 192
            );
            acquired |= ai.brains[0].memory.best(1).is_some_and(|t| t.visible);
            let mut p = world.get::<&mut Position>(actor).unwrap();
            let velocity = ai.velocity(0, p.0, 100.);
            p.0 = crate::game::move_body(p.0, velocity / 120., 14.);
            assert!(p.0.is_finite());
            assert!(COVER.iter().all(|r| !r.contains(p.0)));
        }
        let p = world.get::<&Position>(actor).unwrap().0;
        assert!(acquired, "pursuer never saw target after routing");
        assert!(p.distance(player) < 55., "pursuer stuck at {p:?}");
    }
}
