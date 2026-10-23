//! Task 163: authored-track scenarios through the real session and vehicle.
//! No fixture assigns a pose, lap, gate, timer, record, or private driver state.

use crate::{
    session::{Notice, RaceSession},
    sim::{Controls, LAPS, Pose, ROAD_HALF, Race, Track},
};
use ferravex_core::{FrameStep, session::SessionState};
use ferravex_play::persistence::{RecordDelta, run_micros};
use ferravex_render::prelude::Vec2;

const TICK: f64 = 1. / 120.;
const COUNTDOWN_TICKS: u64 = 360;
const GATES_PER_LAP: usize = 16;
const RACE_SECONDS: usize = 120;
type PoseValue = (Vec2, f32);

fn pose_value(pose: Pose) -> PoseValue {
    (pose.position, pose.angle)
}

fn ghost_values(race: &Race) -> Vec<PoseValue> {
    race.best_ghost.iter().copied().map(pose_value).collect()
}

/// Observable race state, excluding the clock's already-budgeted batch counter.
#[derive(Debug, PartialEq)]
struct State {
    pose: PoseValue,
    velocity: Vec2,
    offroad: bool,
    sliding: bool,
    ticks: u64,
    laps: u32,
    next_gate: usize,
    checkpoints: usize,
    lap_time: f32,
    total_time: f32,
    best: Option<f32>,
    last_lap: Option<f32>,
    comparison: Option<RecordDelta>,
    ghost: Option<PoseValue>,
    best_ghost: Vec<PoseValue>,
    countdown: u32,
    notice: Option<Notice>,
    off_track: bool,
    reversing: bool,
}

#[derive(Debug, PartialEq)]
struct GateVisit {
    lap: u32,
    gate: usize,
    tick: u64,
    pose: PoseValue,
    total_time: f32,
}

#[derive(Debug, PartialEq)]
struct LapResult {
    ticks: u64,
    time: f32,
    best: f32,
    comparison: RecordDelta,
    ghost: Vec<PoseValue>,
}

struct Scenario {
    track: Track,
    race: Race,
    session: RaceSession,
    gates: Vec<GateVisit>,
    laps: Vec<LapResult>,
    lap_start_tick: u64,
    recorded_poses: Vec<PoseValue>,
    countdown_steps: u64,
    terminal_steps: u64,
    zero_frames: usize,
    catch_up_frames: usize,
}

impl Scenario {
    fn new() -> Self {
        let track = Track::new();
        let race = Race::new(&track);
        let recorded_poses = vec![pose_value(race.car.pose)];
        let session = RaceSession::new();
        assert_eq!(session.clock.state(), SessionState::Playing);
        assert_eq!(session.countdown(), 3); // The real host starts automatically.
        Self {
            track,
            race,
            session,
            gates: Vec::new(),
            laps: Vec::new(),
            lap_start_tick: 0,
            recorded_poses,
            countdown_steps: 0,
            terminal_steps: 0,
            zero_frames: 0,
            catch_up_frames: 0,
        }
    }

    fn state(&self) -> State {
        State {
            pose: pose_value(self.race.car.pose),
            velocity: self.race.car.velocity,
            offroad: self.race.car.offroad,
            sliding: self.race.car.sliding,
            ticks: self.race.ticks,
            laps: self.race.laps,
            next_gate: self.race.next_gate,
            checkpoints: self.race.checkpoints_passed(),
            lap_time: self.race.lap_time,
            total_time: self.race.total_time,
            best: self.race.best_time,
            last_lap: self.race.last_lap,
            comparison: self.race.last_comparison,
            ghost: self.race.ghost().map(pose_value),
            best_ghost: ghost_values(&self.race),
            countdown: self.session.countdown(),
            notice: self.session.notice,
            off_track: self.session.off_track(),
            reversing: self.race.driver_reversing(),
        }
    }

