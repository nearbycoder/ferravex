//! Gilded Deck: nine battles to reclaim a city's stolen names.
mod content;
mod layout;
pub mod model;
mod view;
use ferravex_arcade::{Color, Conf, Meta, window};
pub use model::Game;
/// Identity, controls and a short guide for the shared campaign host.
pub const META: Meta = Meta {
    id: "gilded-deck",
    title: "Gilded Deck",
    subtitle: "Deck builder",
    controls: "Arrows/1-5 Select | Act Play | X End Turn/Back | C Deck",
    help: &[
        "Click a card to lift it; click it again to play it.",
        "The enemy's next move floats above its head.",
        "Block fades at your next turn; poison ticks every turn.",
        "Flame cards burn away until the next battle.",
    ],
    accent: Color::new(0.91, 0.73, 0.38, 1.),
};
fn conf() -> Conf {
    window(META.title)
}
async fn game() {
    ferravex_arcade::run::<Game>(META).await;
}
ferravex_render::game!(conf, game);
