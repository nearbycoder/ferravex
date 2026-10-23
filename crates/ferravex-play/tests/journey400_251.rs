//! Atomic choice contracts, legacy RNG fixtures and a real accepted-wave flow.
use ferravex_play::{
    gameplay::{
        LoopError,
        loot::RewardRng,
        progression::Progression,
        upgrades::{UpgradeChoiceError, UpgradeChoices, Upgrades},
        waves::{WaveSequence, WaveSpec},
    },
    input::{ActionMap, Binding, InputCode},
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    hint::black_box,
};

thread_local! {
    static ALLOCATIONS: Cell<Option<usize>> = const { Cell::new(None) };
}
fn count_allocation() {
    let _ = ALLOCATIONS.try_with(|count| {
        if let Some(value) = count.get() {
            count.set(Some(value + 1));
        }
    });
}
struct CountingAllocator;
// SAFETY: unchanged allocator arguments are forwarded to System. TLS observes
// calls on this test thread only and performs no allocation itself.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count_allocation();
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count_allocation();
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        count_allocation();
        unsafe { System.realloc(ptr, layout, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn allocations_during<T>(action: impl FnOnce() -> T) -> (T, usize) {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            ALLOCATIONS.with(|count| count.set(None));
        }
    }
    ALLOCATIONS.with(|count| {
        assert_eq!(count.get(), None);
        count.set(Some(0));
    });
    let reset = Reset;
    let result = action();
    let count = ALLOCATIONS.with(|count| count.get().unwrap());
    drop(reset);
    (result, count)
}

#[test]
fn legacy_constructors_apply_offers_and_exact_entropy_golden() {
    assert!(matches!(Upgrades::new([]), Err(LoopError::Invalid)));
    assert!(matches!(Upgrades::new([0]), Err(LoopError::Invalid)));
    assert!(matches!(Upgrades::new([1; 257]), Err(LoopError::Invalid)));
    let mut ranks = Upgrades::new([1, 2, 3, 4]).unwrap();
    let mut rng = RewardRng::new(7);
    assert_eq!(
        ranks.offers::<4>(&mut rng).unwrap(),
        [Some(3), Some(0), Some(2), Some(1)]
    );
    assert_eq!(rng.next_u64(), 0x73d3_3b66_6a1e_21da);
    let mut untouched = rng;
    assert_eq!(ranks.offers::<5>(&mut rng), Err(LoopError::Invalid));
    assert_eq!(ranks.offers::<0>(&mut rng).unwrap(), []);
    assert_eq!(rng.next_u64(), untouched.next_u64());
    assert_eq!(ranks.apply(0), Ok(1));
    assert_eq!(ranks.apply(0), Err(LoopError::Unavailable));
    assert_eq!(ranks.apply(4), Err(LoopError::Invalid));
    assert_eq!((ranks.rank(0), ranks.rank(4)), (Some(1), None));
    // Old rank storage remains independently Copy; session ownership is not.
    let copy = ranks;
    assert_eq!(copy.rank(0), ranks.rank(0));
}

#[test]
fn redraw_credit_and_rejected_choices_preserve_offer_and_rng() {
    let ranks = Upgrades::new([2; 4]).unwrap();
    let mut session = UpgradeChoices::<4, 2>::new(ranks, 11).unwrap();
    let mut rng = RewardRng::new(7);
    let mut mirror = rng;
    assert_eq!(session.open(&mut rng), Ok(None));
    session.credit(1).unwrap();
    let expected = ranks.offers::<2>(&mut mirror).unwrap();
    let offer = session.open(&mut rng).unwrap().unwrap();
    assert_eq!(offer.choices(), &expected);
    assert_eq!((offer.id().namespace(), offer.id().serial()), (11, 1));
    session.credit(2).unwrap();
    session.credit(0).unwrap();
    for _ in 0..8 {
        assert_eq!(session.open(&mut rng), Ok(Some(offer)));
    }
    let unoffered = (0..4).find(|id| !expected.contains(&Some(*id))).unwrap();
    for id in [unoffered, 4, usize::MAX] {
        assert_eq!(
            session.choose(offer.id(), id),
            Err(UpgradeChoiceError::NotOffered)
        );
    }
    assert_eq!(session.pending(), Some(&offer));
    assert_eq!(session.points(), 3);
    assert!((0..4).all(|id| session.upgrades().rank(id) == Some(0)));
    assert_eq!(rng.next_u64(), mirror.next_u64());
}

