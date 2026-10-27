//! Pure screen layout. Wide landscape canvases get a full-height table between
//! a backglass/score panel and a dot-matrix/work-orders panel; compact
//! canvases stack a header, the table, the dot-matrix display and one hint.
//! Both keep the browser fullscreen menu corner clear of HUD content.
use super::{HEADER, HUD_MAX};
use ferravex_render::prelude::*;

/// Browser fullscreen pins a 44×44 menu button at the canvas's top-right,
/// inset 8px. HUD text and controls stay out of this `(width, height)` corner.
pub(super) const MENU_CORNER: (f32, f32) = (72., 60.);
const MARGIN: f32 = 24.;
const GAP: f32 = 27.;
const PANEL_MIN: f32 = 260.;
const PANEL_MAX: f32 = 380.;
/// Side panels start below the menu corner so both columns line up.
const PANEL_TOP: f32 = MENU_CORNER.1 + 4.;
/// Dot-matrix outer height: a 6px bezel around 22 dot rows.
pub(super) const DMD_BEZEL: f32 = 6.;

/// Compact header geometry in logical pixels. Wide-enough rows put Reset at
/// the right; when that row would reach the menu corner, the score row stops
/// short of it and Reset, best and multiplier move to a second row below it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct HudLayout {
    /// Height reserved above the table.
    pub header: f32,
    pub left: f32,
    /// Right edge of the ball lamps and their label.
    pub balls_right: f32,
    /// Width available to the score line.
    pub score_room: f32,
    /// Width available to the best line before the multiplier label.
    pub best_room: f32,
    pub best_y: f32,
    pub mult_right: f32,
    pub mult_y: f32,
    pub reset: Rect,
}

