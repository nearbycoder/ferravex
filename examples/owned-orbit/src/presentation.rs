//! Local adapters for accepted events, shared feedback policy and binding hints.
use ferravex_engine::{
    math::Vec3,
    play::{
        audio::{Cue, Surface},
        feedback::{
            FeedbackConfig, FeedbackController, FeedbackEvent, FeedbackKind, FeedbackPolicy,
        },
        tween::{Entrance, Motion, RollCounter, ScreenTransition, TransitionEvent, TransitionKind},
    },
    render::preferences,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum AcceptedEvent {
    Launch,
    Impact,
    Panel(u32),
    Precision(u32),
    Wide,
    Split,
    Sticky,
    Save,
    LifeLost,
    SectorCleared,
    /// Presentation-only detail for juice; never a cue or feedback emission.
    Spark(Spark),
}

/// Where and how an accepted event happened, for pops, rings and callouts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Spark {
    Panel {
        at: [f32; 2],
        points: u32,
        destroyed: bool,
        row: usize,
        volley: u32,
    },
    Paddle {
        at: [f32; 2],
        bonus: u32,
    },
    Volley(u32),
    Pickup {
        kind: crate::game::Pickup,
        at: [f32; 2],
    },
    Satellite {
        at: [f32; 2],
        points: u32,
    },
    Drain {
        at: [f32; 2],
    },
    LastPanel {
        at: [f32; 2],
    },
    Flawless(u32),
    Backup {
        at: [f32; 2],
    },
    Caught,
    Release {
        at: [f32; 2],
    },
    Fizzle,
}

/// Presentation-time UI state: how long the current screen has been up, an
/// eased score counter and the wipe between sectors and runs. Aged by host
/// frame time (so the pause card can ease in while play is frozen); it never
/// feeds back into simulation.
pub(crate) struct Screen {
    key: u8,
    /// Seconds since the current screen (phase + pause) appeared.
    pub age: f32,
    /// Seconds of UI time, used for gentle prompt pulses.
    pub clock: f32,
    pub transition: ScreenTransition,
    pub score: RollCounter,
    pub motion: Motion,
}
impl Default for Screen {
    fn default() -> Self {
        Self {
            key: u8::MAX,
            age: 0.,
            clock: 0.,
            transition: ScreenTransition::new(
                TransitionKind::Wipe {
                    direction: std::f32::consts::FRAC_PI_2,
                },
                0.2,
            ),
            score: RollCounter::new(0),
            motion: Motion::Full,
        }
    }
}
impl Screen {
    /// `key` identifies the visible screen; a change restarts entrances and,
    /// for a new sector, sweeps a short wipe across the playfield.
    pub fn update(&mut self, dt: f32, key: u8, score: u32, policy: FeedbackPolicy) {
        let dt = if dt.is_finite() {
            dt.clamp(0., 0.25)
        } else {
            0.
        };
        self.motion = Motion::from(policy);
        if key != self.key {
            // Ready after a countdown means a fresh sector: sweep it in.
            if self.key == 2 && key == 0 {
                self.transition.cover();
            }
            self.key = key;
            self.age = 0.;
        }
        self.age = (self.age + dt).min(60.);
        self.clock = (self.clock + dt) % 3600.;
        if self.transition.advance(dt, self.motion) == Some(TransitionEvent::Covered) {
            self.transition.reveal();
        }
        self.score.set_target(i64::from(score));
        self.score.advance(dt, self.motion);
    }

    /// A new run: sweep in from the top and roll the score back instantly.
    pub fn restarted(&mut self) {
        self.score.snap(0);
        self.transition = Screen::default().transition;
        self.transition.cover();
    }

    /// Staggered fade-up for the `index`th element of the current screen.
    pub fn appear(&self, index: usize) -> (f32, i32) {
        let look = Entrance::fade_up(14.)
            .with_stagger(0.045)
            .with_duration(0.3)
            .sample(self.age, index, self.motion);
        (look.alpha, look.offset[1].round() as i32)
    }
}

/// The results card for a finished run, built once from accepted state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Results {
    pub summary: crate::quips::RunSummary,
    pub rank: crate::quips::Rank,
    pub previous_best: Option<u32>,
    pub new_best: bool,
}

