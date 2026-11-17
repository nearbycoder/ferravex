//! Wildfire Watch: six tactical firefighting and evacuation emergencies.
mod content;
mod layout;
pub mod model;
mod view;
use ferravex_arcade::{Color, Conf, Meta, window};
pub use model::Game;
/// Campaign identity, controls and the short field guide used by the host.
pub const META: Meta = Meta {
    id: "wildfire-watch",
    title: "Wildfire Watch",
    subtitle: "Firefighting tactics",
    controls: "1-3 Crew | Arrows Target | Act Work | X Break | 4 Air drop | 5 Forecast | C End turn",
    accent: Color::new(1., 0.64, 0.32, 1.),
    help: &[
        "Get everyone to base and every fire out by nightfall.",
        "Pick a crew, then click a tile: move, spray, carry, refill.",
        "Ghost flames show next spread; roads and breaks stop it.",
        "Fire on occupied villages or loaded crews costs a heart.",
    ],
};
fn conf() -> Conf {
    window(META.title)
}
async fn game() {
    ferravex_arcade::run::<Game>(META).await;
}
ferravex_render::game!(conf, game);
