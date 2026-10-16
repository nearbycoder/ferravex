//! A headless survival/crafting loop composed from authored data and shared rules.
//! Run: cargo run -p ferravex-play --example gameplay_recipe --offline --locked
use ferravex_play::gameplay::{
    crafting::craft,
    interaction::Interaction,
    inventory::{Inventory, Stack},
    loot::RewardRng,
    progression::Progression,
    upgrades::Upgrades,
    waves::{WaveSequence, WaveSpec},
};
fn main() {
    let mut waves = WaveSequence::new(
        [WaveSpec {
            credits: 3,
            interval: 0.2,
            rest: 0.5,
            reward: 2,
        }; 2],
        4,
    )
    .unwrap();
    let mut bag = Inventory::<4>::new().unwrap();
    let mut xp = Progression::new(3, 2, 3).unwrap();
    let mut upgrades = Upgrades::new([2, 2]).unwrap();
    let mut rng = RewardRng::new(7);
    let mut interact = Interaction::new(0.4).unwrap();
    let mut kills = 0;
    // The example host confirms a defeat immediately after each admitted spawn.
    // A real host replaces this with actors, input, movement and collision.
    for _ in 0..600 {
        if waves.admit(1, 0) {
            kills += 1;
            bag.add(Stack {
                item: 1,
                amount: 1,
                limit: 100,
            })
            .unwrap();
            for _ in 0..xp.award(1) {
                if let Some(id) = upgrades.offers::<2>(&mut rng).unwrap()[0] {
                    upgrades.apply(id).unwrap();
                }
            }
        }
        if let Some(clear) = waves.advance(1. / 60., 0., 0).unwrap() {
            bag.add(Stack {
                item: 2,
                amount: clear.reward,
                limit: 100,
            })
            .unwrap();
        }
        if waves.finished() {
            break;
        }
    }
    // A host confirms the player holding the workbench interaction in range.
    for _ in 0..30 {
        if interact.step(1. / 60., Some((1, 90))).unwrap() {
            craft(
                &mut bag,
                &[(1, 4), (2, 2)],
                &[Stack {
                    item: 3,
                    amount: 1,
                    limit: 10,
                }],
            )
            .unwrap();
        }
    }
    assert!(waves.finished());
    assert_eq!(kills, 6);
    assert_eq!(bag.count(3), 1);
    assert_eq!(bag.count(1), 2);
    assert_eq!(bag.count(2), 2);
    println!(
        "2 waves, {kills} confirmed kills, level {}, one crafted reward",
        xp.level()
    );
}