    /// `None` selects the production autopilot once per actual driving tick;
    /// `Some` supplies held controls, sampled once for the whole host frame.
    fn frame(&mut self, elapsed: f64, active: bool, held: Option<Controls>) -> FrameStep {
        let frame = self.session.advance(elapsed, active);
        assert!(frame.steps <= 8);
        assert_eq!(frame.dt, TICK as f32);
        if self.session.clock.is_playing() {
            self.zero_frames += usize::from(frame.steps == 0);
            self.catch_up_frames += usize::from(frame.steps > 1);
        }
        for _ in 0..frame.steps {
            let controls = held.unwrap_or_else(|| {
                if self.session.countdown() == 0 && !self.race.finished() {
                    self.race.autopilot(&self.track)
                } else {
                    Controls::default()
                }
            });
            self.step(controls, frame.dt);
        }
        frame
    }

    fn step(&mut self, controls: Controls, dt: f32) {
        let before = self.race.car.pose.position;
        let (lap, gate, tick, best) = (
            self.race.laps,
            self.race.next_gate,
            self.race.ticks,
            self.race.best_time,
        );
        let countdown = self.session.countdown();
        let advanced = self.session.step(&mut self.race, &self.track, controls, dt);
        if countdown > 0 {
            self.countdown_steps += 1;
            assert!(!advanced);
            assert_eq!(self.race.ticks, 0);
            assert_eq!(self.race.total_time, 0.);
            assert_eq!(self.race.car.pose.position, self.track.points[0]);
            assert_eq!(self.race.car.velocity, Vec2::ZERO);
            assert_eq!(
                self.session.countdown(),
                (COUNTDOWN_TICKS - self.countdown_steps).div_ceil(120) as u32
            );
            if self.countdown_steps == COUNTDOWN_TICKS {
                assert_eq!(self.session.notice, Some(Notice::Go));
            }
            return;
        }
        if lap == LAPS {
            self.terminal_steps += 1;
            assert!(!advanced);
            assert_eq!(self.race.ticks, tick);
            return;
        }
        assert!(advanced);
        assert_eq!(self.countdown_steps, COUNTDOWN_TICKS);
        assert_eq!(self.race.ticks, tick + 1);
        assert!(self.race.car.pose.position.is_finite());
        assert!(self.race.car.velocity.is_finite());
        let lap_ticks = self.race.ticks - self.lap_start_tick;
        if lap_ticks.is_multiple_of(6) {
            assert!(self.recorded_poses.len() < 6000);
            self.recorded_poses.push(pose_value(self.race.car.pose));
        }
        if self.race.next_gate == gate {
            assert_eq!(self.race.laps, lap);
            return;
        }

        // Check the real trajectory at every accepted gate, not just counters
        // after finishing. Every lap must visit 1..15 then 0 in order.
        assert!(self.gates.len() < LAPS as usize * GATES_PER_LAP);
        assert_eq!(gate, (self.gates.len() + 1) % GATES_PER_LAP);
        assert_eq!(lap as usize, self.gates.len() / GATES_PER_LAP);
        assert_eq!(self.race.next_gate, (gate + 1) % GATES_PER_LAP);
        let (center, tangent) = self.track.gate(gate);
        let after = self.race.car.pose.position;
        let from_side = (before - center).dot(tangent);
        let to_side = (after - center).dot(tangent);
        assert!(from_side < 0. && to_side >= 0.);
        let intersection = before.lerp(after, -from_side / (to_side - from_side));
        assert!(intersection.distance(center) <= ROAD_HALF + 4.);
        self.gates.push(GateVisit {
            lap: lap + 1,
            gate,
            tick: self.race.ticks,
            pose: pose_value(self.race.car.pose),
            total_time: self.race.total_time,
        });
        if gate != 0 {
            assert_eq!(self.race.laps, lap);
            assert_eq!(self.race.checkpoints_passed(), gate);
            assert_eq!(self.session.notice, Some(Notice::Checkpoint(gate)));
            return;
        }

        assert_eq!(self.race.laps, lap + 1);
        assert_eq!(self.race.lap_time, 0.);
        let time = self.race.last_lap.expect("accepted lap time");
        assert!((f64::from(time) - lap_ticks as f64 * TICK).abs() < 0.01);
        let comparison = self.race.last_comparison.expect("accepted lap comparison");
        assert_eq!(comparison.previous, best.and_then(run_micros));
        assert_eq!(comparison.value, run_micros(time).unwrap());
        assert_eq!(
            comparison.delta,
            comparison
                .previous
                .map(|previous| i128::from(comparison.value) - i128::from(previous))
        );
        let new_best = best.is_none_or(|previous| time < previous);
        assert_eq!(
            self.race.best_time,
            Some(best.map_or(time, |previous| previous.min(time)))
        );
        let ghost = ghost_values(&self.race);
        if new_best {
            assert_eq!(
                ghost, self.recorded_poses,
                "best ghost must contain the actual lap's 20 Hz poses"
            );
        } else if let Some(previous) = self.laps.last() {
            assert_eq!(ghost, previous.ghost);
        }
        assert_eq!(
            self.session.notice,
            Some(Notice::Lap {
                number: lap + 1,
                time,
                best: new_best,
                comparison: Some(comparison)
            })
        );
        assert!(self.laps.len() < LAPS as usize);
        self.laps.push(LapResult {
            ticks: lap_ticks,
            time,
            best: self.race.best_time.unwrap(),
            comparison,
            ghost,
        });
        self.lap_start_tick = self.race.ticks;
        self.recorded_poses.clear();
        self.recorded_poses.push(pose_value(self.race.car.pose));
    }

