//! Cosmetic state only: flashes, rings, sparks, score pops, shake, the
//! dot-matrix callout and the rolling score. Every effect is created from an
//! accepted simulation event or contact serial and aged on presentation time,
//! so nothing here can change collision, timing, scoring or input.
//!
//! Reduced motion clears and blocks rings, trails, sparks, bumper punch,
//! shake, the crane's travel animation and the score roll (it snaps). Reduced
//! flashes clears and blocks bumper illumination, table flashes and lamp
//! blinking (lamps stay steadily lit). Text pops and callouts are
//! informational, so they remain, but pops no longer rise under reduced motion.
use super::quips;
use crate::{BUMPERS, CRANE, PinballFeedback, Table};
use ferravex_render::{input::PlayerInput, preferences, prelude::Color};

const BUMPER_SECONDS: f32 = 0.18;
const PUNCH_SECONDS: f32 = 0.16;
const POP_SECONDS: f32 = 0.85;
const POPS: usize = 6;
const SPARKS: usize = 72;
/// Flipper presses needed before the flipper hint retires.
pub(super) const FLIPS_TO_LEARN: u32 = 6;
const SEED: u32 = 0x5a1f_a6e5;

pub(super) const GOLD: Color = Color::new(1.0, 0.80, 0.36, 1.0);
pub(super) const AMBER: Color = Color::new(1.0, 0.62, 0.22, 1.0);

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Pop {
    pub at: [f32; 2],
    pub points: u32,
    pub age: f32,
    pub big: bool,
}

impl Pop {
    pub fn alpha(&self) -> f32 {
        ((POP_SECONDS - self.age) / 0.25).clamp(0., 1.)
    }
    /// Table units risen so far; zero under reduced motion.
    pub fn rise(&self, motion: f32) -> f32 {
        let t = (self.age / POP_SECONDS).min(1.);
        (1. - (1. - t) * (1. - t)) * if self.big { 0.4 } else { 0.7 } * motion.min(1.)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Spark {
    pub at: [f32; 2],
    velocity: [f32; 2],
    pub age: f32,
    pub life: f32,
    pub color: Color,
}

impl Spark {
    pub fn alive(&self) -> bool {
        self.age < self.life
    }
}

/// A fixed spark pool; the oldest slot is overwritten when full.
#[derive(Clone, Copy)]
pub(super) struct Sparks(pub [Spark; SPARKS]);

impl Default for Sparks {
    fn default() -> Self {
        Self([Spark::default(); SPARKS])
    }
}

impl Sparks {
    pub fn iter(&self) -> impl Iterator<Item = &Spark> {
        self.0.iter()
    }
}

/// Importance of a dot-matrix message. Higher replaces lower immediately.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Priority {
    Banter,
    Event,
    Award,
    Jackpot,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Callout {
    pub head: String,
    pub sub: &'static str,
    pub color: Color,
    pub age: f32,
    pub life: f32,
    pub priority: Priority,
}

/// Lessons the player has demonstrated; these survive retries so hints that
/// were learned stay retired for the rest of the session.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Learned {
    pub flips: u32,
}

#[derive(Default)]
pub(super) struct Presentation {
    seen_hits: [u64; 3],
    seen_trail_tick: u32,
    flashes: [f32; 3],
    rings: [f32; 3],
    punch: [f32; 3],
    pub trail_len: usize,
    pub motion_scale: f32,
    flash_scale: f32,
    pub launch_seconds: f32,
    pub pops: [Option<Pop>; POPS],
    pub sparks: Sparks,
    next_spark: usize,
    rng: u32,
    trauma: f32,
    /// Presentation seconds; drives shake and lamp blinking.
    pub time: f32,
    board_flash: f32,
    pub board_flash_color: Color,
    pub callout: Option<Callout>,
    /// Score shown in the header, rolling toward the table's score.
    pub shown_score: f64,
    /// 0 when the magnet is parked at the gantry, 1 when lowered.
    pub crane_drop: f32,
    mash: f32,
    mash_cooldown: f32,
    mashes: u32,
    unsaved_drains: u32,
    pub learned: Learned,
}

impl Presentation {
    /// Run on every presentation frame BEFORE accepting events, even with no
    /// simulation ticks, paused or in Controls. The shared policy is the only
    /// preference store; no game-specific export or persistence is needed.
    pub fn begin_frame(&mut self) {
        let policy = preferences::feedback_policy();
        self.apply_preferences(
            policy.reduced_motion(),
            policy.reduced_flash(),
            policy.intensity(),
        );
    }