pub(crate) struct Presentation {
    pub feedback: FeedbackController<128>,
    serial: u64,
    // Coalesce identical cue categories within one host frame, keeping distinct
    // accepted events (such as launch + rescue) even during catch-up.
    cues: [Option<(Cue, f32)>; 5],
    pub notice: Option<(&'static str, u32)>,
    notice_time: f32,
    pub hints: ControlHints,
    pub juice: crate::juice::Juice,
    pub screen: Screen,
    pub results: Option<Results>,
    /// Personal best score supplied by the host; survives restarts.
    pub best: Option<u32>,
    /// Finished runs, restarts and pauses this session; they rotate quips.
    pub runs: u32,
    pub restarts: u32,
    pub pauses: u32,
}
impl Default for Presentation {
    fn default() -> Self {
        Self {
            feedback: FeedbackController::new(
                FeedbackConfig {
                    translation_scale: 0.,
                    flash: 0.,
                    rumble: 0.,
                    ..Default::default()
                },
                0x4f52424954,
            )
            .expect("bounded presentation"),
            serial: 0,
            cues: [None; 5],
            notice: None,
            notice_time: 0.,
            hints: ControlHints::default(),
            juice: crate::juice::Juice::default(),
            screen: Screen::default(),
            results: None,
            best: None,
            runs: 0,
            restarts: 0,
            pauses: 0,
        }
    }
}
impl Presentation {
    pub fn begin_frame(&mut self, dt: f64) {
        preferences::sync_feedback(&mut self.feedback);
        let dt = if dt.is_finite() {
            dt.clamp(0., 0.25) as f32
        } else {
            0.
        };
        self.feedback
            .advance(dt)
            .expect("validated presentation delta");
        self.cues.fill(None);
        self.juice.chime = None;
        self.notice_time = (self.notice_time - dt).max(0.);
        if self.notice_time == 0. {
            self.notice = None;
        }
    }

    pub fn clear(&mut self) {
        self.feedback.clear();
        self.serial = 0;
        self.cues.fill(None);
        self.notice = None;
        self.notice_time = 0.;
        self.juice.reset();
        self.results = None;
    }

    pub fn accept_tick(&mut self, events: impl Iterator<Item = AcceptedEvent>) {
        // Release the controller's burst admission at tick boundaries without
        // decaying presentation time or replaying audio during catch-up.
        self.feedback.advance(0.).expect("zero presentation delta");
        for event in events {
            self.accept(event);
        }
    }

