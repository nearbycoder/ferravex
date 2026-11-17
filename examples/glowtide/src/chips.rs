//! Icon + short-effect chips that replace sentences on upgrade cards, the
//! character picker, the Shrine and the chest ("+30%" beside a sword instead
//! of "+30% damage."). Every number comes straight from the rules:
//! `weapon_stats` deltas, passive modifiers, character stats and Shrine
//! bonuses, so a chip can never drift from what a pick actually does.
//!
//! One icon means one idea everywhere: sword damage, plus more projectiles,
//! target area, clock cooldown, wind speed, hourglass duration, heart HP,
//! shield armor, star critical hits, leaf luck, hand pickup reach, gem
//! Glimmer, restart reroll and cross banish.
use crate::gfx::{color, rgba};
use crate::hud::{GOLD, PAPER, RED, r};
use crate::meta::Meta;
use crate::sim::content::*;
use crate::sim::{Card, weapon_stats};
use crate::text::{Align, Face, SMALL, Text, width_in};
use ferravex_render::icons::{self, Icon, IconStyle};
use ferravex_render::prelude::{Color, vec2};
use ferravex_render::shapes;
use std::fmt;

/// Up to three chips describe any card, character or Shrine rank.
pub const MAX: usize = 3;
const MINT: [f32; 3] = [0.55, 1., 0.7];

/// A tiny inline string, so chips never allocate.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Label {
    buf: [u8; 14],
    len: u8,
}

impl Label {
    pub fn new(args: fmt::Arguments) -> Self {
        let mut buf = [0; 14];
        let len = crate::menus::fmt_into(&mut buf, args).len() as u8;
        Self { buf, len }
    }
    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.buf[..usize::from(self.len)]).unwrap_or("")
    }
}

impl fmt::Debug for Label {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Good news (mint), a cost (red) or a plain fact (the card's own color).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Good,
    Bad,
    Fact,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Chip {
    pub icon: Icon,
    pub label: Label,
    pub tone: Tone,
}

pub type Chips = [Option<Chip>; MAX];

fn push(out: &mut Chips, icon: Icon, tone: Tone, args: fmt::Arguments) {
    if let Some(slot) = out.iter_mut().find(|c| c.is_none()) {
        *slot = Some(Chip {
            icon,
            label: Label::new(args),
            tone,
        });
    }
}

/// Whole-percent change of a multiplier ratio (1.3 -> 30).
fn pct(ratio: f32) -> i32 {
    ((ratio - 1.) * 100.).round() as i32
}

/// What a brand-new weapon does, in a word or two.
fn gist(id: WeaponId) -> &'static [(Icon, &'static str)] {
    match id {
        WeaponId::WispBolt => &[(Icon::Target, "Homing")],
        WeaponId::HaloOrbit => &[(Icon::Restart, "Orbits")],
        WeaponId::BubbleMortar => &[(Icon::Bomb, "Blast")],
        WeaponId::TideLash => &[(Icon::Sword, "Sweep")],
        WeaponId::ChainSpark => &[(Icon::Bolt, "Chains")],
        WeaponId::LanternAura => &[(Icon::Flame, "Aura"), (Icon::Snowflake, "Slow")],
        WeaponId::ShellBoomerang => &[(Icon::Undo, "Returns")],
        WeaponId::StarSkipper => &[(Icon::Star, "Bounces")],
    }
}

/// What picking `card` does when it would reach level `next`.
pub fn card(card: Card, next: u8) -> Chips {
    let mut out = [None; MAX];
    match card {
        Card::Weapon(id) if next <= 1 => {
            for &(icon, text) in gist(id) {
                push(&mut out, icon, Tone::Fact, format_args!("{text}"));
            }
        }
        Card::Weapon(id) => weapon_level(id, next.min(WEAPON_MAX), &mut out),
        Card::Passive(id) => passive(id, next, &mut out),
        Card::Snack => push(&mut out, Icon::Heart, Tone::Good, format_args!("+30%")),
        Card::Pebble => push(&mut out, Icon::Gem, Tone::Good, format_args!("+25")),
    }
    out
}

