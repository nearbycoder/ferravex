//! Presentation-only juice for accepted match moments: callout banners, kill
//! feed, radio banter, score pops, hit-marker emphasis, a damage camera kick,
//! a celebratory edge glow and the practice scoreline.
//!
//! Everything is created from accepted authority/replay/network events (via
//! [`crate::scoreboard::Tracker`]) and aged by presentation time that stops
//! while offline practice is paused. Nothing here can change movement, shots,
//! damage, timing, replay streams or network messages.
//!
//! Reduced motion removes the camera kick, rising/bouncing text and the duck's
//! flight. Reduced flash removes the edge glow. Text stays: it is information.
use crate::crew::{self, Who};
use crate::geometry::rgb;
use crate::scoreboard::{Moment, Tracker};
use crate::style::{self, BODY, DISPLAY, LIME, SMALL, TITLE, fade};
use ferravex_play::feedback::FeedbackPolicy;
use ferravex_play::rounds::{Phase, Team};
use ferravex_play::tween::Ease;
use ferravex_render::audio::{Cue, GameAudio, Surface};
use ferravex_render::post;
use ferravex_render::prelude::*;
use ferravex_render::shapes::{self, Fill};
use ferravex_render::text::TextAlign;
use std::fmt::Write as _;

pub(super) const INK: Color = style::INK;
pub(super) const MUTED: Color = style::MUTED;
pub(super) const GOLD: Color = style::AMBER;
pub(super) const RED: Color = style::SIGNAL;
pub(super) const TEAL: Color = style::TEAL;
const FEED: usize = 4;
const POPS: usize = 2;
const FEED_LIFE: f32 = 5.;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Marker {
    Hit,
    Headshot,
    Kill,
}

#[derive(Clone, Debug)]
struct Callout {
    head: &'static str,
    sub: String,
    color: Color,
    age: f32,
    life: f32,
    priority: u8,
}

#[derive(Clone, Copy, Debug)]
struct Subtitle {
    speaker: Who,
    line: &'static str,
    age: f32,
}

#[derive(Clone, Copy, Debug)]
struct FeedEntry {
    attacker: u8,
    victim: u8,
    headshot: bool,
    age: f32,
}

#[derive(Clone, Copy, Debug)]
struct Pop {
    text: &'static str,
    color: Color,
    size: f32,
    age: f32,
}

/// Who fragged you last, for the elimination panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Death {
    pub killer: Who,
    pub gloat: Option<&'static str>,
    pub retry: &'static str,
}

pub(super) struct Juice {
    callout: Option<Callout>,
    subtitle: Option<Subtitle>,
    feed: [Option<FeedEntry>; FEED],
    pops: [Option<Pop>; POPS],
    pub marker: Marker,
    pub death: Option<Death>,
    trauma: f32,
    time: f32,
    glow: f32,
    duck_flight: f32,
    motion: f32,
    flash: f32,
    round: Option<(u32, Phase)>,
    clutch_round: Option<u32>,
}

impl Juice {
    pub fn new() -> Self {
        Self {
            callout: None,
            subtitle: None,
            feed: [None; FEED],
            pops: [None; POPS],
            marker: Marker::Hit,
            death: None,
            trauma: 0.,
            time: 0.,
            glow: 0.,
            duck_flight: 0.,
            motion: 1.,
            flash: 1.,
            round: None,
            clutch_round: None,
        }
    }

    /// Restart/replay cuts discard every effect but keep live preferences.
    pub fn reset(&mut self) {
        let policy = (self.motion, self.flash);
        *self = Self::new();
        (self.motion, self.flash) = policy;
    }

    /// Disabled channels are cleared, so re-enabling cannot revive them.
    pub fn apply_policy(&mut self, policy: FeedbackPolicy) {
        self.motion = if policy.reduced_motion() {
            0.
        } else {
            policy.intensity()
        };
        self.flash = if policy.reduced_flash() {
            0.
        } else {
            policy.intensity()
        };
        if self.motion == 0. {
            self.trauma = 0.;
        }
        if self.flash == 0. {
            self.glow = 0.;
        }
    }

    pub fn update(&mut self, dt: f32) {
        if dt <= 0. {
            return;
        }
        self.time += dt;
        self.trauma = (self.trauma - dt * 2.4).max(0.);
        self.glow = (self.glow - dt * 1.6).max(0.);
        self.duck_flight += dt;
        if let Some(c) = &mut self.callout {
            c.age += dt;
            if c.age >= c.life {
                self.callout = None;
            }
        }
        if let Some(s) = &mut self.subtitle {
            s.age += dt;
            if s.age >= 2.6 {
                self.subtitle = None;
            }
        }
        for slot in &mut self.feed {
            if let Some(f) = slot {
                f.age += dt;
                if f.age >= FEED_LIFE {
                    *slot = None;
                }
            }
        }
        for slot in &mut self.pops {
            if let Some(p) = slot {
                p.age += dt;
                if p.age >= 0.9 {
                    *slot = None;
                }
            }
        }
    }

