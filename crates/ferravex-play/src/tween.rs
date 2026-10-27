//! Easing curves, tweens, springs and UI/screen transition timing.
//!
//! Everything here is `Copy`, allocation-free and advanced by a caller-supplied
//! `dt` in seconds, so presentation stays deterministic for a given sequence of
//! timesteps (subject to the platform's `exp`/`sin`/`powf`, as with
//! [`crate::motion`]). Nothing reads a clock. Drive these from the frame delta
//! for menus and HUD polish; drive them from the fixed tick when the motion is
//! part of gameplay timing.
//!
//! Reduced motion is an explicit [`Motion`] argument. Build it from the player's
//! [`FeedbackPolicy`] with `Motion::from(policy)`. Under [`Motion::Reduced`]
//! tweens and springs snap to their targets, entrances only fade, roll counters
//! show the final number and screen transitions become plain fades.
//!
//! ```
//! use ferravex_play::tween::{Ease, Motion, Spring, Tween};
//! let mut fade = Tween::new(0., 1., 0.25, Ease::CubicOut);
//! let mut scale = Spring::underdamped(0.8, 6., 0.45);
//! scale.set_target(1.);
//! for _ in 0..30 {
//!     let alpha = fade.advance(1. / 60., Motion::Full);
//!     let s = scale.advance(1. / 60., Motion::Full);
//!     assert!((0. ..=1.).contains(&alpha) && s.is_finite());
//! }
//! assert!(fade.is_finished() && fade.value() == 1.);
//! ```
use crate::feedback::FeedbackPolicy;
use std::f32::consts::{FRAC_PI_2, PI, TAU};

/// Whether presentation motion is full or reduced for accessibility.
///
/// ```
/// use ferravex_play::{feedback::FeedbackPolicy, tween::Motion};
/// let policy = FeedbackPolicy::new(true, false, 1.).unwrap();
/// assert_eq!(Motion::from(policy), Motion::Reduced);
/// assert_eq!(Motion::from_reduced(false), Motion::Full);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Motion {
    #[default]
    Full,
    Reduced,
}

impl Motion {
    pub const fn from_reduced(reduced: bool) -> Self {
        if reduced { Self::Reduced } else { Self::Full }
    }
    pub const fn is_reduced(self) -> bool {
        matches!(self, Self::Reduced)
    }
}

impl From<FeedbackPolicy> for Motion {
    fn from(policy: FeedbackPolicy) -> Self {
        Self::from_reduced(policy.reduced_motion())
    }
}

impl From<&FeedbackPolicy> for Motion {
    fn from(policy: &FeedbackPolicy) -> Self {
        Self::from(*policy)
    }
}

/// Standard easing curves. [`Ease::apply`] maps `t` in `[0, 1]` (clamped) to
/// eased progress with exact endpoints `0 → 0` and `1 → 1`. Back and elastic
/// curves overshoot outside `[0, 1]` in between; bounce stays inside it.
///
/// ```
/// use ferravex_play::tween::Ease;
/// for ease in Ease::ALL {
///     assert_eq!((ease.apply(0.), ease.apply(1.)), (0., 1.));
/// }
/// assert!(Ease::QuadIn.apply(0.5) < 0.5 && Ease::QuadOut.apply(0.5) > 0.5);
/// assert!(Ease::BackOut.apply(0.7) > 1.); // overshoot
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Ease {
    #[default]
    Linear,
    SineIn,
    SineOut,
    SineInOut,
    QuadIn,
    QuadOut,
    QuadInOut,
    CubicIn,
    CubicOut,
    CubicInOut,
    QuartIn,
    QuartOut,
    QuartInOut,
    ExpoIn,
    ExpoOut,
    ExpoInOut,
    BackIn,
    BackOut,
    BackInOut,
    ElasticIn,
    ElasticOut,
    ElasticInOut,
    BounceIn,
    BounceOut,
    BounceInOut,
}

fn bounce_out(t: f32) -> f32 {
    const N: f32 = 7.5625;
    const D: f32 = 2.75;
    if t < 1. / D {
        N * t * t
    } else if t < 2. / D {
        let t = t - 1.5 / D;
        N * t * t + 0.75
    } else if t < 2.5 / D {
        let t = t - 2.25 / D;
        N * t * t + 0.9375
    } else {
        let t = t - 2.625 / D;
        N * t * t + 0.984375
    }
}

