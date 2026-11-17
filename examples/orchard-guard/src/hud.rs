//! Desktop HUD, banners and overlays in the 1200x760 design space.
//!
//! The HUD speaks in pictograms: icon + number stats, wave pips, key caps
//! under the controls they trigger and one-line banners. Button rectangles
//! for Seed/Upgrade, Pause, Wave and the paused Resume keep their established
//! positions and fills (browser tests locate them by geometry), and Wave
//! stays last in focus order.
use super::*;
use crate::glyph::{self, Glyph, Item};
use crate::host::Refusal;
use crate::juice::{self, Juice, REFUSAL_LIFE};
use crate::quips::{self, Goal, RunSummary};
use crate::save::Best;
use crate::scene::{coin_icon, fit_center, heart_icon, text_width};
use crate::style::sfmt;
use ferravex_render::icons::{self, Icon};
use ferravex_render::shapes;

const SOFT: Color = Color::new(0.45, 0.50, 0.36, 1.);
const WARN: Color = Color::new(0.66, 0.27, 0.18, 1.);
/// Stat tints: damage, fire rate, range and frost slow.
const DAMAGE: Color = Color::new(0.74, 0.31, 0.22, 1.);
const RATE: Color = Color::new(0.86, 0.57, 0.13, 1.);
const RANGE: Color = Color::new(0.24, 0.46, 0.31, 1.);
const SLOW: Color = Color::new(0.24, 0.55, 0.62, 1.);
const ROYAL: Color = Color::new(0.36, 0.23, 0.47, 1.);
pub(super) const SPEED_ID: u64 = 10;
/// Design-space speed toggle. Its right edge stays left of the browser
/// fullscreen menu corner (72 CSS px from the right) at every fitted size.
pub(super) const SPEED_RECT: Rect = Rect {
    x: 976.,
    y: 13.,
    w: 128.,
    h: 44.,
};
/// Leftmost design x of the reserved fullscreen menu corner at the given
/// window size, for layouts fitted with `Viewport::fit` to 1200x760.
#[cfg(test)]
fn corner_x(screen: Vec2) -> (f32, f32) {
    let scale = (screen.x / 1200.).min(screen.y / 760.);
    let left = (screen.x - 1200. * scale) * 0.5;
    let top = (screen.y - 760. * scale) * 0.5;
    ((screen.x - 72. - left) / scale, (60. - top) / scale)
}

/// The current binding for `action` as a short key-cap label, formatted
/// into `buffer` (no allocation). Unbound or unprintable actions read `?`.
pub(super) fn key<'a>(input: &PlayerInput, action: &str, buffer: &'a mut [u8; 48]) -> &'a str {
    input.format_hint(action, buffer).unwrap_or("?")
}

/// A centered pictogram row under a control: its key cap and a preview.
fn caption(rect: Rect, items: &[Item]) {
    glyph::row(
        items,
        vec2(rect.x + rect.w * 0.5, rect.y + rect.h + 11.),
        14.,
        1.,
    );
}

pub(super) fn format_time(seconds: f32) -> String {
    let s = seconds.max(0.) as u32;
    format!("{}:{:02}", s / 60, s % 60)
}

pub(super) fn run_summary(g: &Game) -> RunSummary {
    RunSummary {
        won: g.outcome == Outcome::Won,
        wave: g.wave,
        waves: g.level.waves.len() as u8,
        lives: g.lives,
        max_lives: g.level.lives,
        kills: g.kills,
        perfect_waves: g.perfect_waves,
        royal: g.royal_stopped,
        frost_planted: g.towers.iter().flatten().any(|t| t.kind == Kind::Frost),
        towers: g.towers.iter().flatten().count(),
    }
}

/// Pests still to come in the active wave (arriving plus on the path).
pub(super) fn pests_left(g: &Game) -> usize {
    g.pests.len() + usize::from(g.remaining)
}

/// Pip state for the campaign.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct WaveState {
    /// Waves cleared (filled pips).
    pub cleared: usize,
    /// The wave in progress and the fraction of its pests stopped.
    pub active: Option<(usize, f32)>,
    /// The next wave to send (pulsing pip).
    pub next: Option<usize>,
    /// The wave that overran the orchard.
    pub lost: Option<usize>,
}

pub(super) fn wave_state(g: &Game) -> WaveState {
    let total = g.level.waves.len();
    let wave = usize::from(g.wave);
    let mut state = WaveState {
        cleared: wave,
        active: None,
        next: None,
        lost: None,
    };
    match g.outcome {
        Outcome::Won => state.cleared = total,
        Outcome::Lost => {
            state.cleared = wave.saturating_sub(1);
            state.lost = wave.checked_sub(1);
        }
        Outcome::Playing if g.active && wave > 0 => {
            let count = f32::from(g.level.waves[wave - 1].count).max(1.);
            let done = ((count - pests_left(g) as f32) / count).clamp(0., 1.);
            state.cleared = wave - 1;
            state.active = Some((wave - 1, done));
        }
        Outcome::Playing => state.next = (wave < total).then_some(wave),
    }
    state
}

/// One pip per wave: cleared waves are filled, the wave in progress fills a
/// gold ring as its pests are stopped, the next wave pulses and a lost wave
/// turns red.
pub(super) fn wave_pips(g: &Game, center: Vec2, r: f32) {
    let state = wave_state(g);
    let total = g.level.waves.len();
    let beat = 0.5 + 0.5 * (style::clock() * 3.).sin();
    glyph::pips(
        center,
        total,
        state.cleared,
        state.active,
        state.next,
        beat,
        r,
        GREEN,
    );
    if let Some(i) = state.lost {
        let c = glyph::pip_center(center, total, i, r);
        shapes::fill_circle(c, r, juice::APPLE);
        icons::draw(Icon::Cross, c, r * 1.3, CREAM);
    }
}