    pub fn has_callout(&self) -> bool {
        self.callout.is_some()
    }

    /// Camera-only yaw/pitch kick in radians; aim and commands are untouched.
    pub fn kick(&self) -> (f32, f32) {
        let amount = self.trauma * self.trauma * 0.022 * self.motion;
        (
            (self.time * 47.).sin() * amount,
            (self.time * 39. + 1.1).cos() * amount * 0.8,
        )
    }

    /// An accepted local hit on someone else.
    pub fn hit(&mut self, headshot: bool, sound: &mut GameAudio) {
        if headshot {
            self.marker = Marker::Headshot;
            sound.cue(Cue::Impact(Surface::Metal), 0.55);
            sound.cue(Cue::Click, 0.5);
        } else {
            self.marker = Marker::Hit;
            sound.cue(Cue::Impact(Surface::Metal), 0.3);
        }
    }

    /// An accepted hit on you.
    pub fn hurt(&mut self, damage: u8) {
        if self.motion > 0. {
            self.trauma = (self.trauma + 0.35 + f32::from(damage) / 120.).min(1.);
        }
    }

    fn callout(
        &mut self,
        priority: u8,
        head: &'static str,
        color: Color,
        sub: impl FnOnce(&mut String),
    ) {
        if self
            .callout
            .as_ref()
            .is_some_and(|c| c.priority > priority && c.age < c.life * 0.6)
        {
            return;
        }
        let mut text = self.callout.take().map(|c| c.sub).unwrap_or_default();
        text.clear();
        sub(&mut text);
        self.callout = Some(Callout {
            head,
            sub: text,
            color,
            age: 0.,
            life: 2.2,
            priority,
        });
    }

    fn pop(&mut self, text: &'static str, color: Color, size: f32) {
        let slot = self
            .pops
            .iter()
            .position(Option::is_none)
            .unwrap_or_else(|| {
                (0..POPS)
                    .max_by(|&a, &b| {
                        let age = |i: usize| self.pops[i].map_or(0., |p| p.age);
                        age(a).total_cmp(&age(b))
                    })
                    .unwrap_or(0)
            });
        self.pops[slot] = Some(Pop {
            text,
            color,
            size,
            age: 0.,
        });
    }

    /// Present one accepted moment. `bots` names the practice crew.
    pub fn moment(&mut self, moment: Moment, local: u8, bots: bool, sound: &mut GameAudio) {
        let who = |id| Who { id, local, bots };
        match moment {
            Moment::Feed {
                attacker,
                victim,
                headshot,
            } => {
                self.feed.rotate_right(1);
                self.feed[0] = Some(FeedEntry {
                    attacker,
                    victim,
                    headshot,
                    age: 0.,
                });
            }
            Moment::Kill {
                victim,
                headshot,
                nth,
                first_blood,
                revenge,
            } => {
                self.marker = Marker::Kill;
                sound.cue(Cue::Pickup, 0.5);
                if headshot {
                    self.pop("+1 HEADSHOT", RED, 18.);
                } else {
                    self.pop("+1", GOLD, 22.);
                }
                if bots && let Some(line) = crew::last_words(victim, nth + u32::from(victim)) {
                    self.subtitle = Some(Subtitle {
                        speaker: who(victim),
                        line,
                        age: 0.,
                    });
                }
                if revenge {
                    self.callout(2, "REVENGE", RED, |s| {
                        let _ = write!(s, "PAYBACK DELIVERED TO {}", who(victim));
                    });
                } else if first_blood {
                    self.callout(1, "FIRST BLOOD", GOLD, |s| {
                        s.push_str("THE CREW HAS BEEN NOTIFIED");
                    });
                }
            }
            Moment::Multi(n) => {
                if let Some(head) = crew::multi_kill(n) {
                    sound.cue(Cue::Success, 0.55);
                    self.glow = self.flash;
                    self.callout(3, head, GOLD, |s| {
                        let _ = write!(s, "{n} FRAGS IN QUICK SUCCESSION");
                    });
                }
            }
            Moment::Streak(n) => {
                if let Some((head, sub)) = crew::streak(n) {
                    sound.cue(Cue::Success, 0.45);
                    self.glow = self.glow.max(self.flash * 0.7);
                    self.callout(2, head, TEAL, |s| s.push_str(sub));
                }
            }
            Moment::Died {
                killer,
                nth,
                deaths,
            } => {
                self.trauma = if self.motion > 0. { 1. } else { 0. };
                self.subtitle = None;
                self.death = Some(Death {
                    killer: who(killer),
                    gloat: if bots { crew::gloat(killer, nth) } else { None },
                    retry: crew::retry(deaths),
                });
            }
            Moment::Oops { attacker, nth } => {
                if bots {
                    self.subtitle = Some(Subtitle {
                        speaker: who(attacker),
                        line: crew::oops(nth),
                        age: 0.,
                    });
                }
            }
            Moment::Respawned => {
                self.death = None;
                self.trauma = 0.;
            }
            Moment::Duck => {
                sound.cue(Cue::Pickup, 0.7);
                sound.cue(Cue::Success, 0.6);
                self.duck_flight = 0.;
                self.glow = self.flash;
                self.callout(4, "QUACK!", GOLD, |s| {
                    s.push_str("SECRET FOUND: THE DOCK DUCK");
                });
            }
            Moment::Clock(30) => {
                sound.cue(Cue::Click, 0.6);
                self.callout(1, "30 SECONDS LEFT", GOLD, |s| {
                    s.push_str("MAKE THEM COUNT")
                });
            }
            Moment::Clock(_) => sound.cue(Cue::Click, 0.45),
        }
    }

