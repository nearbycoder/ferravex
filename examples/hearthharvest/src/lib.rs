//! Hearthharvest: fourteen mornings to bring a village back to one table.
//! The renderer-independent [`Game`] is shared by native and WebAssembly hosts.
mod content;
pub mod model;
mod view;
use ferravex_arcade::{Color, Conf, Meta, window};
pub use model::Game;
/// Campaign identity and the in-game, paginated field guide.
pub const META: Meta = Meta {
    id: "hearthharvest",
    title: "Hearthharvest",
    subtitle: "Village farming",
    controls: "Arrows Select | Act Work/Buy | X Seed/Sell | C Sleep | 1-5 Pages",
    help: &[
        "Plant, water, then sleep: watered crops grow nightly.",
        "Three dry nights wilt a crop; heat counts twice.",
        "Deliver each order from your basket by its last day.",
        "Finish day 14 with every table served and 80 coins.",
    ],
    accent: Color::new(0.94, 0.72, 0.36, 1.),
};
fn conf() -> Conf {
    window(META.title)
}
async fn game() {
    ferravex_arcade::run::<Game>(META).await;
}
ferravex_render::game!(conf, game);
