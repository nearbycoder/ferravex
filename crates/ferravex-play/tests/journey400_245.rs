use ferravex_play::gameplay::{
    inventory::{Inventory, Stack},
    loot::{RewardRng, RewardTable},
    pity::{
        PityAttempt, PityCounter, PityDelivery, PityDeliveryError, PityOfferOutcome, PityOutcome,
        PitySettlement,
    },
    projectile::{HitResult, ProjectileBudget},
};

#[test]
fn accepted_hit_delivers_once_after_full_inventory_and_declined_retry() {
    let mut hits = ProjectileBudget::<2>::new(1, 0).unwrap();
    let mut bag = Inventory::<1>::new().unwrap();
    bag.add(Stack {
        item: 99,
        amount: 1,
        limit: 1,
    })
    .unwrap();
    let mut pity = PityDelivery::new(PityCounter::new(2).unwrap(), 45).unwrap();
    let mut rng = RewardRng::new(245);
    let mut expected_rng = rng;
    let table = RewardTable::new([1, 0]).unwrap();
    let mut feedback = 0;
    for (target, expected_hit) in [(101, HitResult::Continue), (102, HitResult::Stop)] {
        assert_eq!(hits.target(target), expected_hit); // Host-confirmed collisions.
        let natural_success = table.roll(&mut rng) == 1;
        table.roll(&mut expected_rng);
        let result = pity
            .offer(PityAttempt::Accepted { natural_success })
            .unwrap();
        if target == 101 {
            assert_eq!(result, PityOfferOutcome::Miss);
            continue;
        }
        let PityOfferOutcome::Reward(offer) = result else {
            panic!("boundary")
        };
        assert_eq!(offer.outcome(), PityOutcome::Guaranteed);
        let payload = Stack {
            item: 5,
            amount: 1,
            limit: 1,
        };
        for _ in 0..2 {
            let inserted = bag.add(payload).is_ok();
            assert!(!inserted);
            assert_eq!(
                pity.settle(offer.id(), inserted),
                Ok(PitySettlement::Pending(offer))
            );
            assert_eq!(pity.misses(), 1);
            assert_eq!(bag.count(5), 0);
            assert_eq!(feedback, 0);
        }
        assert_eq!(hits.target(target), HitResult::Ignored);
        bag.remove(99, 1).unwrap();
        let retry = pity.pending().unwrap();
        assert_eq!(retry, offer);
        let inserted = bag.add(payload).is_ok();
        if let PitySettlement::Delivered(receipt) = pity.settle(retry.id(), inserted).unwrap() {
            assert_eq!(receipt, offer);
            feedback += 1;
        }
        assert!(pity.pending().is_none()); // No repeat destination insertion.
        assert_eq!(
            pity.settle(offer.id(), true),
            Err(PityDeliveryError::StaleOffer)
        );
    }
    assert_eq!(rng.next_u64(), expected_rng.next_u64());
    assert_eq!((bag.count(5), feedback, pity.misses()), (1, 1, 0));
}

#[test]
fn orbit_guard_model_keeps_full_store_counter_and_roll_sequence() {
    // Model the inspected hit_block order, not an execution of the Orbit game:
    // always roll on destruction, then check len < 8 before counter admission.
    let mut legacy = PityCounter::new(6).unwrap();
    for _ in 0..5 {
        assert!(!legacy.attempt(false));
    }
    let mut delivery = PityDelivery::new(legacy, 46).unwrap();
    let table = RewardTable::new([3, 1]).unwrap();
    let mut old_rng = RewardRng::new(245);
    let mut new_rng = old_rng;
    for len in [8, 8, 7, 7, 8, 7, 7, 7, 7, 7, 7, 7] {
        let old_natural = table.roll(&mut old_rng) == 1;
        let old_reward = len < 8 && legacy.attempt(old_natural);
        let new_natural = table.roll(&mut new_rng) == 1;
        let outcome = delivery
            .offer(if len < 8 {
                PityAttempt::Accepted {
                    natural_success: new_natural,
                }
            } else {
                PityAttempt::Deferred
            })
            .unwrap();
        if let PityOfferOutcome::Reward(offer) = outcome {
            assert!(old_reward);
            assert!(matches!(
                delivery.settle(offer.id(), true),
                Ok(PitySettlement::Delivered(_))
            ));
        } else {
            assert!(!old_reward);
        }
        assert_eq!(legacy.misses(), delivery.misses());
        if len == 8 {
            assert!(delivery.pending().is_none());
        }
    }
    assert_eq!(old_rng.next_u64(), new_rng.next_u64());
}
