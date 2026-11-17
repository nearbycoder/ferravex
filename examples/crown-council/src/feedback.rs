//! Presentation effects for accepted (and refused) council actions, placed
//! where each change is drawn: the sealed decree's card, every pillar meter
//! that moved, the leaning that gained a pip, the work that finished.
//! Derived only from the model's per-tick beats; never read by the rules.
use crate::{
    Game,
    model::{
        Beat, CHRONICLE_BUTTON, PAGE_NEXT, PAGE_PREV, Refusal, Screen, WORKS_BUTTON, choice_rect,
        project_rect,
    },
    realm::{BADGE_Y, DANGER, GAUGE_H, GAUGE_TOP, GAUGE_X, level_y, pip, tile},
    view::{ACT_MARK, BADGE_X, PILLARS, POLICIES, SEAL_X, promise},
};
use ferravex_arcade::*;

const GOLD_FX: Color = Color::new(0.97, 0.76, 0.33, 1.);
const GAIN_FX: Color = Color::new(0.2, 0.62, 0.3, 1.);
const LOSS_FX: Color = Color::new(0.86, 0.22, 0.18, 1.);
const TEAL_FX: Color = Color::new(0.3, 0.68, 0.62, 1.);

fn center(r: Rect) -> Vec2 {
    vec2(r.x + r.w * 0.5, r.y + r.h * 0.5)
}
/// Where a pillar's popup starts: just above the meter's new level, but low
/// enough that its rise ends below the pillar's medallion.
fn popup_y(level: f32) -> f32 {
    (level - 14.).clamp(BADGE_Y + 60., GAUGE_TOP + GAUGE_H - 24.)
}
fn signed(value: i16) -> String {
    if value > 0 {
        format!("+{value}")
    } else {
        format!("-{}", value.unsigned_abs())
    }
}