    fn run_seconds(&mut self, fps: usize, seconds: usize) {
        for _ in 0..fps * seconds {
            assert_eq!(self.frame(1. / fps as f64, true, None).dropped_seconds, 0.);
        }
    }

    fn assert_finished(&self) {
        assert!(
            self.race.finished(),
            "only {} laps / gate {} after {} ticks",
            self.race.laps,
            self.race.next_gate,
            self.race.ticks
        );
        assert_eq!(self.session.clock.state(), SessionState::Won);
        assert_eq!(self.laps.len(), LAPS as usize);
        assert_eq!(self.gates.len(), LAPS as usize * GATES_PER_LAP);
        assert_eq!(self.race.checkpoints_passed(), GATES_PER_LAP);
        assert_eq!(
            self.laps.iter().map(|lap| lap.ticks).sum::<u64>(),
            self.race.ticks
        );
        assert!(self.race.total_time > 25. && self.race.total_time < RACE_SECONDS as f32);
        assert_eq!(self.countdown_steps, COUNTDOWN_TICKS);
        // A finish can occur midway through an already-scheduled batch. Those
        // remaining slots count on the clock but must not advance the race.
        assert_eq!(
            self.session.clock.tick(),
            COUNTDOWN_TICKS + self.race.ticks + self.terminal_steps
        );
        assert!(self.race.ghost().is_none());
        assert!(!self.race.best_ghost.is_empty());
    }

    fn restart(&mut self) {
        self.session.restart(&mut self.race, &self.track);
        self.gates.clear();
        self.laps.clear();
        self.lap_start_tick = 0;
        self.recorded_poses.clear();
        self.recorded_poses.push(pose_value(self.race.car.pose));
        self.countdown_steps = 0;
        self.terminal_steps = 0;
        self.zero_frames = 0;
        self.catch_up_frames = 0;
    }
}

#[test]
fn authored_three_lap_race_matches_at_30_60_and_144_hz() {
    let mut reference = Scenario::new();
    reference.run_seconds(60, RACE_SECONDS);
    reference.assert_finished();
    assert!(reference.catch_up_frames > 0);
    for fps in [30, 144] {
        let mut scenario = Scenario::new();
        scenario.run_seconds(fps, RACE_SECONDS);
        scenario.assert_finished();
        assert_eq!(
            scenario.gates, reference.gates,
            "accepted gates at {fps} Hz"
        );
        assert_eq!(
            scenario.laps, reference.laps,
            "accepted laps / records at {fps} Hz"
        );
        assert_eq!(
            scenario.state(),
            reference.state(),
            "final state at {fps} Hz"
        );
        if fps == 144 {
            assert!(scenario.zero_frames > 0);
        } else {
            assert!(scenario.catch_up_frames > 0);
        }
        let finished = scenario.state();
        for active in [true, false, true] {
            assert_eq!(
                scenario
                    .frame(
                        600.,
                        active,
                        Some(Controls {
                            throttle: 1.,
                            steer: 1.,
                            handbrake: true
                        })
                    )
                    .steps,
                0
            );
            assert_eq!(scenario.state(), finished);
            assert_eq!(scenario.session.clock.state(), SessionState::Won);
        }
    }
}

