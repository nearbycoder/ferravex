//! Presentation feedback for accepted commands.
//!
//! [`Show`] records what the latest executed tick accepted (a bounded,
//! per-tick event list) plus a few tick-aged animations: route glides, cannon
//! tracers and hit flashes. Only `Game::step` writes it, always from commands
//! the rules accepted or refused, and no rule ever reads it, so replays and
//! the campaign controller behave exactly as before. `Game::spawn_effects`
//! turns the event list into [`Juice`] effects at the board positions where
//! things happened.
use crate::content::MAX_ENEMIES;
use crate::model::{BOARD_X, BOARD_Y, Game, Pos, TILE_SIZE};
use crate::view::{BLUE, GOLD, GREEN, RED, TEAL, objective_icon, role_color};
use ferravex_arcade::{Color, Cue, Icon, Juice, PAPER, Vec2, lighten, vec2, with_alpha};

/// Most events one tick can produce: eight enemy actions, eight reserve
/// arrivals, three machine hits and the phase change fit with room to spare.
pub(crate) const MAX_EVENTS: usize = 32;
/// Ticks a machine spends crossing one tile.
pub(crate) const STEP_TICKS: u8 = 5;
/// Ticks a squad shot's tracer stays visible.
pub(crate) const TRACER_TICKS: u8 = 16;
/// Ticks the enemy volley's tracers stay visible.
pub(crate) const VOLLEY_TICKS: u8 = 30;
/// Ticks a machine flashes after taking a hit.
pub(crate) const HURT_TICKS: u8 = 16;
/// Ticks the board stays in view after an operation ends, so the final hit
/// lands before the debrief or setback card arrives.
pub(crate) const REVEAL_TICKS: u16 = 30;
/// Ticks the operation card takes to slide in.
pub(crate) const SLIDE_TICKS: u16 = 14;
/// The phase clock stops here, so idle ticks change nothing once settled.
pub(crate) const SETTLED_TICKS: u16 = 60;
/// Ticks the new-turn banner stays up after an enemy phase.
pub(crate) const TURN_TICKS: u16 = 80;

/// Why an order was refused; drawn as the missing thing's icon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// Not enough action points.
    Ap,
    /// No clear route.
    Route,
    /// Target beyond weapon or repair range.
    Range,
    /// Walls block the line of sight.
    Sight,
    /// No repair kits left.
    Kit,
    /// The patient is already at full hull.
    Full,
    /// The skill is already active this turn.
    Ready,
}
impl Refusal {
    /// The icon and one-word popup shown where the order was refused.
    pub(crate) fn badge(self) -> (Icon, &'static str) {
        match self {
            Self::Ap => (Icon::Bolt, "AP"),
            Self::Route => (Icon::Cross, ""),
            Self::Range => (Icon::Target, "Range"),
            Self::Sight => (Icon::EyeOff, ""),
            Self::Kit => (Icon::Wrench, "0"),
            Self::Full => (Icon::Heart, "Full"),
            Self::Ready => (Icon::Check, ""),
        }
    }
}

/// One accepted (or refused) command, recorded where it happened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Event {
    Deployed,
    Selected(u8),
    Moved {
        unit: u8,
        to: Pos,
    },
    Fired {
        from: Pos,
        to: Pos,
        damage: u8,
        disabled: bool,
    },
    Interacted {
        index: u8,
        at: Pos,
    },
    Braced(u8),
    Focused(u8),
    Repaired {
        patient: u8,
        amount: u8,
    },
    EnemyMoved {
        to: Pos,
    },
    Hit {
        at: Pos,
        damage: u8,
        braced: bool,
        disabled: bool,
    },
    Missed {
        at: Pos,
    },
    Guarded {
        at: Pos,
        held: u8,
        needed: u8,
    },
    Contested {
        at: Pos,
    },
    Reinforced {
        at: Pos,
    },
    Turn(u16),
    Refused {
        at: Pos,
        why: Refusal,
    },
    Secured,
    /// The debrief or setback card arrives.
    Revealed,
    Upgraded(u8),
    Retried,
}

/// The events of the latest executed tick, in acceptance order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Events {
    list: [Option<Event>; MAX_EVENTS],
    len: u8,
}
impl Default for Events {
    fn default() -> Self {
        Self {
            list: [None; MAX_EVENTS],
            len: 0,
        }
    }
}
impl Events {
    fn push(&mut self, event: Event) {
        if let Some(slot) = self.list.get_mut(usize::from(self.len)) {
            *slot = Some(event);
            self.len += 1;
        }
    }
    fn clear(&mut self) {
        if self.len > 0 {
            self.list[..usize::from(self.len)].fill(None);
            self.len = 0;
        }
    }
    pub(crate) fn iter(&self) -> impl Iterator<Item = Event> + '_ {
        self.list[..usize::from(self.len)].iter().flatten().copied()
    }
}

