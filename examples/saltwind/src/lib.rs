//! Saltwind: six islands, six charters, and one ship to win back.
//!
//! Rules and headless examples live in [`model`]. The native and browser entry
//! points share the same fixed-tick model through `ferravex-arcade`.
mod content;
pub mod model;
mod view;

use ferravex_arcade::{Color, Conf, Meta, window};
pub use model::Game;

/// Player-facing rules and controls shown by the shared lifecycle host.
pub const META: Meta = Meta {
    id: "saltwind",
    title: "Saltwind",
    subtitle: "Merchant sailing",
    controls: "1-5 Panels | Arrows Select/Steer | Act Buy/Sail | X Sell/Route",
    help: &[
        "Seal six charters, then pay 900 crowns by day 42.",
        "Buy cargo at Market, pick an island, then set sail.",
        "Open sea is quick; dodge squalls or reef your sails.",
        "Each day at sea eats one food. Port planning is free.",
    ],
    accent: Color::new(0.83, 0.66, 0.35, 1.),
};

fn conf() -> Conf {
    window(META.title)
}
async fn game() {
    ferravex_arcade::run::<Game>(META).await;
}
ferravex_render::game!(conf, game);
