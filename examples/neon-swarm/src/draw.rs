//! World-space drawing: the arena, entities and world-anchored juice. Reads
//! game state and presentation only; nothing here can change the simulation.
//!
//! Art direction: a dark, slightly blue containment floor lit by its own
//! emissive things. Everything that matters (the player, drones, bullets,
//! supplies, lamps and the chain-colored border) is drawn with additive neon
//! strokes and glows so the post-processing bloom picks it up, while the
//! floor and cover stay below the bloom threshold and read as matte.
use crate::game::{
    ARENA, BEACONS, Bullet, COVER, ELITE_HEALTH, Enemy, Game, PLAYER_RADIUS, Phase, Pickup,
    Position, Supply,
};
use crate::hud::Line;
use crate::juice::{GOLD, Juice, VIOLET, fade, tier_color};
use crate::presentation::Presentation;
use crate::{AMBER, CORAL, CYAN, FLOOR, INK, MUTED, PAPER};
use ferravex_play::action::ActionPhase;
use ferravex_render::canvas::Canvas2D;
use ferravex_render::post::{glow_circle, with_glow};
use ferravex_render::prelude::*;
use ferravex_render::shapes::{self, Cap, Fill, Join, Stroke};
use ferravex_render::text::{TextAlign, TextStyle, draw_text_styled};

pub(crate) const REPAIR: Color = Color::from_rgba(130, 246, 160, 255);
pub(crate) const BATTERY: Color = Color::from_rgba(140, 200, 255, 255);
const BRUTE: Color = Color::from_rgba(236, 104, 214, 255);
const CENTER: Vec2 = vec2(640., 414.);

/// The world camera with presentation-only shake, aim lead and the
/// final-drone close-up. Pointer mapping excludes shake so aim never jitters.
pub(crate) struct WorldView {
    pub camera: Camera2D,
    target: Vec2,
    zoom: f32,
    canvas: Canvas2D,
}

impl WorldView {
    pub fn new(canvas: Canvas2D, juice: &Juice, game: &Game) -> Self {
        let mut camera = canvas.camera();
        let zoom = juice.zoom();
        let mut target = camera.target + juice.lead();
        if zoom > 1. {
            // Lean toward the duel: halfway between player and the last drone.
            let last = game.nearest_enemy().unwrap_or(game.player);
            let focus = (game.player + last) * 0.5;
            let lean = (zoom - 1.) / 0.08 * 0.35;
            target += (focus - CENTER) * lean;
        }
        camera.target = target + juice.shake();
        camera.zoom *= zoom;
        Self {
            camera,
            target,
            zoom,
            canvas,
        }
    }

    /// Logical screen pixels to world, matching the drawn (zoomed) arena.
    pub fn unproject(&self, screen: Vec2) -> Vec2 {
        let base = self.canvas.unproject(screen);
        self.target + (base - self.canvas.size * 0.5) / self.zoom
    }

    /// World to logical screen pixels (without shake), e.g. the iris focus.
    pub fn project(&self, world: Vec2) -> Vec2 {
        self.canvas
            .project(self.canvas.size * 0.5 + (world - self.target) * self.zoom)
    }
}

fn hash(n: u32) -> f32 {
    let mut x = n.wrapping_mul(0x9e37_79b9) ^ 0x85eb_ca6b;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2c1b_3c6d);
    x ^= x >> 12;
    (x & 0xffff) as f32 / 65535.
}

fn hue(t: f32) -> Color {
    let t = t.rem_euclid(1.) * 6.;
    let x = 1. - (t % 2. - 1.).abs();
    let (r, g, b) = match t as u32 {
        0 => (1., x, 0.),
        1 => (x, 1., 0.),
        2 => (0., 1., x),
        3 => (0., x, 1.),
        4 => (x, 0., 1.),
        _ => (1., 0., x),
    };
    Color::new(r, g, b, 1.)
}

