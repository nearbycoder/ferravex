//! Logical-canvas geometry shared by drawing and the model's pointer mapping,
//! so every clickable surface is drawn exactly where it is hit-tested.
use crate::content::{COLS, ROWS};
use ferravex_arcade::{Rect, Vec2, vec2};

/// Map origin and tile size. The legal planner clicks tile centers here.
pub(crate) const MAP_X: f32 = 28.;
pub(crate) const MAP_Y: f32 = 162.;
pub(crate) const TW: f32 = 50.;
pub(crate) const TH: f32 = 46.;
/// Right-hand command column.
pub(crate) const SIDE_X: f32 = 610.;
pub(crate) const SIDE_W: f32 = 326.;
/// Crew cards: card `id` spans `CARD_Y + id * CARD_STEP` for `CARD_H` units;
/// the whole band (gaps included) selects a crew.
pub(crate) const CARD_Y: f32 = 122.;
pub(crate) const CARD_STEP: f32 = 66.;
pub(crate) const CARD_H: f32 = 60.;
pub(crate) const CARDS: Rect = Rect {
    x: SIDE_X,
    y: CARD_Y,
    w: SIDE_W,
    h: 198.,
};
/// Credits and air sorties.
pub(crate) const WALLET_Y: f32 = 337.;
/// Context action (Space).
pub(crate) const ACT: Rect = Rect {
    x: SIDE_X,
    y: 358.,
    w: SIDE_W,
    h: 50.,
};
/// Firebreak (X) and air drop (4), side by side.
pub(crate) const CUT: Rect = Rect {
    x: SIDE_X,
    y: 416.,
    w: 159.,
    h: 48.,
};
pub(crate) const DROP: Rect = Rect {
    x: SIDE_X + 167.,
    y: 416.,
    w: 159.,
    h: 48.,
};
/// End turn (C).
pub(crate) const END: Rect = Rect {
    x: SIDE_X,
    y: 472.,
    w: SIDE_W,
    h: 56.,
};
/// Forecast toggle (5) in the strip above the map.
pub(crate) const FORECAST: Rect = Rect {
    x: 196.,
    y: 120.,
    w: 62.,
    h: 28.,
};
/// Debrief call to action, inside its card.
pub(crate) const PROCEED: Rect = Rect {
    x: 50.,
    y: 470.,
    w: 515.,
    h: 50.,
};
/// Setback call to action, at the right of a compact bar that leaves the
/// failure visible on the map above it.
pub(crate) const RETRY: Rect = Rect {
    x: 372.,
    y: 472.,
    w: 186.,
    h: 44.,
};
/// Briefing call to action, under the map so the whole district shows.
pub(crate) const DEPLOY: Rect = Rect {
    x: 173.,
    y: 538.,
    w: 260.,
    h: 30.,
};
/// Debrief upgrade cards: card `id` starts `id * SHOP_STEP` right of `SHOP`.
pub(crate) const SHOP: Rect = Rect {
    x: 54.,
    y: 328.,
    w: 155.,
    h: 113.,
};
pub(crate) const SHOP_STEP: f32 = 166.;

/// Half-open hit test: the left and top edges hit, the right and bottom miss.
pub(crate) fn hit(r: Rect, p: Vec2) -> bool {
    p.x >= r.x && p.x < r.x + r.w && p.y >= r.y && p.y < r.y + r.h
}
/// The call-to-action band, which also accepts its right and bottom edges.
pub(crate) fn proceed_at(p: Vec2) -> bool {
    let r = PROCEED;
    p.x >= r.x && p.x <= r.x + r.w && p.y >= r.y && p.y <= r.y + r.h
}
/// The map tile under `p`, if any.
pub(crate) fn tile_at(p: Vec2) -> Option<usize> {
    let map = Rect::new(MAP_X, MAP_Y, TW * COLS as f32, TH * ROWS as f32);
    hit(map, p).then(|| ((p.y - MAP_Y) / TH) as usize * COLS + ((p.x - MAP_X) / TW) as usize)
}
/// Top-left corner of tile `at`.
pub(crate) fn origin(at: usize) -> (f32, f32) {
    (
        MAP_X + (at % COLS) as f32 * TW,
        MAP_Y + (at / COLS) as f32 * TH,
    )
}
/// Center of tile `at`.
pub(crate) fn center(at: usize) -> Vec2 {
    let (x, y) = origin(at);
    vec2(x + TW * 0.5, y + TH * 0.5)
}
/// The crew card under `p`, if any.
pub(crate) fn card_at(p: Vec2) -> Option<usize> {
    hit(CARDS, p).then(|| (((p.y - CARD_Y) / CARD_STEP) as usize).min(2))
}
/// The upgrade card under `p`, if any.
pub(crate) fn shop_at(p: Vec2) -> Option<usize> {
    (0..3).find(|&id| hit(shop_card(id), p))
}
/// Upgrade card `id`.
pub(crate) fn shop_card(id: usize) -> Rect {
    Rect::new(SHOP.x + id as f32 * SHOP_STEP, SHOP.y, SHOP.w, SHOP.h)
}
