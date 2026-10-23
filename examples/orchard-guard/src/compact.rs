//! Narrow-window presentation. The board keeps its world coordinates while the
//! existing shared UI uses logical pixels, including its pointer/touch handling.
use super::*;

const MARGIN: f32 = 8.;
const GAP: f32 = 6.;
const FONT: f32 = 14.;
pub(super) const PLOT_ID: u64 = 200;

#[derive(Clone, Copy)]
pub(super) struct Layout {
    screen: Vec2,
    pub board: Rect,
    pub plots: [Rect; 8],
    pub detail: Rect,
    pub build: [Rect; 2],
    pub wave: Rect,
    pub menu: [Rect; 2],
    pub preview: Rect,
    pub notice: Rect,
}

impl Layout {
    pub fn new(screen: Vec2) -> Self {
        let board = Rect::new(0., 48., screen.x, (screen.x * 590. / 1200.).min(200.));
        let width = (screen.x - 2. * MARGIN).max(1.);
        let column = (width - 3. * GAP) / 4.;
        let y = board.y + board.h + GAP;
        let plots = std::array::from_fn(|i| {
            Rect::new(
                MARGIN + (i % 4) as f32 * (column + GAP),
                y + (i / 4) as f32 * (44. + GAP),
                column,
                44.,
            )
        });
        let detail = Rect::new(MARGIN, y + 100., width, 64.);
        let pair = |y, height| {
            let half = (width - GAP) * 0.5;
            [
                Rect::new(MARGIN, y, half, height),
                Rect::new(MARGIN + half + GAP, y, half, height),
            ]
        };
        let build = pair(detail.y + detail.h + GAP, 48.);
        let wave = Rect::new(MARGIN, build[0].y + 54., width, 44.);
        let menu = pair(wave.y + 50., 44.);
        let preview = Rect::new(MARGIN, menu[0].y + 50., width, 48.);
        let notice = Rect::new(MARGIN, preview.y + 54., width, 48.);
        Self {
            screen,
            board,
            plots,
            detail,
            build,
            wave,
            menu,
            preview,
            notice,
        }
    }

    fn scale(self) -> f32 {
        (self.board.w / 1200.).min(self.board.h / 590.)
    }

    pub fn project(self, point: Vec2) -> Vec2 {
        vec2(
            self.board.x + self.board.w * 0.5,
            self.board.y + self.board.h * 0.5,
        ) + (point - vec2(600., 365.)) * self.scale()
    }

    pub fn pointer(self, point: Vec2) -> Option<Vec2> {
        let world = (point - self.project(vec2(600., 365.))) / self.scale() + vec2(600., 365.);
        (self.board.contains(point)
            && (0. ..=1200.).contains(&world.x)
            && (70. ..=660.).contains(&world.y))
        .then_some(world)
    }

    pub fn camera(self) -> Camera2D {
        Camera2D {
            target: vec2(600., 365.)
                + (self.screen * 0.5 - self.project(vec2(600., 365.))) / self.scale(),
            zoom: Vec2::splat(2. * self.scale()) / self.screen,
            ..Default::default()
        }
    }
}

fn area(rect: Rect) -> UiRect {
    UiRect {
        min: [rect.x, rect.y],
        size: [rect.w, rect.h],
    }
}
fn text(ui: &mut Ui, rect: Rect, value: &str, color: Color) {
    ui.text(
        area(rect),
        value,
        TextStyle {
            size: FONT,
            color,
            wrap: true,
            ..Default::default()
        },
    )
    .unwrap();
}
fn control(ui: &mut Ui, id: u64, rect: Rect, value: &str, enabled: bool, selected: bool) {
    ui.hud_button(
        id,
        area(rect),
        value,
        enabled,
        ButtonStyle {
            fill: if selected { GOLD } else { GREEN },
            ink: if selected { INK } else { CREAM },
            focus: GOLD,
            disabled: Color::from_hex(0x616956),
            size: FONT,
        },
    )
    .unwrap();
}