/// World text in the Modern face, centered on `x`.
pub(crate) fn centered(text: &str, x: f32, y: f32, size: f32, color: Color) {
    draw_text_styled(
        text,
        x,
        y,
        TextStyle::modern(size, color).with_align(TextAlign::Center),
    );
}

fn neon_poly(center: Vec2, sides: u8, radius: f32, angle: f32, width: f32, color: Color) {
    let mut points = [Vec2::ZERO; 8];
    let n = sides.min(8) as usize;
    for (i, point) in points.iter_mut().take(n).enumerate() {
        let a = angle.to_radians() + i as f32 / n as f32 * std::f32::consts::TAU;
        *point = center + Vec2::from_angle(a) * radius;
    }
    shapes::polyline(
        &points[..n],
        Stroke::new(width, color).join(Join::Miter).closed(true),
    );
}

pub(crate) fn backdrop(p: &Presentation) {
    // Deep, cool backdrop beyond the arena so camera lead/zoom never shows a
    // hard edge, with a faint radial lift behind the play field.
    let outer = Rect::new(-700., -500., 2680., 1800.);
    shapes::fill_rounded_rect(
        outer,
        0.,
        Fill::radial(
            CENTER,
            1100.,
            Color::from_rgba(16, 30, 44, 255),
            Color::from_rgba(5, 9, 14, 255),
        ),
    );
    // Distant drifting hex dust (parallax: moves slower than the arena).
    let time = p.juice.time * p.motion_scale();
    let lead = p.juice.lead() * 0.6;
    for n in 0..24 {
        let x = -120. + hash(n + 300) * 1520. + (time * (3. + hash(n) * 4.)).sin() * 10.;
        let y = -80. + (hash(n + 400) * 960. - time * (4. + hash(n + 9) * 6.)).rem_euclid(960.);
        let at = vec2(x, y) + lead;
        if ARENA.contains(at) {
            continue;
        }
        draw_poly(at.x, at.y, 6, 3. + hash(n + 5) * 5., 30., fade(CYAN, 0.06));
    }
}

