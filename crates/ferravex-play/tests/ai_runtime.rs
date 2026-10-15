use ferravex_math::Vec3;
use ferravex_play::{ai::*, navigation::*};
fn c(x: u32, y: u32) -> Cell {
    Cell { x, y }
}
fn candidate(id: u64, x: f32, z: f32) -> Candidate {
    Candidate {
        id,
        position: Vec3::new(x, 0., z),
        groups: 2,
    }
}
fn observer() -> Observer {
    Observer {
        position: Vec3::ZERO,
        forward: Vec3::Z,
    }
}
struct Wall {
    calls: u32,
    clear: bool,
}
impl VisibilityQuery for Wall {
    fn clear(&mut self, _: Vec3, _: Vec3) -> bool {
        self.calls += 1;
        self.clear
    }
}
#[test]
fn sight_applies_cones_masks_range_occlusion_and_deferred_work() {
    let sight = Sight::new(10., std::f32::consts::FRAC_PI_2, 2).unwrap();
    let mut b = WorkBudget::new(10, 2, 0);
    let mut wall = Wall {
        calls: 0,
        clear: true,
    };
    assert_eq!(
        sight.test(observer(), candidate(1, 0., 10.), &mut b, &mut wall),
        Ok(Detection::Detected)
    );
    assert_eq!(
        sight.test(observer(), candidate(1, 0., -1.), &mut b, &mut wall),
        Ok(Detection::Rejected)
    );
    assert_eq!(
        sight.test(observer(), candidate(1, 11., 0.), &mut b, &mut wall),
        Ok(Detection::Rejected)
    );
    assert_eq!(
        sight.test(
            observer(),
            Candidate {
                groups: 1,
                ..candidate(1, 0., 1.)
            },
            &mut b,
            &mut wall
        ),
        Ok(Detection::Rejected)
    );
    wall.clear = false;
    assert_eq!(
        sight.test(observer(), candidate(1, 0., 1.), &mut b, &mut wall),
        Ok(Detection::Rejected)
    );
    wall.clear = true;
    assert_eq!(
        sight.test(observer(), candidate(1, 0., 1.), &mut b, &mut wall),
        Ok(Detection::Deferred)
    );
    assert_eq!(wall.calls, 2);
    assert_eq!(
        sight.test(
            Observer {
                forward: Vec3::ZERO,
                ..observer()
            },
            candidate(1, 0., 1.),
            &mut b,
            &mut wall
        ),
        Err(AiError::InvalidInput)
    );
    let mut b = WorkBudget::new(1, 1, 0);
    let full = Sight::new(f32::MAX, std::f32::consts::TAU, 2).unwrap();
    assert_eq!(
        full.test(
            Observer {
                position: Vec3::splat(-f32::MAX),
                ..observer()
            },
            candidate(1, f32::MAX, 0.),
            &mut b,
            &mut wall
        ),
        Ok(Detection::Rejected)
    );
}
#[test]
fn hearing_memory_expiry_and_capacity_do_not_manufacture_sight() {
    let hearing = Hearing::new(20., 2, 0.25).unwrap();
    let mut b = WorkBudget::new(20, 20, 0);
    let mut wall = Wall {
        calls: 0,
        clear: false,
    };
    let noise = Noise {
        source: 1,
        position: Vec3::Z * 6.,
        groups: 2,
        radius: 20.,
    };
    assert_eq!(
        hearing.test(Vec3::ZERO, noise, &mut b, Some(&mut wall)),
        Ok(Detection::Rejected)
    );
    assert_eq!(
        hearing.test(
            Vec3::ZERO,
            Noise {
                position: Vec3::Z * 5.,
                ..noise
            },
            &mut b,
            Some(&mut wall)
        ),
        Ok(Detection::Detected)
    );
    assert_eq!(
        hearing.test(Vec3::ZERO, noise, &mut b, None),
        Ok(Detection::Detected)
    );
    let mut memory = Memory::new(MemorySettings {
        capacity: 1,
        sight_seconds: 1.,
        hearing_seconds: 2.,
    })
    .unwrap();
    memory.observe(candidate(1, 0., 1.), Sense::Sight).unwrap();
    memory
        .observe(candidate(1, 0., 10.), Sense::Hearing)
        .unwrap();
    assert_eq!(memory.best(2).unwrap().position, Vec3::Z);
    assert!(!memory.observe(candidate(2, 0., 1.), Sense::Sight).unwrap());
    assert_eq!(memory.dropped(), 1);
    memory.advance(0.5).unwrap();
    assert!(!memory.best(2).unwrap().visible);
    memory
        .observe(candidate(1, 0., 3.), Sense::Hearing)
        .unwrap();
    assert!(!memory.best(2).unwrap().visible);
    memory.advance(1.).unwrap();
    memory.advance(1.).unwrap();
    assert!(memory.tracks().is_empty());
    let mut sounds = NoiseBuffer::new(2).unwrap();
    for source in 0..3 {
        sounds.push(Noise { source, ..noise }, 0.5).unwrap();
    }
    assert_eq!(sounds.iter().map(|n| n.source).collect::<Vec<_>>(), [1, 2]);
    assert_eq!(sounds.dropped(), 1);
    sounds.advance(0.5).unwrap();
    assert_eq!(sounds.iter().len(), 0);
    assert!(
        sounds
            .push(
                Noise {
                    radius: f32::NAN,
                    ..noise
                },
                1.
            )
            .is_err()
    );
}
#[test]
fn priority_selection_stops_unbounded_candidates_and_breaks_ties_by_id() {
    let sight = Sight::new(10., std::f32::consts::TAU, 2).unwrap();
    let mut b = WorkBudget::new(3, 3, 0);
    let result = select_target(
        std::iter::repeat(candidate(3, 0., 1.)),
        observer(),
        sight,
        &mut b,
        &mut Unoccluded,
        |_| 1.,
    )
    .unwrap();
    assert_eq!(result.target.unwrap().id, 3);
    assert!(result.incomplete);
    assert_eq!(b.counts().candidates, 3);
    let mut b = WorkBudget::new(8, 8, 0);
    let result = select_target(
        [
            candidate(5, 0., 1.),
            candidate(2, 0., 1.),
            candidate(1, 0., 1.),
        ],
        observer(),
        sight,
        &mut b,
        &mut Unoccluded,
        |c| if c.id == 1 { f32::NAN } else { 1. },
    )
    .unwrap();
    assert_eq!(result.target.unwrap().id, 2);
    assert!(!result.incomplete);
}
#[test]
fn decision_priority_interrupts_timers_and_reset_are_deterministic() {
    let graph = DecisionGraph::new(
        3,
        0,
        &[
            DecisionEdge {
                from: None,
                to: 2,
                all: 4,
                none: 0,
                after: 0.,
            },
            DecisionEdge {
                from: Some(0),
                to: 1,
                all: 1,
                none: 2,
                after: 0.5,
            },
            DecisionEdge {
                from: Some(1),
                to: 0,
                all: 2,
                none: 0,
                after: 0.,
            },
        ],
    )
    .unwrap();
    let mut a = graph.initial();
    let mut b = graph.initial();
    a.update(&graph, 1, 0.25).unwrap();
    assert_eq!(a.state(), 0);
    a.update(&graph, 1, 0.25).unwrap();
    b.update(&graph, 1, 0.5).unwrap();
    assert_eq!(a, b);
    a.update(&graph, 6, 0.).unwrap();
    assert_eq!(a.state(), 2);
    a.reset(&graph);
    assert_eq!(a.state(), 0);
    assert!(a.update(&graph, 0, f32::NAN).is_err());
    assert!(
        DecisionGraph::new(
            2,
            0,
            &[DecisionEdge {
                from: Some(0),
                to: 1,
                all: 1,
                none: 1,
                after: 0.
            }]
        )
        .is_err()
    );
    let mut scheduler = RoundRobin::default();
    let slots: Vec<_> = (0..4).flat_map(|_| scheduler.next(5, 2)).collect();
    assert_eq!(slots, [0, 1, 2, 3, 4, 0, 1, 2]);
    assert_eq!(scheduler.next(0, 4).count(), 0);
    assert_eq!(scheduler.next(2, 99).count(), 2);
}
#[test]
fn incremental_search_matches_weighted_astar_and_charges_reconstruction() {
    let mut grid = Grid::new(17, 11).unwrap();
    for y in 0..10 {
        grid.set_blocked(c(8, y), true).unwrap();
    }
    grid.set_cost(c(4, 10), 50).unwrap();
    for connectivity in [Connectivity::Four, Connectivity::Eight] {
        let expected = grid.path(c(1, 1), c(15, 1), connectivity).unwrap();
        let mut scratch = SearchScratch::new(187).unwrap();
        scratch
            .begin(&grid, c(1, 1), c(15, 1), connectivity)
            .unwrap();
        let mut total = 0;
        while matches!(
            scratch.status(),
            SearchStatus::Searching | SearchStatus::Tracing
        ) {
            let mut budget = WorkBudget::new(0, 0, 3);
            let step = scratch.step_budgeted(&grid, &mut budget);
            assert!(step.work <= 3);
            assert_eq!(step.work, budget.counts().path_steps);
            total += step.work;
        }
        let route: Vec<_> = scratch
            .reverse_path()
            .unwrap()
            .iter()
            .rev()
            .copied()
            .collect();
        assert_eq!(route, expected);
        assert!(total > route.len() as u32);
        let mut short = [c(99, 99); 1];
        assert_eq!(
            scratch.copy_path(&mut short),
            Err(NavigationError::Capacity)
        );
        assert_eq!(short, [c(99, 99)]);
    }
}
#[test]
fn changing_grids_invalidates_active_and_finished_paths() {
    let mut a = Grid::new(3, 3).unwrap();
    let b = Grid::new(3, 3).unwrap();
    let mut s = SearchScratch::new(9).unwrap();
    s.begin(&a, c(0, 0), c(2, 2), Connectivity::Eight).unwrap();
    assert_eq!(s.step(&b, 1).status, SearchStatus::Stale);
    s.begin(&a, c(0, 0), c(2, 2), Connectivity::Eight).unwrap();
    assert_eq!(s.step(&a, 100).status, SearchStatus::Found);
    a.set_blocked(c(1, 1), true).unwrap();
    assert_eq!(s.step(&a, 0).status, SearchStatus::Stale);
    assert!(s.reverse_path().is_none());
    a.set_blocked(c(1, 0), true).unwrap();
    a.set_blocked(c(0, 1), true).unwrap();
    s.begin(&a, c(0, 0), c(2, 2), Connectivity::Eight).unwrap();
    assert_eq!(s.step(&a, 100).status, SearchStatus::NoPath);
}
#[test]
fn planner_backpressure_dedup_fairness_and_stale_ids() {
    let grid = Grid::new(5, 5).unwrap();
    let mut p = PathPlanner::new(25, 2).unwrap();
    let r = PathRequest {
        agent: 1,
        start: c(0, 0),
        end: c(4, 4),
        connectivity: Connectivity::Eight,
    };
    assert_eq!(p.request(r), RequestStatus::Queued);
    assert_eq!(
        p.request(PathRequest { end: c(3, 3), ..r }),
        RequestStatus::Updated
    );
    assert_eq!(
        p.request(PathRequest { agent: 2, ..r }),
        RequestStatus::Queued
    );
    assert_eq!(
        p.request(PathRequest { agent: 3, ..r }),
        RequestStatus::Full
    );
    assert_eq!(p.dropped(), 1);
    for id in [1, 2] {
        while p.result().is_none() {
            p.step(&grid, &mut WorkBudget::new(0, 0, 2));
        }
        let (request, path) = p.result().unwrap();
        assert_eq!(request.agent, id);
        assert!(path.is_ok());
        p.step(&grid, &mut WorkBudget::new(0, 0, 100));
        assert_eq!(p.result().unwrap().0.agent, id);
        p.acknowledge();
    }
    assert_eq!(p.pending(), 0);
    p.request(r);
    p.cancel(1);
    assert_eq!(p.pending(), 0);
}
#[test]
fn authored_paths_carry_distance_across_corners_and_skip_duplicates() {
    let mut route = Route3::new(8).unwrap();
    route
        .set(&[
            Vec3::ZERO,
            Vec3::ZERO,
            Vec3::X * 2.,
            Vec3::X * 2. + Vec3::Z * 3.,
        ])
        .unwrap();
    let mut cursor = PathCursor::default();
    let sample = cursor.advance(&route, 3., 1.).unwrap().unwrap();
    assert_eq!(sample.position, Vec3::new(2., 0., 1.));
    assert_eq!(sample.segment, 2);
    assert!(route.set(&[Vec3::splat(f32::NAN)]).is_err());
    assert_eq!(route.length(), 5.);
    let mut follower = WaypointFollower::default();
    assert_eq!(
        follower.target(&route, Vec3::ZERO, 0.1, 1).unwrap(),
        Some(Vec3::ZERO)
    );
    assert_eq!(
        follower.target(&route, Vec3::ZERO, 0.1, 8).unwrap(),
        Some(Vec3::X * 2.)
    );
    assert_eq!(
        follower.target(&route, Vec3::ZERO, 0.1, 8).unwrap(),
        Some(Vec3::X * 2.)
    );
    let end = cursor.advance(&route, 100., 1.).unwrap().unwrap();
    assert!(end.complete);
    assert_eq!(end.position, Vec3::new(2., 0., 3.));
    route
        .set(&[Vec3::splat(f32::MAX), Vec3::splat(-f32::MAX)])
        .unwrap();
    assert!(
        route
            .sample(route.length() * 0.5)
            .unwrap()
            .unwrap()
            .position
            .is_finite()
    );
}

