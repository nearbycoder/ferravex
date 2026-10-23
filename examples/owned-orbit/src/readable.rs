//! Bounded UTF-8 presentation for the custom browser's readable DOM status.
//! No input sampling, preference storage, simulation updates or bitmap changes.
use crate::{
    game::{Feedback, Game, Phase},
    presentation::ControlHints,
};
use std::fmt::{self, Write};

pub(crate) const STATUS_TEXT_CAPACITY: usize = 2048;

pub(crate) struct ReadableStatus {
    bytes: [u8; STATUS_TEXT_CAPACITY],
    len: usize,
}
impl Default for ReadableStatus {
    fn default() -> Self {
        Self {
            bytes: [0; STATUS_TEXT_CAPACITY],
            len: 0,
        }
    }
}
impl Write for ReadableStatus {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let end = self
            .len
            .checked_add(text.len())
            .filter(|&end| end <= self.bytes.len())
            .ok_or(fmt::Error)?;
        self.bytes[self.len..end].copy_from_slice(text.as_bytes());
        self.len = end;
        Ok(())
    }
}
impl ReadableStatus {
    pub fn as_str(&self) -> &str {
        // Every append copies a complete &str after checking its entire size.
        std::str::from_utf8(&self.bytes[..self.len]).expect("complete UTF-8 writes")
    }

    pub fn refresh(
        &mut self,
        game: &Game,
        paused: bool,
        hints: &ControlHints,
        notice: Option<(&str, u32)>,
    ) {
        self.len = 0;
        if self.compose(game, paused, hints, notice).is_err() {
            self.len = 0;
            // Never publish partial instructions, stale bindings or broken UTF-8.
            let _ = self.write_str("Status text unavailable. Open Controls for current bindings.");
        }
    }

