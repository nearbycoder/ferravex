//! Stormbound: eight islands, one ship, and cannonballs that obey the wind.
mod content;
mod layout;
pub mod model;
mod view;
use ferravex_arcade::{Color, Conf, Meta, window};
pub use model::{Game, Phase};
/// Shared host identity, tutorial and default controls.
pub const META: Meta = Meta {
    id: "stormbound",
    title: "Stormbound",
    subtitle: "Naval artillery",
    controls: "Arrows Aim | Act Fire | 1-3 Ammo | X Hint | C Retreat | 1-4 Shipyard",
    help: &[
        "Sink all three hull sections of every enemy ship.",
        "Wind bends every shot; follow the dotted arc.",
        "Chain fells masts; shells ignore armor and splash.",
        "Spend salvage at the shipyard. Lifeboats retry islands.",
    ],
    accent: Color::new(0.97, 0.76, 0.37, 1.),
};
fn conf() -> Conf {
    window(META.title)
}
async fn game() {
    ferravex_arcade::run::<Game>(META).await;
}
ferravex_render::game!(conf, game);

#[cfg(test)]
mod tests {
    use super::META;
    use ferravex_arcade::{PAPER, style, typography::text_width};
    #[test]
    fn help_is_four_short_bullets_on_one_guide_page() {
        assert!(META.help.len() <= 4);
        for line in META.help {
            assert!(line.split_whitespace().count() < 15, "{line}");
            // The host's guide wraps at 600 units of its 18-unit face.
            assert!(text_width(line, style(18., PAPER)) <= 600., "{line}");
        }
        assert!(META.subtitle.split_whitespace().count() <= 3);
    }
}
