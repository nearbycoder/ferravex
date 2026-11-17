//! Signalbox: eight authored railway dispatch shifts with complete campaign rules.
//!
//! Run `cargo run -p signalbox --offline --locked -j 2` on a supported native
//! graphics host. Pure rules and the legal dispatcher are available as [`Game`].
//! The browser build uses the same model and shared Ferravex input/session host.
mod content;
pub mod model;
mod view;

use ferravex_arcade::{Color, Conf, Meta, window};
pub use model::Game;

/// Player-facing metadata and complete controls for the shared lifecycle host.
pub const META: Meta = Meta {
    id: "signalbox",
    title: "Signalbox",
    subtitle: "Train routing",
    controls: "Left/Right Select | Act Operate | X Run/Pause | C Retry",
    help: &[
        "Send each train to the platform matching its colored letter.",
        "Click points to pick a branch; dashes show the route.",
        "Click a lamp: green admits one train. Meeting trains crash.",
        "X stops the clock to plan. Late trains cost stars.",
    ],
    accent: Color::new(0.92, 0.73, 0.40, 1.),
};

fn conf() -> Conf {
    window(META.title)
}
async fn game() {
    ferravex_arcade::run::<Game>(META).await;
}
ferravex_render::game!(conf, game);
