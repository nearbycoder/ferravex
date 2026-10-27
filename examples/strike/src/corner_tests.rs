//! The browser fullscreen host pins its game-menu button over the top-right
//! 72×60 corner. Paint the real HUD, menus and results through the shared Ui
//! with inspection enabled and check that no text, button or (non-full-bleed)
//! panel reaches into that corner at the audited viewports.
use crate::host::Mode;
use crate::hud::{self, HudState, in_menu_corner};
use crate::juice::{Juice, Marker, PlayLayout};
use crate::scoreboard::{Moment, Tracker};
use crate::{cards, crew};
use ferravex_fps::GameEvent;
use ferravex_play::rounds::{Bomb, Phase, Team, Weapon};
use ferravex_render::audio::GameAudio;
use ferravex_render::input::{PlayerInput, host_frame};
use ferravex_render::prelude::*;
use ferravex_render::ui::{Ui, UiRole, Viewport};

const SIZES: [(f32, f32); 5] = [
    (1280., 800.),
    (1600., 1000.),
    (390., 844.),
    (844., 390.),
    (320., 480.),
];

fn board() -> Tracker {
    let mut t = Tracker::new(0, true);
    for n in 0..6u32 {
        let victim = 1 + (n % 5) as u8;
        t.observe(
            100 + n * 10,
            &[
                GameEvent::Hit {
                    attacker: 0,
                    victim,
                    damage: 100,
                    headshot: true,
                },
                GameEvent::Killed {
                    attacker: 0,
                    victim,
                },
            ],
            |_| {},
        );
    }
    t
}

fn juice(bots: bool) -> Juice {
    let mut sound = GameAudio::new();
    sound.pause();
    let mut j = Juice::new();
    for victim in [3, 4, 1, 2] {
        j.moment(
            Moment::Feed {
                attacker: 5,
                victim,
                headshot: true,
            },
            0,
            bots,
            &mut sound,
        );
    }
    j.moment(
        Moment::Kill {
            victim: 2,
            headshot: true,
            nth: 1,
            first_blood: false,
            revenge: false,
        },
        0,
        bots,
        &mut sound,
    );
    j.moment(Moment::Multi(5), 0, bots, &mut sound);
    j
}

fn view() -> ferravex_fps::tactical::View {
    let player = |id, team| ferravex_fps::tactical::PlayerView {
        id,
        team,
        health: 100,
        money: 16000,
        weapon: Weapon::Sniper,
        armor: 100,
        kit: true,
    };
    ferravex_fps::tactical::View {
        tick: 1,
        phase: Phase::Planted,
        round: 12,
        remaining_ticks: 64 * 99,
        score: [12, 11],
        bomb: Bomb::Carried(0),
        interaction: None,
        players: vec![player(0, Team::Attack), player(1, Team::Defend)],
    }
}

/// Paint one frame and return every offending element's label and bounds.
fn offenders(w: f32, h: f32, paint: impl FnOnce(&mut Ui)) -> Vec<String> {
    let mut ui = Ui::new();
    ui.set_inspection_enabled(true);
    let mut out = Vec::new();
    host_frame(0., w as u32, h as u32, || {
        ui.begin(Viewport::fit([w, h], [w, h], [0.; 4]).unwrap());
        paint(&mut ui);
        for e in ui.inspect().into_iter().flatten() {
            let Some(rect) = e.visible_bounds else {
                continue;
            };
            let full_bleed = rect.size[0] >= w - 1.;
            let checked = match e.role {
                UiRole::Text | UiRole::Button => !e.painted_label.trim().is_empty(),
                UiRole::Panel => !full_bleed,
                _ => false,
            };
            if checked && in_menu_corner(w, rect) {
                out.push(format!("{:?} {:?} {rect:?}", e.role, e.painted_label));
            }
        }
        ui.finish(true);
        ui.paint();
    });
    out
}

fn input() -> PlayerInput {
    crate::controls::Controls::default().input
}

