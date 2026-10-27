//! Qualifier, juice and HUD behavior through the real Lab, match events and ticks.
use super::*;
use ferravex_engine::play::feedback::FeedbackPolicy;
use qualifier::{COUNTDOWN_TICKS, DEBRIEF_CARD_TICKS, RUN_TICKS};

const TICK: f64 = 1. / 64.;

/// Instant aim at the nearest operator in clear sight, for a scripted shooter.
fn aim(lab: &Lab, head: bool) -> Option<InputFrame> {
    let eye = lab.game.player(0).unwrap().eye_position();
    let target = lab
        .game
        .players
        .iter()
        .filter(|p| p.id != 0 && p.is_alive())
        .filter(|p| {
            let to = p.eye_position() - eye;
            let distance = to.length();
            lab.game
                .arena
                .raycast(eye, to / distance, distance)
                .is_none()
        })
        .min_by(|a, b| {
            (a.position - eye)
                .length()
                .total_cmp(&(b.position - eye).length())
        })?;
    let point =
        target.position + target.velocity / 64. + vec3(0., if head { 1.66 } else { 1.05 }, 0.);
    let delta = point - eye;
    Some(InputFrame {
        yaw: delta.x.atan2(-delta.z),
        pitch: delta.y.atan2(delta.x.hypot(delta.z)),
        buttons: ferravex_fps::BUTTON_FIRE,
        ..Default::default()
    })
}

fn hold(lab: &Lab) -> InputFrame {
    let player = lab.game.player(0).unwrap();
    InputFrame {
        yaw: player.yaw,
        pitch: player.pitch,
        ..Default::default()
    }
}

/// Complete the four drills through accepted actions only.
fn finish_drills(lab: &mut Lab) {
    for _ in 0..64 {
        let input = InputFrame {
            move_x: 1.,
            ..Default::default()
        };
        lab.update_interactive(TICK, input, true, 1, 1.);
    }
    for _ in 0..4000 {
        if lab.training_mask() & 0b1000 != 0 {
            break;
        }
        let input = aim(lab, false).unwrap_or_else(|| hold(lab));
        lab.update_interactive(TICK, input, true, 1, 1.);
    }
    let reload = InputFrame {
        buttons: ferravex_fps::BUTTON_RELOAD,
        ..hold(lab)
    };
    lab.update_interactive(TICK, reload, true, 1, 1.);
    for _ in 0..ferravex_fps::RELOAD_TICKS {
        lab.update_interactive(TICK, hold(lab), true, 1, 1.);
    }
    assert_eq!(lab.training_mask(), 15);
}

/// One shot every `pace` ticks after the cooldown; every third shot is a headshot.
fn play_run(lab: &mut Lab, pace: u32) {
    let mut shots = 0;
    let mut wait = 0;
    while !matches!(lab.qualifier_status().phase, QualifierPhase::Debrief(_)) {
        let player = lab.game.player(0).unwrap();
        let mut input = hold(lab);
        if matches!(lab.qualifier_status().phase, QualifierPhase::Running(_)) {
            if player.ammo == 0 {
                input.buttons = ferravex_fps::BUTTON_RELOAD;
            } else if player.fire_cooldown == 0 && player.reload_ticks == 0 {
                if wait > 0 {
                    wait -= 1;
                } else if let Some(aimed) = aim(lab, shots % 3 == 0) {
                    input = aimed;
                    shots += 1;
                    wait = pace;
                }
            }
        }
        lab.update_interactive(TICK, input, true, 1, 1.);
    }
}

#[test]
fn drills_unlock_a_countdown_then_real_hits_score_the_same_way_every_time() {
    let mut results = Vec::new();
    for _ in 0..2 {
        let mut lab = Lab::new_interactive();
        lab.recording = true;
        finish_drills(&mut lab);
        let phase = lab.qualifier_status().phase;
        assert!(
            matches!(phase, QualifierPhase::Countdown(left) if left <= COUNTDOWN_TICKS),
            "{phase:?}"
        );
        let progress = lab.training_progress();
        play_run(&mut lab, 24);
        let result = lab.qualifier_result().unwrap();
        assert!(result.hits >= 10 && result.headshots > 0 && result.eliminations > 0);
        assert!(result.shots >= result.hits);
        assert_eq!(
            result.best_streak,
            lab.qualifier.last().unwrap().best_streak
        );
        assert!(result.score >= result.accuracy_bonus + result.hits * 100);
        // The qualifier never touches the shared FPS rules or replay layout.
        assert_eq!(lab.session_state(), SessionState::Playing);
        assert!(lab.training_progress().shots > progress.shots);
        assert_eq!(lab.qualifier_best(), Some(result.score));
        assert_eq!(lab.previous_best, None, "first run sets the first record");
        results.push((
            result,
            lab.replay_bytes(),
            lab.game.players.clone(),
            lab.game.tick,
        ));
    }
    assert_eq!(
        results[0], results[1],
        "deterministic under identical input"
    );
}

