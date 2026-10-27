//! Host timing and transient race feedback. Gate validation stays in `sim::Race`.
use crate::sim::{Controls, Race, Track};
use ferravex_core::{FrameStep, session::GameSession};
use ferravex_play::persistence::RecordDelta;
use ferravex_render::prelude::Vec2;

const HZ: u32 = 120;
const COUNTDOWN: u32 = 3 * HZ;
const OFFROAD_NOTICE: u32 = HZ / 4;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Notice {
    Go,
    Checkpoint(usize),
    Lap {
        number: u32,
        time: f32,
        best: bool,
        comparison: Option<RecordDelta>,
    },
    BackOnTrack,
}

pub(crate) struct RaceSession {
    pub clock: GameSession,
    countdown_ticks: u32,
    pub notice: Option<Notice>,
    notice_ticks: u32,
    offroad_ticks: u32,
}

impl RaceSession {
    pub fn new() -> Self {
        Self {
            // Preserve automatic entry into the race for interactive and smoke hosts.
            clock: GameSession::new(HZ as f64, 8, true),
            countdown_ticks: COUNTDOWN,
            notice: None,
            notice_ticks: 0,
            offroad_ticks: 0,
        }
    }

    pub fn restart(&mut self, race: &mut Race, track: &Track) {
        race.reset(track);
        self.clock.restart();
        self.countdown_ticks = COUNTDOWN;
        self.notice = None;
        self.notice_ticks = 0;
        self.offroad_ticks = 0;
    }

    pub fn countdown(&self) -> u32 {
        self.countdown_ticks.div_ceil(HZ)
    }

    /// Presentation-only countdown progress: 0 at the start, 1 at GO.
    pub fn countdown_progress(&self) -> f32 {
        1. - self.countdown_ticks as f32 / COUNTDOWN as f32
    }

    pub fn off_track(&self) -> bool {
        self.offroad_ticks >= OFFROAD_NOTICE
    }

    pub fn advance(&mut self, elapsed: f64, active: bool) -> FrameStep {
        // The frame that restores focus/closes settings includes inactive time too.
        // Discard that duration rather than charging it to the resumed race.
        let changed = self.clock.set_paused(!active);
        self.clock.advance(if changed { 0. } else { elapsed })
    }

    /// Returns true only when vehicle simulation advances (not during countdown).
    pub fn step(&mut self, race: &mut Race, track: &Track, controls: Controls, dt: f32) -> bool {
        if !self.clock.is_playing()
            || race.finished()
            || !dt.is_finite()
            || dt <= 0.
            || !controls.throttle.is_finite()
            || !controls.steer.is_finite()
        {
            return false;
        }
        self.notice_ticks = self.notice_ticks.saturating_sub(1);
        if self.notice_ticks == 0 {
            self.notice = None;
        }
        if self.countdown_ticks > 0 {
            self.countdown_ticks -= 1;
            if self.countdown_ticks == 0 {
                self.show(Notice::Go, HZ);
            }
            return false;
        }

        let (lap, gate, best) = (race.laps, race.next_gate, race.best_time);
        race.step(track, controls, dt);
        let was_offroad = self.off_track();
        if race.car.offroad {
            self.offroad_ticks = self.offroad_ticks.saturating_add(1);
        } else {
            self.offroad_ticks = 0;
            if was_offroad {
                self.show(Notice::BackOnTrack, HZ);
            }
        }
        if race.laps > lap {
            self.show(
                Notice::Lap {
                    number: race.laps,
                    time: race.last_lap.unwrap_or(0.),
                    best: race.best_time != best,
                    comparison: race.last_comparison,
                },
                3 * HZ,
            );
        } else if race.next_gate != gate {
            self.show(Notice::Checkpoint(gate), HZ);
        }
        if race.finished() {
            self.clock.finish(true);
        }
        true
    }

    fn show(&mut self, notice: Notice, ticks: u32) {
        self.notice = Some(notice);
        self.notice_ticks = ticks;
    }