/// A machine sliding along the route it was ordered to take.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Glide {
    route: [Pos; 8],
    len: u8,
    age: u8,
}
impl Glide {
    /// Glide along `path` (start first); longer routes keep their last tiles.
    pub(crate) fn along(path: &[Pos]) -> Self {
        let tail = &path[path.len().saturating_sub(8)..];
        let mut route = [Pos::default(); 8];
        route[..tail.len()].copy_from_slice(tail);
        Self {
            route,
            len: tail.len() as u8,
            age: 0,
        }
    }
    fn tick(&mut self) {
        if self.len >= 2 {
            self.age += 1;
            if self.age >= (self.len - 1) * STEP_TICKS {
                *self = Self::default();
            }
        }
    }
    /// Current canvas position and heading, or `None` once arrived.
    pub(crate) fn at(&self) -> Option<(Vec2, Vec2)> {
        if self.len < 2 {
            return None;
        }
        let travelled = f32::from(self.age) / f32::from(STEP_TICKS);
        let i = (travelled as usize).min(usize::from(self.len) - 2);
        let t = (travelled - i as f32).clamp(0., 1.);
        let (a, b) = (self.route[i].center(), self.route[i + 1].center());
        Some((a + (b - a) * t, (b - a).normalize_or(Vec2::X)))
    }
}

/// Presentation-only state advanced by executed ticks. Rules never read it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Show {
    pub events: Events,
    /// Cannon tracers `(from, to, hostile)` that are still fading.
    pub tracers: [Option<(Pos, Pos, bool)>; MAX_ENEMIES],
    /// Remaining tracer ticks.
    pub flash: u8,
    /// Ticks the tracers started with, for fading.
    pub flash_from: u8,
    pub glides: [Glide; 3],
    pub enemy_glides: [Glide; MAX_ENEMIES],
    pub hurt: [u8; 3],
    pub enemy_hurt: [u8; MAX_ENEMIES],
    /// Ticks since the phase last changed, stopping at [`SETTLED_TICKS`].
    pub phase_age: u16,
    /// Ticks since the latest new turn, stopping at [`TURN_TICKS`].
    pub turn_age: u16,
}
impl Default for Show {
    fn default() -> Self {
        Self {
            events: Events::default(),
            tracers: [None; MAX_ENEMIES],
            flash: 0,
            flash_from: 0,
            glides: [Glide::default(); 3],
            enemy_glides: [Glide::default(); MAX_ENEMIES],
            hurt: [0; 3],
            enemy_hurt: [0; MAX_ENEMIES],
            phase_age: SETTLED_TICKS,
            turn_age: TURN_TICKS,
        }
    }
}
impl Show {
    /// Start a tick: forget the previous tick's events and age animations.
    pub(crate) fn tick(&mut self) {
        self.events.clear();
        self.phase_age = (self.phase_age + 1).min(SETTLED_TICKS);
        self.turn_age = (self.turn_age + 1).min(TURN_TICKS);
        self.flash = self.flash.saturating_sub(1);
        for glide in self.glides.iter_mut().chain(self.enemy_glides.iter_mut()) {
            glide.tick();
        }
        for hurt in self.hurt.iter_mut().chain(self.enemy_hurt.iter_mut()) {
            *hurt = hurt.saturating_sub(1);
        }
    }
    /// A new board: drop animations but keep this tick's events and clock.
    pub(crate) fn reset(&mut self) {
        let (events, phase_age) = (self.events, self.phase_age);
        *self = Self {
            events,
            phase_age,
            ..Self::default()
        };
    }
    /// How far the debrief or setback card has slid in (0..=1), or `None`
    /// while the board is still showing the operation's last moments.
    pub(crate) fn reveal(&self) -> Option<f32> {
        let age = self.phase_age.checked_sub(REVEAL_TICKS)?;
        let t = (f32::from(age) / f32::from(SLIDE_TICKS)).min(1.);
        // Ease out: quick arrival, soft landing.
        Some(1. - (1. - t) * (1. - t) * (1. - t))
    }
    pub(crate) fn push(&mut self, event: Event) {
        if matches!(event, Event::Turn(_)) {
            self.turn_age = 0;
        }
        self.events.push(event);
    }
    /// The new-turn banner's opacity (0 when hidden).
    pub(crate) fn turn_banner(&self) -> f32 {
        let age = f32::from(self.turn_age);
        let total = f32::from(TURN_TICKS);
        if age >= total {
            return 0.;
        }
        (age / 5.).min(1.) * ((total - age) / 20.).min(1.)
    }
    /// Show one squad shot's tracer.
    pub(crate) fn tracer(&mut self, from: Pos, to: Pos) {
        self.tracers = [None; MAX_ENEMIES];
        self.tracers[0] = Some((from, to, false));
        self.flash = TRACER_TICKS;
        self.flash_from = TRACER_TICKS;
    }
    /// Show the enemy volley's tracers (at most one per enemy slot).
    pub(crate) fn volley(&mut self, shots: [Option<(Pos, Pos, bool)>; MAX_ENEMIES]) {
        if shots.iter().any(Option::is_some) {
            self.tracers = shots;
            self.flash = VOLLEY_TICKS;
            self.flash_from = VOLLEY_TICKS;
        }
    }
}

