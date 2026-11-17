//! Deep Pressure is a complete six-sortie submarine rescue campaign.
//! See [`model`] for renderer-independent fixed-tick rules and an executable example.
mod art;
mod content;
mod feedback;
pub mod model;
mod overlay;
mod panel;
mod view;

use ferravex_arcade::{Color, Conf, Meta, window};
pub use model::Game;
/// Game identity and player-facing rules consumed by the common arcade host.
pub const META: Meta = Meta {
    id: "deep-pressure",
    title: "Deep Pressure",
    subtitle: "Submarine rescue",
    controls: "Arrows Thrust | Act Rescue/Dock | X Ballast | C Sonar | 1-3 Refit",
    help: &[
        "Slow down by a glowing air bell; tap Space to winch.",
        "Hold Space to brake. Dock in the gold ring to land.",
        "The dock refills air and repairs the hull.",
        "Head home before the air ring reaches its white mark.",
    ],
    accent: Color::new(0.27, 0.87, 0.88, 1.),
};
fn conf() -> Conf {
    window(META.title)
}
async fn game() {
    ferravex_arcade::run::<Game>(META).await;
}
ferravex_render::game!(conf, game);
