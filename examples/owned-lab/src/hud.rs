//! Immediate overlay reuses the host's existing UI vertex stream.
use super::*;
use ferravex_engine::{
    play::objective::ObjectiveProgress,
    render::{self, input::PlayerInput},
};

fn fit(text: &str, width: f32, metrics: render::BitmapFontMetrics) -> &str {
    let mut end = text.len();
    while end > 0 && metrics.measure_line(&text[..end]).unwrap().width > width {
        end = text[..end].char_indices().next_back().unwrap().0;
    }
    &text[..end]
}

// Keep ordinary long remaps and Unbound readable rather than silently cutting
// off the action's binding. Exceptionally large profiles get an explicit route
// to Controls; neither UI storage nor the panel can grow without a bound.
fn wrap(mut text: &str, width: f32, metrics: render::BitmapFontMetrics) -> Vec<&str> {
    let mut lines = Vec::with_capacity(3);
    while !text.is_empty() && lines.len() < 3 {
        let prefix = fit(text, width, metrics);
        let end = if prefix.len() < text.len() {
            prefix
                .rfind(' ')
                .filter(|&end| end != 0)
                .unwrap_or(prefix.len())
        } else {
            prefix.len()
        };
        if end == 0 {
            break;
        }
        lines.push(&text[..end]);
        text = text[end..].trim_start();
    }
    if !text.is_empty() {
        // Do not present a partial final key name as if it were the full binding.
        if let Some(last) = lines.last_mut() {
            *last = "More in Controls";
        }
    }
    lines
}

fn direction(yaw: f32) -> &'static str {
    let slot = ((yaw.rem_euclid(std::f32::consts::TAU) / std::f32::consts::FRAC_PI_4).round()
        as usize)
        % 8;
    ["N", "NE", "E", "SE", "S", "SW", "W", "NW"][slot]
}

