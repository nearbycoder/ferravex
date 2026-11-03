//! Gravity Post: eight planetary deliveries and a complete courier career.
mod content;
pub mod model;
mod view;
use ferravex_arcade::{Color, Conf, Meta, window};
pub use model::{Game, Phase};

/// Identity and complete instructions for the shared campaign host.
pub const META: Meta = Meta {
    id: "gravity-post",
    title: "Gravity Post",
    subtitle: "Lander deliveries",
    controls: "Left/Right Tilt | Up/Act Thrust | Down Brake | X Service | C Retry",
    help: &[
        "Pick up cargo, land safely at the delivery pad, and complete eight jobs.",
        "Left and right tilt the ship, up or held Space fires the main engine. Tilt is limited to 65 degrees. Thrust accelerates along the ship's axis; gravity and wind keep acting after you release it.",
        "Hold Down for a fuel-burning retro-brake. It levels the ship and approaches a slow downward drift. It overrides the main engine. Use it for the final approach, not the entire journey.",
        "The instruments show horizontal speed, descent speed and tilt. Land centered on a pad, nearly upright and below both printed speed limits. Fast or tilted impacts damage the hull and any carried parcel.",
        "Mountain ridges are solid. Climb before crossing them. The ship's feet need the full width of a pad. Borders and rough ground cause damage; destroyed hull or parcel means a wreck.",
        "X works only while landed. The depot, pickup and purple outpost have finite fuel reserves. The outpost also repairs up to 30 hull once per contract. Repeated service cannot create supplies or points.",
        "A carried parcel adds mass, reducing engine acceleration. The optional diamond survey beacon earns 250 points, 20 credits and up to 15 fuel once. It can be skipped without affecting certification.",
        "After delivery, 1 improves the engine, 2 expands the tank and 3 improves landing gear. Every upgrade has two levels. Upgrades persist through the career; Space opens the next contract.",
        "Each world has a storm deadline. A wreck offers a recovery permit; Space spends one to restart that contract. C requests recovery during flight. Three permits cover the whole career; a fourth wreck ends it.",
        "Recovery rolls back the failed contract's earnings, beacon and cargo progress, so it cannot farm rewards. Earlier deliveries and refits remain. R starts a completely new career.",
        "Three stamps reward an intact parcel and hull; two reward a successful but battered delivery. Fuel, remaining time and optional beacons improve the final score. H guide, Esc/P pause and M sound are shared controls.",
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