pub(crate) fn arena(game: &Game, p: &Presentation) {
    let juice = &p.juice;
    let motion = juice.motion();
    let time = juice.time * motion;
    let tier = tier_color(if game.score.chain() > 0 {
        game.score.multiplier()
    } else {
        0
    });
    let frame = Rect::new(ARENA.x - 12., ARENA.y - 12., ARENA.w + 24., ARENA.h + 24.);
    shapes::fill_rounded_rect(frame, 10., Color::from_rgba(14, 25, 31, 255));
    // A soft pool of light in the middle of the floor keeps it from looking flat.
    shapes::fill_rounded_rect(
        ARENA,
        4.,
        Fill::radial(
            CENTER,
            720.,
            Color::from_rgba(24, 42, 50, 255),
            Color::from_rgba(13, 23, 29, 255),
        ),
    );
    // Checker tiles add texture; disco mode tints them like a dance floor.
    let disco = game.style.disco();
    for ty in 0..10 {
        for tx in 0..18 {
            let x = ARENA.x + 8. + tx as f32 * 62.;
            let y = ARENA.y + 4. + ty as f32 * 60.;
            if disco {
                let shade = hue(tx as f32 * 0.07 + ty as f32 * 0.11 + time * 0.25);
                let beat = if motion > 0. && p.flash_scale() > 0. {
                    0.06 + 0.05 * ((time * 4. + (tx + ty) as f32).sin() * 0.5 + 0.5)
                } else {
                    0.08
                };
                draw_rectangle(x + 2., y + 2., 58., 56., fade(shade, beat));
            } else if (tx + ty) % 2 == 0 {
                draw_rectangle(x + 2., y + 2., 58., 56., Color::new(1., 1., 1., 0.018));
            }
        }
    }
    // Thin floor seams with a slow scanner sweep.
    for x in (134..1208).step_by(124) {
        draw_line(
            x as f32,
            ARENA.y,
            x as f32,
            ARENA.bottom(),
            1.,
            Color::new(0.38, 0.93, 0.81, 0.035),
        );
    }
    if motion > 0. {
        let sweep = ARENA.y + (time * 70.).rem_euclid(ARENA.h + 120.) - 60.;
        for (offset, alpha) in [(0., 0.07), (8., 0.04), (16., 0.02)] {
            let y = sweep - offset;
            if y > ARENA.y && y < ARENA.bottom() - 3. {
                draw_rectangle(ARENA.x, y, ARENA.w, 3., fade(CYAN, alpha));
            }
        }
    }
    // Ambient motes drift through the containment field.
    for n in 0..30 {
        let x = ARENA.x + hash(n) * ARENA.w + (time * (6. + hash(n + 99) * 10.)).sin() * 14.;
        let y = ARENA.y
            + (hash(n + 7) * ARENA.h - time * (8. + hash(n + 31) * 14.)).rem_euclid(ARENA.h);
        draw_circle(x, y, 1.2 + hash(n + 3) * 1.4, fade(CYAN, 0.2));
    }
    // Center seal: readable sector number under quiet rotating ticks.
    shapes::stroke_circle(CENTER, 100., 2., Color::from_rgba(40, 70, 76, 255));
    shapes::stroke_circle(CENTER, 88., 1., Color::from_rgba(34, 58, 64, 255));
    with_glow(|| {
        shapes::stroke_arc(
            CENTER,
            94.,
            time * 0.4,
            std::f32::consts::TAU * 0.92,
            Stroke::new(1.5, fade(tier, 0.18)).cap(Cap::Round),
        );
    });
    for i in 0..12 {
        let a = i as f32 / 12. * std::f32::consts::TAU + time * 0.25;
        let d = Vec2::from_angle(a);
        let (a0, a1) = (CENTER + d * 92., CENTER + d * 100.);
        draw_line(a0.x, a0.y, a1.x, a1.y, 3., fade(tier, 0.45));
    }
    draw_text_styled(
        "07",
        CENTER.x,
        CENTER.y + 24.,
        TextStyle::classic(64., Color::from_rgba(44, 80, 86, 255)).with_align(TextAlign::Center),
    );
    centered(
        "Sector",
        CENTER.x,
        CENTER.y - 38.,
        16.,
        Color::from_rgba(60, 98, 104, 255),
    );
    for cover in COVER {
        let shadow = Rect::new(cover.x + 8., cover.y + 12., cover.w, cover.h);
        shapes::fill_rounded_rect(shadow, 6., Color::new(0., 0., 0., 0.45));
        shapes::fill_rounded_rect(
            cover,
            6.,
            Fill::vertical(
                cover,
                Color::from_rgba(46, 66, 72, 255),
                Color::from_rgba(30, 44, 50, 255),
            ),
        );
        shapes::stroke_rounded_rect(cover, 6., 1.5, Color::from_rgba(82, 110, 114, 255));
        with_glow(|| {
            shapes::fill_capsule(
                vec2(cover.x + 10., cover.y + 4.),
                vec2(cover.right() - 10., cover.y + 4.),
                1.6,
                fade(CYAN, 0.55),
            );
        });
        for x in 0..6 {
            let x0 = cover.x + 12. + x as f32 * 13.;
            draw_line(
                x0,
                cover.bottom() - 8.,
                x0 + 7.,
                cover.bottom() - 16.,
                4.,
                Color::from_rgba(176, 136, 62, 255),
            );
        }
    }
    // Chain-tier neon border: the arena itself shows how hot the chain is.
    let glow = if game.score.chain() > 0 { 1. } else { 0.5 };
    with_glow(|| {
        shapes::stroke_rounded_rect(
            Rect::new(ARENA.x - 2., ARENA.y - 2., ARENA.w + 4., ARENA.h + 4.),
            6.,
            3.,
            fade(tier, 0.75 * glow),
        );
    });
    for (x, y, sx, sy) in [
        (ARENA.x, ARENA.y, 1., 1.),
        (ARENA.right(), ARENA.y, -1., 1.),
        (ARENA.x, ARENA.bottom(), 1., -1.),
        (ARENA.right(), ARENA.bottom(), -1., -1.),
    ] {
        let corner = [vec2(x + sx * 36., y), vec2(x, y), vec2(x, y + sy * 36.)];
        with_glow(|| {
            shapes::polyline(
                &corner,
                Stroke::new(4., CYAN).join(Join::Miter).cap(Cap::Square),
            )
        });
    }
    beacons(game, p);
}