impl Ease {
    /// Every curve, in declaration order.
    pub const ALL: [Self; 25] = [
        Self::Linear,
        Self::SineIn,
        Self::SineOut,
        Self::SineInOut,
        Self::QuadIn,
        Self::QuadOut,
        Self::QuadInOut,
        Self::CubicIn,
        Self::CubicOut,
        Self::CubicInOut,
        Self::QuartIn,
        Self::QuartOut,
        Self::QuartInOut,
        Self::ExpoIn,
        Self::ExpoOut,
        Self::ExpoInOut,
        Self::BackIn,
        Self::BackOut,
        Self::BackInOut,
        Self::ElasticIn,
        Self::ElasticOut,
        Self::ElasticInOut,
        Self::BounceIn,
        Self::BounceOut,
        Self::BounceInOut,
    ];

    /// Eased progress for `t` (clamped to `[0, 1]`; NaN counts as 0).
    pub fn apply(self, t: f32) -> f32 {
        if t.is_nan() || t <= 0. {
            return 0.;
        }
        if t >= 1. {
            return 1.;
        }
        const C1: f32 = 1.70158;
        const C2: f32 = C1 * 1.525;
        const C3: f32 = C1 + 1.;
        const C4: f32 = TAU / 3.;
        const C5: f32 = TAU / 4.5;
        let inout = |t: f32, f: fn(f32) -> f32| {
            if t < 0.5 {
                f(2. * t) * 0.5
            } else {
                1. - f(2. - 2. * t) * 0.5
            }
        };
        match self {
            Self::Linear => t,
            Self::SineIn => 1. - (t * FRAC_PI_2).cos(),
            Self::SineOut => (t * FRAC_PI_2).sin(),
            Self::SineInOut => -((PI * t).cos() - 1.) * 0.5,
            Self::QuadIn => t * t,
            Self::QuadOut => 1. - (1. - t) * (1. - t),
            Self::QuadInOut => inout(t, |t| t * t),
            Self::CubicIn => t * t * t,
            Self::CubicOut => 1. - (1. - t).powi(3),
            Self::CubicInOut => inout(t, |t| t * t * t),
            Self::QuartIn => t.powi(4),
            Self::QuartOut => 1. - (1. - t).powi(4),
            Self::QuartInOut => inout(t, |t| t.powi(4)),
            Self::ExpoIn => 2_f32.powf(10. * t - 10.),
            Self::ExpoOut => 1. - 2_f32.powf(-10. * t),
            Self::ExpoInOut => inout(t, |t| 2_f32.powf(10. * t - 10.)),
            Self::BackIn => C3 * t * t * t - C1 * t * t,
            Self::BackOut => {
                let u = t - 1.;
                1. + C3 * u * u * u + C1 * u * u
            }
            Self::BackInOut => {
                if t < 0.5 {
                    let u = 2. * t;
                    u * u * ((C2 + 1.) * u - C2) * 0.5
                } else {
                    let u = 2. * t - 2.;
                    (u * u * ((C2 + 1.) * u + C2) + 2.) * 0.5
                }
            }
            Self::ElasticIn => -(2_f32.powf(10. * t - 10.)) * ((t * 10. - 10.75) * C4).sin(),
            Self::ElasticOut => 2_f32.powf(-10. * t) * ((t * 10. - 0.75) * C4).sin() + 1.,
            Self::ElasticInOut => {
                let s = ((20. * t - 11.125) * C5).sin();
                if t < 0.5 {
                    -(2_f32.powf(20. * t - 10.) * s) * 0.5
                } else {
                    2_f32.powf(-20. * t + 10.) * s * 0.5 + 1.
                }
            }
            Self::BounceIn => 1. - bounce_out(1. - t),
            Self::BounceOut => bounce_out(t),
            Self::BounceInOut => {
                if t < 0.5 {
                    (1. - bounce_out(1. - 2. * t)) * 0.5
                } else {
                    (1. + bounce_out(2. * t - 1.)) * 0.5
                }
            }
        }
    }
}

/// Linear interpolation `a + (b - a) * t` (`t` is not clamped).
///
/// ```
/// assert_eq!(ferravex_play::tween::lerp(10., 20., 0.25), 12.5);
/// ```
pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn step(dt: f32) -> f32 {
    if dt.is_finite() { dt.max(0.) } else { 0. }
}

