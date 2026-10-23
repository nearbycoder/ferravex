//! Executable first-party fixed-tick recipe and warmed allocation probe.
//! Headless/native PCM evidence only; no device or listening claim.
use ferravex_core::session::GameSession;
use ferravex_play::{
    feedback::{FeedbackConfig, FeedbackController, FeedbackEvent},
    gameplay::magazine::Magazine,
    input::{ActionMap, Binding, InputCode},
    objective::{ObjectiveGraph, ObjectiveKind, ObjectiveSpec},
};
use ferravex_render::{
    audio::{
        self, Cue, GameAudio,
        batch::{Coalesce, CueBatch, CueCategory, QueueOutcome},
    },
    prelude::Vec3,
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};

const FIRE: CueCategory = CueCategory {
    key: 1,
    cue: Cue::Fire,
    policy: Coalesce::Strongest,
    critical: false,
};
const TERMINAL: CueCategory = CueCategory {
    key: 2,
    cue: Cue::Success,
    policy: Coalesce::First,
    critical: true,
};
fn objective() -> ObjectiveGraph<1> {
    // A tiny authored survival timer, not a forced game phase/result.
    ObjectiveGraph::new(
        297,
        [ObjectiveSpec {
            id: 1,
            all: 0,
            any: 0,
            exclusive_group: 0,
            kind: ObjectiveKind::Timer { seconds: 3. / 64. },
        }],
    )
    .unwrap()
}

#[test]
fn fixed_tick_accepted_burst_one_flush_empty_pause_reset_and_real_retry() {
    audio::stop_all();
    audio::set_muted(false);
    let mut session = GameSession::new(64., 4, true);
    let mut ammo = Magazine::new(3, 0, 1.).unwrap();
    let mut rules = objective();
    let mut feedback = FeedbackController::<8>::new(FeedbackConfig::default(), 297).unwrap();
    let mut input = ActionMap::default();
    let code = InputCode::Key(32);
    let fire = input.try_action(vec![Binding::digital(code)]).unwrap();
    let mut owner = GameAudio::new();
    input.feed(code, 1.);
    {
        let mut cues = CueBatch::<2>::new(&mut owner).unwrap();
        assert_eq!(session.advance(0.).steps, 0);
        assert!(cues.is_empty());
        assert_eq!(cues.flush().submitted, 0);
        // The pending edge is not consumed on a zero-tick frame.
    }
    {
        let mut cues = CueBatch::<2>::new(&mut owner).unwrap();
        let ticks = session.advance_batch(4. / 64.);
        let mut executed = 0;
        let mut presses = 0;
        for tick in ticks.ticks() {
            if !session.is_playing() {
                break;
            }
            executed += 1;
            let action = input.consume_tick(fire);
            presses += usize::from(action.pressed);
            let accepted = action.value > 0.5 && ammo.fire();
            let event = FeedbackEvent::fire(Vec3::ZERO, 1.).unwrap();
            if feedback
                .emit_if_accepted(tick.tick, accepted, event)
                .unwrap()
                .is_some()
            {
                let _ = cues.queue(FIRE, None, 0.4).unwrap();
            }
            // Feedback's existing ID history owns duplicate filtering, not batch.
            assert!(
                feedback
                    .emit_if_accepted(tick.tick, accepted, event)
                    .unwrap()
                    .is_none()
            );
            if rules.advance(tick.dt).unwrap().completed != 0 {
                assert!(session.finish(true));
                assert_eq!(cues.queue(TERMINAL, None, 0.6), Ok(QueueOutcome::Retained));
            }
        }
        assert_eq!(
            (session.tick(), executed, presses, ammo.loaded()),
            (4, 3, 1, 0)
        );
        assert_eq!(ticks.frame().steps, 4); // Issued count includes unexecuted tail.
        let report = cues.flush();
        assert_eq!(
            (
                report.batch.retained,
                report.batch.coalesced,
                report.submitted,
                report.admitted()
            ),
            (2, 2, 2, 2)
        );
        assert_eq!(report.outcomes[1].unwrap().request.category, TERMINAL);
        assert_eq!(cues.flush().submitted, 0);
        assert_eq!(session.advance(0.25).steps, 0);
    }
    let mut pcm = [[0.; 2]; 512];
    assert!(audio::render(&mut pcm, audio::SAMPLE_RATE));
    assert!(pcm.iter().flatten().all(|v| v.is_finite()));
    assert!(pcm.iter().flatten().any(|v| v.abs() > 0.001));
    {
        let mut cues = CueBatch::<2>::new(&mut owner).unwrap();
        let _ = cues.queue(FIRE, None, 0.4).unwrap();
        assert_eq!(cues.pause(), 1);
        assert_eq!(cues.flush().batch.cancelled, 1);
        let _ = cues.queue(TERMINAL, None, 0.6).unwrap();
        assert_eq!(cues.reset(), 1);
        assert_eq!(cues.flush().submitted, 0);
    }
    assert!(owner.is_paused());
    session.restart();
    ammo = Magazine::new(3, 0, 1.).unwrap();
    rules = objective();
    feedback.clear();
    // This recipe is a local session. An authoritative network/replay clock
    // would continue while only local input/presentation is suspended.
    assert!(session.set_paused(true));
    {
        let mut cues = CueBatch::<1>::new(&mut owner).unwrap();
        cues.pause();
        assert_eq!(session.advance(30.).steps, 0);
        assert_eq!(session.tick(), 0);
        assert_eq!(cues.flush().submitted, 0);
    }
    assert!(session.set_paused(false));
    input.feed(code, 0.);
    input.feed(code, 1.);
    owner.resume();
    {
        let mut cues = CueBatch::<1>::new(&mut owner).unwrap();
        assert_eq!(cues.flush().submitted, 0);
        for tick in session.advance_batch(4. / 64.).ticks() {
            if !session.is_playing() {
                break;
            }
            let accepted = input.consume_tick(fire).value > 0.5 && ammo.fire();
            let event = FeedbackEvent::fire(Vec3::ZERO, 1.).unwrap();
            if feedback
                .emit_if_accepted(tick.tick, accepted, event)
                .unwrap()
                .is_some()
            {
                let _ = cues.queue(FIRE, None, 0.4).unwrap();
            }
            if rules.advance(tick.dt).unwrap().completed != 0 {
                assert!(session.finish(true));
                assert!(
                    matches!(cues.queue(TERMINAL, None, 0.6), Ok(QueueOutcome::Evicted(old)) if old.category == FIRE)
                );
            }
        }
        let report = cues.flush();
        assert_eq!(
            (
                report.submitted,
                report.admitted(),
                report.batch.evicted,
                report.batch.coalesced
            ),
            (1, 1, 1, 2)
        );
        assert_eq!(report.outcomes[0].unwrap().request.category, TERMINAL);
        assert_eq!(cues.flush().submitted, 0);
    }
    owner.reset();
    assert_eq!(audio::stats().active, 0);
}