fn beacons(game: &Game, p: &Presentation) {
    let time = p.juice.time;
    for (i, at) in BEACONS.into_iter().enumerate() {
        let lit = game.style.beacons & (1 << i) != 0;
        let vertical = at.x == ARENA.x || at.x == ARENA.right();
        let (w, h) = if vertical { (12., 28.) } else { (28., 12.) };
        shapes::fill_rounded_rect(Rect::new(at.x - w * 0.5, at.y - h * 0.5, w, h), 4., INK);
        let color = if game.style.disco() {
            hue(i as f32 * 0.25 + time * 0.4 * p.motion_scale())
        } else if lit {
            CYAN
        } else {
            CORAL
        };
        let alpha = if lit || p.flash_scale() == 0. {
            1.
        } else {
            // Unlit lamps idle with a slow, low-contrast blink.
            0.55 + 0.35 * ((time * 2.2 + i as f32).sin() * 0.5 + 0.5)
        };
        if lit {
            glow_circle(at.x, at.y, 70., color, 0.35);
        } else {
            glow_circle(at.x, at.y, 26., color, 0.22 * alpha);
        }
        shapes::fill_circle(at, 6., fade(color, alpha));
        shapes::stroke_circle(at, 9., 1.5, fade(color, 0.6 * alpha));
    }
}

pub(crate) fn spawn_preview(game: &Game) {
    if game.phase != Phase::Playing {
        return;
    }
    let (admission, _) = game.spawn_preview();
    let Some(wait) = admission.next_available_in().filter(|wait| *wait <= 0.6) else {
        return;
    };
    // Eligible zones, not a promised spawn: the fixed-tick selector still
    // chooses the farthest safe position when admission succeeds.
    let strength = (1. - wait as f32 / 0.6).clamp(0., 1.);
    let color = fade(AMBER, 0.45 + 0.55 * strength);
    for p in game.entry_candidates() {
        shapes::fill_circle(p, 20., fade(AMBER, 0.04 + 0.08 * strength));
        with_glow(|| {
            shapes::stroke_arc(
                p,
                22.,
                -std::f32::consts::FRAC_PI_2,
                std::f32::consts::TAU * strength.max(0.05),
                Stroke::new(2.5, color).cap(Cap::Round),
            );
        });
        shapes::stroke_circle(p, 22., 1., fade(AMBER, 0.3));
        for i in 0..4 {
            let d = Vec2::from_angle(i as f32 * std::f32::consts::FRAC_PI_2 + 0.785);
            let (a, b) = (p + d * 31., p + d * 24.);
            draw_line(a.x, a.y, b.x, b.y, 3., color);
        }
        if game.wave == 1 {
            let x = p.x.clamp(ARENA.x + 30., ARENA.right() - 30.);
            let y = if p.y < CENTER.y { p.y + 44. } else { p.y - 32. };
            centered("Entry", x, y, 12., color);
        }
    }
}

fn stretched_poly(
    center: Vec2,
    sides: u8,
    radius: f32,
    angle: f32,
    scale: Vec2,
    dir: Vec2,
    c: Color,
) {
    let across = vec2(-dir.y, dir.x);
    let point = |i: u8| {
        let a = angle.to_radians() + i as f32 / sides as f32 * std::f32::consts::TAU;
        let local = Vec2::from_angle(a) * radius;
        let along = local.dot(dir) * scale.x;
        let side = local.dot(across) * scale.y;
        center + dir * along + across * side
    };
    for i in 0..sides {
        draw_triangle(center, point(i), point(i + 1), c);
    }
}