/// A one-shot eased transition from `from` to `to` over `duration` seconds,
/// optionally after a delay. Non-finite or negative timesteps count as zero;
/// a nonpositive duration finishes immediately.
///
/// ```
/// use ferravex_play::tween::{Ease, Motion, Tween};
/// let mut slide = Tween::new(-40., 0., 0.4, Ease::BackOut).delayed(0.1);
/// assert_eq!(slide.advance(0.1, Motion::Full), -40.); // still delayed
/// slide.advance(0.2, Motion::Full);
/// assert!(slide.value() > -40. && !slide.is_finished());
/// slide.advance(1., Motion::Full);
/// assert_eq!(slide.value(), 0.);
/// slide.retarget(100.); // continue smoothly from the current value
/// assert_eq!(Tween::new(0., 5., 1., Ease::Linear).advance(0.01, Motion::Reduced), 5.);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tween {
    from: f32,
    to: f32,
    duration: f32,
    delay: f32,
    elapsed: f32,
    ease: Ease,
}

impl Tween {
    pub fn new(from: f32, to: f32, duration: f32, ease: Ease) -> Self {
        Self {
            from,
            to,
            duration: if duration.is_finite() {
                duration.max(0.)
            } else {
                0.
            },
            delay: 0.,
            elapsed: 0.,
            ease,
        }
    }
    /// A tween that already sits at `value`.
    pub fn settled(value: f32) -> Self {
        Self::new(value, value, 0., Ease::Linear)
    }
    /// Waits `delay` seconds before starting.
    pub fn delayed(mut self, delay: f32) -> Self {
        self.delay = if delay.is_finite() { delay.max(0.) } else { 0. };
        self
    }
    /// Advances time and returns the new value. Reduced motion jumps to the end.
    pub fn advance(&mut self, dt: f32, motion: Motion) -> f32 {
        if motion.is_reduced() {
            self.finish();
        } else {
            let end = self.delay + self.duration;
            self.elapsed = (self.elapsed + step(dt)).min(end);
        }
        self.value()
    }
    /// Linear progress in `[0, 1]`, excluding the delay.
    pub fn progress(&self) -> f32 {
        if self.duration <= 0. {
            return if self.elapsed >= self.delay { 1. } else { 0. };
        }
        ((self.elapsed - self.delay) / self.duration).clamp(0., 1.)
    }
    /// Current eased value; exactly `to` once finished.
    pub fn value(&self) -> f32 {
        let p = self.progress();
        if p >= 1. {
            self.to
        } else {
            lerp(self.from, self.to, self.ease.apply(p))
        }
    }
    pub fn target(&self) -> f32 {
        self.to
    }
    pub fn is_finished(&self) -> bool {
        self.progress() >= 1.
    }
    /// Jumps to the end.
    pub fn finish(&mut self) {
        self.elapsed = self.delay + self.duration;
    }
    /// Restarts from `from` (keeping duration, delay and easing).
    pub fn restart(&mut self) {
        self.elapsed = 0.;
    }
    /// Starts a new tween from the current value toward `to` without a delay.
    pub fn retarget(&mut self, to: f32) {
        if to != self.to || !self.is_finished() {
            self.from = self.value();
            self.to = to;
            self.delay = 0.;
            self.elapsed = 0.;
        }
    }
}

/// A damped harmonic spring toward `target`, integrated analytically so any
/// `dt` is stable and splitting a step gives the same result (up to rounding).
///
/// `frequency` is the undamped angular speed in Hz; `damping` is the damping
/// ratio: `1` is critically damped (fastest without overshoot), below `1`
/// overshoots and wobbles, above `1` is sluggish.
///
/// ```
/// use ferravex_play::tween::{Motion, Spring};
/// let mut hover = Spring::critical(0., 8.);
/// hover.set_target(1.);
/// let mut peak: f32 = 0.;
/// for _ in 0..120 { peak = peak.max(hover.advance(1. / 60., Motion::Full)); }
/// assert!(peak <= 1. && hover.is_settled(1e-3));
///
/// let mut pop = Spring::underdamped(0., 5., 0.3);
/// pop.set_target(1.);
/// let mut peak: f32 = 0.;
/// for _ in 0..120 { peak = peak.max(pop.advance(1. / 60., Motion::Full)); }
/// assert!(peak > 1.2); // springy overshoot
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spring {
    pub value: f32,
    pub velocity: f32,
    pub target: f32,
    omega: f32,
    damping: f32,
}

