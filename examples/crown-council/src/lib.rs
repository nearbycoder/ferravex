//! A complete thirty-council reign with authored consequences and four legacies.
mod content;
mod feedback;
pub mod model;
mod realm;
mod view;
use ferravex_arcade::{Color, Conf, Meta, window};
pub use model::{Ending, Game};
/// Identity and readable onboarding used by the shared Ferravex arcade host.
pub const META: Meta = Meta {
    id: "crown-council",
    title: "Crown & Council",
    subtitle: "Kingdom strategy",
    controls: "1-3 Choose | Arrows Select | Act Confirm | X Works | C History",
    help: &[
        "Seal one of three decrees at each of thirty councils.",
        "Any pillar at zero ends the reign: coin, folk, guard or faith.",
        "Royal works cost coin now and pay off a few councils later.",
        "Your most used seal names your legacy; kept promises return.",
    ],
    accent: Color::new(0.71, 0.50, 0.19, 1.),
};
fn conf() -> Conf {
    window(META.title)
}
async fn game() {
    ferravex_arcade::run::<Game>(META).await;
}
ferravex_render::game!(conf, game);
