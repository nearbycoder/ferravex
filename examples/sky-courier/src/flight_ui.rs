//! Pixel-sized flight HUD; inactive menus replace conflicting navigation rows.
use super::*;

const PANEL: Color = Color::new(0.08, 0.17, 0.19, 0.96);
const BODY_SIZE: f32 = 14.;
const MENU_HELP: &str = "Controls / F2: bindings\nFlight guide below";
const FOOTER_HELP: &str = "Controls / F2: bindings";

struct Layout {
    viewport: Viewport,
    x: f32,
    width: f32,
    height: f32,
    narrow: bool,
}

impl Layout {
    fn new(screen: [f32; 2]) -> Result<Self, ferravex_play::ui::UiError> {
        let viewport = Viewport::responsive(screen, [8.; 4])?;
        let [available, height] = viewport.bounds().size;
        let width = available.min(960.);
        Ok(Self {
            viewport,
            x: (available - width) * 0.5,
            width,
            height,
            narrow: width < 600.,
        })
    }

    fn rect(&self, x: f32, y: f32, w: f32, h: f32) -> UiRect {
        ui_rect(self.x + x, y, w.max(0.), h.max(0.))
    }

    fn row(&self, y: f32, height: f32) -> UiRect {
        self.rect(12., y, self.width - 24., height)
    }

    fn menu(&self) -> MenuLayout {
        let title = if self.narrow { 40. } else { 28. };
        let detail = if self.narrow { 112. } else { 64. };
        let summary = if self.narrow { 84. } else { 56. };
        let confirm = if self.narrow { 42. } else { 28. };
        let help = 28.;
        let height =
            16. + title + 8. + detail + 8. + summary + 12. + 48. + 12. + confirm + 8. + help + 16.;
        // The web host reserves enough height, including in scrollable fullscreen.
        // Keep top alignment in smaller native windows instead of shrinking text.
        let y = 84. + ((self.height - 84. - height) * 0.5).max(0.);
        let mut next = y + 16.;
        let mut row = |height, gap| {
            let rect = self.row(next, height);
            next += height + gap;
            rect
        };
        let title = row(title, 8.);
        let detail = row(detail, 8.);
        let summary = row(summary, 12.);
        let button_y = next;
        next += 60.;
        let confirm_rect = self.row(next, confirm);
        next += confirm + 8.;
        let help = self.row(next, help);
        let button_width = (self.width - 24.).min(240.);
        MenuLayout {
            panel: self.rect(0., y, self.width, height),
            title,
            detail,
            summary,
            button: self.rect(
                (self.width - button_width) * 0.5,
                button_y,
                button_width,
                48.,
            ),
            confirm: confirm_rect,
            help,
        }
    }

    fn footer(&self) -> UiRect {
        let height = if self.narrow { 190. } else { 124. };
        self.rect(0., (self.height - height).max(0.), self.width, height)
    }
}

struct MenuLayout {
    panel: UiRect,
    title: UiRect,
    detail: UiRect,
    summary: UiRect,
    button: UiRect,
    confirm: UiRect,
    help: UiRect,
}

fn menu_copy(state: SessionState, hull: u8) -> (&'static str, &'static str, &'static str) {
    match state {
        SessionState::Ready => (
            "NORTH COAST MAIL RUN",
            "Clear five amber gates in order. Pass the cross at 16 m altitude and at most 18 m/s. You have 90s; three impacts end the run.",
            "START FLIGHT",
        ),
        SessionState::Paused => (
            "FLIGHT PAUSED",
            "Flight and timer are frozen. Close Controls, return focus to the game, then resume here or use your Pause binding.",
            "RESUME FLIGHT",
        ),
        SessionState::Won => (
            "MAIL DELIVERED",
            "All five gates cleared. The north coast is connected again. Fly again to start a fresh mail run.",
            "FLY AGAIN",
        ),
        SessionState::Lost if hull == 0 => (
            "AIRCRAFT GROUNDED",
            "Hull exhausted. Use smaller turns and watch the altitude guidance. Retry restores the aircraft and all 90 seconds.",
            "RETRY FLIGHT",
        ),
        SessionState::Lost => (
            "DELIVERY WINDOW CLOSED",
            "Time ran out. Impacts cost five seconds. Clear gates in order, then slow to 18 m/s before crossing the pad at 16 m altitude.",
            "RETRY FLIGHT",
        ),
        SessionState::Playing => unreachable!(),
    }
}

