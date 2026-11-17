//! Tin Automata: twelve robot-programming workbenches and a factory to restore.
mod content;
mod layout;
pub mod model;
mod view;
use ferravex_arcade::{Color, Conf, Meta, window};
pub use model::{Game, Phase};
/// Identity, controls and the short field guide.
pub const META: Meta = Meta {
    id: "tin-automata",
    title: "Tin Automata",
    subtitle: "Programming puzzles",
    controls: "Left/Right Command | Up/Down Tape | Act Write | X Delete | 1 Run | 2 Step | C Stop | 3 Undo | 4 Clear | 5 Hint",
    accent: Color::new(0.43, 0.86, 0.77, 1.),
    help: &[
        "Fill the tape with commands, then run it to deliver every parcel.",
        "Grab a parcel and Drop it on its numbered dock; switches open gates.",
        "Repeat plays the previous command two or three more times.",
        "Three stars need par length, no faults and no hint.",
    ],
};
fn conf() -> Conf {
    window(META.title)
}
async fn game() {
    ferravex_arcade::run::<Game>(META).await;
}
ferravex_render::game!(conf, game);
