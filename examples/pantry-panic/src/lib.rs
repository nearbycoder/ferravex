//! Pantry Panic: build a neighborhood restaurant through seven complete services.
//! [`Game`] contains the pure fixed-tick kitchen model, shared by native and Wasm.
mod content;
pub mod model;
mod view;
use ferravex_arcade::{Color, Conf, Meta, window};
pub use model::Game;
/// Campaign identity, control hints and the shared host's paginated kitchen guide.
pub const META: Meta = Meta {
    id: "pantry-panic",
    title: "Pantry Panic",
    subtitle: "Cooking game",
    controls: "Arrows Move | Act Use/Hold to Chop | X Discard | C Recipe | 1-3 Upgrade",
    help: &[
        "Buy, chop, mix, cook, plate and serve each guest's order.",
        "Hold Space at the board to chop; tap Space at other stations.",
        "Click a station to walk there, then click it again to use it.",
        "Grab pots before they burn. X bins food, empties the bowl or cleans pots.",
    ],
    accent: Color::new(1., 0.62, 0.3, 1.),
};
fn conf() -> Conf {
    window(META.title)
}
async fn game() {
    ferravex_arcade::run::<Game>(META).await;
}
ferravex_render::game!(conf, game);