#[test]
fn pause_with_a_real_record_resumes_identically_and_restart_replays_the_track() {
    let mut reference = Scenario::new();
    let mut interrupted = Scenario::new();
    for _ in 0..60 * RACE_SECONDS {
        reference.frame(1. / 60., true, None);
        interrupted.frame(1. / 60., true, None);
        if interrupted.race.laps == 1 {
            break;
        }
    }
    assert_eq!(interrupted.race.laps, 1);
    assert!(interrupted.race.ghost().is_some());
    assert!(matches!(
        interrupted.session.notice,
        Some(Notice::Lap { number: 1, .. })
    ));
    assert_eq!(interrupted.frame(TICK / 2., true, None).steps, 0);
    assert_eq!(reference.frame(TICK / 2., true, None).steps, 0);
    let before = interrupted.state();
    let tick = interrupted.session.clock.tick();
    for elapsed in [90., 0., 600.] {
        assert_eq!(interrupted.frame(elapsed, false, None).steps, 0);
        assert_eq!(interrupted.session.clock.state(), SessionState::Paused);
        assert_eq!(interrupted.state(), before);
        assert_eq!(interrupted.session.clock.tick(), tick);
    }
    // The transition frame includes inactive time and is deliberately discarded.
    let resumed = interrupted.frame(600., true, None);
    assert_eq!(resumed.steps, 0);
    assert_eq!(resumed.dropped_seconds, 0.);
    assert!((resumed.alpha - 0.5).abs() < 1e-6);
    assert_eq!(interrupted.state(), before);
    assert_eq!(interrupted.session.clock.state(), SessionState::Playing);
    assert_eq!(interrupted.frame(TICK / 2., true, None).steps, 1);
    assert_eq!(reference.frame(TICK / 2., true, None).steps, 1);
    assert_eq!(interrupted.state(), reference.state());
    interrupted.run_seconds(60, RACE_SECONDS);
    reference.run_seconds(60, RACE_SECONDS);
    interrupted.assert_finished();
    reference.assert_finished();
    assert_eq!(interrupted.gates, reference.gates);
    assert_eq!(interrupted.laps, reference.laps);
    assert_eq!(interrupted.state(), reference.state());

    let best = interrupted.race.best_time;
    let ghost = ghost_values(&interrupted.race);
    interrupted.restart();
    assert_eq!(interrupted.race.best_time, best);
    assert_eq!(ghost_values(&interrupted.race), ghost);
    assert_eq!(interrupted.race.ghost().unwrap().position, ghost[0].0);
    assert_eq!(
        interrupted.race.car.pose.position,
        interrupted.track.points[0]
    );
    assert_eq!(interrupted.race.car.velocity, Vec2::ZERO);
    assert_eq!(
        (
            interrupted.race.ticks,
            interrupted.race.laps,
            interrupted.race.next_gate
        ),
        (0, 0, 1)
    );
    assert_eq!(
        (interrupted.race.total_time, interrupted.race.lap_time),
        (0., 0.)
    );
    assert!(interrupted.race.last_lap.is_none());
    assert!(interrupted.race.last_comparison.is_none());
    assert_eq!(interrupted.session.clock.tick(), 0);
    assert_eq!(interrupted.session.clock.state(), SessionState::Playing);
    assert_eq!(interrupted.session.countdown(), 3);
    assert_eq!(interrupted.session.notice, None);
    assert!(!interrupted.session.off_track());
    assert!(!interrupted.race.driver_reversing());
    assert_eq!(interrupted.frame(0., true, None).steps, 0);
    interrupted.run_seconds(144, RACE_SECONDS);
    interrupted.assert_finished();
    assert_eq!(
        interrupted.gates, reference.gates,
        "restart resets driver as well as vehicle"
    );
    assert_eq!(interrupted.race.ticks, reference.race.ticks);
    assert_eq!(interrupted.race.total_time, reference.race.total_time);
    assert_eq!(interrupted.race.best_time, best);
    assert_eq!(ghost_values(&interrupted.race), ghost);
    for (replayed, original) in interrupted.laps.iter().zip(&reference.laps) {
        assert_eq!(replayed.ticks, original.ticks);
        assert_eq!(replayed.time, original.time);
        assert_eq!(replayed.comparison.previous, best.and_then(run_micros));
        assert_eq!(
            replayed.ghost, ghost,
            "equal/slower laps cannot replace the retained best ghost"
        );
    }
}