impl Spring {
    /// Spring at rest at `value`. Invalid parameters fall back to 1 Hz / ratio 1.
    pub fn new(value: f32, frequency: f32, damping: f32) -> Self {
        let frequency = if frequency.is_finite() && frequency > 0. {
            frequency
        } else {
            1.
        };
        let damping = if damping.is_finite() && damping > 0. {
            damping
        } else {
            1.
        };
        let value = if value.is_finite() { value } else { 0. };
        Self {
            value,
            velocity: 0.,
            target: value,
            omega: frequency * TAU,
            damping,
        }
    }
    /// Critically damped: settles as fast as possible without overshoot.
    pub fn critical(value: f32, frequency: f32) -> Self {
        Self::new(value, frequency, 1.)
    }
    /// Bouncy spring with damping ratio in `(0, 1)` (e.g. `0.3`–`0.6`).
    pub fn underdamped(value: f32, frequency: f32, damping: f32) -> Self {
        Self::new(value, frequency, damping.clamp(0.01, 0.999))
    }
    pub fn set_target(&mut self, target: f32) {
        if target.is_finite() {
            self.target = target;
        }
    }
    /// Adds instantaneous velocity (a "kick" for pops and shakes).
    pub fn impulse(&mut self, velocity: f32) {
        if velocity.is_finite() {
            self.velocity += velocity;
        }
    }
    /// Places the spring at `value` with no velocity.
    pub fn snap(&mut self, value: f32) {
        if value.is_finite() {
            self.value = value;
            self.target = value;
            self.velocity = 0.;
        }
    }
    /// Within `epsilon` of the target with velocity below `epsilon`.
    pub fn is_settled(&self, epsilon: f32) -> bool {
        (self.value - self.target).abs() <= epsilon && self.velocity.abs() <= epsilon
    }
    /// Advances by `dt` and returns the value. Reduced motion snaps to target.
    pub fn advance(&mut self, dt: f32, motion: Motion) -> f32 {
        if motion.is_reduced() {
            let target = self.target;
            self.snap(target);
            return self.value;
        }
        let t = step(dt);
        if t == 0. {
            return self.value;
        }
        let w = self.omega;
        let z = self.damping;
        let x0 = self.value - self.target;
        let v0 = self.velocity;
        let (x, v) = if (z - 1.).abs() < 1e-4 {
            let e = (-w * t).exp();
            let c = v0 + w * x0;
            ((x0 + c * t) * e, (v0 - w * c * t) * e)
        } else if z < 1. {
            let wd = w * (1. - z * z).sqrt();
            let a = z * w;
            let b = (v0 + a * x0) / wd;
            let e = (-a * t).exp();
            let (sin, cos) = (wd * t).sin_cos();
            (
                e * (x0 * cos + b * sin),
                e * ((b * wd - a * x0) * cos - (a * b + x0 * wd) * sin),
            )
        } else {
            let root = (z * z - 1.).sqrt();
            let r1 = -w * (z - root);
            let r2 = -w * (z + root);
            let c2 = (v0 - r1 * x0) / (r2 - r1);
            let c1 = x0 - c2;
            let (e1, e2) = ((r1 * t).exp(), (r2 * t).exp());
            (c1 * e1 + c2 * e2, r1 * c1 * e1 + r2 * c2 * e2)
        };
        if x.is_finite() && v.is_finite() {
            self.value = self.target + x;
            self.velocity = v;
        } else {
            let target = self.target;
            self.snap(target);
        }
        self.value
    }
}

/// How a staggered UI element appears at one instant: multiply its alpha,
/// translate it by `offset` and scale it about its center by `scale`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Appearance {
    pub alpha: f32,
    pub offset: [f32; 2],
    pub scale: f32,
}

impl Appearance {
    /// Fully shown, in place.
    pub const SHOWN: Self = Self {
        alpha: 1.,
        offset: [0., 0.],
        scale: 1.,
    };
    /// Fully hidden.
    pub const HIDDEN: Self = Self {
        alpha: 0.,
        offset: [0., 0.],
        scale: 1.,
    };
}

