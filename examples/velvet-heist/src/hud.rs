//! Read-only interface panels, all icons, sprites, meters and numbers: the
//! clock, alarm and medal strip over the map, the stealth sidebar and its
//! pointer buttons, the header objective, the between-heist cards and the
//! results card.
use crate::model::{COIN_ZONE, CROUCH_ZONE, Cause, Game, Phase, WORK_ZONE, Zone};
use crate::sprites::*;
use crate::sprites::{GOLD, RED};
use crate::view::Exposure;
use ferravex_arcade::typography::{TextAlign, TextStyle, draw_text_styled, text_width};
use ferravex_arcade::*;

/// Vertical center of the strip above the map.
pub(crate) const STRIP_Y: f32 = 136.;
pub(crate) const CLOCK_AT: Vec2 = vec2(44., STRIP_Y);
const BELL_X: f32 = 142.;
const METER_X: f32 = 160.;
const METER_W: f32 = 352.;
/// Forecast medal `i` in the strip.
pub(crate) fn medal_at(i: usize) -> Vec2 {
    vec2(566. + i as f32 * 28., STRIP_Y)
}
/// Medal `i` on the debrief card.
pub(crate) fn card_medal_at(i: usize) -> Vec2 {
    vec2(124. + i as f32 * 54., 322.)
}
/// The debrief card's banked points, right of its medals.
pub(crate) fn bank_at() -> Vec2 {
    vec2(326., 322.)
}
const EYE_AT: Vec2 = Vec2::new(730., 180.);
/// Card frame over the map.
const CARD_X: f32 = 72.;
const CARD_Y: f32 = 190.;
const CARD_W: f32 = 535.;
const CARD_H: f32 = 336.;
const IN_X: f32 = 100.;
const IN_W: f32 = 479.;