/// Request this tick's effects; see [`ferravex_arcade::ArcadeGame::effects`].
pub(crate) fn effects(game: &Game, _cue: Cue, fx: &mut Juice) {
    for beat in game.beats() {
        match beat {
            Beat::Decreed { council } => decreed(game, council, fx),
            Beat::Completed(j) => {
                let c = center(tile(j));
                fx.burst(c, GOLD_FX, 26);
                fx.ring(c, GOLD_FX, 46.);
                fx.sparkle(c, GOLD_FX);
                fx.popup_icon(c - vec2(0., 40.), Icon::Check, "", GAIN_FX);
            }
            Beat::Promise(bits) => {
                for bit in (0..7).map(|b| 1u16 << b).filter(|b| bits & b != 0) {
                    let (glyph, name) = promise(bit);
                    fx.toast(Some(glyph), name, GOLD_FX);
                }
            }
            Beat::Act(act) => {
                // Rises off the briefing's heading as it settles in.
                let at = vec2(331., 306.);
                fx.callout(at, ACT_MARK[act.min(2)], GOLD_FX);
                fx.ring(vec2(331., 212.), GOLD_FX, 110.);
                fx.sparkle(vec2(331., 200.), GOLD_FX);
                fx.confetti(vec2(331., 250.), 36);
            }
            Beat::Collapse(k) => {
                let base = vec2(GAUGE_X[k], GAUGE_TOP + GAUGE_H - 12.);
                fx.burst(base, LOSS_FX, 48);
                fx.ring(base, LOSS_FX, 70.);
                fx.popup_icon(vec2(GAUGE_X[k], BADGE_Y - 4.), Icon::Skull, "", LOSS_FX);
                fx.shake(0.35);
            }
            Beat::Crowned => {
                for x in GAUGE_X {
                    fx.sparkle(vec2(x, BADGE_Y), GOLD_FX);
                }
            }
            Beat::Commissioned(j) => {
                let card = project_rect(j);
                let badge = vec2(BADGE_X, card.y + 38.);
                fx.ring(badge, TEAL_FX, 40.);
                fx.burst(badge, GOLD_FX, 18);
                // The coin leaves the treasury meter; the tile starts building.
                let cost = crate::model::PROJECTS[j].cost;
                let top = level_y(f32::from(game.resources[0]));
                fx.popup_icon(
                    vec2(GAUGE_X[0], popup_y(top)),
                    Icon::Coin,
                    &signed(-cost),
                    LOSS_FX,
                );
                fx.ring(center(tile(j)), TEAL_FX, 40.);
            }
            Beat::Refused(j, why) => {
                let badge = vec2(BADGE_X, project_rect(j).y + 38.);
                fx.ring(badge, LOSS_FX, 34.);
                fx.shake(0.12);
                let (glyph, word) = match why {
                    Refusal::Built => (Icon::Check, "Built"),
                    Refusal::Coin => (Icon::Coin, "No coin"),
                    Refusal::Late => (Icon::Hourglass, "Too late"),
                };
                fx.popup_icon(badge - vec2(60., 6.), glyph, word, LOSS_FX);
            }
            Beat::Selected(i) => {
                let r = if game.screen == Screen::Projects {
                    project_rect(i)
                } else {
                    choice_rect(i)
                };
                fx.ring(vec2(71., r.y + r.h * 0.5), TEAL_FX, 26.);
            }
            Beat::Toggled(screen) => {
                let button = match screen {
                    Screen::Projects => WORKS_BUTTON,
                    Screen::Chronicle => CHRONICLE_BUTTON,
                    _ => continue,
                };
                fx.ring(center(button), TEAL_FX, 26.);
            }
            Beat::Paged(_) => {
                fx.ring(center(PAGE_PREV).lerp(center(PAGE_NEXT), 0.5), TEAL_FX, 24.);
            }
            Beat::Begun => {
                fx.ring(center(crate::model::BEGIN), GOLD_FX, 60.);
            }
        }
    }
}
/// A sealed decree: the wax seal stamps, each moved pillar shows its actual
/// change, the policy's leaning gains a pip, and danger tints the edges.
fn decreed(game: &Game, council: usize, fx: &mut Juice) {
    let Some(record) = game.history.get(council).copied().flatten() else {
        return;
    };
    let choice = record.petition.choices[record.choice];
    let (_, color) = POLICIES[choice.legacy];
    let stamp = vec2(SEAL_X, choice_rect(record.choice).y + 35.);
    fx.ring(stamp, color, 48.);
    fx.burst(stamp, color, 18);
    for k in 0..4 {
        let delta = record.after[k] - record.before[k];
        if delta == 0 {
            continue;
        }
        let top = level_y(f32::from(record.after[k]));
        let tone = if delta > 0 { GAIN_FX } else { LOSS_FX };
        let at = vec2(GAUGE_X[k], popup_y(top));
        fx.popup_icon(at, PILLARS[k].0, &signed(delta), tone);
        if delta.abs() >= 10 {
            fx.ring(vec2(GAUGE_X[k], top), tone, 30.);
        }
    }
    let n = usize::from(game.legacy[choice.legacy]).saturating_sub(1);
    fx.sparkle(pip(choice.legacy, n.min(29)), color);
    if game.outcome() != Outcome::Playing {
        return;
    }
    let low = *record.after.iter().min().unwrap_or(&100);
    if low <= DANGER {
        let severity = f32::from(DANGER + 1 - low) / f32::from(DANGER);
        fx.vignette(Color::new(
            0.85,
            0.12,
            0.1,
            0.2 + 0.25 * severity.clamp(0., 1.),
        ));
        for (x, (before, after)) in GAUGE_X
            .into_iter()
            .zip(record.before.into_iter().zip(record.after))
        {
            if after <= DANGER && before > DANGER {
                fx.ring(vec2(x, level_y(f32::from(DANGER))), LOSS_FX, 36.);
                fx.shake(0.18);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Screen;

    fn press(game: &mut Game, input: Input) -> Juice {
        let cue = game.step(&input);
        let mut fx = Juice::new();
        effects(game, cue, &mut fx);
        fx
    }
    fn begin(game: &mut Game) {
        while game.screen != Screen::Council {
            game.step(&Input {
                primary: true,
                ..Input::default()
            });
        }
    }
    #[test]
    fn a_sealed_decree_is_felt_on_its_card_and_every_moved_pillar() {
        let mut game = Game::new();
        begin(&mut game);
        let fx = press(
            &mut game,
            Input {
                slot: Some(1),
                ..Input::default()
            },
        );
        let record = game.history[0].unwrap();
        let moved = (0..4)
            .filter(|&k| record.after[k] != record.before[k])
            .count();
        let counts = fx.counts();
        assert_eq!(counts.popups, moved, "one popup per moved pillar");
        assert!(counts.rings >= 1 && counts.sparks > 0);
        // The first decree promises the levee: its name is toasted.
        assert!(counts.toast);
        // An idle tick that follows adds nothing.
        let idle = press(&mut game, Input::default());
        assert_eq!(idle.counts(), Default::default());
    }
    #[test]
    fn refused_and_accepted_commissions_are_felt_differently() {
        let mut game = Game::new();
        begin(&mut game);
        game.step(&Input {
            secondary: true,
            ..Input::default()
        });
        let bought = press(
            &mut game,
            Input {
                slot: Some(1),
                ..Input::default()
            },
        );
        assert_eq!(bought.counts().popups, 1);
        assert!(bought.counts().rings >= 2);
        let refused = press(
            &mut game,
            Input {
                slot: Some(1),
                ..Input::default()
            },
        );
        assert_eq!(refused.counts().popups, 1, "a one-word refusal");
        assert_eq!(refused.counts().rings, 1);
        assert_eq!(refused.counts().sparks, 0);
    }
    #[test]
    fn a_new_act_gets_a_callout_and_collapse_marks_the_fallen_pillar() {
        let mut game = Game::new();
        begin(&mut game);
        while game.count < 9 {
            let slot = game.strategic_choice(None) + 1;
            game.step(&Input {
                slot: Some(slot),
                ..Input::default()
            });
        }
        let slot = game.strategic_choice(None) + 1;
        let fx = press(
            &mut game,
            Input {
                slot: Some(slot),
                ..Input::default()
            },
        );
        assert_eq!(game.screen, Screen::Briefing);
        assert!(game.beats().any(|b| b == Beat::Act(1)));
        assert!(fx.counts().sparks > 30, "confetti for the new act");

        let mut doomed = Game::new();
        let mut last = Juice::new();
        while doomed.outcome() == Outcome::Playing {
            begin(&mut doomed);
            last = press(
                &mut doomed,
                Input {
                    slot: Some(1),
                    ..Input::default()
                },
            );
        }
        let fallen = doomed
            .beats()
            .find_map(|b| match b {
                Beat::Collapse(k) => Some(k),
                _ => None,
            })
            .expect("collapse beat");
        assert_eq!(doomed.resources[fallen], 0);
        assert!(last.counts().sparks >= 40);
    }
}