    fn apply_preferences(&mut self, reduced_motion: bool, reduced_flash: bool, intensity: f32) {
        self.motion_scale = if reduced_motion { 0. } else { intensity };
        self.flash_scale = if reduced_flash { 0. } else { intensity };
        if self.motion_scale == 0. {
            self.rings.fill(0.);
            self.punch.fill(0.);
            self.trail_len = 0;
            self.trauma = 0.;
            self.sparks = Sparks::default();
        }
        if self.flash_scale == 0. {
            self.flashes.fill(0.);
            self.board_flash = 0.;
        }
    }

    /// Discard every effect for a fresh game; learned hints are kept.
    pub fn reset(&mut self) {
        let learned = self.learned;
        *self = Self::default();
        self.learned = learned;
        self.begin_frame();
    }

    fn random(&mut self) -> f32 {
        // xorshift32: presentation-only variety, reseeded on reset.
        let mut x = if self.rng == 0 { SEED } else { self.rng };
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x >> 8) as f32 / (1 << 24) as f32
    }

    fn burst(&mut self, at: [f32; 2], count: usize, speed: f32, colors: &[Color]) {
        if self.motion_scale == 0. {
            return;
        }
        for n in 0..count {
            let angle = self.random() * std::f32::consts::TAU;
            let v = speed * (0.45 + 0.55 * self.random());
            let life = 0.35 + 0.35 * self.random();
            self.sparks.0[self.next_spark] = Spark {
                at,
                velocity: [angle.cos() * v, angle.sin() * v],
                age: 0.,
                life,
                color: colors[n % colors.len()],
            };
            self.next_spark = (self.next_spark + 1) % SPARKS;
        }
    }

    fn pop(&mut self, at: [f32; 2], points: u32, big: bool) {
        let slot = self
            .pops
            .iter()
            .position(Option::is_none)
            .unwrap_or_else(|| {
                // Replace the oldest pop rather than stacking unbounded labels.
                (0..POPS)
                    .max_by(|&a, &b| {
                        let age = |i: usize| self.pops[i].map_or(0., |p| p.age);
                        age(a).total_cmp(&age(b))
                    })
                    .unwrap_or(0)
            });
        self.pops[slot] = Some(Pop {
            at,
            points,
            age: 0.,
            big,
        });
    }

    fn shake(&mut self, amount: f32) {
        if self.motion_scale > 0. {
            self.trauma = (self.trauma + amount).min(1.);
        }
    }

    fn flash_board(&mut self, amount: f32, color: Color) {
        if self.flash_scale > 0. {
            self.board_flash = self.board_flash.max(amount);
            self.board_flash_color = color;
        }
    }

    /// Show a dot-matrix message unless a more important one is still fresh.
    pub(super) fn say(
        &mut self,
        head: String,
        sub: &'static str,
        color: Color,
        life: f32,
        priority: Priority,
    ) {
        if let Some(current) = &self.callout
            && current.priority > priority
            && current.age < current.life * 0.6
        {
            return;
        }
        self.callout = Some(Callout {
            head,
            sub,
            color,
            age: 0.,
            life,
            priority,
        });
    }