#[test]
fn bell_restarts_from_the_debrief_and_reset_keeps_only_the_personal_best() {
    let mut lab = Lab::new_interactive();
    finish_drills(&mut lab);
    play_run(&mut lab, 64);
    let first = lab.qualifier_result().unwrap();
    assert!(!hud::card_visible(&lab), "TIME! has the screen first");
    for _ in 0..hud::CARD_DELAY_TICKS {
        lab.update_interactive(TICK, hold(&lab), true, 1, 1.);
    }
    assert!(hud::card_visible(&lab));
    for _ in 0..DEBRIEF_CARD_TICKS {
        lab.update_interactive(TICK, hold(&lab), true, 1, 1.);
    }
    assert!(!hud::card_visible(&lab), "card gives way to free practice");
    // Fire once at the bell from wherever the player stands.
    let eye = lab.game.player(0).unwrap().eye_position();
    let delta = qualifier::BELL - eye;
    let ring = InputFrame {
        yaw: delta.x.atan2(-delta.z),
        pitch: delta.y.atan2(delta.x.hypot(delta.z)),
        buttons: ferravex_fps::BUTTON_FIRE,
        ..Default::default()
    };
    lab.game.player_mut(0).unwrap().fire_cooldown = 0;
    lab.update_interactive(TICK, ring, true, 1, 1.);
    assert!(matches!(
        lab.qualifier_status().phase,
        QualifierPhase::Countdown(_)
    ));
    assert_eq!(lab.qualifier_result(), Some(first), "last card is retained");
    play_run(&mut lab, 12);
    let second = lab.qualifier_result().unwrap();
    assert_eq!(lab.qualifier_status().runs, 2);
    assert_eq!(lab.previous_best, Some(first.score));
    assert_eq!(lab.qualifier_best(), Some(first.score.max(second.score)));
    let best = lab.qualifier_best();
    assert!(lab.reset_interactive());
    assert_eq!(lab.qualifier_status().phase, QualifierPhase::Locked);
    assert_eq!(lab.qualifier_result(), None);
    assert_eq!(lab.qualifier_best(), best, "reset keeps the personal best");
}

#[test]
fn peer_sessions_freeze_the_qualifier_and_hide_the_bell() {
    let mut lab = Lab::new_interactive();
    finish_drills(&mut lab);
    for _ in 0..COUNTDOWN_TICKS + 10 {
        lab.update_interactive(TICK, hold(&lab), true, 1, 1.);
    }
    let status = lab.qualifier_status();
    assert!(matches!(status.phase, QualifierPhase::Running(_)));
    let bell_items = lab.items.len();
    lab.peer.set_mode(1);
    for _ in 0..64 {
        lab.update_interactive(TICK, InputFrame::default(), false, 1, 1.);
    }
    assert_eq!(
        lab.qualifier_status(),
        status,
        "peer ticks never score offline runs"
    );
    assert!(
        lab.items.len() < bell_items,
        "bell and bullseyes are offline only"
    );
}

#[test]
fn eliminations_tip_operators_over_with_confetti_and_respect_reduced_motion() {
    for reduced in [false, true] {
        preferences::set_feedback_policy(if reduced {
            FeedbackPolicy::new(true, true, 1.).unwrap()
        } else {
            FeedbackPolicy::default()
        });
        let mut lab = Lab::new_interactive();
        lab.update_interactive(TICK, hold(&lab), true, 1, 1.);
        // Bring an operator into clear sight, then headshot it.
        let origin = lab.game.player(0).unwrap().position;
        lab.game.player_mut(1).unwrap().position = origin + vec3(0., 0., -4.);
        let shot = aim(&lab, true).unwrap();
        let before = lab.items.len();
        lab.update_interactive(TICK, shot, true, 1, 1.);
        assert!(!lab.game.player(1).unwrap().is_alive());
        assert_eq!(lab.kills, 1);
        let fallen = lab.juice.fallen(1).expect("cutout lies down");
        assert!(lab.items.len() >= before, "fallen cutout still renders");
        let confetti = lab
            .particles
            .iter()
            .filter(|p| confetti_index(p.color) != 0)
            .count();
        if reduced {
            assert_eq!(confetti, 0);
            assert!(fallen.age >= juice::FALL_TIME);
            assert_eq!(lab.hit_marker, 0.);
        } else {
            assert_eq!(
                confetti, 12,
                "coral, teal and cream; gold shares the spark mesh"
            );
            assert_eq!(lab.juice.marker, juice::Marker::Elimination);
            assert!(lab.hit_marker > 0.18);
        }
        assert!(
            lab.juice
                .pops()
                .any(|(pop, _)| pop == juice::Pop::Eliminated)
        );
        // The browser host uploads at most 256 instances per mesh.
        let mut per_mesh = vec![0; lab.mesh_data.len()];
        for row in lab.shadow_instances.chunks(17) {
            per_mesh[row[0] as usize] += 1;
        }
        assert!(per_mesh.iter().all(|&n| n <= 256), "{per_mesh:?}");
        // Respawn clears the cutout.
        for _ in 0..ferravex_fps::RESPAWN_TICKS + 1 {
            lab.update_interactive(TICK, hold(&lab), true, 1, 1.);
        }
        assert!(lab.game.player(1).unwrap().is_alive());
        assert!(lab.juice.fallen(1).is_none());
    }
    preferences::set_feedback_policy(FeedbackPolicy::default());
}