/// Where board callouts start: over the top bulkhead row, so a rising name
/// or turn number never hides a machine.
pub(crate) const CALLOUT_AT: Vec2 = vec2(
    BOARD_X + TILE_SIZE * crate::content::COLS as f32 * 0.5,
    BOARD_Y + TILE_SIZE * 1.2,
);
const EMBER: Color = Color::new(1., 0.62, 0.3, 1.);
const DUST: Color = Color::new(0.62, 0.66, 0.68, 1.);

impl Game {
    /// Turn this tick's events into effects where they happened. Routine
    /// orders get small effects; disabled machines, arrivals and phase
    /// changes get big ones.
    pub(crate) fn spawn_effects(&self, cue: Cue, fx: &mut Juice) {
        let _ = cue;
        // A new board (next operation or a retry): the old board's bursts and
        // popups would float over the wrong map, so they go first.
        if self
            .show
            .events
            .iter()
            .any(|e| matches!(e, Event::Upgraded(_) | Event::Retried))
        {
            fx.clear();
        }
        for event in self.show.events.iter() {
            match event {
                Event::Deployed => {
                    for unit in self.units.iter().filter(|u| u.hp > 0) {
                        let at = unit.pos.center();
                        fx.ring(at, role_color(unit.role), 38.);
                        fx.sparkle(at, lighten(role_color(unit.role), 0.3));
                    }
                    fx.callout(CALLOUT_AT, self.content().name, GOLD);
                }
                Event::Selected(i) => {
                    let unit = self.units[usize::from(i)];
                    fx.ring(unit.pos.center(), lighten(role_color(unit.role), 0.25), 32.);
                }
                Event::Moved { unit, to } => {
                    let color = role_color(self.units[usize::from(unit)].role);
                    fx.ring(to.center(), with_alpha(color, 0.8), 24.);
                }
                Event::Fired {
                    from,
                    to,
                    damage,
                    disabled,
                } => {
                    let (a, b) = (from.center(), to.center());
                    let dir = (b - a).normalize_or(Vec2::X);
                    fx.burst(a + dir * 20., lighten(GOLD, 0.4), 6);
                    fx.burst(b, EMBER, 16);
                    fx.popup(b - vec2(0., 26.), &format!("-{damage}"), lighten(GOLD, 0.3));
                    fx.shake(0.28);
                    if disabled {
                        fx.burst(b, RED, 36);
                        fx.burst(b, EMBER, 20);
                        fx.ring(b, GOLD, 56.);
                        fx.popup_icon(b - vec2(0., 8.), Icon::Skull, "", PAPER);
                        fx.shake(0.32);
                    }
                }
                Event::Interacted { index, at } => {
                    let c = at.center();
                    let icon = objective_icon(self.content().objective, usize::from(index));
                    fx.ring(c, TEAL, 50.);
                    fx.sparkle(c, lighten(TEAL, 0.4));
                    fx.popup_icon(c - vec2(0., 30.), icon, "", lighten(TEAL, 0.3));
                }
                Event::Braced(i) => {
                    let c = self.units[usize::from(i)].pos.center();
                    fx.ring(c, GOLD, 36.);
                    fx.popup_icon(c - vec2(0., 30.), Icon::Shield, "-2", lighten(GOLD, 0.2));
                }
                Event::Focused(i) => {
                    let c = self.units[usize::from(i)].pos.center();
                    fx.ring(c, TEAL, 36.);
                    fx.sparkle(c, lighten(TEAL, 0.4));
                    fx.popup_icon(c - vec2(0., 30.), Icon::Bomb, "+2", lighten(TEAL, 0.2));
                }
                Event::Repaired { patient, amount } => {
                    let c = self.units[usize::from(patient)].pos.center();
                    fx.ring(c, BLUE, 36.);
                    fx.sparkle(c, GREEN);
                    fx.popup_icon(c - vec2(0., 30.), Icon::Heart, &format!("+{amount}"), GREEN);
                }
                Event::EnemyMoved { to } => {
                    fx.burst(to.center() + vec2(0., 14.), DUST, 5);
                }
                Event::Hit {
                    at,
                    damage,
                    braced,
                    disabled,
                } => {
                    let c = at.center();
                    fx.burst(c, RED, 18);
                    fx.burst(c, EMBER, 8);
                    fx.popup_icon(c - vec2(0., 28.), Icon::Heart, &format!("-{damage}"), RED);
                    if braced {
                        fx.ring(c, GOLD, 34.);
                    }
                    if disabled {
                        fx.burst(c, RED, 40);
                        fx.ring(c, RED, 60.);
                        fx.popup_icon(c - vec2(0., 6.), Icon::Skull, "", PAPER);
                        fx.vignette(Color::new(0.9, 0.12, 0.1, 0.5));
                        fx.shake(0.4);
                    } else {
                        fx.shake(0.18);
                    }
                }
                Event::Missed { at } => {
                    let c = at.center();
                    fx.burst(c + vec2(0., 10.), DUST, 10);
                    fx.popup(c - vec2(0., 20.), "Miss", with_alpha(PAPER, 0.85));
                }
                Event::Guarded { at, held, needed } => {
                    let c = at.center();
                    fx.ring(c, TEAL, 44.);
                    fx.popup_icon(
                        c - vec2(0., 32.),
                        Icon::Shield,
                        &format!("{held}/{needed}"),
                        lighten(TEAL, 0.2),
                    );
                }
                Event::Contested { at } => {
                    let c = at.center();
                    fx.ring(c, RED, 44.);
                    fx.popup_icon(c - vec2(0., 32.), Icon::Shield, "0", RED);
                }
                Event::Reinforced { at } => {
                    let c = at.center();
                    fx.ring(c, RED, 48.);
                    fx.burst(c, RED, 16);
                    fx.popup_icon(c - vec2(0., 32.), Icon::Alert, "", RED);
                }
                // The view draws the new turn's banner; the volley's hits
                // and misses above are the felt part.
                Event::Turn(_) => {}
                Event::Refused { at, why } => {
                    let c = at.center();
                    let (icon, word) = why.badge();
                    fx.ring(c, RED, 26.);
                    fx.popup_icon(c - vec2(0., 26.), icon, word, RED);
                    fx.shake(0.12);
                }
                // The final hit's own burst carries the moment; the
                // debrief card follows with its medals.
                Event::Secured => {}
                Event::Revealed => {
                    if self.phase == crate::model::Phase::Debrief {
                        let medals = self.medals[self.mission_index];
                        for k in 0..medals {
                            let at = crate::view::medal_center(k);
                            fx.sparkle(at, lighten(GOLD, 0.3));
                            fx.ring(at, GOLD, 34.);
                        }
                    } else {
                        fx.ring(crate::view::setback_badge(), RED, 90.);
                    }
                }
                Event::Upgraded(slot) => {
                    // The upgrade lands on every machine's card.
                    let slot = usize::from(slot);
                    let gain = crate::view::UPGRADE_GAIN[slot.min(2)];
                    for unit in 0..3 {
                        let at = crate::view::card_stat(unit, slot);
                        fx.sparkle(at, lighten(GOLD, 0.3));
                        fx.popup(at, gain, lighten(GOLD, 0.2));
                    }
                }
                Event::Retried => {
                    for unit in &self.units {
                        fx.ring(unit.pos.center(), with_alpha(TEAL, 0.7), 30.);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_are_bounded_and_cleared_each_tick() {
        let mut show = Show::default();
        for _ in 0..MAX_EVENTS + 8 {
            show.push(Event::Secured);
        }
        assert_eq!(show.events.iter().count(), MAX_EVENTS);
        show.tick();
        assert_eq!(show.events.iter().count(), 0);
        assert_eq!(show, Show::default());
    }

    #[test]
    fn glides_follow_the_route_and_finish() {
        let route = [Pos::new(1, 1), Pos::new(2, 1), Pos::new(2, 2)];
        let mut glide = Glide::along(&route);
        assert_eq!(glide.at().unwrap().0, route[0].center());
        for _ in 0..STEP_TICKS {
            glide.tick();
        }
        assert_eq!(glide.at().unwrap().0, route[1].center());
        for _ in 0..STEP_TICKS {
            glide.tick();
        }
        assert!(glide.at().is_none());
        // Long routes keep their final tiles, never more than eight.
        let long: Vec<_> = (0..11).map(|x| Pos::new(x, 3)).collect();
        let glide = Glide::along(&long);
        assert_eq!(glide.at().unwrap().0, Pos::new(3, 3).center());
    }
}
