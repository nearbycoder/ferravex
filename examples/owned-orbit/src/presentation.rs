//! Local adapters for accepted events, shared feedback policy and binding hints.
use ferravex_engine::{
    math::Vec3,
    play::{
        audio::{Cue, Surface},
        feedback::{FeedbackConfig, FeedbackController, FeedbackEvent, FeedbackKind},
    },
    render::preferences,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AcceptedEvent {
    Launch,
    Impact,
    Panel(u32),
    Precision(u32),
    Wide,
    Save,
    LifeLost,
    SectorCleared,
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
        let (kind, slot, cue, gain) = match event {
            AcceptedEvent::Launch => (FeedbackKind::Dash, 0, Cue::Jump, 0.45),
            AcceptedEvent::Impact => (FeedbackKind::Hit, 1, Cue::Impact(Surface::Metal), 0.5),
            AcceptedEvent::Panel(_) | AcceptedEvent::Precision(_) | AcceptedEvent::Wide => {
                (FeedbackKind::Pickup, 2, Cue::Pickup, 0.45)
            }
            AcceptedEvent::Save => (FeedbackKind::Pickup, 2, Cue::Pickup, 0.55),
            AcceptedEvent::LifeLost => (FeedbackKind::Hurt, 3, Cue::Failure, 0.45),
            AcceptedEvent::SectorCleared => (FeedbackKind::Pickup, 4, Cue::Success, 0.65),
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
