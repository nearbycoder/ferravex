//! Platform-free, fixed-tick golf rules with bounded collision substeps.
use crate::course::{self, Hole, RADIUS, SIZE, Surface};
use ferravex_math::{Vec2, vec2};
pub const DT: f32 = 1. / 120.;
pub const MAX_STROKES: u8 = 12;
const DRAG: f32 = 1.8;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Aiming,
    Rolling,
    Water,
    Holed,
    Finished,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Events {
    pub bank: bool,
    pub water: bool,
    pub holed: bool,
    pub stopped: bool,
    /// Fastest approach speed of an accepted wall or rail contact, in cm/s.
    pub impact_cms: u16,
    /// A moving copper barrier struck the ball this tick.
    pub moving_hit: bool,
    /// The ball crossed the cup too fast to drop. At most once per stroke.
    pub lip_out: bool,
    /// The ball rolled onto sand this tick.
    pub sand: bool,
}
#[derive(Clone, Debug)]
pub struct Round {
    pub hole_index: usize,
    pub course: Hole,
    pub ball: Vec2,
    pub velocity: Vec2,
    pub phase: Phase,
    pub strokes: u8,
    pub scores: [Option<u8>; 12],
    pub practice: bool,
    pub time: f32,
    pub last_safe: Vec2,
    pub penalties: u8,
    pub picked_up: [bool; 12],
    /// Distinct bank contacts since the current stroke began.
    pub stroke_banks: u8,
    /// A moving copper barrier touched the ball since the current stroke began.
    pub stroke_assist: bool,
    water_timer: f32,
    route_index: usize,
    stroke_lipped: bool,
    last_bank: f32,
    in_sand: bool,
}
impl Default for Round {
    fn default() -> Self {
        Self::new()
    }
}
impl Round {
    pub fn new() -> Self {
        let c = course::hole(0);
        Self {
            ball: c.tee,
            last_safe: c.tee,
            course: c,
            hole_index: 0,
            velocity: Vec2::ZERO,
            phase: Phase::Aiming,
            strokes: 0,
            scores: [None; 12],
            practice: false,
            time: 0.,
            penalties: 0,
            picked_up: [false; 12],
            stroke_banks: 0,
            stroke_assist: false,
            water_timer: 0.,
            route_index: 0,
            stroke_lipped: false,
            last_bank: f32::NEG_INFINITY,
            in_sand: false,
        }
    }
    pub fn load(&mut self, index: usize) {
        self.hole_index = index.min(11);
        self.course = course::hole(self.hole_index);
        self.ball = self.course.tee;
        self.last_safe = self.ball;
        self.velocity = Vec2::ZERO;
        self.phase = Phase::Aiming;
        self.strokes = 0;
        self.penalties = 0;
        self.time = 0.;
        self.water_timer = 0.;
        self.route_index = 0;
        self.in_sand = false;
        self.begin_stroke();
    }
    fn begin_stroke(&mut self) {
        self.stroke_banks = 0;
        self.stroke_assist = false;
        self.stroke_lipped = false;
        self.last_bank = f32::NEG_INFINITY;
    }
    pub fn practice_hole(&mut self, index: usize) {
        *self = Self::new();
        self.practice = true;
        self.load(index);
    }
    pub fn total(&self) -> u16 {
        self.scores.iter().flatten().map(|&v| u16::from(v)).sum()
    }
    pub fn par_played(&self) -> u16 {
        self.scores
            .iter()
            .enumerate()
            .filter(|(_, s)| s.is_some())
            .map(|(i, _)| u16::from(course::hole(i).par))
            .sum()
    }
    /// Reject invalid/late shots without changing the ball, strokes or safe lie.
    pub fn shoot(&mut self, direction: Vec2, power: f32) -> bool {
        if self.phase != Phase::Aiming
            || !direction.is_finite()
            || direction.length_squared() < 0.0001
            || !power.is_finite()
            || !(0.05..=1.).contains(&power)
        {
            return false;
        }
        self.last_safe = self.ball;
        self.velocity = direction.normalize() * (power * 11.5);
        self.strokes += 1;
        self.phase = Phase::Rolling;
        self.begin_stroke();
        true
    }
    pub fn drop_ball(&mut self) -> bool {
        if matches!(self.phase, Phase::Holed | Phase::Finished | Phase::Water) {
            return false;
        }
        self.strokes = (self.strokes + 1).min(MAX_STROKES);
        self.penalties += 1;
        self.ball = self.last_safe;
        self.velocity = Vec2::ZERO;
        self.phase = Phase::Aiming;
        self.check_cap();
        true
    }
    pub fn pick_up(&mut self) {
        if matches!(self.phase, Phase::Aiming | Phase::Rolling) {
            self.strokes = MAX_STROKES;
            self.picked_up[self.hole_index] = true;
            self.finish_hole();
        }
    }
    fn check_cap(&mut self) {
        if self.strokes >= MAX_STROKES {
            self.picked_up[self.hole_index] = true;
            self.finish_hole();
        }
    }
    fn finish_hole(&mut self) {
        self.velocity = Vec2::ZERO;
        self.scores[self.hole_index] = Some(self.strokes);
        self.phase = Phase::Holed;
    }
    pub fn next_hole(&mut self) {
        if self.phase != Phase::Holed {
            return;
        }
        if self.hole_index == 11 {
            self.phase = Phase::Finished
        } else {
            self.load(self.hole_index + 1)
        }
    }
    /// Execute exactly one 1/120 second step. Presentation never calls with frame dt.
    pub fn tick(&mut self) -> Events {
        let mut e = Events::default();
        self.time += DT;
        if self.phase == Phase::Water {
            self.water_timer -= DT;
            if self.water_timer <= 0. {
                self.ball = self.last_safe;
                self.phase = Phase::Aiming;
                self.check_cap();
                e.stopped = true;
            }
            return e;
        }
        // A moving barrier can nudge a resting ball. That is physics, not an
        // extra putt; it must not leave the ball embedded in a rotating gate.
        if self.phase == Phase::Aiming
            && self.course.walls.iter().any(|w| {
                if w.spin == 0. && w.travel == Vec2::ZERO {
                    return false;
                }
                let (center, angle) = w.pose(self.time);
                let local = Vec2::from_angle(-angle).rotate(self.ball - center);
                local.distance(local.clamp(-w.half, w.half)) < RADIUS
            })
        {
            self.phase = Phase::Rolling;
        }
        if self.phase != Phase::Rolling {
            return e;
        }
        for sub in 0..4 {
            let dt = DT / 4.;
            let time = self.time - DT + (sub + 1) as f32 * dt;
            let sand = self
                .course
                .patches
                .iter()
                .any(|p| p.surface == Surface::Sand && p.contains(self.ball));
            if sand && !self.in_sand {
                e.sand = true;
            }
            self.in_sand = sand;
            let speed = self.velocity.length();
            let drag = if sand { 7.2 } else { DRAG };
            // Static friction holds a resting ball. A moving ball responds to slopes.
            self.velocity *= ((speed - drag * dt).max(0.)) / speed.max(0.00001);
            if speed > 0.06 {
                self.velocity -= self.course.slope(self.ball) * (5.8 * dt);
            }
            self.ball += self.velocity * dt;
            for axis in 0..2 {
                let v = if axis == 0 { self.ball.x } else { self.ball.y };
                let hi = if axis == 0 { SIZE.x } else { SIZE.y };
                if v < RADIUS || v > hi - RADIUS {
                    let approach = if axis == 0 {
                        self.ball.x = v.clamp(RADIUS, hi - RADIUS);
                        let approach = self.velocity.x.abs();
                        self.velocity.x *= -0.76;
                        approach
                    } else {
                        self.ball.y = v.clamp(RADIUS, hi - RADIUS);
                        let approach = self.velocity.y.abs();
                        self.velocity.y *= -0.76;
                        approach
                    };
                    self.contact(&mut e, approach, time, false);
                }
            }
            for index in 0..self.course.walls.len() {
                let w = self.course.walls[index];
                let (center, angle) = w.pose(time);
                let r = Vec2::from_angle(-angle);
                let local = r.rotate(self.ball - center);
                let nearest = local.clamp(-w.half, w.half);
                let delta = local - nearest;
                let d = delta.length();
                if d < RADIUS {
                    let (n, depth) = if d > 0.00001 {
                        (delta / d, RADIUS - d)
                    } else {
                        let gap = w.half - local.abs();
                        if gap.x < gap.y {
                            (
                                vec2(if local.x < 0. { -1. } else { 1. }, 0.),
                                RADIUS + gap.x,
                            )
                        } else {
                            (
                                vec2(0., if local.y < 0. { -1. } else { 1. }),
                                RADIUS + gap.y,
                            )
                        }
                    };
                    let n = Vec2::from_angle(angle).rotate(n);
                    self.ball += n * (depth + 0.0001);
                    let arm = self.ball - center;
                    let wall_v =
                        w.travel * (0.9 * (time * 0.9).cos()) + vec2(-arm.y, arm.x) * w.spin;
                    let into = (self.velocity - wall_v).dot(n);
                    if into < 0. {
                        self.velocity -= n * (1.72 * into);
                        self.velocity = self.velocity.clamp_length_max(15.);
                        let moving = w.spin != 0. || w.travel != Vec2::ZERO;
                        self.contact(&mut e, -into, time, moving);
                    }
                }
            }
            if self
                .course
                .patches
                .iter()
                .any(|p| p.surface == Surface::Water && p.contains(self.ball))
            {
                self.phase = Phase::Water;
                self.velocity = Vec2::ZERO;
                self.strokes = (self.strokes + 1).min(MAX_STROKES);
                self.penalties += 1;
                self.water_timer = 0.9;
                e.water = true;
                return e;
            }
            let to = self.course.cup - self.ball;
            let dist = to.length();
            if dist < 0.37 && self.velocity.length() < 3.7 {
                self.ball = self.course.cup;
                self.finish_hole();
                e.holed = true;
                return e;
            }
            if dist < 0.37 && !self.stroke_lipped {
                self.stroke_lipped = true;
                e.lip_out = true;
            }
            if dist < 0.85 && self.velocity.length() < 2.4 {
                self.velocity += to.normalize_or_zero() * (3.2 * dt);
            }
        }
        if self.velocity.length() < 0.055 {
            self.velocity = Vec2::ZERO;
            self.phase = Phase::Aiming;
            self.last_safe = self.ball;
            self.check_cap();
            e.stopped = true;
        }
        e
    }
    /// Report one accepted contact. Rattles within 0.1 s count as one bank.
    fn contact(&mut self, e: &mut Events, approach: f32, time: f32, moving: bool) {
        e.bank = true;
        e.impact_cms = e
            .impact_cms
            .max((approach * 100.).clamp(0., f32::from(u16::MAX)) as u16);
        e.moving_hit |= moving;
        self.stroke_assist |= moving;
        if time - self.last_bank > 0.1 {
            self.stroke_banks = self.stroke_banks.saturating_add(1);
        }
        self.last_bank = time;
    }
    /// Legal demonstration shot: bounded prediction of the real simulation toward
    /// authored route points. It never places the ball or awards a score directly.
    pub fn suggested_shot(&mut self) -> (Vec2, f32) {
        while self.route_index + 1 < self.course.route.len()
            && self.ball.distance(self.course.route[self.route_index]) < 0.95
        {
            self.route_index += 1;
        }
        let target = self.course.route[self.route_index];
        let offset = target - self.ball;
        let angle = offset.to_angle();
        let base = (2. * DRAG * offset.length()).sqrt() / 11.5;
        let mut best = (f32::INFINITY, Vec2::from_angle(angle), base.clamp(0.06, 1.));
        for a in [-0.15, -0.075, 0., 0.075, 0.15] {
            for factor in [0.72, 0.82, 0.9, 0.96, 1., 1.05, 1.12, 1.22, 1.4] {
                let power = (base * factor).clamp(0.06, 1.);
                let dir = Vec2::from_angle(angle + a);
                let mut trial = self.clone();
                trial.shoot(dir, power);
                for _ in 0..1080 {
                    trial.tick();
                    if trial.phase != Phase::Rolling {
                        break;
                    }
                }
                let cost = if trial.phase == Phase::Holed && !trial.picked_up[trial.hole_index] {
                    -100.
                } else if trial.phase == Phase::Water {
                    1000.
                } else {
                    trial.ball.distance(target)
                };
                if cost < best.0 {
                    best = (cost, dir, power);
                }
            }
        }
        (best.1, best.2)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn twelve_distinct_courses() {
        let mut names = std::collections::BTreeSet::new();
        for i in 0..12 {
            let h = course::hole(i);
            assert!(names.insert(h.name));
            assert!(h.tee.distance(h.cup) > 4.);
            assert_eq!(h.route.last(), Some(&h.cup));
        }
    }
    #[test]
    fn invalid_and_rolling_shots_are_atomic() {
        let mut r = Round::new();
        assert!(!r.shoot(Vec2::ZERO, 0.5));
        assert!(!r.shoot(vec2(1., 0.), f32::NAN));
        assert_eq!(r.strokes, 0);
        assert!(r.shoot(vec2(1., 0.), 0.6));
        let before = r.clone();
        assert!(!r.shoot(vec2(1., 0.), 0.8));
        assert_eq!(r.velocity, before.velocity);
        assert_eq!(r.strokes, 1);
    }
    #[test]
    fn water_returns_to_safe_lie_and_costs_one() {
        let mut r = Round::new();
        r.load(3);
        assert!(r.shoot(vec2(1., -0.5), 0.85));
        for _ in 0..2000 {
            r.tick();
            if r.phase != Phase::Rolling && r.phase != Phase::Water {
                break;
            }
        }
        assert_eq!(r.penalties, 1);
        assert_eq!(r.strokes, 2);
        assert_eq!(r.ball, r.course.tee);
    }
    #[test]
    fn bank_never_tunnels() {
        let mut r = Round::new();
        r.load(1);
        r.shoot(vec2(1., 0.), 1.);
        let mut hit = false;
        for _ in 0..500 {
            hit |= r.tick().bank;
            assert!(r.ball.x < 8.57);
        }
        assert!(hit);
    }
    #[test]
    fn every_course_can_be_legally_holed() {
        for i in 0..12 {
            let mut r = Round::new();
            r.load(i);
            for _ in 0..MAX_STROKES {
                if r.phase == Phase::Holed {
                    break;
                }
                let (d, p) = r.suggested_shot();
                assert!(r.shoot(d, p));
                for _ in 0..2400 {
                    r.tick();
                    if r.phase == Phase::Aiming || r.phase == Phase::Holed {
                        break;
                    }
                }
            }
            assert_eq!(r.phase, Phase::Holed, "{} {:?}", r.course.name, r.ball);
            assert!(
                !r.picked_up[i],
                "{} picked up at {:?}",
                r.course.name, r.ball
            );
        }
    }
    #[test]
    fn next_is_gated_and_score_is_retained() {
        let mut r = Round::new();
        r.next_hole();
        assert_eq!(r.hole_index, 0);
        r.pick_up();
        assert_eq!(r.scores[0], Some(MAX_STROKES));
        r.next_hole();
        assert_eq!(r.hole_index, 1);
        assert_eq!(r.total(), 12);
        assert_eq!(r.strokes, 0);
    }
}

#[cfg(test)]
mod session_regressions {
    use super::*;
    use ferravex_core::session::GameSession;
    #[test]
    fn display_rates_and_pauses_preserve_actual_ball_state() {
        let simulate = |hz: f32| {
            let mut r = Round::new();
            let mut session = GameSession::new(120., 12, true);
            assert!(r.shoot(Vec2::X, 0.44));
            for frame in 0..(hz as usize * 5) {
                if frame == hz as usize {
                    let before = (r.ball, r.velocity, r.time);
                    session.set_paused(true);
                    assert_eq!(session.advance(20.).steps, 0);
                    assert_eq!((r.ball, r.velocity, r.time), before);
                    session.set_paused(false);
                }
                for _ in 0..session.advance(1. / f64::from(hz)).steps {
                    r.tick();
                }
            }
            (r.ball, r.velocity, r.phase, r.strokes)
        };
        let baseline = simulate(60.);
        for hz in [30., 144.] {
            let other = simulate(hz);
            assert!(baseline.0.distance(other.0) < 0.001);
            assert_eq!(baseline.2, other.2);
            assert_eq!(baseline.3, other.3);
        }
    }
    #[test]
    fn full_round_records_twelve_earned_scores() {
        let mut r = Round::new();
        for i in 0..12 {
            assert_eq!(r.hole_index, i);
            while r.phase != Phase::Holed {
                let (dir, power) = r.suggested_shot();
                assert!(r.shoot(dir, power));
                for _ in 0..2400 {
                    r.tick();
                    if matches!(r.phase, Phase::Aiming | Phase::Holed) {
                        break;
                    }
                }
            }
            assert!(!r.picked_up[i]);
            r.next_hole();
        }
        assert_eq!(r.phase, Phase::Finished);
        assert!(r.scores.iter().all(Option::is_some));
        assert_eq!(r.par_played(), 40);
        assert!(r.total() < 80);
    }
    #[test]
    fn bunker_changes_rolling_distance() {
        let mut sand = Round::new();
        sand.course
            .patches
            .push(course::Patch::new(0., 0., 18., 12., Surface::Sand));
        let mut grass = Round::new();
        sand.shoot(Vec2::X, 0.4);
        grass.shoot(Vec2::X, 0.4);
        for _ in 0..800 {
            sand.tick();
            grass.tick();
        }
        assert!(sand.ball.x + 2. < grass.ball.x);
    }
    #[test]
    fn practice_and_restart_do_not_reuse_completed_scores() {
        let mut r = Round::new();
        r.pick_up();
        r.practice_hole(10);
        assert!(r.practice);
        assert!(r.scores.iter().all(Option::is_none));
        assert_eq!(r.hole_index, 10);
        assert_eq!(r.strokes, 0);
        r = Round::new();
        assert!(!r.practice);
        assert_eq!(r.hole_index, 0);
    }
}

#[cfg(test)]
mod moving_contact_regression {
    use super::*;
    #[test]
    fn windmill_nudges_a_resting_ball_without_adding_a_stroke() {
        let mut r = Round::new();
        r.load(5);
        r.ball = vec2(8.6, 7.5);
        r.last_safe = r.ball;
        r.tick();
        assert_eq!(r.strokes, 0);
        assert!(r.ball.x < 8.6);
        assert!(r.velocity.x < 0.);
        assert_eq!(r.phase, Phase::Rolling);
    }
}

#[cfg(test)]
mod style_event_regressions {
    use super::*;
    fn roll(r: &mut Round) -> Vec<Events> {
        let mut events = Vec::new();
        for _ in 0..2400 {
            events.push(r.tick());
            if !matches!(r.phase, Phase::Rolling | Phase::Water) {
                break;
            }
        }
        events
    }
    #[test]
    fn a_hot_putt_lips_out_once_and_never_drops() {
        let mut r = Round::new();
        assert!(r.shoot(Vec2::X, 0.9));
        let events = roll(&mut r);
        assert_eq!(events.iter().filter(|e| e.lip_out).count(), 1);
        assert!(events.iter().all(|e| !e.holed));
        assert_eq!(r.phase, Phase::Aiming);
        // A fresh stroke re-arms the lip-out callout.
        let before = r.clone();
        assert!(r.shoot(-Vec2::X, 0.05));
        assert!(!r.stroke_lipped && r.stroke_banks == 0 && !r.stroke_assist);
        assert_eq!(r.strokes, before.strokes + 1);
    }
    #[test]
    fn banks_count_distinct_contacts_with_impact_strength() {
        let mut r = Round::new();
        r.load(1);
        assert!(r.shoot(Vec2::X, 1.));
        let events = roll(&mut r);
        let contacts = events.iter().filter(|e| e.bank).count();
        assert!(r.stroke_banks >= 2, "wall and rail: {}", r.stroke_banks);
        assert!(usize::from(r.stroke_banks) <= contacts);
        let hardest = events.iter().map(|e| e.impact_cms).max().unwrap();
        assert!(hardest > 500, "a full-power wall hit is hard: {hardest}");
        assert!(events.iter().all(|e| !e.moving_hit), "stone is not copper");
        assert!(!r.stroke_assist);
    }
    #[test]
    fn copper_contact_marks_the_hidden_assist() {
        let mut r = Round::new();
        r.load(5);
        r.ball = vec2(8.6, 7.5);
        r.last_safe = r.ball;
        let e = r.tick();
        assert!(e.moving_hit && e.bank);
        assert!(r.stroke_assist);
    }
    #[test]
    fn entering_a_bunker_reports_sand_once() {
        let mut r = Round::new();
        r.load(2);
        assert!(r.shoot(Vec2::X, 0.6));
        let events = roll(&mut r);
        assert_eq!(events.iter().filter(|e| e.sand).count(), 1);
    }
    #[test]
    fn style_counters_do_not_change_the_ball() {
        // Event bookkeeping is observation only: the same putt from a clone
        // with stale counters follows the identical path.
        let mut a = Round::new();
        a.load(1);
        let mut b = a.clone();
        b.stroke_banks = 9;
        b.stroke_assist = true;
        assert!(a.shoot(vec2(1., 0.3), 0.8) && b.shoot(vec2(1., 0.3), 0.8));
        for _ in 0..900 {
            a.tick();
            b.tick();
            assert_eq!((a.ball, a.velocity, a.phase), (b.ball, b.velocity, b.phase));
        }
    }
}