pub(super) fn draw(lab: &Lab, _input: &PlayerInput, width: f32, height: f32, view: u32) {
    if width < 180. || height < 260. {
        return;
    }
    let panel_width = (width - 32.).min(340.);
    let x = if width >= 800. {
        width - panel_width - 80.
    } else {
        16.
    };
    let compact = width < 800. && height < 480.;
    let y = if compact {
        height - 200.
    } else if width >= 800. {
        16.
    } else {
        152.
    };
    let metrics = render::get_default_font().try_metrics(14, 1.).unwrap();
    let line_height = metrics.line_height() + 4.;
    let tint = render::Color::new(0.94, 0.93, 0.86, 1.);
    let player = lab.game.player(0).unwrap();
    let next = lab
        .training_objectives()
        .find(|view| !lab.objectives.complete(view.index));
    let objective = if lab.peer.mode != 0 {
        String::new()
    } else if let Some(next) = next {
        match next.index {
            0 => {
                let ObjectiveProgress::Event { count, .. } = next.progress else {
                    unreachable!()
                };
                format!("Move {:.1}/5 m", f32::from(count) / 100.)
            }
            1 => "Fire a shot".to_owned(),
            2 if player.reload_ticks > 0 => {
                format!("Reloading: {:.1}s", f32::from(player.reload_ticks) / 64.)
            }
            2 => "Reload".to_owned(),
            _ => "Hit an operator".to_owned(),
        }
    } else {
        "4/4 complete - free practice".to_owned()
    };
    let orientation = if view == 2 {
        format!("North up / facing {}", direction(player.yaw))
    } else {
        format!(
            "Facing {} / {:.0} degrees",
            direction(player.yaw),
            player.yaw.to_degrees().rem_euclid(360.)
        )
    };
    let guide = if lab.peer.mode != 0 {
        "".to_owned()
    } else if lab.drill_notice > 0. {
        format!("Drill complete: {}/4", lab.training_mask().count_ones())
    } else if let Some(target) = lab.target_hint.filter(|_| lab.objectives.complete(2)) {
        let delta = target.position - player.eye_position();
        let yaw = delta.x.atan2(-delta.z);
        let offset = (yaw - player.yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;
        let bearing = if offset.abs() < 0.15 {
            "ahead"
        } else if offset < 0. {
            "left"
        } else {
            "right"
        };
        format!(
            "Operator {}: {} / {:.0} m",
            target.id,
            bearing,
            delta.length()
        )
    } else {
        String::new()
    };
    let lines = if compact {
        [
            objective.as_str(),
            if lab.peer.mode != 0 {
                lab.peer_status()
            } else {
                guide.as_str()
            },
            "",
            "",
        ]
    } else {
        [
            if lab.peer.mode != 0 {
                lab.peer_status()
            } else {
                ""
            },
            &objective,
            if view == 2 { &orientation } else { "" },
            &guide,
        ]
    };
    let lines: Vec<_> = lines
        .into_iter()
        .take(if compact { 2 } else { 4 })
        .flat_map(|text| wrap(text, panel_width - 16., metrics))
        .collect();
    render::draw_rectangle(
        x,
        y,
        panel_width,
        line_height * lines.len() as f32 + 16.,
        render::Color::new(0.06, 0.10, 0.13, 0.9),
    );
    for (line, text) in lines.into_iter().enumerate() {
        render::draw_text(
            text,
            x + 8.,
            y + 8. + metrics.baseline() + line as f32 * line_height,
            14.,
            tint,
        );
    }
    if lab.hit_marker > 0. && !lab.feedback.policy().reduced_flash() {
        for (dx, dy) in [(-1., -1.), (1., -1.), (-1., 1.), (1., 1.)] {
            render::draw_line(
                width * 0.5 + dx * 7.,
                height * 0.5 + dy * 7.,
                width * 0.5 + dx * 12.,
                height * 0.5 + dy * 12.,
                2.,
                tint,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checked_hud_width_and_cardinal_directions() {
        let metrics = render::get_default_font().try_metrics(14, 1.).unwrap();
        assert!(
            metrics
                .measure_line(fit("Controller button 10 / Space", 80., metrics))
                .unwrap()
                .width
                <= 80.
        );
        assert_eq!(fit("ééé", 11., metrics), "é");
        assert_eq!(direction(0.), "N");
        assert_eq!(direction(std::f32::consts::FRAC_PI_2), "E");
        assert_eq!(direction(-std::f32::consts::FRAC_PI_2), "W");
    }

    #[test]
    fn journey200_199_long_current_bindings_unbound_and_orientation_remain_whole() {
        use render::input::{ActionKind, Binding, InputCode};
        let mut input = PlayerInput::transient("journey200-199-hints");
        input
            .action(
                "reload",
                "Reload",
                ActionKind::Button,
                vec![
                    Binding::digital(InputCode::Key(0xffe4)),
                    Binding::digital(InputCode::Key(0xffe2)),
                ],
            )
            .unwrap();
        let mut buffer = [0; 256];
        let metrics = render::get_default_font().try_metrics(14, 1.).unwrap();
        // 320px short host minus its 44px native pan surface, panel margins
        // and text inset: logical dimensions, independent of framebuffer DPR.
        let width = 320. - 44. - 32. - 16.;
        let hint = input.format_hint("reload", &mut buffer).unwrap();
        let copy = format!("Reload: {hint}");
        let lines = wrap(&copy, width, metrics);
        assert!(lines.len() > 1);
        assert_eq!(lines.join(" "), "Reload: Right Ctrl / Right Shift");
        assert!(
            lines
                .iter()
                .all(|line| metrics.measure_line(line).unwrap().width <= width)
        );
        input.rebind("reload", vec![]).unwrap();
        let hint = input.format_hint("reload", &mut buffer).unwrap();
        assert_eq!(
            wrap(&format!("Reload: {hint}"), width, metrics),
            ["Reload: Unbound"]
        );
        for yaw in [0., std::f32::consts::FRAC_PI_4, std::f32::consts::PI] {
            let copy = format!("North up / facing {}", direction(yaw));
            assert_eq!(wrap(&copy, width, metrics), [copy.as_str()]);
        }
        let huge = "Right Ctrl / ".repeat(64);
        let bounded = wrap(&huge, width, metrics);
        assert_eq!(bounded.len(), 3);
        assert_eq!(bounded[2], "More in Controls");
    }
}