#[test]
fn hud_layouts_keep_panels_apart_at_desktop_narrow_and_compact_sizes() {
    for (w, h) in [
        (1280., 800.),
        (1600., 1000.),
        (1280., 737.),
        (800., 600.),
        (390., 682.),
        (276., 640.),
        (232., 640.),
        (640., 360.),
    ] {
        let layout = hud::layout(w, h).unwrap();
        let (px, py, pw) = layout.panel;
        let (rx, ry, rw) = layout.radio;
        let (cx, cy, cw) = layout.card;
        for (x, width) in [(px, pw), (rx, rw), (cx, cw)] {
            assert!(
                x >= 8. && x + width <= w - 8.,
                "{w}x{h} inside the viewport"
            );
        }
        // Radio height: header plus its maximum wrapped lines and padding.
        let radio_bottom = ry + 16. + 18. + layout.radio_lines as f32 * 20.;
        let separate_columns = rx + rw <= px;
        assert!(
            separate_columns || radio_bottom <= py,
            "{w}x{h}: radio and objective panel overlap"
        );
        assert!(
            py + layout.panel_max <= h - 120.,
            "{w}x{h}: clear the DOM vitals strip"
        );
        assert!(
            cy + layout.card_max <= h - 96.,
            "{w}x{h}: card clears the vitals strip"
        );
        assert!(layout.card_max >= 150., "{w}x{h}: card fits its essentials");
        if layout.compact && w >= 600. {
            assert!(
                px + pw <= w * 0.5 + 26.,
                "{w}x{h}: compact panel clears score pops"
            );
        }
        if w < 800. && h >= 480. {
            assert!(
                py + layout.panel_max <= 294.,
                "narrow guide stays above touch rings"
            );
            let rings = (h - 309. - 34.).min(h - 208. - 48.);
            assert!(
                cy + layout.card_max <= rings.max(294.),
                "{w}x{h}: card clears touch rings"
            );
        }
    }
    assert!(hud::layout(170., 600.).is_none());
}

#[test]
fn hud_draws_every_phase_at_every_size_without_panicking() {
    use ferravex_engine::render::{self, input::PlayerInput};
    let input = PlayerInput::transient("owned-lab-hud-phases");
    let mut lab = Lab::new_interactive();
    let mut snapshots = vec![];
    lab.update_interactive(TICK, hold(&lab), true, 2, 1.);
    snapshots.push(QualifierPhase::Locked);
    finish_drills(&mut lab);
    for _ in 0..COUNTDOWN_TICKS / 2 {
        lab.update_interactive(TICK, hold(&lab), true, 1, 1.);
    }
    let check = |lab: &Lab| {
        for (w, h) in [
            (1280, 800),
            (1600, 1000),
            (390, 682),
            (276, 640),
            (232, 640),
            (640, 360),
        ] {
            for view in [1, 2] {
                render::input::host_frame(1. / 60., w, h, || {
                    hud::draw(lab, &input, w as f32, h as f32, view);
                });
            }
        }
    };
    check(&lab);
    play_run(&mut lab, 30);
    check(&lab);
    for _ in 0..hud::CARD_DELAY_TICKS {
        lab.update_interactive(TICK, hold(&lab), true, 1, 1.);
    }
    assert!(hud::card_visible(&lab));
    check(&lab);
    for _ in 0..DEBRIEF_CARD_TICKS {
        lab.update_interactive(TICK, hold(&lab), true, 1, 1.);
    }
    check(&lab);
    assert!(!hud::card_visible(&lab));
    assert_eq!(
        lab.qualifier_result().map(|r| r.shots > 0),
        Some(true),
        "{RUN_TICKS}-tick run recorded shots"
    );
}

#[test]
fn results_copy_always_offers_a_record_line_and_a_next_goal() {
    let result = QualifierResult {
        score: 4100,
        ..Default::default()
    };
    assert_eq!(hud::record_lines(result, None).0, "First record set!");
    assert_eq!(
        hud::record_lines(result, Some(5000)).0,
        "Best 5,000 / 900 to beat it"
    );
    assert!(
        hud::record_lines(result, Some(3000))
            .0
            .starts_with("New personal best! +1,100")
    );
    assert_eq!(hud::next_goal(result), "Next: A at 7,500 (3,400 to go)");
    let top = QualifierResult {
        score: 20_000,
        ..Default::default()
    };
    assert!(hud::next_goal(top).starts_with("Top grade"));
    assert_eq!(hud::thousands(1_234_567), "1,234,567");
    assert_eq!(hud::thousands(999), "999");
}