pub(super) fn hud(
    ui: &mut Ui,
    layout: Layout,
    campaign: &Campaign,
    save_error: bool,
    input: &PlayerInput,
) {
    let g = &campaign.game;
    ui.panel(area(Rect::new(0., 0., layout.screen.x, 48.)), CREAM)
        .unwrap();
    ui.panel(
        area(Rect::new(
            0.,
            layout.board.y + layout.board.h,
            layout.screen.x,
            (layout.screen.y - layout.board.y - layout.board.h).max(0.),
        )),
        CREAM,
    )
    .unwrap();
    text(
        ui,
        Rect::new(MARGIN, 5., layout.screen.x - 16., 20.),
        &format!(
            "WAVE {}/{}   {} HEARTS",
            g.wave,
            g.level.waves.len(),
            g.lives
        ),
        INK,
    );
    text(
        ui,
        Rect::new(MARGIN, 25., layout.screen.x - 16., 20.),
        &format!("{} COINS   {} STOPPED", g.money, g.kills),
        INK,
    );
    // Large screen-space numbers identify the board even at 320 logical pixels.
    for (i, plot) in g.level.plots.iter().enumerate() {
        let p = layout.project(*plot);
        ui.label(
            area(Rect::new(p.x - 9., p.y + 6., 18., 18.)),
            &(i + 1).to_string(),
            TextStyle {
                size: FONT,
                color: INK,
                align: Align::Center,
                ..Default::default()
            },
        )
        .unwrap();
    }
    for (i, rect) in layout.plots.into_iter().enumerate() {
        control(
            ui,
            PLOT_ID + i as u64,
            rect,
            &(i + 1).to_string(),
            g.outcome == Outcome::Playing && !campaign.session.is_paused(),
            g.selected == i,
        );
    }
    let detail = if let Some(tower) = g.towers[g.selected] {
        let next = Tower {
            level: (tower.level + 1).min(2),
            ..tower
        };
        format!(
            "PLOT {}: {} L{}\n{:.0} DMG / {:.0} RANGE / {:.2}S\nNEXT: {:.0} DMG / {:.0}R / {:.2}S\n{}",
            g.selected + 1,
            tower.kind.name(),
            tower.level + 1,
            tower.damage(),
            tower.range(),
            tower.reload(),
            next.damage(),
            next.range(),
            next.reload(),
            availability(
                g.upgrade_status(g.selected),
                if tower.kind == Kind::Frost {
                    "SLOWS TO 45% SPEED"
                } else {
                    "TARGETS NEAREST EXIT"
                }
            )
        )
    } else {
        let seed = Tower::planted(Kind::Seed);
        let frost = Tower::planted(Kind::Frost);
        let funds = match (
            g.build_status(g.selected, Kind::Seed),
            g.build_status(g.selected, Kind::Frost),
        ) {
            (
                Err(ActionError::Funds { missing: seed }),
                Err(ActionError::Funds { missing: frost }),
            ) => format!("NEED {seed} SEED / {frost} FROST COINS"),
            (_, Err(ActionError::Funds { missing })) => format!("FROST NEEDS {missing} MORE COINS"),
            _ => "FROST SLOWS TO 45% SPEED".into(),
        };
        format!(
            "EMPTY PLOT {}\nSEED {:.0} DMG / {:.0}R / {:.2}S\nFROST {:.0} DMG / {:.0}R / {:.2}S\n{}",
            g.selected + 1,
            seed.damage(),
            seed.range(),
            seed.reload(),
            frost.damage(),
            frost.range(),
            frost.reload(),
            funds
        )
    };
    text(ui, layout.detail, &detail, INK);
    if let Some(tower) = g.towers[g.selected] {
        let status = g.upgrade_status(g.selected);
        control(
            ui,
            3,
            Rect::new(layout.build[0].x, layout.build[0].y, layout.detail.w, 48.),
            &if tower.level == 2 {
                "FULLY GROWN".into()
            } else {
                format!(
                    "UPGRADE / {} COINS [{}]",
                    tower.upgrade_cost(),
                    hint(input, "upgrade")
                )
            },
            status.is_ok(),
            false,
        );
    } else {
        for (i, kind, action, name) in [
            (0, Kind::Seed, "select_one", "SEED"),
            (1, Kind::Frost, "select_two", "FROST"),
        ] {
            control(
                ui,
                i as u64 + 1,
                layout.build[i],
                &format!("{name} {} [{}]", kind.price(), hint(input, action)),
                g.build_status(g.selected, kind).is_ok(),
                false,
            );
        }
    }
    control(
        ui,
        4,
        layout.wave,
        &if g.active {
            format!("{} PESTS LEFT", g.pests.len() + usize::from(g.remaining))
        } else {
            format!("SEND WAVE {} [{}]", g.wave + 1, hint(input, "wave"))
        },
        g.wave_status().is_ok(),
        false,
    );
    control(
        ui,
        6,
        layout.menu[0],
        "PAUSE",
        campaign.session.is_started(),
        false,
    );
    control(ui, 5, layout.menu[1], "RESTART", true, false);
    let preview = if let Some(next) = g.next_wave() {
        format!(
            "NEXT: {} PESTS / {} RUNNERS\nHEALTH {:.0}/{:.0} SPEED {:.0}/{:.0}\nEVERY {:.2}S / +{} COINS",
            next.spec.count,
            next.fast_count,
            next.spec.hp,
            next.spec.hp * next.spec.fast_factor,
            next.spec.speed,
            next.spec.fast_speed,
            next.spec.interval,
            next.spec.reward
        )
    } else {
        format!(
            "{} ON PATH / {} ARRIVING\nBuild and upgrade during waves.",
            g.pests.len(),
            g.remaining
        )
    };
    text(ui, layout.preview, &preview, GREEN);
    let message = if save_error {
        "Save unavailable. Keep this window open."
    } else {
        campaign
            .notice()
            .unwrap_or(if g.towers.iter().all(Option::is_none) {
                "Choose a plot, then plant a tower before sending a wave."
            } else {
                "Select plots above. Seed hits hard; Frost slows pests."
            })
    };
    text(ui, layout.notice, message, INK);
}

