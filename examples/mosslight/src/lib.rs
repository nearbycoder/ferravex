//! Mosslight: a complete six-floor turn-based expedition beneath a stolen sun.
//!
//! The pure [`Game`] model can run without a window. The native and WebAssembly
//! hosts use the same rules through [`ferravex_arcade::ArcadeGame`].
mod content;
pub mod model;
mod view;

use ferravex_arcade::{Color, Conf, Meta, window};
pub use model::Game;

/// Identity, default controls and the in-game field manual.
pub const META: Meta = Meta {
    id: "mosslight",
    title: "Mosslight",
    subtitle: "Dungeon crawler",
    controls: "Arrows Move/Attack | Act Guard/Exit | X Heal | C Burst | 1-3 Relic",
    help: &[
        "Collect the key on each floor, reach the stairs, and defeat the final boss.",
        "Tap arrows or WASD to move one tile. Bump a creature to strike it. Every accepted move, strike, guard, potion or burst gives enemies one turn. Holding a direction does not repeat. Idle time is safe: think as long as you like.",
        "RED marked tiles show attacks that will resolve after your next action. Move out of them, kill the attacker, or guard with Space to reduce each hit by 3. A crossed sword above a creature means it is attacking; a sleeping mark means recovery.",
        "X or E drinks one flask for up to 12 health. C or Q spends one lantern charge to damage ALL foes within two tiles. A burst crosses small walls. Invalid actions spend nothing. A used moonwell cannot refill you again.",
        "At each safe camp, choose one relic: 1 sharpens your blade, 2 grows your maximum health, 3 expands the lantern. Every camp also restores 7 health, one flask and one charge. Make a build that suits your tactics.",
        "Rootlings chase and bite. Knights strike from two tiles away. Spore bulbs fire six-tile beams, stopped by walls. The Root King alternates a long cross with a close ring. Every attack has a recovery turn.",
        "Amber jars and defeated enemies increase your score. The ending rewards surviving health and efficient turns. Inspect any tile with a click; clicking a neighboring tile moves or attacks. R begins a fresh expedition, Esc pauses, H opens this manual.",
    ],
    accent: Color::new(0.55, 0.83, 0.49, 1.),
};
fn conf() -> Conf {
    window(META.title)
}
async fn game() {
    ferravex_arcade::run::<Game>(META).await;
}
ferravex_render::game!(conf, game);
