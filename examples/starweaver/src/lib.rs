//! Starweaver: a finite exploration, salvage and gate-building expedition.
mod content;
pub mod model;
mod view;
use ferravex_arcade::{Color, Conf, Meta, window};
pub use model::Game;

/// Identity, input hints and the expedition's built-in field manual.
pub const META: Meta = Meta {
    id: "starweaver",
    title: "Starweaver",
    subtitle: "Space strategy",
    controls: "1-5 Panels | Arrows Select | Act Confirm | X Cargo",
    accent: Color::new(0.41, 0.85, 0.9, 1.),
    help: &[
        "Collect three relics, build the jump gate, and escape before turn 52.",
        "Choose panels with 1-5 or click a tab. Arrows choose a star or action. Space accepts. Click a star, then JUMP; other action rows accept on click.",
        "Chart lanes cost one fuel and one watch. First arrival can damage hull; revisit safely. Planning and menu navigation never spend watches.",
        "Survey each sector once before mining or exploring. Finite deposits yield two units; a Deep Scanner raises this to three. A full hold stops extraction.",
        "Wayfarer holds the Navigator's key. Hollow Choir holds the Singing lens. Silent Crown holds the Sentinel core. Survey then explore each landmark.",
        "Forge shows exact alloy, crystal and circuit costs. Relics unlock the first three recipes. Mounted mechanisms and upgrades use no cargo space.",
        "Ship: one crystal makes up to three fuel. One alloy plus one circuit repairs 25 hull. Loom and Verdant sell fuel, repair hull and buy your cargo.",
        "X on Ship changes the cargo type to sell. X on a mining row discards one unit for no reward. Services cost one watch; the one-use distress tow costs six.",
        "The shield halves damage. The cradle adds six hold spaces. Upgrades are optional investments; spare material also buys fuel, hull and time.",
        "The star collapses at watch 52. Zero hull also ends the run. Every accepted work action spends a watch. H for help, P pause, R a fresh expedition.",
        "Far Beacon has nine stranded engineers. Their optional rescue repairs 10 hull and earns 25 credits. Escape sooner with more hull and credits for a higher score.",
        "A route to begin: Loom -> Cinder -> Wayfarer for the key. Salvage alloy and circuits there. Cinder connects to Hollow Choir's crystal deposits.",
    ],
};
fn conf() -> Conf {
    window(META.title)
}
async fn game() {
    ferravex_arcade::run::<Game>(META).await;
}
ferravex_render::game!(conf, game);
