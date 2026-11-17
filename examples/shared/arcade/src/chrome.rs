//! Host-owned presentation: header, footer, menu cards and feedback motion.
//!
//! Everything here is derived from accepted events and wall-clock frame time;
//! none of it feeds back into the simulation. The chrome speaks in icons,
//! pips and numbers; words are reserved for names and short headlines.
use crate::paint::{self, card, chip, darken, divider, lighten, with_alpha};
use crate::*;
use ferravex_render::icons::IconStyle;
use ferravex_render::shapes::{self, Fill, Shadow};
use ferravex_render::text::{TextAlign, TextStyle, draw_text_styled, text_width};

/// Logical rectangles shared by drawing and pointer hit-testing.
pub(crate) const CARD: Rect = Rect {
    x: 130.,
    y: 88.,
    w: 700.,
    h: 470.,
};
pub(crate) const LEFT: f32 = 170.;
pub(crate) const PRIMARY: Rect = Rect {
    x: 170.,
    y: 484.,
    w: 300.,
    h: 48.,
};
pub(crate) const SECONDARY: Rect = Rect {
    x: 490.,
    y: 484.,
    w: 300.,
    h: 48.,
};
pub(crate) const WIDE: Rect = Rect {
    x: 170.,
    y: 420.,
    w: 620.,
    h: 46.,
};
/// Vertical center of the footer's icon buttons.
const FOOTER_Y: f32 = 612.;
const FOOTER_RADIUS: f32 = 17.;
/// The area where the host's toasts appear (inside the footer rail).
pub(crate) const TOAST_AT: Vec2 = vec2(400., 612.);
/// Where the score's floating gains rise from (the host shifts them left of
/// the star so they never cross the digits).
pub(crate) const SCORE_AT: Vec2 = vec2(872., 48.);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FooterButton {
    Pause,
    Help,
    Controls,
    Sound,
}
const FOOTER: [(FooterButton, &str, &str); 4] = [
    (FooterButton::Pause, "Pause", "Esc"),
    (FooterButton::Help, "How to play", "H"),
    (FooterButton::Controls, "Controls", "F2"),
    (FooterButton::Sound, "Sound", "M"),
];
fn footer_center(i: usize) -> Vec2 {
    vec2(920. - (FOOTER.len() - 1 - i) as f32 * 44., FOOTER_Y)
}
/// The footer button under `p` (logical units), if any.
pub(crate) fn footer_button_at(p: Vec2) -> Option<FooterButton> {
    FOOTER
        .iter()
        .enumerate()
        .find(|(i, _)| (p - footer_center(*i)).length() <= FOOTER_RADIUS + 4.)
        .map(|(_, (b, _, _))| *b)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Overlay {
    #[default]
    None,
    Title,
    Help,
    Paused,
    Ended,
}

/// Number of eased hover values: three overlay buttons, four footer buttons.
const HOVERS: usize = 7;

/// Eased presentation state for one host.
#[derive(Default)]
pub(crate) struct Chrome {
    time: f32,
    shown_score: f32,
    score_pulse: f32,
    flash: f32,
    hover: [f32; HOVERS],
    overlay: Overlay,
    /// Fades in after the overlay changes; fades out after it closes.
    presence: f32,
    last: Overlay,
    /// Seconds the current overlay has been showing.
    overlay_age: f32,
}

fn approach(value: &mut f32, target: f32, rate: f32, dt: f32) {
    *value += (target - *value) * (1. - (-rate * dt).exp());
    if (target - *value).abs() < 0.001 {
        *value = target;
    }
}

impl Chrome {
    pub fn new(score: u32) -> Self {
        Self {
            shown_score: score as f32,
            ..Self::default()
        }
    }
    /// Forget motion from a previous run.
    pub fn reset(&mut self, score: u32) {
        *self = Self {
            time: self.time,
            ..Self::new(score)
        };
    }
    /// React to a cue the game accepted during an executed tick.
    pub fn cue(&mut self, cue: Cue) {
        if matches!(cue, Cue::Win | Cue::Good) {
            self.flash = self.flash.max(if cue == Cue::Win { 1. } else { 0.5 });
        }
    }
    pub fn time(&self) -> f32 {
        self.time
    }
    pub fn overlay_age(&self) -> f32 {
        self.overlay_age
    }
    /// Advance eased values by real frame time.
    pub fn update(
        &mut self,
        dt: f32,
        score: u32,
        overlay: Overlay,
        pointer: Vec2,
        buttons: &[Rect],
    ) {
        let dt = if dt.is_finite() {
            dt.clamp(0., 0.1)
        } else {
            0.
        };
        self.time += dt;
        let target = score as f32;
        if target > self.shown_score + 0.5 {
            self.score_pulse = 1.;
        }
        if target < self.shown_score {
            self.shown_score = target;
        }
        approach(&mut self.shown_score, target, 9., dt);
        if (target - self.shown_score).abs() < 0.5 {
            self.shown_score = target;
        }
        self.score_pulse = (self.score_pulse - dt * 2.5).max(0.);
        self.flash = (self.flash - dt * 1.4).max(0.);
        if overlay != self.overlay {
            if overlay == Overlay::None {
                self.last = self.overlay;
            } else {
                self.presence = self.presence.min(0.35);
                self.overlay_age = 0.;
            }
            self.overlay = overlay;
        }
        self.overlay_age += dt;
        let shown = if overlay == Overlay::None { 0. } else { 1. };
        approach(&mut self.presence, shown, 14., dt);
        for (i, hover) in self.hover.iter_mut().enumerate() {
            let over = if i < 3 {
                overlay != Overlay::None && buttons.get(i).is_some_and(|r| r.contains(pointer))
            } else {
                (pointer - footer_center(i - 3)).length() <= FOOTER_RADIUS + 4.
            };
            approach(hover, if over { 1. } else { 0. }, 18., dt);
        }
    }
    /// The overlay to draw this frame (a closing one keeps fading out).
    pub fn visible_overlay(&self) -> Overlay {
        if self.overlay != Overlay::None {
            self.overlay
        } else if self.presence > 0.01 {
            self.last
        } else {
            Overlay::None
        }
    }
    #[cfg(test)]
    pub fn presence(&self) -> f32 {
        self.presence
    }
    pub fn hover(&self, index: usize) -> f32 {
        self.hover.get(index).copied().unwrap_or(0.)
    }

    /// Title, progress, score and the objective (visual when the game draws it).
    pub fn header<G: ArcadeGame>(&self, meta: Meta, hud: &Hud, game: &G) {
        let top = Rect::new(0., 0., WIDTH, 104.);
        shapes::fill_rounded_rect(
            top,
            0.,
            Fill::vertical(top, lighten(INK, 0.045), darken(INK, 0.1)),
        );
        // Brand mark and title.
        let mark = Rect::new(22., 22., 6., 20.);
        shapes::fill_rounded_rect(mark, 3., lighten(meta.accent, 0.5 * self.flash));
        if self.flash > 0.01 {
            shapes::glow_disc(
                vec2(25., 32.),
                22.,
                with_alpha(meta.accent, 0.35 * self.flash),
            );
        }
        let title = TextStyle::modern(22., PAPER).with_shadow(Color::new(0., 0., 0., 0.5));
        let title_w = draw_text_styled(meta.title, 40., 40., title).width;
        // Progress pips, then the stage name, kept clear of the score column.
        let mut x = 40. + title_w + 18.;
        if let Some((current, total)) = game.progress().filter(|(_, t)| *t > 0) {
            x += crate::paint::pips(x, 32., current, total, meta.accent) + 14.;
        }
        let room = 720. - x;
        let mut size = 14.;
        while size > 10.
            && text_width(&hud.stage, TextStyle::modern(size, meta.accent)) + size * 1.3 > room
        {
            size -= 1.;
        }
        if room > 60. && !hud.stage.is_empty() {
            chip(&hud.stage, x, 32. - size * 0.725, size, meta.accent);
        }
        // Score: a star and a rolling number, no label needed.
        let digits = grouped(self.shown_score.round() as u32);
        let pop = 1. + 0.12 * self.score_pulse;
        let value = TextStyle::modern(30. * pop, lighten(meta.accent, 0.25 * self.score_pulse))
            .with_shadow(Color::new(0., 0., 0., 0.5))
            .with_align(TextAlign::Right)
            .with_pixel_snap(self.score_pulse <= 0.);
        let w = draw_text_styled(&digits, 940., 50., value).width;
        icons::draw_styled(
            Icon::Star,
            vec2(940. - w - 18., 39.),
            22. * pop,
            IconStyle::new(meta.accent).shadow(Color::new(0., 0., 0., 0.45)),
        );
        // Objective: the game's own visual summary, or a target and a line.
        if !game.draw_objective(OBJECTIVE_AREA) && !hud.objective.is_empty() {
            icons::draw(
                Icon::Target,
                vec2(32., 81.),
                18.,
                with_alpha(meta.accent, 0.9),
            );
            paint::fit(
                &hud.objective,
                48.,
                87.,
                888.,
                17.,
                Color::new(0.84, 0.88, 0.92, 1.),
            );
        }
        divider(0., 103., WIDTH, with_alpha(meta.accent, 0.7));
    }

    /// Steady footer: four round icon buttons with key tooltips on hover.
    pub fn footer(&self, muted: bool, can_pause: bool, meta: Meta) {
        let bottom = Rect::new(0., 584., WIDTH, 56.);
        shapes::fill_rounded_rect(
            bottom,
            0.,
            Fill::vertical(bottom, darken(INK, 0.05), lighten(INK, 0.02)),
        );
        divider(0., 584., WIDTH, Color::new(1., 1., 1., 0.1));
        for (i, (button, label, key)) in FOOTER.iter().enumerate() {
            let hover = self.hover(3 + i);
            let at = footer_center(i);
            let enabled = *button != FooterButton::Pause || can_pause;
            let fill = mix_color(
                Color::new(0.11, 0.14, 0.19, 1.),
                meta.accent,
                0.22 * hover * f32::from(u8::from(enabled)),
            );
            shapes::fill_circle(
                at + vec2(0., 2.),
                FOOTER_RADIUS,
                Color::new(0., 0., 0., 0.45),
            );
            shapes::fill_circle(at, FOOTER_RADIUS, fill);
            shapes::stroke_circle(
                at,
                FOOTER_RADIUS,
                1.,
                with_alpha(meta.accent, 0.18 + 0.5 * hover),
            );
            let icon = match button {
                FooterButton::Pause => Icon::Pause,
                FooterButton::Help => Icon::Question,
                FooterButton::Controls => Icon::Gamepad,
                FooterButton::Sound if muted => Icon::SoundOff,
                FooterButton::Sound => Icon::SoundOn,
            };
            let ink = if !enabled {
                Color::new(0.4, 0.45, 0.5, 1.)
            } else if *button == FooterButton::Sound && muted {
                Color::new(0.94, 0.45, 0.42, 1.)
            } else {
                lighten(MUTED, 0.4 * hover)
            };
            icons::draw(icon, at, 18., ink);
            if hover > 0.05 && enabled {
                tooltip(at, label, key, hover);
            }
        }
    }

    /// Dim the game and raise the menu card; returns the card's opacity.
    fn backdrop(&self, meta: Meta) -> f32 {
        let a = self.presence;
        paint::panel(0., 0., WIDTH, HEIGHT, with_alpha(INK, 0.74 * a));
        let rise = 14. * (1. - a);
        let r = Rect::new(CARD.x, CARD.y + rise, CARD.w, CARD.h);
        shapes::drop_shadow(
            r,
            16.,
            Shadow::soft(Color::new(0., 0., 0., 0.55 * a))
                .offset(vec2(0., 10.))
                .blur(36.),
        );
        let base = Color::new(0.072, 0.095, 0.13, a);
        shapes::fill_rounded_rect(
            r,
            16.,
            Fill::vertical(r, lighten(base, 0.035), darken(base, 0.08)),
        );
        shapes::stroke_rounded_rect(r, 16., 1., Color::new(1., 1., 1., 0.08 * a));
        let stripe = Rect::new(r.x + 40., r.y, 72., 3.);
        shapes::fill_rounded_rect(stripe, 1.5, with_alpha(meta.accent, a));
        a
    }

    /// Eyebrow and headline; returns the alpha and vertical rise in use.
    pub fn heading(&self, meta: Meta, eyebrow: &str, title: &str) -> (f32, f32) {
        let a = self.backdrop(meta);
        let rise = 14. * (1. - a);
        let eyebrow_style =
            TextStyle::modern(14., with_alpha(meta.accent, a)).with_letter_spacing(1.);
        draw_text_styled(&eyebrow.to_uppercase(), LEFT, 138. + rise, eyebrow_style);
        let headline = TextStyle::modern(40., with_alpha(PAPER, a)).with_shadow(Color::new(
            0.,
            0.,
            0.,
            0.5 * a,
        ));
        let width = text_width(title, headline);
        let headline = if width > 520. {
            headline.with_size(40. * 520. / width)
        } else {
            headline
        };
        draw_text_styled(title, LEFT, 190. + rise, headline);
        (a, rise)
    }

    /// Poster-style title over the live attract demo; returns the alpha.
    pub fn title_scrim(&self) -> f32 {
        let a = self.presence;
        let upper = Rect::new(0., 0., WIDTH, HEIGHT * 0.5);
        let lower = Rect::new(0., HEIGHT * 0.5, WIDTH, HEIGHT * 0.5);
        shapes::fill_rounded_rect(
            upper,
            0.,
            Fill::vertical(upper, with_alpha(INK, 0.92 * a), with_alpha(INK, 0.62 * a)),
        );
        shapes::fill_rounded_rect(
            lower,
            0.,
            Fill::vertical(lower, with_alpha(INK, 0.62 * a), with_alpha(INK, 0.95 * a)),
        );
        // A soft pool of shade behind the centered copy keeps the title and
        // key caps legible over a busy demo; the edges stay lively.
        // The whole canvas, so the gradient fades out with no visible edge.
        let pool = Rect::new(0., 0., WIDTH, HEIGHT);
        shapes::fill_rounded_rect(
            pool,
            0.,
            Fill::radial(
                vec2(WIDTH * 0.5, 320.),
                420.,
                with_alpha(INK, 0.6 * a),
                with_alpha(INK, 0.),
            ),
        );
        a
    }

    /// A menu button with an icon before its label.
    #[allow(clippy::too_many_arguments)]
    pub fn button(
        &self,
        index: usize,
        rect: Rect,
        icon: Icon,
        label: &str,
        primary: bool,
        meta: Meta,
        a: f32,
    ) {
        let hover = self.hover(index);
        let lift = 2. * hover;
        let r = Rect::new(rect.x, rect.y - lift + 14. * (1. - a), rect.w, rect.h);
        if primary {
            // A slow breathing glow invites the obvious next action.
            let breath = 0.5 + 0.5 * (self.time * 2.6).sin();
            shapes::drop_shadow(
                r,
                12.,
                Shadow::soft(with_alpha(meta.accent, (0.18 + 0.2 * breath) * a))
                    .blur(18. + 8. * breath),
            );
        }
        shapes::drop_shadow(
            r,
            12.,
            Shadow::soft(Color::new(0., 0., 0., (0.3 + 0.15 * hover) * a))
                .offset(vec2(0., 3. + lift))
                .blur(12. + 6. * hover),
        );
        let (fill, ink) = if primary {
            (lighten(meta.accent, 0.12 * hover), INK)
        } else {
            (
                mix_color(Color::new(0.13, 0.16, 0.21, 1.), meta.accent, 0.12 * hover),
                PAPER,
            )
        };
        let fill = with_alpha(fill, a);
        shapes::fill_rounded_rect(r, 12., Fill::vertical(r, lighten(fill, 0.1), fill));
        shapes::stroke_rounded_rect(
            r,
            12.,
            1.,
            if primary {
                Color::new(1., 1., 1., 0.25 * a)
            } else {
                with_alpha(meta.accent, (0.25 + 0.5 * hover) * a)
            },
        );
        let mut style = TextStyle::modern(19., with_alpha(ink, a));
        if !primary {
            style = style.with_shadow(Color::new(0., 0., 0., 0.4 * a));
        }
        let icon_size = 18.;
        let gap = 10.;
        let w = icon_size + gap + text_width(label, style);
        let c = r.center();
        let left = c.x - w * 0.5;
        let ink_icon = with_alpha(if primary { INK } else { meta.accent }, a);
        icons::draw(icon, vec2(left + icon_size * 0.5, c.y), icon_size, ink_icon);
        draw_text_styled(
            label,
            left + icon_size + gap,
            style.centered_baseline(c.y),
            style,
        );
    }

    pub fn page_dots(&self, page: usize, pages: usize, meta: Meta, a: f32) {
        let gap = 16.;
        let right = CARD.x + CARD.w - 40.;
        for i in 0..pages.min(12) {
            let x = right - (pages.min(12) - 1 - i) as f32 * gap;
            let current = i == page;
            let w = if current { 18. } else { 7. };
            let r = Rect::new(x - w + 3.5, 172., w, 7.);
            shapes::fill_rounded_rect(
                r,
                3.5,
                if current {
                    with_alpha(meta.accent, a)
                } else {
                    Color::new(1., 1., 1., 0.18 * a)
                },
            );
        }
    }
}

/// A small label above a footer button: action name and its key.
fn tooltip(at: Vec2, label: &str, key: &str, hover: f32) {
    let a = hover.clamp(0., 1.);
    let s = TextStyle::modern(13., with_alpha(PAPER, a));
    let key_w = (text_width(key, s) + 12.).max(20.);
    let w = text_width(label, s) + key_w + 26.;
    let h = 28.;
    let x = (at.x - w * 0.5).min(WIDTH - 8. - w);
    let r = Rect::new(x, at.y - FOOTER_RADIUS - 10. - h - 4. * (1. - a), w, h);
    shapes::drop_shadow(
        r,
        8.,
        Shadow::soft(Color::new(0., 0., 0., 0.4 * a)).blur(10.),
    );
    shapes::fill_rounded_rect(r, 8., Color::new(0.09, 0.115, 0.155, 0.97 * a));
    shapes::stroke_rounded_rect(r, 8., 1., Color::new(1., 1., 1., 0.1 * a));
    let baseline = s.centered_baseline(r.y + h * 0.5);
    let tw = draw_text_styled(label, r.x + 10., baseline, s).width;
    paint::keycap_faded(key, "", r.x + 10. + tw + 6., r.y + 4., PAPER, a);
}

fn mix_color(a: Color, b: Color, t: f32) -> Color {
    shapes::mix(a, b, t)
}

pub(crate) fn keycap_width(key: &str, action: &str) -> f32 {
    let s = TextStyle::modern(13., PAPER);
    let cap = (text_width(key, s) + 12.).max(20.);
    if action.is_empty() {
        cap
    } else {
        cap + 7. + text_width(action, s)
    }
}

/// Thousands separators for scores and money: `12480` → `12,480`.
///
/// ```
/// assert_eq!(ferravex_arcade::grouped(12480), "12,480");
/// ```
pub fn grouped(value: u32) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// A surface for game panels drawn in the same language as the host chrome.
pub fn surface(x: f32, y: f32, w: f32, h: f32) {
    card(x, y, w, h, Color::new(0.07, 0.095, 0.13, 1.));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scores_group_thousands() {
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(999), "999");
        assert_eq!(grouped(1000), "1,000");
        assert_eq!(grouped(1234567), "1,234,567");
    }
    #[test]
    fn score_rolls_up_and_snaps_down_on_restart() {
        let mut chrome = Chrome::new(0);
        for _ in 0..3 {
            chrome.update(1. / 60., 500, Overlay::None, Vec2::ZERO, &[]);
        }
        assert!(chrome.shown_score > 0. && chrome.shown_score < 500.);
        for _ in 0..120 {
            chrome.update(1. / 60., 500, Overlay::None, Vec2::ZERO, &[]);
        }
        assert_eq!(chrome.shown_score, 500.);
        chrome.update(1. / 60., 20, Overlay::None, Vec2::ZERO, &[]);
        assert_eq!(chrome.shown_score, 20.);
    }
    #[test]
    fn overlays_fade_in_and_keep_drawing_while_closing() {
        let mut chrome = Chrome::new(0);
        chrome.update(1. / 60., 0, Overlay::Title, Vec2::ZERO, &[]);
        assert_eq!(chrome.visible_overlay(), Overlay::Title);
        for _ in 0..60 {
            chrome.update(1. / 60., 0, Overlay::Title, Vec2::ZERO, &[]);
        }
        assert!(chrome.presence() > 0.99);
        assert!(chrome.overlay_age() > 0.9);
        chrome.update(1. / 60., 0, Overlay::None, Vec2::ZERO, &[]);
        assert_eq!(chrome.visible_overlay(), Overlay::Title, "fading out");
        for _ in 0..60 {
            chrome.update(1. / 60., 0, Overlay::None, Vec2::ZERO, &[]);
        }
        assert_eq!(chrome.visible_overlay(), Overlay::None);
    }
    #[test]
    fn footer_buttons_hit_test_their_circles_only() {
        for (i, (button, _, _)) in FOOTER.iter().enumerate() {
            let c = footer_center(i);
            assert_eq!(footer_button_at(c), Some(*button));
            assert_eq!(footer_button_at(c + vec2(0., 30.)), None);
        }
        // Buttons stay inside the footer and the logical canvas.
        for i in 0..FOOTER.len() {
            let c = footer_center(i);
            assert!(c.x + FOOTER_RADIUS <= WIDTH && c.y - FOOTER_RADIUS >= 584.);
        }
        // The game area never reaches a footer button.
        assert_eq!(footer_button_at(vec2(800., 560.)), None);
    }
    #[test]
    fn footer_hover_eases_toward_the_pointer() {
        let mut chrome = Chrome::new(0);
        for _ in 0..30 {
            chrome.update(1. / 60., 0, Overlay::None, footer_center(1), &[]);
        }
        assert!(chrome.hover(4) > 0.9 && chrome.hover(3) < 0.01);
    }
}
