use crate::{sim::Run, text, trial::Trial};
use ferravex_render::{input::PlayerInput, prelude::*};

pub(crate) fn hint(player: &PlayerInput, action: &str) -> String {
    let mut buffer = [0; 512];
    player
        .format_hint(action, &mut buffer)
        .unwrap_or("Controls")
        .to_owned()
}

/// Fit to actual bitmap advances; abbreviate exceptionally long remapped labels
/// rather than shrinking them to unreadable sizes or drawing over neighboring UI.
pub(crate) fn fitted(label: &str, width: f32, size: u16) -> (String, f32, f32) {
    let font = get_default_font();
    let base = font.try_metrics(size, 1.).unwrap();
    let measured = base.measure_line(label).unwrap().width;
    let scale = (width.max(1.) / measured.max(1.)).clamp(10. / size.max(10) as f32, 1.);
    let metrics = font.try_metrics(size, scale).unwrap();
    let mut label = label.to_owned();
    if metrics.measure_line(&label).unwrap().width > width {
        let suffix = metrics.measure_line("...").unwrap().width;
        let count = ((width - suffix).max(0.) / metrics.glyph_advance()) as usize;
        if let Some((end, _)) = label.char_indices().nth(count) {
            label.truncate(end);
        }
        // The final measurement accounts for accumulated fractional advances.
        while !label.is_empty() && metrics.measure_line(&label).unwrap().width + suffix > width {
            label.pop();
        }
        label.push_str("...");
    }
    let measured = metrics.measure_line(&label).unwrap().width;
    (label, size as f32 * scale, measured)
}

pub(crate) fn centered(label: &str, y: f32, size: u16, color: Color) {
    centered_in(label, y, size, color, (screen_width() - 32.).max(32.));
}

pub(crate) fn centered_in(label: &str, y: f32, size: u16, color: Color, width: f32) {
    let (label, size, width) = fitted(label, width, size);
    text(&label, (screen_width() - width) * 0.5, y, size, color);
}

pub(crate) fn goal(run: &Run) -> String {
    let (label, position) = if run.gems() == run.level.gems.len() {
        ("FINISH ARCH".to_owned(), run.level.finish)
    } else if let Some(cp) = run.level.checkpoints.get(run.checkpoint) {
        (format!("CHECKPOINT {}", run.checkpoint + 1), cp.position)
    } else if let Some(index) = run.collected.iter().position(|&collected| !collected) {
        (format!("CRYSTAL {}", index + 1), run.level.gems[index])
    } else {
        ("FINISH ARCH".to_owned(), run.level.finish)
    };
    format!("NEXT {label} / {:.0}m", run.position().distance(position))
}

/// Transient gameplay notices yield to pause/focus/Controls overlays. Their
/// tick-owned lifetime is retained, so resuming shows the remaining notice.
pub(crate) fn notice(run: &Run, trial: &Trial, player: &PlayerInput) -> Option<(String, String)> {
    if !trial.session.is_playing() {
        return None;
    }
    if let Some(count) = trial.countdown() {
        Some((
            count.to_string(),
            "GET READY / TIMER STARTS ON GO".to_string(),
        ))
    } else if trial.showing_go() {
        Some(("GO!".to_string(), "COLLECT EVERY CRYSTAL".to_string()))
    } else if run.notice > 0. {
        Some(if run.recovered {
            (
                "BACK ON TRACK".to_string(),
                format!(
                    "{} / CRYSTALS KEPT / FALL {}",
                    if run.checkpoint == 0 {
                        "START".to_string()
                    } else {
                        format!("CHECKPOINT {}", run.checkpoint)
                    },
                    run.falls
                ),
            )
        } else {
            (
                "CHECKPOINT SAVED".to_string(),
                format!(
                    "{} / {} / {} TO RECOVER",
                    run.checkpoint,
                    run.level.checkpoints.len(),
                    hint(player, "recover")
                ),
            )
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferravex_render::input::{Binding, InputCode, Preset};

    #[test]
    fn remapped_hints_and_fitted_labels_follow_real_bindings_and_font_widths() {
        let mut player = PlayerInput::preset("marble-run-hints", Preset::Racer);
        player
            .rebind(
                "brake",
                vec![Binding::digital(InputCode::Key(KeyCode::B as u16))],
            )
            .unwrap();
        assert_eq!(hint(&player, "brake"), "B");
        assert!(!hint(&player, "brake").contains("Space"));
        let (label, size, width) = fitted(&"LONG CUSTOM BINDING / ".repeat(20), 288., 17);
        assert!(label.ends_with("..."));
        assert!(size >= 10.);
        assert!(width <= 288.);
        assert_eq!(
            get_default_font()
                .try_metrics(17, size / 17.)
                .unwrap()
                .measure_line(&label)
                .unwrap()
                .width,
            width
        );
    }

    #[test]
    fn guidance_names_checkpoint_then_missing_crystals_then_finish() {
        let mut run = Run::new();
        assert!(goal(&run).starts_with("NEXT CHECKPOINT 1"));
        run.checkpoint = run.level.checkpoints.len();
        assert!(goal(&run).starts_with("NEXT CRYSTAL 1"));
        run.collected.fill(true);
        assert!(goal(&run).starts_with("NEXT FINISH ARCH"));
    }

    #[test]
    fn transient_notices_yield_to_overlays_without_losing_tick_owned_lifetime() {
        let mut run = Run::new();
        let mut trial = Trial::new(false);
        let mut player = PlayerInput::transient("marble-notice");
        crate::trial_actions(&mut player);
        let first = notice(&run, &trial, &player).unwrap();
        assert_eq!(first.0, "3");
        assert_eq!(trial.advance(30., true).steps, 0);
        assert_eq!(notice(&run, &trial, &player), None);
        assert_eq!(trial.advance(0., false).steps, 0);
        assert_eq!(notice(&run, &trial, &player), Some(first));

        // Actual countdown and GO ticks, then an accepted recovery at spawn.
        for _ in 0..481 {
            let frame = trial.advance(1. / 120., false);
            for _ in 0..frame.steps {
                trial.step(&mut run, crate::sim::Controls::default());
            }
        }
        trial.request_recovery();
        let frame = trial.advance(1. / 120., false);
        for _ in 0..frame.steps {
            trial.step(&mut run, crate::sim::Controls::default());
        }
        let recovered = notice(&run, &trial, &player).unwrap();
        assert_eq!(recovered.0, "BACK ON TRACK");
        assert!(recovered.1.contains("CRYSTALS KEPT / FALL 1"));
        let frozen = (
            run.elapsed,
            run.notice,
            trial.pose(&run),
            trial.visual_time(&run),
        );
        assert_eq!(trial.advance(30., true).steps, 0);
        assert_eq!(notice(&run, &trial, &player), None);
        assert_eq!(
            (
                run.elapsed,
                run.notice,
                trial.pose(&run),
                trial.visual_time(&run)
            ),
            frozen
        );
        trial.advance(0., false);
        assert_eq!(notice(&run, &trial, &player), Some(recovered));
        for _ in 0..360 {
            let frame = trial.advance(1. / 120., false);
            for _ in 0..frame.steps {
                trial.step(&mut run, crate::sim::Controls::default());
            }
        }
        assert_eq!(notice(&run, &trial, &player), None);
    }
}
