//! Bespoke heist sprites shared by the floor plan and the interface, so the
//! header, sidebar and cards show exactly what the map shows. Every sprite is
//! centered on its anchor and scales from its authored map size.
use ferravex_arcade::shapes::{self, Cap, Fill, Join, Stroke};
use ferravex_arcade::typography::{TextAlign, TextStyle, draw_text_styled};
use ferravex_arcade::*;

pub(crate) const GOLD: Color = Color::new(0.88, 0.71, 0.39, 1.);
pub(crate) const VELVET: Color = Color::new(0.61, 0.33, 0.67, 1.);
pub(crate) const ORCHID: Color = Color::new(0.80, 0.60, 0.88, 1.);
pub(crate) const MINT: Color = Color::new(0.42, 0.86, 0.75, 1.);
pub(crate) const RED: Color = Color::new(0.99, 0.34, 0.35, 1.);
pub(crate) const AMBER: Color = Color::new(1.0, 0.68, 0.3, 1.);
pub(crate) const ROSE: Color = Color::new(0.96, 0.42, 0.55, 1.);
pub(crate) const CARD: Color = Color::new(0.085, 0.06, 0.11, 1.);
/// Ink for empty slots and spent resources.
pub(crate) const GHOST: Color = Color::new(1., 1., 1., 0.16);