// Hints are complete current bindings or an explicit route to the editor. Never
// clip a key name, substitute a default, or make the font smaller to fit a remap.
fn binding_fits(label: &str, first: &str, second: &str, width: f32) -> bool {
    let count = label.chars().count()
        + 2
        + first.chars().count()
        + if second.is_empty() {
            0
        } else {
            1 + second.chars().count()
        };
    count as f32 * BODY_SIZE * 0.75 <= width
}

fn binding_row(
    ui: &mut Ui,
    rect: UiRect,
    label: &str,
    first: &str,
    second: &str,
) -> Result<(), ferravex_play::ui::UiError> {
    let (first, second) = if binding_fits(label, first, second, rect.size[0]) {
        (first, second)
    } else {
        ("Open Controls", "")
    };
    ui.text_fmt(
        rect,
        ui_text(BODY_SIZE, CREAM, Align::Left),
        format_args!(
            "{}: {}{}{}",
            label,
            first,
            if second.is_empty() { "" } else { " " },
            second
        ),
    )
}

fn queue(
    run: &FlightRun,
    ui: &mut Ui,
    player: &PlayerInput,
    layout: &Layout,
) -> Result<(), ferravex_play::ui::UiError> {
    let game = &run.flight;
    let width = layout.width;
    let seconds = game.countdown();
    ui.panel(layout.rect(0., 0., width, 48.), PANEL)?;
    ui.text(
        layout.rect(12., 6., width - 110., 20.),
        "SKY COURIER",
        ui_text(18., CREAM, Align::Left),
    )?;
    ui.text_fmt(
        layout.rect(width - 96., 6., 84., 20.),
        ui_text(18., if seconds <= 15 { AMBER } else { CREAM }, Align::Right),
        format_args!("{:02}:{:02}", seconds / 60, seconds % 60),
    )?;
    ui.text(
        layout.rect(12., 29., width - 110., 16.),
        "NORTH COAST POST",
        ui_text(12., CREAM, Align::Left),
    )?;
    ui.text_fmt(
        layout.rect(width - 108., 29., 96., 16.),
        ui_text(14., CREAM, Align::Right),
        format_args!("HULL {}/3", game.hull),
    )?;

    for (i, slot) in
        ferravex_play::ui::slots(layout.rect(0., 54., width, 22.), ROUTE.len(), 4., false)?
            .enumerate()
    {
        ui.panel(
            slot,
            if i < game.next {
                CREAM
            } else if i == game.next {
                AMBER
            } else {
                PANEL
            },
        )?;
        let style = ui_text(14., if i <= game.next { INK } else { CREAM }, Align::Center);
        if i == GATES {
            ui.text(slot, if layout.narrow { "POST" } else { "DELIVER" }, style)?;
        } else if layout.narrow {
            ui.text_fmt(slot, style, format_args!("{}", i + 1))?;
        } else {
            ui.text_fmt(slot, style, format_args!("GATE {}", i + 1))?;
        }
    }

    let state = run.session.state();
    if state != SessionState::Playing {
        let menu = layout.menu();
        let (title, detail, button) = menu_copy(state, game.hull);
        ui.panel(menu.panel, PANEL)?;
        ui.text(
            menu.title,
            title,
            ui_text(if layout.narrow { 20. } else { 24. }, CREAM, Align::Center),
        )?;
        ui.text(menu.detail, detail, ui_text(16., CREAM, Align::Center))?;
        if matches!(state, SessionState::Won | SessionState::Lost) {
            ui.text_fmt(menu.summary, ui_text(BODY_SIZE, AMBER, Align::Center), format_args!(
                "Gates {}/{} / flight {:.1}s\nImpacts {} (-{}s)\nRetry resets route, hull and time. Release held controls.",
                game.next.min(GATES), GATES, run.elapsed, 3 - game.hull, (3 - game.hull) * 5))?;
        } else {
            ui.text(menu.summary, "Each impact costs 5s. Recover at the last gate (or start). Release held flight controls after starting or retrying.", ui_text(BODY_SIZE, AMBER, Align::Center))?;
        }
        ui.button_with_options(
            1,
            menu.button,
            button,
            ButtonOptions {
                enabled: !player.controls_open(),
                selected: false,
            },
            ButtonStyle {
                fill: AMBER,
                ink: INK,
                focus: CREAM,
                ..Default::default()
            },
        )?;
        let mut buffer = [0; 96];
        let confirm = control_hint(player, "confirm", &mut buffer);
        ui.text_fmt(
            menu.confirm,
            ui_text(BODY_SIZE, CREAM, Align::Center),
            format_args!("Confirm: {} / or select the button", confirm),
        )?;
        ui.text(
            menu.help,
            MENU_HELP,
            ui_text(BODY_SIZE, CREAM, Align::Center),
        )?;
        // No navigation, recovery or telemetry beneath an inactive menu.
        return Ok(());
    }

    let mut slow_buffer = [0; 96];
    let slow = control_hint(player, "brake", &mut slow_buffer);
    if let Some(nav) = game.navigation() {
        ui.panel(layout.rect(0., 82., width, 86.), PANEL)?;
        ui.text_fmt(
            layout.row(88., 32.),
            ui_text(16., AMBER, Align::Center),
            format_args!("{} / {} / {:.0} m", nav.turn(), nav.climb(), nav.distance),
        )?;
        if nav.bearing.abs() > std::f32::consts::FRAC_PI_2 {
            ui.text(
                layout.row(122., 42.),
                "Target behind: circle back toward the next gate or delivery cross.",
                ui_text(BODY_SIZE, CREAM, Align::Center),
            )?;
        } else if game.next == GATES && game.motor.speed > DELIVERY_SPEED {
            ui.text_fmt(
                layout.row(122., 42.),
                ui_text(BODY_SIZE, CREAM, Align::Center),
                format_args!("SLOW DOWN: {}\nAt most 18 m/s (35 kt)", slow),
            )?;
        } else if game.next == GATES {
            ui.text(
                layout.row(122., 42.),
                "SPEED READY\nCross the pad at 16 m altitude",
                ui_text(BODY_SIZE, CREAM, Align::Center),
            )?;
        } else {
            ui.text_fmt(
                layout.row(122., 42.),
                ui_text(BODY_SIZE, CREAM, Align::Center),
                format_args!("Gate {} of 5: aim for the amber center", game.next + 1),
            )?;
        }
    }
    if game.recovery > 0. && game.outcome == Outcome::Flying {
        ui.panel(layout.rect(0., 174., width, 36.), PANEL)?;
        ui.text_fmt(
            layout.row(178., 28.),
            ui_text(BODY_SIZE, AMBER, Align::Center),
            format_args!(
                "{} / -5s\nProtected {:.1}s",
                if game.next == 0 {
                    "BACK AT START"
                } else {
                    "BACK AT LAST GATE"
                },
                game.recovery
            ),
        )?;
    } else if game.gate_notice > 0. && game.outcome == Outcome::Flying {
        ui.panel(layout.rect(0., 174., width, 36.), PANEL)?;
        ui.text_fmt(
            layout.row(178., 28.),
            ui_text(BODY_SIZE, AMBER, Align::Center),
            format_args!("GATE {} CLEARED\nBoost replenished", game.next),
        )?;
    }

    let footer = layout.footer();
    let bottom = footer.min[1];
    ui.panel(footer, PANEL)?;
    let stat_width = if layout.narrow {
        width - 24.
    } else {
        width * 0.5 - 24.
    };
    ui.text_fmt(
        layout.rect(12., bottom + 8., stat_width, 18.),
        ui_text(BODY_SIZE, CREAM, Align::Left),
        format_args!(
            "{:.0} kt / {:.0} m / {:+.1} m/s",
            game.motor.speed * 1.94,
            game.pos.y.max(0.),
            game.motor
                .telemetry()
                .expect("validated motor")
                .vertical_speed
        ),
    )?;
    ui.text_fmt(
        layout.rect(
            if layout.narrow { 12. } else { width * 0.5 },
            bottom + if layout.narrow { 26. } else { 8. },
            stat_width,
            18.,
        ),
        ui_text(
            BODY_SIZE,
            AMBER,
            if layout.narrow {
                Align::Left
            } else {
                Align::Right
            },
        ),
        format_args!(
            "{} / BOOST {}%",
            game.flight_mode(),
            (game.boost_energy * 100.) as u32
        ),
    )?;
    let bar = bottom + if layout.narrow { 46. } else { 30. };
    ui.panel(layout.rect(12., bar, width - 24., 3.), INK)?;
    ui.panel(
        layout.rect(12., bar, (width - 24.) * game.boost_energy, 3.),
        AMBER,
    )?;
    let mut turn_buffers = [[0; 96]; 3];
    let mut climb_buffers = [[0; 96]; 3];
    let mut boost_buffer = [0; 96];
    let mut pause_buffer = [0; 96];
    let mut retry_buffer = [0; 96];
    let turn = movement_hint(
        player,
        ["move_x", "move_left", "move_right"],
        &mut turn_buffers,
    );
    let climb = movement_hint(
        player,
        ["move_y", "move_forward", "move_back"],
        &mut climb_buffers,
    );
    let entries = [
        ("Turn", turn.0, turn.1),
        ("Climb", climb.0, climb.1),
        (
            "Boost",
            control_hint(player, "boost", &mut boost_buffer),
            "",
        ),
        ("Slow", slow, ""),
        (
            "Pause",
            control_hint(player, "pause", &mut pause_buffer),
            "",
        ),
        (
            "Retry",
            control_hint(player, "restart", &mut retry_buffer),
            "",
        ),
    ];
    for (index, (label, first, second)) in entries.into_iter().enumerate() {
        let (column, row) = if layout.narrow {
            (0, index)
        } else {
            (index % 2, index / 2)
        };
        let column_width = if layout.narrow { width } else { width * 0.5 };
        let rect = layout.rect(
            12. + column as f32 * column_width,
            bar + 9. + row as f32 * if layout.narrow { 18. } else { 20. },
            column_width - 24.,
            18.,
        );
        binding_row(ui, rect, label, first, second)?;
    }
    ui.text(
        layout.row(bottom + footer.size[1] - 18., 16.),
        FOOTER_HELP,
        ui_text(BODY_SIZE, CREAM, Align::Left),
    )?;
    Ok(())
}

