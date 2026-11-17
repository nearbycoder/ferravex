//! Gravity Post: eight planetary deliveries and a complete courier career.
mod cards;
mod content;
mod feedback;
pub mod model;
mod panel;
mod view;
use ferravex_arcade::{Color, Conf, Meta, window};
pub use model::{Game, Phase};

/// Identity, controls and the short field guide for the shared campaign host.
pub const META: Meta = Meta {
    id: "gravity-post",
    title: "Gravity Post",
    subtitle: "Lander deliveries",
    controls: "Left/Right Tilt | Up/Act Thrust | Down Brake | X Service | C Retry",
    help: &[
        "Load at the gold pad, then deliver to the green pad.",
        "Land slowly and upright: keep the marker in the green box.",
        "X on a landed pad loads, delivers, refuels or repairs.",
        "Beat the storm clock; wrecks spend recovery permits.",
    ],
    accent: Color::new(0.96, 0.72, 0.33, 1.),
};
fn conf() -> Conf {
    window(META.title)
}
async fn game() {
    ferravex_arcade::run::<Game>(META).await;
}
ferravex_render::game!(conf, game);