pub(crate) fn entities(game: &Game, p: &Presentation, emphasis: f32) {
    let motion = p.motion_scale();
    let time = p.juice.time;
    for (pos, pickup) in &mut game.world.query::<(&Position, &Pickup)>() {
        let at = pos.0;
        let pulse = 1. + (time * 5.).sin() * 0.1 * motion;
        let color = match pickup.kind {
            Supply::Repair => REPAIR,
            Supply::Battery => BATTERY,
        };
        let blink = pickup.remaining < 3. && p.flash_scale() > 0. && (time * 8.).sin() < 0.;
        let color = if blink { MUTED } else { color };
        glow_circle(at.x, at.y, 46. * pulse, color, 0.3);
        // A gentle bob sells "floating supply" without moving its hitbox.
        let at = at - vec2(0., (time * 3.).sin() * 2. * motion);
        match pickup.kind {
            Supply::Repair => {
                draw_poly(at.x, at.y, 4, 15. * pulse, 45., INK);
                with_glow(|| neon_poly(at, 4, 15. * pulse, 45., 2., color));
                draw_rectangle(at.x - 7., at.y - 2.5, 14., 5., color);
                draw_rectangle(at.x - 2.5, at.y - 7., 5., 14., color);
            }
            Supply::Battery => {
                draw_poly(at.x, at.y, 6, 15. * pulse, 30., INK);
                with_glow(|| neon_poly(at, 6, 15. * pulse, 30., 2., color));
                shapes::polyline(
                    &[
                        vec2(at.x + 3., at.y - 8.),
                        vec2(at.x - 3., at.y + 1.),
                        vec2(at.x + 3., at.y - 1.),
                        vec2(at.x - 3., at.y + 8.),
                    ],
                    Stroke::new(2.5, color).join(Join::Round),
                );
            }
        }
        if pickup.remaining < 3. {
            shapes::stroke_arc(
                at,
                21.,
                -std::f32::consts::FRAC_PI_2,
                std::f32::consts::TAU * (pickup.remaining / 3.),
                Stroke::new(2., AMBER).cap(Cap::Round),
            );
        }
        if at.distance(game.player) > 48. {
            let name = match pickup.kind {
                Supply::Repair => "Repair",
                Supply::Battery => "Shield",
            };
            centered(name, at.x, at.y + 36., 12., fade(color, 0.9));
        }
    }
    for (pos, enemy) in &mut game.world.query::<(&Position, &Enemy)>() {
        enemy_body(game, pos.0, &enemy, p, motion, time);
    }
    with_glow(|| {
        for (pos, bullet) in &mut game.world.query::<(&Position, &Bullet)>() {
            let dir = bullet.velocity.normalize_or_zero();
            let tail = pos.0 - dir * 30.;
            shapes::fill_capsule(tail, pos.0, 4.5, fade(CYAN, 0.22));
            shapes::fill_capsule(pos.0 - dir * 18., pos.0, 2.2, CYAN);
        }
    });
    for (pos, _) in &mut game.world.query::<(&Position, &Bullet)>() {
        shapes::fill_circle(pos.0, 3., PAPER);
    }
    if let Some(marker) = game.hit_marker {
        let color = if marker.lethal { CYAN } else { PAPER };
        let reach = (if marker.lethal { 14. } else { 10. }) * (1. + emphasis * 0.35);
        with_glow(|| {
            for (x, y) in [(-1., -1.), (1., -1.), (-1., 1.), (1., 1.)] {
                let d = vec2(x, y);
                shapes::polyline(
                    &[marker.position + d * 4., marker.position + d * reach],
                    Stroke::new(2.5, color).cap(Cap::Round),
                );
            }
        });
    }
    player(game, p);
}

