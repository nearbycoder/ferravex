//! Starweaver: a finite exploration, salvage and gate-building expedition.
mod content;
mod fx;
mod layout;
mod look;
pub mod model;
mod panels;
mod view;
use ferravex_arcade::{Color, Conf, Meta, window};
pub use model::Game;

/// Identity, input hints and the expedition's short field manual.
pub const META: Meta = Meta {
    id: "starweaver",
    title: "Starweaver",
    subtitle: "Space strategy",
    controls: "1-5 Panels | Arrows Select | Act Confirm | X Cargo",
    accent: Color::new(0.41, 0.85, 0.9, 1.),
    help: &[
        "Recover three relics, forge their gate parts, then open the gate at Loom.",
        "Actions cost watches. Watch 52 or zero hull ends the run.",
        "Survey a sector to unlock its deposits and landmark.",
        "Loom and Verdant refuel, repair and trade.",
    ],
};
fn conf() -> Conf {
    window(META.title)
}
async fn game() {
    ferravex_arcade::run::<Game>(META).await;
}
ferravex_render::game!(conf, game);