/// Fade/slide/scale entrance shared by a list of elements with a per-item
/// stagger. Sample with the time since the list opened; play it backwards
/// (decreasing `elapsed`) for an exit.
///
/// ```
/// use ferravex_play::tween::{Entrance, Motion};
/// let menu = Entrance::fade_up(12.).with_stagger(0.05);
/// let first = menu.sample(0.1, 0, Motion::Full);
/// let third = menu.sample(0.1, 2, Motion::Full);
/// assert!(first.alpha > third.alpha && first.offset[1] < third.offset[1]);
/// assert_eq!(menu.sample(10., 5, Motion::Full).alpha, 1.);
/// // Reduced motion keeps the fade but drops movement and stagger.
/// let calm = menu.sample(0.1, 2, Motion::Reduced);
/// assert_eq!((calm.offset, calm.scale), ([0., 0.], 1.));
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Entrance {
    pub duration: f32,
    /// Extra delay per item index.
    pub stagger: f32,
    pub ease: Ease,
    pub fade: bool,
    /// Starting offset, eased to zero.
    pub slide: [f32; 2],
    /// Starting scale, eased to one.
    pub scale_from: f32,
}

impl Entrance {
    /// Fades in while rising `distance` units (0.28 s, cubic out).
    pub fn fade_up(distance: f32) -> Self {
        Self {
            duration: 0.28,
            stagger: 0.04,
            ease: Ease::CubicOut,
            fade: true,
            slide: [0., distance],
            scale_from: 1.,
        }
    }
    /// Fades in while growing from `scale_from` with a slight overshoot.
    pub fn pop(scale_from: f32) -> Self {
        Self {
            duration: 0.32,
            stagger: 0.04,
            ease: Ease::BackOut,
            fade: true,
            slide: [0., 0.],
            scale_from,
        }
    }
    /// Plain fade.
    pub fn fade() -> Self {
        Self {
            duration: 0.2,
            stagger: 0.03,
            ease: Ease::QuadOut,
            fade: true,
            slide: [0., 0.],
            scale_from: 1.,
        }
    }
    pub fn with_stagger(mut self, seconds: f32) -> Self {
        self.stagger = seconds;
        self
    }
    pub fn with_duration(mut self, seconds: f32) -> Self {
        self.duration = seconds;
        self
    }
    pub fn with_ease(mut self, ease: Ease) -> Self {
        self.ease = ease;
        self
    }
    /// Seconds until item `count - 1` has fully arrived.
    pub fn total_duration(&self, count: usize) -> f32 {
        self.duration.max(0.) + self.stagger.max(0.) * count.saturating_sub(1) as f32
    }
    /// Appearance of item `index` at `elapsed` seconds after the list opened.
    pub fn sample(&self, elapsed: f32, index: usize, motion: Motion) -> Appearance {
        let duration = self.duration.max(1e-6);
        let elapsed = if elapsed.is_finite() { elapsed } else { 0. };
        if motion.is_reduced() {
            let t = (elapsed / duration).clamp(0., 1.);
            return Appearance {
                alpha: if self.fade { t } else { 1. },
                ..Appearance::SHOWN
            };
        }
        let local = elapsed - self.stagger.max(0.) * index as f32;
        let t = (local / duration).clamp(0., 1.);
        let eased = self.ease.apply(t);
        let fade = Ease::QuadOut.apply(t);
        let rest = 1. - eased;
        Appearance {
            alpha: if self.fade { fade } else { 1. },
            offset: [self.slide[0] * rest, self.slide[1] * rest],
            scale: lerp(self.scale_from, 1., eased),
        }
    }
}

/// An integer display that rolls toward its target, like a score counter.
/// Every roll takes `duration` seconds regardless of size (eased, quick start
/// and gentle landing) and the shown number never passes the target.
///
/// ```
/// use ferravex_play::tween::{Motion, RollCounter};
/// let mut score = RollCounter::new(0);
/// score.set_target(1_250);
/// let mut last = 0;
/// for _ in 0..20 {
///     let shown = score.advance(1. / 60., Motion::Full);
///     assert!(shown >= last && shown <= 1_250);
///     last = shown;
/// }
/// assert!(score.is_rolling());
/// for _ in 0..60 { score.advance(1. / 60., Motion::Full); }
/// assert_eq!(score.shown(), 1_250);
/// score.set_target(2_000);
/// assert_eq!(score.advance(0.016, Motion::Reduced), 2_000);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RollCounter {
    from: f64,
    target: i64,
    elapsed: f32,
    /// Seconds per roll (default 0.6).
    pub duration: f32,
    /// Roll curve (default [`Ease::QuartOut`]).
    pub ease: Ease,
}