    /// Round transitions and 1vX clutches from the replicated tactical view.
    pub fn tactical(
        &mut self,
        view: Option<&ferravex_fps::tactical::View>,
        local: u8,
        sound: &mut GameAudio,
    ) {
        let Some(view) = view else {
            self.round = None;
            return;
        };
        let previous = self.round.replace((view.round, view.phase));
        let changed = previous != Some((view.round, view.phase));
        let me = view.player(local);
        if changed && previous.is_some() {
            match view.phase {
                Phase::Live => {
                    sound.cue(Cue::Click, 0.6);
                    let round = view.round;
                    self.callout(1, "LIVE!", GOLD, |s| {
                        let _ = write!(s, "ROUND {round} · GOOD LUCK OUT THERE");
                    });
                }
                Phase::Planted => {
                    let attack = me.is_some_and(|p| p.team == Team::Attack);
                    self.callout(2, "BOMB PLANTED", RED, |s| {
                        s.push_str(if attack {
                            "HOLD THE SITE"
                        } else {
                            "GO DEFUSE IT. NO PRESSURE."
                        });
                    });
                }
                _ => {}
            }
        }
        let Some(me) = me.filter(|p| p.health > 0) else {
            return;
        };
        if !matches!(view.phase, Phase::Live | Phase::Planted)
            || self.clutch_round == Some(view.round)
        {
            return;
        }
        let alive = |team: bool| {
            view.players
                .iter()
                .filter(|p| p.id != local && p.health > 0 && (p.team == me.team) == team)
                .count()
        };
        let (friends, enemies) = (alive(true), alive(false));
        let total_friends = view
            .players
            .iter()
            .filter(|p| p.id != local && p.team == me.team)
            .count();
        if friends == 0 && total_friends > 0 && enemies >= 1 {
            self.clutch_round = Some(view.round);
            sound.cue(Cue::Success, 0.4);
            self.callout(3, "CLUTCH TIME", TEAL, |s| {
                let _ = write!(s, "1V{enemies} · YOUR TEAM IS WATCHING");
            });
        }
    }

    /// The duck's transform while it is still visible: resting, then (with
    /// motion) a short flight off the container after it is found.
    pub fn duck(&self, found: bool) -> Option<(Vec3, f32, f32)> {
        let base = crate::scoreboard::DUCK - Vec3::Y * 0.23;
        if !found {
            return Some((base, 0.6, 1.3));
        }
        let t = self.duck_flight;
        if self.motion == 0. || t > 1.4 {
            return None;
        }
        let rise = vec3(t * 2.2, t * 4.5 - t * t * 1.2, t * 1.4);
        Some((base + rise, 0.6 + t * 11., 1.3 * (1. - t / 1.4).max(0.1)))
    }

    /// Amber edge glow for multi-kills, streaks and secrets (reduced flash: off).
    pub fn draw_glow(&self) {
        if self.glow <= 0. || self.flash == 0. {
            return;
        }
        let (w, h) = (screen_width(), screen_height());
        let a = self.glow * 0.5 * self.flash.min(1.);
        let edge = fade(GOLD, a);
        let clear = fade(GOLD, 0.);
        let band = 46.;
        for (rect, fill) in [
            (
                Rect::new(0., 0., w, band),
                Fill::vertical(Rect::new(0., 0., w, band), edge, clear),
            ),
            (
                Rect::new(0., h - band, w, band),
                Fill::vertical(Rect::new(0., h - band, w, band), clear, edge),
            ),
            (
                Rect::new(0., 0., band, h),
                Fill::horizontal(Rect::new(0., 0., band, h), edge, clear),
            ),
            (
                Rect::new(w - band, 0., band, h),
                Fill::horizontal(Rect::new(w - band, 0., band, h), clear, edge),
            ),
        ] {
            post::with_glow(|| shapes::fill_rounded_rect(rect, 0., fill));
        }
    }