#[test]
fn scratch_matches_reference_on_many_weighted_and_disconnected_maps() {
    let mut seed = 1234567_u64;
    let mut random = || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        (seed >> 32) as u32
    };
    let mut scratch = SearchScratch::new(100).unwrap();
    for _ in 0..48 {
        let mut grid = Grid::new(10, 10).unwrap();
        for y in 0..10 {
            for x in 0..10 {
                if (x, y) == (0, 0) || (x, y) == (9, 9) {
                    continue;
                }
                if random() % 5 == 0 {
                    grid.set_blocked(c(x, y), true).unwrap();
                } else {
                    grid.set_cost(c(x, y), (1 + random() % 20) as u16).unwrap();
                }
            }
        }
        for mode in [Connectivity::Four, Connectivity::Eight] {
            let expected = grid.path(c(0, 0), c(9, 9), mode);
            scratch.begin(&grid, c(0, 0), c(9, 9), mode).unwrap();
            while matches!(
                scratch.status(),
                SearchStatus::Searching | SearchStatus::Tracing
            ) {
                scratch.step(&grid, 7);
            }
            match expected {
                Ok(expected) => {
                    let result: Vec<_> = scratch
                        .reverse_path()
                        .unwrap()
                        .iter()
                        .rev()
                        .copied()
                        .collect();
                    assert_eq!(result, expected);
                }
                Err(NavigationError::NoPath) => assert_eq!(scratch.status(), SearchStatus::NoPath),
                Err(e) => panic!("unexpected {e:?}"),
            }
        }
    }
}

#[test]
fn matching_current_global_destination_holds_priority_without_oscillation() {
    let graph = DecisionGraph::new(
        2,
        0,
        &[
            DecisionEdge {
                from: None,
                to: 1,
                all: 1,
                none: 0,
                after: 0.,
            },
            DecisionEdge {
                from: None,
                to: 0,
                all: 0,
                none: 0,
                after: 0.,
            },
        ],
    )
    .unwrap();
    let mut state = graph.initial();
    state.update(&graph, 1, 0.).unwrap();
    assert_eq!(state.state(), 1);
    assert_eq!(state.update(&graph, 1, 0.5).unwrap(), None);
    assert_eq!(state.state(), 1);
    assert_eq!(state.elapsed(), 0.5);
    state.update(&graph, 0, 0.).unwrap();
    assert_eq!(state.state(), 0);
}