pub(super) fn overlay(ui: &mut Ui, layout: Layout, campaign: &Campaign, input: &PlayerInput) {
    let g = &campaign.game;
    let paused = campaign.session.is_paused();
    if !paused && g.outcome == Outcome::Playing {
        return;
    }
    ui.modal(100).unwrap();
    ui.panel(
        area(Rect::new(0., 0., layout.screen.x, layout.screen.y)),
        Color::new(0.14, 0.20, 0.13, 0.75),
    )
    .unwrap();
    let card = Rect::new(
        8.,
        ((layout.screen.y - 284.) * 0.5).max(0.),
        layout.screen.x - 16.,
        284.,
    );
    ui.panel(area(card), CREAM).unwrap();
    let line = |y, height| Rect::new(card.x + 8., card.y + y, card.w - 16., height);
    text(
        ui,
        line(16., 24.),
        if paused {
            "ORCHARD PAUSED"
        } else if g.outcome == Outcome::Won {
            "HARVEST SAVED"
        } else {
            "THE PESTS GOT THROUGH"
        },
        INK,
    );
    text(
        ui,
        line(46., 40.),
        &format!(
            "WAVE {}/{}\n{} STOPPED / {} HEARTS",
            g.wave,
            g.level.waves.len(),
            g.kills,
            g.lives
        ),
        GREEN,
    );
    text(
        ui,
        line(88., 56.),
        if paused {
            "Pests, reloads and the harvest clock are frozen."
        } else if g.outcome == Outcome::Won {
            "Every wave cleared. Plant again for a fresh run."
        } else {
            "Cover bends and mix Seed and Frost towers. Try a new planting plan."
        },
        INK,
    );
    if paused {
        control(
            ui,
            7,
            line(156., 44.),
            &format!("RESUME [{}]", hint(input, "pause")),
            true,
            false,
        );
    }
    control(
        ui,
        8,
        line(210., 44.),
        &format!("PLANT AGAIN [{}]", hint(input, "restart")),
        true,
        false,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portrait_controls_remain_readable_and_contained() {
        for (width, height) in [(320., 734.), (390., 769.), (540., 776.), (899., 776.)] {
            let layout = Layout::new(vec2(width, height));
            for rect in layout
                .plots
                .into_iter()
                .chain(layout.build)
                .chain([layout.wave])
                .chain(layout.menu)
            {
                assert!(rect.w >= 44. && rect.h >= 44., "{rect:?}");
                assert!(
                    rect.x >= 0.
                        && rect.y >= 0.
                        && rect.x + rect.w <= width
                        && rect.y + rect.h <= height
                );
            }
            assert!(layout.notice.y + layout.notice.h <= height);
            assert!(layout.plots[0].y >= layout.board.y + layout.board.h);
            for label in ["SEED 60 [1]", "FROST 80 [2]"] {
                assert!(
                    label.len() as f32 * FONT * 0.75 <= layout.build[0].w - 16.,
                    "{label} at {width}"
                );
            }
        }
    }

    #[test]
    fn compact_touch_selects_builds_and_modal_blocks_underlying_controls() {
        use ferravex_render::input::{
            self,
            touch::{Event, Phase},
        };
        let layout = Layout::new(vec2(320., 734.));
        let mut ui = Ui::new();
        ui.set_auto_focus(false);
        let mut input = PlayerInput::transient("orchard-compact-test");
        let mut campaign = Campaign::new(Game::new(), false);
        let reserved = ui.reserved_bytes();
        let frame = |ui: &mut Ui, input: &mut PlayerInput, campaign: &mut Campaign| {
            input::host_frame(0., 320, 734, || {
                input.sample(true);
                ui.begin(Viewport::responsive([320., 734.], [0.; 4]).unwrap());
                hud(ui, layout, campaign, false, input);
                overlay(ui, layout, campaign, input);
                ui.finish_player(input, true);
                let action = hud_action(ui, campaign.game.level.plots.len());
                campaign.frame(0., false, false, [action, None, None]);
                action
            })
        };
        frame(&mut ui, &mut input, &mut campaign);
        let tap = |rect: Rect| {
            for phase in [Phase::Start, Phase::End] {
                input::touch_event(Event {
                    id: 42,
                    phase,
                    position: vec2(rect.x + rect.w * 0.5, rect.y + rect.h * 0.5),
                });
            }
        };
        tap(layout.plots[7]);
        assert_eq!(
            frame(&mut ui, &mut input, &mut campaign),
            Some(Action::Select(Some(7)))
        );
        assert_eq!(campaign.game.selected, 7);
        tap(layout.build[1]);
        assert_eq!(
            frame(&mut ui, &mut input, &mut campaign),
            Some(Action::Build(Kind::Frost))
        );
        assert_eq!(campaign.game.towers[7].unwrap().kind, Kind::Frost);
        assert_eq!(campaign.game.money, 120);
        // A held-start gesture alone cannot purchase; releasing outside cancels.
        input::touch_event(Event {
            id: 43,
            phase: Phase::Start,
            position: vec2(100., layout.build[0].y + 20.),
        });
        assert_eq!(frame(&mut ui, &mut input, &mut campaign), None);
        input::touch_event(Event {
            id: 43,
            phase: Phase::End,
            position: vec2(1., 1.),
        });
        assert_eq!(frame(&mut ui, &mut input, &mut campaign), None);
        assert_eq!(campaign.game.money, 120);
        campaign.frame(0., false, false, [Some(Action::Wave), None, None]);
        campaign.frame(0., false, false, [Some(Action::Pause), None, None]);
        tap(layout.menu[1]);
        assert_eq!(frame(&mut ui, &mut input, &mut campaign), None);
        assert!(campaign.session.is_paused());
        assert_eq!(campaign.game.money, 120);
        assert_eq!(ui.reserved_bytes(), reserved);
    }

    #[test]
    fn narrow_board_camera_and_hit_transform_agree_after_resize() {
        for screen in [vec2(320., 734.), vec2(390., 769.), vec2(899., 776.)] {
            let layout = Layout::new(screen);
            let game = Game::new();
            for plot in &game.level.plots {
                let p = layout.project(*plot);
                assert!(layout.pointer(p).unwrap().distance(*plot) < 0.001);
                let camera = layout.camera();
                let rendered = screen * 0.5 + (*plot - camera.target) * camera.zoom * screen * 0.5;
                assert!(rendered.distance(p) < 0.001);
            }
            assert!(layout.pointer(vec2(100., 20.)).is_none());
            assert!(
                layout
                    .pointer(vec2(100., layout.plots[0].y + 22.))
                    .is_none()
            );
        }
    }
}