    /// Rising score pops beside the crosshair, newest at the bottom.
    pub fn draw_pops(&self) {
        let (cx, cy) = (screen_width() * 0.5, screen_height() * 0.5);
        for p in self.pops.iter().flatten() {
            let rank = self.pops.iter().flatten().filter(|o| o.age < p.age).count() as f32;
            let rise = if self.motion > 0. { p.age * 30. } else { 0. };
            let alpha = (1. - (p.age - 0.55).max(0.) / 0.35).clamp(0., 1.);
            let grow = if self.motion > 0. {
                1. + 0.4 * (1. - p.age / 0.16).max(0.).powi(2)
            } else {
                1.
            };
            let style = style::classic(p.size * grow, fade(p.color, alpha));
            let width = style::width(p.text, style);
            style::line(
                p.text,
                (cx + 26.).min(screen_width() - 8. - width),
                cy - 12. - rank * 26. - rise.min(18.),
                style,
                TextAlign::Left,
            );
        }
    }

    /// Callout banner and radio subtitle. `top` is the first free row below
    /// the scoreline/objective strip; `bottom` the last free row above
    /// footers, vitals and touch controls.
    pub fn draw_banners(&self, top: f32, bottom: f32) {
        let top = top.max(crate::hud::MENU_CORNER[1]);
        let w = screen_width();
        let h = screen_height();
        if let Some(c) = &self.callout {
            let alpha = ((c.life - c.age) / 0.35).clamp(0., 1.);
            // Stamp in with a little overshoot, then hold.
            let enter = if self.motion > 0. {
                Ease::BackOut.apply((c.age / 0.22).min(1.))
            } else {
                1.
            };
            let base = style::fit(c.head, style::classic(DISPLAY, c.color), w - 64., 16.);
            let size = base.size * (0.7 + 0.3 * enter);
            let head = base.with_size(size).with_color(fade(c.color, alpha));
            let sub = style::fit(&c.sub, style::modern(BODY, INK), w - 48., 12.);
            let y = (h * 0.22).max(top + base.size + 14.);
            let band =
                (style::width(c.head, base).max(style::width(&c.sub, sub)) + 56.).min(w - 16.);
            let rect = Rect::new((w - band) * 0.5, y - base.size - 12., band, base.size + 40.);
            style::plate(rect, 8., style::PLATE, alpha);
            // Accent rules sweep out from the center.
            let sweep = if self.motion > 0. {
                Ease::CubicOut.apply((c.age / 0.3).min(1.))
            } else {
                1.
            };
            let rule = (band - 24.) * 0.5 * sweep;
            shapes::fill_rounded_rect(
                Rect::new(w * 0.5 - rule, rect.y + rect.h - 3., rule * 2., 2.),
                1.,
                fade(c.color, alpha * 0.9),
            );
            if self.flash > 0. {
                post::with_glow(|| {
                    shapes::glow_disc(
                        vec2(w * 0.5, y - base.size * 0.45),
                        band * 0.35,
                        fade(c.color, 0.10 * alpha * self.flash.min(1.)),
                    )
                });
            }
            style::line(c.head, w * 0.5, y, head, TextAlign::Center);
            style::line(
                &c.sub,
                w * 0.5,
                y + 22.,
                sub.with_color(fade(INK, alpha)),
                TextAlign::Center,
            );
        }
        if let Some(s) = &self.subtitle {
            let alpha = ((2.6 - s.age) / 0.4).clamp(0., 1.);
            let rise = if self.motion > 0. {
                8. * (1. - Ease::CubicOut.apply((s.age / 0.25).min(1.)))
            } else {
                0.
            };
            let y = (h * 0.5 + 92.).min(bottom - 6.).max(h * 0.5 + 52.) + rise;
            let name = style::Buf::<40>::fmt(format_args!("{}", s.speaker));
            let quote = style::Buf::<80>::fmt(format_args!("“{}”", s.line));
            let speaker = style::modern(SMALL, fade(GOLD, alpha));
            let words = style::modern(BODY, fade(INK, alpha));
            let sw = style::width(name.as_str(), speaker);
            let qw = style::width(quote.as_str(), words);
            let row = sw + 12. + qw;
            if row + 40. <= w {
                let left = (w - row) * 0.5;
                let rect = Rect::new(left - 14., y - 18., row + 28., 26.);
                style::plate(rect, 6., style::PLATE, alpha);
                style::accent(rect, TEAL, alpha);
                style::line(name.as_str(), left, y - 1., speaker, TextAlign::Left);
                style::line(quote.as_str(), left + sw + 12., y, words, TextAlign::Left);
            } else {
                let words = style::fit(quote.as_str(), words, w - 40., 12.);
                let rect = Rect::new(12., y - 36., w - 24., 46.);
                style::plate(rect, 6., style::PLATE, alpha);
                style::accent(rect, TEAL, alpha);
                style::line(name.as_str(), w * 0.5, y - 20., speaker, TextAlign::Center);
                style::line(quote.as_str(), w * 0.5, y, words, TextAlign::Center);
            }
        }
    }