    /// Points toward the nearest road segment without moving the car or awarding gates.
    pub fn recovery_target(&self, race: &Race, track: &Track) -> Option<Vec2> {
        if !self.off_track() || race.finished() {
            return None;
        }
        Some(track.project(race.car.pose.position).position.truncate())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferravex_core::session::SessionState;
    use ferravex_render::prelude::*;

    fn frame(session: &mut RaceSession, race: &mut Race, track: &Track, dt: f64, active: bool) {
        let frame = session.advance(dt, active);
        for _ in 0..frame.steps {
            session.step(race, track, Controls::default(), frame.dt);
        }
    }

    #[test]
    fn invalid_steps_do_not_consume_countdown_or_expire_notices() {
        let track = Track::new();
        let mut race = Race::new(&track);
        let mut session = RaceSession::new();
        session.show(Notice::Go, HZ);
        for dt in [0., -1., f32::NAN, f32::INFINITY] {
            assert!(!session.step(&mut race, &track, Controls::default(), dt));
        }
        assert!(!session.step(
            &mut race,
            &track,
            Controls {
                steer: f32::NAN,
                ..Default::default()
            },
            1. / 120.
        ));
        assert_eq!(session.countdown_ticks, COUNTDOWN);
        assert_eq!(session.notice_ticks, HZ);
        assert_eq!(race.ticks, 0);
    }

    #[test]
    fn countdown_blocks_motion_and_timers_at_all_frame_rates() {
        let track = Track::new();
        for fps in [30, 60, 144] {
            let mut race = Race::new(&track);
            let mut session = RaceSession::new();
            let start = race.car.pose.position;
            for _ in 0..3 * fps {
                let frame = session.advance(1. / fps as f64, true);
                for _ in 0..frame.steps {
                    session.step(
                        &mut race,
                        &track,
                        Controls {
                            throttle: 1.,
                            ..Default::default()
                        },
                        frame.dt,
                    );
                }
            }
            assert_eq!(session.countdown(), 0);
            assert_eq!(session.notice, Some(Notice::Go));
            assert_eq!(race.car.pose.position, start);
            assert_eq!(race.total_time, 0.);
            assert_eq!(race.ticks, 0);
            frame(&mut session, &mut race, &track, 1. / 120., true);
            assert_eq!(race.ticks, 1);
        }
    }

    #[test]
    fn pause_freezes_countdown_race_and_feedback_and_discards_resume_gap() {
        let track = Track::new();
        let mut race = Race::new(&track);
        let mut session = RaceSession::new();
        frame(&mut session, &mut race, &track, 1. / 60., true);
        let countdown = session.countdown_ticks;
        frame(&mut session, &mut race, &track, 90., false);
        frame(&mut session, &mut race, &track, 90., true);
        assert_eq!(session.countdown_ticks, countdown);
        for _ in 0..180 {
            frame(&mut session, &mut race, &track, 1. / 60., true);
        }
        let before = (
            race.ticks,
            race.total_time,
            race.lap_time,
            session.notice_ticks,
        );
        frame(&mut session, &mut race, &track, 600., false);
        assert_eq!(session.clock.state(), SessionState::Paused);
        frame(&mut session, &mut race, &track, 600., false);
        frame(&mut session, &mut race, &track, 600., true);
        assert_eq!(
            before,
            (
                race.ticks,
                race.total_time,
                race.lap_time,
                session.notice_ticks
            )
        );
        frame(&mut session, &mut race, &track, 1. / 120., true);
        assert_eq!(race.ticks, before.0 + 1);
    }

    #[test]
    fn recovery_guidance_does_not_award_a_gate_or_create_a_best_lap() {
        let track = Track::new();
        let mut race = Race::new(&track);
        let mut session = RaceSession::new();
        session.countdown_ticks = 0;
        race.car.pose.position = vec2(500., 280.);
        for _ in 0..30 {
            frame(&mut session, &mut race, &track, 1. / 120., true);
        }
        let target = session.recovery_target(&race, &track).unwrap();
        assert!(track.nearest(target).1 < 0.01);
        assert_eq!(race.car.pose.position, vec2(500., 280.));
        assert_eq!((race.laps, race.next_gate), (0, 1));
        assert!(race.best_time.is_none());
        race.car.pose.position = target;
        frame(&mut session, &mut race, &track, 1. / 120., true);
        assert!(!session.off_track());
        assert_eq!(session.notice, Some(Notice::BackOnTrack));
    }

    #[test]
    fn restart_from_pause_clears_recovery_and_partial_tick() {
        let track = Track::new();
        let mut race = Race::new(&track);
        let mut session = RaceSession::new();
        session.countdown_ticks = 0;
        race.car.pose.position = vec2(500., 280.);
        for _ in 0..30 {
            frame(&mut session, &mut race, &track, 1. / 120., true);
        }
        assert!(session.off_track());
        assert_eq!(session.advance(1. / 240., true).steps, 0);
        frame(&mut session, &mut race, &track, 10., false);
        assert!(session.clock.is_paused());
        session.restart(&mut race, &track);
        assert!(session.clock.is_playing());
        assert!(!session.off_track());
        assert_eq!(session.notice, None);
        assert_eq!(race.car.pose.position, track.points[0]);
        assert_eq!(race.total_time, 0.);
        assert_eq!(session.countdown(), 3);
        assert_eq!(session.advance(1. / 240., true).steps, 0);
    }

    #[test]
    fn autoplay_finishes_session_and_restart_keeps_record_but_clears_debt() {
        let track = Track::new();
        let mut race = Race::new(&track);
        let mut session = RaceSession::new();
        for _ in 0..12_000 {
            let frame = session.advance(1. / 60., true);
            for _ in 0..frame.steps {
                let input = if session.countdown() == 0 {
                    race.autopilot(&track)
                } else {
                    Controls::default()
                };
                session.step(&mut race, &track, input, frame.dt);
            }
            if race.finished() {
                break;
            }
        }
        assert!(race.finished());
        assert_eq!(session.clock.state(), SessionState::Won);
        let before = (race.ticks, race.total_time);
        frame(&mut session, &mut race, &track, 20., true);
        assert_eq!(before, (race.ticks, race.total_time));
        let best = race.best_time;
        let ghost = race.best_ghost.len();
        session.restart(&mut race, &track);
        assert_eq!(race.best_time, best);
        assert_eq!(race.best_ghost.len(), ghost);
        assert_eq!(race.total_time, 0.);
        assert_eq!((race.laps, race.next_gate), (0, 1));
        assert_eq!(race.car.velocity, Vec2::ZERO);
        assert_eq!(session.clock.state(), SessionState::Playing);
        assert_eq!(session.clock.tick(), 0);
        assert_eq!(session.countdown(), 3);
        assert_eq!(session.notice, None);
        assert!(!session.off_track());
        assert_eq!(session.advance(0., true).steps, 0);
    }
}
