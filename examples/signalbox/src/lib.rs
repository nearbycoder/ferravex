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
        "Route every train to its matching platform without collisions.",
        "West points lead to A (north) and B (south); east points lead to C (north) and D (south). STRAIGHT continues along the main line.",
        "Click one of the six bottom controls, or select with Left/Right and press Space. Up/Down cycle a control; 1-5 select the first five controls.",
        "A green gate or bridge signal admits ONE train, then returns to red. The bridge is a single track: opposing trains collide. Fast mail can catch slow freight.",
        "X stops railway time so you can study the timetable and change controls. Plan freely. Points cannot be changed while a train is within their protected area.",
        "Late trains still count, but lose punctuality bonuses. Gold: no delay. Silver: at most 45 seconds combined delay. Deliver everyone before the closing bell.",
        "Bridge inspections block new entrances during the marked times. Trains already crossing may finish. Rain and snow reduce speed.",
        "C spends one of three career retries to replay a shift. Wrong platforms, collisions and the closing bell are incidents. R starts the whole career again.",
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