impl RollCounter {
    pub fn new(value: i64) -> Self {
        Self {
            from: value as f64,
            target: value,
            elapsed: 0.,
            duration: 0.6,
            ease: Ease::QuartOut,
        }
    }
    /// Rolls from the currently shown value toward `target`.
    pub fn set_target(&mut self, target: i64) {
        if target != self.target {
            self.from = self.exact();
            self.target = target;
            self.elapsed = 0.;
        }
    }
    /// Shows `value` immediately.
    pub fn snap(&mut self, value: i64) {
        self.target = value;
        self.from = value as f64;
        self.elapsed = self.duration;
    }
    pub fn target(&self) -> i64 {
        self.target
    }
    fn exact(&self) -> f64 {
        let t = if self.duration > 0. {
            self.elapsed / self.duration
        } else {
            1.
        };
        if t >= 1. {
            return self.target as f64;
        }
        let k = self.ease.apply(t) as f64;
        self.from + (self.target as f64 - self.from) * k
    }
    /// Number to display (rounded toward where the roll started).
    pub fn shown(&self) -> i64 {
        let value = self.exact();
        let shown = if value < self.target as f64 {
            value.floor()
        } else {
            value.ceil()
        };
        shown as i64
    }
    pub fn is_rolling(&self) -> bool {
        self.shown() != self.target
    }
    /// Advances the roll and returns the number to display.
    pub fn advance(&mut self, dt: f32, motion: Motion) -> i64 {
        if motion.is_reduced() {
            self.snap(self.target);
        } else {
            self.elapsed = (self.elapsed + step(dt)).min(self.duration.max(0.));
        }
        self.shown()
    }
}

/// Visual style of a [`ScreenTransition`]. Drawing lives in the renderer
/// (`ferravex_render::ui::kit::draw_screen_transition`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum TransitionKind {
    /// Uniform fade to a color.
    #[default]
    Fade,
    /// A soft-edged band sweeping across the screen in `direction`
    /// (radians, 0 = left to right, Y down).
    Wipe { direction: f32 },
    /// A closing circle around a focus point.
    Iris,
}

/// What a [`ScreenTransition`] is doing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TransitionPhase {
    /// Nothing covers the screen.
    #[default]
    Idle,
    /// Coverage rising toward 1.
    Covering,
    /// Fully covered; swap scenes, then call [`ScreenTransition::reveal`].
    Covered,
    /// Coverage falling toward 0.
    Revealing,
}

/// Completion edge reported once by [`ScreenTransition::advance`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TransitionEvent {
    Covered,
    Revealed,
}

/// Cover → swap → reveal timing for scene changes (wipe, iris or fade).
/// Under reduced motion [`ScreenTransition::kind`] reports [`TransitionKind::Fade`].
///
/// ```
/// use ferravex_play::tween::{Motion, ScreenTransition, TransitionEvent, TransitionKind, TransitionPhase};
/// let mut t = ScreenTransition::new(TransitionKind::Iris, 0.4);
/// t.cover();
/// let mut events = Vec::new();
/// for _ in 0..60 {
///     if let Some(e) = t.advance(1. / 60., Motion::Full) {
///         events.push(e);
///         if e == TransitionEvent::Covered { t.reveal(); } // load the next scene here
///     }
/// }
/// assert_eq!(events, [TransitionEvent::Covered, TransitionEvent::Revealed]);
/// assert_eq!((t.phase(), t.coverage()), (TransitionPhase::Idle, 0.));
/// assert_eq!(t.kind(Motion::Reduced), TransitionKind::Fade);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScreenTransition {
    kind: TransitionKind,
    phase: TransitionPhase,
    duration: f32,
    elapsed: f32,
    pub ease: Ease,
}