    fn accept(&mut self, event: AcceptedEvent) {
        match event {
            AcceptedEvent::Spark(spark) => return self.juice.accept(spark),
            AcceptedEvent::Save => self.juice.saved(),
            _ => {}
        }
        let (kind, slot, cue, gain) = match event {
            AcceptedEvent::Launch => (FeedbackKind::Dash, 0, Cue::Jump, 0.45),
            AcceptedEvent::Impact => (FeedbackKind::Hit, 1, Cue::Impact(Surface::Metal), 0.5),
            AcceptedEvent::Panel(_)
            | AcceptedEvent::Precision(_)
            | AcceptedEvent::Wide
            | AcceptedEvent::Split
            | AcceptedEvent::Sticky => (FeedbackKind::Pickup, 2, Cue::Pickup, 0.45),
            AcceptedEvent::Save => (FeedbackKind::Pickup, 2, Cue::Pickup, 0.55),
            AcceptedEvent::LifeLost => (FeedbackKind::Hurt, 3, Cue::Failure, 0.45),
            AcceptedEvent::SectorCleared => (FeedbackKind::Pickup, 4, Cue::Success, 0.65),
            AcceptedEvent::Spark(_) => return,
        };
        self.serial = self.serial.checked_add(1).unwrap_or_else(|| {
            self.feedback.clear();
            1
        });
        let event_feedback = FeedbackEvent::new(kind, Vec3::ZERO, 1.).expect("authored event");
        if !matches!(self.feedback.emit(self.serial, event_feedback), Ok(Some(_))) {
            return;
        }
        let gain = self.cues[slot].map_or(gain, |(_, old)| old.max(gain));
        self.cues[slot] = Some((cue, gain));
        let notice = match event {
            AcceptedEvent::Panel(points) => Some(("PANEL", points)),
            AcceptedEvent::Precision(points) => Some(("PRECISE RETURN", points)),
            AcceptedEvent::Wide => Some(("WIDE PADDLE", 50)),
            AcceptedEvent::Split => Some(("MULTI-BALL", 50)),
            AcceptedEvent::Sticky => Some(("STICKY PADDLE", 50)),
            _ => None,
        };
        if let Some(notice) = notice {
            // A run of panel contacts should not immediately obscure a reward.
            if self.notice.is_none_or(|(label, _)| label == "PANEL")
                || !matches!(event, AcceptedEvent::Panel(_))
            {
                self.notice = Some(notice);
                self.notice_time = 1.6;
            }
        }
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub fn take_cues(&mut self) -> impl Iterator<Item = (Cue, f32)> {
        std::mem::take(&mut self.cues).into_iter().flatten()
    }
}

pub(crate) struct ControlHints {
    pub launch: String,
    pub movement: String,
    pub pause: String,
    pub restart: String,
}
impl Default for ControlHints {
    fn default() -> Self {
        Self {
            launch: "SPACE / ENTER / TAP".into(),
            movement: "A D / ARROWS / DRAG".into(),
            pause: "ESC".into(),
            restart: "R".into(),
        }
    }
}
impl ControlHints {
    #[cfg(any(target_arch = "wasm32", test))]
    pub fn update(&mut self, input: &ferravex_engine::render::input::PlayerInput) {
        use ferravex_engine::render::input::HintDevice;
        fn append_hint(
            out: &mut String,
            input: &ferravex_engine::render::input::PlayerInput,
            name: &str,
        ) {
            let mut buffer = [0; 192];
            out.push_str(
                input
                    .format_hint(name, &mut buffer)
                    .unwrap_or("SEE CONTROLS"),
            );
        }
        for (label, name) in [
            (&mut self.launch, "launch"),
            (&mut self.pause, "pause"),
            (&mut self.restart, "restart"),
        ] {
            label.clear();
            append_hint(label, input, name);
            label.make_ascii_uppercase();
        }
        self.movement.clear();
        match input.current_device() {
            HintDevice::Touch => self.movement.push_str("DRAG TO STEER"),
            HintDevice::Gamepad => {
                self.movement.push_str("STEER ");
                append_hint(&mut self.movement, input, "move_x");
            }
            HintDevice::KeyboardMouse => {
                append_hint(&mut self.movement, input, "move_left");
                self.movement.push_str(" / ");
                append_hint(&mut self.movement, input, "move_right");
                self.movement.push_str(" / DRAG");
            }
        }
        self.movement.make_ascii_uppercase();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catchup_admits_each_tick_coalesces_cues_and_retains_terminal_feedback() {
        let mut presentation = Presentation::default();
        presentation.begin_frame(0.1);
        for _ in 0..12 {
            presentation.accept_tick(std::iter::repeat_n(AcceptedEvent::Impact, 16));
        }
        presentation.accept_tick([AcceptedEvent::SectorCleared].into_iter());
        assert_eq!(presentation.feedback.dropped, 0);
        let cues: Vec<_> = presentation.take_cues().collect();
        assert_eq!(
            cues,
            [(Cue::Impact(Surface::Metal), 0.5), (Cue::Success, 0.65)]
        );
        assert_eq!(presentation.take_cues().count(), 0);
        presentation.begin_frame(0.);
        assert_eq!(presentation.take_cues().count(), 0);
    }

    #[test]
    fn precision_notice_survives_panel_hits_and_expires_on_presentation_time() {
        let mut presentation = Presentation::default();
        presentation
            .accept_tick([AcceptedEvent::Precision(100), AcceptedEvent::Panel(200)].into_iter());
        assert_eq!(presentation.notice, Some(("PRECISE RETURN", 100)));
        for _ in 0..7 {
            presentation.begin_frame(0.25);
        }
        assert_eq!(presentation.notice, None);
        presentation.accept_tick([AcceptedEvent::Panel(200)].into_iter());
        assert_eq!(presentation.notice, Some(("PANEL", 200)));
        presentation.clear();
        assert_eq!(presentation.notice, None);
        assert_eq!(presentation.take_cues().count(), 0);
    }
}