pub(crate) fn rgba(r: f32, g: f32, b: f32, a: f32) -> Color {
    Color::new(r, g, b, a)
}
/// A round-capped, round-joined stroke.
pub(crate) fn pen(width: f32, color: Color) -> Stroke {
    Stroke::new(width, color).cap(Cap::Round).join(Join::Round)
}
/// `color` with its alpha multiplied by `a`.
pub(crate) fn fade(color: Color, a: f32) -> Color {
    with_alpha(color, color.a * a)
}
/// Accent of an interactive object tile.
pub(crate) fn kind_color(kind: u8) -> Color {
    match kind {
        b'L' => PAPER,
        b'T' | b'E' => MINT,
        _ => GOLD,
    }
}
/// A required record: a ledger page with a gold header and a red seal.
pub(crate) fn record(cx: f32, cy: f32, s: f32, a: f32) {
    let (w, h) = (15. * s, 19. * s);
    let (x, y) = (cx - w * 0.5, cy - h * 0.5);
    draw_rectangle(x + 1.5, y + 1.5, w, h, rgba(0., 0., 0., 0.4 * a));
    draw_rectangle(x, y, w, h, fade(PAPER, a));
    draw_rectangle(x + 2. * s, y + 3. * s, w - 4. * s, 2.5 * s, fade(GOLD, a));
    for i in 0..3 {
        draw_rectangle(
            x + 2. * s,
            y + (8. + i as f32 * 3.) * s,
            w - 5. * s,
            s.max(1.),
            fade(MUTED, a),
        );
    }
    shapes::fill_circle(vec2(x + w - 2. * s, y + h - 2. * s), 3.5 * s, fade(RED, a));
}
/// The empty place of an uncollected record or artwork.
pub(crate) fn slot(cx: f32, cy: f32, w: f32, h: f32, color: Color) {
    rounded(
        cx - w * 0.5,
        cy - h * 0.5,
        w,
        h,
        2.5,
        rgba(0., 0., 0., 0.28),
    );
    outline(cx - w * 0.5, cy - h * 0.5, w, h, 2.5, 1.5, color);
}
/// An optional artwork: a gilt frame around a moonlit landscape.
pub(crate) fn painting(cx: f32, cy: f32, w: f32, h: f32, a: f32) {
    let (x, y) = (cx - w * 0.5, cy - h * 0.5);
    draw_rectangle(x + 1.5, y + 1.5, w, h, rgba(0., 0., 0., 0.35 * a));
    draw_rectangle(x, y, w, h, fade(GOLD, a));
    draw_rectangle(x + 0.5, y + 0.5, w - 1., 1., rgba(1., 0.94, 0.75, a));
    let inset = (w * 0.104).max(1.5);
    let (ix, iy, iw, ih) = (x + inset, y + inset, w - inset * 2., h - inset * 2.);
    draw_rectangle(ix, iy, iw, ih, rgba(0.16, 0.22, 0.42, a));
    shapes::fill_triangle(
        vec2(ix, iy + ih),
        vec2(ix + iw * 0.4, iy + ih * 0.35),
        vec2(ix + iw * 0.8, iy + ih),
        rgba(0.30, 0.52, 0.42, a),
    );
    shapes::fill_triangle(
        vec2(ix + iw * 0.45, iy + ih),
        vec2(ix + iw * 0.75, iy + ih * 0.55),
        vec2(ix + iw, iy + ih),
        rgba(0.22, 0.40, 0.34, a),
    );
    shapes::fill_circle(
        vec2(ix + iw * 0.78, iy + ih * 0.3),
        ih * 0.14,
        rgba(1., 0.85, 0.5, a),
    );
}
/// A distraction coin of radius `r`.
pub(crate) fn coin(cx: f32, cy: f32, r: f32, a: f32) {
    shapes::fill_circle(vec2(cx, cy), r, fade(darken(GOLD, 0.25), a));
    shapes::fill_circle(vec2(cx - r * 0.09, cy - r * 0.09), r * 0.82, fade(GOLD, a));
    draw_rectangle(
        cx - r * 0.27,
        cy - r * 0.55,
        r * 0.27,
        r * 0.73,
        rgba(1., 0.95, 0.75, 0.9 * a),
    );
}
/// The brass pass: a key with a round bow, `hole` showing through it.
pub(crate) fn key(cx: f32, cy: f32, s: f32, color: Color, hole: Color) {
    let bow = vec2(cx - 4.5 * s, cy);
    shapes::fill_circle(bow, 5. * s, color);
    shapes::fill_circle(bow, 2.2 * s, hole);
    shapes::fill_capsule(vec2(cx - 0.5 * s, cy), vec2(cx + 8.5 * s, cy), s, color);
    shapes::fill_capsule(
        vec2(cx + 6.5 * s, cy),
        vec2(cx + 6.5 * s, cy + 4. * s),
        s,
        color,
    );
    shapes::fill_capsule(
        vec2(cx + 3.5 * s, cy),
        vec2(cx + 3.5 * s, cy + 3. * s),
        s,
        color,
    );
}
/// A padlock: the door needs the brass pass.
pub(crate) fn padlock(cx: f32, cy: f32, s: f32, color: Color) {
    shapes::stroke_arc(
        vec2(cx, cy - 1.5 * s),
        3.3 * s,
        std::f32::consts::PI,
        std::f32::consts::PI,
        Stroke::new(1.7 * s, color),
    );
    draw_rectangle(
        cx - 3.3 * s - 0.85 * s,
        cy - 1.5 * s,
        1.7 * s,
        1.5 * s,
        color,
    );
    draw_rectangle(
        cx + 3.3 * s - 0.85 * s,
        cy - 1.5 * s,
        1.7 * s,
        1.5 * s,
        color,
    );
    rounded(cx - 4.6 * s, cy - 0.5 * s, 9.2 * s, 7. * s, 1.5 * s, color);
    shapes::fill_circle(
        vec2(cx, cy + 2.6 * s),
        1.1 * s,
        rgba(0.1, 0.06, 0.08, color.a),
    );
}
/// A security door. `locked` adds the padlock shown until the pass is found.
pub(crate) fn door(c: Vec2, s: f32, locked: bool, shimmer: f32) {
    let r = Rect::new(c.x - 13. * s, c.y - 17. * s, 26. * s, 34. * s);
    shapes::fill_rounded_rect(
        r,
        3. * s,
        Fill::vertical(r, rgba(0.42, 0.30, 0.22, 1.), rgba(0.27, 0.19, 0.15, 1.)),
    );
    shapes::stroke_rounded_rect(r, 3. * s, 2. * s, lighten(GOLD, 0.35 * shimmer));
    for i in 0..2 {
        for top in [5., 19.] {
            draw_rectangle(
                r.x + (5. + i as f32 * 9.) * s,
                r.y + top * s,
                6. * s,
                11. * s,
                rgba(0.2, 0.14, 0.11, 0.8),
            );
        }
    }
    shapes::fill_circle(vec2(c.x + 6. * s, c.y + 3. * s), 2.5 * s, GOLD);
    if locked {
        shapes::fill_circle(
            vec2(c.x, c.y + 1. * s),
            8. * s,
            rgba(0.08, 0.05, 0.07, 0.72),
        );
        padlock(c.x, c.y, 1.05 * s, lighten(GOLD, 0.1));
    }
}
/// The extraction stair: a mint door with chevrons that run when it is open.
pub(crate) fn exit_sign(c: Vec2, w: f32, h: f32, ready: bool, t: f32) {
    let r = Rect::new(c.x - w * 0.5, c.y - h * 0.5, w, h);
    let a = if ready { 1. } else { 0.55 };
    shapes::fill_rounded_rect(
        r,
        w * 0.17,
        Fill::vertical(
            r,
            rgba(0.1, 0.35, 0.3, 0.92 * a),
            rgba(0.04, 0.16, 0.15, 0.92),
        ),
    );
    shapes::stroke_rounded_rect(r, w * 0.17, (w * 0.05).max(1.), fade(MINT, a));
    let shift = if ready {
        (t * 3.).fract() * w * 0.12
    } else {
        0.
    };
    let u = w / 30.;
    for i in 0..2 {
        let ax = c.x - 5. * u + i as f32 * 7. * u + shift;
        shapes::polyline(
            &[
                vec2(ax - 3. * u, c.y - 6. * u),
                vec2(ax + 3. * u, c.y),
                vec2(ax - 3. * u, c.y + 6. * u),
            ],
            pen(2. * u, fade(MINT, a)),
        );
    }
}
/// The beam console: blinking lines while live, a tick once rewired.
pub(crate) fn console(c: Vec2, s: f32, live: bool, blink: bool) {
    let r = Rect::new(c.x - 13. * s, c.y - 14. * s, 26. * s, 28. * s);
    shapes::fill_rounded_rect(r, 4. * s, rgba(0.05, 0.16, 0.16, 1.));
    shapes::stroke_rounded_rect(r, 4. * s, 1.5 * s, if live { MINT } else { MUTED });
    if live {
        for (i, w) in [12., 7., 10.].into_iter().enumerate() {
            draw_rectangle(
                r.x + 4. * s,
                r.y + (5. + i as f32 * 5.) * s,
                w * s,
                2. * s,
                MINT,
            );
        }
        if blink {
            draw_rectangle(r.x + 16. * s, r.y + 15. * s, 4. * s, 3. * s, MINT);
        }
    } else {
        check(c, 6. * s, MINT);
    }
}
/// A security beam between two emitters, `w` wide; `flicker` scales its glow.
pub(crate) fn beam(c: Vec2, w: f32, live: bool, flicker: f32) {
    let tint = if live { RED } else { rgba(0.25, 0.4, 0.35, 1.) };
    for ex in [c.x - w * 0.5, c.x + w * 0.5 - 5.] {
        draw_rectangle(ex, c.y - 5., 5., 10., rgba(0.12, 0.1, 0.14, 1.));
        draw_rectangle(ex + 1., c.y - 1., 3., 2., tint);
    }
    let (a, b) = (c.x - w * 0.5 + 5., c.x + w * 0.5 - 5.);
    if live {
        draw_line(a, c.y, b, c.y, 5., rgba(1., 0.3, 0.3, 0.22 * flicker));
        draw_line(a, c.y, b, c.y, 2., fade(RED, flicker));
        draw_line(a, c.y, b, c.y, 0.8, rgba(1., 0.85, 0.85, flicker));
    } else {
        draw_line(a, c.y, b, c.y, 1., fade(tint, 0.6 * flicker));
    }
}
/// A concealment cabinet. With `peek`, a hidden thief's eyes show in the seam.
pub(crate) fn cabinet(c: Vec2, s: f32, peek: Option<f32>) {
    let r = Rect::new(c.x - 12. * s, c.y - 16. * s, 24. * s, 32. * s);
    draw_rectangle(
        r.x + 2. * s,
        r.y + r.h - 0.5 * s,
        r.w - 4. * s,
        2. * s,
        rgba(0., 0., 0., 0.4),
    );
    shapes::fill_rounded_rect(
        r,
        3. * s,
        Fill::vertical(r, rgba(0.30, 0.18, 0.36, 1.), rgba(0.18, 0.11, 0.24, 1.)),
    );
    shapes::stroke_rounded_rect(r, 3. * s, s, ORCHID);
    draw_rectangle(
        c.x - 0.5 * s,
        r.y + 3. * s,
        s,
        r.h - 6. * s,
        rgba(0.08, 0.05, 0.1, 1.),
    );
    draw_rectangle(c.x - 3. * s, c.y + 3. * s, 1.5 * s, 3. * s, GOLD);
    draw_rectangle(c.x + 1.5 * s, c.y + 3. * s, 1.5 * s, 3. * s, GOLD);
    if let Some(open) = peek {
        let gap = Rect::new(c.x - 4. * s, c.y - 7. * s, 8. * s, 5. * s);
        shapes::fill_rounded_rect(gap, 2. * s, rgba(0.02, 0.01, 0.03, 1.));
        let h = 1.6 * s * open.clamp(0.15, 1.);
        draw_rectangle(c.x - 3. * s, c.y - 4.5 * s - h * 0.5, 1.8 * s, h, PAPER);
        draw_rectangle(c.x + 1.2 * s, c.y - 4.5 * s - h * 0.5, 1.8 * s, h, PAPER);
    }
}
/// A guard's head and peaked cap, eyes along `look`.
pub(crate) fn guard_head(c: Vec2, s: f32, look: (f32, f32), a: f32) {
    shapes::fill_circle(c + vec2(0., 2. * s), 7. * s, rgba(0.84, 0.67, 0.58, a));
    draw_rectangle(
        c.x - 9. * s,
        c.y - 4. * s,
        18. * s,
        4. * s,
        rgba(0.32, 0.17, 0.22, a),
    );
    draw_rectangle(
        c.x - 6. * s,
        c.y - 8. * s,
        12. * s,
        5. * s,
        rgba(0.40, 0.21, 0.27, a),
    );
    draw_rectangle(
        c.x - 6. * s,
        c.y - 8. * s,
        12. * s,
        s,
        rgba(0.6, 0.35, 0.4, a),
    );
    draw_rectangle(c.x - 1.5 * s, c.y - 6. * s, 3. * s, 2. * s, fade(GOLD, a));
    let ex = c.x + look.0 * 2. * s;
    let ey = c.y + 2. * s + look.1 * s;
    draw_rectangle(ex - 3. * s, ey, 2. * s, 2. * s, fade(INK, a));
    draw_rectangle(ex + s, ey, 2. * s, 2. * s, fade(INK, a));
}
/// The violet thief, feet at `c.y + 12 s`, lowered while crouching.
pub(crate) fn thief(c: Vec2, s: f32, lowered: f32, look: f32, a: f32) {
    let top = c.y + (-13. + lowered) * s;
    let base = c.y + 12. * s;
    shapes::fill_triangle(
        vec2(c.x, top),
        vec2(c.x - 11. * s, base),
        vec2(c.x + 11. * s, base),
        fade(VELVET, a),
    );
    shapes::fill_triangle(
        vec2(c.x, top),
        vec2(c.x + 2. * s, base),
        vec2(c.x + 11. * s, base),
        fade(darken(VELVET, 0.25), a),
    );
    let hy = c.y + (-6. + lowered) * s;
    shapes::fill_circle(vec2(c.x, hy), 8. * s, fade(ORCHID, a));
    shapes::fill_circle(
        vec2(c.x - 2.5 * s, hy - 2.5 * s),
        3. * s,
        rgba(1., 0.9, 1., 0.35 * a),
    );
    let mask = Rect::new(c.x - 6. * s, hy - 2. * s, 12. * s, 5. * s);
    shapes::fill_rounded_rect(mask, 2.5 * s, fade(INK, a));
    let lx = look * 1.5 * s;
    draw_rectangle(c.x - 3.5 * s + lx, hy - s, 2. * s, 2. * s, fade(PAPER, a));
    draw_rectangle(c.x + 1.5 * s + lx, hy - s, 2. * s, 2. * s, fade(PAPER, a));
}
/// Quarter arcs of sound to the right of `c`: footsteps that guards hear.
pub(crate) fn sound_arcs(c: Vec2, s: f32, color: Color) {
    for (i, r) in [3.5, 6.5, 9.5].into_iter().enumerate() {
        shapes::stroke_arc(
            c,
            r * s,
            -0.7,
            1.4,
            pen(1.4 * s, fade(color, 1. - i as f32 * 0.25)),
        );
    }
}
/// How a forecast medal stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Medal {
    /// Earned or still on track.
    Lit,
    /// Lost to the alarm.
    Lost,
    /// Waiting for both artworks.
    Pending,
}
/// A medal disc of radius `r`.
pub(crate) fn medal(c: Vec2, r: f32, state: Medal, a: f32) {
    match state {
        Medal::Lit => {
            shapes::fill_circle(
                c,
                r,
                Fill::radial(
                    c - vec2(r * 0.3, r * 0.3),
                    r * 1.2,
                    rgba(1., 0.9, 0.6, a),
                    fade(darken(GOLD, 0.25), a),
                ),
            );
            shapes::stroke_circle(c, r, (r * 0.08).max(1.), rgba(1., 0.95, 0.8, 0.5 * a));
            icon(
                Icon::Star,
                c.x,
                c.y,
                r * 1.15,
                rgba(0.5, 0.33, 0.1, 0.9 * a),
            );
        }
        Medal::Lost => {
            shapes::fill_circle(c, r, rgba(1., 1., 1., 0.05 * a));
            shapes::stroke_circle(c, r, 1., rgba(1., 1., 1., 0.14 * a));
            let d = r * 0.5;
            draw_line(
                c.x - d,
                c.y + d,
                c.x + d,
                c.y - d,
                (r * 0.16).max(1.5),
                fade(RED, 0.7 * a),
            );
        }
        Medal::Pending => {
            shapes::fill_circle(c, r, rgba(0.88, 0.71, 0.39, 0.08 * a));
            shapes::stroke_circle(c, r, (r * 0.1).max(1.), fade(GOLD, 0.45 * a));
            painting(c.x, c.y, r * 1.1, r * 0.86, 0.55 * a);
        }
    }
}
/// A speech bubble with a single ASCII glyph ("!" or "?").
pub(crate) fn bubble(c: Vec2, glyph: &str, fill: Color, a: f32) {
    shapes::fill_triangle(
        vec2(c.x - 4., c.y + 5.),
        vec2(c.x + 2., c.y + 6.),
        vec2(c.x - 6., c.y + 13.),
        fade(fill, a),
    );
    shapes::fill_circle(c, 9., fade(fill, a));
    shapes::stroke_circle(c, 9., 1., rgba(1., 1., 1., 0.35 * a));
    let s = TextStyle::modern(15., rgba(0.08, 0.04, 0.06, a)).with_align(TextAlign::Center);
    draw_text_styled(glyph, c.x, s.centered_baseline(c.y), s);
}
/// Aiming reticle of radius `r`.
pub(crate) fn reticle(c: Vec2, r: f32, color: Color) {
    shapes::stroke_circle(c, r, 1.5, color);
    for (dx, dy) in [(1., 0.), (-1., 0.), (0., 1.), (0., -1.)] {
        let d = vec2(dx, dy);
        shapes::fill_capsule(c + d * (r * 0.65), c + d * (r * 1.45), 0.8, color);
    }
}
/// A tick mark of half-width `r`.
pub(crate) fn check(c: Vec2, r: f32, color: Color) {
    shapes::polyline(
        &[
            c + vec2(-r, 0.),
            c + vec2(-r * 0.3, r * 0.65),
            c + vec2(r, -r * 0.7),
        ],
        pen((r * 0.34).max(1.5), color),
    );
}
/// A small round badge with a tick: collected, done.
pub(crate) fn done_badge(c: Vec2, r: f32) {
    shapes::fill_circle(c, r, MINT);
    shapes::stroke_circle(c, r, 1., rgba(0.02, 0.1, 0.08, 0.6));
    check(c, r * 0.55, rgba(0.03, 0.14, 0.11, 1.));
}
/// A cross of half-width `r`: refused, invalid.
pub(crate) fn cross(c: Vec2, r: f32, color: Color) {
    let w = (r * 0.4).max(1.5);
    shapes::fill_capsule(c - vec2(r, r), c + vec2(r, r), w * 0.5, color);
    shapes::fill_capsule(c + vec2(-r, r), c + vec2(r, -r), w * 0.5, color);
}
/// A right-pointing chevron of half-height `r`.
pub(crate) fn chevron(c: Vec2, r: f32, color: Color) {
    shapes::polyline(
        &[
            c + vec2(-r * 0.5, -r),
            c + vec2(r * 0.5, 0.),
            c + vec2(-r * 0.5, r),
        ],
        pen((r * 0.35).max(1.5), color),
    );
}
/// A Manhattan-distance outline (a diamond) of radius `r` around `c`.
pub(crate) fn diamond(c: Vec2, r: f32, width: f32, color: Color) {
    shapes::polyline(
        &[
            c + vec2(0., -r),
            c + vec2(r, 0.),
            c + vec2(0., r),
            c + vec2(-r, 0.),
        ],
        Stroke::new(width, color).closed(true),
    );
}
