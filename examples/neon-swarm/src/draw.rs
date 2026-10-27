//! World-space drawing: the arena, entities and world-anchored juice. Reads
//! game state and presentation only; nothing here can change the simulation.
use crate::game::{
    ARENA, BEACONS, Bullet, COVER, ELITE_HEALTH, Enemy, Game, PLAYER_RADIUS, Phase, Pickup,
    Position, Supply,
};
use crate::juice::{self, GOLD, Juice, MAGENTA, VIOLET, fade, tier_color};
use crate::presentation::Presentation;
use crate::{AMBER, CORAL, CYAN, FLOOR, INK, LINE, MUTED, PAPER, label, text_width};
use ferravex_play::action::ActionPhase;
use ferravex_render::canvas::Canvas2D;
use ferravex_render::prelude::*;

pub(crate) const REPAIR: Color = Color::from_rgba(130, 246, 160, 255);
pub(crate) const BATTERY: Color = Color::from_rgba(140, 200, 255, 255);
const BRUTE: Color = Color::from_rgba(226, 104, 196, 255);
const CENTER: Vec2 = vec2(640., 414.);

/// The world camera with presentation-only shake and the final-drone close-up.
/// Returns the camera and a pointer mapping without shake, so aim follows
/// what the player sees but never jitters with the camera.
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
        let mut target = camera.target;
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

pub(crate) fn centered(text: &str, x: f32, y: f32, size: u16, color: Color) {
    label(text, x - text_width(text, size) * 0.5, y, size, color);
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
    draw_rectangle(
        ARENA.x - 12.,
        ARENA.y - 12.,
        ARENA.w + 24.,
        ARENA.h + 24.,
        Color::from_rgba(20, 34, 40, 255),
    );
    draw_rectangle(ARENA.x, ARENA.y, ARENA.w, ARENA.h, FLOOR);
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
                draw_rectangle(x + 2., y + 2., 58., 56., Color::from_rgba(20, 34, 40, 255));
            }
        }
    }
    // A slow scanner sweep keeps the floor alive without competing with combat.
    if motion > 0. {
        let sweep = ARENA.y + (time * 70.).rem_euclid(ARENA.h + 120.) - 60.;
        for (offset, alpha) in [(0., 0.06), (8., 0.035), (16., 0.02)] {
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
        draw_circle(x, y, 1.2 + hash(n + 3) * 1.4, fade(CYAN, 0.18));
    }
    // Center seal: readable sector number under quiet rotating ticks.
    draw_circle_lines(
        CENTER.x,
        CENTER.y,
        100.,
        2.,
        Color::from_rgba(38, 64, 70, 255),
    );
    draw_circle_lines(
        CENTER.x,
        CENTER.y,
        88.,
        1.,
        Color::from_rgba(34, 56, 62, 255),
    );
    for i in 0..12 {
        let a = i as f32 / 12. * std::f32::consts::TAU + time * 0.25;
        let d = Vec2::from_angle(a);
        let (a0, a1) = (CENTER + d * 92., CENTER + d * 100.);
        draw_line(a0.x, a0.y, a1.x, a1.y, 3., fade(tier, 0.45));
    }
    centered(
        "07",
        CENTER.x,
        CENTER.y + 22.,
        64,
        Color::from_rgba(48, 84, 90, 255),
    );
    centered(
        "SECTOR",
        CENTER.x,
        CENTER.y - 36.,
        13,
        Color::from_rgba(56, 92, 98, 255),
    );
    for cover in COVER {
        draw_rectangle(cover.x + 6., cover.y + 9., cover.w, cover.h, INK);
        draw_rectangle(
            cover.x,
            cover.y,
            cover.w,
            cover.h,
            Color::from_rgba(38, 56, 62, 255),
        );
        draw_rectangle(cover.x, cover.y, cover.w, 4., fade(CYAN, 0.45));
        draw_rectangle_lines(
            cover.x,
            cover.y,
            cover.w,
            cover.h,
            1.5,
            Color::from_rgba(78, 104, 108, 255),
        );
        for x in 0..6 {
            let x0 = cover.x + 10. + x as f32 * 13.;
            draw_line(
                x0,
                cover.bottom() - 8.,
                x0 + 7.,
                cover.bottom() - 16.,
                4.,
                Color::from_rgba(170, 132, 62, 255),
            );
        }
    }
    // Chain-tier neon border: the arena itself shows how hot the chain is.
    let glow = if game.score.chain() > 0 { 0.7 } else { 0.35 };
    draw_rectangle_lines(
        ARENA.x - 6.,
        ARENA.y - 6.,
        ARENA.w + 12.,
        ARENA.h + 12.,
        8.,
        fade(tier, 0.12 * glow),
    );
    draw_rectangle_lines(
        ARENA.x - 2.,
        ARENA.y - 2.,
        ARENA.w + 4.,
        ARENA.h + 4.,
        3.,
        fade(tier, glow),
    );
    for (x, y, sx, sy) in [
        (ARENA.x, ARENA.y, 1., 1.),
        (ARENA.right(), ARENA.y, -1., 1.),
        (ARENA.x, ARENA.bottom(), 1., -1.),
        (ARENA.right(), ARENA.bottom(), -1., -1.),
    ] {
        draw_line(x, y, x + sx * 34., y, 4., CYAN);
        draw_line(x, y, x, y + sy * 34., 4., CYAN);
    }
    beacons(game, p);
}

