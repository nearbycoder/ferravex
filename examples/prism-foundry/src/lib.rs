//! Prism Foundry: twelve authored optical logic puzzles and a complete restoration campaign.
mod content;
pub mod model;
mod view;
use ferravex_arcade::{Color, Conf, Meta, window};
pub use model::Game;
/// Static identity and complete rules for the shared campaign host.
pub const META: Meta = Meta {
    id: "prism-foundry",
    title: "Prism Foundry",
    subtitle: "Laser puzzles",
    controls: "Arrows Select | Act Rotate | X Undo | C Reset | 4 Hint | 5 Check",
    help: &[
        "Turn mirrors and filters to light every target. Solve twelve puzzles.",
        "Move the gold cursor with arrows or WASD. Space rotates a mirror, splitter or filter under it. Clicking a device selects and rotates it immediately. Empty cells, sources, locks and stone cannot rotate.",
        "Mirrors alternate / and backslash orientations. A splitter sends one beam straight through and reflects a second branch. Sources and stone absorb incoming light. Empty crossings leave both beams traveling straight.",
        "White contains red, green and blue. Diamond filters cycle R, G, B and pass only that channel; incompatible colors are stopped. Filter after a splitter to give each branch a different color.",
        "Colors combine at a lock: R+G=Y (yellow), R+B=M (magenta), G+B=C (cyan), and R+G+B=W (white). The lock requires an exact match; extra colors spoil it. Letters supplement the beam colors.",
        "X undoes the last turn, restoring its earlier light path and turn count. The most recent 128 turns are retained. C resets the current chamber. Neither can change already sealed chambers or farm points.",
        "Three stars require no hints, resets or rejected seals and no more turns than par. Two stars allow up to twice par and two resets; otherwise completion earns one. A hint caps the chamber at two stars. There is no time limit and local retries are unlimited.",
        "Press 4 for the engineer's chamber-specific hint. Letters A-K and rows 1-7 identify workbench cells. Keep following each light path from its source. In the last wing, solve each colored branch before recombining them.",
        "Seal all twelve chambers to awaken the Daybreak Engine and earn your engineering rank. R restarts the entire campaign. H opens this guide; Esc/P pauses; M toggles sound.",
    ],
    accent: Color::new(0.91, 0.73, 0.39, 1.),
};
fn conf() -> Conf {
    window(META.title)
}
async fn game() {
    ferravex_arcade::run::<Game>(META).await;
}
ferravex_render::game!(conf, game);