fn clock_text(seconds: u32) -> String {
    format!("{}:{:02}", seconds / 60, seconds % 60)
}
/// `1690` as `1,690`.
pub(crate) fn grouped(n: u32) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}
/// A number centered vertically on `cy`; returns its width.
fn number(value: &str, x: f32, cy: f32, size: f32, color: Color) -> f32 {
    let s = style(size, color);
    draw_text_styled(value, x, s.centered_baseline(cy), s).width
}
/// Width of a bare key cap.
fn cap_width(key: &str) -> f32 {
    (text_width(key, TextStyle::modern(13., PAPER)) + 12.).max(20.)
}
/// A raised sidebar button; returns how far it lifts under the pointer.
fn button(zone: Zone, enabled: bool, accent: Color) -> f32 {
    let p = pointer();
    let hover = enabled && zone.contains(p.x, p.y);
    let lift = if hover { 2. } else { 0. };
    let (x, y, w, h) = (zone.x, zone.y - lift, zone.w, zone.h);
    rounded(x, y + 3. + lift, w, h, 10., rgba(0., 0., 0., 0.35));
    let fill = if hover {
        rgba(0.17, 0.115, 0.21, 1.)
    } else {
        rgba(0.115, 0.08, 0.145, 1.)
    };
    rounded_gradient(x, y, w, h, 10., lighten(fill, 0.04), fill);
    let rim = if hover {
        with_alpha(accent, 0.75)
    } else {
        rgba(1., 1., 1., 0.07)
    };
    outline(x, y, w, h, 10., if hover { 1.5 } else { 1. }, rim);
    lift
}
impl Game {
    /// Clock ring, alarm meter and the medal forecast above the map.
    pub(crate) fn draw_strip(&self) {
        let t = clock();
        let live = self.phase == Phase::Infiltrating;
        let total = self.content().seconds;
        let left = self.seconds_left();
        let urgent = live && left <= 20;
        let k = pulse(t, 2.);
        let time_color = if urgent {
            shapes::mix(RED, lighten(RED, 0.55), k)
        } else {
            GOLD
        };
        if urgent {
            glow(
                CLOCK_AT.x,
                CLOCK_AT.y,
                22. + 5. * k,
                with_alpha(RED, 0.3 * k),
            );
        }
        let remaining = 1. - self.elapsed as f32 / (total * 60) as f32;
        ring_meter(CLOCK_AT.x, CLOCK_AT.y, 13., 3.5, remaining, time_color);
        icon(Icon::Clock, CLOCK_AT.x, CLOCK_AT.y, 14., time_color);
        number(&clock_text(left), 64., STRIP_Y, 20., time_color);
        // Alarm: a bell that rattles while a guard watches, a meter that
        // remembers its peak (the peak decides the medals) and quarter marks.
        let alarm = f32::from(self.alarm) / 1000.;
        let peak = f32::from(self.peak_alarm) / 1000.;
        let rising = live && self.seen;
        let jiggle = if rising { (t * 42.).sin() * 2. } else { 0. };
        let bell = if rising || alarm > 0. {
            RED
        } else if peak > 0. {
            lighten(RED, 0.35)
        } else {
            MUTED
        };
        if rising {
            glow(BELL_X, STRIP_Y, 16. + 4. * k, with_alpha(RED, 0.3));
        }
        icon(Icon::Bell, BELL_X + jiggle, STRIP_Y, 19., bell);
        let (y, h) = (STRIP_Y - 6., 12.);
        rounded(METER_X, y, METER_W, h, 6., rgba(0.02, 0.012, 0.03, 0.85));
        outline(METER_X, y, METER_W, h, 6., 1., rgba(1., 1., 1., 0.07));
        if peak > alarm {
            rounded(
                METER_X,
                y,
                (METER_W * peak).max(h),
                h,
                6.,
                with_alpha(RED, 0.24),
            );
        }
        if alarm > 0. {
            let w = (METER_W * alarm).max(h);
            let top = lighten(RED, if rising { 0.2 + 0.25 * k } else { 0.2 });
            rounded_gradient(METER_X, y, w, h, 6., top, darken(RED, 0.18));
            if rising {
                glow(
                    METER_X + w - 6.,
                    STRIP_Y,
                    18.,
                    with_alpha(RED, 0.35 + 0.25 * k),
                );
            }
        }
        for q in 1..4 {
            draw_rectangle(
                METER_X + METER_W * q as f32 / 4.,
                y + 3.,
                1.,
                h - 6.,
                rgba(1., 1., 1., 0.13),
            );
        }
        // Medals an extraction would earn now: the alarm breaks two, both
        // artworks complete the third.
        let clean = self.peak_alarm == 0;
        let art = self.extras >= self.extra_total;
        for i in 0..3 {
            let state = match i {
                0 => Medal::Lit,
                _ if !clean => Medal::Lost,
                1 => Medal::Lit,
                _ if art => Medal::Lit,
                _ => Medal::Pending,
            };
            medal(medal_at(i), 10., state, 1.);
        }
    }
    /// The stealth sidebar: exposure, guards, the three action buttons and kit.
    pub(crate) fn draw_sidebar(&self) {
        let t = clock();
        card(670., 116., 274., 448., CARD);
        let live = self.phase == Phase::Infiltrating;
        self.draw_exposure(t, live);
        self.draw_posture(live);
        self.draw_coins(live, t);
        self.draw_action(live, t);
        self.draw_kit(t);
    }
    fn draw_exposure(&self, t: f32, live: bool) {
        let exposure = self.exposure();
        let (eye, word, color) = match exposure {
            Exposure::Seen => (Icon::Eye, "SEEN", RED),
            Exposure::Hidden => (Icon::EyeOff, "HIDDEN", ORCHID),
            Exposure::Exposed => (Icon::Eye, "EXPOSED", AMBER),
        };
        let seen = exposure == Exposure::Seen;
        let k = pulse(t, if seen { 3. } else { 0.5 });
        glow(
            EYE_AT.x,
            EYE_AT.y,
            50. + 8. * k,
            with_alpha(color, if seen { 0.3 + 0.2 * k } else { 0.12 + 0.05 * k }),
        );
        shapes::fill_circle(EYE_AT, 38., rgba(0.045, 0.03, 0.06, 1.));
        shapes::stroke_circle(EYE_AT, 38., 3., with_alpha(color, 0.9));
        if seen {
            shapes::stroke_circle(EYE_AT, 42. + 4. * k, 1.5, with_alpha(RED, 0.5 * (1. - k)));
        }
        icon(eye, EYE_AT.x, EYE_AT.y, 42., color);
        let s = TextStyle::modern(16., color).with_letter_spacing(1.6);
        draw_text_styled(word, 784., 172., s);
        // One head per guard; its bubble says who watches and who searches.
        for (i, g) in self.guards[..self.guard_count].iter().enumerate() {
            let at = vec2(796. + i as f32 * 34., 202.);
            let watching = live && self.watching(g);
            let a = if watching || g.search > 0 { 1. } else { 0.75 };
            guard_head(at, 0.95, (0., 0.), a);
            let mark = if watching {
                Some(("!", RED))
            } else if g.search > 0 {
                Some(("?", AMBER))
            } else {
                None
            };
            if let Some((glyph, fill)) = mark {
                let s = TextStyle::modern(13., fill).with_align(TextAlign::Center);
                shapes::fill_circle(at + vec2(10., -9.), 6.5, rgba(0.05, 0.03, 0.06, 1.));
                shapes::stroke_circle(at + vec2(10., -9.), 6.5, 1.2, fill);
                draw_text_styled(glyph, at.x + 10., s.centered_baseline(at.y - 9.), s);
            }
        }
    }
    /// X: standing (fast, loud) or crouching (slow, silent, hides in shadow).
    fn draw_posture(&self, live: bool) {
        let z = CROUCH_ZONE;
        let lift = button(z, live, ORCHID);
        let cy = z.y + z.h * 0.5 - lift;
        let a = if live { 1. } else { 0.55 };
        keycap("X", "", z.x + 14., cy - 10., PAPER);
        let up = !self.crouched;
        let stand = vec2(z.x + 70., cy - 1.);
        let low = vec2(z.x + 178., cy - 1.);
        let dim = 0.28 * a;
        if up {
            glow(stand.x, stand.y + 2., 22., with_alpha(AMBER, 0.16 * a));
        } else {
            glow(low.x, low.y + 4., 22., with_alpha(ORCHID, 0.2 * a));
        }
        thief(stand, 0.78, 0., 0., if up { a } else { dim });
        sound_arcs(
            stand + vec2(12., -4.),
            0.8,
            with_alpha(AMBER, if up { 0.95 * a } else { dim }),
        );
        thief(low, 0.78, 5., 0., if up { dim } else { a });
        // A two-way switch between the postures.
        let track = Rect::new(z.x + 104., cy - 7., 44., 14.);
        rounded(
            track.x,
            track.y,
            track.w,
            track.h,
            7.,
            rgba(0., 0., 0., 0.45),
        );
        outline(
            track.x,
            track.y,
            track.w,
            track.h,
            7.,
            1.,
            rgba(1., 1., 1., 0.1),
        );
        let knob = vec2(
            if up {
                track.x + 7.
            } else {
                track.x + track.w - 7.
            },
            cy,
        );
        shapes::fill_circle(knob, 5.5, fade(if up { AMBER } else { ORCHID }, a.max(0.6)));
    }
    /// C: the heist's three coins and the marked landing.
    fn draw_coins(&self, live: bool, t: f32) {
        let z = COIN_ZONE;
        let lift = button(z, live && self.coins > 0, GOLD);
        let cy = z.y + z.h * 0.5 - lift;
        let a = if live { 1. } else { 0.55 };
        keycap("C", "", z.x + 14., cy - 10., PAPER);
        for i in 0..3u8 {
            let x = z.x + 66. + f32::from(i) * 30.;
            if i < self.coins {
                coin(x, cy, 9., a);
            } else {
                shapes::stroke_circle(vec2(x, cy), 8., 1.5, GHOST);
            }
        }
        let at = vec2(z.x + z.w - 26., cy);
        if self.aim.is_some() {
            glow(at.x, at.y, 16., with_alpha(GOLD, 0.2 + 0.1 * pulse(t, 1.5)));
            reticle(at, 8., fade(GOLD, a));
        } else {
            reticle(at, 8., with_alpha(MUTED, 0.35 * a));
            icon(
                Icon::Pointer,
                at.x + 9.,
                at.y + 9.,
                12.,
                with_alpha(MUTED, 0.55 * a),
            );
        }
    }
    /// Space: what it would act on now, its reward, and progress while working.
    fn draw_action(&self, live: bool, t: f32) {
        let z = WORK_ZONE;
        let target = self
            .interaction()
            .filter(|_| live && self.work.is_none())
            .map(|(_, kind)| kind);
        let accent = target.map_or(MUTED, kind_color);
        let lift = button(z, target.is_some(), accent);
        let cy = z.y + z.h * 0.5 - lift;
        keycap("Space", "", z.x + 14., cy - 10., PAPER);
        let at = vec2(z.x + 110., cy);
        if let Some(w) = self.work {
            let done = 1. - f32::from(w.remaining) / f32::from(w.total);
            let color = kind_color(w.kind);
            glow(at.x, at.y, 30., with_alpha(color, 0.2));
            object(w.kind, at, 0.85, t);
            ring_meter(at.x, at.y, 24., 4., done, color);
            self.draw_reward(w.kind, z.x + 150., cy);
        } else if let Some(kind) = target {
            let k = pulse(t, 1.6);
            glow(
                at.x,
                at.y,
                26. + 4. * k,
                with_alpha(kind_color(kind), 0.16 + 0.1 * k),
            );
            object(kind, at, 0.85, t);
            self.draw_reward(kind, z.x + 150., cy);
        } else {
            icon(Icon::Hand, at.x, at.y, 24., with_alpha(MUTED, 0.35));
        }
    }
    /// What finishing the work earns: points, an unlocked door or dark beams.
    fn draw_reward(&self, kind: u8, x: f32, cy: f32) {
        let points = match kind {
            b'L' => 250,
            b'$' => 175,
            b'E' => self.extraction_points(),
            b'T' => {
                chevron(vec2(x + 4., cy), 5., with_alpha(MUTED, 0.7));
                beam(vec2(x + 40., cy), 36., false, 1.);
                done_badge(vec2(x + 62., cy - 9.), 6.);
                return;
            }
            _ => {
                chevron(vec2(x + 4., cy), 5., with_alpha(MUTED, 0.7));
                icon(Icon::Unlock, x + 30., cy, 22., GOLD);
                return;
            }
        };
        icon(Icon::Star, x + 10., cy, 18., GOLD);
        number(&format!("+{}", grouped(points)), x + 22., cy, 18., GOLD);
    }
    /// The brass pass and the beam grid, as they stand on this floor.
    fn draw_kit(&self, t: f32) {
        for (i, accent) in [GOLD, MINT].into_iter().enumerate() {
            let x = 682. + i as f32 * 129.;
            rounded(x, 444., 121., 64., 12., rgba(0., 0., 0., 0.24));
            outline(x, 444., 121., 64., 12., 1., with_alpha(accent, 0.12));
        }
        let pass = vec2(742., 476.);
        if self.key {
            glow(
                pass.x,
                pass.y,
                28.,
                with_alpha(GOLD, 0.16 + 0.05 * pulse(t, 0.6)),
            );
            key(pass.x, pass.y, 1.9, GOLD, CARD);
            done_badge(pass + vec2(40., -18.), 7.);
        } else {
            key(pass.x, pass.y, 1.9, GHOST, CARD);
            padlock(pass.x + 40., pass.y - 17., 1.3, with_alpha(GOLD, 0.7));
        }
        let grid = vec2(871., 476.);
        if !self.tiles.contains(&b'!') {
            // No beams on this floor: an empty, quiet slot.
            beam(grid - vec2(0., 8.), 64., false, 0.35);
            beam(grid + vec2(0., 8.), 64., false, 0.35);
        } else if self.disabled {
            beam(grid - vec2(0., 8.), 64., false, 1.);
            beam(grid + vec2(0., 8.), 64., false, 1.);
            done_badge(grid + vec2(40., -18.), 7.);
        } else {
            let flicker = 0.75 + 0.25 * (t * 30.).sin();
            glow(grid.x, grid.y, 30., with_alpha(RED, 0.12));
            beam(grid - vec2(0., 8.), 64., true, flicker);
            beam(grid + vec2(0., 8.), 64., true, flicker);
            icon(Icon::Bolt, grid.x + 40., grid.y - 18., 15., RED);
        }
        // Career favors: each heart is one retry.
        for i in 0..3u8 {
            let at = vec2(773. + f32::from(i) * 34., 533.);
            if i < self.retries {
                icon(Icon::Heart, at.x, at.y, 24., ROSE);
            } else {
                icon(Icon::Heart, at.x, at.y, 24., GHOST);
            }
        }
    }
    /// The header objective: records to an exit, then the optional art.
    pub(crate) fn draw_goal(&self, area: Rect) {
        let t = clock();
        let cy = area.y + area.h * 0.5;
        let mut x = area.x + 16.;
        for i in 0..self.needed {
            if i < self.stolen {
                record(x, cy, 1.05, 1.);
                done_badge(vec2(x + 7., cy + 8.), 5.);
            } else {
                slot(x, cy, 19., 23., with_alpha(PAPER, 0.22));
                record(x, cy, 1.05, 0.3);
            }
            x += 27.;
        }
        let ready = self.stolen >= self.needed;
        chevron(
            vec2(x, cy),
            5.,
            if ready { MINT } else { with_alpha(MUTED, 0.6) },
        );
        let exit = vec2(x + 24., cy);
        if ready {
            glow(
                exit.x,
                exit.y,
                20. + 4. * pulse(t, 1.2),
                with_alpha(MINT, 0.3),
            );
        }
        exit_sign(exit, 24., 26., ready, t);
        // Optional artworks: the third medal.
        x = exit.x + 34.;
        let plus = with_alpha(MUTED, 0.7);
        shapes::fill_capsule(vec2(x - 5., cy), vec2(x + 5., cy), 1.1, plus);
        shapes::fill_capsule(vec2(x, cy - 5.), vec2(x, cy + 5.), 1.1, plus);
        x += 22.;
        for i in 0..self.extra_total {
            if i < self.extras {
                painting(x, cy, 22., 17., 1.);
                done_badge(vec2(x + 10., cy + 7.), 5.);
            } else {
                slot(x, cy, 26., 21., with_alpha(GOLD, 0.2));
                painting(x, cy, 22., 17., 0.3);
            }
            x += 30.;
        }
    }
    pub(crate) fn draw_card(&self) {
        let t = clock();
        draw_rectangle(
            crate::model::MAP_X - 1.,
            crate::model::MAP_Y - 1.,
            crate::view::MAP_W + 1.,
            crate::view::MAP_H + 1.,
            rgba(0.02, 0.01, 0.03, 0.62),
        );
        card(CARD_X, CARD_Y, CARD_W, CARD_H, rgba(0.1, 0.07, 0.13, 0.99));
        match self.phase {
            Phase::Briefing => self.draw_briefing(t),
            Phase::Debrief => self.draw_debrief(t),
            Phase::Setback => self.draw_setback(t),
            _ => {}
        }
    }
    /// Any click or Space confirms; the bar lights while the pointer is over the card.
    fn confirm(&self, label: &str, glyph: Icon, accent: Color, t: f32) {
        let p = pointer();
        let hover =
            (CARD_X..CARD_X + CARD_W).contains(&p.x) && (CARD_Y..CARD_Y + CARD_H).contains(&p.y);
        let (x, y, w, h) = (IN_X, 458., IN_W, 46.);
        let k = pulse(t, 0.8);
        let tint = if hover { 0.32 } else { 0.16 + 0.06 * k };
        rounded(x, y + 3., w, h, 12., rgba(0., 0., 0., 0.35));
        rounded_gradient(
            x,
            y,
            w,
            h,
            12.,
            with_alpha(accent, tint + 0.06),
            with_alpha(accent, tint),
        );
        outline(
            x,
            y,
            w,
            h,
            12.,
            1.5,
            with_alpha(accent, if hover { 0.9 } else { 0.5 }),
        );
        let cap = cap_width("Space");
        let s = style(18., PAPER);
        let lw = text_width(label, s);
        let total = cap + 14. + 24. + 8. + lw;
        let mut cx = x + (w - total) * 0.5;
        keycap("Space", "", cx, y + h * 0.5 - 10., PAPER);
        cx += cap + 14.;
        icon(glyph, cx + 12., y + h * 0.5, 22., PAPER);
        cx += 32.;
        draw_text_styled(label, cx, s.centered_baseline(y + h * 0.5), s);
    }
    fn draw_briefing(&self, t: f32) {
        let heist = self.content();
        draw_text_styled(heist.name, IN_X, 232., style(26., PAPER));
        draw_text_styled(heist.object, IN_X, 258., TextStyle::modern(15., ORCHID));
        divider(IN_X, 276., IN_W, rgba(1., 1., 1., 0.12));
        // What waits on this floor, as counts.
        let cy = 306.;
        let mut x = IN_X + 14.;
        ring_meter(x, cy, 12., 3., 1., GOLD);
        icon(Icon::Clock, x, cy, 13., GOLD);
        x += number(&clock_text(heist.seconds), x + 20., cy, 20., GOLD) + 50.;
        let count = |sprite: &dyn Fn(Vec2), n: usize, x: &mut f32, color: Color| {
            sprite(vec2(*x, cy));
            *x += number(&n.to_string(), *x + 16., cy, 20., color) + 46.;
        };
        count(
            &|c| record(c.x, c.y, 1.1, 1.),
            usize::from(self.needed),
            &mut x,
            PAPER,
        );
        count(
            &|c| guard_head(c, 1., (0., 0.), 1.),
            self.guard_count,
            &mut x,
            lighten(RED, 0.3),
        );
        count(
            &|c| painting(c.x, c.y, 24., 19., 0.8),
            usize::from(self.extra_total),
            &mut x,
            GOLD,
        );
        let doors = self.tiles.iter().filter(|&&tile| tile == b'=').count();
        count(&|c| door(c, 0.75, true, 0.), doors, &mut x, GOLD);
        let beams = self.tiles.iter().filter(|&&tile| tile == b'!').count();
        if beams > 0 {
            count(&|c| beam(c, 26., true, 1.), beams, &mut x, RED);
        }
        // This floor's lesson as a picture equation.
        rounded(IN_X, 340., IN_W, 92., 14., rgba(0., 0., 0., 0.26));
        outline(IN_X, 340., IN_W, 92., 14., 1., rgba(1., 1., 1., 0.06));
        draw_tip(self.level, IN_X + IN_W * 0.5, 386., t);
        self.confirm("Enter", Icon::Play, ORCHID, t);
    }
    fn draw_debrief(&self, t: f32) {
        draw_text_styled("Escaped", IN_X, 236., style(28., MINT));
        draw_text_styled(
            self.content().name,
            IN_X,
            262.,
            TextStyle::modern(15., MUTED),
        );
        // Missed medals show why: the alarm broke them, or the art was left.
        let earned = self.medals[self.level];
        for i in 0..3u8 {
            let at = card_medal_at(usize::from(i));
            let state = if i < earned {
                glow(
                    at.x,
                    at.y,
                    34.,
                    with_alpha(GOLD, 0.18 + 0.06 * pulse(t + f32::from(i) * 0.3, 0.7)),
                );
                Medal::Lit
            } else if earned == 2 {
                Medal::Pending
            } else {
                Medal::Lost
            };
            medal(at, 22., state, 1.);
        }
        // The heist's points, banked beside its medals.
        let bank = bank_at();
        glow(
            bank.x + 60.,
            bank.y,
            46.,
            with_alpha(GOLD, 0.08 + 0.04 * pulse(t, 0.7)),
        );
        icon(Icon::Star, bank.x, bank.y, 30., GOLD);
        number(
            &format!(
                "+{}",
                grouped(self.score.saturating_sub(self.opening_score))
            ),
            bank.x + 24.,
            bank.y,
            32.,
            GOLD,
        );
        // Haul: records, artworks and the time left on the clock.
        let cy = 396.;
        let mut x = IN_X + 10.;
        record(x, cy, 1.15, 1.);
        x += number(&self.stolen.to_string(), x + 16., cy, 22., PAPER) + 46.;
        painting(x, cy, 26., 20., 1.);
        x += number(
            &format!("{}/{}", self.extras, self.extra_total),
            x + 20.,
            cy,
            22.,
            GOLD,
        ) + 52.;
        icon(Icon::Clock, x, cy, 20., GOLD);
        number(&clock_text(self.seconds_left()), x + 16., cy, 22., GOLD);
        self.confirm("Next", Icon::ArrowRight, MINT, t);
    }
    fn draw_setback(&self, t: f32) {
        let (word, color) = match self.cause {
            Cause::Guard => ("Caught", RED),
            Cause::Alarm => ("Lockdown", RED),
            Cause::Time => ("Time up", AMBER),
        };
        let at = vec2(IN_X + 34., 250.);
        glow(
            at.x,
            at.y,
            44.,
            with_alpha(color, 0.18 + 0.08 * pulse(t, 1.)),
        );
        shapes::fill_circle(at, 32., rgba(0.05, 0.03, 0.06, 1.));
        shapes::stroke_circle(at, 32., 2.5, color);
        match self.cause {
            Cause::Guard => {
                guard_head(at + vec2(0., 2.), 2.1, (0., 0.), 1.);
                bubble(at + vec2(24., -26.), "!", RED, 1.);
            }
            Cause::Alarm => icon(Icon::Bell, at.x, at.y, 36., RED),
            Cause::Time => icon(Icon::Hourglass, at.x, at.y, 36., AMBER),
        }
        draw_text_styled(word, IN_X + 84., 248., style(28., color));
        draw_text_styled(
            self.content().name,
            IN_X + 84.,
            272.,
            TextStyle::modern(15., MUTED),
        );
        // Favors: a retry spends the brightest heart.
        let cy = 370.;
        for i in 0..3u8 {
            let c = vec2(IN_X + 30. + f32::from(i) * 56., cy);
            let spend = i + 1 == self.retries;
            if i < self.retries {
                let k = if spend { pulse(t, 1.4) } else { 0. };
                if spend {
                    glow(c.x, c.y, 30. + 4. * k, with_alpha(ROSE, 0.2 + 0.15 * k));
                }
                icon(Icon::Heart, c.x, c.y, 38. + 3. * k, ROSE);
            } else {
                icon(Icon::Heart, c.x, c.y, 38., GHOST);
            }
        }
        if self.retries > 0 {
            let c = vec2(IN_X + 30. + f32::from(self.retries - 1) * 56., cy);
            let s = style(17., ROSE).with_align(TextAlign::Center);
            draw_text_styled("-1", c.x, cy + 44., s);
        }
        self.confirm("Retry", Icon::Restart, ROSE, t);
    }
    /// The ending's key numbers for the host's results card.
    pub(crate) fn draw_results(&self, area: Rect) {
        let t = clock();
        let medals = self.medals.iter().sum::<u8>();
        let cy = area.y + 40.;
        let col = area.w / 3.;
        let won = self.phase == Phase::Escaped;
        // Column 1: heists cleared (as pips) or records saved.
        let c1 = area.x + col * 0.5;
        if won {
            record(c1 - 38., cy, 2., 1.);
            number(&self.total_loot.to_string(), c1 - 12., cy, 34., PAPER);
        } else {
            let cleared = self.cleared() as u32;
            let w = pips(c1 - 38., cy, cleared + 1, 6, ORCHID);
            number(&format!("{cleared}/6"), c1 - 38. + w + 12., cy, 26., PAPER);
        }
        // Column 2: medals of 18.
        let c2 = area.x + col * 1.5;
        medal(vec2(c2 - 40., cy), 19., Medal::Lit, 1.);
        number(&format!("{medals}/18"), c2 - 14., cy, 30., GOLD);
        // Column 3: favors kept, or records saved when caught.
        let c3 = area.x + col * 2.5;
        if won {
            icon(Icon::Heart, c3 - 36., cy, 34., ROSE);
            number(&self.retries.to_string(), c3 - 10., cy, 34., ROSE);
        } else {
            record(c3 - 38., cy, 2., 1.);
            number(&self.total_loot.to_string(), c3 - 12., cy, 34., PAPER);
        }
        // Each heist's medals.
        let row = area.y + 106.;
        let step = area.w / 6.;
        for (heist, &got) in self.medals.iter().enumerate() {
            let x = area.x + step * (heist as f32 + 0.5);
            for m in 0..3u8 {
                let at = vec2(x - 18. + f32::from(m) * 18., row);
                if m < got {
                    let k = 0.06 * pulse(t + heist as f32 * 0.2, 0.6);
                    glow(at.x, at.y, 12., with_alpha(GOLD, 0.12 + k));
                    medal(at, 7., Medal::Lit, 1.);
                } else {
                    shapes::stroke_circle(at, 6., 1.2, GHOST);
                }
            }
        }
    }
}
/// One heist's lesson as a picture equation, centered on (`cx`, `cy`).
fn draw_tip(level: usize, cx: f32, cy: f32, t: f32) {
    use Glyph::*;
    let tips: &[&[Glyph]] = match level {
        0 => &[
            &[Crouched, Plus, Shade, Equals, Hidden],
            &[Pass, Arrow, Door],
        ],
        1 => &[&[Console, Arrow, BeamOff]],
        2 => &[&[Cabinet, Equals, Hidden], &[Coin, Arrow, Guard]],
        3 => &[&[Records(3), Arrow, Exit], &[Pass, Arrow, Door]],
        4 => &[&[BeamOn, Arrow, Bell]],
        _ => &[&[Records(3), Arrow, Exit]],
    };
    let gap = 56.;
    let widths: f32 = tips
        .iter()
        .map(|tip| tip.iter().map(|g| g.width()).sum::<f32>())
        .sum::<f32>()
        + gap * (tips.len() as f32 - 1.);
    let mut x = cx - widths * 0.5;
    for (i, tip) in tips.iter().enumerate() {
        if i > 0 {
            draw_rectangle(x + gap * 0.5, cy - 22., 1., 44., rgba(1., 1., 1., 0.1));
            x += gap;
        }
        for glyph in tip.iter() {
            let w = glyph.width();
            glyph.draw(vec2(x + w * 0.5, cy), t);
            x += w;
        }
    }
}
/// Pictograph pieces for the briefing lessons.
#[derive(Clone, Copy)]
enum Glyph {
    Crouched,
    Shade,
    Hidden,
    Pass,
    Door,
    Console,
    BeamOff,
    BeamOn,
    Bell,
    Cabinet,
    Coin,
    Guard,
    Records(u8),
    Exit,
    Plus,
    Equals,
    Arrow,
}
impl Glyph {
    fn width(self) -> f32 {
        match self {
            Glyph::Plus | Glyph::Equals | Glyph::Arrow => 36.,
            Glyph::Records(n) => 24. * f32::from(n) + 16.,
            Glyph::BeamOff | Glyph::BeamOn => 64.,
            _ => 56.,
        }
    }
    fn draw(self, c: Vec2, t: f32) {
        let ink = with_alpha(MUTED, 0.85);
        match self {
            Glyph::Crouched => {
                glow(c.x, c.y + 4., 24., with_alpha(ORCHID, 0.16));
                thief(c + vec2(0., -3.), 1.35, 5., 0., 1.);
            }
            Glyph::Shade => {
                let r = Rect::new(c.x - 21., c.y - 21., 42., 42.);
                rounded(r.x, r.y, r.w, r.h, 5., rgba(0.17, 0.10, 0.23, 1.));
                draw_line(
                    r.x + 6.,
                    r.y + r.h - 7.,
                    r.x + r.w - 7.,
                    r.y + 6.,
                    1.5,
                    rgba(0.32, 0.2, 0.42, 1.),
                );
                outline(r.x, r.y, r.w, r.h, 5., 1., with_alpha(ORCHID, 0.45));
            }
            Glyph::Hidden => {
                glow(
                    c.x,
                    c.y,
                    28.,
                    with_alpha(ORCHID, 0.14 + 0.06 * pulse(t, 0.6)),
                );
                icon(Icon::EyeOff, c.x, c.y, 40., ORCHID);
            }
            Glyph::Pass => {
                glow(c.x, c.y, 24., with_alpha(GOLD, 0.16));
                key(c.x, c.y, 2., GOLD, rgba(0.1, 0.07, 0.13, 1.));
            }
            Glyph::Door => door(c, 1.2, false, pulse(t, 0.8)),
            Glyph::Console => console(c, 1.35, true, (t * 1.5).fract() < 0.5),
            Glyph::BeamOff => {
                beam(c, 54., true, 0.55);
                cross(c, 9., rgba(0.08, 0.04, 0.06, 0.9));
                cross(c, 7.5, PAPER);
            }
            Glyph::BeamOn => beam(c, 54., true, 0.75 + 0.25 * (t * 30.).sin()),
            Glyph::Bell => {
                glow(c.x, c.y, 26., with_alpha(RED, 0.18));
                icon(Icon::Bell, c.x + (t * 30.).sin() * 1.2, c.y, 38., RED);
            }
            Glyph::Cabinet => cabinet(c, 1.35, Some(1.)),
            Glyph::Coin => coin(c.x, c.y, 12., 1.),
            Glyph::Guard => {
                guard_head(c + vec2(0., 7.), 1.6, (-1., 0.), 1.);
                bubble(c + vec2(15., -17.), "?", AMBER, 1.);
            }
            Glyph::Records(n) => {
                let left = c.x - 24. * f32::from(n) * 0.5 + 12.;
                for i in 0..n {
                    record(left + f32::from(i) * 24., c.y, 1.35, 1.);
                }
            }
            Glyph::Exit => exit_sign(c, 38., 40., true, t),
            Glyph::Plus => {
                shapes::fill_capsule(c - vec2(8., 0.), c + vec2(8., 0.), 1.8, ink);
                shapes::fill_capsule(c - vec2(0., 8.), c + vec2(0., 8.), 1.8, ink);
            }
            Glyph::Equals => {
                shapes::fill_capsule(c + vec2(-8., -4.5), c + vec2(8., -4.5), 1.8, ink);
                shapes::fill_capsule(c + vec2(-8., 4.5), c + vec2(8., 4.5), 1.8, ink);
            }
            Glyph::Arrow => {
                shapes::fill_capsule(c - vec2(9., 0.), c + vec2(7., 0.), 1.8, ink);
                chevron(c + vec2(5., 0.), 7., ink);
            }
        }
    }
}
/// The sprite of an interactive tile `kind`, scaled by `s`.
pub(crate) fn object(kind: u8, c: Vec2, s: f32, t: f32) {
    match kind {
        b'K' => key(c.x, c.y, 1.3 * s, GOLD, rgba(0.1, 0.07, 0.13, 1.)),
        b'=' => door(c, s, false, pulse(t, 1.)),
        b'T' => console(c, s, true, (t * 1.5).fract() < 0.5),
        b'L' => record(c.x, c.y, 1.25 * s, 1.),
        b'$' => painting(c.x, c.y, 28. * s, 22. * s, 1.),
        _ => exit_sign(c, 30. * s, 32. * s, true, t),
    }
}