    /// Recent eliminations, newest first. Returns the bottom of the feed.
    pub fn draw_feed(&self, local: u8, bots: bool, x_right: f32, top: f32, max_rows: usize) -> f32 {
        let mut y = top.max(crate::hud::MENU_CORNER[1] + 4.);
        for (row, entry) in self.feed.iter().flatten().take(max_rows).enumerate() {
            let alpha = ((FEED_LIFE - entry.age) / 0.5).clamp(0., 1.);
            // Newest rows slide in from the right.
            let slide = if self.motion > 0. && row == 0 {
                24. * (1. - Ease::CubicOut.apply((entry.age / 0.2).min(1.)))
            } else {
                0.
            };
            let who = |id| Who { id, local, bots };
            let attacker = style::Buf::<40>::fmt(format_args!("{}", who(entry.attacker)));
            let victim = style::Buf::<40>::fmt(format_args!("{}", who(entry.victim)));
            let tint = |id: u8| if id == local { GOLD } else { INK };
            let a_style = style::modern(SMALL, fade(tint(entry.attacker), alpha));
            let v_style = style::modern(
                SMALL,
                fade(
                    if entry.victim == local {
                        RED
                    } else {
                        tint(entry.victim)
                    },
                    alpha,
                ),
            );
            let a = style::width(attacker.as_str(), a_style);
            let v = style::width(victim.as_str(), v_style);
            let mid = if entry.headshot { 30. } else { 22. };
            let width = a + mid + v + 20.;
            let left = (x_right - width).max(8.) + slide;
            let rect = Rect::new(left, y, width, 22.);
            let mine = entry.attacker == local;
            style::plate(
                rect,
                5.,
                if mine {
                    Color::new(0.16, 0.12, 0.04, 0.8)
                } else {
                    style::PLATE
                },
                alpha,
            );
            if mine || entry.victim == local {
                style::accent(rect, if mine { GOLD } else { RED }, alpha);
            }
            let baseline = y + 15.;
            style::line(
                attacker.as_str(),
                left + 10.,
                baseline,
                a_style,
                TextAlign::Left,
            );
            let arrow_x = left + 10. + a + 5.;
            style::line(
                "→",
                arrow_x,
                baseline,
                style::modern(SMALL, fade(MUTED, alpha)),
                TextAlign::Left,
            );
            if entry.headshot {
                style::headshot_icon(vec2(arrow_x + 19., y + 11.), 4.5, fade(RED, alpha));
            }
            style::line(
                victim.as_str(),
                arrow_x + mid - 5.,
                baseline,
                v_style,
                TextAlign::Left,
            );
            y += 26.;
        }
        y
    }

    /// The whole in-play overlay owned by juice, laid out top to bottom:
    /// error row, scoreline/objective strip, kill feed, callout banner, radio
    /// subtitle, onboarding hint and score pops. The fullscreen menu corner,
    /// vitals, replay footer and touch controls stay clear.
    pub fn draw_play(&self, board: &Tracker, l: PlayLayout<'_>) {
        let (w, h) = (screen_width(), screen_height());
        let mut top: f32 = if l.notice { 44. } else { 12. };
        if l.touch {
            top = top.max(if l.round { 124. } else { 54. });
        }
        if l.round {
            top = top.max(95.);
        }
        // Right end of whatever strip sits at the top center.
        let strip_right = if l.practice {
            let (left, width, _) = scoreline_rect(w, top);
            top = self.draw_scoreline(board, top) + 8.;
            left + width
        } else if l.round {
            let (left, width) = crate::hud::tactical_strip(w);
            left + width
        } else {
            w * 0.5
        };
        if w - 16. - strip_right - 24. >= 250. && !l.touch {
            // Top-right, below the fullscreen menu corner.
            self.draw_feed(l.local, l.bots, w - 16., 64., 4);
        } else if !self.has_callout() {
            // Narrow views stack the feed under the strip; a callout banner
            // borrows those rows while it is showing.
            let rows = if h < 600. { 2 } else { 3 };
            top = self.draw_feed(l.local, l.bots, w - 12., top, rows) + 4.;
        }
        let anchor = if l.touch {
            top.max(crate::hud::MENU_CORNER[1]) + 24.
        } else if l.footer {
            h - 164.
        } else {
            h - 86.
        };
        if let Some(hint) = l.hint {
            draw_hint(hint, anchor, self.time);
        }
        let bottom = if l.touch {
            h - 236.
        } else if l.hint.is_some() {
            anchor - 30.
        } else {
            anchor + 8.
        };
        if !l.roster {
            let banner_top = if l.hint.is_some() && l.touch {
                anchor + 12.
            } else {
                top
            };
            self.draw_banners(banner_top, bottom);
            if l.alive {
                self.draw_pops();
            }
        }
    }

