//! Procedural vector icons for weapons, passives and fallback cards.
use crate::gfx::{Painter, mix3, rgba, scale3};
use crate::sim::Card;
use crate::sim::content::*;
use ferravex_render::prelude::{Vec2, vec2};

fn rot(v: Vec2, a: f32) -> Vec2 {
    let (s, c) = a.sin_cos();
    vec2(v.x * c - v.y * s, v.x * s + v.y * c)
}

/// Draw `card`'s icon centered at `at` with radius `r`. `t` animates gently.
pub fn card(p: &mut Painter, card: Card, at: Vec2, r: f32, t: f32, evolved: bool) {
    match card {
        Card::Weapon(w) => weapon(p, w, at, r, t, evolved),
        Card::Passive(id) => passive(p, id, at, r, t),
        Card::Snack => {
            let c = [0.45, 0.9, 0.45];
            for k in 0..3 {
                let a = k as f32 * 2.1 + 0.5;
                let q = at + vec2(a.cos(), a.sin()) * r * 0.35;
                p.disc_grad(
                    false,
                    q,
                    r * 0.38,
                    rgba(mix3(c, [1.; 3], 0.4), 1.),
                    rgba(scale3(c, 0.6), 1.),
                    12,
                );
            }
            p.glow(at, r * 2., rgba(c, 0.25));
        }
        Card::Pebble => {
            let c = [1., 0.85, 0.5];
            p.ellipse(
                false,
                at,
                r * 0.7,
                r * 0.5,
                0.3,
                rgba([1., 1., 0.9], 1.),
                rgba(scale3(c, 0.7), 1.),
                16,
            );
            p.disc(
                true,
                at + vec2(-r * 0.25, -r * 0.2),
                r * 0.15,
                rgba([1.; 3], 0.9),
                6,
            );
            p.glow(at, r * 2., rgba(c, 0.35));
        }
    }
}

