//! Gilded Deck: nine battles to reclaim a city's stolen names.
mod content;
pub mod model;
mod view;
use ferravex_arcade::{Color, Conf, Meta, window};
pub use model::Game;
/// Identity, controls and complete rules for the shared campaign host.
pub const META: Meta = Meta {
    id: "gilded-deck",
    title: "Gilded Deck",
    subtitle: "Deck builder",
    controls: "Arrows/1-5 Select | Act Play | X End Turn/Back | C Deck",
    help: &[
        "Build your deck and defeat nine enemies by playing cards.",
        "Begin by choosing one of three relics. Read the enemy's visible intent before playing. Arrows or 1-5 select cards, Space plays the selected card. Click a card to inspect it, then click PLAY. X ends your turn.",
        "Cards cost energy. Start each turn with 3 energy and 5 cards; relics can improve both. The hand holds seven. End turn discards your hand. Empty draw piles reshuffle the discard pile in a repeatable order.",
        "Exhausted cards are gone for the current battle only. All original cards return next battle. Wounds cannot be played, are discarded at turn end and disappear after the battle. C inspects your whole permanent deck; arrows turn pages.",
        "Block absorbs attacks and clears at your next turn. Enemy armor reduces EACH hit. Poison ignores both. Enemy poison ticks before its intent; your poison ticks after its intent, including new poison. Each tick deals its stack count and then removes one stack.",
        "Strength adds damage to every hit. Weak reduces attack damage by one quarter; vulnerable increases it by half, rounding down at each step. Both last the shown number of turns. Thorns hurt an attacker when its hit costs you HP.",
        "Win a battle to earn coins, score and 4 HP. Choose one of three card rewards or press X to skip. A smaller deck reaches its best cards more often. The two vault elites also grant a choice of powerful relics.",
        "Each camp allows ONE action: rest for 18 HP, forge one permanent card upgrade, or buy one shop item. Shops sell a card for 35 coins, a 28-HP tonic for 25, or remove one ungilded Gilt Strike for 20. Then choose Depart.",
        "Gold Spark, Insight and Sharpen support long turns but exhaust. Night Ink beats armor; Hammerfall sets up vulnerable; Mirror Edge spends your block for damage. Clean Slate removes your poison, weak and vulnerable.",
        "A fight ends after 40 turns, so endless healing is not an escape. Battle score rewards fast victories; the ending rewards remaining HP. The Regent gains 3 strength once below half HP. H pauses for this guide; Esc/P pause; M sound.",
    ],
    accent: Color::new(0.91, 0.73, 0.38, 1.),
};
fn conf() -> Conf {
    window(META.title)
}
async fn game() {
    ferravex_arcade::run::<Game>(META).await;
}
ferravex_render::game!(conf, game);
