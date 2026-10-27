//! Pixel-sized flight HUD; inactive menus replace conflicting navigation rows.
//!
//! Hierarchy while flying: clock/gates/hull top-left, tips and streak top-right,
//! one navigation capsule, one transient callout, one fading coach hint and
//! the telemetry footer. Menus are compact cards: the title, detail, summary and
//! button rows keep their established geometry, and a short extra block below
//! the button carries the customer reaction, personal best and next goal.
use super::presentation::{GOLD, MINT};
use super::*;
use ferravex_render::ui::text_layout::LineLimit;

const PANEL: Color = Color::new(0.08, 0.17, 0.19, 0.96);
const MIST: Color = Color::new(0.72, 0.84, 0.84, 1.0);
const DIM: Color = Color::new(0.24, 0.33, 0.34, 0.96);
const BODY_SIZE: f32 = 14.;
const EXTRA_ROWS: usize = 4;
const EXTRA_ROW: f32 = 18.;
/// The browser host pins a 44px game-menu button at the fullscreen canvas's
/// top-right corner (inset 8px). Keep HUD copy out of this screen region.
const MENU_CORNER: [f32; 2] = [72., 60.];

struct Layout {
    viewport: Viewport,
    x: f32,
    width: f32,
    height: f32,
    narrow: bool,
    /// Largest layout-relative x a top-row element may reach while staying
    /// clear of the reserved menu corner.
    top_right: f32,
}

/// Top-row badges: clock/gate/hull on the left, tips and streak on the right.
struct TopBar {
    left: UiRect,
    left_strip: UiRect,
    pips: f32,
    tips: UiRect,
    tips_text: UiRect,
    chip: UiRect,
    tips_strip: UiRect,
}

impl Layout {
    fn new(screen: [f32; 2]) -> Result<Self, ferravex_play::ui::UiError> {
        let viewport = Viewport::responsive(screen, [8.; 4])?;
        let [available, height] = viewport.bounds().size;
        let width = available.min(960.);
        let x = (available - width) * 0.5;
        let origin = viewport.pixels().min[0];
        Ok(Self {
            viewport,
            x,
            width,
            height,
            narrow: width < 600.,
            top_right: width.min(screen[0] - MENU_CORNER[0] - origin - x),
        })
    }

    fn top_bar(&self) -> TopBar {
        // Compact badges let both clusters fit left of the menu corner at 320px.
        let (left, pips, tips, chip) = if self.narrow {
            (124., 78., 110., 34.)
        } else {
            (280., 230., 170., 40.)
        };
        let right = self.top_right;
        let tips_x = right - tips;
        TopBar {
            left: self.rect(0., 0., left, 30.),
            left_strip: self.rect(0., 36., left, 4.),
            pips,
            tips: self.rect(tips_x, 0., tips, 30.),
            tips_text: self.rect(tips_x + 10., 6., tips - chip - 14., 20.),
            chip: self.rect(right - chip - 4., 5., chip, 20.),
            tips_strip: self.rect(tips_x, 36., tips, 4.),
        }
    }

    fn rect(&self, x: f32, y: f32, w: f32, h: f32) -> UiRect {
        ui_rect(self.x + x, y, w.max(0.), h.max(0.))
    }

    fn row(&self, y: f32, height: f32) -> UiRect {
        self.rect(12., y, self.width - 24., height)
    }

    /// A horizontally centred capsule, never wider than the HUD.
    fn centered(&self, y: f32, width: f32, height: f32) -> UiRect {
        let width = width.min(self.width);
        self.rect((self.width - width) * 0.5, y, width, height)
    }

    fn menu(&self) -> MenuLayout {
        let width = self.width.min(420.);
        let left = (self.width - width) * 0.5;
        let title = 28.;
        let detail = 40.;
        let summary = 36.;
        let extra = EXTRA_ROWS as f32 * EXTRA_ROW;
        let height = 16. + title + 8. + detail + 8. + summary + 12. + 48. + 12. + extra + 8.;
        // Sit a little high so the aircraft stays visible beneath the card.
        // Keep top alignment in smaller native windows instead of shrinking text.
        let y = ((self.height - height) * 0.38).round().max(8.);
        let mut next = y + 16.;
        let mut row = |height, gap| {
            let rect = self.rect(left + 12., next, width - 24., height);
            next += height + gap;
            rect
        };
        let title = row(title, 8.);
        let detail = row(detail, 8.);
        let summary = row(summary, 12.);
        let button_y = next;
        let extra_y = button_y + 48. + 12.;
        let button_width = (width - 24.).min(240.);
        MenuLayout {
            panel: self.rect(left, y, width, height),
            title,
            detail,
            summary,
            button: self.rect(
                left + (width - button_width) * 0.5,
                button_y,
                button_width,
                48.,
            ),
            extra: std::array::from_fn(|i| {
                self.rect(
                    left + 12.,
                    extra_y + i as f32 * EXTRA_ROW,
                    width - 24.,
                    EXTRA_ROW,
                )
            }),
        }
    }