impl ScreenTransition {
    /// `duration` is the time for each half (cover and reveal).
    pub fn new(kind: TransitionKind, duration: f32) -> Self {
        Self {
            kind,
            phase: TransitionPhase::Idle,
            duration: if duration.is_finite() {
                duration.max(0.)
            } else {
                0.
            },
            elapsed: 0.,
            ease: Ease::CubicInOut,
        }
    }
    pub fn phase(&self) -> TransitionPhase {
        self.phase
    }
    /// The kind to draw this frame (reduced motion draws a fade).
    pub fn kind(&self, motion: Motion) -> TransitionKind {
        if motion.is_reduced() {
            TransitionKind::Fade
        } else {
            self.kind
        }
    }
    /// Starts covering the screen (from the current coverage).
    pub fn cover(&mut self) {
        if matches!(
            self.phase,
            TransitionPhase::Idle | TransitionPhase::Revealing
        ) {
            self.elapsed = self.duration * self.linear();
            self.phase = TransitionPhase::Covering;
        }
    }
    /// Starts revealing the screen (from the current coverage).
    pub fn reveal(&mut self) {
        if matches!(
            self.phase,
            TransitionPhase::Covered | TransitionPhase::Covering
        ) {
            self.elapsed = self.duration * (1. - self.linear());
            self.phase = TransitionPhase::Revealing;
        }
    }
    fn linear(&self) -> f32 {
        let t = if self.duration > 0. {
            (self.elapsed / self.duration).clamp(0., 1.)
        } else {
            1.
        };
        match self.phase {
            TransitionPhase::Idle => 0.,
            TransitionPhase::Covered => 1.,
            TransitionPhase::Covering => t,
            TransitionPhase::Revealing => 1. - t,
        }
    }
    /// Eased screen coverage: 0 = clear, 1 = fully covered.
    pub fn coverage(&self) -> f32 {
        self.ease.apply(self.linear())
    }
    /// Advances time; returns an event on the frame a half completes.
    /// Reduced motion halves the duration (the fade still reads as a cut).
    pub fn advance(&mut self, dt: f32, motion: Motion) -> Option<TransitionEvent> {
        let scale = if motion.is_reduced() { 2. } else { 1. };
        match self.phase {
            TransitionPhase::Covering | TransitionPhase::Revealing => {
                self.elapsed += step(dt) * scale;
                if self.elapsed < self.duration {
                    return None;
                }
                self.elapsed = 0.;
                if self.phase == TransitionPhase::Covering {
                    self.phase = TransitionPhase::Covered;
                    Some(TransitionEvent::Covered)
                } else {
                    self.phase = TransitionPhase::Idle;
                    Some(TransitionEvent::Revealed)
                }
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_ease_has_exact_endpoints_and_is_finite_and_continuous() {
        for ease in Ease::ALL {
            assert_eq!(ease.apply(0.), 0., "{ease:?}");
            assert_eq!(ease.apply(1.), 1., "{ease:?}");
            assert_eq!(ease.apply(-3.), 0.);
            assert_eq!(ease.apply(f32::NAN), 0.);
            assert_eq!(ease.apply(7.), 1.);
            let mut prev = 0.;
            for i in 1..=1000 {
                let v = ease.apply(i as f32 / 1000.);
                assert!(v.is_finite());
                assert!(
                    (v - prev).abs() < 0.05,
                    "{ease:?} jumps at {i}: {prev} -> {v}"
                );
                prev = v;
            }
        }
        // Symmetry of in/out pairs.
        for (a, b) in [
            (Ease::QuadIn, Ease::QuadOut),
            (Ease::CubicIn, Ease::CubicOut),
            (Ease::ExpoIn, Ease::ExpoOut),
            (Ease::BackIn, Ease::BackOut),
            (Ease::BounceIn, Ease::BounceOut),
        ] {
            for i in 0..=20 {
                let t = i as f32 / 20.;
                assert!(
                    (a.apply(t) - (1. - b.apply(1. - t))).abs() < 1e-4,
                    "{a:?} {t}"
                );
            }
        }
        for t in [0.25, 0.5, 0.75] {
            assert!((Ease::QuadInOut.apply(t) + Ease::QuadInOut.apply(1. - t) - 1.).abs() < 1e-6);
        }
        assert!((0..=100).all(|i| (0. ..=1.).contains(&Ease::BounceOut.apply(i as f32 / 100.))));
    }

    #[test]
    fn tween_endpoints_delay_retarget_and_bad_input() {
        let mut t = Tween::new(2., 10., 1., Ease::QuadOut).delayed(0.5);
        assert_eq!(t.value(), 2.);
        t.advance(0.5, Motion::Full);
        assert_eq!(t.value(), 2.);
        t.advance(f32::NAN, Motion::Full);
        t.advance(-1., Motion::Full);
        assert_eq!(t.progress(), 0.);
        t.advance(0.5, Motion::Full);
        assert_eq!(t.value(), lerp(2., 10., Ease::QuadOut.apply(0.5)));
        t.advance(100., Motion::Full);
        assert_eq!((t.value(), t.is_finished()), (10., true));
        t.retarget(0.);
        assert_eq!(t.value(), 10.);
        t.advance(0.5, Motion::Full);
        assert!(t.value() < 10. && t.value() > 0.);
        assert_eq!(Tween::new(1., 3., f32::NAN, Ease::Linear).value(), 3.);
        // Splitting time gives the same value as one step.
        let mut a = Tween::new(0., 1., 1., Ease::ElasticOut);
        let mut b = a;
        a.advance(0.6, Motion::Full);
        for _ in 0..6 {
            b.advance(0.1, Motion::Full);
        }
        assert!((a.value() - b.value()).abs() < 1e-5);
    }

    #[test]
    fn springs_converge_split_steps_and_respect_damping() {
        for damping in [0.2, 0.5, 1.0, 2.5] {
            let mut a = Spring::new(0., 3., damping);
            a.set_target(1.);
            let mut b = a;
            let mut peak: f32 = 0.;
            for _ in 0..600 {
                peak = peak.max(a.advance(1. / 120., Motion::Full));
            }
            for _ in 0..300 {
                b.advance(1. / 60., Motion::Full);
            }
            assert!(a.is_settled(1e-3), "{damping}: {a:?}");
            assert!((a.value - b.value).abs() < 1e-4);
            if damping >= 1. {
                assert!(peak <= 1. + 1e-5, "{damping} overshoots {peak}");
            } else {
                assert!(peak > 1.01, "{damping} should overshoot");
            }
        }
        let mut s = Spring::critical(0., 2.);
        s.set_target(5.);
        assert_eq!(s.advance(1., Motion::Reduced), 5.);
        s.impulse(f32::NAN);
        s.advance(1e30, Motion::Full);
        assert!(s.value.is_finite());
    }

    #[test]
    fn entrance_roll_and_screen_transition_edges() {
        let e = Entrance::pop(0.8);
        let a = e.sample(0., 0, Motion::Full);
        assert_eq!((a.alpha, a.scale), (0., 0.8));
        assert_eq!(
            e.sample(e.total_duration(4), 3, Motion::Full),
            Appearance::SHOWN
        );
        assert_eq!(e.sample(f32::NAN, 0, Motion::Full).alpha, 0.);

        let mut r = RollCounter::new(100);
        r.set_target(40);
        let mut prev = 100;
        while r.is_rolling() {
            let v = r.advance(1. / 60., Motion::Full);
            assert!(v <= prev && v >= 40);
            prev = v;
        }
        assert_eq!(r.shown(), 40);
        let mut big = RollCounter::new(0);
        big.set_target(1_000_000_000);
        let mut frames = 0;
        while big.is_rolling() {
            big.advance(1. / 60., Motion::Full);
            frames += 1;
        }
        assert!(frames < 120, "{frames}");

        let mut t = ScreenTransition::new(TransitionKind::Wipe { direction: 0. }, 0.5);
        assert_eq!(t.advance(1., Motion::Full), None);
        t.cover();
        t.advance(0.25, Motion::Full);
        let half = t.coverage();
        assert!(half > 0. && half < 1.);
        t.reveal(); // reverse mid-way from the same coverage
        assert!((t.coverage() - half).abs() < 1e-5);
        assert_eq!(
            t.advance(0.3, Motion::Full),
            Some(TransitionEvent::Revealed)
        );
        t.cover();
        assert_eq!(
            t.advance(0.25, Motion::Reduced),
            Some(TransitionEvent::Covered)
        );
        assert_eq!(t.coverage(), 1.);
    }
}