fn top_bar(campaign: &Campaign, juice: &Juice) {
    let g = &campaign.game;
    ferravex_render::shapes::fill_rounded_rect(
        Rect::new(0., 0., 1200., 70.),
        0.,
        ferravex_render::shapes::Fill::vertical(
            Rect::new(0., 0., 1200., 70.),
            style::BAR_TOP,
            style::BAR_BOTTOM,
        ),
    );
    draw_rectangle(0., 68., 1200., 2., Color::new(0.23, 0.39, 0.24, 0.35));
    // Logo apple.
    draw_circle(38., 37., 14., juice::APPLE);
    draw_circle(33., 32., 4., Color::new(1., 0.8, 0.7, 0.6));
    draw_line(38., 24., 40., 16., 3., Color::from_hex(0x85704c));
    draw_ellipse(46., 18., 7., 3.5, -20., juice::LEAF);
    label("Orchard Guard", 64., 46., 24., INK);
    icons::draw(Icon::Flag, vec2(531., 35.), 24., GREEN);
    wave_pips(g, vec2(608., 35.), 10.);
    // Hearts and coins wobble when they change.
    let wobble = (juice.hurt * 30.).sin() * 4. * juice.hurt * juice.motion().max(0.4);
    let red = mix_color(juice::APPLE, Color::from_hex(0xff4d3a), juice.hurt);
    heart_icon(760. + wobble, 34., 14., red);
    label(sfmt!("{}", g.lives).as_str(), 782., 46., 26., INK);
    let bump = 1. + juice.coin * 0.25 * juice.motion();
    coin_icon(860., 34., 13. * bump);
    label(sfmt!("{}", g.money).as_str(), 880., 46., 26., INK);
}

/// Declared after Restart and before Wave so Wave stays last in focus order.
fn speed_button(ui: &mut Ui, campaign: &Campaign) {
    let fast = campaign.fast;
    // Painted by the shared control style: gold when fast-forward is on.
    style::toggle(
        ui,
        SPEED_ID,
        SPEED_RECT,
        [Some(Glyph::FastForward), None],
        if fast { "2×" } else { "1×" },
        fast,
        22.,
    );
}

fn mix_color(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0., 1.);
    Color::new(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
        1.,
    )
}

/// The items of a one-line toast: pictogram, headline, signed value chip.
/// `value` must hold the formatted chip number.
pub(super) fn toast_items<'a>(
    toast: &'a juice::Toast,
    value: &'a str,
    out: &mut [Item<'a>; 5],
) -> usize {
    let mut n = 0;
    if let Some(glyph) = toast.glyph {
        out[n] = Item::Glyph(glyph, toast.color);
        n += 1;
    }
    out[n] = Item::Text(&toast.head, toast.color);
    n += 1;
    if let Some((glyph, v)) = toast.value {
        out[n] = Item::Gap(0.35);
        out[n + 1] = Item::Glyph(glyph, juice::APPLE);
        out[n + 2] = Item::Text(value, if v < 0 { juice::APPLE } else { juice::GOLD });
        n += 3;
    }
    n
}

/// The incoming-wave preview as pictograms: beetles, zoomies, their health
/// and the clearing reward. A crown marks the wave with the secret guest.
pub(super) struct Preview {
    beetles: style::StackStr<96>,
    zoomies: style::StackStr<96>,
    hp: style::StackStr<96>,
    reward: style::StackStr<96>,
    fast: bool,
    royal: bool,
}

impl Preview {
    pub fn new(next: &game::WavePreview) -> Self {
        let zoomies = next.fast_count;
        Self {
            beetles: sfmt!("{}", u32::from(next.spec.count) - zoomies),
            zoomies: sfmt!("{zoomies}"),
            hp: sfmt!("{:.0}", next.spec.hp),
            reward: sfmt!("+{}", next.spec.reward),
            fast: zoomies > 0,
            royal: quips::royal_wave(next.number),
        }
    }

    pub fn items<'a>(&'a self, out: &mut [Item<'a>; 14]) -> usize {
        let mut n = 0;
        let mut push = |item| {
            out[n] = item;
            n += 1;
        };
        push(Item::Glyph(Glyph::Beetle, INK));
        push(Item::Text(self.beetles.as_str(), INK));
        if self.fast {
            push(Item::Gap(0.45));
            push(Item::Glyph(Glyph::Zoomie, INK));
            push(Item::Text(self.zoomies.as_str(), INK));
        }
        push(Item::Gap(0.45));
        push(Item::Glyph(Glyph::Health, INK));
        push(Item::Text(self.hp.as_str(), INK));
        push(Item::Gap(0.45));
        push(Item::Glyph(Glyph::Coin, INK));
        push(Item::Text(self.reward.as_str(), Color::from_hex(0x9a6a12)));
        if self.royal {
            push(Item::Gap(0.45));
            push(Item::Glyph(
                Glyph::Icon(Icon::Crown),
                Color::from_hex(0xd99a1e),
            ));
            push(Item::Text("?", ROYAL));
        }
        n
    }
}

