//! Fossil Frontier: six excavation permits and a museum worth opening.
mod content;
pub mod model;
mod view;
use ferravex_arcade::{Color, Conf, Meta, window};
pub use model::{Game, Phase};
/// Campaign identity and complete field manual for the shared host.
pub const META: Meta = Meta {
    id: "fossil-frontier",
    title: "Fossil Frontier",
    subtitle: "Excavation game",
    controls: "1-5 Panels | Arrows Select | Act Dig/Confirm | X Survey | C Tool",
    accent: Color::new(0.91, 0.68, 0.38, 1.),
    help: &[
        "Dig up and identify fossils at six sites to complete the skeleton.",
        "Pages: 1 Dig, 2 Lab, 3 Camp, 4 Museum, 5 Atlas. Arrows move the field cursor. Clicking a field tile selects it. Space uses the chosen tool. C cycles Brush, Chisel and Drill; their sidebar buttons also select them.",
        "X surveys the chosen tile and its four neighbors for one supply and two daylight. A pale trace marks a buried specimen. Numbers show remaining depth; darker stone is harder. Surveying an already known area costs nothing.",
        "Brush removes one layer safely for one supply. Chisel removes up to two for two supplies; Drill removes up to four for three. Hard rock reduces their power. Rough tools damage a fossil if their last cut reaches it, so switch to Brush near the bone.",
        "An exposed specimen needs one more Space action to jacket it, costing one supply and two daylight. An empty bedrock tile gives no reward. A badly damaged signature part requires permit recovery.",
        "Lab: Up/Down selects a sample, Left/Right chooses an identification, Space submits it. Read BOTH anatomical traits and compare the Atlas. Correct research awards money, renown and score once; wrong research loses six coins and one renown.",
        "Work spends daylight rather than real time. Camp can buy twelve supplies for ten coins, or start another field day for twelve coins. Each permit allows three days and five supply packs. Planning and changing pages are free.",
        "Camp also sells two levels each of precision brushes and drill motors. Precision makes brushing faster and reduces rough-tool damage. Motors increase drilling power. Renown gates upgrades; successful permits keep them.",
        "Choose Open Next Site at camp when its obligations are met. Four identified specimens in excellent condition earn three medals. Optional finds, leftover daylight and research renown improve your career score.",
        "Three recovery grants cover the career. Recover Permit at camp, or Space after a setback, restores that site's starting money, tools, supplies and score; its specimens are removed. A fourth recovery request ends the career.",
        "Open the sixth site's museum to finish the Giant skeleton and see your ending. R begins a whole new career. H opens this guide, Esc/P pauses, M sound and F2 controls are shared.",
    ],
};
fn conf() -> Conf {
    window(META.title)
}
async fn game() {
    ferravex_arcade::run::<Game>(META).await;
}
ferravex_render::game!(conf, game);