pub fn weapon(p: &mut Painter, w: WeaponId, at: Vec2, r: f32, t: f32, evolved: bool) {
    let c = if evolved {
        mix3(w.info().color, [1., 0.85, 0.5], 0.5)
    } else {
        w.info().color
    };
    p.glow(at, r * 2.2, rgba(c, if evolved { 0.45 } else { 0.28 }));
    match w {
        WeaponId::WispBolt => {
            for k in 0..3 {
                let off = vec2(
                    -r * 0.45 + k as f32 * r * 0.4,
                    r * 0.35 - k as f32 * r * 0.35,
                );
                let dir = vec2(1., -0.8).normalize();
                p.line_grad(
                    false,
                    at + off - dir * r * 0.5,
                    at + off,
                    r * 0.22,
                    rgba(c, 0.),
                    rgba(c, 0.9),
                );
                p.disc_grad(false, at + off, r * 0.2, rgba([1.; 3], 1.), rgba(c, 1.), 10);
            }
            if evolved {
                p.line(
                    false,
                    at - vec2(r * 0.8, -r * 0.8),
                    at + vec2(r * 0.8, -r * 0.8),
                    r * 0.12,
                    rgba([1.; 3], 1.),
                );
            }
        }
        WeaponId::HaloOrbit => {
            p.ring(false, at, r * 0.6, r * 0.68, rgba(c, 0.6), rgba(c, 0.6), 24);
            for k in 0..3 {
                let a = t * 2. + k as f32 * std::f32::consts::TAU / 3.;
                let q = at + vec2(a.cos(), a.sin()) * r * 0.64;
                p.disc_grad(false, q, r * 0.22, rgba([1., 1., 0.9], 1.), rgba(c, 1.), 10);
            }
            p.disc(false, at, r * 0.18, rgba([1., 0.95, 0.8], 1.), 10);
        }
        WeaponId::BubbleMortar => {
            p.disc_grad(
                false,
                at + vec2(r * 0.15, r * 0.1),
                r * 0.55,
                rgba(mix3(c, [1.; 3], 0.3), 0.5),
                rgba(c, 0.8),
                16,
            );
            p.ring(
                false,
                at + vec2(r * 0.15, r * 0.1),
                r * 0.5,
                r * 0.56,
                rgba([1.; 3], 0.9),
                rgba([1.; 3], 0.9),
                16,
            );
            p.disc(
                false,
                at + vec2(-r * 0.45, -r * 0.45),
                r * 0.22,
                rgba(c, 0.8),
                10,
            );
            p.disc(
                false,
                at + vec2(-r * 0.05, -r * 0.1),
                r * 0.12,
                rgba([1.; 3], 0.9),
                6,
            );
        }
        WeaponId::TideLash => {
            p.arc(
                false,
                at + vec2(-r * 0.3, r * 0.4),
                r * 0.75,
                r * 0.95,
                -1.6,
                0.2,
                rgba(c, 0.9),
                rgba(mix3(c, [1.; 3], 0.4), 1.),
                8,
            );
            p.arc(
                false,
                at + vec2(-r * 0.3, r * 0.4),
                r * 0.45,
                r * 0.6,
                -1.4,
                0.0,
                rgba(c, 0.6),
                rgba(c, 0.8),
                6,
            );
        }
        WeaponId::ChainSpark => {
            let pts = [
                at + vec2(-r * 0.3, -r * 0.8),
                at + vec2(r * 0.15, -r * 0.1),
                at + vec2(-r * 0.15, 0.),
                at + vec2(r * 0.35, r * 0.8),
            ];
            for k in 0..3 {
                p.line(
                    false,
                    pts[k],
                    pts[k + 1],
                    r * 0.2,
                    rgba(mix3(c, [1.; 3], 0.5), 1.),
                );
            }
        }
        WeaponId::LanternAura => {
            p.disc_grad(true, at, r * 0.9, rgba(c, 0.5), rgba(c, 0.), 20);
            p.round_rect_v(
                false,
                at + vec2(-r * 0.3, -r * 0.45),
                vec2(r * 0.6, r * 0.85),
                r * 0.15,
                rgba([1., 0.9, 0.6], 1.),
                rgba(c, 1.),
            );
            p.rect(
                false,
                at + vec2(-r * 0.38, -r * 0.55),
                vec2(r * 0.76, r * 0.14),
                rgba([0.4, 0.25, 0.15], 1.),
            );
            p.disc(
                false,
                at + vec2(0., -r * 0.05),
                r * 0.13,
                rgba([1.; 3], 1.),
                8,
            );
        }
        WeaponId::ShellBoomerang => {
            let spin = t * 3.;
            p.disc_grad(
                false,
                at,
                r * 0.6,
                rgba(mix3(c, [1.; 3], 0.4), 1.),
                rgba(scale3(c, 0.6), 1.),
                16,
            );
            for k in 0..3 {
                let a = spin + k as f32 * 2.1;
                p.line(
                    false,
                    at,
                    at + vec2(a.cos(), a.sin()) * r * 0.6,
                    r * 0.08,
                    rgba(scale3(c, 0.35), 1.),
                );
            }
            p.arc(
                false,
                at,
                r * 0.75,
                r * 0.85,
                0.3,
                2.6,
                rgba(c, 0.),
                rgba(c, 0.8),
                6,
            );
        }
        WeaponId::StarSkipper => {
            p.star(
                false,
                at,
                r * 0.7,
                r * 0.3,
                5,
                t * 1.5,
                rgba([1., 1., 0.85], 1.),
                rgba(c, 1.),
            );
            let trail = rot(vec2(-1., 0.6).normalize(), 0.) * r;
            p.line_grad(
                false,
                at + trail,
                at + trail * 0.4,
                r * 0.15,
                rgba(c, 0.),
                rgba(c, 0.6),
            );
        }
    }
}