#[test]
fn repeated_clicks_keypresses_and_old_modal_cannot_spend_twice() {
    let mut session = UpgradeChoices::<2, 2>::new(Upgrades::new([3; 2]).unwrap(), 1).unwrap();
    let mut rng = RewardRng::new(9);
    session.credit(3).unwrap();
    let old = session.open(&mut rng).unwrap().unwrap();
    let id = old.choices()[0].unwrap();
    let accepted = session.choose(old.id(), id).unwrap();
    assert_eq!((accepted.offer, accepted.upgrade), (old.id(), id));
    assert_eq!((accepted.rank_before, accepted.rank_after), (0, 1));
    assert_eq!((accepted.points_before, accepted.points_after), (3, 2));
    assert_eq!(session.pending(), None);
    // Repeated UI dispatches with the original token, including another branch.
    for clicked in [id, old.choices()[1].unwrap(), id] {
        assert_eq!(
            session.choose(old.id(), clicked),
            Err(UpgradeChoiceError::StaleOffer)
        );
    }
    let new = session.open(&mut rng).unwrap().unwrap();
    assert_eq!(new.id().serial(), 2);
    assert_eq!(
        session.choose(old.id(), id),
        Err(UpgradeChoiceError::StaleOffer)
    );
    assert_eq!(session.pending(), Some(&new));
    assert_eq!(
        (session.points(), session.upgrades().rank(id)),
        (2, Some(1))
    );
}

#[test]
fn checked_credit_overflow_is_atomic_and_credit_is_explicitly_not_deduplicated() {
    let mut session = UpgradeChoices::<1, 1>::new(Upgrades::new([2]).unwrap(), 3).unwrap();
    assert_eq!(session.credit(u32::MAX), Ok(u32::MAX));
    let offer = session.open(&mut RewardRng::new(1)).unwrap().unwrap();
    assert_eq!(session.credit(1), Err(UpgradeChoiceError::BalanceOverflow));
    assert_eq!(
        session.credit(u32::MAX),
        Err(UpgradeChoiceError::BalanceOverflow)
    );
    assert_eq!(session.credit(0), Ok(u32::MAX));
    assert_eq!(session.pending(), Some(&offer));
    assert_eq!(session.upgrades().rank(0), Some(0));
    let accepted = session.choose(offer.id(), 0).unwrap();
    assert_eq!(accepted.points_after, u32::MAX - 1);
    assert_eq!(session.credit(1), Ok(u32::MAX));
    let reset = session.reset();
    assert_eq!(reset.discarded_points, u32::MAX);
    assert_eq!(session.credit(1), Ok(1));
    assert_eq!(session.credit(1), Ok(2)); // Caller must deduplicate awards.
}

#[test]
fn capped_short_zero_width_and_zero_balance_use_exact_entropy() {
    let mut ranks = Upgrades::new([1, 1, 2]).unwrap();
    ranks.apply(0).unwrap();
    ranks.apply(1).unwrap();
    let mut session = UpgradeChoices::<3, 3>::new(ranks, 1).unwrap();
    session.credit(3).unwrap();
    let mut rng = RewardRng::new(19);
    let mut mirror = rng;
    for rank in 1..=2 {
        let offer = session.open(&mut rng).unwrap().unwrap();
        assert_eq!(offer.choices(), &[Some(2), None, None]);
        mirror.next_u64(); // Even the one eligible choice uses one old RNG draw.
        assert_eq!(session.choose(offer.id(), 2).unwrap().rank_after, rank);
    }
    assert_eq!(session.open(&mut rng), Ok(None)); // All capped, balance retained.
    assert_eq!(session.points(), 1);
    assert_eq!(rng.next_u64(), mirror.next_u64());
    let reset = session.reset();
    assert_eq!(reset.discarded_points, 1);
    let mut mirror = rng;
    assert_eq!(session.open(&mut rng), Ok(None));
    assert_eq!(rng.next_u64(), mirror.next_u64());
    session.credit(1).unwrap();
    assert_eq!(session.open(&mut rng).unwrap().unwrap().id().serial(), 3);

    let mut zero = UpgradeChoices::<1, 0>::new(Upgrades::new([1]).unwrap(), 1).unwrap();
    zero.credit(1).unwrap();
    let mut mirror = rng;
    assert_eq!(zero.open(&mut rng), Ok(None));
    assert_eq!((zero.pending(), zero.points()), (None, 1));
    assert_eq!(rng.next_u64(), mirror.next_u64());
    assert!(matches!(
        UpgradeChoices::<1, 2>::new(Upgrades::new([1]).unwrap(), 1),
        Err(UpgradeChoiceError::Invalid)
    ));
}