fn enemy_body(game: &Game, at: Vec2, enemy: &Enemy, p: &Presentation, motion: f32, time: f32) {
    let base = if enemy.elite {
        GOLD
    } else if enemy.heavy {
        BRUTE
    } else {
        CORAL
    };
    let lit = if enemy.flash > 0. {
        base.lerp(PAPER, p.flash_scale())
    } else {
        base
    };
    let to_player = (game.player - at).normalize_or_zero();
    let heading = to_player.to_angle().to_degrees();
    // Anticipation: a windup crouches and leans in; the strike pops out.
    let (squash, lean) = match enemy.attack.phase() {
        ActionPhase::Windup => {
            let t = enemy.attack.progress();
            (1. - 0.16 * t, 5. * t)
        }
        ActionPhase::Active => (1.18, 8.),
        _ => (1., 0.),
    };
    let at_body = at + to_player * lean * motion;
    // Hits puff the body up briefly; spawn flash doubles as a pop-in.
    let r = enemy.radius * (1. + enemy.flash * 2.4 * motion) * (1. + (squash - 1.) * motion);
    if enemy.stagger.active() {
        with_glow(|| shapes::stroke_circle(at, r + 9., 2.5, fade(CYAN, 0.8)));
        for i in 0..3 {
            let a = time * 5. * motion + i as f32 * 2.09;
            let s = at + Vec2::from_angle(a) * (r + 9.) - vec2(0., 2.);
            draw_poly(s.x, s.y, 4, 4., a.to_degrees(), CYAN);
        }
    }
    match enemy.attack.phase() {
        ActionPhase::Windup => {
            let progress = enemy.attack.progress();
            // The ring is the actual contact range; progress fills it.
            let reach = enemy.radius + PLAYER_RADIUS;
            shapes::fill_circle(at, reach, fade(AMBER, 0.06 + 0.12 * progress));
            shapes::stroke_circle(at, reach, 1.5, fade(AMBER, 0.6));
            with_glow(|| {
                shapes::stroke_arc(
                    at,
                    reach,
                    -std::f32::consts::FRAC_PI_2,
                    std::f32::consts::TAU * progress.max(0.05),
                    Stroke::new(3. + progress * 2., AMBER).cap(Cap::Round),
                );
            });
            let badge = at - vec2(0., enemy.radius + 24.);
            shapes::fill_circle(badge, 10., AMBER);
            draw_text_styled(
                "!",
                badge.x,
                badge.y + 7.,
                TextStyle::classic(16., INK).with_align(TextAlign::Center),
            );
        }
        ActionPhase::Active => {
            with_glow(|| {
                shapes::stroke_circle(at, enemy.radius + PLAYER_RADIUS, 4., CORAL);
            });
        }
        _ => {}
    }
    glow_circle(
        at_body.x,
        at_body.y,
        r * 2.6,
        base,
        if enemy.elite { 0.5 } else { 0.22 },
    );
    let sides = if enemy.heavy { 6 } else { 4 };
    draw_poly(
        at_body.x + 3.,
        at_body.y + 6.,
        sides,
        r + 2.,
        heading,
        Color::new(0., 0., 0., 0.5),
    );
    draw_poly(
        at_body.x,
        at_body.y,
        sides,
        r,
        heading,
        Color::from_rgba(34, 12, 22, 255),
    );
    draw_poly(
        at_body.x,
        at_body.y,
        sides,
        r * 0.78,
        heading,
        fade(base, 0.35),
    );
    with_glow(|| neon_poly(at_body, sides, r, heading, 3., lit));
    if enemy.elite {
        // Crown spikes and a violet core make the Overseer unmistakable.
        for i in -1..=1 {
            let a = (heading + 180. + i as f32 * 40.).to_radians();
            let d = Vec2::from_angle(a);
            let side = vec2(-d.y, d.x) * 6.;
            let base_point = at_body + d * r * 0.9;
            draw_triangle(
                base_point - side,
                base_point + side,
                at_body + d * (r + 14.),
                lit,
            );
        }
        shapes::fill_circle(at_body, r * 0.42, VIOLET);
        glow_circle(at_body.x, at_body.y, r, VIOLET, 0.6);
        let fraction = (enemy.health / ELITE_HEALTH).clamp(0., 1.);
        let bar = Rect::new(at.x - 32., at.y - r - 28., 64., 6.);
        shapes::fill_rounded_rect(bar, 3., INK);
        shapes::fill_rounded_rect(
            Rect::new(bar.x + 1., bar.y + 1., (bar.w - 2.) * fraction, bar.h - 2.),
            2.,
            GOLD,
        );
    } else if enemy.heavy {
        for pip in 0..(enemy.health / 24.).ceil() as u32 {
            shapes::fill_rounded_rect(
                Rect::new(at.x - 13. + pip as f32 * 9., at.y - r - 13., 6., 4.),
                2.,
                lit,
            );
        }
    }
    // A single eye tracks the player: readable facing, mild menace.
    let eye = at_body + to_player * r * 0.28;
    let eye_r = if enemy.heavy { 5. } else { 3.8 };
    shapes::fill_circle(eye, eye_r, PAPER);
    shapes::fill_circle(eye + to_player * eye_r * 0.4, eye_r * 0.5, INK);
}

