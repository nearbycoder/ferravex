//! Deep Pressure is a complete six-sortie submarine rescue campaign.
//! See [`model`] for renderer-independent fixed-tick rules and an executable example.
mod content;
pub mod model;
mod view;

use ferravex_arcade::{Color, Conf, Meta, window};
pub use model::Game;
/// Game identity and player-facing rules consumed by the common arcade host.
pub const META: Meta = Meta {
    id: "deep-pressure",
    title: "Deep Pressure",
    subtitle: "Submarine rescue",
    controls: "Arrows Thrust | Act Rescue/Dock | X Ballast | C Sonar | 1-3 Refit",
    help: &[
        "Rescue 27 divers across six sorties. Return to base before oxygen runs out.",
        "Arrows or WASD thrust. Inertia carries you after releasing a direction. X changes ballast: light ballast rises; neutral ballast holds depth.",
        "Approach a cyan air bell slowly, then tap Space to winch its survivor aboard. Your first cabin has two berths. Hold Space to brake, then release and tap again.",
        "Return to the yellow dock at the upper left. Slow down and tap Space to unload survivors, refill oxygen and repair your hull without charge.",
        "Air drains continuously while diving. Passengers and deep water use more. Watch the instrument panel and leave time for the trip home.",
        "Avoid reefs, drifting jellyfish, mines and cycling thermal vents. C spends two seconds of air on sonar, revealing exact danger circles for three seconds.",
        "Each landed survivor pays 12 credits. Between sorties, 1 buys air, 2 armor and 3 cabin space. Each refit has two levels. Arrow selection plus X, or clicking a card, also buys it.",
        "A wreck loses that sortie's progress and earnings. Space spends one of three recovery tugs to retry. Earlier sorties and purchased refits survive; a fourth wreck ends the career.",
        "Bronze, silver and gold sortie medals reward careful, timely rescues. Space launches or continues. R restarts the whole career; H help; Esc/P pause; M sound.",
    ],
    accent: Color::new(0.27, 0.87, 0.88, 1.),
};
fn conf() -> Conf {
    window(META.title)
}
async fn game() {
    ferravex_arcade::run::<Game>(META).await;
}
ferravex_render::game!(conf, game);