fn moving_car() -> Scenario {
    let mut scenario = Scenario::new();
    let gas = Controls {
        throttle: 1.,
        ..Default::default()
    };
    for _ in 0..180 {
        scenario.frame(1. / 60., true, Some(gas));
    }
    for _ in 0..30 {
        scenario.frame(TICK, true, Some(gas));
    }
    assert_eq!(scenario.race.ticks, 30);
    assert!(scenario.race.car.velocity.length() > 30.);
    scenario
}

#[test]
fn held_controls_on_zero_tick_frames_are_not_invented_as_action_edges() {
    let mut tapped = moving_car();
    let mut released = moving_car();
    let pulse = Controls {
        throttle: -1.,
        steer: 1.,
        handbrake: true,
    };
    let before = tapped.state();
    assert_eq!(tapped.frame(TICK / 2., true, Some(pulse)).steps, 0);
    assert_eq!(tapped.state(), before);
    // Driving reads held values. A press and release wholly between ticks has
    // no queued gameplay edge; the next tick uses the released sample.
    assert_eq!(
        tapped
            .frame(TICK / 2., true, Some(Controls::default()))
            .steps,
        1
    );
    released.frame(TICK, true, Some(Controls::default()));
    assert_eq!(tapped.state(), released.state());
    let mut held = moving_car();
    held.frame(TICK, true, Some(pulse));
    assert_ne!(held.race.car.velocity, tapped.race.car.velocity);
    assert_ne!(held.race.car.pose.angle, tapped.race.car.pose.angle);

    let held = Controls {
        throttle: 0.5,
        steer: -0.4,
        handbrake: true,
    };
    assert_eq!(tapped.frame(8. * TICK, true, Some(held)).steps, 8);
    for _ in 0..8 {
        released.frame(TICK, true, Some(held));
    }
    assert_eq!(
        tapped.state(),
        released.state(),
        "held controls apply to every catch-up tick"
    );
    assert_eq!(tapped.race.ticks, 39);
}

#[test]
fn over_budget_elapsed_drops_time_without_debt_and_paused_restart_clears_remainder() {
    let mut stalled = moving_car();
    let mut reference = moving_car();
    let frame = stalled.frame(1., true, None);
    assert_eq!(frame.steps, 8);
    assert!((frame.dropped_seconds - 112. * TICK).abs() < 1e-12);
    for _ in 0..8 {
        reference.frame(TICK, true, None);
    }
    assert_eq!(stalled.state(), reference.state());
    assert_eq!(stalled.session.clock.tick(), reference.session.clock.tick());
    assert_eq!(stalled.frame(0., true, None).steps, 0);
    stalled.frame(TICK, true, None);
    reference.frame(TICK, true, None);
    assert_eq!(stalled.state(), reference.state());
    assert_eq!(stalled.frame(TICK / 2., true, None).steps, 0);
    stalled.frame(90., false, None);
    assert_eq!(stalled.session.clock.state(), SessionState::Paused);
    stalled.restart();
    let mut fresh = Scenario::new();
    assert_eq!(stalled.state(), fresh.state());
    assert_eq!(stalled.session.clock.state(), SessionState::Playing);
    assert_eq!(stalled.session.clock.tick(), 0);
    assert_eq!(stalled.frame(TICK / 2., true, None).steps, 0);
    fresh.frame(TICK / 2., true, None);
    stalled.run_seconds(30, RACE_SECONDS);
    fresh.run_seconds(30, RACE_SECONDS);
    stalled.assert_finished();
    fresh.assert_finished();
    assert_eq!(stalled.gates, fresh.gates);
    assert_eq!(stalled.laps, fresh.laps);
    assert_eq!(stalled.state(), fresh.state());
}
