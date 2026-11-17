//! Screen geometry shared by drawing and pointer hit-testing, in the 960×640
//! logical canvas, so every clickable shape is exactly where it is drawn.
use ferravex_arcade::{Rect, Vec2, vec2};
use ferravex_play::navigation::Cell;

/// Board tile edge.
pub(crate) const TILE: f32 = 54.;
/// Top-left corner of tile (0, 0).
pub(crate) const BOARD: Vec2 = vec2(25., 150.);
/// The program panel on the right.
pub(crate) const PANEL: Rect = Rect::new(530., 116., 414., 452.);
const LEFT: f32 = 540.;
const WIDTH: f32 = 394.;
/// Instruction palette tiles.
const PALETTE_TOP: f32 = 128.;
const PALETTE_W: f32 = 51.;
const PALETTE_H: f32 = 64.;
/// Tape cells: four rows of eight.
const TAPE_TOP: f32 = 210.;
pub(crate) const CELL: f32 = 44.;
pub(crate) const CELL_GAP: f32 = 6.;
pub(crate) const COLUMNS: usize = 8;
/// Vertical center of the medal, step and fault readouts.
pub(crate) const STATS_Y: f32 = 432.;
const BUTTON_TOP: f32 = 458.;
const BUTTON_W: f32 = 61.;
const BUTTON_H: f32 = 72.;
/// The solved card's "next workbench" button.
pub(crate) const NEXT: Rect = Rect::new(540., 478., 394., 62.);

/// Center of a board cell.
pub(crate) fn center(cell: Cell) -> Vec2 {
    BOARD + vec2((cell.x as f32 + 0.5) * TILE, (cell.y as f32 + 0.5) * TILE)
}
/// Palette tile `i` (0..7, in [`crate::model::Op::ALL`] order).
pub(crate) fn palette(i: usize) -> Rect {
    let step = (WIDTH - PALETTE_W) / 6.;
    Rect::new(LEFT + i as f32 * step, PALETTE_TOP, PALETTE_W, PALETTE_H)
}
/// Tape cell `i` (0..32), eight to a row.
pub(crate) fn tape(i: usize) -> Rect {
    Rect::new(
        LEFT + (i % COLUMNS) as f32 * (CELL + CELL_GAP),
        TAPE_TOP + (i / COLUMNS) as f32 * (CELL + CELL_GAP),
        CELL,
        CELL,
    )
}
/// The palette tile under `p`.
pub(crate) fn palette_at(p: Vec2) -> Option<usize> {
    (0..7).find(|&i| palette(i).contains(p))
}
/// The tape cell under `p`.
pub(crate) fn tape_at(p: Vec2) -> Option<usize> {
    (0..crate::model::CAPACITY).find(|&i| tape(i).contains(p))
}

/// The execution and editing buttons, left to right.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Button {
    Run,
    Step,
    Stop,
    Undo,
    Clear,
    Hint,
}
impl Button {
    pub(crate) const ALL: [Self; 6] = [
        Self::Run,
        Self::Step,
        Self::Stop,
        Self::Undo,
        Self::Clear,
        Self::Hint,
    ];
    pub(crate) fn rect(self) -> Rect {
        let step = (WIDTH - BUTTON_W) / 5.;
        Rect::new(
            LEFT + self as usize as f32 * step,
            BUTTON_TOP,
            BUTTON_W,
            BUTTON_H,
        )
    }
    /// The numbered key this button mirrors; Stop mirrors C instead.
    pub(crate) fn slot(self) -> Option<usize> {
        match self {
            Self::Run => Some(1),
            Self::Step => Some(2),
            Self::Stop => None,
            Self::Undo => Some(3),
            Self::Clear => Some(4),
            Self::Hint => Some(5),
        }
    }
    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::Run => "1",
            Self::Step => "2",
            Self::Stop => "C",
            Self::Undo => "3",
            Self::Clear => "4",
            Self::Hint => "5",
        }
    }
    pub(crate) fn at(p: Vec2) -> Option<Self> {
        Self::ALL.into_iter().find(|b| b.rect().contains(p))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn interactive_shapes_stay_inside_the_play_area_and_never_overlap() {
        let mut rects: Vec<Rect> = (0..7).map(palette).collect();
        rects.extend((0..32).map(tape));
        rects.extend(Button::ALL.map(Button::rect));
        for (i, a) in rects.iter().enumerate() {
            assert!(a.x >= PANEL.x && a.right() <= PANEL.right(), "{a:?}");
            assert!(a.y >= PANEL.y && a.bottom() <= PANEL.bottom(), "{a:?}");
            for b in &rects[i + 1..] {
                assert!(!a.overlaps(b), "{a:?} overlaps {b:?}");
            }
        }
        assert!(NEXT.bottom() <= PANEL.bottom() && NEXT.right() <= PANEL.right());
        let corner = center(Cell { x: 8, y: 6 }) + vec2(TILE, TILE) * 0.5;
        assert!(corner.x < PANEL.x && corner.y <= 568.);
        assert_eq!(tape_at(tape(9).center()), Some(9));
        assert_eq!(palette_at(palette(6).center()), Some(6));
        assert_eq!(Button::at(Button::Hint.rect().center()), Some(Button::Hint));
        assert_eq!(Button::at(vec2(-5., -5.)), None);
    }
}