fn beacons(game: &Game, p: &Presentation) {
    let time = p.juice.time;
    for (i, at) in BEACONS.into_iter().enumerate() {
        let lit = game.style.beacons & (1 << i) != 0;
        let vertical = at.x == ARENA.x || at.x == ARENA.right();
        let (w, h) = if vertical { (10., 26.) } else { (26., 10.) };
        draw_rectangle(at.x - w * 0.5, at.y - h * 0.5, w, h, INK);
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
            draw_circle(at.x, at.y, 15., fade(color, 0.22));
        }
        draw_circle(at.x, at.y, 6., fade(color, alpha));
        draw_circle_lines(at.x, at.y, 9., 1.5, fade(color, 0.6 * alpha));
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
        draw_circle(p.x, p.y, 20., fade(AMBER, 0.04 + 0.08 * strength));
        draw_circle_lines(p.x, p.y, 22., 2., color);
        for i in 0..4 {
            let d = Vec2::from_angle(i as f32 * std::f32::consts::FRAC_PI_2 + 0.785);
            let (a, b) = (p + d * 30., p + d * 22.);
            draw_line(a.x, a.y, b.x, b.y, 3., color);
        }
        if game.wave == 1 {
            let x = (p.x - 21.).clamp(ARENA.x + 6., ARENA.right() - 50.);
            let y = if p.y < CENTER.y { p.y + 44. } else { p.y - 32. };
            label("ENTRY", x, y, 13, color);
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
        draw_circle(at.x, at.y, 22. * pulse, fade(color, 0.12));
        match pickup.kind {
            Supply::Repair => {
                draw_poly(at.x, at.y, 4, 15. * pulse, 45., fade(color, 0.3));
                draw_poly(at.x, at.y, 4, 12. * pulse, 45., INK);
                draw_rectangle(at.x - 7., at.y - 2.5, 14., 5., color);
                draw_rectangle(at.x - 2.5, at.y - 7., 5., 14., color);
            }
            Supply::Battery => {
                draw_poly(at.x, at.y, 6, 15. * pulse, 30., fade(color, 0.3));
                draw_poly(at.x, at.y, 6, 12. * pulse, 30., INK);
                draw_line(at.x + 3., at.y - 8., at.x - 3., at.y + 1., 3., color);
                draw_line(at.x - 3., at.y + 1., at.x + 3., at.y - 1., 3., color);
                draw_line(at.x + 3., at.y - 1., at.x - 3., at.y + 8., 3., color);
            }
        }
        if pickup.remaining < 3. {
            draw_circle_lines(at.x, at.y, 20., 1.5, AMBER);
        }
        if at.distance(game.player) > 48. {
            let name = match pickup.kind {
                Supply::Repair => "REPAIR",
                Supply::Battery => "SHIELD",
            };
            centered(name, at.x, at.y + 34., 11, fade(color, 0.85));
        }
    }
    for (pos, enemy) in &mut game.world.query::<(&Position, &Enemy)>() {
        enemy_body(game, pos.0, &enemy, p, motion, time);
    }
    for (pos, bullet) in &mut game.world.query::<(&Position, &Bullet)>() {
        let tail = pos.0 - bullet.velocity.normalize() * 24.;
        draw_line(tail.x, tail.y, pos.0.x, pos.0.y, 8., fade(CYAN, 0.18));
        draw_line(tail.x, tail.y, pos.0.x, pos.0.y, 3.5, CYAN);
        draw_circle(pos.0.x, pos.0.y, 3.5, PAPER);
    }
    if let Some(marker) = game.hit_marker {
        let color = if marker.lethal { CYAN } else { PAPER };
        let reach = (if marker.lethal { 14. } else { 10. }) * (1. + emphasis * 0.35);
        for (x, y) in [(-1., -1.), (1., -1.), (-1., 1.), (1., 1.)] {
            draw_line(
                marker.position.x + x * 4.,
                marker.position.y + y * 4.,
                marker.position.x + x * reach,
                marker.position.y + y * reach,
                3.,
                color,
            );
        }
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
    // Hits puff the body up briefly; spawn flash doubles as a pop-in.
    let r = enemy.radius * (1. + enemy.flash * 2.4 * motion);
    let to_player = (game.player - at).normalize_or_zero();
    let heading = to_player.to_angle().to_degrees();
    if enemy.stagger.active() {
        draw_circle_lines(at.x, at.y, r + 9., 2.5, CYAN);
        for i in 0..3 {
            let a = time * 5. * motion + i as f32 * 2.09;
            let s = at + Vec2::from_angle(a) * (r + 9.) - vec2(0., 2.);
            draw_poly(s.x, s.y, 4, 4., a.to_degrees(), CYAN);
        }
    }
    match enemy.attack.phase() {
        ActionPhase::Windup => {
            let progress = enemy.attack.progress();
            // The ring is the actual contact range; progress thickens it.
            let reach = enemy.radius + PLAYER_RADIUS;
            draw_circle(at.x, at.y, reach, fade(AMBER, 0.08 + 0.12 * progress));
            draw_circle_lines(at.x, at.y, reach, 2. + progress * 3., AMBER);
            let badge = at - vec2(0., enemy.radius + 24.);
            draw_circle(badge.x, badge.y, 10., AMBER);
            centered("!", badge.x, badge.y + 7., 18, INK);
        }
        ActionPhase::Active => {
            draw_circle_lines(at.x, at.y, enemy.radius + PLAYER_RADIUS, 4., CORAL);
        }
        _ => {}
    }
    draw_circle(at.x, at.y, r + 7., fade(base, 0.1));
    let sides = if enemy.heavy { 6 } else { 4 };
    draw_poly(at.x + 3., at.y + 6., sides, r + 2., heading, INK);
    draw_poly(at.x, at.y, sides, r, heading, lit);
    draw_poly(
        at.x,
        at.y,
        sides,
        r * 0.62,
        heading,
        Color::from_rgba(40, 14, 24, 255),
    );
    if enemy.elite {
        // Crown spikes and a violet core make the Overseer unmistakable.
        for i in -1..=1 {
            let a = (heading + 180. + i as f32 * 40.).to_radians();
            let d = Vec2::from_angle(a);
            let side = vec2(-d.y, d.x) * 6.;
            let base_point = at + d * r * 0.9;
            draw_triangle(
                base_point - side,
                base_point + side,
                at + d * (r + 14.),
                lit,
            );
        }
        draw_circle(at.x, at.y, r * 0.42, VIOLET);
        let fraction = (enemy.health / ELITE_HEALTH).clamp(0., 1.);
        draw_rectangle(at.x - 32., at.y - r - 26., 64., 6., INK);
        draw_rectangle(at.x - 31., at.y - r - 25., 62. * fraction, 4., GOLD);
    } else if enemy.heavy {
        for pip in 0..(enemy.health / 24.).ceil() as u32 {
            draw_rectangle(at.x - 12. + pip as f32 * 9., at.y - r - 12., 6., 4., lit);
        }
    }
    // A single eye tracks the player: readable facing, mild menace.
    let eye = at + to_player * r * 0.28;
    let eye_r = if enemy.heavy { 5. } else { 3.8 };
    draw_circle(eye.x, eye.y, eye_r, PAPER);
    let pupil = eye + to_player * eye_r * 0.4;
    draw_circle(pupil.x, pupil.y, eye_r * 0.5, INK);
}

fn player(game: &Game, p: &Presentation) {
    let at = game.player;
    let juice = &p.juice;
    let angle = game.facing.to_angle().to_degrees();
    if p.dash > 0.0 {
        for index in 1..5 {
            let ghost = at - juice.stretch_dir * index as f32 * 13.;
            draw_poly(
                ghost.x,
                ghost.y,
                6,
                15.,
                angle,
                fade(CYAN, 0.2 * p.motion_scale() / index as f32),
            );
        }
    }
    draw_circle(at.x, at.y, 30., fade(CYAN, 0.08));
    draw_circle(at.x + 3., at.y + 6., 17., INK);
    let color = if game.health == 0 {
        MUTED
    } else if p.hurt > 0.6 {
        CYAN.lerp(CORAL, p.flash_scale())
    } else {
        CYAN
    };
    let scale = juice.player_scale();
    let dir = juice.stretch_dir.normalize_or(Vec2::X);
    stretched_poly(at, 6, 16., angle, scale, dir, color);
    stretched_poly(at, 6, 10., angle, scale, dir, FLOOR);
    draw_circle(at.x, at.y, 4., PAPER);
    let nose = at + game.facing * 26.;
    draw_line(at.x, at.y, nose.x, nose.y, 6., color);
    if p.muzzle > 0.0 {
        draw_circle(nose.x, nose.y, 8., fade(PAPER, p.flash_scale()));
    }
    let shield = game.shield.value() / game.shield.maximum();
    if shield > 0. && game.health > 0 {
        draw_circle_lines(
            at.x,
            at.y,
            23.,
            1. + 2. * shield,
            fade(crate::draw::BATTERY, 0.25 + 0.45 * shield),
        );
    }
    if p.hurt > 0.0 {
        draw_circle_lines(at.x, at.y, 26., 2., fade(CORAL, p.hurt * p.flash_scale()));
    }
}

/// Rings, score pops, low-health edge warning and the evade prompt.
pub(crate) fn overlays(game: &Game, p: &Presentation, world_hint: bool) {
    let juice = &p.juice;
    for ring in juice.rings.iter().flatten() {
        let t = ring.progress();
        let ease = 1. - (1. - t) * (1. - t);
        let radius = ring.from + (ring.to - ring.from) * ease;
        draw_circle_lines(
            ring.at.x,
            ring.at.y,
            radius,
            ring.width * (1. - t * 0.6),
            fade(ring.color, 1. - t),
        );
    }
    for pop in juice.pops.iter().flatten() {
        let rise = juice.rise(pop);
        let size = pop.size.round() as u16;
        let alpha = pop.alpha();
        let y = pop.at.y - 18. - rise;
        centered(&pop.text, pop.at.x + 2., y + 2., size, fade(INK, alpha));
        centered(&pop.text, pop.at.x, y, size, fade(pop.color, alpha));
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
                "EVADE! ATTACK WINDING UP".to_owned()
            } else {
                format!("EVADE! {threats} ATTACKS WINDING UP")
            };
            let width = text_width(&text, 18);
            draw_rectangle(
                640. - width * 0.5 - 14.,
                672.,
                width + 28.,
                30.,
                fade(INK, 0.9),
            );
            draw_rectangle(640. - width * 0.5 - 14., 672., 4., 30., AMBER);
            label(&text, 640. - width * 0.5, 694., 18, AMBER);
        }
    }
}

/// Mouse reticle; it brightens with the chain tier.
pub(crate) fn reticle(at: Vec2, game: &Game) {
    let color = juice::tier_color(if game.score.chain() > 0 {
        game.score.multiplier()
    } else {
        0
    });
    draw_circle_lines(at.x, at.y, 9., 1.6, color);
    draw_circle(at.x, at.y, 1.6, color);
    for d in [Vec2::X, -Vec2::X, Vec2::Y, -Vec2::Y] {
        let (a, b) = (at + d * 6., at + d * 14.);
        draw_line(a.x, a.y, b.x, b.y, 1.6, color);
    }
    let _ = (LINE, MAGENTA);
}