    pub fn finish_frame(&mut self, dt: f32, table: &Table, events: PinballFeedback) {
        let dt = if dt.is_finite() {
            dt.clamp(0., 0.25)
        } else {
            0.
        };
        self.time += dt;
        self.launch_seconds = (self.launch_seconds - dt).max(0.);
        self.board_flash = (self.board_flash - dt * 2.5).max(0.);
        self.trauma = (self.trauma - dt * 1.6).max(0.);
        self.mash = (self.mash - dt * 3.).max(0.);
        self.mash_cooldown = (self.mash_cooldown - dt).max(0.);
        for pop in &mut self.pops {
            if let Some(p) = pop {
                p.age += dt;
                if p.age >= POP_SECONDS {
                    *pop = None;
                }
            }
        }
        for spark in &mut self.sparks.0 {
            if spark.alive() {
                spark.age += dt;
                let drag = (1. - dt * 3.).max(0.);
                spark.velocity = [spark.velocity[0] * drag, spark.velocity[1] * drag + dt * 6.];
                spark.at[0] += spark.velocity[0] * dt;
                spark.at[1] += spark.velocity[1] * dt;
            }
        }
        if let Some(callout) = &mut self.callout {
            callout.age += dt;
            if callout.age >= callout.life {
                self.callout = None;
            }
        }
        // The score rolls up quickly; reduced motion shows it immediately.
        let target = f64::from(table.score);
        if self.motion_scale == 0. || target < self.shown_score {
            self.shown_score = target;
        } else {
            let rate = ((target - self.shown_score) * 7.).max(900.);
            self.shown_score = (self.shown_score + rate * f64::from(dt)).min(target);
        }
        let lowered = if table.crane_armed || table.crane_hold > 0. {
            1.
        } else {
            0.
        };
        self.crane_drop = if self.motion_scale == 0. {
            lowered
        } else if lowered > self.crane_drop {
            (self.crane_drop + dt * 2.2).min(1.)
        } else {
            (self.crane_drop - dt * 1.4).max(0.)
        };

        if events.launches > 0 {
            self.launch_seconds = 0.65;
            self.shake(0.12);
        }
        if events.drains > 0 {
            self.launch_seconds = 0.;
        }
        for (i, bumper) in BUMPERS.into_iter().enumerate() {
            self.flashes[i] = (self.flashes[i] - dt).max(0.);
            self.rings[i] = (self.rings[i] - dt).max(0.);
            self.punch[i] = (self.punch[i] - dt).max(0.);
            if table.bumper_hits[i] != self.seen_hits[i] {
                if self.flash_scale > 0. {
                    self.flashes[i] = BUMPER_SECONDS;
                }
                if self.motion_scale > 0. {
                    self.rings[i] = BUMPER_SECONDS;
                    self.punch[i] = PUNCH_SECONDS;
                }
                self.pop([bumper[0], bumper[1] - 0.75], table.bumper_points[i], false);
                self.burst(bumper, 7, 5., &[GOLD, AMBER, super::CREAM]);
                self.shake(0.07);
            }
        }
        // Consume serials even while effects are suppressed. Enabling a channel
        // cannot replay contacts accepted under the previous preference.
        self.seen_hits = table.bumper_hits;
        self.accept_rules(table, events);

        // Reuse the fixed simulation trail, exposing only samples produced after
        // motion was enabled. Never reveal pre-toggle points on a zero-tick frame.
        let ticks = table.trail_tick.wrapping_sub(self.seen_trail_tick);
        let samples = (u64::from(ticks) + u64::from(self.seen_trail_tick % 3)) / 3;
        self.seen_trail_tick = table.trail_tick;
        if self.motion_scale == 0.
            || table.waiting
            || table.over()
            || events.drains > 0
            || table.crane_hold > 0.
        {
            self.trail_len = 0;
        } else {
            self.trail_len = (self.trail_len + samples.min(12) as usize).min(12);
        }
    }

