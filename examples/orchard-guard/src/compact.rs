//! Narrow-window presentation. The board keeps its world coordinates while the
//! existing shared UI uses logical pixels, including its pointer/touch handling.
//! The Ui pass owns panels, hit-testing and focus; [`paint`] then draws the
//! same pictogram vocabulary as the desktop HUD over it.
use super::*;
use crate::glyph::{self, Glyph, Item};
use crate::style::sfmt;
use ferravex_render::icons::Icon;

const MARGIN: f32 = 8.;
const GAP: f32 = 6.;
const FONT: f32 = 14.;
pub(super) const PLOT_ID: u64 = 200;
/// Browser fullscreen pins a 44x44 menu button 8px from the top-right corner;
/// keep text and controls out of this reserved region (logical pixels).
pub(super) const MENU_CORNER: Vec2 = vec2(72., 60.);

#[derive(Clone, Copy)]
pub(super) struct Layout {
    screen: Vec2,
    pub board: Rect,
    pub plots: [Rect; 8],
    pub detail: Rect,
    pub speed: Rect,
    /// Two top status lines, inset clear of the fullscreen menu corner.
    pub status: [Rect; 2],
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
        // Speed sits at the right of the plot row, away from the menu corner.
        let speed = Rect::new(screen.x - MARGIN - 60., dock + 4., 60., 44.);
        let detail = Rect::new(108., dock + 4., (speed.x - GAP - 108.).max(1.), 38.);
        let line = (screen.x - 2. * MARGIN - MENU_CORNER.x).max(1.);
        let status = [
            Rect::new(MARGIN, 5., line, 20.),
            Rect::new(MARGIN, 25., line, 20.),
        ];
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
        let preview = Rect::new(MARGIN, 52., line, 18.);
        let notice = Rect::new(MARGIN, 76., width, 36.);
        Self {
            screen,
            board,
            plots,
            detail,
            speed,
            status,
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

    /// World-text enlargement so pops and labels stay legible when the board
    /// is drawn small.
    pub fn text_boost(self) -> f32 {
        (0.6 / self.scale()).clamp(1., 3.)
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

/// Glyph faces for the controls painted after the Ui pass: the Ui keeps
/// hit-testing, focus and fills; these rows replace word labels.
fn face(campaign: &Campaign) -> [(u64, [Option<Glyph>; 2], style::StackStr<96>); 6] {
    let g = &campaign.game;
    let upgrade = match g.towers[g.selected] {
        Some(tower) if tower.level == 2 => {
            (3, [Some(Glyph::Icon(Icon::Crown)), None], sfmt!("Max"))
        }
        Some(tower) => (
            3,
            [Some(Glyph::Icon(Icon::ArrowUp)), Some(Glyph::Coin)],
            sfmt!("{}", tower.upgrade_cost()),
        ),
        None => (0, [None; 2], sfmt!("")),
    };
    let wave = if g.active {
        (
            4,
            [Some(Glyph::Beetle), None],
            sfmt!("{}", hud::pests_left(g)),
        )
    } else if let Some(next) = g.next_wave() {
        (
            4,
            [Some(Glyph::Icon(Icon::Play)), None],
            sfmt!("Wave {}", next.number),
        )
    } else {
        (4, [Some(Glyph::Icon(Icon::Trophy)), None], sfmt!(""))
    };
    [
        (
            1,
            [Some(Glyph::Tower(Kind::Seed)), Some(Glyph::Coin)],
            sfmt!("{}", Kind::Seed.price()),
        ),
        (
            2,
            [Some(Glyph::Tower(Kind::Frost)), Some(Glyph::Coin)],
            sfmt!("{}", Kind::Frost.price()),
        ),
        upgrade,
        wave,
        (6, [Some(Glyph::Icon(Icon::Pause)), None], sfmt!("")),
        (5, [Some(Glyph::Icon(Icon::Restart)), None], sfmt!("")),
    ]
}

pub(super) fn hud(
    ui: &mut Ui,
    layout: Layout,
    campaign: &Campaign,
    _juice: &crate::juice::Juice,
    _save_error: bool,
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
        &format!("Plot {}", g.selected + 1),
        true,
        picker,
    );
    if g.towers[g.selected].is_some() {
        let status = g.upgrade_status(g.selected);
        control(ui, 3, upgrade_rect(layout), "", status.is_ok(), false);
    } else {
        for (i, kind) in [(0, Kind::Seed), (1, Kind::Frost)] {
            control(
                ui,
                i as u64 + 1,
                layout.build[i],
                "",
                g.build_status(g.selected, kind).is_ok(),
                false,
            );
        }
    }
    control(ui, 4, layout.wave, "", g.wave_status().is_ok(), false);
    control(
        ui,
        6,
        layout.menu[0],
        "",
        campaign.session.is_started(),
        false,
    );
    control(ui, 5, layout.menu[1], "", true, false);
    control(
        ui,
        crate::hud::SPEED_ID,
        layout.speed,
        "",
        true,
        campaign.fast,
    );
    if !g.active && g.next_wave().is_some() {
        // A backing strip keeps the preview legible where it meets the board.
        let p = layout.preview;
        ui.panel(
            area(Rect::new(p.x - 4., p.y - 1., p.w + 8., p.h + 2.)),
            Color::new(0.98, 0.95, 0.84, 0.92),
        )
        .unwrap();
    }
}

fn upgrade_rect(layout: Layout) -> Rect {
    Rect::new(
        layout.build[0].x,
        layout.build[0].y,
        layout.screen.x - 16.,
        44.,
    )
}

fn control_rect(layout: Layout, id: u64) -> Option<Rect> {
    match id {
        1 => Some(layout.build[0]),
        2 => Some(layout.build[1]),
        3 => Some(upgrade_rect(layout)),
        4 => Some(layout.wave),
        5 => Some(layout.menu[1]),
        6 => Some(layout.menu[0]),
        _ => None,
    }
}

/// Shrink `size` so a row of `items` fits `max` logical pixels.
fn fit(items: &[Item], size: f32, max: f32) -> f32 {
    let w = glyph::row_width(items, size);
    if w > max && w > 0. {
        (size * max / w).max(8.)
    } else {
        size
    }
}

/// Screen-space pictograms over the Ui pass: status, preview, tower detail,
/// control faces, banners and the pause/results stats.
pub(super) fn paint(
    layout: Layout,
    campaign: &Campaign,
    juice: &crate::juice::Juice,
    best: Option<crate::save::Best>,
    save_error: bool,
    input: &PlayerInput,
) {
    let g = &campaign.game;
    if campaign.session.is_paused() || g.outcome != Outcome::Playing {
        paint_overlay(layout, campaign, best);
        return;
    }
    // One status row: wave pips, hearts, coins and pests stopped.
    let status = Rect::new(
        layout.status[0].x,
        layout.status[0].y,
        layout.status[0].w,
        layout.status[1].y + layout.status[1].h - layout.status[0].y,
    );
    let hearts = sfmt!("{}", g.lives);
    let coins = sfmt!("{}", g.money);
    let stopped = sfmt!("{}", g.kills);
    let red = if juice.hurt > 0.3 {
        crate::juice::APPLE
    } else {
        INK
    };
    let items = [
        Item::Glyph(Glyph::Heart, crate::juice::APPLE),
        Item::Text(hearts.as_str(), red),
        Item::Gap(0.5),
        Item::Glyph(Glyph::Coin, INK),
        Item::Text(coins.as_str(), INK),
        Item::Gap(0.5),
        Item::Glyph(Glyph::Squashed, INK),
        Item::Text(stopped.as_str(), INK),
    ];
    let pip_r = 6.;
    let pips_w = pip_r * 3.2 * (g.level.waves.len().max(1) - 1) as f32 + pip_r * 2.;
    let size = fit(&items, 17., status.w - pips_w - 14.);
    let cy = status.y + status.h * 0.5;
    hud::wave_pips(g, vec2(status.x + pips_w * 0.5, cy), pip_r);
    glyph::row_at(&items, status.x + pips_w + 14., cy, size, 1.);
    if !g.active
        && let Some(next) = g.next_wave()
    {
        let preview = hud::Preview::new(&next);
        let mut items = [Item::Gap(0.); 14];
        let n = preview.items(&mut items);
        let p = layout.preview;
        let size = fit(&items[..n], 15., p.w);
        glyph::row_at(&items[..n], p.x, p.y + p.h * 0.5, size, 1.);
    }
    // The selected tower's name, level pips and core stats; an empty plot
    // points down at the tower choices.
    let d = layout.detail;
    if let Some(tower) = g.towers[g.selected] {
        let name = tower.kind.name();
        let style = style::body(FONT, INK);
        let w = style::text(name, d.x, style.centered_baseline(d.y + 9.), style);
        for n in 0..3u8 {
            let c = vec2(d.x + w + 10. + f32::from(n) * 10., d.y + 9.);
            ferravex_render::shapes::fill_circle(c, 4., Color::new(0.24, 0.31, 0.18, 0.25));
            if n <= tower.level {
                ferravex_render::shapes::fill_circle(c, 3., crate::juice::GOLD);
            }
        }
        let damage = sfmt!("{:.0}", tower.damage());
        let range = sfmt!("{:.0}", tower.range());
        let items = [
            Item::Glyph(Glyph::Icon(Icon::Sword), Color::new(0.74, 0.31, 0.22, 1.)),
            Item::Text(damage.as_str(), INK),
            Item::Gap(0.5),
            Item::Glyph(Glyph::Icon(Icon::Target), GREEN),
            Item::Text(range.as_str(), INK),
        ];
        glyph::row_at(&items, d.x, d.y + 28., fit(&items, FONT, d.w), 1.);
    } else {
        let bob = (style::clock() * 4.).sin() * 4. * juice.motion();
        ferravex_render::icons::draw(
            Icon::ArrowDown,
            vec2(d.x + d.w * 0.5, d.y + d.h * 0.5 + bob),
            26.,
            GREEN,
        );
    }
    // Control faces and refusal flashes.
    let refused = juice
        .refusal
        .filter(|(_, age)| *age < 0.4)
        .and_then(|(refusal, _)| hud::refusal_control(refusal));
    let tower = g.towers[g.selected].is_some();
    for (id, glyphs, label) in face(campaign) {
        let shown = match id {
            1 | 2 => !tower,
            3 => tower,
            0 => false,
            _ => true,
        };
        let Some(rect) = control_rect(layout, id).filter(|_| shown) else {
            continue;
        };
        let enabled = match id {
            1 => g.build_status(g.selected, Kind::Seed).is_ok(),
            2 => g.build_status(g.selected, Kind::Frost).is_ok(),
            3 => g.upgrade_status(g.selected).is_ok(),
            4 => g.wave_status().is_ok(),
            6 => campaign.session.is_started(),
            _ => true,
        };
        let ink = if enabled {
            CREAM
        } else {
            Color::new(0.88, 0.87, 0.78, 1.)
        };
        let mut items = [Item::Gap(0.); 3];
        let mut n = 0;
        for glyph in glyphs.into_iter().flatten() {
            items[n] = Item::Glyph(glyph, ink);
            n += 1;
        }
        if !label.as_str().is_empty() {
            items[n] = Item::Text(label.as_str(), ink);
            n += 1;
        }
        let size = fit(&items[..n], 18., rect.w - 12.);
        glyph::row(
            &items[..n],
            rect.center(),
            size,
            if enabled { 1. } else { 0.8 },
        );
        if refused == Some(id) {
            ferravex_render::shapes::fill_rounded_rect(rect, 6., Color::new(0.86, 0.27, 0.2, 0.4));
        }
    }
    let speed = layout.speed;
    let ink = if campaign.fast { INK } else { CREAM };
    glyph::row(
        &[
            Item::Glyph(Glyph::FastForward, ink),
            Item::Text(if campaign.fast { "2×" } else { "1×" }, ink),
        ],
        speed.center(),
        15.,
        1.,
    );
    // Toasts, refusal chips and storage trouble share the notice strip.
    let n = layout.notice;
    let center = vec2(n.x + n.w * 0.5, n.y + n.h * 0.5);
    if let Some((refusal, age)) = juice.refusal {
        let a = (age / 0.1)
            .min(1.)
            .min((crate::juice::REFUSAL_LIFE - age) / 0.3)
            .clamp(0., 1.);
        let number = hud::refusal_number(g, refusal);
        let (mut b0, mut b1, mut b2) = ([0; 48], [0; 48], [0; 48]);
        let keys = [
            hud::key(input, "select_one", &mut b0),
            hud::key(input, "select_two", &mut b1),
            hud::key(input, "upgrade", &mut b2),
        ];
        let mut items = [Item::Gap(0.); 6];
        let count = hud::refusal_items(refusal, number.as_str(), keys, &mut items);
        if count > 0 {
            let w = glyph::row_width(&items[..count], 16.) + 52.;
            let chip = Rect::new(center.x - w * 0.5, n.y + 3., w, 30.);
            style::dark_chip(chip, a);
            ferravex_render::icons::badge(
                Icon::Cross,
                vec2(chip.x + 17., chip.y + 15.),
                20.,
                style::alpha(crate::juice::APPLE, a),
                style::alpha(CREAM, a),
            );
            glyph::row_at(&items[..count], chip.x + 32., chip.y + 15., 16., a);
            return;
        }
    }
    if let Some(toast) = &juice.toast {
        let a = toast.alpha();
        let value = toast.value.map_or(sfmt!(""), |(_, v)| sfmt!("{v:+}"));
        let mut items = [Item::Gap(0.); 5];
        let count = hud::toast_items(toast, value.as_str(), &mut items);
        let size = fit(&items[..count], 18., n.w - 36.);
        let w = glyph::row_width(&items[..count], size) + 32.;
        let pill = Rect::new(center.x - w * 0.5, n.y + 1., w, 34.);
        style::dark_chip(pill, a);
        glyph::row(&items[..count], vec2(center.x, pill.y + 17.), size, a);
    } else if save_error {
        let items = [
            Item::Glyph(Glyph::Icon(Icon::Alert), crate::juice::GOLD),
            Item::Text("Save unavailable", CREAM),
        ];
        let w = glyph::row_width(&items, 15.) + 28.;
        let chip = Rect::new(center.x - w * 0.5, n.y + 3., w, 30.);
        style::dark_chip(chip, 1.);
        glyph::row(&items, vec2(center.x, chip.y + 15.), 15., 1.);
    }
}

/// The pause/results card rectangle; short landscape screens start it
/// below the fullscreen menu corner.
fn card(layout: Layout, paused: bool) -> Rect {
    let tall = if paused { 220. } else { 320. };
    Rect::new(
        8.,
        ((layout.screen.y - tall) * 0.5)
            .max(MENU_CORNER.y)
            .min((layout.screen.y - tall).max(0.)),
        layout.screen.x - 16.,
        tall,
    )
}

fn paint_overlay(layout: Layout, campaign: &Campaign, best: Option<crate::save::Best>) {
    let g = &campaign.game;
    let paused = campaign.session.is_paused();
    let card = card(layout, paused);
    let line = |y: f32, height: f32| Rect::new(card.x + 8., card.y + y, card.w - 16., height);
    // Campaign pips with hearts and pests stopped.
    let stats = line(46., 40.);
    let hearts = sfmt!("{}", g.lives);
    let stopped = sfmt!("{}", g.kills);
    let items = [
        Item::Glyph(Glyph::Heart, crate::juice::APPLE),
        Item::Text(hearts.as_str(), INK),
        Item::Gap(0.6),
        Item::Glyph(Glyph::Squashed, INK),
        Item::Text(stopped.as_str(), INK),
    ];
    let pip_r = 7.;
    let pips_w = pip_r * 3.2 * (g.level.waves.len().max(1) - 1) as f32 + pip_r * 2.;
    let size = fit(&items, 18., stats.w - pips_w - 20.);
    let total = pips_w + 20. + glyph::row_width(&items, size);
    let left = stats.x + (stats.w - total) * 0.5;
    let cy = stats.y + stats.h * 0.5;
    hud::wave_pips(g, vec2(left + pips_w * 0.5, cy), pip_r);
    glyph::row_at(&items, left + pips_w + 20., cy, size, 1.);
    if paused {
        return;
    }
    let best_score = best.and_then(|b| b.previous_score);
    let record = match best_score {
        Some(score) => sfmt!("{score}"),
        None => sfmt!("First run!"),
    };
    let lucky = sfmt!("{}", g.lucky_hits);
    let new_best = best.is_some_and(|b| b.new_score && b.previous_score.is_some());
    let items = [
        Item::Glyph(Glyph::Icon(Icon::Trophy), Color::new(0.86, 0.57, 0.13, 1.)),
        Item::Text(record.as_str(), GREEN),
        Item::Gap(0.6),
        Item::Glyph(Glyph::Lucky, Color::new(0.74, 0.31, 0.22, 1.)),
        Item::Text(lucky.as_str(), GREEN),
        Item::Gap(0.6),
        Item::Glyph(Glyph::Icon(Icon::Star), crate::juice::GOLD),
        Item::Text("New best!", crate::juice::APPLE),
    ];
    let n = if new_best { 8 } else { 5 };
    let row = line(134., 40.);
    let size = fit(&items[..n], 17., row.w);
    glyph::row(
        &items[..n],
        vec2(row.x + row.w * 0.5, row.y + row.h * 0.5),
        size,
        1.,
    );
    let run = crate::hud::run_summary(g);
    let goal = line(176., 60.);
    hud::goal_row(
        crate::quips::next_goal(run, best_score),
        hud::fastest(g, best),
        vec2(goal.x + goal.w * 0.5, goal.y + goal.h * 0.5),
        if layout.screen.x < 360. { 13. } else { 15. },
    );
}

pub(super) fn overlay(
    ui: &mut Ui,
    layout: Layout,
    campaign: &Campaign,
    _best: Option<crate::save::Best>,
    _input: &PlayerInput,
) {
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
    let card = card(layout, paused);
    ui.panel(area(card), CREAM).unwrap();
    let line = |y, height| Rect::new(card.x + 8., card.y + y, card.w - 16., height);
    text(
        ui,
        line(16., 24.),
        if paused {
            "Paused"
        } else if g.outcome == Outcome::Won {
            "Harvest saved!"
        } else {
            "Orchard overrun!"
        },
        INK,
    );
    if paused {
        control(ui, 7, line(100., 44.), "Resume", true, false);
        control(ui, 8, line(154., 44.), "Plant again", true, false);
        return;
    }
    let run = crate::hud::run_summary(g);
    let rank = crate::quips::rank(run);
    ui.panel(area(line(96., 30.)), GOLD).unwrap();
    text(ui, line(102., 20.), rank.title, INK);
    control(ui, 8, line(254., 44.), "Plant again", true, false);
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
            for label in ["Seed 60", "Frost 80"] {
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
                hud(
                    ui,
                    layout,
                    campaign,
                    &crate::juice::Juice::new(),
                    false,
                    input,
                    true,
                );
                overlay(ui, layout, campaign, None, input);
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

    #[test]
    fn fullscreen_menu_corner_stays_clear_of_compact_controls_and_text() {
        for screen in [
            vec2(390., 844.),
            vec2(844., 390.),
            vec2(320., 480.),
            vec2(320., 734.),
            vec2(899., 776.),
        ] {
            let layout = Layout::new(screen);
            let clear = |r: Rect| r.x + r.w <= screen.x - MENU_CORNER.x || r.y >= MENU_CORNER.y;
            for rect in layout
                .plots
                .into_iter()
                .chain(layout.build)
                .chain(layout.menu)
                .chain(layout.status)
                .chain([
                    layout.wave,
                    layout.speed,
                    layout.detail,
                    layout.preview,
                    layout.notice,
                ])
            {
                assert!(clear(rect), "{screen:?}: {rect:?}");
            }
            assert!(layout.speed.w >= 44. && layout.speed.y + layout.speed.h <= screen.y);
            assert!(
                layout.detail.w >= text_width_14("Seed Sling L1"),
                "{screen:?}"
            );
            // Results/pause cards begin below the corner when they fit there.
            for tall in [220., 320.] {
                let y = ((screen.y - tall) * 0.5)
                    .max(MENU_CORNER.y)
                    .min((screen.y - tall).max(0.));
                assert!(
                    y >= MENU_CORNER.y && y + tall <= screen.y,
                    "{screen:?} {tall}"
                );
            }
        }
    }

    fn text_width_14(s: &str) -> f32 {
        crate::scene::text_width(s, FONT)
    }
}