    fn compose(
        &mut self,
        g: &Game,
        paused: bool,
        hints: &ControlHints,
        notice: Option<(&str, u32)>,
    ) -> fmt::Result {
        if paused {
            self.write_str("Paused. Resume to continue. ")?;
        }
        self.write_str(match g.phase {
            Phase::Ready => "Ready.",
            Phase::Playing => "Playing.",
            Phase::Transition => "Sector 1 secured. Preparing reinforced panels in sector 2.",
            Phase::Won => "Both sectors cleared.",
            Phase::Lost => "Run ended.",
        })?;
        write!(
            self,
            " Sector {} of 2. Score {}. {} {} remaining.",
            g.sector,
            g.score,
            g.lives,
            if g.lives == 1 { "life" } else { "lives" }
        )?;
        match g.phase {
            Phase::Ready if hints.launch == "UNBOUND" => self.write_str(" Launch is unbound. Open Controls to bind it.")?,
            Phase::Ready if hints.launch == "SEE CONTROLS" => self.write_str(" Open Controls for the current launch binding.")?,
            Phase::Ready => write!(self, " Launch: {}.", hints.launch)?,
            Phase::Won | Phase::Lost if hints.launch == "UNBOUND" => self.write_str(" Retry is unbound. Open Controls to bind it.")?,
            Phase::Won | Phase::Lost if hints.launch == "SEE CONTROLS" => self.write_str(" Open Controls for the current retry binding; retry and launch need separate presses.")?,
            Phase::Won | Phase::Lost => write!(
                self,
                " Retry: {}. Release, then press {} again to launch.",
                hints.launch, hints.launch
            )?,
            _ => {}
        }
        self.write_char('\n')?;
        writeln!(
            self,
            "Panels: {} remaining; {} cleared. Precision streak: {}. Best precision: {}. Ball saves: {}.",
            g.remaining(),
            g.clears,
            g.precision.streak(),
            g.precision.best(),
            g.saves
        )?;
        match g.phase {
            Phase::Ready => {
                writeln!(
                    self,
                    "Serve charge: {:.0}%. Full charge pierces armor until the first paddle return.",
                    g.serve_charge.power() * 100.
                )?;
            }
            Phase::Playing => {
                if g.armor_piercing() {
                    self.write_str("Armor piercing until first paddle return. ")?;
                }
                if g.ball_save > 0. {
                    write!(self, "Opening ball save: {:.1}s. ", g.ball_save)?;
                }
                if g.wide_time > 0. {
                    write!(self, "Wide paddle: {:.1}s. ", g.wide_time)?;
                }
                if g.feedback() == Some(Feedback::BallSaved) {
                    self.write_str("Ball saved. ")?;
                }
                if let Some((label, points)) = notice {
                    write!(self, "{label} +{points}. ")?;
                }
                writeln!(self, "Launch / retry: {}.", hints.launch)?;
            }
            Phase::Transition => writeln!(
                self,
                "Sector 2 in {:.1}s. Reinforced panels need two hits.",
                g.transition_remaining()
            )?,
            Phase::Won | Phase::Lost => {
                writeln!(self, "Run time: {:.0}s.", g.time)?;
            }
        }
        if !matches!(g.phase, Phase::Won | Phase::Lost) {
            writeln!(
                self,
                "Move: {}. Pause / resume: {}.",
                hints.movement, hints.pause
            )?;
        }
        write!(
            self,
            "Restart: {}. F2 / Controls shows all bindings.",
            hints.restart
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_writes_are_atomic_and_overflow_does_not_publish_partial_copy() {
        let mut text = ReadableStatus::default();
        text.write_str("é→").unwrap();
        assert_eq!(text.as_str(), "é→");
        assert!(text.write_str(&"x".repeat(STATUS_TEXT_CAPACITY)).is_err());
        assert_eq!(text.as_str(), "é→");
        let hints = ControlHints {
            launch: "é".repeat(STATUS_TEXT_CAPACITY),
            ..ControlHints::default()
        };
        text.refresh(&Game::new(), false, &hints, None);
        assert_eq!(
            text.as_str(),
            "Status text unavailable. Open Controls for current bindings."
        );
    }

    #[test]
    fn real_loss_reports_earned_counts_and_retry_remains_a_separate_press() {
        use crate::{Orbit, game::Input};
        let mut app = Orbit::new();
        for tick in 0..3840 {
            app.advance_input(
                1. / 120.,
                Input {
                    axis: 1.,
                    launch: [240, 1440, 2640].contains(&tick),
                    restart: false,
                },
            );
        }
        assert_eq!(app.game.phase, Phase::Lost);
        assert_eq!(app.game.lives, 0);
        assert!(app.game.score > 0);
        let expected = format!(
            "Panels: {} remaining; {} cleared. Precision streak: {}. Best precision: {}. Ball saves: {}.",
            app.game.remaining(),
            app.game.clears,
            app.game.precision.streak(),
            app.game.precision.best(),
            app.game.saves
        );
        let before = (
            app.game.time,
            app.game.ball,
            app.game.velocity,
            app.game.score,
            app.session_state(),
        );
        let pixels = app.canvas.frame.pixels().to_vec();
        let copy = app.status_text_v1().to_owned();
        assert!(copy.starts_with("Run ended. Sector 1 of 2."));
        assert!(copy.contains(&expected));
        assert!(copy.contains("Release, then press"));
        assert_eq!(
            before,
            (
                app.game.time,
                app.game.ball,
                app.game.velocity,
                app.game.score,
                app.session_state()
            )
        );
        assert_eq!(pixels, app.canvas.frame.pixels());
        app.advance_input(
            1. / 120.,
            Input {
                launch: true,
                ..Input::default()
            },
        );
        assert_eq!(app.game.phase, Phase::Ready);
        assert!(app.status_text_v1().contains("Score 0. 3 lives remaining."));
        assert!(
            app.status_text_v1()
                .contains("Panels: 35 remaining; 0 cleared.")
        );
        app.advance_input(
            1. / 120.,
            Input {
                launch: true,
                ..Input::default()
            },
        );
        assert_eq!(app.game.phase, Phase::Playing);
    }

    #[test]
    fn real_campaign_reports_transition_pause_and_victory_without_advancing_on_reads() {
        let mut app = crate::Orbit::new();
        let mut transition_seen = false;
        for _ in 0..72000 {
            let input = app.game.autoplay();
            app.advance_input(1. / 120., input);
            if app.game.phase == Phase::Transition && !transition_seen {
                transition_seen = true;
                assert!(app.status_text_v1().contains("Sector 2 in"));
                app.set_paused(true);
                let remaining = app.game.transition_remaining();
                assert!(
                    app.status_text_v1()
                        .starts_with("Paused. Resume to continue. Sector 1 secured.")
                );
                assert_eq!(remaining, app.game.transition_remaining());
                app.set_paused(false);
            }
            if app.game.phase == Phase::Won {
                break;
            }
        }
        assert!(transition_seen);
        assert_eq!(app.game.phase, Phase::Won);
        let best = app.game.precision.best();
        let saves = app.game.saves;
        assert!(
            app.status_text_v1()
                .starts_with("Both sectors cleared. Sector 2 of 2.")
        );
        assert!(
            app.status_text_v1()
                .contains("Panels: 0 remaining; 70 cleared.")
        );
        assert!(
            app.status_text_v1()
                .contains(&format!("Best precision: {best}. Ball saves: {saves}."))
        );
    }
}
