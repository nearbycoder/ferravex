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
                self.juice.radio(quips::drill(index));
            }
        }
    }

    /// Advance the qualifier on an offline fixed tick and present what it accepted.
    pub(super) fn step_qualifier(&mut self, actions: qualifier::TickActions, dt: f32) {
        use juice::Pop;
        let out = self.qualifier.tick(actions, dt);
        let policy = self.feedback.policy();
        if out.armed {
            self.cold_nudges = 0;
            let status = self.qualifier.status();
            self.juice.radio(if out.bell_rung {
                quips::rearmed(status.runs)
            } else {
                quips::ARMED
            });
            self.juice.big("3");
            self.sound.cue(Cue::Click, 0.5);
        }
        if out.bell_rung {
            self.juice.ring(policy);
            self.sound
                .at(Cue::Impact(Surface::Metal), qualifier::BELL, 0.9);
        }
        if let Some(seconds) = out.countdown {
            self.juice.big(if seconds == 2 { "2" } else { "1" });
            self.sound.cue(Cue::Click, 0.5);
        }
        if out.started {
            self.juice.big(quips::GO);
            self.sound.cue(Cue::Success, 0.45);
        }
        if out.bell_bonus {
            self.juice.pop(Pop::Bell);
            self.juice.radio(quips::BELL_SECRET);
        } else if let Some((points, multiplier)) = out.points {
            let headshot = actions.hit == Some(true);
            self.juice.pop(if headshot {
                Pop::Headshot { points, multiplier }
            } else {
                Pop::Points { points, multiplier }
            });
        }
        if let Some(count) = out.headshot_streak {
            self.juice.pop(Pop::HeadshotStreak(count));
        }
        if let Some(streak) = out.streak_milestone {
            self.juice.radio(quips::streak(streak));
            self.sound.cue(Cue::Pickup, 0.5);
        } else if out.first_headshot {
            self.juice.radio(quips::FIRST_HEADSHOT);
        } else if out.cold {
            self.juice.radio(quips::cold(self.cold_nudges));
            self.cold_nudges += 1;
        }
        if out.finished {
            let result = self.qualifier.last().expect("finished run has results");
            let previous = self.bests.record(result.score);
            // The results card carries the sergeant's verdict; keep the radio clear.
            self.juice.clear_radio();
            self.juice.big("TIME!");
            self.sound.cue(
                match result.grade() {
                    Grade::S | Grade::A | Grade::B => Cue::Success,
                    Grade::C => Cue::Pickup,
                    // A comic trombone for "Danger to Pigeons".
                    Grade::D => Cue::Failure,
                },
                0.6,
            );
            self.previous_best = previous;
        }
    }

    /// A celebratory burst for an elimination; motion reduction clears it.
    pub(super) fn confetti(&mut self, at: Vec3) {
        if self.feedback.policy().reduced_motion() {
            return;
        }
        let colors = [
            [0.83, 0.71, 0.42, 1.],
            [0.88, 0.38, 0.29, 1.],
            [0.25, 0.70, 0.65, 1.],
            [0.96, 0.92, 0.84, 1.],
        ];
        // A fixed fan, phased by simulation time; presentation only.
        for i in 0..16 {
            let angle = i as f32 * 2.399 + self.time * 3.;
            let speed = 2.2 + (i % 4) as f32 * 0.6;
            let color = colors[i % colors.len()];
            let _ = self.particles.spawn(ParticleDesc {
                position: at.to_array(),
                velocity: [
                    angle.cos() * speed,
                    3.6 + (i % 3) as f32,
                    angle.sin() * speed,
                ],
                acceleration: [0., -7.5, 0.],
                lifetime: 1.1,
                start_color: color,
                end_color: [color[0], color[1], color[2], 0.],
                start_size: 0.11,
                end_size: 0.06,
            });
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