/// The difference between a weapon's stats at `next - 1` and `next`.
fn weapon_level(id: WeaponId, next: u8, out: &mut Chips) {
    let a = weapon_stats(id, next - 1, false);
    let b = weapon_stats(id, next, false);
    let good = Tone::Good;
    if b.amount > a.amount {
        push(
            out,
            Icon::Plus,
            good,
            format_args!("+{}", b.amount - a.amount),
        );
    }
    if b.damage > a.damage * 1.001 {
        push(
            out,
            Icon::Sword,
            good,
            format_args!("+{}%", pct(b.damage / a.damage)),
        );
    }
    if b.area > a.area * 1.001 {
        push(
            out,
            Icon::Target,
            good,
            format_args!("+{}%", pct(b.area / a.area)),
        );
    }
    if b.cooldown < a.cooldown * 0.999 {
        let cut = -pct(b.cooldown / a.cooldown);
        push(out, Icon::Clock, good, format_args!("-{cut}%"));
    }
    if b.speed > a.speed * 1.001 {
        push(
            out,
            Icon::Wind,
            good,
            format_args!("+{}%", pct(b.speed / a.speed)),
        );
    }
    if b.pierce > a.pierce {
        let more = b.pierce - a.pierce;
        push(out, Icon::ArrowRight, good, format_args!("+{more} pierce"));
    }
    if b.duration.is_finite() && b.duration > a.duration + 0.001 {
        if id == WeaponId::ShellBoomerang {
            // A boomerang's "duration" is how far it flies out.
            let farther = pct(b.duration / a.duration);
            push(out, Icon::Undo, good, format_args!("+{farther}%"));
        } else {
            let longer = b.duration - a.duration;
            push(out, Icon::Hourglass, good, format_args!("+{longer:.0}s"));
        }
    }
    if b.extra > a.extra {
        match id {
            WeaponId::TideLash => push(out, Icon::ArrowLeft, good, format_args!("Both ways")),
            WeaponId::ChainSpark => {
                let jumps = b.extra - a.extra;
                push(out, Icon::Bolt, good, format_args!("+{jumps:.0} jumps"));
            }
            WeaponId::LanternAura => {
                let slow = b.extra * 100.;
                push(out, Icon::Snowflake, good, format_args!("{slow:.0}% slow"));
            }
            _ => {}
        }
    }
}

/// One rank of a passive, mirroring `Game::recompute_mods`.
fn passive(id: PassiveId, next: u8, out: &mut Chips) {
    let g = Tone::Good;
    match id {
        PassiveId::MoonPearl => push(out, Icon::Clock, g, format_args!("-8%")),
        PassiveId::TidalSpiral => push(out, Icon::Target, g, format_args!("+10%")),
        PassiveId::PressureValve => push(out, Icon::Sword, g, format_args!("+10%")),
        PassiveId::StingingCells => {
            push(out, Icon::Star, g, format_args!("+6% crit"));
            push(out, Icon::Sword, g, format_args!("+15% crit"));
        }
        // Odd levels add a projectile to every weapon; even levels set it up.
        PassiveId::EchoShell if next % 2 == 1 => push(out, Icon::Plus, g, format_args!("+1 all")),
        PassiveId::EchoShell => push(out, Icon::Plus, Tone::Fact, format_args!("+1 next")),
        PassiveId::KelpTea => {
            push(out, Icon::Drop, g, format_args!("+0.3/s"));
            push(out, Icon::Heart, g, format_args!("+8"));
        }
        PassiveId::SwiftFins => {
            push(out, Icon::Wind, g, format_args!("+8%"));
            push(out, Icon::Clock, g, format_args!("-8% dash"));
        }
        PassiveId::LuckyBarnacle => {
            push(out, Icon::Leaf, g, format_args!("+10% luck"));
            push(out, Icon::Star, g, format_args!("+1% crit"));
        }
        PassiveId::MagnetAnemone => push(out, Icon::Hand, g, format_args!("+30%")),
        PassiveId::ThickShell => {
            push(out, Icon::Shield, g, format_args!("+1"));
            push(out, Icon::Heart, g, format_args!("+15"));
        }
    }
}

