//! Logical-canvas geometry shared by drawing and pointer routing. The view
//! draws every clickable control inside these rectangles, and the rules map
//! a click inside one to the key action it mirrors, so a click and a key
//! press always take the same rule path.
use ferravex_math::Rect;

/// The sea chart: sky, ships and shot arcs.
pub(crate) const SCENE: Rect = Rect::new(20., 116., 920., 370.);
/// The gun deck bar under the chart.
pub(crate) const BAR: Rect = Rect::new(20., 494., 920., 74.);

/// Top and height of the gun deck's buttons.
const ROW: f32 = 503.;
const ROW_H: f32 = 56.;
/// Square arrow steppers, vertically centered in the bar.
const STEP: f32 = 30.;
const STEP_Y: f32 = 516.;

/// Angle block and its left/right steppers.
pub(crate) const ANGLE: Rect = Rect::new(32., ROW, 146., ROW_H);
pub(crate) const ANGLE_DOWN: Rect = Rect::new(38., STEP_Y, STEP, STEP);
pub(crate) const ANGLE_UP: Rect = Rect::new(142., STEP_Y, STEP, STEP);
/// Powder block, its charge meter and its down/up steppers.
pub(crate) const POWER: Rect = Rect::new(186., ROW, 268., ROW_H);
pub(crate) const POWER_DOWN: Rect = Rect::new(192., STEP_Y, STEP, STEP);
pub(crate) const POWER_UP: Rect = Rect::new(418., STEP_Y, STEP, STEP);
pub(crate) const METER: Rect = Rect::new(274., 525., 136., 12.);
/// Roundshot, chainshot and shell racks (keys 1-3).
pub(crate) const AMMO: [Rect; 3] = [
    Rect::new(462., ROW, 80., ROW_H),
    Rect::new(548., ROW, 80., ROW_H),
    Rect::new(634., ROW, 80., ROW_H),
];
/// The navigator's firing table toggle (X).
pub(crate) const TABLE: Rect = Rect::new(722., ROW, 52., ROW_H);
/// The fire button (Space).
pub(crate) const FIRE: Rect = Rect::new(782., ROW, 146., ROW_H);

/// Modal card for briefings, the dock and setbacks.
pub(crate) const CARD: Rect = Rect::new(200., 140., 560., 322.);
/// Briefing: begin the battle (Space).
pub(crate) const START: Rect = Rect::new(232., 394., 260., 46.);
/// Dock purchases (keys 1-4).
pub(crate) const SHOP: [Rect; 4] = [
    Rect::new(232., 246., 242., 60.),
    Rect::new(486., 246., 242., 60.),
    Rect::new(232., 314., 242., 60.),
    Rect::new(486., 314., 242., 60.),
];
/// Dock: sail for the next island (Space).
pub(crate) const SAIL: Rect = Rect::new(232., 394., 260., 46.);
/// Setback: spend a lifeboat (C) or retire (Space).
pub(crate) const RETRY: Rect = Rect::new(232., 394., 260., 46.);
pub(crate) const RETIRE: Rect = Rect::new(508., 394., 220., 46.);

#[cfg(test)]
mod tests {
    use super::*;
    fn inside(outer: Rect, inner: Rect) -> bool {
        inner.x >= outer.x
            && inner.y >= outer.y
            && inner.right() <= outer.right()
            && inner.bottom() <= outer.bottom()
    }
    #[test]
    fn controls_sit_inside_their_surfaces_without_overlapping() {
        let deck = [
            ANGLE_DOWN, ANGLE_UP, POWER_DOWN, POWER_UP, AMMO[0], AMMO[1], AMMO[2], TABLE, FIRE,
        ];
        for (i, a) in deck.iter().enumerate() {
            assert!(inside(BAR, *a), "{a:?}");
            for b in &deck[i + 1..] {
                assert!(!a.overlaps(b), "{a:?} overlaps {b:?}");
            }
        }
        assert!(inside(ANGLE, ANGLE_DOWN) && inside(ANGLE, ANGLE_UP));
        assert!(inside(POWER, POWER_DOWN) && inside(POWER, POWER_UP) && inside(POWER, METER));
        let modal = [
            SHOP[0], SHOP[1], SHOP[2], SHOP[3], SAIL, START, RETRY, RETIRE,
        ];
        for rect in modal {
            assert!(inside(CARD, rect), "{rect:?}");
        }
        for (i, a) in SHOP.iter().enumerate() {
            assert!(!a.overlaps(&SAIL));
            for b in &SHOP[i + 1..] {
                assert!(!a.overlaps(b));
            }
        }
        assert!(!RETRY.overlaps(&RETIRE));
        assert!(inside(SCENE, CARD) && SCENE.bottom() < BAR.y);
    }
}
