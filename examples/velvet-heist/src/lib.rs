//! Velvet Heist: six stealth infiltrations to return a city's stolen history.
mod content;
mod feedback;
mod hud;
pub mod model;
mod sprites;
mod view;
use ferravex_arcade::{Color, Conf, Meta, window};
pub use model::{Game, Phase};
/// Identity, bindings and the four-line field guide.
pub const META: Meta = Meta {
    id: "velvet-heist",
    title: "Velvet Heist",
    subtitle: "Stealth heist",
    controls: "Arrows Move | Act Interact | X Crouch | C Throw coin | Click Aim",
    accent: Color::new(0.88, 0.65, 0.94, 1.),
    help: &[
        "Take every record, then reach the glowing exit before the clock runs out.",
        "Crouch in violet shadow or a cabinet to hide; standing steps are loud.",
        "Bright cones would see you now. Being seen fills the alarm and costs medals.",
        "The pass opens doors, consoles kill beams, coins lure guards. Hearts are retries.",
    ],
};
fn conf() -> Conf {
    window(META.title)
}
async fn game() {
    ferravex_arcade::run::<Game>(META).await;
}
ferravex_render::game!(conf, game);
