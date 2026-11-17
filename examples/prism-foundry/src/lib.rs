//! Prism Foundry: twelve authored optical logic puzzles and a complete restoration campaign.
//!
//! The play screen explains itself with light, color and icons: hovering a
//! device previews its next beam, locks show their channels as colored arcs,
//! and the workbench tracks pace as stars against a par meter.
mod content;
pub mod model;
mod view;
use ferravex_arcade::{Color, Conf, Meta, window};
pub use model::Game;
/// Static identity and complete rules for the shared campaign host.
pub const META: Meta = Meta {
    id: "prism-foundry",
    title: "Prism Foundry",
    subtitle: "Laser puzzles",
    controls: "Arrows Select | Act Rotate | X Undo | C Reset | 4 Hint | 5 Seal",
    help: &[
        "Turn mirrors, splitters and filters to light each lock.",
        "Locks want exact colors: R+G=Y, R+B=M, G+B=C, RGB=W.",
        "Splitters fork beams. Filters pass R, then G, then B.",
        "Three stars: at par, no hint, reset or failed seal.",
    ],
    accent: Color::new(0.91, 0.73, 0.39, 1.),
};
fn conf() -> Conf {
    window(META.title)
}
async fn game() {
    ferravex_arcade::run::<Game>(META).await;
}
ferravex_render::game!(conf, game);
