//! Wildfire Watch: six tactical firefighting and evacuation emergencies.
mod content;
pub mod model;
mod view;
use ferravex_arcade::{Color, Conf, Meta, window};
pub use model::Game;
/// Campaign identity and the complete field manual used by the shared host.
pub const META: Meta = Meta {
    id: "wildfire-watch",
    title: "Wildfire Watch",
    subtitle: "Firefighting tactics",
    controls: "1-3 Crew | Arrows Target | Act Work | X Firebreak | C End Turn",
    accent: Color::new(1., 0.64, 0.32, 1.),
    help: &[
        "Evacuate every resident and contain the fire in six missions.",
        "1 selects Engine, 2 Ranger and 3 Rescue. Arrows or WASD move the gold targeting cursor. Space acts on that tile. Clicking a map tile selects it and attempts the same action. Planning takes no time.",
        "Each crew has four action points per turn. Moving costs one point per tile, or two through living forest. A path must fit your remaining points. The crews can share a tile. Roads, water and rock have no fuel.",
        "Target a fire to spray it: two action points and one water. Engine reaches two tiles and removes three heat; other crews reach one tile and remove two. An extinguished tile stays wet for three spreads.",
        "Stand beside a village and target it to load residents for two points. Rescue carries six people; other crews carry two. Return beside the white base and target it to unload for one point. Aboard is not yet safe.",
        "Target base or adjacent water to refill a partly empty tank for one point. Engine begins with six water and other crews with three. An empty tank cannot spray.",
        "X cuts the targeted, adjacent unburned vegetation into a permanent firebreak. Rangers spend one point; others spend two. A firebreak cannot be cut through a burning tile or village.",
        "C ends the turn. Red-outlined tiles show the exact next ignitions under the displayed wind. Fire consumes fuel, grows hotter and spreads simultaneously. Water, rock, roads, wet ground and cut breaks stop it. 5 toggles the forecast.",
        "4 calls an air drop at the cursor: twelve credits and one sortie. It removes three heat from the center and four neighbors and wets them. A drop with no fire in its footprint is rejected without charge.",
        "An occupied village burning or a loaded crew caught by fire fails the incident. An empty crew retreats to base without water. Empty village damage lowers the medal. You must finish before the displayed turn deadline.",
        "Clearing an incident earns credits and one to three medals. At the debrief, 1 buys tank capacity, 2 crew training and 3 air support; each has two levels. Space opens the next briefing. Keep credits for emergency drops.",
        "You have three career-wide retries. Space at a setback restores that incident's starting cash, map and rescues, so retries cannot farm rewards. A fourth failure ends the career. R starts a fresh career; H guide; Esc/P pause.",
    ],
};
fn conf() -> Conf {
    window(META.title)
}
async fn game() {
    ferravex_arcade::run::<Game>(META).await;
}
ferravex_render::game!(conf, game);