pub(super) fn hud(
    run: &FlightRun,
    ui: &mut Ui,
    player: &mut PlayerInput,
    accept_input: bool,
) -> Result<bool, ferravex_play::ui::UiError> {
    let layout = Layout::new([screen_width(), screen_height()])?;
    ui.begin(layout.viewport);
    queue(run, ui, player, &layout)?;
    ui.finish_player(
        player,
        accept_input && run.session.state() != SessionState::Playing,
    );
    ui.paint();
    Ok(ui.activated(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferravex_play::ui::{UiState, Widget};
    use ferravex_render::input::{Binding, InputCode};

    #[test]
    fn essential_help_keeps_every_glyph_in_its_reserved_rows() {
        for screen in [[320., 600.], [390., 600.], [1280., 560.]] {
            let layout = Layout::new(screen).unwrap();
            for (text, rect) in [
                (MENU_HELP, layout.menu().help),
                (FOOTER_HELP, layout.row(0., 16.)),
            ] {
                // These deliberately explicit lines must never wrap into an
                // unavailable extra row. The bitmap advance is six of eight
                // cells per glyph; the old 27-character rows failed at 320px.
                assert!(text.lines().count() as f32 * BODY_SIZE <= rect.size[1]);
                for line in text.lines() {
                    assert!(line.chars().count() as f32 * BODY_SIZE * 0.75 <= rect.size[0]);
                }
            }
        }
    }

    #[test]
    fn portrait_menu_is_readable_and_pointer_targets_match_paint() {
        for screen in [[320., 600.], [390., 600.], [1280., 560.], [1280., 800.]] {
            let layout = Layout::new(screen).unwrap();
            assert_eq!(layout.viewport.scale(), 1., "never shrink HUD fonts");
            let menu = layout.menu();
            let bounds = layout.viewport.bounds();
            assert_eq!(menu.panel.intersect(bounds), Some(menu.panel));
            let rows = [
                menu.title,
                menu.detail,
                menu.summary,
                menu.button,
                menu.confirm,
                menu.help,
            ];
            for rect in rows {
                assert_eq!(rect.intersect(menu.panel), Some(rect));
            }
            for pair in rows.windows(2) {
                assert!(pair[0].min[1] + pair[0].size[1] <= pair[1].min[1]);
            }
            assert!(menu.panel.min[1] >= 84., "header/progress stay separate");
            assert!(menu.button.size[0] >= 44. && menu.button.size[1] >= 44.);
            let mut input = UiState::new();
            input
                .add(Widget {
                    id: 1,
                    rect: menu.button,
                    clip: Some(bounds),
                    enabled: true,
                    focusable: true,
                })
                .unwrap();
            let logical = [menu.button.min[0] + 1., menu.button.min[1] + 1.];
            let pixels =
                std::array::from_fn(|axis| layout.viewport.pixels().min[axis] + logical[axis]);
            let point = layout.viewport.unproject(pixels).unwrap();
            assert_eq!(input.pointer_down(point), Some(1));
            assert_eq!(input.pointer_up(point), Some(1));
            input.pointer_down(point);
            assert_eq!(input.pointer_up([menu.button.min[0] - 1., point[1]]), None);
            assert!(
                layout.footer().min[1] - 210. >= 160.,
                "a notice still leaves room to see the aircraft"
            );
        }
    }

    #[test]
    fn narrow_binding_rows_keep_whole_remaps_or_explicit_editor_fallback() {
        let mut player = PlayerInput::preset("sky-courier-layout-test", Preset::Flight);
        let mut buffer = [0; 96];
        let mut axis = [[0; 96]; 3];
        player
            .rebind("move_left", vec![Binding::digital(InputCode::Key(0xff55))])
            .unwrap();
        player
            .rebind("move_right", vec![Binding::digital(InputCode::Key(0xff56))])
            .unwrap();
        let hints = movement_hint(&player, ["move_x", "move_left", "move_right"], &mut axis);
        assert!(binding_fits("Turn", hints.0, hints.1, 280.));
        player
            .rebind(
                "boost",
                vec![Binding::digital(InputCode::Key(KeyCode::Backspace as u16))],
            )
            .unwrap();
        let boost = control_hint(&player, "boost", &mut buffer);
        assert_eq!(boost, "Backspace");
        assert!(binding_fits("Boost", boost, "", 280.));
        player.rebind("brake", vec![]).unwrap();
        assert_eq!(control_hint(&player, "brake", &mut buffer), "Unbound");
        assert!(binding_fits("Slow", "Unbound", "", 280.));
        assert!(!binding_fits(
            "Slow",
            "PageUp / PageDown / Backspace",
            "",
            280.
        ));
        assert!(binding_fits("Slow", "Open Controls", "", 280.));
        assert!(!binding_fits("Turn", "A / Left", "D / Right", 180.));
    }

    #[test]
    fn all_menu_and_flight_states_queue_within_existing_ui_budgets_without_ticks() {
        let mut run = FlightRun::new(false);
        let player = PlayerInput::preset("sky-courier-menu-test", Preset::Flight);
        let mut ui = Ui::new();
        // Use real lifecycle/autoplay input for terminal menus, with supplied
        // fixed time. UI construction itself must not advance any state.
        let paint = |run: &FlightRun, ui: &mut Ui| {
            for screen in [[320., 600.], [390., 600.], [1280., 560.]] {
                let layout = Layout::new(screen).unwrap();
                ui.begin(layout.viewport);
                queue(run, ui, &player, &layout).unwrap();
            }
        };
        paint(&run, &mut ui);
        assert_eq!(run.session.tick(), 0);
        assert_eq!(run.flight.remaining, 90.);
        run.session.start();
        paint(&run, &mut ui);
        run.session.set_paused(true);
        paint(&run, &mut ui);
        assert_eq!(run.session.tick(), 0);
        run.session.set_paused(false);
        for _ in 0..12000 {
            if !run.session.is_playing() {
                break;
            }
            run.advance(1. / 120., Input::default(), true);
        }
        assert_eq!(run.session.state(), SessionState::Won);
        let tick = run.session.tick();
        let remaining = run.flight.remaining;
        paint(&run, &mut ui);
        assert_eq!(run.session.tick(), tick);
        assert_eq!(run.flight.remaining, remaining);
        run.restart();
        run.advance(0., Input::default(), false);
        for _ in 0..12000 {
            if !run.session.is_playing() {
                break;
            }
            run.advance(
                1. / 120.,
                Input {
                    climb: -1.,
                    boost: true,
                    ..Input::default()
                },
                false,
            );
        }
        assert_eq!(run.session.state(), SessionState::Lost);
        assert_eq!(run.flight.hull, 0);
        paint(&run, &mut ui);
    }
}