    /// Callouts, pops and big moments for accepted rule events.
    fn accept_rules(&mut self, table: &Table, events: PinballFeedback) {
        let presses = events.flipper_presses.iter().filter(|p| **p).count() as u32;
        self.learned.flips = self.learned.flips.saturating_add(presses);
        self.mash += presses as f32;
        if self.mash > 9. && self.mash_cooldown == 0. && !table.waiting {
            self.mash_cooldown = 15.;
            self.mashes += 1;
            let line = quips::mash(self.mashes);
            self.say(
                "EASY THERE!".into(),
                line,
                super::MINT,
                2.2,
                Priority::Banter,
            );
        }
        if let Some(line) = quips::chain(events.chain) {
            self.say(
                format!("{line}  x{}", events.chain),
                "CHAIN BONUS ON EVERY HIT",
                AMBER,
                1.4,
                Priority::Event,
            );
        }
        if events.crane_arms > 0 {
            self.say(
                "CRANE ARMED!".into(),
                "SHOOT UNDER THE MAGNET",
                GOLD,
                2.4,
                Priority::Award,
            );
            self.flash_board(0.25, GOLD);
        }
        if events.skill_shots > 0 {
            self.say(
                format!("SKILL SHOT  +{}", quips::grouped(crate::SKILL_SHOT_POINTS)),
                "THE FOREMAN NODS. SLOWLY.",
                GOLD,
                2.2,
                Priority::Award,
            );
            self.flash_board(0.3, GOLD);
            self.burst(BUMPERS[0], 16, 7., &[GOLD, super::CREAM]);
        }
        if events.secrets > 0 {
            self.say(
                format!("OVERTIME PAY  +{}", quips::grouped(crate::OVERTIME_POINTS)),
                "SECRET: 45S ON ONE BALL",
                GOLD,
                3.,
                Priority::Jackpot,
            );
            self.flash_board(0.3, GOLD);
        }
        if events.jackpots > 0 {
            self.say(
                format!("JACKPOT  +{}", quips::grouped(table.last_jackpot)),
                quips::jackpot(table.jackpots),
                GOLD,
                2.6,
                Priority::Jackpot,
            );
            self.pop([CRANE[0], CRANE[1] + 1.75], table.last_jackpot, true);
            self.shake(0.55);
            self.flash_board(0.45, super::CREAM);
            self.burst(CRANE, 30, 9., &[GOLD, AMBER, super::CREAM, super::RUST]);
        }
        if events.crane_drops > 0 {
            self.shake(0.2);
            self.burst(CRANE, 10, 4., &[super::BRASS, super::CREAM]);
        }
        if events.saves > 0 {
            self.say(
                "BALL SAVED".into(),
                quips::save(table.saves),
                super::MINT,
                2.,
                Priority::Award,
            );
            self.flash_board(0.2, super::MINT);
        }
        if events.drains > events.saves {
            self.unsaved_drains += 1;
            self.shake(0.35);
            self.flash_board(0.35, super::RUST);
            self.burst([4.9, 14.9], 14, 6., &[super::RUST, AMBER]);
            if !table.over() {
                let head = match table.lives {
                    1 => "LAST BALL!".to_owned(),
                    n => format!("DRAINED  /  {n} LEFT"),
                };
                self.say(
                    head,
                    quips::drain(self.unsaved_drains, table.lives),
                    super::CREAM,
                    2.6,
                    Priority::Award,
                );
            }
        }
    }

    /// Opacity of the strongest live jackpot pop; table labels under it fade.
    pub fn big_pop(&self) -> f32 {
        self.pops
            .iter()
            .flatten()
            .filter(|p| p.big)
            .map(Pop::alpha)
            .fold(0., f32::max)
    }

    pub fn flash(&self, bumper: usize) -> f32 {
        self.flashes[bumper] / BUMPER_SECONDS * self.flash_scale
    }

    pub fn ring(&self, bumper: usize) -> f32 {
        self.rings[bumper] / BUMPER_SECONDS * self.motion_scale
    }

    /// Bumper cap scale, briefly enlarged by a hit; 1 under reduced motion.
    pub fn punch(&self, bumper: usize) -> f32 {
        let t = self.punch[bumper] / PUNCH_SECONDS;
        1. + 0.14 * t * t * self.motion_scale.min(1.)
    }

    /// Table overlay flash, 0..=1, already scaled by the flash preference.
    pub fn board_flash(&self) -> f32 {
        self.board_flash * self.flash_scale.min(1.)
    }

