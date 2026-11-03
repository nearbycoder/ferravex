//! A complete thirty-council reign with authored consequences and four legacies.
mod content;
pub mod model;
mod view;
use ferravex_arcade::{Color, Conf, Meta, window};
pub use model::{Ending, Game};
/// Identity and readable onboarding used by the shared Ferravex arcade host.
pub const META: Meta = Meta {
    id: "crown-council",
    title: "Crown & Council",
    subtitle: "Kingdom strategy",
    controls: "1-3 Choose | Arrows Select | Act Confirm | X Projects | C History",
    help: &[
        "Make 30 decisions while keeping every resource above zero.",
        "Each council has three authored choices. Press 1, 2 or 3, click a card, or use direction keys and Enter. The forecast lists Coin (Treasury), Folk (Commons), Guard and Faith (Chapel). Every decree includes 2 coin upkeep. Values cap at 100.",
        "Read the sealed decree after each decision, then Enter to continue. Early promises alter later petitions: flood defenses, the Salt Charter, the Orchard Oath, the Open Ledger, schools, courts and a northern peace compact all return.",
        "X opens projects. Press 1-4 or select and Enter to invest once in each work. Costs are immediate; rewards arrive after the stated number of future council decisions. A project cannot rescue a pillar that the current decree reduced to zero. Plans too late to finish cannot be commissioned.",
        "C opens the chronicle. Arrows browse every accepted decree and its actual resource changes, including completed projects. Enter or C returns. X and C can switch between records and investments without spending a council.",
        "Survive thirty councils to earn a Charter, Iron, Commerce or Concordat ending from your most frequent policy. Ties favor Charter, then Iron, then Commerce. Grade S requires all reserves at least 40 and three works; A needs 25 and two works; B needs 15. R begins a fresh reign; H reopens this guide.",
    ],
    accent: Color::new(0.71, 0.50, 0.19, 1.),
};
fn conf() -> Conf {
    window(META.title)
}
async fn game() {
    ferravex_arcade::run::<Game>(META).await;
}
ferravex_render::game!(conf, game);