/// A character's strengths and costs against the base lantern.
pub fn character(i: usize) -> Chips {
    let c = &CHARACTERS[i.min(CHARACTERS.len() - 1)];
    let mut out = [None; MAX];
    let tone = |up: bool| if up { Tone::Good } else { Tone::Bad };
    let hp = c.max_hp;
    let hp_tone = if hp == 100. {
        Tone::Fact
    } else {
        tone(hp > 100.)
    };
    push(&mut out, Icon::Heart, hp_tone, format_args!("{hp:.0}"));
    for (icon, ratio) in [(Icon::Sword, c.might), (Icon::Target, c.area)] {
        if (ratio - 1.).abs() > 0.001 {
            push(
                &mut out,
                icon,
                tone(ratio > 1.),
                format_args!("{:+}%", pct(ratio)),
            );
        }
    }
    // Bramble's shell: `Game::recompute_mods` grants character 3 one armor.
    if i == 3 {
        push(&mut out, Icon::Shield, Tone::Good, format_args!("+1"));
    }
    if (c.speed - 1.).abs() > 0.001 {
        let up = c.speed > 1.;
        push(
            &mut out,
            Icon::Wind,
            tone(up),
            format_args!("{:+}%", pct(c.speed)),
        );
    }
    out
}

/// What one more rank of Shrine item `item` grants, from `Meta::bonuses`.
pub fn shrine(item: usize) -> Chips {
    let base = Meta::default().bonuses();
    let mut meta = Meta::default();
    meta.shrine[item.min(SHRINE_ITEMS - 1)] = 1;
    let b = meta.bonuses();
    let mut out = [None; MAX];
    let g = Tone::Good;
    if b.might > base.might {
        push(
            &mut out,
            Icon::Sword,
            g,
            format_args!("+{}%", pct(b.might / base.might)),
        );
    }
    if b.max_hp > base.max_hp {
        push(
            &mut out,
            Icon::Heart,
            g,
            format_args!("+{:.0}", b.max_hp - base.max_hp),
        );
    }
    if b.speed > base.speed {
        push(
            &mut out,
            Icon::Wind,
            g,
            format_args!("+{}%", pct(b.speed / base.speed)),
        );
    }
    if b.magnet > base.magnet {
        push(
            &mut out,
            Icon::Hand,
            g,
            format_args!("+{}%", pct(b.magnet / base.magnet)),
        );
    }
    if b.rerolls > base.rerolls {
        push(
            &mut out,
            Icon::Restart,
            g,
            format_args!("+{}", b.rerolls - base.rerolls),
        );
    }
    if b.banishes > base.banishes {
        push(
            &mut out,
            Icon::Cross,
            g,
            format_args!("+{}", b.banishes - base.banishes),
        );
    }
    if b.greed > base.greed {
        push(
            &mut out,
            Icon::Gem,
            g,
            format_args!("+{}%", pct(b.greed / base.greed)),
        );
    }
    if b.revives > base.revives {
        push(&mut out, Icon::Heart, g, format_args!("Revive"));
    }
    out
}

fn tone_color(tone: Tone, fact: [f32; 3]) -> [f32; 3] {
    match tone {
        Tone::Good => MINT,
        Tone::Bad => RED,
        Tone::Fact => fact,
    }
}

/// Chip height for a text size (SMALL or BODY).
pub fn height(size: f32) -> f32 {
    if size <= SMALL { 18. } else { 28. }
}

/// Width of one chip at a text size.
pub fn width(chip: &Chip, size: f32) -> f32 {
    let icon = height(size) - 8.;
    7. + icon + 5. + width_in(Face::Modern, chip.label.as_str(), size) + 9.
}

/// Draw one chip with its left edge at `x`, centered on `cy`. Returns its width.
pub fn draw(tx: &mut Text, chip: &Chip, x: f32, cy: f32, size: f32, fact: [f32; 3], a: f32) -> f32 {
    let w = width(chip, size);
    let h = height(size);
    let c = tone_color(chip.tone, fact);
    let rect = r(vec2(x, cy - h * 0.5), vec2(w, h));
    shapes::fill_rounded_rect(rect, h * 0.5, Color::new(0.02, 0.04, 0.09, 0.72 * a));
    shapes::stroke_rounded_rect(rect, h * 0.5, 1., color(rgba(c, 0.5 * a)));
    let icon = h - 8.;
    icons::draw_styled(
        chip.icon,
        vec2(x + 7. + icon * 0.5, cy),
        icon,
        IconStyle::new(color(rgba(c, a))).shadow(Color::new(0., 0., 0., 0.4 * a)),
    );
    tx.label(
        vec2(x + 7. + icon + 5., cy - size * 0.5),
        w,
        chip.label.as_str(),
        size,
        [PAPER[0], PAPER[1], PAPER[2], a],
        Align::Left,
    );
    w
}