    /// Whether lamp blinking is permitted; otherwise lamps are steady.
    pub fn blink(&self, hz: f32) -> bool {
        self.flash_scale == 0. || (self.time * hz).fract() < 0.6
    }

    /// Screen offset in pixels; zero under reduced motion.
    pub fn shake_offset(&self) -> [f32; 2] {
        let amount = self.trauma * self.trauma * 12. * self.motion_scale.min(1.);
        [
            (self.time * 57.).sin() * amount,
            (self.time * 43. + 1.3).cos() * amount * 0.8,
        ]
    }
}

/// Caller-owned, bounded labels from the current binding/device. A huge custom
/// binding list directs players to Controls rather than showing obsolete keys.
pub(super) fn hint<'a>(input: &PlayerInput, action: &str, buffer: &'a mut [u8]) -> &'a str {
    input.format_hint(action, buffer).unwrap_or("SEE CONTROLS")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BUMPERS, Controls, PinballGame};
    use ferravex_physics::d2::Vector;
    use ferravex_render::input::{ActionKind, Binding, InputCode};

    const DT: f32 = 1. / 120.;

    fn hit(table: &mut Table, bumper: usize) -> PinballFeedback {
        let before = (table.score, table.hits);
        table.waiting = false;
        let [x, y] = BUMPERS[bumper];
        table.world.bodies[table.ball].set_translation(Vector::new(x, y - 0.78), true);
        table.world.bodies[table.ball].set_linvel(Vector::new(0., 2.), true);
        table.step(DT, Controls::default());
        PinballFeedback {
            points: table.score - before.0,
            hits: table.hits - before.1,
            chain: table.chain,
            ..Default::default()
        }
    }

    #[test]
    fn independent_channels_clear_on_zero_tick_frames_without_reviving_effects() {
        let mut table = Table::new();
        let mut view = Presentation::default();
        view.begin_frame(); // Actual shared-policy read before the first contact.
        let events = hit(&mut table, 0);
        assert_eq!(events.points, 100);
        view.finish_frame(DT, &table, events);
        assert!(view.flash(0) > 0. && view.ring(0) > 0.);
        assert!(view.punch(0) > 1.);
        let simulation_timer = table.bumper_flash[0];

        // The same reducer used by begin_frame; native renderer does not
        // reexport a named FeedbackPolicy constructor. Host ABI testing belongs
        // to the parent's fresh Wasm/browser pass.
        view.apply_preferences(false, true, 1.);
        view.finish_frame(0., &table, PinballFeedback::default());
        assert_eq!(view.flash(0), 0.);
        assert!(view.ring(0) > 0.);
        view.apply_preferences(false, false, 1.);
        view.finish_frame(0., &table, PinballFeedback::default());
        assert_eq!(view.flash(0), 0., "enabling flash must not revive a hit");
        view.apply_preferences(true, false, 1.);
        assert_eq!(view.ring(0), 0.);
        assert_eq!(view.punch(0), 1.);
        assert!(view.sparks.iter().all(|s| !s.alive()));
        assert_eq!(view.shake_offset(), [0., 0.]);
        view.apply_preferences(false, false, 1.);
        view.finish_frame(0., &table, PinballFeedback::default());
        assert_eq!(view.ring(0), 0., "enabling motion must not revive a ring");
        assert!(view.sparks.iter().all(|s| !s.alive()));
        assert_eq!(table.bumper_flash[0], simulation_timer);
        assert!(
            view.pops.iter().flatten().any(|p| p.points == 100),
            "textual score feedback survives"
        );

        let events = hit(&mut table, 1);
        view.finish_frame(DT, &table, events);
        assert!(
            view.flash(1) > 0. && view.ring(1) > 0.,
            "fresh hits still work"
        );
        assert!(view.sparks.iter().any(Spark::alive));
        view.apply_preferences(true, false, 0.5);
        assert_eq!(view.ring(1), 0.);
        assert_eq!(
            view.flash(1),
            0.5,
            "motion and intensity do not disable flash"
        );
        view.apply_preferences(false, false, 0.);
        view.apply_preferences(false, false, 1.);
        assert_eq!(view.flash(1), 0.);
    }

    #[test]
    fn suppressed_hits_do_not_reappear_or_change_contact_debounce() {
        let mut table = Table::new();
        let mut control = Table::new();
        let mut view = Presentation::default();
        view.apply_preferences(true, true, 1.);
        for repeat in 0..3 {
            let event = hit(&mut table, 0);
            assert_eq!(event, hit(&mut control, 0));
            view.finish_frame(DT, &table, event);
            assert_eq!(view.flash(0), 0.);
            assert_eq!(view.ring(0), 0.);
            assert_eq!(view.board_flash(), 0.);
            assert_eq!(view.shake_offset(), [0., 0.]);
            // Leave the contact for a tick, then return inside the debounce on
            // the second iteration. Wait out the debounce before the third.
            for _ in 0..if repeat == 0 { 1 } else { 24 } {
                for t in [&mut table, &mut control] {
                    t.world.bodies[t.ball].set_translation(Vector::new(4.4, 2.), true);
                    t.world.bodies[t.ball].set_linvel(Vector::ZERO, true);
                    t.step(DT, Controls::default());
                }
                view.finish_frame(DT, &table, PinballFeedback::default());
            }
            assert_eq!(table.score, if repeat < 2 { 100 } else { 200 });
        }
        view.apply_preferences(false, false, 1.);
        view.finish_frame(0., &table, PinballFeedback::default());
        assert_eq!(view.flash(0), 0.);
        assert_eq!(view.ring(0), 0.);
        assert_eq!(view.trail_len, 0);
        assert_eq!(table.position(), control.position());
        assert_eq!(table.bumper_flash, control.bumper_flash);
        assert_eq!(table.score, control.score);
    }

    #[test]
    fn reenabling_motion_only_exposes_new_trail_samples_and_reset_clears_notices() {
        let mut game = PinballGame::new(true);
        let mut view = Presentation::default();
        view.begin_frame();
        let events = game.advance(12. / 120., Controls::default());
        view.finish_frame(0.1, game.table(), events);
        assert_eq!(view.trail_len, 4);
        assert!(view.launch_seconds > 0.);
        view.apply_preferences(true, false, 1.);
        assert_eq!(view.trail_len, 0);
        let events = game.advance(12. / 120., Controls::default());
        view.finish_frame(0.1, game.table(), events);
        view.apply_preferences(false, false, 1.);
        view.finish_frame(0., game.table(), PinballFeedback::default());
        assert_eq!(view.trail_len, 0);
        let events = game.advance(3. / 120., Controls::default());
        view.finish_frame(0.025, game.table(), events);
        assert_eq!(view.trail_len, 1);
        game.set_paused(true);
        let events = game.advance(60., Controls::default());
        view.finish_frame(0.1, game.table(), events);
        assert_eq!(
            view.trail_len, 1,
            "paused presentation cannot invent trail samples"
        );
        view.learned.flips = 3;
        game.restart();
        view.reset();
        assert_eq!(view.trail_len, 0);
        assert!(view.pops.iter().all(Option::is_none));
        assert_eq!(view.callout, None);
        assert_eq!(view.shown_score, 0.);
        assert_eq!(view.launch_seconds, 0.);
        assert_eq!(view.learned.flips, 3, "learned hints stay retired");
        let events = game.advance(0., Controls::default());
        view.finish_frame(0., game.table(), events);
        assert_eq!(view.trail_len, 0);
    }

    #[test]
    fn bounded_score_pops_expire_without_simulation_ticks() {
        let mut table = Table::new();
        let mut view = Presentation::default();
        view.begin_frame();
        for bumper in [0, 1, 2, 0, 1, 2, 0, 1] {
            // Clear the debounce so every iteration is an accepted contact.
            table.bumper_flash = [0.; 3];
            // Away from every bumper and from the crane's catch zone.
            table.world.bodies[table.ball].set_translation(Vector::new(7.4, 6.6), true);
            table.step(DT, Controls::default());
            let events = hit(&mut table, bumper);
            assert!(events.points >= 100);
            view.finish_frame(DT, &table, events);
        }
        assert_eq!(table.hits, 8);
        assert_eq!(
            view.pops.iter().flatten().count(),
            POPS,
            "pops stay bounded"
        );
        let time = table.time;
        view.finish_frame(f32::NAN, &table, PinballFeedback::default());
        assert_eq!(view.pops.iter().flatten().count(), POPS);
        for _ in 0..4 {
            view.finish_frame(0.25, &table, PinballFeedback::default());
        }
        assert!(view.pops.iter().all(Option::is_none));
        assert_eq!(view.flash(0), 0.);
        assert_eq!(view.ring(1), 0.);
        assert_eq!(table.time, time);
        assert_eq!(view.shown_score, f64::from(table.score), "roll settles");
    }

    #[test]
    fn callouts_follow_accepted_events_and_respect_priority() {
        let mut table = Table::new();
        let mut view = Presentation::default();
        view.apply_preferences(false, false, 1.);
        // Three quick hits on alternating bumpers make a chain callout.
        for bumper in [0, 1, 0] {
            table.bumper_flash = [0.; 3];
            let events = hit(&mut table, bumper);
            view.finish_frame(DT, &table, events);
        }
        assert_eq!(table.chain, 3);
        let chain = view.callout.clone().expect("chain callout");
        assert!(chain.head.starts_with("CLANK"));
        assert_eq!(chain.priority, Priority::Event);
        // Banter cannot interrupt a fresh event, but a jackpot always can.
        view.say("NOISE".into(), "", GOLD, 1., Priority::Banter);
        assert_eq!(view.callout.as_ref().unwrap().head, chain.head);
        view.finish_frame(
            DT,
            &table,
            PinballFeedback {
                jackpots: 1,
                ..Default::default()
            },
        );
        assert!(view.callout.as_ref().unwrap().head.starts_with("JACKPOT"));
        assert!(view.pops.iter().flatten().any(|p| p.big));
        assert_eq!(view.big_pop(), 1., "crane labels fade under the pop");
        for _ in 0..20 {
            view.finish_frame(0.25, &table, PinballFeedback::default());
        }
        assert_eq!(view.callout, None, "callouts expire on presentation time");
    }

    #[test]
    fn flipper_mashing_earns_one_rate_limited_foreman_remark() {
        let mut table = Table::new();
        table.waiting = false;
        let mut view = Presentation::default();
        let mash = PinballFeedback {
            flipper_presses: [true; 2],
            ..Default::default()
        };
        for _ in 0..6 {
            view.finish_frame(0.05, &table, mash);
        }
        let remark = view.callout.clone().expect("mash remark");
        assert_eq!(remark.priority, Priority::Banter);
        view.callout = None;
        for _ in 0..6 {
            view.finish_frame(0.05, &table, mash);
        }
        assert_eq!(view.callout, None, "cooldown prevents nagging");
        assert_eq!(view.learned.flips, 24);
    }

    #[test]
    fn hints_follow_remaps_and_report_unbound_or_oversized_lists_honestly() {
        let mut input = PlayerInput::transient("pinball-hint-test");
        input
            .action(
                "launch",
                "Launch",
                ActionKind::Button,
                vec![Binding::digital(InputCode::Key(32))],
            )
            .unwrap();
        let mut text = [0; 128];
        assert_eq!(hint(&input, "launch", &mut text), "Space");
        input
            .rebind("launch", vec![Binding::digital(InputCode::Key(106))])
            .unwrap();
        assert_eq!(hint(&input, "launch", &mut text), "J");
        input.rebind("launch", vec![]).unwrap();
        assert_eq!(hint(&input, "launch", &mut text), "Unbound");
        input
            .rebind("launch", vec![Binding::digital(InputCode::Key(106)); 64])
            .unwrap();
        assert_eq!(hint(&input, "launch", &mut text), "SEE CONTROLS");
    }
}