pub(super) fn hud_layout(w: f32) -> HudLayout {
    let hud_width = (w - 24.).clamp(1., HUD_MAX);
    let left = (w - hud_width) * 0.5;
    let right = (w + hud_width) * 0.5;
    if right <= w - MENU_CORNER.0 {
        let reset = Rect::new(right - 68., 8., 68., 44.);
        let balls_right = reset.x - 14.;
        HudLayout {
            header: HEADER,
            left,
            balls_right,
            score_room: (balls_right - 70. - left).max(40.),
            best_room: (balls_right - 96. - left).max(40.),
            best_y: 76.,
            mult_right: balls_right,
            mult_y: 76.,
            reset,
        }
    } else {
        let balls_right = w - MENU_CORNER.0 - 4.;
        let reset = Rect::new(right - 68., MENU_CORNER.1 + 4., 68., 44.);
        HudLayout {
            header: MENU_CORNER.1 + 52.,
            left,
            balls_right,
            score_room: (balls_right - 70. - left).max(40.),
            best_room: (reset.x - 12. - 76. - left).max(40.),
            best_y: reset.y + 28.,
            mult_right: reset.x - 12.,
            mult_y: reset.y + 28.,
            reset,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Layout {
    /// Side panels beside a full-height table.
    pub wide: bool,
    pub scale: f32,
    pub origin: Vec2,
    pub board: Rect,
    /// Compact header (also computed for wide layouts, but unused there).
    pub header: HudLayout,
    /// Wide side panels; zero-sized in compact layouts.
    pub left: Rect,
    pub right: Rect,
    /// Dot-matrix display including its bezel.
    pub dmd: Rect,
    /// Baseline of the compact hint line (wide layouts use the right panel).
    pub hint_y: f32,
    pub reset: Rect,
    /// Lowest y a centered card may start at.
    pub card_top: f32,
}

/// Device-aligned dot pitch: at least two logical pixels per dot, rounded
/// to whole device pixels so the dot grid never shimmers.
pub(super) fn dot_pitch(dpi: f32, large: bool) -> f32 {
    let dpi = if dpi.is_finite() && dpi > 0. { dpi } else { 1. };
    let logical = if large { 2.5 } else { 2. };
    (logical * dpi).round().max(2.) / dpi
}

pub(super) fn layout(w: f32, h: f32) -> Layout {
    let header = hud_layout(w);
    let tall_scale = ((h - 2. * MARGIN) / 16.).min((w - 28.) / 10.5).max(1.);
    let board_w = 10.3 * tall_scale;
    let side = (w - board_w) * 0.5 - GAP - MARGIN;
    if w > h && side >= PANEL_MIN {
        let scale = tall_scale;
        let origin = vec2(w * 0.5 - 5.15 * scale, MARGIN);
        let board = Rect::new(origin.x, origin.y, board_w, 16. * scale);
        let panel_w = side.min(PANEL_MAX);
        let panel_h = h - PANEL_TOP - MARGIN;
        let left = Rect::new(board.x - GAP - panel_w, PANEL_TOP, panel_w, panel_h);
        let right = Rect::new(board.x + board.w + GAP, PANEL_TOP, panel_w, panel_h);
        let dmd_h = 22. * 2.5 + 2. * DMD_BEZEL;
        let dmd = Rect::new(right.x, right.y, right.w, dmd_h);
        let reset = Rect::new(left.x, left.y + left.h - 44., 128_f32.min(left.w), 44.);
        Layout {
            wide: true,
            scale,
            origin,
            board,
            header,
            left,
            right,
            dmd,
            hint_y: dmd.y + dmd.h + 30.,
            reset,
            card_top: MARGIN,
        }
    } else {
        let dmd_h = 22. * 2. + 2. * DMD_BEZEL;
        let footer = 12. + dmd_h + 22. + 6.;
        let scale = ((h - header.header - 4. - footer) / 16.)
            .min((w - 28.) / 10.5)
            .max(1.);
        let origin = vec2(w * 0.5 - 5.15 * scale, header.header + 4.);
        let board = Rect::new(origin.x, origin.y, 10.3 * scale, 16. * scale);
        let width = board.w.max((w - 24.).clamp(1., 400.));
        let dmd = Rect::new(w * 0.5 - width * 0.5, board.y + board.h + 12., width, dmd_h);
        Layout {
            wide: false,
            scale,
            origin,
            board,
            header,
            left: Rect::new(0., 0., 0., 0.),
            right: Rect::new(0., 0., 0., 0.),
            dmd,
            hint_y: dmd.y + dmd.h + 20.,
            reset: header.reset,
            card_top: header.header + 4.,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) const SIZES: [(f32, f32); 9] = [
        (1280., 800.),
        (1600., 1000.),
        (390., 844.),
        (844., 390.),
        (320., 480.),
        (320., 540.),
        (390., 694.),
        (800., 1000.),
        (1024., 768.),
    ];

    fn clear_of_corner(name: &str, r: Rect, w: f32) {
        let in_corner = r.x + r.w > w - MENU_CORNER.0 && r.y < MENU_CORNER.1;
        assert!(
            !in_corner,
            "{name} {r:?} enters the menu corner at width {w}"
        );
    }

    #[test]
    fn layouts_fit_every_canvas_and_keep_the_menu_corner_clear() {
        for (w, h) in SIZES {
            let l = layout(w, h);
            let inside = |name: &str, r: Rect| {
                assert!(
                    r.x >= 4. && r.y >= 0. && r.x + r.w <= w - 4. && r.y + r.h <= h - 4.,
                    "{name} {r:?} inside {w}x{h}"
                );
                clear_of_corner(name, r, w);
            };
            inside("board", l.board);
            inside("dmd", l.dmd);
            inside("reset", l.reset);
            assert!(l.reset.w >= 44. && l.reset.h >= 44.);
            assert!(l.dmd.w >= 260., "dmd too narrow at {w}x{h}");
            if l.wide {
                inside("left panel", l.left);
                inside("right panel", l.right);
                assert!(l.left.x + l.left.w <= l.board.x - 16.);
                assert!(l.right.x >= l.board.x + l.board.w + 16.);
                assert!(l.reset.y + l.reset.h <= l.left.y + l.left.h + 0.01);
                assert!(l.hint_y <= h - 8.);
            } else {
                assert!(l.board.y >= l.header.header, "table clears the header");
                assert!(l.dmd.y >= l.board.y + l.board.h + 8.);
                assert!(l.hint_y <= h - 6., "hint fits at {w}x{h}");
                let hud = l.header;
                for (name, r) in [
                    ("score", Rect::new(hud.left, 0., hud.score_room, 58.)),
                    (
                        "best",
                        Rect::new(hud.left, hud.best_y - 13., hud.best_room, 13.),
                    ),
                    ("balls", Rect::new(hud.balls_right - 64., 8., 64., 39.)),
                    (
                        "mult",
                        Rect::new(hud.mult_right - 70., hud.mult_y - 13., 70., 13.),
                    ),
                ] {
                    inside(name, r);
                    assert!(r.y + r.h <= hud.header);
                }
                assert!(hud.left + hud.score_room <= hud.balls_right - 64.);
                assert!(hud.left + hud.best_room <= hud.mult_right - 70.);
                assert!(hud.mult_right <= hud.reset.x || hud.mult_y < hud.reset.y);
            }
            // Cards are centered on the table and never enter the corner.
            let card_w = (w - 24.).clamp(1., 460.);
            let card = Rect::new(w * 0.5 - card_w * 0.5, l.card_top, card_w, 10.);
            clear_of_corner("card", card, w);
        }
        assert!(layout(1280., 800.).wide && layout(844., 390.).wide);
        assert!(!layout(390., 844.).wide && !layout(800., 1000.).wide);
    }

    #[test]
    fn dot_pitch_lands_on_whole_device_pixels() {
        for dpi in [1., 1.25, 1.5, 2., 3.] {
            for large in [false, true] {
                let pitch = dot_pitch(dpi, large) * dpi;
                assert!((pitch - pitch.round()).abs() < 1e-4 && pitch >= 2.);
            }
        }
        assert_eq!(dot_pitch(f32::NAN, false), 2.);
    }
}