    fn footer(&self) -> UiRect {
        let height = 58.;
        self.rect(0., (self.height - height).max(0.), self.width, height)
    }
}

struct MenuLayout {
    panel: UiRect,
    title: UiRect,
    detail: UiRect,
    summary: UiRect,
    button: UiRect,
    extra: [UiRect; EXTRA_ROWS],
}

impl MenuLayout {
    /// Second line inside the 40px detail row.
    fn detail_second(&self) -> UiRect {
        ui_rect(
            self.detail.min[0],
            self.detail.min[1] + 21.,
            self.detail.size[0],
            18.,
        )
    }
}

/// Stored progress the cards can brag about; storage is optional.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Progress {
    /// Best delivered tips, if any delivery was ever recorded.
    pub best_tips: Option<u32>,
}

/// Snapshot taken when a shift ends, before this result was recorded.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Results {
    pub previous_best: Option<u32>,
    pub shift: u32,
    /// Autoplay results are shown but never recorded.
    pub demo: bool,
}

/// Onboarding hint shown above the footer until the player has done it once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Hint {
    FirstRing,
    Boost,
    Centre,
}

/// Learned-once coaching. Ages on accepted ticks only, so pause freezes it.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Coach {
    passed_ring: bool,
    boosted: bool,
    bullseye: bool,
    current: Option<Hint>,
    alpha: f32,
}

impl Coach {
    pub(super) fn update(&mut self, flight: &Flight, seconds: f32) {
        self.passed_ring |= flight.next > 0;
        self.boosted |= flight.boosting;
        self.bullseye |= flight.ledger.bullseyes > 0;
        let wanted = if flight.outcome != Outcome::Flying || flight.recovery > 0. {
            None
        } else if !self.passed_ring {
            Some(Hint::FirstRing)
        } else if !self.boosted && flight.next < GATES {
            Some(Hint::Boost)
        } else if !self.bullseye && (2..GATES).contains(&flight.next) {
            Some(Hint::Centre)
        } else {
            None
        };
        if wanted == self.current && wanted.is_some() {
            self.alpha = (self.alpha + seconds * 4.).min(1.);
        } else {
            // Fade the old hint out before showing the next one.
            self.alpha = (self.alpha - seconds * 3.).max(0.);
            if self.alpha == 0. {
                self.current = wanted;
            }
        }
    }

    pub(super) fn visible(&self) -> Option<(Hint, f32)> {
        self.current
            .filter(|_| self.alpha > 0.)
            .map(|h| (h, self.alpha))
    }
}

/// Host-owned context the HUD reads; it never mutates game state.
pub(super) struct HudView<'a> {
    pub progress: Progress,
    pub results: &'a Results,
    pub coach: &'a Coach,
    pub pauses: u32,
}

fn faded(color: Color, alpha: f32) -> Color {
    Color::new(color.r, color.g, color.b, color.a * alpha.clamp(0., 1.))
}

fn fits(text: &str, width: f32, size: f32) -> bool {
    text.chars().count() as f32 * size * 0.75 <= width
}

fn menu_copy(state: SessionState, hull: u8) -> (&'static str, &'static str, &'static str) {
    match state {
        SessionState::Ready => ("SKY COURIER", "5 GATES / 1 DELIVERY", "START FLIGHT"),
        SessionState::Paused => ("FLIGHT PAUSED", "", "RESUME FLIGHT"),
        SessionState::Won => ("MAIL DELIVERED", "", "FLY AGAIN"),
        SessionState::Lost if hull == 0 => ("AIRCRAFT GROUNDED", "3 IMPACTS", "RETRY FLIGHT"),
        SessionState::Lost => ("TIME UP", "", "RETRY FLIGHT"),
        SessionState::Playing => unreachable!(),
    }
}