    /// Practice scoreline: YOU frags, the authoritative clock, CREW frags.
    /// Returns the bottom of the strip.
    pub fn draw_scoreline(&self, tracker: &Tracker, top: f32) -> f32 {
        let w = screen_width();
        let Some(seconds) = tracker.remaining_seconds() else {
            return top;
        };
        let (left, width, top) = scoreline_rect(w, top);
        let rect = Rect::new(left, top, width, 48.);
        style::plate(rect, 8., style::PLATE, 1.);
        let you = tracker.lines[usize::from(tracker.local())].kills;
        let crew: u32 = tracker
            .lines
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != usize::from(tracker.local()))
            .map(|(_, l)| l.kills)
            .sum();
        let urgent = seconds <= 10;
        let pulse = if urgent && self.motion > 0. {
            (self.time * 6.).sin().max(0.)
        } else {
            0.
        };
        if urgent && self.flash > 0. {
            post::with_glow(|| {
                shapes::glow_disc(
                    vec2(w * 0.5, top + 24.),
                    46. + 10. * pulse,
                    fade(RED, 0.16 * self.flash.min(1.)),
                )
            });
        }
        style::line_fmt(
            w * 0.5,
            top + 36.,
            style::classic(
                DISPLAY * (1. + 0.08 * pulse),
                if urgent { RED } else { INK },
            ),
            TextAlign::Center,
            format_args!("{}:{:02}", seconds / 60, seconds % 60),
        );
        // Big numbers on the outside, small labels toward the clock.
        let number = style::classic(TITLE, GOLD);
        let n = style::line_fmt(
            left + 14.,
            top + 34.,
            number,
            TextAlign::Left,
            format_args!("{you}"),
        );
        style::line(
            "You",
            n.x + n.w + 6.,
            top + 33.,
            style::modern(SMALL, MUTED),
            TextAlign::Left,
        );
        let c = style::line_fmt(
            left + width - 14.,
            top + 34.,
            number.with_color(TEAL),
            TextAlign::Right,
            format_args!("{crew}"),
        );
        style::line(
            "Crew",
            c.x - 6.,
            top + 33.,
            style::modern(SMALL, MUTED),
            TextAlign::Right,
        );
        let mut bottom = top + 48.;
        if tracker.streak >= 2 {
            let chip = Rect::new(w * 0.5 - 56., bottom + 5., 112., 22.);
            style::plate(chip, 11., Color::new(0.14, 0.17, 0.05, 0.85), 1.);
            style::line_fmt(
                w * 0.5,
                bottom + 21.,
                style::modern(SMALL, LIME),
                TextAlign::Center,
                format_args!("▲ Streak {}", tracker.streak),
            );
            bottom += 27.;
        }
        bottom
    }
}

/// Which first-match hint to show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum HintKind {
    FireReload,
    Scoreboard,
}

/// A hint and the player's current key labels for it.
#[derive(Clone, Copy, Debug)]
pub(super) struct Hint<'a> {
    pub kind: HintKind,
    pub keys: [&'a str; 2],
}

/// What the in-play overlay needs to know about the frame.
#[derive(Clone, Copy, Debug)]
pub(super) struct PlayLayout<'a> {
    pub practice: bool,
    pub round: bool,
    pub touch: bool,
    /// An error/command notice occupies the first row.
    pub notice: bool,
    pub footer: bool,
    pub roster: bool,
    pub alive: bool,
    pub hint: Option<Hint<'a>>,
    pub local: u8,
    pub bots: bool,
}

/// First-match hints that disappear as soon as each action is used.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Onboarding {
    pub fired: bool,
    pub roster: bool,
}