#[test]
fn seeded_restart_reset_and_abandon_keep_old_callbacks_stale() {
    let mut session = UpgradeChoices::<4, 3>::new(Upgrades::new([2; 4]).unwrap(), 31).unwrap();
    let mut rng = RewardRng::new(7);
    session.credit(2).unwrap();
    let old = session.open(&mut rng).unwrap().unwrap();
    assert_eq!(session.abandon(), Some(old));
    assert_eq!(session.abandon(), None);
    assert_eq!(session.points(), 2);
    let mut rng = RewardRng::new(7);
    let second = session.open(&mut rng).unwrap().unwrap();
    assert_eq!(old.choices(), second.choices());
    assert_ne!(old.id(), second.id());
    let id = second.choices()[0].unwrap();
    assert_eq!(
        session.choose(old.id(), id),
        Err(UpgradeChoiceError::StaleOffer)
    );
    let reset = session.reset();
    assert_eq!((reset.discarded_points, reset.pending), (2, Some(second)));
    session.credit(1).unwrap();
    let third = session.open(&mut RewardRng::new(7)).unwrap().unwrap();
    assert_eq!(old.choices(), third.choices());
    assert_eq!(third.id().serial(), 3);
    for stale in [old.id(), second.id()] {
        assert_eq!(
            session.choose(stale, id),
            Err(UpgradeChoiceError::StaleOffer)
        );
    }
    assert_eq!(session.pending(), Some(&third));
}

#[test]
fn namespaces_distinguish_recreation_but_reuse_is_caller_responsibility() {
    let mut a = UpgradeChoices::<1, 1>::new(Upgrades::new([1]).unwrap(), 1).unwrap();
    let mut b = UpgradeChoices::<1, 1>::new(Upgrades::new([1]).unwrap(), 2).unwrap();
    let mut reused = UpgradeChoices::<1, 1>::new(Upgrades::new([1]).unwrap(), 1).unwrap();
    for session in [&mut a, &mut b, &mut reused] {
        session.credit(1).unwrap();
    }
    let offer = a.open(&mut RewardRng::new(1)).unwrap().unwrap();
    let other = b.open(&mut RewardRng::new(1)).unwrap().unwrap();
    assert_ne!(offer.id(), other.id());
    assert_eq!(b.choose(offer.id(), 0), Err(UpgradeChoiceError::StaleOffer));
    assert_eq!(b.pending(), Some(&other));
    assert_eq!(
        reused.open(&mut RewardRng::new(1)).unwrap().unwrap().id(),
        offer.id()
    );
}

