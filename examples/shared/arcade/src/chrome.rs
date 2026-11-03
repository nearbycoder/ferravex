//! Host-owned presentation: header, footer, menu cards and feedback motion.
//!
//! Everything here is derived from accepted events and wall-clock frame time;
//! none of it feeds back into the simulation.
use crate::paint::{self, card, chip, darken, divider, keycap, lighten, with_alpha};
use crate::*;
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Overlay {
    #[default]
    None,
    Title,
    Help,
    Paused,
    Ended,
}

/// Eased presentation state for one host.
#[derive(Default)]
pub(crate) struct Chrome {
    time: f32,
    shown_score: f32,
    score_pulse: f32,
    shake: f32,
    flash: f32,
    hover: [f32; 3],
    overlay: Overlay,
    /// Fades in after the overlay changes; fades out after it closes.
    presence: f32,
    last: Overlay,
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
        let calm = ferravex_render::preferences::feedback_policy().reduced_motion();
        match cue {
            Cue::Bad if !calm => self.shake = 1.,
            Cue::Win => self.flash = 1.,
            _ => {}
        }
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
        self.shake = (self.shake - dt * 4.5).max(0.);
        self.flash = (self.flash - dt * 1.4).max(0.);
        if overlay != self.overlay {
            if overlay == Overlay::None {
                self.last = self.overlay;
            } else {
                self.presence = self.presence.min(0.35);
            }
            self.overlay = overlay;
        }
        let shown = if overlay == Overlay::None { 0. } else { 1. };
        approach(&mut self.presence, shown, 14., dt);
        for (i, hover) in self.hover.iter_mut().enumerate() {
            let over =
                overlay != Overlay::None && buttons.get(i).is_some_and(|r| r.contains(pointer));
            approach(hover, if over { 1. } else { 0. }, 18., dt);
        }
    }
    /// Logical-unit camera offset for the current shake.
    pub fn shake_offset(&self) -> Vec2 {
        if self.shake <= 0. {
            return Vec2::ZERO;
        }
        let amp = 4. * self.shake * self.shake;
        vec2((self.time * 97.).sin(), (self.time * 71.).cos()) * amp
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

    pub fn header(&self, meta: Meta, hud: &Hud) {
        let top = Rect::new(0., 0., WIDTH, 104.);
        shapes::fill_rounded_rect(
            top,
            0.,
            Fill::vertical(top, lighten(INK, 0.045), darken(INK, 0.1)),
        );
        // Brand mark and title.
        let mark = Rect::new(22., 22., 6., 20.);
        shapes::fill_rounded_rect(mark, 3., lighten(meta.accent, 0.5 * self.flash));
        let title = TextStyle::modern(24., PAPER).with_shadow(Color::new(0., 0., 0., 0.5));
        let title_w = draw_text_styled(meta.title, 40., 41., title).width;
        // Stage chip, shrunk to stay clear of the score column.
        let x = 40. + title_w + 16.;
        let room = 760. - x;
        let mut size = 14.;
        while size > 10.
            && text_width(&hud.stage, TextStyle::modern(size, meta.accent)) + size * 1.3 > room
        {
            size -= 1.;
        }
        if room > 60. && !hud.stage.is_empty() {
            chip(&hud.stage, x, 32. - size * 0.725, size, meta.accent);
        }
        // Score with a short roll-up and pulse.
        let label = TextStyle::modern(11., MUTED)
            .with_letter_spacing(1.5)
            .with_align(TextAlign::Right);
        draw_text_styled("SCORE", 940., 26., label);
        let digits = grouped(self.shown_score.round() as u32);
        let value = TextStyle::modern(30., lighten(meta.accent, 0.2 * self.score_pulse))
            .with_shadow(Color::new(0., 0., 0., 0.5))
            .with_align(TextAlign::Right);
        draw_text_styled(&digits, 940., 62., value);
        // Objective.
        paint::fit(
            &hud.objective,
            22.,
            84.,
            916.,
            17.,
            Color::new(0.84, 0.88, 0.92, 1.),
        );
        divider(0., 103., WIDTH, with_alpha(meta.accent, 0.7));
    }

    pub fn footer(&self, muted: bool) {
        let bottom = Rect::new(0., 584., WIDTH, 56.);
        shapes::fill_rounded_rect(
            bottom,
            0.,
            Fill::vertical(bottom, darken(INK, 0.05), lighten(INK, 0.02)),
        );
        divider(0., 584., WIDTH, Color::new(1., 1., 1., 0.1));
        let mut x = 22.;
        for (key, action) in [("Esc", "Pause"), ("H", "Help"), ("F2", "Controls")] {
            x += keycap(key, action, x, 600., MUTED) + 22.;
        }
        let action = if muted { "Sound off" } else { "Sound on" };
        let w = keycap_width("M", action);
        keycap("M", action, 938. - w, 600., MUTED);
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
        let headline = if width > 620. {
            headline.with_size(40. * 620. / width)
        } else {
            headline
        };
        draw_text_styled(title, LEFT, 190. + rise, headline);
        (a, rise)
    }

    pub fn button(&self, index: usize, rect: Rect, label: &str, primary: bool, meta: Meta, a: f32) {
        let hover = self.hover(index);
        let lift = 2. * hover;
        let r = Rect::new(rect.x, rect.y - lift + 14. * (1. - a), rect.w, rect.h);
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
        let mut style = TextStyle::modern(19., with_alpha(ink, a)).with_align(TextAlign::Center);
        if !primary {
            style = style.with_shadow(Color::new(0., 0., 0., 0.4 * a));
        }
        let c = r.center();
        draw_text_styled(label, c.x, style.centered_baseline(c.y), style);
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

/// `12480` → `12,480`.
pub(crate) fn grouped(value: u32) -> String {
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
        chrome.update(1. / 60., 0, Overlay::None, Vec2::ZERO, &[]);
        assert_eq!(chrome.visible_overlay(), Overlay::Title, "fading out");
        for _ in 0..60 {
            chrome.update(1. / 60., 0, Overlay::None, Vec2::ZERO, &[]);
        }
        assert_eq!(chrome.visible_overlay(), Overlay::None);
    }
    #[test]
    fn shake_decays_to_rest() {
        let mut chrome = Chrome::new(0);
        chrome.cue(Cue::Bad);
        chrome.update(1. / 60., 0, Overlay::None, Vec2::ZERO, &[]);
        for _ in 0..30 {
            chrome.update(1. / 60., 0, Overlay::None, Vec2::ZERO, &[]);
        }
        assert_eq!(chrome.shake_offset(), Vec2::ZERO);
    }
}