// Hints are complete current bindings or an explicit route to the editor. Never
// clip a key name, substitute a default, or make the font smaller to fit a remap.
#[cfg(test)]
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

/// Compact arrows keep navigation on one line even at 320 pixels.
fn nav_copy(nav: &Navigation, narrow: bool) -> (&'static str, &'static str) {
    let turn = match (nav.turn(), narrow) {
        ("TURN LEFT", false) => "← TURN LEFT",
        ("TURN LEFT", true) => "← LEFT",
        ("TURN RIGHT", false) => "TURN RIGHT →",
        ("TURN RIGHT", true) => "RIGHT →",
        (_, false) => "ON COURSE",
        (_, true) => "AHEAD",
    };
    let climb = match (nav.climb(), narrow) {
        ("CLIMB", _) => "↑ CLIMB",
        ("DESCEND", false) => "↓ DESCEND",
        ("DESCEND", true) => "↓ DIVE",
        (_, false) => "HOLD ALTITUDE",
        (_, true) => "LEVEL",
    };
    (turn, climb)
}

fn queue_flight(
    run: &FlightRun,
    ui: &mut Ui,
    player: &PlayerInput,
    layout: &Layout,
    view: &HudView,
) -> Result<(), ferravex_play::ui::UiError> {
    let game = &run.flight;
    let ledger = &game.ledger;
    let width = layout.width;
    let seconds = game.countdown();
    let narrow = layout.narrow;

    // Clock, target and hull, with the six-part route strip beneath.
    let top = layout.top_bar();
    ui.panel(top.left, PANEL)?;
    ui.text_fmt(
        layout.rect(if narrow { 10. } else { 12. }, 6., 64., 20.),
        ui_text(16., if seconds <= 15 { AMBER } else { CREAM }, Align::Left),
        format_args!("{:02}:{:02}", seconds / 60, seconds % 60),
    )?;
    if !narrow {
        if game.next == GATES {
            ui.text(
                layout.rect(84., 8., 84., 18.),
                "DROP",
                ui_text(14., CREAM, Align::Left),
            )?;
        } else {
            ui.text_fmt(
                layout.rect(84., 8., 84., 18.),
                ui_text(14., CREAM, Align::Left),
                format_args!("GATE {}/{}", game.next + 1, GATES),
            )?;
        }
        ui.text(
            layout.rect(180., 8., 48., 18.),
            "HULL",
            ui_text(14., MIST, Align::Left),
        )?;
    }
    for i in 0..3 {
        ui.panel(
            layout.rect(top.pips + i as f32 * 14., 9., 10., 12.),
            if i < game.hull { CORAL } else { DIM },
        )?;
    }
    for (i, slot) in ferravex_play::ui::slots(top.left_strip, ROUTE.len(), 4., false)?.enumerate() {
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
    }

    // Tips and the streak multiplier, mirrored on the right but kept clear of
    // the browser's fullscreen menu corner.
    let multiplier = ledger.multiplier();
    ui.panel(top.tips, PANEL)?;
    ui.text_fmt(
        top.tips_text,
        ui_text(16., GOLD, Align::Left),
        format_args!("${}", ledger.tips),
    )?;
    let chip_text = ui_rect(top.chip.min[0], top.chip.min[1] + 3., top.chip.size[0], 16.);
    if multiplier > 1 {
        ui.panel(top.chip, GOLD)?;
        ui.text_fmt(
            chip_text,
            ui_text(14., INK, Align::Center),
            format_args!("×{multiplier}"),
        )?;
    } else {
        ui.text(chip_text, "×1", ui_text(14., MIST, Align::Center))?;
    }
    for (i, slot) in
        ferravex_play::ui::slots(top.tips_strip, courier::MAX_MULTIPLIER as usize, 4., false)?
            .enumerate()
    {
        ui.panel(slot, if (i as u32) < multiplier { GOLD } else { PANEL })?;
    }

    let mut slow_buffer = [0; 96];
    let slow = control_hint(player, "brake", &mut slow_buffer);
    if let Some(nav) = game.navigation() {
        let delivery = game.next == GATES;
        let behind = nav.bearing.abs() > std::f32::consts::FRAC_PI_2;
        let height = if delivery || behind { 84. } else { 34. };
        ui.panel(layout.centered(52., 440., height), PANEL)?;
        let (turn, climb) = nav_copy(&nav, narrow);
        ui.text_fmt(
            layout.row(58., 22.),
            ui_text(14., AMBER, Align::Center),
            format_args!(
                "{}  {}  {:.0}M{}",
                turn,
                climb,
                nav.distance,
                if delivery && !narrow { " TO POST" } else { "" }
            ),
        )?;
        if behind {
            ui.text(
                layout.row(86., 42.),
                "TURN BACK",
                ui_text(BODY_SIZE, CREAM, Align::Center),
            )?;
        } else if delivery && game.motor.speed > DELIVERY_SPEED {
            ui.text_fmt(
                layout.row(86., 42.),
                ui_text(BODY_SIZE, CREAM, Align::Center),
                format_args!("SLOW DOWN: {}\nAt most 18 m/s (35 kt)", slow),
            )?;
        } else if delivery {
            ui.text(
                layout.row(86., 42.),
                "SPEED READY / 16m",
                ui_text(BODY_SIZE, CREAM, Align::Center),
            )?;
        }
    }

    // One callout at a time: recovery outranks hidden rings, then gates.
    let callout = if game.recovery > 0. && game.outcome == Outcome::Flying {
        let where_ = if game.next == 0 {
            "START".to_owned()
        } else {
            format!("GATE {}", game.next)
        };
        Some((
            quips::impact(ledger.last_impact, ledger.impacts).to_owned(),
            CORAL,
            format!("-5S · BACK AT {where_} · SHIELD {:.1}S", game.recovery),
            1.,
        ))
    } else if game.bonus_notice > 0. {
        let found = ledger.bonus_count();
        let (head, sub) = if ledger.scenic() {
            (
                "SCENIC ROUTE COMPLETE!".to_owned(),
                format!(
                    "+${} · ALL 3 HIDDEN RINGS",
                    courier::BONUS_TIP + courier::SCENIC_TIP
                ),
            )
        } else {
            (
                format!("{} {found}/3", quips::bonus(found)),
                format!("+${} · BOOST TOPPED UP", courier::BONUS_TIP),
            )
        };
        Some((head, MINT, sub, (game.bonus_notice / 0.3).min(1.)))
    } else if game.gate_notice > 0. && game.next > 0 && game.next <= GATES {
        let gate = game.next - 1;
        ledger.gates[gate].map(|pass| {
            let mut sub = format!("+${} TIP", pass.tip);
            if pass.hot {
                sub.push_str(" · HOT");
            }
            if let Some(milestone) = quips::streak(pass.multiplier) {
                sub = format!("{sub} · {milestone}");
            } else if pass.multiplier > 1 {
                sub = format!("{sub} · ×{} STREAK", pass.multiplier);
            }
            (
                quips::gate(pass.line, gate).to_owned(),
                if pass.line == courier::Line::Scrape {
                    CREAM
                } else {
                    GOLD
                },
                sub,
                (game.gate_notice / 0.3).min(1.),
            )
        })
    } else {
        None
    };
    if let Some((head, color, sub, alpha)) = callout {
        let rect = layout.centered(146., 420., 46.);
        ui.panel(rect, faded(PANEL, alpha))?;
        let inner = |y: f32, h: f32| ui_rect(rect.min[0] + 10., y, rect.size[0] - 20., h);
        ui.wrapped_text(
            inner(rect.min[1] + 5., 18.),
            &head,
            ui_text(16., faded(color, alpha), Align::Center),
            LineLimit::Ellipsis(1),
        )?;
        ui.wrapped_text(
            inner(rect.min[1] + 26., 16.),
            &sub,
            ui_text(BODY_SIZE, faded(MIST, alpha), Align::Center),
            LineLimit::Ellipsis(1),
        )?;
    }

    let footer = layout.footer();
    let bottom = footer.min[1];
    if let Some((hint, alpha)) = view.coach.visible() {
        let mut buffer = [0; 96];
        let copy = match hint {
            Hint::FirstRing => "STEER THROUGH THE AMBER RING".to_owned(),
            Hint::Boost => {
                let key = control_hint(player, "boost", &mut buffer);
                if key == "Open Controls" || key == "Unbound" {
                    "RINGS REFILL YOUR BOOST".to_owned()
                } else {
                    format!("HOLD {key} TO BOOST")
                }
            }
            Hint::Centre => "THREAD THE CENTRE: BIGGER TIPS".to_owned(),
        };
        let rect = layout.centered(bottom - 36., 380., 28.);
        ui.panel(rect, faded(PANEL, alpha * 0.85))?;
        ui.wrapped_text(
            ui_rect(rect.min[0] + 10., rect.min[1] + 7., rect.size[0] - 20., 16.),
            &copy,
            ui_text(BODY_SIZE, faded(MIST, alpha), Align::Center),
            LineLimit::Ellipsis(1),
        )?;
    }

    ui.panel(footer, PANEL)?;
    let stat_width = if narrow {
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
            if narrow { 12. } else { width * 0.5 },
            bottom + if narrow { 26. } else { 8. },
            stat_width,
            18.,
        ),
        ui_text(
            BODY_SIZE,
            AMBER,
            if narrow { Align::Left } else { Align::Right },
        ),
        format_args!(
            "{} / BOOST {}%",
            game.flight_mode(),
            (game.boost_energy * 100.) as u32
        ),
    )?;
    let bar = bottom + if narrow { 46. } else { 30. };
    ui.panel(layout.rect(12., bar, width - 24., 3.), INK)?;
    ui.panel(
        layout.rect(12., bar, (width - 24.) * game.boost_energy, 3.),
        if game.boost_exhausted { CORAL } else { AMBER },
    )?;
    Ok(())
}