thread_local! { static ALLOCATIONS: Cell<Option<usize>> = const { Cell::new(None) }; }
struct CountingAllocator;
fn allocated() {
    let _ = ALLOCATIONS.try_with(|count| {
        if let Some(n) = count.get() {
            count.set(Some(n + 1));
        }
    });
}
// SAFETY: Unchanged allocator arguments and results are forwarded to System.
// The thread-local counter uses Cell and does not allocate or retain pointers.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        allocated();
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        allocated();
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        allocated();
        unsafe { System.realloc(ptr, layout, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;
struct StopCounting;
impl Drop for StopCounting {
    fn drop(&mut self) {
        ALLOCATIONS.with(|count| count.set(None));
    }
}

#[test]
fn warmed_collection_flush_cancel_pause_reset_and_pcm_allocate_nothing() {
    let mut owner = GameAudio::new();
    let run = |owner: &mut GameAudio| {
        owner.resume();
        let mut cues = CueBatch::<5>::new(owner).unwrap();
        for key in 1..=5 {
            let category = CueCategory { key, ..FIRE };
            let position = Some(Vec3::new(key as f32, 0., 0.));
            let _ = cues.queue(category, position, 0.3).unwrap();
            let _ = cues.queue(category, position, 0.4).unwrap();
        }
        let _ = cues
            .queue(CueCategory { key: 6, ..TERMINAL }, None, 0.6)
            .unwrap();
        let _ = cues
            .queue(CueCategory { key: 7, ..FIRE }, None, 0.4)
            .unwrap();
        let report = cues.flush();
        assert_eq!(report.batch.evicted, 1);
        assert_eq!(report.batch.dropped, 1);
        assert_eq!(report.submitted, 5);
        assert_eq!(cues.flush().submitted, 0);
        let _ = cues.queue(FIRE, None, 0.4).unwrap();
        assert_eq!(cues.cancel(), 1);
        let _ = cues.queue(FIRE, None, 0.4).unwrap();
        assert_eq!(cues.pause(), 1);
        let _ = cues.queue(FIRE, None, 0.4).unwrap();
        assert_eq!(cues.flush().suppressed, 1);
        cues.reset();
        let mut pcm = [[0.; 2]; 512];
        assert!(audio::render(&mut pcm, audio::SAMPLE_RATE));
        assert!(pcm.iter().flatten().all(|v| v.is_finite()));
    };
    run(&mut owner);
    ALLOCATIONS.with(|count| count.set(Some(0)));
    let guard = StopCounting;
    for _ in 0..128 {
        run(&mut owner);
    }
    let count = ALLOCATIONS.with(|count| count.get().unwrap());
    drop(guard);
    assert_eq!(count, 0);
}