fn player(game: &Game, p: &Presentation) {
    let at = game.player;
    let juice = &p.juice;
    let angle = game.facing.to_angle().to_degrees();
    // A pool of cyan light the player carries across the floor.
    glow_circle(at.x, at.y, 110., CYAN, 0.16);
    if p.dash > 0.0 {
        with_glow(|| {
            for index in 1..6 {
                let ghost = at - juice.stretch_dir * index as f32 * 12.;
                neon_poly(
                    ghost,
                    6,
                    15.,
                    angle,
                    2.,
                    fade(CYAN, 0.35 * p.motion_scale() / index as f32),
                );
            }
        });
    }
    shapes::fill_circle(at + vec2(3., 6.), 17., Color::new(0., 0., 0., 0.5));
    let color = if game.health == 0 {
        MUTED
    } else if p.hurt > 0.6 {
        CYAN.lerp(CORAL, p.flash_scale())
    } else {
        CYAN
    };
    let scale = juice.player_scale();
    let dir = juice.stretch_dir.normalize_or_zero();
    let dir = if dir == Vec2::ZERO { Vec2::X } else { dir };
    stretched_poly(
        at,
        6,
        16.,
        angle,
        scale,
        dir,
        Color::from_rgba(12, 40, 44, 255),
    );
    with_glow(|| {
        let mut points = [Vec2::ZERO; 6];
        let across = vec2(-dir.y, dir.x);
        for (i, point) in points.iter_mut().enumerate() {
            let a = angle.to_radians() + i as f32 / 6. * std::f32::consts::TAU;
            let local = Vec2::from_angle(a) * 16.;
            *point = at + dir * local.dot(dir) * scale.x + across * local.dot(across) * scale.y;
        }
        shapes::polyline(
            &points,
            Stroke::new(3.5, color).join(Join::Miter).closed(true),
        );
        let nose = at + game.facing * 26.;
        shapes::fill_capsule(at + game.facing * 8., nose, 3., color);
    });
    shapes::fill_circle(at, 4., PAPER);
    let nose = at + game.facing * 26.;
    if p.muzzle > 0.0 {
        glow_circle(nose.x, nose.y, 22., PAPER, 1.2 * p.flash_scale());
    }
    let shield = game.shield.value() / game.shield.maximum();
    if shield > 0. && game.health > 0 {
        with_glow(|| {
            shapes::stroke_arc(
                at,
                24.,
                angle.to_radians() + std::f32::consts::PI * (1. - shield),
                std::f32::consts::TAU * shield,
                Stroke::new(1.5 + 1.5 * shield, fade(BATTERY, 0.5)).cap(Cap::Round),
            );
        });
    }
    if p.hurt > 0.0 {
        shapes::stroke_circle(at, 27., 2., fade(CORAL, p.hurt * p.flash_scale()));
    }
}