/// Split chips into rows no wider than `max_w`: calls `row(start, end, width)`
/// for each row and returns the number of rows.
fn wrap(chips: &Chips, max_w: f32, size: f32, mut row: impl FnMut(usize, usize, f32)) -> usize {
    let mut drawn = 0;
    let mut rows = 0;
    while drawn < MAX {
        // Take as many chips as fit on this row (at least one).
        let mut end = drawn;
        let mut row_w = 0.;
        while let Some(Some(chip)) = chips.get(end) {
            let w = width(chip, size) + if end > drawn { GAP } else { 0. };
            if end > drawn && row_w + w > max_w {
                break;
            }
            row_w += w;
            end += 1;
        }
        if end == drawn {
            break;
        }
        row(drawn, end, row_w);
        rows += 1;
        drawn = end;
    }
    rows
}

const GAP: f32 = 6.;

/// Height `rows` will use for these chips.
pub fn measure(chips: &Chips, max_w: f32, size: f32) -> f32 {
    let n = wrap(chips, max_w, size, |_, _, _| {}) as f32;
    (n * (height(size) + GAP) - GAP).max(0.)
}

/// `preferred` text size if the chips fit `room` at it, else the small size.
pub fn fit(chips: &Chips, max_w: f32, room: f32, preferred: f32) -> f32 {
    if measure(chips, max_w, preferred) <= room {
        preferred
    } else {
        SMALL
    }
}

/// Lay chips out in rows no wider than `max_w`, centered in that width when
/// `center` (else from its left edge `x`). Returns the height used.
#[allow(clippy::too_many_arguments)]
pub fn rows(
    tx: &mut Text,
    chips: &Chips,
    x: f32,
    max_w: f32,
    top: f32,
    size: f32,
    center: bool,
    fact: [f32; 3],
    a: f32,
) -> f32 {
    let h = height(size);
    let mut y = top;
    wrap(chips, max_w, size, |start, end, row_w| {
        let mut at = if center { x + (max_w - row_w) * 0.5 } else { x };
        for chip in chips[start..end].iter().flatten() {
            at += draw(tx, chip, at, y + h * 0.5, size, fact, a) + GAP;
        }
        y += h + GAP;
    });
    (y - top - GAP).max(0.)
}

