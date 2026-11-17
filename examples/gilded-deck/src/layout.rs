//! Screen geometry shared by drawing and pointer hit-testing, in the arcade
//! host's 960×640 logical canvas. The model hit-tests these rectangles and
//! the view draws into them, so a button can never drift from its click area.
use ferravex_arcade::{Rect, Vec2, vec2};

// Battle --------------------------------------------------------------------

/// The hand fills the span between the energy column and the end-turn column.
const HAND_LEFT: f32 = 142.;
const HAND_RIGHT: f32 = 818.;
const CARD_MAX_W: f32 = 128.;
const CARD_GAP: f32 = 8.;
/// Top edge of a resting hand card, and every hand card's height.
pub(crate) const HAND_TOP: f32 = 412.;
pub(crate) const CARD_H: f32 = 146.;
/// How far the selected card rises; it stays clickable where it is drawn.
pub(crate) const RAISE: f32 = 16.;

/// `(x, width)` of hand position `index` in a hand of `count` cards.
pub(crate) fn hand_slot(index: usize, count: usize) -> (f32, f32) {
    let n = count.max(1) as f32;
    let span = HAND_RIGHT - HAND_LEFT;
    let width = ((span - CARD_GAP * (n - 1.)) / n).min(CARD_MAX_W);
    let total = width * n + CARD_GAP * (n - 1.);
    let x = HAND_LEFT + (span - total) * 0.5 + index as f32 * (width + CARD_GAP);
    (x, width)
}
/// The hand card under `p`, if any.
pub(crate) fn hand_at(p: Vec2, count: usize) -> Option<usize> {
    if !(p.y >= HAND_TOP - RAISE && p.y < HAND_TOP + CARD_H) {
        return None;
    }
    (0..count).find(|&i| {
        let (x, w) = hand_slot(i, count);
        p.x >= x && p.x < x + w
    })
}
/// End the turn (same as X).
pub(crate) const END_TURN: Rect = Rect::new(830., 412., 108., 56.);
/// Fighters' portrait anchors (the figure spans about 70 units above and
/// below) and the centers of their bodies, where hits land.
pub(crate) const HERO: Vec2 = vec2(236., 262.);
pub(crate) const ENEMY: Vec2 = vec2(700., 262.);
/// Health bars sit under the portraits; status badges under the bars.
pub(crate) const BAR_Y: f32 = 344.;
pub(crate) const BADGE_Y: f32 = 380.;
/// The enemy's next move floats above its head.
pub(crate) const INTENT_Y: f32 = 172.;
pub(crate) const ENERGY: Vec2 = vec2(78., 450.);
pub(crate) const DRAW_PILE: Vec2 = vec2(78., 532.);
pub(crate) const DISCARD_PILE: Vec2 = vec2(858., 532.);
pub(crate) const EXHAUST_PILE: Vec2 = vec2(912., 532.);

// Every phase -----------------------------------------------------------------

/// Opens the permanent deck (same as C).
pub(crate) const DECK_BUTTON: Rect = Rect::new(28., 122., 104., 32.);

// Choices: relics, rewards, camp and shop -----------------------------------

/// Where the Liberator stands between battles.
pub(crate) const MENU_HERO: Vec2 = vec2(130., 318.);
const OPTIONS_LEFT: f32 = 244.;
const OPTIONS_RIGHT: f32 = 932.;
const OPTION_TOP: f32 = 182.;
const OPTION_H: f32 = 258.;
/// Horizontal center of the choices, for headlines and buttons.
pub(crate) const OPTIONS_CX: f32 = (OPTIONS_LEFT + OPTIONS_RIGHT) * 0.5;

/// Choice card `index` of `count` (three or four) side-by-side choices.
pub(crate) fn option_rect(index: usize, count: usize) -> Rect {
    let (w, gap) = if count > 3 { (160., 14.) } else { (204., 22.) };
    let n = count.max(1) as f32;
    let total = w * n + gap * (n - 1.);
    let x = OPTIONS_LEFT + (OPTIONS_RIGHT - OPTIONS_LEFT - total) * 0.5 + index as f32 * (w + gap);
    Rect::new(x, OPTION_TOP, w, OPTION_H)
}
/// The choice card under `p` among `count` choices.
pub(crate) fn option_at(p: Vec2, count: usize) -> Option<usize> {
    (0..count).find(|&i| option_rect(i, count).contains(p))
}
/// Passes a card reward (same as X).
pub(crate) const SKIP: Rect = Rect::new(OPTIONS_CX - 80., 458., 160., 44.);

// Forge ---------------------------------------------------------------------

/// Cards per forge and deck page.
pub(crate) const PAGE: usize = 8;
/// Forge grid cell `slot` (0..8) on the current page.
pub(crate) fn forge_cell(slot: usize) -> Rect {
    let (col, row) = ((slot % 4) as f32, (slot / 4) as f32);
    Rect::new(40. + col * 136., 166. + row * 172., 124., 160.)
}
pub(crate) fn forge_at(p: Vec2) -> Option<usize> {
    (0..PAGE).find(|&i| forge_cell(i).contains(p))
}
/// Gild the selected card (same as Space) or return to camp (same as X).
pub(crate) const GILD: Rect = Rect::new(604., 476., 200., 50.);
pub(crate) const BACK: Rect = Rect::new(816., 476., 112., 50.);

// Deck inspector ------------------------------------------------------------

pub(crate) fn deck_cell(slot: usize) -> Rect {
    let (col, row) = ((slot % 4) as f32, (slot / 4) as f32);
    Rect::new(176. + col * 156., 134. + row * 198., 140., 184.)
}
/// Close the inspector (same as C), or turn its pages (same as the arrows).
pub(crate) const DECK_CLOSE: Rect = Rect::new(878., 130., 42., 42.);
pub(crate) const DECK_PREV: Rect = Rect::new(64., 300., 56., 56.);
pub(crate) const DECK_NEXT: Rect = Rect::new(840., 300., 56., 56.);

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hand_slots_fit_between_the_side_columns_without_overlap() {
        for count in 1..=7 {
            let mut right = HAND_LEFT;
            for i in 0..count {
                let (x, w) = hand_slot(i, count);
                assert!(x >= right - 0.01 && w > 80.);
                right = x + w;
                let mid = vec2(x + w * 0.5, HAND_TOP + 40.);
                assert_eq!(hand_at(mid, count), Some(i));
            }
            assert!(right <= HAND_RIGHT + 0.01);
        }
        assert!(END_TURN.left() > HAND_RIGHT && DECK_BUTTON.right() < HAND_LEFT);
        assert_eq!(hand_at(vec2(f32::NAN, 450.), 5), None);
    }
    #[test]
    fn choices_and_buttons_never_overlap() {
        for count in [3, 4] {
            for i in 0..count {
                let r = option_rect(i, count);
                assert_eq!(option_at(r.center(), count), Some(i));
                assert!(r.left() >= OPTIONS_LEFT && r.right() <= OPTIONS_RIGHT);
                assert!(!r.overlaps(&SKIP) && !r.overlaps(&DECK_BUTTON));
            }
        }
        for i in 0..PAGE {
            let r = forge_cell(i);
            assert_eq!(forge_at(r.center()), Some(i));
            assert!(!r.overlaps(&GILD) && !r.overlaps(&BACK) && !r.overlaps(&DECK_BUTTON));
            assert!(r.bottom() < 568.);
            let d = deck_cell(i);
            assert!(!d.overlaps(&DECK_PREV) && !d.overlaps(&DECK_NEXT) && d.bottom() < 560.);
        }
    }
}
