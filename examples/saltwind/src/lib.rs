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
    subtitle: "Sailing and trading",
    controls: "1-5 Panels | Arrows Select/Steer | Act Buy/Sail | X Sell/Route",
    help: &[
        "Complete six deliveries and repay 900 crowns before day 42.",
        "Begin at Market [2]. Buy 3 grain with Space, open Chart [1], select Brinewatch and press Space to sail.",
        "At the destination, open Charters [3] and press Space to deliver. Each sealed charter pays you and reveals the next.",
        "Market: arrows choose goods, food or repairs. Space buys one crate or services; X sells one crate. Cargo space and stock are limited.",
        "Chart: arrows or an island click choose a port. X toggles sheltered/open sea. Sheltered takes one extra day and avoids storm damage.",
        "One sea day uses one food. Watch the forecast! At sea, Up/Down steer around storm rings; Space reefs sails, reducing storm damage but slowing you.",
        "Shipyard [4] sells faster sails, more hold space and armor. Save enough crowns for cargo, food and the final debt.",
        "Planning in port costs no days. Log [5] can wait one day. R restarts the whole voyage; H opens this guide, P/Esc pauses.",
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
