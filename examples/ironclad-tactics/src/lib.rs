//! Ironclad Tactics: six authored operations to liberate the Copper Coast.
//!
//! The renderer-independent [`Game`] accepts the same commands from a person,
//! a browser host, or the deterministic headless campaign controller.
mod content;
mod fx;
pub mod model;
mod view;

use ferravex_arcade::{Color, Conf, Meta, window};
pub use model::Game;

/// Game identity, controls and the campaign field manual.
pub const META: Meta = Meta {
    id: "ironclad-tactics",
    title: "Ironclad Tactics",
    subtitle: "Squad tactics",
    controls: "1-3 Unit | Arrows Target | Act Move/Attack | X Skill | C End Turn",
    help: &[
        "Click a machine, then a tile to move or an enemy to fire.",
        "Moving costs 1 AP per tile; firing costs 2 AP.",
        "Red lines mark enemy shots: dodge, take cover or strike first.",
        "X: Brass braces, Kestrel focuses, Mica repairs. C ends the turn.",
    ],
    accent: Color::new(0.94, 0.69, 0.28, 1.),
};
fn conf() -> Conf {
    window(META.title)
}
async fn game() {
    ferravex_arcade::run::<Game>(META).await;
}
ferravex_render::game!(conf, game);
