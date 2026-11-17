//! Mooncourt: six lunar rivals, a racket, and the Earthrise Cup.
mod content;
pub mod model;
mod view;
use ferravex_arcade::{Color, Conf, Meta, window};
pub use model::{Game, Phase};
/// Identity and complete player-facing rules for the shared campaign host.
pub const META: Meta = Meta {
    id: "mooncourt",
    title: "Mooncourt",
    subtitle: "Racket sports",
    controls: "Arrows Move | 1-3 Aim | Act Drive | X Lob | C Smash",
    accent: Color::new(0.58, 0.83, 0.98, 1.),
    help: &[
        "Win two games per match; beat all six rivals for the cup.",
        "Stand on the landing ring; swing when your reach ring turns gold.",
        "A smash needs a high ball and 26 stamina. Lobs buy time.",
        "Wins earn training. A lost match spends a heart; the fourth ends the tour.",
    ],
};
fn conf() -> Conf {
    window(META.title)
}
async fn game() {
    ferravex_arcade::run::<Game>(META).await;
}
ferravex_render::game!(conf, game);
