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
    subtitle: "Detective puzzles",
    controls: "1-4 Tabs | Arrows Select | Act Inspect | X Notebook | C Connect",
    help: &[
        "Solve six cases by collecting evidence and checking witness statements.",
        "Use 1 Locations, 2 People, 3 Notebook, 4 Theory. Up/Down selects a row. Space performs its action. Click a tab or row to select it; click the large action buttons to act. There is no time limit: read at your own pace.",
        "At Locations, Space examines the selected scene. At People, Space records a statement. Physical records are independently verified facts; statements may be lies. Record all six before filing a conclusion.",
        "In Notebook, select a collected record and press Space to pin it. Pin a second record, then C tests the connection. Look for a witness's denial that directly conflicts with physical evidence. X clears the pins without losing evidence.",
        "Once a contradiction is connected, return to People, select its witness, and press C to confront them. The signed response is added as C1 in Notebook. It explains the method and motive; read it before accusing.",
        "In Theory, Up/Down chooses WHO, HOW or WHY; Left/Right cycles its answer. Clicking an answer cycles it too. Space files all three. A false supported accusation costs one credibility point. Losing all four ends the campaign.",
        "Each solved case has an explanatory edition. Space opens the next dossier. Finish all six for the final newspaper and a reporter rank. New evidence, valid connections and solved cases score only once; careful accusations earn more.",
        "R begins a fresh six-case campaign. H opens this guide; Esc/P pauses. The shared host saves best scores and completed runs, but an unfinished investigation is not saved. Autoplay uses ordinary commands and leaves your records unchanged.",
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
