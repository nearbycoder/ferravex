//! Deterministic demo pilot used by `--autoplay`, smoke captures and tests.
//!
//! It reads only game state and returns the same [`Input`] a human would
//! produce: steer, dash, pick cards. It kites the horde in wide arcs, steps out
//! of telegraphs, dashes through danger, hoovers up motes when it is safe and
//! builds toward evolutions.
use super::content::*;
use super::{Card, Choice, Game, Input, PLAYER_RADIUS, Phase, PickupKind, Shape};
use ferravex_render::prelude::{Vec2, vec2};

const DIRECTIONS: usize = 24;

/// Pause this many ticks on a level-up/treasure screen so demos are readable.
pub const READ_TICKS: u32 = 40;

pub fn autoplay(game: &Game) -> Input {
    match game.phase {
        Phase::LevelUp => {
            if game.menu_ticks < READ_TICKS {
                return Input::default();
            }
            return Input {
                choice: choose_card(game),
                ..Input::default()
            };
        }
        Phase::Chest => {
            if game.menu_ticks < READ_TICKS + 20 {
                return Input::default();
            }
            return Input {
                choice: Choice::Pick(0),
                ..Input::default()
            };
        }
        Phase::Won | Phase::Lost => return Input::default(),
        Phase::Running => {}
    }
    let p = game.player.pos;
    // Score candidate directions by sampling a short look-ahead.
    let mut best = (f32::NEG_INFINITY, Vec2::ZERO);
    let mut danger_here = 0.;
    let hp = game.player.hp / game.player.max_hp.max(1.);
    let goal = goal(game, hp);
    let swirl = swirl_direction(game);
    for k in 0..=DIRECTIONS {
        let dir = if k == DIRECTIONS {
            Vec2::ZERO
        } else {
            let a = k as f32 / DIRECTIONS as f32 * std::f32::consts::TAU;
            vec2(a.cos(), a.sin())
        };
        let mut score = 0.;
        for look in [26., 60.] {
            let at = p + dir * look;
            score -= danger(game, at) * if look < 30. { 1.4 } else { 1. };
        }
        if k == DIRECTIONS {
            danger_here = danger(game, p);
            score -= 0.6; // standing still is rarely wise in the deep
        } else {
            if let Some(goal) = goal {
                score += dir.dot((goal - p).normalize_or_zero()) * 0.9;
            }
            score += dir.dot(swirl) * 0.55;
            score += dir.dot(game.player.vel.normalize_or_zero()) * 0.25;
            // Do not wander too far from the origin's reef.
            score -= dir.dot(p.normalize_or_zero()) * (p.length() / 4000.).min(0.6);
        }
        if score > best.0 {
            best = (score, dir);
        }
    }
    let dir = best.1;
    let dash_ready = game.player.dash_cd <= 0.;
    let threatened = danger_here > 2.2 || in_hazard(game, p, 0.45);
    Input {
        movement: dir,
        dash: dash_ready && threatened && dir != Vec2::ZERO,
        choice: Choice::None,
    }
}

/// Kite around the centroid of nearby foes: the classic survivors circle.
fn swirl_direction(game: &Game) -> Vec2 {
    let p = game.player.pos;
    let mut sum = Vec2::ZERO;
    let mut n = 0.;
    game.grid.visit(p, 320., |j| {
        if let Some(f) = game.foes.get(j)
            && !f.dead
        {
            sum += f.pos;
            n += 1.;
        }
    });
    if n < 1. {
        return Vec2::ZERO;
    }
    let away = (p - sum / n).normalize_or_zero();
    let tangent = vec2(-away.y, away.x);
    (tangent * 0.8 + away * 0.45).normalize_or_zero()
}

