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
        let width = (screen.x - 2. * MARGIN).max(1.);
        let board = Rect::new(0., 48., screen.x, (screen.y - 200.).max(60.));
        let dock = screen.y - 144.;
        let column = (width - 3. * GAP) / 4.;
        // Plot choices are an on-demand palette, not a second copy of the board.
        let plots = std::array::from_fn(|i| {
            Rect::new(
                MARGIN + (i % 4) as f32 * (column + GAP),
                dock - 108. + (i / 4) as f32 * 50.,
                column,
                44.,
            )
        });
        let detail = Rect::new(108., dock + 4., (width - 100.).max(1.), 38.);
        let half = (width - GAP) * 0.5;
        let build = [
            Rect::new(MARGIN, dock + 46., half, 44.),
            Rect::new(MARGIN + half + GAP, dock + 46., half, 44.),
        ];
        let third = (width - 2. * GAP) / 3.;
        let wave = Rect::new(MARGIN, dock + 96., third, 44.);
        let menu = [
            Rect::new(MARGIN + third + GAP, dock + 96., third, 44.),
            Rect::new(MARGIN + 2. * (third + GAP), dock + 96., third, 44.),
        ];
        let preview = Rect::new(MARGIN, 52., width - 64., 18.);
        let notice = Rect::new(MARGIN, 76., width, 36.);
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
    _input: &PlayerInput,
    picker: bool,
) {
    let g = &campaign.game;
    if campaign.session.is_paused() || g.outcome != Outcome::Playing {
        return;
    }
    ui.panel(area(Rect::new(0., 0., layout.screen.x, 48.)), CREAM)
        .unwrap();
    ui.panel(
        area(Rect::new(0., layout.screen.y - 148., layout.screen.x, 148.)),
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
    if picker {
        ui.panel(
            area(Rect::new(0., layout.plots[0].y - 8., layout.screen.x, 108.)),
            CREAM,
        )
        .unwrap();
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
    }
    control(
        ui,
        9,
        Rect::new(MARGIN, layout.screen.y - 140., 92., 44.),
        &format!("PLOT {}", g.selected + 1),
        true,
        picker,
    );
    let detail = if let Some(tower) = g.towers[g.selected] {
        format!(
            "{} L{}\n{:.0} DMG / {:.0}R",
            tower.kind.name(),
            tower.level + 1,
            tower.damage(),
            tower.range()
        )
    } else {
        "EMPTY PLOT\nChoose a tower".into()
    };
    text(ui, layout.detail, &detail, INK);
    if let Some(tower) = g.towers[g.selected] {
        let status = g.upgrade_status(g.selected);
        control(
            ui,
            3,
            Rect::new(
                layout.build[0].x,
                layout.build[0].y,
                layout.screen.x - 16.,
                44.,
            ),
            &if tower.level == 2 {
                "FULLY GROWN".into()
            } else {
                format!("UPGRADE {} COINS", tower.upgrade_cost())
            },
            status.is_ok(),
            false,
        );
    } else {
        for (i, kind, name) in [(0, Kind::Seed, "SEED"), (1, Kind::Frost, "FROST")] {
            control(
                ui,
                i as u64 + 1,
                layout.build[i],
                &format!("{name} {}", kind.price()),
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
            format!("{} LEFT", g.pests.len() + usize::from(g.remaining))
        } else {
            format!("WAVE {}", g.wave + 1)
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
    if !g.active
        && let Some(next) = g.next_wave()
    {
        text(
            ui,
            layout.preview,
            &format!("NEXT: {} PESTS", next.spec.count),
            GREEN,
        );
    }
    if save_error {
        text(ui, layout.notice, "SAVE UNAVAILABLE", CREAM);
    } else if let Some(message) = campaign.notice() {
        ui.panel(area(layout.notice), GREEN).unwrap();
        text(ui, layout.notice, notice_text(message), CREAM);
    }
}

pub(super) fn overlay(ui: &mut Ui, layout: Layout, campaign: &Campaign, _input: &PlayerInput) {
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
        ((layout.screen.y - 220.) * 0.5).max(0.),
        layout.screen.x - 16.,
        220.,
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
    if paused {
        control(ui, 7, line(100., 44.), "RESUME", true, false);
    }
    control(ui, 8, line(154., 44.), "PLANT AGAIN", true, false);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portrait_controls_remain_readable_and_contained() {
        for (width, height) in [
            (320., 480.),
            (390., 568.),
            (844., 390.),
            (320., 734.),
            (390., 769.),
            (540., 776.),
            (899., 776.),
        ] {
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
            assert!(
                layout.plots[0].y > layout.board.y,
                "palette overlays the board only on demand"
            );
            for label in ["SEED 60", "FROST 80"] {
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
                hud(ui, layout, campaign, false, input, true);
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
                    .pointer(vec2(100., layout.build[0].y + 22.))
                    .is_none()
            );
        }
    }
}
