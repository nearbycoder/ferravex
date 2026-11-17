//! Paper Alibi: six linked mysteries and a newspaper's duty to verify its claims.
//!
//! The pure [`Game`] rules work without a renderer. See [`model`] for a runnable
//! example; the native and browser entry points use the same campaign.
mod content;
pub mod model;
mod view;
use ferravex_arcade::{Color, Conf, Meta, window};
pub use model::Game;

/// Identity, default controls and the complete in-game field guide.
pub const META: Meta = Meta {
    id: "paper-alibi",
    title: "Paper Alibi",
    subtitle: "Detective deduction",
    controls: "1-4 Tabs | Arrows Select | Act Inspect | C Connect | X Notebook",
    help: &[
        "Examine places and interview every witness.",
        "Pin a claim beside the fact it breaks; connect.",
        "Confront the liar, then accuse: who, how and why.",
        "Wrong accusations cost hearts; four end the run.",
    ],
    accent: Color::new(0.87, 0.66, 0.36, 1.),
};
fn conf() -> Conf {
    window(META.title)
}
async fn game() {
    ferravex_arcade::run::<Game>(META).await;
}
ferravex_render::game!(conf, game);
