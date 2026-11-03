//! Pantry Panic: build a neighborhood restaurant through seven complete services.
//! [`Game`] contains the pure fixed-tick kitchen model, shared by native and Wasm.
mod content;
pub mod model;
mod view;
use ferravex_arcade::{Color, Conf, Meta, window};
pub use model::Game;
/// Campaign identity, control hints and the shared host's paginated kitchen guide.
pub const META: Meta = Meta {
    id: "pantry-panic",
    title: "Pantry Panic",
    subtitle: "Cooking game",
    controls: "Arrows Move | Act Use/Hold to Chop | X Discard | C Recipe | 1-3 Upgrade",
    help: &[
        "Prepare the ordered meals and meet the quota in seven services.",
        "Arrows or WASD move the cook. Stand on the marked floor tile beside a station and tap Space to use it. Clicking a station walks to its tile; you still press Space to work.",
        "Each pantry ingredient costs 2 coins. Collect one with empty hands, then go to CHOP and HOLD Space until the progress bar fills. Moving away cancels partial chopping. Rice is rinsed and portioned at this bench too.",
        "Take each prepared ingredient to MIX and tap Space to add it. Once the bowl contains an exact recipe, tap Space with empty hands to lift the whole bowl. Duplicate or raw ingredients are rejected.",
        "Garden Salad goes directly to PLATE. All other bowls must cook on either burner. Tap Space to deposit a bowl, then return with empty hands when its timer turns green. Food burns after five extra seconds; burner upgrades extend this grace.",
        "Take cooked food to PLATE, then deliver the plated dish at SERVE. Only a matching, waiting guest pays. Faster service earns better tips. You can prepare another dish while the first one cooks.",
        "C cycles six recipe cards. Click an order ticket to see its recipe. The top tickets show each guest's patience; next arrivals and the overall closing time are predictable. The H field guide pauses play.",
        "X discards held food without a refund. With empty hands at MIX it clears a mistaken bowl; at a burned pot it cleans that burner. Discarding and burning lower your service medal, so keep your recipe exact.",
        "Between services, 1 buys a sharper knife, 2 faster shoes and 3 safer burners. Each has two levels. Keep enough coins for ingredients. Space continues to the next briefing, and Space again opens the doors.",
        "A failed service offers three shared retries across the career. Retrying restores the cash, score and dishes served from the start of that shift; it cannot farm earnings. A fourth failure ends the career. R starts a whole new restaurant.",
    ],
    accent: Color::new(1., 0.62, 0.3, 1.),
};
fn conf() -> Conf {
    window(META.title)
}
async fn game() {
    ferravex_arcade::run::<Game>(META).await;
}
ferravex_render::game!(conf, game);