#[test]
fn maximum_rank_and_pool_bounds_do_not_overflow_or_allocate() {
    let (_, control) = allocations_during(|| black_box(Box::new(1_u64)));
    assert!(control > 0);
    let mut choices = UpgradeChoices::<256, 256>::new(Upgrades::new([1; 256]).unwrap(), 5).unwrap();
    let mut rng = RewardRng::new(12);
    choices.credit(256).unwrap();
    let (_, measured) = allocations_during(|| {
        for _ in 0..256 {
            let offer = choices.open(&mut rng).unwrap().unwrap();
            assert_eq!(choices.open(&mut rng), Ok(Some(offer)));
            let id = offer.choices()[0].unwrap();
            assert_eq!(
                choices.choose(offer.id(), usize::MAX),
                Err(UpgradeChoiceError::NotOffered)
            );
            let accepted = choices.choose(offer.id(), id).unwrap();
            assert_eq!(accepted.rank_after, 1);
            assert_eq!(
                choices.choose(offer.id(), id),
                Err(UpgradeChoiceError::StaleOffer)
            );
        }
        assert_eq!(choices.open(&mut rng), Ok(None));
        let reset = choices.reset();
        assert_eq!(reset.discarded_points, 0);
        choices.credit(u32::MAX).unwrap();
        assert_eq!(choices.credit(1), Err(UpgradeChoiceError::BalanceOverflow));
        let offer = choices.open(&mut rng).unwrap().unwrap();
        assert_eq!(choices.abandon(), Some(offer));
    });
    assert_eq!(measured, 0);
    let mut high = UpgradeChoices::<1, 1>::new(Upgrades::new([u8::MAX]).unwrap(), 6).unwrap();
    high.credit(256).unwrap();
    for rank in 1..=u8::MAX {
        let offer = high.open(&mut rng).unwrap().unwrap();
        assert_eq!(high.choose(offer.id(), 0).unwrap().rank_after, rank);
    }
    assert_eq!(high.points(), 1);
    assert_eq!(high.open(&mut rng), Ok(None));
}

#[test]
fn accepted_wave_promotions_credit_once_and_quick_tap_spends_on_one_executed_tick() {
    let mut waves = WaveSequence::new(
        [WaveSpec {
            credits: 1,
            interval: 0.125,
            rest: 0.0,
            reward: 10,
        }],
        1,
    )
    .unwrap();
    let mut xp = Progression::new(10, 0, 4).unwrap();
    let mut choices = UpgradeChoices::<2, 2>::new(Upgrades::new([2; 2]).unwrap(), 9).unwrap();
    let mut rng = RewardRng::new(7);
    assert!(waves.admit(1, 0));
    assert_eq!(waves.advance(0.125, 0.0, 1).unwrap(), None); // Actor remains alive.
    assert_eq!(choices.points(), 0);
    // Headless host confirms this admitted actor's defeat; no fabricated win.
    let clear = waves.advance(0.125, 0.0, 0).unwrap().unwrap();
    let report = xp.award_report(u64::from(clear.reward));
    assert_eq!(report.promotions, 1);
    choices.credit(u32::from(report.promotions)).unwrap();
    let offer = choices.open(&mut rng).unwrap().unwrap();
    assert_eq!(waves.advance(0.125, 0.0, 0).unwrap(), None);
    assert_eq!(choices.points(), 1); // No duplicate clear or replayed report credit.
    let mut input = ActionMap::default();
    let code = InputCode::Key(32);
    let confirm = input.try_action(vec![Binding::digital(code)]).unwrap();
    input.feed(code, 1.0);
    input.feed(code, 0.0);
    assert!(input.peek(confirm).pressed); // A redraw/zero-tick frame consumes none.
    assert_eq!(choices.open(&mut rng).unwrap(), Some(offer));
    let chosen = offer.choices()[0].unwrap();
    let mut accepted = 0;
    for _ in 0..2 {
        // Two explicitly executed authority ticks.
        let action = input.consume_tick(confirm);
        if action.pressed {
            let receipt = choices.choose(offer.id(), chosen).unwrap();
            assert_eq!((receipt.rank_after, receipt.points_after), (1, 0));
            accepted += 1;
        }
        assert_eq!(waves.advance(1.0 / 64.0, 0.0, 0).unwrap(), None);
    }
    assert_eq!(accepted, 1);
    assert_eq!(
        choices.choose(offer.id(), chosen),
        Err(UpgradeChoiceError::StaleOffer)
    );
    assert_eq!(
        (
            xp.level(),
            choices.points(),
            choices.upgrades().rank(chosen)
        ),
        (2, 0, Some(1))
    );
}
