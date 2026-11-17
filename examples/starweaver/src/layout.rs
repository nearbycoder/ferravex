//! Canvas regions shared by the rules and the view.
//!
//! The model hit-tests clicks with these functions and the view uses the same
//! ones for hover highlights and effect anchors, so a highlighted thing always
//! does what a click on it does. All values are logical canvas units.
use crate::content::CHART;
use ferravex_arcade::{Vec2, vec2};

/// Panel tabs: five 176-unit tabs every 184 units from x 24, y 116..145.
pub(crate) const TAB_X: f32 = 24.;
pub(crate) const TAB_TOP: f32 = 116.;
pub(crate) const TAB_H: f32 = 29.;
pub(crate) const TAB_STEP: f32 = 184.;
pub(crate) const TAB_W: f32 = 176.;
/// The right-hand action column.
pub(crate) const COL: f32 = 640.;
pub(crate) const COL_W: f32 = 282.;
/// Action rows: 30 tall, every 34 units.
pub(crate) const ROW_H: f32 = 30.;
pub(crate) const ROW_STEP: f32 = 34.;
/// The confirm button under the action column.
pub(crate) const BUTTON_Y: f32 = 526.;
pub(crate) const BUTTON_H: f32 = 30.;
/// Encounter approaches: 700 × 48 cards every 62 units from (130, 330).
pub(crate) const CHOICE_X: f32 = 130.;
pub(crate) const CHOICE_W: f32 = 700.;
pub(crate) const CHOICE_TOP: f32 = 330.;
pub(crate) const CHOICE_STEP: f32 = 62.;
pub(crate) const CHOICE_H: f32 = 48.;
/// A click this close to a star selects it.
pub(crate) const STAR_RADIUS: f32 = 29.;

/// The tab under `p`, if any.
pub(crate) fn tab_at(p: Vec2) -> Option<usize> {
    let inside = (TAB_TOP..TAB_TOP + TAB_H).contains(&p.y) && (TAB_X..936.).contains(&p.x);
    inside.then(|| ((p.x - TAB_X) / TAB_STEP) as usize)
}
/// The star under `p`; the last charted match wins, as it always has.
pub(crate) fn star_at(p: Vec2) -> Option<usize> {
    let mut hit = None;
    for (i, site) in CHART.iter().enumerate() {
        if (p - vec2(site.x, site.y)).length_squared() < STAR_RADIUS * STAR_RADIUS {
            hit = Some(i);
        }
    }
    hit
}
/// Top of the first action row on `page`; Survey leaves room for the hold.
pub(crate) fn row_top(page: usize) -> f32 {
    if page == 1 { 282. } else { 230. }
}
/// The row of `page` (with `rows` entries) under `p`, if any.
pub(crate) fn row_at(page: usize, rows: usize, p: Vec2) -> Option<usize> {
    if !(COL..COL + COL_W).contains(&p.x) {
        return None;
    }
    let top = row_top(page);
    (0..rows).find(|&i| {
        let y = top + i as f32 * ROW_STEP;
        (y..y + ROW_H).contains(&p.y)
    })
}
/// Whether `p` is on the confirm button.
pub(crate) fn button_hit(p: Vec2) -> bool {
    (COL..COL + COL_W).contains(&p.x) && (BUTTON_Y..BUTTON_Y + BUTTON_H).contains(&p.y)
}
/// The encounter approach under `p`, if any.
pub(crate) fn choice_at(p: Vec2) -> Option<usize> {
    if !(CHOICE_X..CHOICE_X + CHOICE_W).contains(&p.x) {
        return None;
    }
    (0..3).find(|&i| {
        let y = CHOICE_TOP + i as f32 * CHOICE_STEP;
        (y..y + CHOICE_H).contains(&p.y)
    })
}

/// Center of star `i` on the chart.
pub(crate) fn star(i: usize) -> Vec2 {
    vec2(CHART[i].x, CHART[i].y)
}
/// Center of action row `row` on `page`.
pub(crate) fn row_center(page: usize, row: usize) -> Vec2 {
    vec2(
        COL + COL_W * 0.5,
        row_top(page) + row as f32 * ROW_STEP + ROW_H * 0.5,
    )
}
/// Center of the confirm button.
pub(crate) fn button_center() -> Vec2 {
    vec2(COL + COL_W * 0.5, BUTTON_Y + BUTTON_H * 0.5)
}
/// Center of encounter approach `i`.
pub(crate) fn choice_center(i: usize) -> Vec2 {
    vec2(
        CHOICE_X + CHOICE_W * 0.5,
        CHOICE_TOP + i as f32 * CHOICE_STEP + CHOICE_H * 0.5,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regions_match_their_anchors_and_never_overlap() {
        for i in 0..5 {
            let x = TAB_X + i as f32 * TAB_STEP + TAB_W * 0.5;
            assert_eq!(tab_at(vec2(x, TAB_TOP + 10.)), Some(i));
        }
        assert_eq!(tab_at(vec2(935., 130.)), Some(4));
        assert_eq!(tab_at(vec2(500., 150.)), None);
        for i in 0..CHART.len() {
            assert_eq!(star_at(star(i)), Some(i));
        }
        for page in 1..4 {
            for row in 0..6 {
                assert_eq!(row_at(page, 6, row_center(page, row)), Some(row));
            }
            assert_eq!(row_at(page, 6, vec2(COL - 1., row_center(page, 0).y)), None);
        }
        assert!(button_hit(button_center()));
        for i in 0..3 {
            assert_eq!(choice_at(choice_center(i)), Some(i));
        }
        assert_eq!(choice_at(vec2(480., CHOICE_TOP + CHOICE_H + 2.)), None);
    }
}