/// Toasts take the banner slot; otherwise preparation previews the next wave.
fn banner(campaign: &Campaign, juice: &Juice) {
    let g = &campaign.game;
    let cy = 104.;
    if let Some(toast) = &juice.toast {
        let a = toast.alpha();
        let drop = (1. - a) * 10. * juice.motion();
        let value = toast.value.map_or(sfmt!(""), |(_, v)| sfmt!("{v:+}"));
        let mut items = [Item::Gap(0.); 5];
        let n = toast_items(toast, value.as_str(), &mut items);
        let size = 24.;
        let w = glyph::row_width(&items[..n], size) + 52.;
        let pill = Rect::new(600. - w * 0.5, cy - 25. - drop, w, 50.);
        style::dark_chip(pill, a);
        shapes::stroke_rounded_rect(
            style::grow(pill, -3.),
            22.,
            1.5,
            style::alpha(toast.color, 0.55 * a),
        );
        glyph::row(&items[..n], vec2(600., pill.y + pill.h * 0.5), size, a);
        return;
    }
    if g.active || g.outcome != Outcome::Playing {
        return;
    }
    let Some(next) = g.next_wave() else { return };
    let preview = Preview::new(&next);
    let mut items = [Item::Gap(0.); 14];
    let n = preview.items(&mut items);
    let size = 22.;
    let w = glyph::row_width(&items[..n], size) + 56.;
    let card = Rect::new(600. - w * 0.5, cy - 24., w, 48.);
    shapes::drop_shadow(
        card,
        24.,
        shapes::Shadow::soft(style::SHADOW)
            .offset(vec2(0., 4.))
            .blur(12.),
    );
    shapes::fill_rounded_rect(card, 24., Color::new(0.99, 0.96, 0.87, 0.97));
    shapes::stroke_rounded_rect(card, 24., 2., GOLD);
    glyph::row(&items[..n], vec2(600., cy), size, 1.);
}

