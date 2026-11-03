//! Ironclad Tactics: six authored operations to liberate the Copper Coast.
//!
//! The renderer-independent [`Game`] accepts the same commands from a person,
//! a browser host, or the deterministic headless campaign controller.
mod content;
pub mod model;
mod view;

use ferravex_arcade::{Color, Conf, Meta, window};
pub use model::Game;

/// Game identity, controls and the campaign field manual.
pub const META: Meta = Meta {
    id: "ironclad-tactics",
    title: "Ironclad Tactics",
    subtitle: "Squad tactics",
    controls: "1-3 Unit | Arrows Target | Act Move/Attack | X Skill | C End Turn",
    help: &[
        "Complete six missions using movement, attacks, cover and squad skills.",
        "Select BRASS, KESTREL or MICA with 1, 2 or 3, or click their card. Arrows/WASD move the targeting cursor; Space issues its order. Clicking a board tile issues that order immediately. Click an ally to select it. C/Q or 4 ends the squad turn.",
        "Moving costs 1 action point (AP) per tile along a clear four-way path. Shooting costs 2 AP, requires line of sight and costs no ammunition. Walls block movement and fire; low cover reduces each incoming hit by 1. Friendly machines block movement but not fire.",
        "BRASS is the Vanguard: 18 hull, 4 AP, 4 damage, range 4. X braces for 1 AP, reducing every incoming hit by 2 until next turn. Moving removes brace. KESTREL is the Scout: 14 hull, 5 AP, 3 damage, range 6. X spends 1 AP to add 2 damage to the next shot.",
        "MICA is the Engineer: 15 hull, 4 AP, 3 damage, range 4. Aim at an ally using the cursor, then X to repair up to 6 hull for 2 AP and one of two mission kits. Repair range is 2 tiles. If the cursor has no ally, MICA repairs herself. Invalid orders spend nothing.",
        "Red aim lines and crossed tiles show exactly where enemy fire will land when you end the turn. Move away or disable the attacker. Amber arrows show one-tile enemy movement. Enemies never retarget during their phase. Sentries never move; reserve entry markers show their arrival turns.",
        "Interact with an objective from its tile or one adjacent tile for 2 AP. Rescue assigns its operator to the interacting machine: that carrier must survive and reach E. Capture/hold needs an ally within 1 tile and no enemy within 1 tile for consecutive enemy phases.",
        "Earn a permanent squad upgrade after each of the first five victories: +3 hull, +1 cannon damage or +1 AP; each can be chosen twice. New deployments repair the whole squad and refill two kits. Medals reward surviving machines and quick victories.",
        "You have three mission retries for the campaign. After a setback, Space/X restarts only that mission and discards its score; C concedes the campaign. R restarts the entire campaign. Score gives 100 per disabled enemy plus mission, survivor and speed bonuses. Win the final operation for the liberation ending.",
    ],
    accent: Color::new(0.94, 0.69, 0.28, 1.),
};
fn conf() -> Conf {
    window(META.title)
}
async fn game() {
    ferravex_arcade::run::<Game>(META).await;
}
ferravex_render::game!(conf, game);