#[test]
fn in_play_hud_keeps_the_fullscreen_menu_corner_clear() {
    let input = input();
    let player = ferravex_fps::Player::new(0, Vec3::ZERO);
    let mut dead = player.clone();
    dead.health = 0;
    dead.respawn_ticks = 100;
    let board = board();
    assert!(board.streak >= 2, "the streak badge is part of the check");
    for (w, h) in SIZES {
        for touch in [false, true] {
            for (practice, round) in [(true, false), (false, true), (false, false)] {
                for notice in [false, true] {
                    for local in [&player, &dead] {
                        let j = juice(practice);
                        let tactical = view();
                        let bad = offenders(w, h, |ui| {
                            let error = if notice {
                                "AN INTERACTION IS ALREADY IN PROGRESS FOR A VERY LONG TIME"
                            } else {
                                ""
                            };
                            hud::overlay(
                                ui,
                                local,
                                HudState {
                                    input: &input,
                                    active: true,
                                    touch_mode: touch,
                                    diagnostics: false,
                                    frame_ms: 16.,
                                    triangles: 1000,
                                    tick: 10,
                                    hit_timer: 0.2,
                                    shot_timer: 0.,
                                    round_phase: round.then_some(Phase::Planted),
                                    error,
                                    marker: Marker::Headshot,
                                    death: Some(crate::juice::Death {
                                        killer: crew::Who {
                                            id: 4,
                                            local: 0,
                                            bots: practice,
                                        },
                                        gloat: crew::gloat(4, 1),
                                        retry: crew::retry(1),
                                    }),
                                },
                            );
                            hud::tactical(
                                ui,
                                round.then_some(&tactical),
                                (!round && !practice).then_some((1, 2)),
                                0,
                                true,
                                &input,
                            );
                            j.draw_play(
                                ui,
                                &board,
                                PlayLayout {
                                    practice,
                                    round,
                                    touch,
                                    notice,
                                    footer: false,
                                    roster: false,
                                    alive: local.health > 0,
                                    hint: Some("HOLD TAB TO SEE THE SCOREBOARD"),
                                    local: 0,
                                    bots: practice,
                                },
                            );
                        });
                        assert!(
                            bad.is_empty(),
                            "{w}x{h} touch={touch} practice={practice} round={round} \
                             notice={notice}: {bad:#?}"
                        );
                    }
                }
            }
        }
        let players: Vec<_> = (0..6)
            .map(|id| ferravex_fps::Player::new(id, Vec3::ZERO))
            .collect();
        let bad = offenders(w, h, |ui| hud::roster(ui, &players, 0, &board, true));
        assert!(bad.is_empty(), "{w}x{h} scoreboard: {bad:#?}");
    }
}

#[test]
fn menus_cards_and_results_keep_the_fullscreen_menu_corner_clear() {
    let input = input();
    let board = board();
    let results = cards::Results::new(&board, Some(40), true);
    let mut rows: [String; 3] = Default::default();
    for (w, h) in SIZES {
        let bad = offenders(w, h, |ui| cards::results(ui, &results));
        assert!(bad.is_empty(), "{w}x{h} results: {bad:#?}");
        for (mode, played) in [
            (Mode::Practice, false),
            (Mode::Practice, true),
            (Mode::Replay, true),
            (Mode::Network, true),
        ] {
            for touch in [false, true] {
                let bad = offenders(w, h, |ui| {
                    hud::menu(ui, mode, played, touch, &input);
                    let (left, bottom) = hud::menu_extent(w, h, mode, played, touch, &input);
                    if played {
                        cards::pause_rows(&board, Some(40), 3, &mut rows);
                    } else {
                        cards::briefing_rows(Some((40, 12)), &input, &mut rows);
                    }
                    cards::info(ui, left, bottom, &rows, cards::PAUSE_COLORS);
                });
                assert!(bad.is_empty(), "{w}x{h} {mode:?} menu: {bad:#?}");
            }
        }
    }
}