/// Rings, score pops, low-health edge warning and the evade prompt.
pub(crate) fn overlays(game: &Game, p: &Presentation, world_hint: bool) {
    let juice = &p.juice;
    with_glow(|| {
        for ring in juice.rings.iter().flatten() {
            let t = ring.progress();
            let ease = 1. - (1. - t) * (1. - t);
            let radius = ring.from + (ring.to - ring.from) * ease;
            shapes::stroke_circle(
                ring.at,
                radius,
                ring.width * (1. - t * 0.6),
                fade(ring.color, 1. - t),
            );
        }
    });
    for pop in juice.pops.iter().flatten() {
        let rise = juice.rise(pop);
        let alpha = pop.alpha();
        // Pops land with a tiny overshoot, then rise and fade.
        let land = (pop.age / 0.12).min(1.);
        let scale = 1. + (1. - land) * 0.4 * juice.motion();
        let style = TextStyle::classic(pop.size * scale, fade(pop.color, alpha))
            .with_outline(fade(INK, alpha))
            .with_align(TextAlign::Center)
            .with_pixel_snap(scale == 1.);
        draw_text_styled(pop.text.as_str(), pop.at.x, pop.at.y - 18. - rise, style);
    }
    if game.phase == Phase::Playing && game.health > 0 && game.health <= 30 {
        // Edge warning: slow pulse, or a steady tint under reduced flashes.
        let pulse = if p.flash_scale() > 0. {
            0.6 + 0.4 * ((juice.time * 4.).sin() * 0.5 + 0.5)
        } else {
            0.7
        };
        let strength = (1. - game.health as f32 / 30.) * 0.5 + 0.5;
        for (inset, alpha) in [(0., 0.22), (10., 0.12), (22., 0.06)] {
            draw_rectangle_lines(
                ARENA.x + inset,
                ARENA.y + inset,
                ARENA.w - inset * 2.,
                ARENA.h - inset * 2.,
                12.,
                fade(CORAL, alpha * pulse * strength),
            );
        }
    }
    if world_hint && game.phase == Phase::Playing && game.wave_clear_remaining().is_none() {
        let threats = game
            .world
            .query::<&Enemy>()
            .iter()
            .filter(|enemy| enemy.attack.phase() == ActionPhase::Windup)
            .count();
        if threats > 0 {
            let text = if threats == 1 {
                Line::from("Evade! Attack winding up")
            } else {
                crate::hud::line!("Evade! {threats} attacks winding up")
            };
            let style = TextStyle::modern(16., AMBER).with_align(TextAlign::Center);
            let width = ferravex_render::text::text_width(text.as_str(), style);
            let pill = Rect::new(640. - width * 0.5 - 16., 670., width + 32., 30.);
            shapes::fill_rounded_rect(pill, 15., fade(INK, 0.9));
            shapes::stroke_rounded_rect(pill, 15., 1.5, fade(AMBER, 0.7));
            draw_text_styled(text.as_str(), 640., 691., style);
        }
    }
}

/// Mouse reticle; it brightens with the chain tier.
pub(crate) fn reticle(at: Vec2, game: &Game) {
    let color = tier_color(if game.score.chain() > 0 {
        game.score.multiplier()
    } else {
        0
    });
    with_glow(|| {
        shapes::stroke_circle(at, 9., 1.6, color);
        for d in [Vec2::X, -Vec2::X, Vec2::Y, -Vec2::Y] {
            shapes::polyline(
                &[at + d * 6., at + d * 14.],
                Stroke::new(1.6, color).cap(Cap::Round),
            );
        }
    });
    shapes::fill_circle(at, 1.6, color);
    let _ = FLOOR;
}
