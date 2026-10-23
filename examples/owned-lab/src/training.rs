//! Read-only objective views share the accepted-action counters, never host time.
use super::*;
use ferravex_engine::play::objective::{ObjectiveKind, ObjectiveSpec, ObjectiveState};

pub(super) fn objectives() -> ObjectiveGraph<4> {
    ObjectiveGraph::new(
        144,
        std::array::from_fn(|i| ObjectiveSpec {
            id: i as u16,
            all: 0,
            any: 0,
            exclusive_group: 0,
            kind: ObjectiveKind::Event {
                id: i as u16,
                count: if i == 0 { 500 } else { 1 },
            },
        }),
    )
    .expect("four independent training drills")
}

impl Lab {
    /// Inspect the four independent drills in move/fire/reload/hit order.
    /// Movement is reported in centimeters; other targets require one accepted action.
    /// Inspection does not consume events, advance the match or alter replay bytes.
    ///
    /// ```
    /// use ferravex_engine::play::objective::{ObjectiveProgress, ObjectiveState};
    /// let lab = owned_lab::Lab::new_interactive();
    /// let drills: Vec<_> = lab.training_objectives().collect();
    /// assert_eq!(drills.len(), 4);
    /// assert!(drills.iter().all(|d| d.state == ObjectiveState::Ready));
    /// assert_eq!(drills[0].progress, ObjectiveProgress::Event { count: 0, required: 500 });
    /// ```
    pub fn training_objectives(&self) -> impl ExactSizeIterator<Item = ObjectiveView> + '_ {
        self.objectives.iter()
    }

    pub(super) fn training_mask(&self) -> u32 {
        self.training_objectives().fold(0, |mask, view| {
            mask | (u32::from(view.state == ObjectiveState::Complete) << view.index)
        })
    }

    pub(super) fn sync_training(&mut self, before: TrainingProgress) {
        let amounts = [
            (self.training.distance * 100.).floor() as u16
                - (before.distance * 100.).floor() as u16,
            u16::from(self.training.shots > before.shots),
            u16::from(self.training.reloads > before.reloads),
            u16::from(self.training.hits > before.hits),
        ];
        for (index, amount) in amounts.into_iter().enumerate() {
            if amount == 0 || self.objectives.complete(index) {
                continue;
            }
            let frame = self
                .objectives
                .event(
                    index as u16,
                    amount,
                    self.objectives.last_event_serial() + 1,
                )
                .expect("bounded positive training increments");
            if frame.completed != 0 {
                self.drill_notice = 2.5;
                self.sound.cue(Cue::Pickup, 0.4);
            }
        }
    }

    pub(super) fn accept_feedback(&mut self, kind: FeedbackKind, position: Vec3) -> u8 {
        if self.feedback_serial == u64::MAX {
            self.feedback.clear();
            self.feedback_serial = 0;
        }
        self.feedback_serial += 1;
        let event = FeedbackEvent::new(kind, position, 1.).expect("finite accepted FPS event");
        if let Ok(Some(response)) = self.feedback.emit(self.feedback_serial, event) {
            self.sound
                .at(response.cue, response.position, response.gain);
            response.particles
        } else {
            0
        }
    }
}