impl Onboarding {
    /// The single most useful unlearned hint, if any.
    pub fn hint(&self, frags: u32) -> Option<HintKind> {
        if !self.fired {
            Some(HintKind::FireReload)
        } else if !self.roster && frags >= 1 {
            Some(HintKind::Scoreboard)
        } else {
            None
        }
    }
}

/// A small key cap; returns its width.
fn keycap(x: f32, y: f32, label: &str, alpha: f32) -> f32 {
    let style = style::modern(SMALL, fade(INK, alpha));
    let w = (style::width(label, style) + 14.).max(22.);
    let lip = Rect::new(x, y + 2., w, 20.);
    let cap = Rect::new(x, y, w, 20.);
    shapes::fill_rounded_rect(lip, 5., fade(Color::new(0.01, 0.015, 0.015, 0.9), alpha));
    shapes::fill_rounded_rect(
        cap,
        5.,
        Fill::vertical(
            cap,
            fade(Color::from_hex(0x3a4744), alpha),
            fade(Color::from_hex(0x27312f), alpha),
        ),
    );
    shapes::stroke_rounded_rect(cap, 5., 1., fade(Color::new(1., 1., 1., 0.16), alpha));
    style::line(label, x + w * 0.5, y + 14., style, TextAlign::Center);
    w
}

fn keycap_width(label: &str) -> f32 {
    (style::width(label, style::modern(SMALL, INK)) + 14.).max(22.)
}

/// One centered hint pill made of key caps and short actions.
pub(super) fn draw_hint(hint: Hint<'_>, y: f32, time: f32) {
    let w = screen_width();
    let (parts, n): ([(&str, &str); 2], usize) = match hint.kind {
        HintKind::FireReload => ([(hint.keys[0], "Fire"), (hint.keys[1], "Reload")], 2),
        HintKind::Scoreboard => ([(hint.keys[0], "Hold for scoreboard"), ("", "")], 1),
    };
    let label = style::modern(SMALL, MUTED);
    let gap = 16.;
    let total: f32 = parts[..n]
        .iter()
        .map(|(key, action)| keycap_width(key) + 6. + style::width(action, label))
        .sum::<f32>()
        + gap * (n as f32 - 1.);
    let scale = ((w - 40.) / (total + 28.)).min(1.);
    if scale < 0.75 {
        return;
    }
    let breathe = 0.85 + 0.15 * (time * 2.4).sin().abs();
    let rect = Rect::new((w - total - 28.) * 0.5, y - 19., total + 28., 30.);
    style::plate(rect, 8., style::PLATE, 1.);
    let mut x = rect.x + 14.;
    for (key, action) in &parts[..n] {
        x += keycap(x, y - 14., key, 1.) + 6.;
        let r = style::line(
            action,
            x,
            y + 0.5,
            label.with_color(fade(INK, breathe)),
            TextAlign::Left,
        );
        x += r.w + gap;
    }
}

/// The practice scoreline: centered, and below the fullscreen menu corner
/// when a narrow view would put its right end under it. (left, width, top)
pub(super) fn scoreline_rect(w: f32, top: f32) -> (f32, f32, f32) {
    let width = (w - 24.).min(320.);
    let left = (w - width) * 0.5;
    let corner = crate::hud::MENU_CORNER;
    let top = if left + width > w - corner[0] {
        top.max(corner[1] + 4.)
    } else {
        top
    };
    (left, width, top)
}

/// Flat duck colors, shared by the model builder.
pub(super) fn duck_colors() -> [Color; 3] {
    [rgb(0xf2c84b), rgb(0xe8873a), rgb(0x1f2421)]
}

