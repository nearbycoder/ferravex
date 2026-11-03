//! Stormbound: eight islands, one ship, and cannonballs that obey the wind.
mod content;
pub mod model;
mod view;
use ferravex_arcade::{Color, Conf, Meta, window};
pub use model::{Game, Phase};
/// Shared host identity, tutorial and default controls.
pub const META: Meta = Meta {
    id: "stormbound",
    title: "Stormbound",
    subtitle: "Artillery game",
    controls: "Arrows Aim/Power | Act Fire | 1-3 Ammo | X Aim Guide | C Retreat",
    help: &[
        "Sink the enemy ship in eight battles. Adjust for distance and wind.",
        "Left/right change cannon angle. Up/down change powder power. Space fires one selected round. Wind pushes every projectile throughout its flight; the dotted line predicts the actual flight. The faint gold line remembers your previous shot.",
        "Keys 1, 2, 3 select roundshot, chainshot and explosive shells. Roundshot damages hull and strips three armor. Chainshot deals 52 mast damage but little hull damage. Shells ignore armor and burst into hull sections within 57 units of the impact.",
        "X toggles your navigator's firing table. It suggests a legal angle, power and ammunition using the same physical simulation. Set these with the arrows; the table never changes your aim for you. The forecast tells you which section your current settings will hit.",
        "A fallen enemy mast delays alternate return volleys and worsens its powder setting. Losing your own mast caps power at 76. Enemy captains use different arcs, shot patterns and ammunition; every shot must physically touch a surviving section to cause damage.",
        "After a victory, spend salvage at the dock. 1 repairs 80 hull and 25 mast; 2 upgrades guns; 3 adds armor; 4 expands shell magazines. Space sails. Hull damage carries between islands; ammunition and purchased armor refill when sailing.",
        "C retreats from an aiming turn. After retreat, sinking or empty magazines, C spends one of two lifeboats and restores the island approach; Space retires. Retrying grants no salvage or score. Your failed shots remain in campaign accuracy. R starts a fresh whole campaign.",
        "Finch teaches range, Flint low shots, Vane rigging, Brack armor, Long Watch high arcs, Rose reversing gusts, the Bell Brothers double volleys, and Nocturne weak-section targeting. H returns to this guide, Esc pauses, M toggles sound.",
    ],
    accent: Color::new(0.97, 0.76, 0.37, 1.),
};
fn conf() -> Conf {
    window(META.title)
}
async fn game() {
    ferravex_arcade::run::<Game>(META).await;
}
ferravex_render::game!(conf, game);
