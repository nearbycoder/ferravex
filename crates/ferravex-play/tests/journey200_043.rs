use ferravex_play::{objective::*, persistence::SaveData};

fn node(id: u16, all: u64, kind: ObjectiveKind) -> ObjectiveSpec {
    ObjectiveSpec {
        id,
        all,
        any: 0,
        exclusive_group: 0,
        kind,
    }
}
fn assert_read_only<const N: usize>(graph: &ObjectiveGraph<N>) {
    let snapshot = graph.snapshot();
    let bytes = snapshot.encode(42).unwrap();
    let serial = graph.last_event_serial();
    for view in graph.iter() {
        assert_eq!(graph.inspect(view.index), Some(view));
        assert_eq!(graph.state(view.index), Some(view.state));
        assert_eq!(
            graph.complete(view.index),
            view.state == ObjectiveState::Complete
        );
        match view.state {
            ObjectiveState::Locked => {
                assert_ne!(view.blockers.missing_all | view.blockers.missing_any, 0);
                assert_eq!(view.blockers.exclusive_winner, None);
            }
            ObjectiveState::Ready | ObjectiveState::Complete => {
                assert_eq!(view.blockers, ObjectiveBlockers::default());
            }
            ObjectiveState::Skipped => {
                assert!(graph.complete(view.blockers.exclusive_winner.unwrap()));
            }
        }
    }
    assert_eq!(graph.inspect(N), None);
    assert_eq!(graph.inspect(usize::MAX), None);
    assert_eq!(graph.last_event_serial(), serial);
    assert_eq!(graph.snapshot(), snapshot);
    assert_eq!(graph.snapshot().encode(42).unwrap(), bytes);
}
#[test]
fn inspection_tracks_all_any_unlocks_and_event_boundaries() {
    let mut dependent = node(40, 1, ObjectiveKind::Event { id: 5, count: 1 });
    dependent.any = 0b110;
    let mut graph = ObjectiveGraph::new(
        1,
        [
            node(10, 0, ObjectiveKind::Event { id: 5, count: 2 }),
            node(20, 0, ObjectiveKind::Event { id: 7, count: 1 }),
            node(30, 0, ObjectiveKind::Event { id: 8, count: 1 }),
            dependent,
            node(50, 0b1000, ObjectiveKind::Gate),
        ],
    )
    .unwrap();
    assert_eq!(
        graph.inspect(3).unwrap().blockers,
        ObjectiveBlockers {
            missing_all: 1,
            missing_any: 0b110,
            ..Default::default()
        }
    );
    assert_read_only(&graph);
    graph.event(5, 1, 1).unwrap();
    assert_eq!(
        graph.inspect(0).unwrap().progress,
        ObjectiveProgress::Event {
            count: 1,
            required: 2
        }
    );
    assert_read_only(&graph);
    graph.event(7, 1, 2).unwrap();
    assert_eq!(graph.inspect(3).unwrap().blockers.missing_all, 1);
    assert_eq!(graph.inspect(3).unwrap().blockers.missing_any, 0);
    assert_read_only(&graph);
    assert_eq!(graph.event(5, 1, 3).unwrap().completed, 1);
    let unlocked = graph.inspect(3).unwrap();
    assert_eq!(unlocked.state, ObjectiveState::Ready);
    assert_eq!(
        unlocked.progress,
        ObjectiveProgress::Event {
            count: 0,
            required: 1
        }
    );
    assert_read_only(&graph);
    assert_eq!(graph.event(5, u16::MAX, 3).unwrap().completed, 0);
    assert_eq!(graph.inspect(3), Some(unlocked));
    graph.event(999, 1, 4).unwrap();
    assert_eq!(graph.inspect(3), Some(unlocked));
    assert_eq!(graph.last_event_serial(), 4);
    assert_eq!(graph.event(5, u16::MAX, 5).unwrap().completed, 0b11000);
    assert_eq!(graph.inspect(4).unwrap().progress, ObjectiveProgress::Gate);
    assert_read_only(&graph);
}
#[test]
fn inspection_distinguishes_skipped_prerequisites_and_exclusive_winners() {
    let mut a = node(10, 0, ObjectiveKind::Event { id: 1, count: 2 });
    a.exclusive_group = 255;
    let mut b = node(20, 0, ObjectiveKind::Event { id: 2, count: 1 });
    b.exclusive_group = 255;
    let mut any_branch = node(40, 0, ObjectiveKind::Gate);
    any_branch.any = 3;
    let mut only_loser = node(50, 0, ObjectiveKind::Gate);
    only_loser.any = 1;
    let mut either_pending = node(70, 0, ObjectiveKind::Gate);
    either_pending.any = 1 | (1 << 5);
    let mut graph = ObjectiveGraph::new(
        1,
        [
            a,
            b,
            node(30, 1, ObjectiveKind::Gate),
            any_branch,
            only_loser,
            node(60, 0, ObjectiveKind::Event { id: 3, count: 1 }),
            either_pending,
        ],
    )
    .unwrap();
    graph.event(1, 1, 1).unwrap();
    assert_read_only(&graph);
    let frame = graph.event(2, 1, 2).unwrap();
    assert_eq!(frame.completed, 0b1010);
    assert_eq!(frame.skipped, 1);
    let loser = graph.inspect(0).unwrap();
    assert_eq!(loser.state, ObjectiveState::Skipped);
    assert_eq!(loser.blockers.exclusive_winner, Some(1));
    assert_eq!(
        loser.progress,
        ObjectiveProgress::Event {
            count: 1,
            required: 2
        }
    );
    assert_eq!(graph.inspect(1).unwrap().blockers.exclusive_winner, None);
    assert_eq!(
        graph.inspect(2).unwrap().blockers,
        ObjectiveBlockers {
            missing_all: 1,
            skipped_prerequisites: 1,
            ..Default::default()
        }
    );
    assert_eq!(graph.inspect(3).unwrap().state, ObjectiveState::Complete);
    assert_eq!(
        graph.inspect(4).unwrap().blockers,
        ObjectiveBlockers {
            missing_any: 1,
            skipped_prerequisites: 1,
            ..Default::default()
        }
    );
    assert_eq!(graph.inspect(6).unwrap().blockers.missing_any, 1 | (1 << 5));
    assert_eq!(graph.inspect(6).unwrap().blockers.skipped_prerequisites, 1);
    assert_read_only(&graph);
    assert_eq!(
        graph.event(1, u16::MAX, 3).unwrap(),
        ObjectiveFrame::default()
    );
    assert_eq!(graph.inspect(0), Some(loser));
    assert_eq!(graph.event(3, 1, 4).unwrap().completed, (1 << 5) | (1 << 6));
    assert_read_only(&graph);
}
#[test]
fn timer_views_preserve_new_unlock_boundaries_and_restore() {
    let specs = [
        node(10, 0, ObjectiveKind::Timer { seconds: 0.5 }),
        node(20, 1, ObjectiveKind::Timer { seconds: 1. }),
        node(30, 2, ObjectiveKind::Gate),
    ];
    let mut graph = ObjectiveGraph::new(1, specs).unwrap();
    assert_read_only(&graph);
    assert_eq!(graph.advance(1.).unwrap().completed, 1);
    let unlocked = graph.inspect(1).unwrap();
    assert_eq!(unlocked.state, ObjectiveState::Ready);
    assert_eq!(
        unlocked.progress,
        ObjectiveProgress::Timer {
            elapsed: 0.,
            seconds: 1.
        }
    );
    assert_read_only(&graph);
    graph.advance(0.25).unwrap();
    assert_eq!(
        graph.inspect(1).unwrap().progress,
        ObjectiveProgress::Timer {
            elapsed: 0.25,
            seconds: 1.
        }
    );
    graph.event(999, 1, 100).unwrap();
    let bytes = graph.snapshot().encode(42).unwrap();
    let mut restored = ObjectiveGraph::new(1, specs).unwrap();
    restored
        .restore(ObjectiveSnapshot::decode(&bytes).unwrap().0)
        .unwrap();
    assert!(graph.iter().eq(restored.iter()));
    assert_read_only(&restored);
    let before = restored.snapshot();
    assert_eq!(restored.advance(f32::NAN), Err(ObjectiveError::Invalid));
    assert_eq!(restored.event(1, 0, 101), Err(ObjectiveError::Invalid));
    assert_eq!(restored.event(1, 1, 0), Err(ObjectiveError::Invalid));
    assert_eq!(restored.snapshot(), before);
    assert_eq!(
        restored.event(999, 1, 100).unwrap(),
        ObjectiveFrame::default()
    );
    assert_eq!(graph.advance(60.).unwrap(), restored.advance(60.).unwrap());
    assert_eq!(graph.snapshot(), restored.snapshot());
    assert!(
        graph
            .iter()
            .all(|view| view.state == ObjectiveState::Complete)
    );
    assert_eq!(
        graph.inspect(1).unwrap().progress,
        ObjectiveProgress::Timer {
            elapsed: 1.,
            seconds: 1.
        }
    );
    assert_eq!(graph.last_event_serial(), 100);
    assert_read_only(&graph);
}
#[test]
fn full_timer_progress_does_not_misreport_a_skipped_branch_as_complete() {
    let mut slow = node(10, 0, ObjectiveKind::Timer { seconds: 0.5 });
    slow.exclusive_group = 1;
    let mut fast = node(20, 0, ObjectiveKind::Timer { seconds: 0.25 });
    fast.exclusive_group = 1;
    let mut graph = ObjectiveGraph::new(1, [slow, fast]).unwrap();
    graph.advance(1.).unwrap();
    let skipped = graph.inspect(0).unwrap();
    assert_eq!(skipped.state, ObjectiveState::Skipped);
    assert_eq!(skipped.blockers.exclusive_winner, Some(1));
    assert_eq!(
        skipped.progress,
        ObjectiveProgress::Timer {
            elapsed: 0.5,
            seconds: 0.5
        }
    );
    assert_eq!(graph.inspect(1).unwrap().state, ObjectiveState::Complete);
    assert_read_only(&graph);
}
#[test]
fn inspection_does_not_resettle_existing_exclusive_gates() {
    let mut event = node(10, 0, ObjectiveKind::Event { id: 5, count: 2 });
    event.exclusive_group = 1;
    let mut gate = node(20, 0, ObjectiveKind::Gate);
    gate.exclusive_group = 1;
    // The existing one-pass settlement leaves the earlier event ready when
    // this later gate wins. A read-only view must preserve that saved state,
    // not silently settle the graph or report the event as already skipped.
    let mut graph = ObjectiveGraph::new(1, [event, gate]).unwrap();
    assert_eq!(graph.inspect(0).unwrap().state, ObjectiveState::Ready);
    assert_eq!(graph.inspect(1).unwrap().state, ObjectiveState::Complete);
    assert_read_only(&graph);
    graph.event(5, 1, 1).unwrap();
    assert_eq!(
        graph.inspect(0).unwrap().progress,
        ObjectiveProgress::Event {
            count: 1,
            required: 2
        }
    );
    assert_read_only(&graph);
}
#[test]
fn iteration_uses_declaration_slots_and_checks_bounds_at_capacity() {
    let mut specs = std::array::from_fn::<_, 64, _>(|i| {
        node(1000 - i as u16, 0, ObjectiveKind::Event { id: 5, count: 1 })
    });
    specs[0].all = 1 << 63;
    specs[0].kind = ObjectiveKind::Gate;
    let mut graph = ObjectiveGraph::new(1, specs).unwrap();
    assert_eq!(graph.inspect(0).unwrap().blockers.missing_all, 1 << 63);
    {
        let mut views = graph.iter();
        assert_eq!(views.len(), 64);
        assert_eq!(views.next().unwrap().index, 0);
        assert_eq!(views.next_back().unwrap().index, 63);
        assert_eq!(views.len(), 62);
        for (i, view) in views.by_ref().enumerate() {
            assert_eq!(view.index, i + 1);
            assert_eq!(view.spec.id, 999 - i as u16);
        }
        assert_eq!(views.len(), 0);
        assert_eq!(views.next(), None);
        assert_eq!(views.next_back(), None);
        assert_eq!(views.next(), None);
    }
    assert_read_only(&graph);
    assert_eq!(graph.event(5, 1, 1).unwrap().completed, u64::MAX);
    assert_read_only(&graph);
}
#[test]
fn event_progress_saturates_at_the_largest_authored_target() {
    let mut graph = ObjectiveGraph::new(
        1,
        [node(
            10,
            0,
            ObjectiveKind::Event {
                id: 5,
                count: u16::MAX,
            },
        )],
    )
    .unwrap();
    graph.event(5, u16::MAX - 1, 1).unwrap();
    assert_eq!(
        graph.inspect(0).unwrap().progress,
        ObjectiveProgress::Event {
            count: u16::MAX - 1,
            required: u16::MAX
        }
    );
    assert_read_only(&graph);
    graph.event(5, u16::MAX, 2).unwrap();
    assert_eq!(
        graph.inspect(0).unwrap().progress,
        ObjectiveProgress::Event {
            count: u16::MAX,
            required: u16::MAX
        }
    );
    assert_eq!(graph.inspect(0).unwrap().state, ObjectiveState::Complete);
    assert_read_only(&graph);
}
