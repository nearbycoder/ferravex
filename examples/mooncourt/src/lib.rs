//! Mooncourt: six lunar rivals, a racket, and the Earthrise Cup.
mod content;
pub mod model;
mod view;
use ferravex_arcade::{Color, Conf, Meta, window};
pub use model::{Game, Phase};
/// Identity and complete player-facing rules for the shared campaign host.
pub const META: Meta = Meta {
    id: "mooncourt",
    title: "Mooncourt",
    subtitle: "Racket sports",
    controls: "Arrows Move | 1-3 Aim | Act Drive | X Lob | C Smash",
    accent: Color::new(0.58, 0.83, 0.98, 1.),
    help: &[
        "Win two games per match and beat all six opponents.",
        "Arrows or WASD move freely on your half of the court. Your astronaut is blue at the bottom. The racket reaches nearby balls at racket height; use the ground shadow to judge position.",
        "1 aims left, 2 center, 3 right on your opponent's court. The three target buttons also respond to clicks. Choose your target before you swing; aiming never spends stamina.",
        "Tap Space for a drive, X for a high recovery lob or C for a fast smash. A press opens a short swing window. Holding a key does not repeat it. The racket cannot return your own shot.",
        "A smash needs at least 26 stamina and a ball at least 14 units above ground; otherwise it becomes a drive. Low shots near the net can hit its tape. Drives cost 7 stamina, lobs 4, smashes 26; a swing also costs 2.",
        "The ring shows the incoming landing spot. Reach it before the ball, then step toward the net to meet the ball higher. A gold ring around you and SWING NOW mark a reachable return. One bounce is allowed. A second bounce, an out ball or a net fault ends the point.",
        "Your serve waits for Space, X or C and uses a safe drive. The opponent serves after a short countdown. Service changes after each game. Aim away from the receiver to open the rally.",
        "Stamina recovers during play and between points. Below 12 stamina you move more slowly. Lobs give breathing room. Return toward center after sending the ball wide.",
        "After a match, select 1 speed, 2 recovery or 3 racket reach. Each training track has three levels. Space may skip training. Each rival has a court briefing and a distinct movement and shot style.",
        "A lost match can be retried with a wildcard. You have three for the whole tournament. A retry restores the start-of-match score, so lost matches cannot farm points. The fourth loss ends the run.",
        "Points, games and match wins earn score; winning 2-0 earns three medals, 2-1 earns two. Complete all six matches for the trophy. H guide, P pause, R new tournament, M sound.",
    ],
};
fn conf() -> Conf {
    window(META.title)
}
async fn game() {
    ferravex_arcade::run::<Game>(META).await;
}
ferravex_render::game!(conf, game);