fn queue_menu(
    run: &FlightRun,
    ui: &mut Ui,
    player: &PlayerInput,
    layout: &Layout,
    view: &HudView,
) -> Result<(), ferravex_play::ui::UiError> {
    let game = &run.flight;
    let ledger = &game.ledger;
    let state = run.session.state();
    let menu = layout.menu();
    let (title, detail, button) = menu_copy(state, game.hull);
    let text_width = menu.detail.size[0];
    ui.panel(menu.panel, PANEL)?;
    ui.text(
        menu.title,
        title,
        ui_text(if layout.narrow { 20. } else { 24. }, CREAM, Align::Center),
    )?;
    // The cream detail line is fixed copy; everything else uses other inks.
    if !detail.is_empty() {
        ui.text(menu.detail, detail, ui_text(16., CREAM, Align::Center))?;
    }
    let first = ui_rect(
        menu.detail.min[0],
        menu.detail.min[1],
        menu.detail.size[0],
        18.,
    );
    let finished = matches!(state, SessionState::Won | SessionState::Lost);
    let delivered = state == SessionState::Won;
    let mut extra: [(String, Color, usize); EXTRA_ROWS] = Default::default();
    if finished {
        let (rank, next) = quips::rank(ledger, delivered, game.hull, game.next.min(GATES));
        let mut lines = [rank.to_owned(), String::new()];
        lines[1] = if let Some(drop) = ledger.drop {
            format!("{} · SHIFT #{}", drop.grade.name(), view.results.shift)
        } else if game.hull == 0 {
            String::new()
        } else {
            format!("THE CLOCK WON · SHIFT #{}", view.results.shift)
        };
        if detail.is_empty() {
            ui.text(first, &lines[0], ui_text(16., GOLD, Align::Center))?;
            ui.wrapped_text(
                menu.detail_second(),
                &lines[1],
                ui_text(BODY_SIZE, MIST, Align::Center),
                LineLimit::Ellipsis(1),
            )?;
        } else {
            ui.wrapped_text(
                menu.detail_second(),
                &lines[0],
                ui_text(BODY_SIZE, GOLD, Align::Center),
                LineLimit::Ellipsis(1),
            )?;
        }
        if delivered {
            ui.text_fmt(
                menu.summary,
                ui_text(BODY_SIZE, AMBER, Align::Center),
                format_args!(
                    "TIPS ${} · BEST ×{}\n{:.1}S · {} HITS · RINGS {}/3",
                    ledger.tips,
                    ledger.best_streak.min(courier::MAX_MULTIPLIER),
                    run.elapsed,
                    3 - game.hull,
                    ledger.bonus_count()
                ),
            )?;
        } else {
            ui.text_fmt(
                menu.summary,
                ui_text(BODY_SIZE, AMBER, Align::Center),
                format_args!(
                    "UNPAID TIPS ${}\nGATES {}/{} · -{}S PENALTY",
                    ledger.tips,
                    game.next.min(GATES),
                    GATES,
                    (3 - game.hull) * 5
                ),
            )?;
        }
        let customer = quips::customer(ledger, delivered, game.hull, view.results.shift);
        extra[0] = (customer.to_owned(), MIST, 2);
        let previous = view.results.previous_best;
        extra[2] = if view.results.demo {
            ("DEMO FLIGHT · NOT RECORDED".to_owned(), MIST, 1)
        } else if !delivered {
            match previous {
                Some(best) => (format!("BEST ${best} STILL STANDS"), GOLD, 1),
                None => ("DELIVER TO SET A BEST".to_owned(), GOLD, 1),
            }
        } else {
            match previous {
                None => ("FIRST DELIVERY ON RECORD!".to_owned(), MINT, 1),
                Some(best) if ledger.tips > best => (format!("NEW BEST! OLD ${best}"), MINT, 1),
                Some(best) if ledger.tips == best => (format!("TIED YOUR BEST ${best}"), GOLD, 1),
                Some(best) => (
                    format!("BEST ${best} · ${} TO BEAT", best - ledger.tips),
                    GOLD,
                    1,
                ),
            }
        };
        extra[3] = match next {
            _ if delivered && ledger.bonus_count() < 3 && ledger.bonus_count() > 0 => (
                format!("HIDDEN RINGS {}/3 · KEEP LOOKING", ledger.bonus_count()),
                MIST,
                1,
            ),
            Some((tips, title)) if delivered => {
                let long = format!("NEXT: {title} AT ${tips}");
                if fits(&long, text_width, BODY_SIZE) {
                    (long, MIST, 1)
                } else {
                    (format!("NEXT TITLE AT ${tips}"), MIST, 1)
                }
            }
            Some((_, goal)) => (goal.to_owned(), MIST, 1),
            None => ("TOP RANK. THE GULLS BOW.".to_owned(), MINT, 1),
        };
    } else {
        // Ready and pause keep the established rules summary.
        ui.text(
            menu.summary,
            "90 seconds / 3 hull",
            ui_text(BODY_SIZE, AMBER, Align::Center),
        )?;
        if state == SessionState::Ready {
            ui.text(
                menu.detail_second(),
                "THREAD RINGS FOR TIPS",
                ui_text(BODY_SIZE, GOLD, Align::Center),
            )?;
            extra[0] = ("CLEAN STREAKS MULTIPLY ×5".to_owned(), MIST, 1);
            extra[1] = ("SLOW DOWN FOR THE DROP".to_owned(), MIST, 1);
            extra[2] = match view.progress.best_tips {
                Some(best) => (format!("YOUR BEST: ${best}"), GOLD, 1),
                None => ("FIRST SHIFT. NO PRESSURE.".to_owned(), GOLD, 1),
            };
            extra[3] = ("3 GOLDEN RINGS HIDE NEARBY".to_owned(), MINT, 1);
        } else {
            ui.text(
                first,
                quips::pause(view.pauses),
                ui_text(BODY_SIZE, GOLD, Align::Center),
            )?;
            let seconds = game.countdown();
            extra[0] = (
                format!("TIPS ${} · ×{}", ledger.tips, ledger.multiplier()),
                GOLD,
                1,
            );
            extra[1] = (
                if game.next >= GATES {
                    format!("{:02}:{:02} LEFT · DROP NEXT", seconds / 60, seconds % 60)
                } else {
                    format!(
                        "{:02}:{:02} LEFT · GATE {}/{}",
                        seconds / 60,
                        seconds % 60,
                        game.next + 1,
                        GATES
                    )
                },
                MIST,
                1,
            );
            extra[2] = (format!("HIDDEN RINGS {}/3", ledger.bonus_count()), MINT, 1);
            let mut buffer = [0; 96];
            let restart = control_hint(player, "restart", &mut buffer);
            extra[3] = if fits(&format!("{restart} RESTARTS"), text_width, BODY_SIZE)
                && restart != "Open Controls"
                && restart != "Unbound"
            {
                (format!("{restart} RESTARTS"), MIST, 1)
            } else {
                ("RESTART: SEE CONTROLS".to_owned(), MIST, 1)
            };
        }
    }
    // Each entry starts on its own row; a two-line entry owns the next row.
    for (row, (text, color, lines)) in extra.iter().enumerate() {
        if text.is_empty() {
            continue;
        }
        let lines = (*lines).clamp(1, EXTRA_ROWS - row);
        let first = menu.extra[row];
        ui.wrapped_text(
            ui_rect(
                first.min[0],
                first.min[1],
                first.size[0],
                EXTRA_ROW * lines as f32,
            ),
            text,
            ui_text(BODY_SIZE, *color, Align::Center),
            LineLimit::Ellipsis(lines),
        )?;
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
    Ok(())
}

fn queue(
    run: &FlightRun,
    ui: &mut Ui,
    player: &PlayerInput,
    layout: &Layout,
    view: &HudView,
) -> Result<(), ferravex_play::ui::UiError> {
    if run.session.state() == SessionState::Playing {
        queue_flight(run, ui, player, layout, view)
    } else {
        // No navigation, recovery or telemetry beneath an inactive menu.
        queue_menu(run, ui, player, layout, view)
    }
}

pub(super) fn hud(
    run: &FlightRun,
    ui: &mut Ui,
    player: &mut PlayerInput,
    accept_input: bool,
    view: &HudView,
) -> Result<bool, ferravex_play::ui::UiError> {
    let layout = Layout::new([screen_width(), screen_height()])?;
    ui.begin(layout.viewport);
    queue(run, ui, player, &layout, view)?;
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
    fn fullscreen_telemetry_reserves_most_of_the_viewport_for_flight() {
        for screen in [[320., 480.], [390., 844.], [844., 390.], [1280., 800.]] {
            let layout = Layout::new(screen).unwrap();
            let footer = layout.footer();
            assert_eq!(
                layout.viewport.scale(),
                1.,
                "HUD text stays in logical pixels"
            );
            assert_eq!(footer.size[1], 58.);
            assert!(footer.min[1] >= screen[1] * 0.75);
            let menu = layout.menu();
            // Cards replace flight entirely, so they may use more of a short
            // landscape screen, but must stay fully contained.
            assert!(menu.panel.size[1] <= 300.);
            assert_eq!(
                menu.panel.intersect(layout.viewport.bounds()),
                Some(menu.panel)
            );
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
            let mut rows = vec![menu.title, menu.detail, menu.summary, menu.button];
            rows.extend(menu.extra);
            for rect in &rows {
                assert_eq!(rect.intersect(menu.panel), Some(*rect));
            }
            for pair in rows.windows(2) {
                assert!(pair[0].min[1] + pair[0].size[1] <= pair[1].min[1]);
            }
            let second = menu.detail_second();
            assert_eq!(second.intersect(menu.detail), Some(second));
            assert!(menu.panel.size[1] <= 300., "menus stay compact");
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

    const AUDIT_SIZES: [[f32; 2]; 5] = [
        [1280., 800.],
        [1600., 1000.],
        [390., 844.],
        [844., 390.],
        [320., 480.],
    ];

    /// Logical layout rectangle to CSS-pixel `[left, top, right, bottom]`.
    fn pixels(layout: &Layout, rect: UiRect) -> [f32; 4] {
        let origin = layout.viewport.pixels().min;
        let left = origin[0] + rect.min[0];
        let top = origin[1] + rect.min[1];
        [left, top, left + rect.size[0], top + rect.size[1]]
    }

    fn clear_of_menu_corner(screen: [f32; 2], edges: [f32; 4]) -> bool {
        edges[2] <= screen[0] - MENU_CORNER[0] || edges[1] >= MENU_CORNER[1]
    }

    #[test]
    fn hud_and_cards_keep_the_fullscreen_menu_corner_clear() {
        for screen in AUDIT_SIZES {
            let layout = Layout::new(screen).unwrap();
            let top = layout.top_bar();
            let tips_text = pixels(&layout, top.tips_text);
            // The widest realistic tip total still fits before the chip.
            assert!(fits("$1999", top.tips_text.size[0], 16.), "{screen:?}");
            for rect in [
                top.left,
                top.left_strip,
                top.tips,
                top.tips_text,
                top.chip,
                top.tips_strip,
                layout.rect(top.pips + 28., 9., 10., 12.),
                layout.centered(52., 440., 84.),
                layout.centered(146., 420., 46.),
                layout.footer(),
            ] {
                let edges = pixels(&layout, rect);
                assert!(
                    clear_of_menu_corner(screen, edges),
                    "{screen:?}: {edges:?} enters the menu corner"
                );
                assert!(
                    edges[0] >= 0. && edges[2] <= screen[0],
                    "{screen:?}: {edges:?}"
                );
            }
            // Left and right clusters never overlap each other.
            assert!(pixels(&layout, top.left)[2] + 4. <= pixels(&layout, top.tips)[0]);
            assert!(tips_text[2] <= pixels(&layout, top.chip)[0]);
            let menu = layout.menu();
            let mut rows = vec![
                menu.panel,
                menu.title,
                menu.detail,
                menu.summary,
                menu.button,
            ];
            rows.extend(menu.extra);
            for rect in rows {
                let edges = pixels(&layout, rect);
                assert!(
                    clear_of_menu_corner(screen, edges),
                    "{screen:?}: card {edges:?} enters the menu corner"
                );
            }
        }
    }

    #[test]
    fn navigation_copy_fits_one_line_at_the_narrowest_supported_width() {
        let layout = Layout::new([320., 480.]).unwrap();
        assert!(layout.narrow);
        let nav = Navigation {
            bearing: -1.,
            altitude: -20.,
            distance: 999.,
        };
        let (turn, climb) = nav_copy(&nav, true);
        let line = format!("{turn}  {climb}  999M");
        assert!(fits(&line, layout.width - 24., 14.), "{line}");
        let wide = Layout::new([1280., 800.]).unwrap();
        let (turn, climb) = nav_copy(&nav, false);
        assert!(fits(
            &format!("{turn}  {climb}  999M TO POST"),
            wide.width.min(440.) - 24.,
            14.
        ));
    }

    #[test]
    fn coach_hints_fade_on_accepted_time_and_stay_learned_after_retry() {
        let mut coach = Coach::default();
        let mut flight = Flight::new();
        coach.update(&flight, 0.);
        assert_eq!(coach.visible(), None, "nothing appears without a tick");
        for _ in 0..30 {
            coach.update(&flight, 1. / 60.);
        }
        assert_eq!(coach.visible().map(|v| v.0), Some(Hint::FirstRing));
        let frozen = coach.visible();
        coach.update(&flight, 0.);
        assert_eq!(coach.visible(), frozen, "pause freezes the fade");
        flight.next = 1;
        for _ in 0..60 {
            coach.update(&flight, 1. / 60.);
        }
        assert_eq!(coach.visible().map(|v| v.0), Some(Hint::Boost));
        flight.boosting = true;
        flight.ledger.bullseyes = 1;
        for _ in 0..60 {
            coach.update(&flight, 1. / 60.);
        }
        assert_eq!(coach.visible(), None);
        // Retrying keeps learned skills; the first-ring hint does not return.
        let retry = Flight::new();
        for _ in 0..60 {
            coach.update(&retry, 1. / 60.);
        }
        assert_eq!(coach.visible(), None);
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
        let coach = Coach::default();
        // Use real lifecycle/autoplay input for terminal menus, with supplied
        // fixed time. UI construction itself must not advance any state.
        let paint = |run: &FlightRun, ui: &mut Ui| {
            for results in [
                Results::default(),
                Results {
                    previous_best: Some(9999),
                    shift: 120,
                    demo: false,
                },
                Results {
                    previous_best: Some(1),
                    shift: 2,
                    demo: true,
                },
            ] {
                let view = HudView {
                    progress: Progress {
                        best_tips: results.previous_best,
                    },
                    results: &results,
                    coach: &coach,
                    pauses: results.shift,
                };
                for screen in [[320., 600.], [390., 600.], [1280., 560.], [844., 390.]] {
                    let layout = Layout::new(screen).unwrap();
                    ui.begin(layout.viewport);
                    queue(run, ui, &player, &layout, &view).unwrap();
                }
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
            if run.session.tick().is_multiple_of(90) {
                paint(&run, &mut ui);
            }
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
            paint(&run, &mut ui);
        }
        assert_eq!(run.session.state(), SessionState::Lost);
        assert_eq!(run.flight.hull, 0);
        paint(&run, &mut ui);
    }
}
