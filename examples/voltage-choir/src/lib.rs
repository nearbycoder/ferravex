//! Voltage Choir: an original six-track, four-lane rhythm concert.
//! The pure [`Game`] model uses the same note judgments for humans and autoplay.
mod content;
pub mod model;
mod view;
use ferravex_arcade::{Color, Conf, Meta, window};
pub use model::Game;

/// The shared host's identity, control hints and field-guide pages.
pub const META: Meta = Meta {
    id: "voltage-choir",
    title: "Voltage Choir",
    subtitle: "Rhythm game",
    controls: "Arrows Hit Notes | Act Continue | X Encore | C Retry",
    help: &[
        "Hit each arrow as its note crosses the line.",
        "Linked notes are chords: press both arrows together.",
        "Misses and stray taps drain the crowd; beat the goal.",
        "Full Encore? Press X for bonus points and a shield.",
    ],
    accent: Color::new(0.42, 0.84, 0.98, 1.),
};
fn conf() -> Conf {
    window(META.title)
}
async fn game() {
    ferravex_arcade::run::<Game>(META).await;
}
ferravex_render::game!(conf, game);
