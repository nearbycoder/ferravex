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
    controls: "Arrows Move/Attack | Act Guard/Exit | X Heal | C Burst | 1-3 Relic | Click Move/Use",
    help: &[
        "Find each floor's key, then take the stairs down.",
        "Red tiles strike after your next action. Step out!",
        "Bursts hit every foe within two tiles, through walls.",
        "Nothing moves until you act. Hover to preview a move.",
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
