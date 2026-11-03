//! Hearthharvest: fourteen mornings to bring a village back to one table.
//! The renderer-independent [`Game`] is shared by native and WebAssembly hosts.
mod content;
pub mod model;
mod view;
use ferravex_arcade::{Color, Conf, Meta, window};
pub use model::Game;
/// Campaign identity and the in-game, paginated field guide.
pub const META: Meta = Meta {
    id: "hearthharvest",
    title: "Hearthharvest",
    subtitle: "Farm management",
    controls: "Arrows Select | Act Work/Buy | X Crop/Sell | C Sleep | 1-5 Panels",
    help: &[
        "Complete three orders and finish day 14 with at least 80 coins.",
        "Begin with four turnips. On Field [1], arrows select a plot; Space plants the selected seed, waters a growing crop, or harvests a ripe crop. Click a plot to select it. X cycles turnip, bean and pumpkin seeds.",
        "Planting costs two energy; watering and harvesting cost one. Turnips need two watered nights, beans three, pumpkins four. Rain waters old AND new crops for free. Ready crops can wait safely until you have harvest energy.",
        "C opens the sleep card. Space confirms; X cancels. Sleep advances one day and restores energy. Dry crops gain one stress per night, or two in heat; three stress means wilt. Clear wilted crops with Space. Frost pauses growth without cold frames.",
        "Seed Stall [2]: arrows choose a crop, Space buys one seed, X sells one harvested crop. Stock refreshes daily. Keep the ingredients needed by future orders; seeds cannot be sold as produce.",
        "Village [3]: Space delivers the first unfinished order, only after its opening day and only with all ingredients. Soup Night opens day 1, Founders' Table day 6, and Lantern Feast day 10. Missing a deadline ends the season.",
        "Tools [4]: arrows select, Space buys. Brass Can waters a plot plus right/lower neighbors. Work Boots add six daily energy. Cold Frames protect growth on the day-ten frost. On this page X composts the selected EMPTY farm plot for two coins and one energy.",
        "Harvesting wears soil down. After two harvests, exhausted soil adds one growing night; compost restores it. Almanac [5] shows all fourteen forecasts and today's letter. Plan ahead: late pumpkins will not have time to ripen.",
        "Your ending rank rewards surplus coins, a productive farm and tool ownership. Harvests and village deliveries score points; invalid actions never create crops or money. R starts a fresh season, H opens this guide, Esc pauses.",
    ],
    accent: Color::new(0.94, 0.72, 0.36, 1.),
};
fn conf() -> Conf {
    window(META.title)
}
async fn game() {
    ferravex_arcade::run::<Game>(META).await;
}
ferravex_render::game!(conf, game);
