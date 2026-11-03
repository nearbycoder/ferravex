//! Velvet Heist: six stealth infiltrations to return a city's stolen history.
mod content;
pub mod model;
mod view;
use ferravex_arcade::{Color, Conf, Meta, window};
pub use model::{Game, Phase};
/// Identity, bindings and the complete stealth field guide.
pub const META: Meta = Meta {
    id: "velvet-heist",
    title: "Velvet Heist",
    subtitle: "Stealth game",
    controls: "Arrows Move | Act Interact | X Crouch | C Distract",
    accent: Color::new(0.88, 0.65, 0.94, 1.),
    help: &[
        "Collect every record and return to the exit without getting caught.",
        "Arrows or WASD move. X toggles crouching: quiet movement is slower but violet shadows and H cabinets conceal you unless a guard is immediately adjacent. Standing moves twice as fast and nearby guards hear your steps.",
        "Guard cones show normal standing vision. Crouching shortens sight range. Solid walls and closed doors block sight. A guard who spots you searches your last location, then returns to the patrol after three seconds without a clue.",
        "Space works on your tile or an adjacent object. K grants a reusable brass pass; = doors require it. Taking records and opening doors takes a moment while guards keep moving. Moving cancels unfinished work.",
        "The green T console takes 1.5 seconds to rewire and disables every red beam for that heist. Crossing a live beam adds 45 percent alarm. Two crossings plus a brief sighting can trigger lockdown.",
        "Click a clear tile to mark a coin target, then C throws within five steps. Without a marked tile, C throws three steps in your facing direction. Walls block throws. Nearby guards investigate the landing. You have three coins per heist.",
        "Being seen raises the alarm. Break sight and stay unseen for one second to let it decay. A guard sharing your tile, a full alarm or the heist deadline causes capture. Planning during play still spends time; H and Esc/P pause.",
        "A clean heist with both optional artworks earns three medals; a clean required extraction earns two; an alarmed extraction earns one. Faster extraction adds points, but no points come from repeated invalid interactions.",
        "Three recovery favors allow local retries. Space after capture spends one and restores that heist's starting score, objects, time and guards. A fourth failure ends the career. Space after extraction continues. R starts a whole new career.",
    ],
};
fn conf() -> Conf {
    window(META.title)
}
async fn game() {
    ferravex_arcade::run::<Game>(META).await;
}
ferravex_render::game!(conf, game);
