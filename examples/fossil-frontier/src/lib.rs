//! Fossil Frontier: six excavation permits and a museum worth opening.
mod content;
pub mod model;
mod view;
use ferravex_arcade::{Color, Conf, Meta, window};
pub use model::{Game, Phase};
/// Campaign identity, controls and a four-line field guide for the shared host.
pub const META: Meta = Meta {
    id: "fossil-frontier",
    title: "Fossil Frontier",
    subtitle: "Excavation management",
    controls: "1-5 Panels | Arrows Select | Act Dig/Confirm | X Survey | C Tool",
    accent: Color::new(0.91, 0.68, 0.38, 1.),
    help: &[
        "Survey to reveal depths and pale fossil traces before you dig.",
        "Rough tools cut deep but crack bone; brush the final layer.",
        "Jacket exposed fossils, then match their silhouettes in the Lab.",
        "Identify the Giant part plus one more, then open the next site at Camp.",
    ],
};
fn conf() -> Conf {
    window(META.title)
}
async fn game() {
    ferravex_arcade::run::<Game>(META).await;
}
ferravex_render::game!(conf, game);