pub fn passive(p: &mut Painter, id: PassiveId, at: Vec2, r: f32, t: f32) {
    let c = id.info().color;
    p.glow(at, r * 1.8, rgba(c, 0.2));
    let dark = rgba(scale3(c, 0.45), 1.);
    let light = rgba(mix3(c, [1.; 3], 0.5), 1.);
    match id {
        PassiveId::MoonPearl => {
            p.disc_grad(false, at, r * 0.55, light, rgba(c, 1.), 16);
            p.disc(
                false,
                at + vec2(r * 0.2, -r * 0.1),
                r * 0.45,
                rgba([0.1, 0.12, 0.2], 0.55),
                16,
            );
            p.disc(
                false,
                at + vec2(-r * 0.2, -r * 0.2),
                r * 0.1,
                rgba([1.; 3], 0.9),
                6,
            );
        }
        PassiveId::TidalSpiral => {
            for k in 0..14 {
                let f = k as f32 / 14.;
                let a = f * 9. + t;
                let q = at + vec2(a.cos(), a.sin()) * r * (0.1 + f * 0.6);
                p.disc(false, q, r * (0.06 + f * 0.08), rgba(c, 0.5 + f * 0.5), 6);
            }
        }
        PassiveId::PressureValve => {
            p.disc_grad(false, at, r * 0.6, light, dark, 16);
            p.line(
                false,
                at,
                at + rot(vec2(0., -r * 0.45), (t * 2.).sin() * 0.8),
                r * 0.12,
                rgba([1., 0.4, 0.3], 1.),
            );
            p.rect(
                false,
                at + vec2(-r * 0.15, -r * 0.85),
                vec2(r * 0.3, r * 0.25),
                dark,
            );
        }
        PassiveId::StingingCells => {
            for k in 0..6 {
                let a = k as f32 / 6. * std::f32::consts::TAU + t * 0.5;
                let d = vec2(a.cos(), a.sin());
                p.tri(
                    false,
                    at + d * r * 0.25 + vec2(-d.y, d.x) * r * 0.12,
                    at + d * r * 0.75,
                    at + d * r * 0.25 - vec2(-d.y, d.x) * r * 0.12,
                    light,
                    rgba([1., 1., 0.8], 1.),
                    light,
                );
            }
            p.disc(false, at, r * 0.28, rgba(c, 1.), 10);
        }
        PassiveId::EchoShell => {
            p.arc(false, at, r * 0.25, r * 0.65, -2.4, 2.4, light, dark, 10);
            p.ring(
                false,
                at,
                r * 0.75,
                r * 0.82,
                rgba(c, 0.6),
                rgba(c, 0.6),
                16,
            );
        }
        PassiveId::KelpTea => {
            p.round_rect_v(
                false,
                at + vec2(-r * 0.45, -r * 0.2),
                vec2(r * 0.9, r * 0.7),
                r * 0.2,
                rgba([0.9, 0.95, 1.], 1.),
                rgba([0.7, 0.75, 0.85], 1.),
            );
            p.rect(
                false,
                at + vec2(-r * 0.35, -r * 0.1),
                vec2(r * 0.7, r * 0.2),
                rgba(c, 1.),
            );
            p.line(
                false,
                at + vec2(-r * 0.1, -r * 0.3),
                at + vec2(-r * 0.2, -r * 0.7),
                r * 0.08,
                rgba(c, 0.8),
            );
            p.line(
                false,
                at + vec2(r * 0.1, -r * 0.3),
                at + vec2(r * 0.2, -r * 0.65),
                r * 0.08,
                rgba(c, 0.8),
            );
        }
        PassiveId::SwiftFins => {
            for k in 0..2 {
                let o = vec2(-r * 0.25 + k as f32 * r * 0.4, 0.);
                p.tri(
                    false,
                    at + o + vec2(-r * 0.3, -r * 0.55),
                    at + o + vec2(r * 0.3, 0.),
                    at + o + vec2(-r * 0.3, r * 0.55),
                    light,
                    rgba(c, 1.),
                    dark,
                );
            }
        }
        PassiveId::LuckyBarnacle => {
            p.disc_grad(false, at + vec2(0., r * 0.1), r * 0.6, light, dark, 6);
            p.disc(
                false,
                at + vec2(0., -r * 0.1),
                r * 0.22,
                rgba([0.2, 0.15, 0.1], 1.),
                8,
            );
            p.star(
                true,
                at + vec2(r * 0.55, -r * 0.5),
                r * 0.25,
                r * 0.08,
                4,
                t,
                rgba([1.; 3], 1.),
                rgba(c, 0.8),
            );
        }
        PassiveId::MagnetAnemone => {
            for k in 0..9 {
                let a = k as f32 / 9. * std::f32::consts::TAU + (t * 2.).sin() * 0.1;
                p.line_grad(
                    false,
                    at,
                    at + vec2(a.cos(), a.sin()) * r * 0.7,
                    r * 0.12,
                    dark,
                    light,
                );
            }
            p.disc(false, at, r * 0.25, light, 10);
        }
        PassiveId::ThickShell => {
            p.arc(
                false,
                at + vec2(0., r * 0.3),
                0.,
                r * 0.75,
                std::f32::consts::PI,
                std::f32::consts::TAU,
                light,
                dark,
                10,
            );
            for k in 0..4 {
                let x = -r * 0.5 + k as f32 * r * 0.33;
                p.line(
                    false,
                    at + vec2(0., r * 0.3),
                    at + vec2(x, -r * 0.35),
                    r * 0.06,
                    dark,
                );
            }
        }
    }
}