/// Legacy fixed-cell metrics retained for the results layout contract.
pub(super) fn measure(text: &str, size: f32) -> f32 {
    text.chars().count() as f32 * 6. * size / 8.
}
pub(super) fn fit_size(text: &str, size: f32, width: f32) -> f32 {
    let natural = measure(text, size);
    if natural <= width {
        size
    } else {
        (size * width / natural.max(1.)).max(12.)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moments_rank_callouts_and_reduced_motion_clears_kick_and_flight() {
        let mut sound = GameAudio::new();
        sound.pause();
        let mut j = Juice::new();
        j.apply_policy(FeedbackPolicy::default());
        j.moment(Moment::Multi(3), 0, true, &mut sound);
        j.moment(Moment::Streak(3), 0, true, &mut sound);
        assert_eq!(
            j.callout.as_ref().unwrap().head,
            "TRIPLE THREAT",
            "lower priority waits"
        );
        j.update(2.);
        j.moment(Moment::Streak(3), 0, true, &mut sound);
        assert_eq!(j.callout.as_ref().unwrap().head, "HEATING UP");
        j.moment(Moment::Duck, 0, true, &mut sound);
        assert_eq!(j.callout.as_ref().unwrap().head, "QUACK!");
        j.hurt(30);
        j.update(0.01);
        assert_ne!(j.kick(), (0., 0.));
        assert!(j.duck(true).is_some(), "the duck flies away");
        j.apply_policy(FeedbackPolicy::new(true, true, 1.).unwrap());
        assert_eq!(j.kick(), (0., 0.));
        assert_eq!(j.duck(true), None, "no flight under reduced motion");
        assert!(j.duck(false).is_some());
        assert_eq!(j.glow, 0.);
        j.hurt(30);
        assert_eq!(j.kick(), (0., 0.));
        j.apply_policy(FeedbackPolicy::default());
        assert_eq!(j.kick(), (0., 0.), "re-enabling does not revive the kick");
    }

    #[test]
    fn feed_and_pops_are_bounded_and_deaths_quote_their_killer() {
        let mut sound = GameAudio::new();
        sound.pause();
        let mut j = Juice::new();
        for n in 0..20u8 {
            j.moment(
                Moment::Feed {
                    attacker: 0,
                    victim: 1 + n % 5,
                    headshot: n % 2 == 0,
                },
                0,
                true,
                &mut sound,
            );
            j.moment(
                Moment::Kill {
                    victim: 1 + n % 5,
                    headshot: true,
                    nth: u32::from(n),
                    first_blood: n == 0,
                    revenge: false,
                },
                0,
                true,
                &mut sound,
            );
        }
        assert_eq!(j.feed.iter().flatten().count(), FEED);
        assert_eq!(j.feed[0].unwrap().victim, 1 + 19 % 5, "newest first");
        assert_eq!(j.pops.iter().flatten().count(), POPS);
        assert_eq!(j.marker, Marker::Kill);
        j.moment(
            Moment::Died {
                killer: 2,
                nth: 1,
                deaths: 1,
            },
            0,
            true,
            &mut sound,
        );
        let death = j.death.unwrap();
        assert_eq!(death.killer.to_string(), "FORKLIFT FRAN");
        assert!(death.gloat.is_some());
        j.moment(Moment::Respawned, 0, true, &mut sound);
        assert_eq!(j.death, None);
        j.moment(
            Moment::Died {
                killer: 2,
                nth: 1,
                deaths: 2,
            },
            1,
            false,
            &mut sound,
        );
        assert_eq!(
            j.death.unwrap().gloat,
            None,
            "network opponents never get scripted lines"
        );
        j.update(FEED_LIFE + 1.);
        assert_eq!(j.feed.iter().flatten().count(), 0);
        assert_eq!(j.pops.iter().flatten().count(), 0);
    }

    #[test]
    fn tactical_rounds_announce_live_plant_and_one_clutch_per_round() {
        use ferravex_fps::tactical::{PlayerView, View};
        use ferravex_play::rounds::{Bomb, Weapon};
        let mut sound = GameAudio::new();
        sound.pause();
        let player = |id, team, health| PlayerView {
            id,
            team,
            health,
            money: 800,
            weapon: Weapon::Pistol,
            armor: 0,
            kit: false,
        };
        let mut view = View {
            tick: 1,
            phase: Phase::Buy,
            round: 2,
            remaining_ticks: 640,
            score: [1, 0],
            bomb: Bomb::Carried(0),
            interaction: None,
            players: vec![
                player(0, Team::Attack, 100),
                player(1, Team::Attack, 100),
                player(2, Team::Defend, 100),
                player(3, Team::Defend, 100),
            ],
        };
        let mut j = Juice::new();
        j.tactical(Some(&view), 0, &mut sound);
        assert!(j.callout.is_none(), "joining mid-phase is not a transition");
        view.phase = Phase::Live;
        j.tactical(Some(&view), 0, &mut sound);
        assert_eq!(j.callout.as_ref().unwrap().head, "LIVE!");
        assert_eq!(
            j.callout.as_ref().unwrap().sub,
            "ROUND 2 · GOOD LUCK OUT THERE"
        );
        j.update(3.);
        view.players[1].health = 0;
        j.tactical(Some(&view), 0, &mut sound);
        assert_eq!(j.callout.as_ref().unwrap().head, "CLUTCH TIME");
        assert_eq!(
            j.callout.as_ref().unwrap().sub,
            "1V2 · YOUR TEAM IS WATCHING"
        );
        j.update(3.);
        j.tactical(Some(&view), 0, &mut sound);
        assert!(j.callout.is_none(), "one clutch callout per round");
        view.players[0].health = 0;
        view.round = 3;
        j.tactical(Some(&view), 0, &mut sound);
        assert!(
            j.callout.as_ref().is_none_or(|c| c.head != "CLUTCH TIME"),
            "eliminated players are not clutching"
        );
    }
}