fn goal(game: &Game, hp: f32) -> Option<Vec2> {
    let p = game.player.pos;
    let mut best: Option<(f32, Vec2)> = None;
    for pickup in &game.pickups {
        let weight = match pickup.kind {
            PickupKind::Chest(_) => 3.,
            PickupKind::Heal if hp < 0.6 => 2.5,
            PickupKind::Magnet => 2.,
            PickupKind::Coin(_) => 1.,
            _ => 0.4,
        };
        let d = (pickup.pos - p).length();
        if d < 520. {
            let score = weight * 400. / (d + 60.);
            if best.is_none_or(|b| score > b.0) {
                best = Some((score, pickup.pos));
            }
        }
    }
    if best.is_none() {
        for m in game.motes.iter().filter(|m| !m.pulled) {
            let d = (m.pos - p).length();
            if d < 260. {
                let score = (m.value as f32).sqrt() * 100. / (d + 40.);
                if best.is_none_or(|b| score > b.0) {
                    best = Some((score, m.pos));
                }
            }
        }
    }
    best.map(|b| b.1)
}

fn in_hazard(game: &Game, at: Vec2, soon: f32) -> bool {
    game.telegraphs.iter().any(|t| {
        t.delay - t.age < soon
            && (t.damage > 0. || t.style == 1)
            && t.shape.contains(at, PLAYER_RADIUS + 6.)
    })
}

fn danger(game: &Game, at: Vec2) -> f32 {
    let mut d = 0.;
    game.grid.visit(at, 150., |j| {
        let Some(f) = game.foes.get(j) else {
            return;
        };
        if f.dead {
            return;
        }
        let gap = (f.pos - at).length() - f.radius - PLAYER_RADIUS;
        let mut weight = 1. + f.damage / 12.;
        if f.kind == FoeKind::Sneel && f.state >= 1 {
            weight *= 2.5;
        }
        if f.kind.is_boss() {
            weight *= 4.;
        }
        if gap < 120. {
            d += weight * (1. / (gap.max(4.) / 22.)).min(6.) * 0.35;
        }
    });
    for b in &game.bullets {
        let ahead = b.pos + b.vel * 0.18;
        for q in [b.pos, ahead] {
            let gap = (q - at).length() - b.radius - PLAYER_RADIUS;
            if gap < 50. {
                d += (1. / (gap.max(3.) / 14.)).min(6.) * 0.8;
            }
        }
    }
    for t in &game.telegraphs {
        let pad = PLAYER_RADIUS + 10.;
        if t.shape.contains(at, pad) {
            let urgency = 1. + t.progress() * 3.;
            d += match t.shape {
                Shape::Circle { .. } => 3. * urgency,
                Shape::Lane { .. } => 4. * urgency,
            };
        }
    }
    if let Some(boss) = game.boss() {
        // Give the big ones room.
        let gap = (boss.pos - at).length() - boss.radius;
        if gap < 140. {
            d += (140. - gap) / 25.;
        }
    }
    d
}

fn card_score(game: &Game, card: Card) -> f32 {
    let hp = game.player.hp / game.player.max_hp.max(1.);
    match card {
        Card::Weapon(w) => match game.weapon(w) {
            Some(slot) => {
                let pair = game.passive_level(w.info().pair) > 0;
                100. + f32::from(slot.level) * 4. + if pair { 20. } else { 0. }
            }
            None => {
                if game.weapon_count() < 4 {
                    92.
                } else {
                    25.
                }
            }
        },
        Card::Passive(p) => {
            let pairs = game
                .weapons
                .iter()
                .flatten()
                .any(|s| s.id.info().pair == p && !s.evolved);
            let base = match p {
                PassiveId::MoonPearl | PassiveId::PressureValve | PassiveId::TidalSpiral => 64.,
                PassiveId::ThickShell | PassiveId::KelpTea => 52.,
                PassiveId::EchoShell => 58.,
                PassiveId::MagnetAnemone => 34.,
                _ => 44.,
            };
            base + if pairs { 30. } else { 0. }
        }
        Card::Snack => {
            if hp < 0.5 {
                85.
            } else {
                5.
            }
        }
        Card::Pebble => 10.,
    }
}

fn choose_card(game: &Game) -> Choice {
    let cards = game.offer.cards();
    let mut best = (f32::NEG_INFINITY, 0);
    for (i, &card) in cards.iter().enumerate() {
        let s = card_score(game, card);
        if s > best.0 {
            best = (s, i);
        }
    }
    if best.0 < 40. && game.rerolls > 0 {
        return Choice::Reroll;
    }
    Choice::Pick(best.1 as u8)
}