/// A gold "evolve" tag: a star and the chest that will do it.
pub fn evolve_tag(p: &mut crate::gfx::Painter, center_x: f32, cy: f32, t: f32, a: f32) {
    let w = 74.;
    let rect = r(vec2(center_x - w * 0.5, cy - 11.), vec2(w, 22.));
    let pulse = 0.65 + 0.35 * (t * 5.).sin().abs();
    shapes::fill_rounded_rect(rect, 11., color(rgba(GOLD, 0.22 * a)));
    shapes::stroke_rounded_rect(rect, 11., 1.5, color(rgba(GOLD, pulse * a)));
    icons::draw(
        Icon::Star,
        vec2(rect.x + 14., cy),
        14.,
        color(rgba(GOLD, a)),
    );
    crate::world::chest_icon(p, vec2(rect.x + w - 20., cy + 1.), 0.42, t, 0.);
    let arrow = r(vec2(rect.x + 27., cy - 1.), vec2(12., 2.));
    shapes::fill_rounded_rect(arrow, 1., color(rgba(GOLD, a)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::{Game, RunConfig};

    fn labels(chips: &Chips) -> Vec<String> {
        chips
            .iter()
            .flatten()
            .map(|c| c.label.as_str().to_owned())
            .collect()
    }

    #[test]
    fn weapon_chips_match_the_authored_stat_changes() {
        // Spot checks against `weapon_stats` and the old sentences.
        let wisp = |l| labels(&card(Card::Weapon(WeaponId::WispBolt), l)).join(" ");
        assert_eq!(wisp(1), "Homing");
        assert_eq!(wisp(2), "+1");
        assert_eq!(wisp(3), "+30%");
        assert_eq!(wisp(4), "+1 pierce");
        assert_eq!(wisp(6), "-20%");
        assert_eq!(wisp(7), "+1 +40%");
        let halo = |l| labels(&card(Card::Weapon(WeaponId::HaloOrbit), l)).join(" ");
        assert_eq!(halo(3), "+20% +25%");
        assert_eq!(halo(5), "+1s");
        let lash = labels(&card(Card::Weapon(WeaponId::TideLash), 2));
        assert_eq!(lash, ["Both ways"]);
        let shell = labels(&card(Card::Weapon(WeaponId::ShellBoomerang), 4));
        assert_eq!(shell, ["+25%"]);
        let aura = labels(&card(Card::Weapon(WeaponId::LanternAura), 7));
        assert_eq!(aura, ["+25%", "40% slow"]);
        // Every level of every weapon says something, briefly.
        for id in WeaponId::ALL {
            for level in 1..=WEAPON_MAX {
                let chips = card(Card::Weapon(id), level);
                assert!(chips[0].is_some(), "{id:?} level {level}");
                for chip in chips.iter().flatten() {
                    let text = chip.label.as_str();
                    assert!(!text.is_empty() && text.split(' ').count() <= 2, "{text}");
                }
            }
        }
    }

    #[test]
    fn passive_chips_match_recompute_mods() {
        for id in PassiveId::ALL {
            let mut game = Game::new(RunConfig::default());
            let before = game.mods;
            game.apply_card(Card::Passive(id));
            let after = game.mods;
            let chips = labels(&card(Card::Passive(id), 1));
            let expect: Vec<String> = match id {
                PassiveId::MoonPearl => vec![format!("-{}%", -pct(after.haste / before.haste))],
                PassiveId::TidalSpiral => vec![format!("+{}%", pct(after.area / before.area))],
                PassiveId::PressureValve => vec![format!("+{}%", pct(after.might / before.might))],
                PassiveId::StingingCells => vec![
                    format!("+{:.0}% crit", (after.crit - before.crit) * 100.),
                    format!("+{:.0}% crit", (after.crit_mult - before.crit_mult) * 100.),
                ],
                PassiveId::EchoShell => vec![format!("+{} all", after.amount - before.amount)],
                PassiveId::KelpTea => vec![
                    format!("+{:.1}/s", after.regen - before.regen),
                    format!("+{:.0}", after.max_hp - before.max_hp),
                ],
                PassiveId::SwiftFins => vec![
                    format!("+{}%", pct(after.move_speed / before.move_speed)),
                    format!("-{}% dash", -pct(after.dash_cd / before.dash_cd)),
                ],
                PassiveId::LuckyBarnacle => vec![
                    format!("+{}% luck", pct(after.luck / before.luck)),
                    format!("+{:.0}% crit", (after.crit - before.crit) * 100.),
                ],
                PassiveId::MagnetAnemone => {
                    vec![format!("+{}%", pct(after.magnet / before.magnet))]
                }
                PassiveId::ThickShell => vec![
                    format!("+{:.0}", after.armor - before.armor),
                    format!("+{:.0}", after.max_hp - before.max_hp),
                ],
            };
            assert_eq!(chips, expect, "{id:?}");
        }
        assert_eq!(
            labels(&card(Card::Passive(PassiveId::EchoShell), 2)),
            ["+1 next"]
        );
        assert_eq!(labels(&card(Card::Snack, 1)), ["+30%"]);
        assert_eq!(labels(&card(Card::Pebble, 1)), ["+25"]);
    }

    #[test]
    fn characters_and_shrine_chips_mirror_their_stats() {
        assert_eq!(labels(&character(0)), ["100"]);
        assert_eq!(labels(&character(1)), ["120", "+10%", "-6%"]);
        assert_eq!(labels(&character(2)), ["90", "+15%", "+6%"]);
        assert_eq!(labels(&character(3)), ["150", "+1", "-10%"]);
        let mut bramble = Game::new(RunConfig {
            character: 3,
            ..RunConfig::default()
        });
        bramble.apply_card(Card::Passive(PassiveId::MoonPearl));
        assert_eq!(bramble.mods.armor, 1., "Bramble's shield chip is real");
        let shrine: Vec<Vec<String>> = (0..SHRINE_ITEMS).map(|i| labels(&shrine(i))).collect();
        assert_eq!(
            shrine,
            [
                vec!["+5%"],
                vec!["+10"],
                vec!["+4%"],
                vec!["+15%"],
                vec!["+1"],
                vec!["+1"],
                vec!["+10%"],
                vec!["Revive"]
            ]
        );
        let label = Label::new(format_args!("{}", "much too long for a chip"));
        assert_eq!(
            label.as_str().len(),
            14,
            "labels truncate instead of allocating"
        );
    }
}