/// The pictogram answer to a refused command, after a red cross badge.
pub(super) fn refusal_items<'a>(
    refusal: Refusal,
    number: &'a str,
    keys: [&'a str; 3],
    out: &mut [Item<'a>; 6],
) -> usize {
    use game::ActionError as E;
    let items: &[Item] = match refusal {
        Refusal::Rejected(_, E::Funds { .. }) => &[
            Item::Glyph(Glyph::Coin, INK),
            Item::Text(number, juice::APPLE),
        ],
        // Already built: upgrade it instead.
        Refusal::Rejected(_, E::Occupied) => &[
            Item::Glyph(Glyph::Icon(Icon::ArrowUp), CREAM),
            Item::Key(keys[2]),
        ],
        // Nothing to upgrade: plant first.
        Refusal::Rejected(_, E::Empty) => &[
            Item::Glyph(Glyph::Tower(Kind::Seed), CREAM),
            Item::Key(keys[0]),
            Item::Glyph(Glyph::Tower(Kind::Frost), CREAM),
            Item::Key(keys[1]),
        ],
        Refusal::Rejected(_, E::FullyGrown) => &[
            Item::Glyph(Glyph::Icon(Icon::Crown), juice::GOLD),
            Item::Text("Max", CREAM),
        ],
        Refusal::Rejected(_, E::WaveActive) => &[
            Item::Glyph(Glyph::Icon(Icon::Hourglass), CREAM),
            Item::Glyph(Glyph::Beetle, CREAM),
            Item::Text(number, CREAM),
        ],
        Refusal::Rejected(_, E::NoMoreWaves) => &[
            Item::Glyph(Glyph::Icon(Icon::Flag), CREAM),
            Item::Glyph(Glyph::Icon(Icon::Check), juice::LEAF),
        ],
        Refusal::Rejected(_, E::Paused) => &[Item::Glyph(Glyph::Icon(Icon::Pause), CREAM)],
        Refusal::Busy(_) => &[Item::Glyph(Glyph::Icon(Icon::Hourglass), CREAM)],
        Refusal::Rejected(_, E::Finished | E::InvalidPlot) => &[],
    };
    out[..items.len()].copy_from_slice(items);
    items.len()
}

/// The number a refusal chip shows: the coin shortfall or pests still out.
pub(super) fn refusal_number(g: &Game, refusal: Refusal) -> style::StackStr<96> {
    match refusal {
        Refusal::Rejected(_, game::ActionError::Funds { missing }) => sfmt!("-{missing}"),
        Refusal::Rejected(_, game::ActionError::WaveActive) => sfmt!("{}", pests_left(g)),
        _ => sfmt!(""),
    }
}

/// The control a refusal belongs to, so it can shake.
pub(super) fn refusal_control(refusal: Refusal) -> Option<u64> {
    let action = match refusal {
        Refusal::Rejected(action, _) | Refusal::Busy(action) => action,
    };
    match action {
        Action::Build(Kind::Seed) => Some(1),
        Action::Build(Kind::Frost) => Some(2),
        Action::Upgrade => Some(3),
        Action::Wave => Some(4),
        _ => None,
    }
}

/// Refusals and storage trouble surface as short pictogram chips above the
/// tower panel; successful actions speak through the board instead.
fn notice(campaign: &Campaign, juice: &Juice, input: &PlayerInput, save_error: bool) {
    let cy = 637.;
    if let Some((refusal, age)) = juice.refusal {
        let a = (age / 0.1)
            .min(1.)
            .min((REFUSAL_LIFE - age) / 0.3)
            .clamp(0., 1.);
        let g = &campaign.game;
        let number = refusal_number(g, refusal);
        let (mut b0, mut b1, mut b2) = ([0; 48], [0; 48], [0; 48]);
        let keys = [
            key(input, "select_one", &mut b0),
            key(input, "select_two", &mut b1),
            key(input, "upgrade", &mut b2),
        ];
        let mut items = [Item::Gap(0.); 6];
        let n = refusal_items(refusal, number.as_str(), keys, &mut items);
        if n > 0 {
            let size = 18.;
            let w = glyph::row_width(&items[..n], size) + size * 1.6 + 30.;
            let rise = (1. - (age / 0.15).min(1.)) * 6. * juice.motion();
            let chip = Rect::new(600. - w * 0.5, cy - 17. + rise, w, 34.);
            style::dark_chip(chip, a);
            icons::badge(
                Icon::Cross,
                vec2(chip.x + 20., chip.y + 17.),
                22.,
                style::alpha(juice::APPLE, a),
                style::alpha(CREAM, a),
            );
            glyph::row_at(&items[..n], chip.x + 38., chip.y + 17., size, a);
        }
        return;
    }
    if save_error {
        let items = [
            Item::Glyph(Glyph::Icon(Icon::Alert), juice::GOLD),
            Item::Text("Save unavailable", CREAM),
        ];
        let w = glyph::row_width(&items, 16.) + 32.;
        let chip = Rect::new(600. - w * 0.5, cy - 15., w, 30.);
        style::dark_chip(chip, 1.);
        glyph::row(&items, vec2(600., cy), 16., 1.);
    }
}

/// The selected tower: name with level pips, then damage, fire rate and range.
fn tower_card(tower: Tower) {
    let name = tower.kind.name();
    label(name, 24., 691., 21., INK);
    let x = 24. + text_width(name, 21.) + 16.;
    for n in 0..3u8 {
        let c = vec2(x + f32::from(n) * 13., 684.);
        shapes::fill_circle(c, 5., Color::new(0.24, 0.31, 0.18, 0.25));
        if n <= tower.level {
            shapes::fill_circle(c, 4., juice::GOLD);
        }
    }
    let damage = sfmt!("{:.0}", tower.damage());
    let reload = sfmt!("{:.2}s", tower.reload());
    let range = sfmt!("{:.0}", tower.range());
    glyph::row_at(
        &[
            Item::Glyph(Glyph::Icon(Icon::Sword), DAMAGE),
            Item::Text(damage.as_str(), INK),
            Item::Gap(0.7),
            Item::Glyph(Glyph::Icon(Icon::Bolt), RATE),
            Item::Text(reload.as_str(), INK),
        ],
        24.,
        716.,
        16.,
        1.,
    );
    let frost = [
        Item::Glyph(Glyph::Icon(Icon::Target), RANGE),
        Item::Text(range.as_str(), INK),
        Item::Gap(0.7),
        Item::Glyph(Glyph::Icon(Icon::Snowflake), SLOW),
    ];
    let items = if tower.kind == Kind::Frost {
        &frost[..]
    } else {
        &frost[..2]
    };
    glyph::row_at(items, 24., 741., 16., 1.);
}

/// An empty plot, as drawn on the board, pointing at the tower choices.
fn empty_card(motion: f32) {
    let c = vec2(64., 711.);
    shapes::fill_circle(
        c,
        24.,
        shapes::Fill::radial(
            c - vec2(8., 9.),
            34.,
            Color::from_hex(0xe0d4a2),
            Color::from_hex(0xbfb07c),
        ),
    );
    shapes::stroke_circle(c, 24., 2.5, CREAM);
    for (dx, dy) in [(7., 0.), (0., 7.)] {
        draw_line(
            c.x - dx,
            c.y - dy,
            c.x + dx,
            c.y + dy,
            2.5,
            Color::from_hex(0x7d8a5f),
        );
    }
    let bob = (style::clock() * 4.).sin() * 6. * motion;
    icons::draw(Icon::ArrowRight, vec2(156. + bob, 711.), 34., GREEN);
}

pub(super) fn hud(
    ui: &mut Ui,
    campaign: &Campaign,
    juice: &Juice,
    save_error: bool,
    input: &PlayerInput,
) {
    let g = &campaign.game;
    if campaign.session.is_paused() || g.outcome != Outcome::Playing {
        // Keep the frame recognisable under overlays; no controls are live.
        top_bar(campaign, juice);
        draw_rectangle(0., 660., 1200., 100., CREAM);
        return;
    }
    banner(campaign, juice);
    notice(campaign, juice, input, save_error);
    top_bar(campaign, juice);
    draw_rectangle(0., 660., 1200., 100., CREAM);
    draw_rectangle(0., 660., 1200., 2., Color::new(0.23, 0.39, 0.24, 0.35));
    let mut buffer = [0; 48];
    if let Some(tower) = g.towers[g.selected] {
        tower_card(tower);
        let status = g.upgrade_status(g.selected);
        let rect = Rect::new(240., 683., 415., 55.);
        if tower.level == 2 {
            style::control_glyphs(
                ui,
                3,
                rect,
                [Some(Glyph::Icon(Icon::Crown)), None],
                "Max",
                status.is_ok(),
                22.,
            );
        } else {
            let cost = sfmt!("{}", tower.upgrade_cost());
            style::control_glyphs(
                ui,
                3,
                rect,
                [Some(Glyph::Icon(Icon::ArrowUp)), Some(Glyph::Coin)],
                cost.as_str(),
                status.is_ok(),
                24.,
            );
        }
        let hint = key(input, "upgrade", &mut buffer);
        match status {
            Ok(()) => {
                // Preview the next level with the same stat pictograms.
                let next = Tower {
                    level: tower.level + 1,
                    ..tower
                };
                let damage = sfmt!("{:.0}", next.damage());
                let reload = sfmt!("{:.2}s", next.reload());
                let range = sfmt!("{:.0}", next.range());
                caption(
                    rect,
                    &[
                        Item::Key(hint),
                        Item::Gap(0.3),
                        Item::Glyph(Glyph::Icon(Icon::Sword), DAMAGE),
                        Item::Text(damage.as_str(), GREEN),
                        Item::Gap(0.5),
                        Item::Glyph(Glyph::Icon(Icon::Bolt), RATE),
                        Item::Text(reload.as_str(), GREEN),
                        Item::Gap(0.5),
                        Item::Glyph(Glyph::Icon(Icon::Target), RANGE),
                        Item::Text(range.as_str(), GREEN),
                    ],
                );
            }
            Err(ActionError::Funds { missing }) => {
                let short = sfmt!("-{missing}");
                caption(
                    rect,
                    &[
                        Item::Key(hint),
                        Item::Gap(0.3),
                        Item::Glyph(Glyph::Coin, INK),
                        Item::Text(short.as_str(), WARN),
                    ],
                );
            }
            Err(_) => {}
        }
    } else {
        empty_card(juice.motion());
        for (id, kind, x, action) in [
            (1, Kind::Seed, 240., "select_one"),
            (2, Kind::Frost, 443., "select_two"),
        ] {
            let status = g.build_status(g.selected, kind);
            let rect = Rect::new(x, 683., 190., 55.);
            let price = sfmt!("{}", kind.price());
            style::control_glyphs(
                ui,
                id,
                rect,
                [Some(Glyph::Tower(kind)), Some(Glyph::Coin)],
                price.as_str(),
                status.is_ok(),
                24.,
            );
            let hint = key(input, action, &mut buffer);
            let damage = sfmt!("{:.0}", Tower::planted(kind).damage());
            let short = match status {
                Err(ActionError::Funds { missing }) => Some(sfmt!("-{missing}")),
                _ => None,
            };
            match (&short, kind) {
                (Some(short), _) => caption(
                    rect,
                    &[
                        Item::Key(hint),
                        Item::Gap(0.3),
                        Item::Glyph(Glyph::Coin, INK),
                        Item::Text(short.as_str(), WARN),
                    ],
                ),
                (None, Kind::Seed) => caption(
                    rect,
                    &[
                        Item::Key(hint),
                        Item::Gap(0.3),
                        Item::Glyph(Glyph::Icon(Icon::Sword), DAMAGE),
                        Item::Text(damage.as_str(), GREEN),
                    ],
                ),
                (None, Kind::Frost) => caption(
                    rect,
                    &[
                        Item::Key(hint),
                        Item::Gap(0.3),
                        Item::Glyph(Glyph::Icon(Icon::Snowflake), SLOW),
                        Item::Glyph(Glyph::Icon(Icon::Sword), DAMAGE),
                        Item::Text(damage.as_str(), GREEN),
                    ],
                ),
            }
        }
    }
    let pause = Rect::new(695., 683., 110., 55.);
    style::control_glyphs(
        ui,
        6,
        pause,
        [Some(Glyph::Icon(Icon::Pause)), None],
        "",
        campaign.session.is_started(),
        30.,
    );
    caption(pause, &[Item::Key(key(input, "pause", &mut buffer))]);
    let restart = Rect::new(817., 683., 128., 55.);
    style::control_glyphs(
        ui,
        5,
        restart,
        [Some(Glyph::Icon(Icon::Restart)), None],
        "",
        true,
        30.,
    );
    caption(restart, &[Item::Key(key(input, "restart", &mut buffer))]);
    speed_button(ui, campaign);
    let wave = Rect::new(957., 683., 215., 55.);
    let ready = g.wave_status().is_ok();
    if g.active {
        let left = sfmt!("{}", pests_left(g));
        style::control_glyphs(
            ui,
            4,
            wave,
            [Some(Glyph::Beetle), None],
            left.as_str(),
            ready,
            24.,
        );
    } else if let Some(next) = g.next_wave() {
        // Once something is planted, the send button breathes to invite a wave.
        if ready && g.towers.iter().any(Option::is_some) {
            let beat = 0.5 + 0.5 * (style::clock() * 3.2).sin();
            shapes::fill_rounded_rect(
                style::grow(wave, 3. + 5. * beat),
                16.,
                Color::new(0.96, 0.69, 0.27, 0.16 + 0.18 * beat),
            );
        }
        let title = sfmt!("Wave {}", next.number);
        style::control_glyphs(
            ui,
            4,
            wave,
            [Some(Glyph::Icon(Icon::Play)), None],
            title.as_str(),
            ready,
            22.,
        );
        let count = sfmt!("{}", next.spec.count);
        caption(
            wave,
            &[
                Item::Key(key(input, "wave", &mut buffer)),
                Item::Gap(0.3),
                Item::Glyph(Glyph::Beetle, INK),
                Item::Text(count.as_str(), GREEN),
            ],
        );
    } else {
        style::control_glyphs(
            ui,
            4,
            wave,
            [Some(Glyph::Icon(Icon::Trophy)), None],
            "",
            ready,
            26.,
        );
    }
}

/// One results stat: a large pictogram, its value and an optional badge.
fn stat(x: f32, cy: f32, glyph: Glyph, tint: Color, value: &str, badge: Option<&str>) {
    glyph::draw(glyph, vec2(x + 17., cy), 32., tint, 1.);
    let style = style::body(26., INK);
    let w = style::text(value, x + 44., style.centered_baseline(cy), style);
    if let Some(badge) = badge {
        let bx = x + 44. + w + 12.;
        let bw = text_width(badge, 12.) + 14.;
        let pill = Rect::new(bx, cy - 10., bw, 20.);
        shapes::fill_rounded_rect(pill, 10., GOLD);
        let small = style::body(12., INK);
        style::text_center(badge, bx + bw * 0.5, small.centered_baseline(cy), small);
    }
}

/// The next goal as pictograms: a target, then what to do.
pub(super) fn goal_row(goal: Goal, fastest: f32, center: Vec2, size: f32) {
    let target = Item::Glyph(Glyph::Icon(Icon::Target), GREEN);
    let a = match goal {
        Goal::BeatBest { more, .. } => sfmt!("+{more}"),
        Goal::AllHearts(n) => sfmt!("{n}/{n}"),
        Goal::FindRoyal(wave) => sfmt!("{wave}"),
        Goal::Fastest => sfmt!("< {}", format_time(fastest)),
        _ => sfmt!(""),
    };
    let b = match goal {
        Goal::BeatBest { best, .. } => sfmt!("{best}"),
        _ => sfmt!(""),
    };
    let items: &[Item] = match goal {
        Goal::BeatBest { .. } => &[
            target,
            Item::Glyph(Glyph::Squashed, INK),
            Item::Text(a.as_str(), GREEN),
            Item::Gap(0.5),
            Item::Glyph(Glyph::Icon(Icon::Trophy), RATE),
            Item::Text(b.as_str(), INK),
        ],
        Goal::PlantTower => &[
            target,
            Item::Glyph(Glyph::Tower(Kind::Seed), INK),
            Item::Glyph(Glyph::Icon(Icon::Plus), GREEN),
            Item::Text("Plant towers", GREEN),
        ],
        Goal::PlantFrost => &[
            target,
            Item::Glyph(Glyph::Tower(Kind::Frost), INK),
            Item::Glyph(Glyph::Icon(Icon::Snowflake), SLOW),
            Item::Text("Try frost", GREEN),
        ],
        Goal::MoreTowers => &[
            target,
            Item::Glyph(Glyph::Tower(Kind::Seed), INK),
            Item::Glyph(Glyph::Tower(Kind::Frost), INK),
            Item::Glyph(Glyph::Icon(Icon::Plus), GREEN),
            Item::Text("More towers", GREEN),
        ],
        Goal::UpgradeEarly => &[
            target,
            Item::Glyph(Glyph::Icon(Icon::ArrowUp), GREEN),
            Item::Text("Upgrade early", GREEN),
        ],
        Goal::AllHearts(_) => &[
            target,
            Item::Glyph(Glyph::Heart, juice::APPLE),
            Item::Text(a.as_str(), GREEN),
            Item::Glyph(Glyph::Icon(Icon::ArrowRight), SOFT),
            Item::Text("Orchard Legend", GREEN),
        ],
        Goal::FindRoyal(_) => &[
            target,
            Item::Glyph(Glyph::Icon(Icon::Crown), Color::from_hex(0xd99a1e)),
            Item::Text("?", ROYAL),
            Item::Gap(0.4),
            Item::Glyph(Glyph::Icon(Icon::Flag), GREEN),
            Item::Text(a.as_str(), GREEN),
        ],
        Goal::Fastest => &[
            target,
            Item::Glyph(Glyph::Icon(Icon::Clock), GREEN),
            Item::Text(a.as_str(), GREEN),
        ],
    };
    glyph::row(items, center, size, 1.);
}

/// The fastest harvest to beat: the previous record or this run.
pub(super) fn fastest(g: &Game, best: Option<Best>) -> f32 {
    best.and_then(|b| b.previous_fastest_us)
        .map_or(g.time, |us| (us as f32 / 1e6).min(g.time))
}

pub(super) fn overlay(
    ui: &mut Ui,
    campaign: &Campaign,
    best: Option<Best>,
    input: &PlayerInput,
    festival: impl FnOnce(),
) {
    let g = &campaign.game;
    let paused = campaign.session.is_paused();
    if !paused && g.outcome == Outcome::Playing {
        return;
    }
    let mut buffer = [0; 48];
    draw_rectangle(0., 0., 1200., 760., Color::new(0.14, 0.20, 0.13, 0.55));
    if paused {
        style::card(Rect::new(345., 215., 510., 300.), 18., 1.);
        draw_rectangle(345., 215., 510., 6., GOLD);
        glyph::row(
            &[
                Item::Glyph(Glyph::Icon(Icon::Pause), GREEN),
                Item::Text("Paused", INK),
            ],
            vec2(600., 258.),
            30.,
            1.,
        );
        fit_center(
            quips::pause(g.kills + u32::from(g.wave)),
            600.,
            300.,
            15.,
            470.,
            GREEN,
        );
        let hearts = sfmt!("{}", g.lives);
        let stopped = sfmt!("{}", g.kills);
        let coins = sfmt!("{}", g.money);
        let items = [
            Item::Glyph(Glyph::Heart, juice::APPLE),
            Item::Text(hearts.as_str(), INK),
            Item::Gap(0.6),
            Item::Glyph(Glyph::Squashed, INK),
            Item::Text(stopped.as_str(), INK),
            Item::Gap(0.6),
            Item::Glyph(Glyph::Coin, INK),
            Item::Text(coins.as_str(), INK),
        ];
        let size = 20.;
        let pips_w = 8. * 3.2 * (g.level.waves.len().max(1) - 1) as f32 + 16.;
        let total = pips_w + 28. + glyph::row_width(&items, size);
        let left = 600. - total * 0.5;
        wave_pips(g, vec2(left + pips_w * 0.5, 346.), 8.);
        glyph::row_at(&items, left + pips_w + 28., 346., size, 1.);
        ui.modal(100).unwrap();
        let resume = Rect::new(375., 404., 215., 55.);
        style::control_glyphs(
            ui,
            7,
            resume,
            [Some(Glyph::Icon(Icon::Play)), None],
            "Resume",
            true,
            21.,
        );
        caption(resume, &[Item::Key(key(input, "pause", &mut buffer))]);
        let again = Rect::new(610., 404., 215., 55.);
        style::control_glyphs(
            ui,
            8,
            again,
            [Some(Glyph::Icon(Icon::Restart)), None],
            "Plant again",
            true,
            21.,
        );
        caption(again, &[Item::Key(key(input, "restart", &mut buffer))]);
        return;
    }
    festival();
    let won = g.outcome == Outcome::Won;
    let run = run_summary(g);
    let rank = quips::rank(run);
    let (x, y, w, h) = (280., 88., 640., 562.);
    draw_rectangle(x + 5., y + 6., w, h, Color::new(0.1, 0.14, 0.09, 0.3));
    draw_rectangle(x, y, w, h, CREAM);
    // Harvest-festival bunting (muted after a loss).
    let colors = if won {
        [juice::APPLE, GOLD, juice::LEAF, juice::ICE]
    } else {
        [
            SOFT,
            Color::from_hex(0x9b8a6a),
            SOFT,
            Color::from_hex(0x8a8f78),
        ]
    };
    let sag = |t: f32| (t * std::f32::consts::PI).sin() * 10.;
    for i in 0..16 {
        let t0 = i as f32 / 16.;
        let t1 = (i as f32 + 1.) / 16.;
        let a = vec2(x + w * t0, y + sag(t0));
        let b = vec2(x + w * t1, y + sag(t1));
        let mid = (a + b) * 0.5 + vec2(0., 20.);
        draw_triangle(a, b, mid, colors[i % 4]);
    }
    let title = if won {
        "Harvest saved!"
    } else {
        "Orchard overrun!"
    };
    center(title, 600., y + 74., 36., INK);
    wave_pips(g, vec2(600., y + 98.), 7.);
    let ribbon = if won { GOLD } else { Color::from_hex(0xd8c49a) };
    draw_rectangle(400., y + 116., 400., 66., ribbon);
    draw_triangle(
        vec2(400., y + 116.),
        vec2(380., y + 149.),
        vec2(400., y + 182.),
        ribbon,
    );
    draw_triangle(
        vec2(800., y + 116.),
        vec2(820., y + 149.),
        vec2(800., y + 182.),
        ribbon,
    );
    let rank_style = style::body(30., INK);
    style::text_fit(
        rank.title,
        600.,
        rank_style.centered_baseline(y + 149.),
        rank_style,
        370.,
    );
    fit_center(rank.quip, 600., y + 206., 15., 590., GREEN);
    let best_score = best.and_then(|b| b.previous_score);
    let new_score = best.is_some_and(|b| b.new_score && b.previous_score.is_some());
    let (c1, c2, top) = (340., 630., y + 254.);
    let kills = sfmt!("{}", g.kills);
    stat(
        c1,
        top,
        Glyph::Squashed,
        INK,
        kills.as_str(),
        new_score.then_some("New best!"),
    );
    let hearts = sfmt!("{} / {}", g.lives, g.level.lives);
    stat(
        c2,
        top,
        Glyph::Heart,
        juice::APPLE,
        hearts.as_str(),
        (won && g.lives == g.level.lives).then_some("Flawless"),
    );
    let perfect = sfmt!("{} / {}", g.perfect_waves, g.level.waves.len());
    stat(
        c1,
        top + 60.,
        Glyph::Icon(Icon::Star),
        RATE,
        perfect.as_str(),
        None,
    );
    let lucky = sfmt!("{}", g.lucky_hits);
    stat(c2, top + 60., Glyph::Lucky, DAMAGE, lucky.as_str(), None);
    if won {
        let new_time = best.is_some_and(|b| b.new_fastest && b.previous_fastest_us.is_some());
        stat(
            c1,
            top + 120.,
            Glyph::Icon(Icon::Clock),
            GREEN,
            &format_time(g.time),
            new_time.then_some("Fastest!"),
        );
    } else {
        let survived = sfmt!("{} / {}", g.wave.saturating_sub(1), g.level.waves.len());
        stat(
            c1,
            top + 120.,
            Glyph::Icon(Icon::Flag),
            GREEN,
            survived.as_str(),
            None,
        );
    }
    let previous = match (best_score, best.and_then(|b| b.previous_fastest_us)) {
        (Some(score), Some(us)) => format!("{score} · {}", format_time(us as f32 / 1e6)),
        (Some(score), None) => score.to_string(),
        _ => "First run!".into(),
    };
    stat(
        c2,
        top + 120.,
        Glyph::Icon(Icon::Trophy),
        RATE,
        &previous,
        None,
    );
    if g.royal_stopped {
        glyph::row(
            &[
                Item::Glyph(Glyph::Icon(Icon::Crown), Color::from_hex(0xd99a1e)),
                Item::Text("Sir Crunchalot", ROYAL),
                Item::Glyph(Glyph::Icon(Icon::Check), GREEN),
            ],
            vec2(600., y + 427.),
            16.,
            1.,
        );
    }
    draw_rectangle(
        310.,
        y + 446.,
        580.,
        32.,
        Color::new(0.23, 0.39, 0.24, 0.12),
    );
    goal_row(
        quips::next_goal(run, best_score),
        fastest(g, best),
        vec2(600., y + 462.),
        19.,
    );
    ui.modal(100).unwrap();
    let again = Rect::new(490., y + 488., 220., 55.);
    style::control_glyphs(
        ui,
        8,
        again,
        [Some(Glyph::Icon(Icon::Restart)), None],
        "Plant again",
        true,
        22.,
    );
    caption(again, &[Item::Key(key(input, "restart", &mut buffer))]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::ActionError as E;

    #[test]
    fn fullscreen_menu_corner_stays_clear_of_desktop_hud() {
        for screen in [
            vec2(1280., 800.),
            vec2(1600., 1000.),
            vec2(900., 600.),
            vec2(2560., 1440.),
        ] {
            let (x, y) = corner_x(screen);
            // The speed toggle is the rightmost top-bar element; four-digit
            // coin counts end at 880 + 4 glyphs of 26px.
            let coins_right = 880. + text_width("9999", 26.);
            for right in [SPEED_RECT.x + SPEED_RECT.w, coins_right] {
                assert!(right <= x || SPEED_RECT.y >= y, "{screen:?}: {right} > {x}");
            }
            assert!(
                coins_right + 12. <= SPEED_RECT.x,
                "coins and speed stay apart"
            );
        }
    }

    #[test]
    fn wave_pips_track_cleared_active_next_and_lost_waves() {
        let state = |cleared, active, next, lost| WaveState {
            cleared,
            active,
            next,
            lost,
        };
        let mut g = Game::new();
        assert_eq!(wave_state(&g), state(0, None, Some(0), None));
        g.launch();
        assert_eq!(
            wave_state(&g),
            state(0, Some((0, 0.)), None, None),
            "nothing stopped yet"
        );
        g.remaining = 0;
        g.pests.clear();
        assert_eq!(wave_state(&g).active, Some((0, 1.)));
        g.active = false;
        assert_eq!(wave_state(&g), state(1, None, Some(1), None));
        g.outcome = Outcome::Lost;
        assert_eq!(wave_state(&g), state(0, None, None, Some(0)));
        g.outcome = Outcome::Won;
        assert_eq!(wave_state(&g).cleared, g.level.waves.len());
    }

    #[test]
    fn refusal_chips_keep_exact_amounts_and_point_at_their_control() {
        let mut g = Game::new();
        let funds = Refusal::Rejected(Action::Upgrade, E::Funds { missing: 48 });
        assert_eq!(refusal_number(&g, funds).as_str(), "-48");
        assert_eq!(refusal_control(funds), Some(3));
        let mut out = [Item::Gap(0.); 6];
        let n = refusal_items(funds, "-48", ["1", "2", "U"], &mut out);
        assert!(matches!(
            out[..n],
            [Item::Glyph(Glyph::Coin, _), Item::Text("-48", _)]
        ));
        // Already built: the chip points at the upgrade key instead.
        let occupied = Refusal::Rejected(Action::Build(Kind::Seed), E::Occupied);
        let n = refusal_items(occupied, "", ["1", "2", "U"], &mut out);
        assert!(matches!(out[..n], [_, Item::Key("U")]));
        assert_eq!(refusal_control(occupied), Some(1));
        g.launch();
        let busy = Refusal::Rejected(Action::Wave, E::WaveActive);
        assert_eq!(
            refusal_number(&g, busy).as_str(),
            pests_left(&g).to_string()
        );
        assert_eq!(refusal_control(Refusal::Busy(Action::Wave)), Some(4));
        // A click beside the plots has no chip: the plots pulse instead.
        let missed = Refusal::Rejected(Action::Select(None), E::InvalidPlot);
        assert_eq!(refusal_items(missed, "", ["1", "2", "U"], &mut out), 0);
        assert_eq!(refusal_control(missed), None);
    }

    #[test]
    fn previews_show_every_count_and_the_royal_crown_only_on_its_wave() {
        let mut g = Game::new();
        let preview = Preview::new(&g.next_wave().unwrap());
        let mut out = [Item::Gap(0.); 14];
        let n = preview.items(&mut out);
        let texts: Vec<_> = out[..n]
            .iter()
            .filter_map(|item| match item {
                Item::Text(s, _) => Some(*s),
                _ => None,
            })
            .collect();
        let wave = g.next_wave().unwrap();
        assert_eq!(
            texts,
            [
                (u32::from(wave.spec.count) - wave.fast_count).to_string(),
                wave.fast_count.to_string(),
                format!("{:.0}", wave.spec.hp),
                format!("+{}", wave.spec.reward),
            ]
        );
        g.wave = 2;
        let preview = Preview::new(&g.next_wave().unwrap());
        let n = preview.items(&mut out);
        assert!(
            matches!(out[n - 1], Item::Text("?", _)),
            "wave three wears the crown"
        );
    }
}
