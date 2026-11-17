//! Screen-space UI built with the modern kit: score chip, hole title card,
//! power dock, headline banner, the between-hole sheet, menus and results.
//!
//! Every function runs twice per frame. The layout pass (`paint == false`,
//! during `prepare`) submits only `Ui` input: invisible buttons, the modal
//! blocker and a transparent anchor rectangle for each centered card. The
//! paint pass (`paint == true`, during `draw`, after the post-processing HUD
//! boundary) draws the same rectangles with the kit. One function owns both,
//! so layout and visuals cannot drift apart.
//!
//! Words are kept to names, numbers, short headlines and button labels:
//! everything else is a mark from [`super::marks`] (flag = par, ball =
//! strokes, pips = holes, circles and squares = scorecard results).
use super::marks::{self, Mark};
use super::*;
use ferravex_play::tween::{Appearance, Entrance};
use ferravex_render::{
    icons::Icon,
    shapes::{self, Fill, Shadow, Stroke},
    text::{TextAlign, TextStyle as Type, draw_text_styled, text_width},
    ui::kit::{self, ButtonKind, KitTheme},
};
use std::fmt::Write as _;

/// Copper Greens palette on the kit's roles (contrast-checked by hand: ink
/// and muted copy on `surface` exceed 4.5:1; dark ink on the copper accent ~7:1).
pub(super) fn golf_theme() -> KitTheme {
    KitTheme {
        backdrop: Color::new(0.03, 0.07, 0.05, 0.55),
        surface: Color::from_rgba(27, 51, 43, 255),
        raised: Color::from_rgba(44, 80, 64, 255),
        header: Color::from_rgba(38, 70, 56, 255),
        ink: PAPER,
        muted: DIM,
        disabled: Color::from_rgba(118, 132, 116, 255),
        accent: Color::from_rgba(228, 142, 88, 255),
        accent_ink: Color::from_rgba(36, 18, 8, 255),
        success: MINT,
        warning: GOLD,
        danger: Color::from_rgba(226, 112, 92, 255),
        focus: GOLD,
        line: Color::new(1., 1., 1., 0.1),
        shadow: Color::new(0., 0., 0., 0.5),
        ..KitTheme::default()
    }
}

/// Reusable formatting buffer, so HUD copy never allocates per frame.
#[derive(Default)]
pub(super) struct Scribe(String);
impl Scribe {
    pub fn f(&mut self, args: std::fmt::Arguments<'_>) -> &str {
        self.0.clear();
        let _ = self.0.write_fmt(args);
        &self.0
    }
}

/// Presentation-only "just changed" timers for HUD numbers: a putt bumps the
/// stroke count, a finished hole bumps the round badge. Never read by rules.
#[derive(Clone, Copy, Debug)]
pub(super) struct Bumps {
    strokes: u8,
    strokes_age: f32,
    to_par: i16,
    to_par_age: f32,
}
impl Default for Bumps {
    fn default() -> Self {
        Self {
            strokes: 0,
            strokes_age: f32::INFINITY,
            to_par: 0,
            to_par_age: f32::INFINITY,
        }
    }
}
impl Bumps {
    /// Note this frame's values; strokes bump only when they grow.
    pub fn track(&mut self, strokes: u8, to_par: i16, dt: f32) {
        self.strokes_age += dt;
        self.to_par_age += dt;
        if strokes > self.strokes {
            self.strokes_age = 0.;
        }
        if to_par != self.to_par {
            self.to_par_age = 0.;
        }
        self.strokes = strokes;
        self.to_par = to_par;
    }
    fn scale(age: f32, motion: f32) -> f32 {
        let t = (1. - age / 0.35).max(0.) * motion.min(1.);
        1. + 0.45 * t * t
    }
    pub fn strokes_scale(&self, motion: f32) -> f32 {
        Self::scale(self.strokes_age, motion)
    }
    pub fn badge_scale(&self, motion: f32) -> f32 {
        Self::scale(self.to_par_age, motion)
    }
}

/// Button motion slot for a `Ui` id (10..=25 and practice 100..=111).
pub(super) fn slot(id: u64) -> usize {
    if id >= 100 {
        26 + (id - 100).min(13) as usize
    } else {
        (id as usize).min(25)
    }
}
pub(super) const SLOTS: usize = 40;

fn modern(size: f32, color: Color) -> Type {
    Type::modern(size, color)
}
fn classic(size: f32, color: Color) -> Type {
    Type::classic(size, color)
}
/// Shrink in crisp 8 px steps until `s` fits `max_w`.
fn fit(mut style: Type, s: &str, max_w: f32) -> Type {
    while style.size > 8. && text_width(s, style) > max_w {
        style.size = (style.size - 8.).max(8.);
    }
    style
}
fn surface(r: Rect, radius: f32, theme: &KitTheme, a: f32) {
    shapes::drop_shadow(
        r,
        radius,
        Shadow::soft(Color::new(0., 0., 0., 0.45 * a))
            .offset(vec2(0., 6.))
            .blur(22.),
    );
    shapes::fill_rounded_rect(
        r,
        radius,
        Fill::vertical(r, fade(theme.raised, a), fade(theme.surface, a)),
    );
    shapes::stroke_rounded_rect(r, radius, 1., fade(theme.line, a));
}
fn ink_shadow(a: f32) -> Color {
    Color::new(0., 0., 0., 0.4 * a)
}

impl App {
    pub(super) fn px(&self, r: UiRect) -> Rect {
        kit::ui_rect(self.ui.viewport(), r)
    }
    pub(super) fn motion(&self) -> Motion {
        Motion::from_reduced(self.juice.motion() == 0.)
    }
    /// 0..=1 idle breathing on the presentation clock (still when reduced).
    fn breathe(&self, hz: f32) -> f32 {
        0.5 + 0.5 * (self.ambient * hz).sin()
    }
    /// One button: `Ui` input in the layout pass, kit visuals in the paint pass.
    #[allow(clippy::too_many_arguments)]
    fn kit_button(
        &mut self,
        paint: bool,
        id: u64,
        r: UiRect,
        label: &str,
        kind: ButtonKind,
        enabled: bool,
        look: Appearance,
    ) {
        if !paint {
            self.ui
                .button(id, r, "", enabled, kit::INVISIBLE_BUTTON)
                .expect("bounded golf button");
            return;
        }
        let motion = self.motion();
        let state = kit::ui_button_state(&self.ui, id, enabled);
        let rect = kit::appear(self.px(r), look);
        let m = &mut self.buttons[slot(id)];
        m.update(state, self.frame_dt, motion);
        kit::button(rect, label, kind, m, &self.theme.faded(look.alpha));
    }

    /// Top-level screen for this frame: play HUD, sheet, menus and banner.
    pub(super) fn screen(&mut self, paint: bool, w: f32, h: f32, scene: Scene) {
        let modal = self.modal_open();
        let sheet = self.sheet_open();
        if !modal && !sheet {
            self.hud(paint, w, h, scene);
        }
        if !modal && paint {
            self.banner(w, scene);
        }
        if sheet {
            self.hole_sheet(paint, w, h);
        } else if modal {
            self.modal(paint, w, h);
        }
    }

    /// Score chip, hole title card, onboarding keys, power meter and shot dock.
    fn hud(&mut self, paint: bool, w: f32, h: f32, scene: Scene) {
        let chip = rect(10., 10., chip_width(w), 48.);
        if paint {
            self.paint_chip(chip);
            self.title_card(w, scene);
        } else {
            self.ui
                .button(15, chip, "", true, kit::INVISIBLE_BUTTON)
                .expect("bounded golf chip");
        }
        if self.round.phase != Phase::Aiming {
            return;
        }
        let dock_w = (w - 24.).min(420.);
        let x = (w - dock_w) * 0.5;
        let y = h - 64.;
        let gap = 6.;
        let bw = (dock_w - gap * 4.) / 5.;
        if paint {
            self.paint_power(x, y, dock_w, w);
        }
        for (i, id, text) in [
            (0, 10, "←"),
            (1, 11, "→"),
            (2, 12, "-"),
            (3, 13, "+"),
            (4, 14, "Putt"),
        ] {
            let kind = if id == 14 {
                ButtonKind::Primary
            } else {
                ButtonKind::Secondary
            };
            self.kit_button(
                paint,
                id,
                rect(x + i as f32 * (bw + gap), y, bw, 48.),
                text,
                kind,
                true,
                Appearance::SHOWN,
            );
        }
    }

    /// Hole name over twelve hole pips, then par (flag) and strokes (ball),
    /// and the round's to-par badge. The whole chip opens the scorecard.
    fn paint_chip(&mut self, chip: UiRect) {
        let theme = self.theme;
        let r = self.px(chip);
        let state = kit::ui_button_state(&self.ui, 15, true);
        let motion = self.motion();
        let m = &mut self.buttons[slot(15)];
        m.update(state, self.frame_dt, motion);
        let (hover, focus) = (m.hover(), m.focus());
        surface(r, 12., &theme, 1.);
        if hover > 0.01 {
            shapes::fill_rounded_rect(r, 12., Color::new(1., 1., 1., 0.05 * hover));
        }
        if focus > 0.01 {
            let g = 3.;
            shapes::stroke_rounded_rect(
                Rect::new(r.x - g, r.y - g, r.w + 2. * g, r.h + 2. * g),
                15.,
                2.,
                fade(GOLD, focus),
            );
        }
        let pop = self.juice.motion();
        let mut s = std::mem::take(&mut self.scribe);
        let badge = Rect::new(r.x + r.w - 56., r.y + 7., 48., r.h - 14.);
        let to_par = self.round_to_par();
        let text = if to_par == 0 {
            "E"
        } else {
            s.f(format_args!("{to_par:+}"))
        };
        marks::to_par_badge(badge, to_par, text, 24., 1., self.bumps.badge_scale(pop));
        self.scribe = s;

        let left = r.x + 12.;
        let right = badge.x - 10.;
        let mut name_x = left;
        if self.round.practice {
            Mark::Icon(Icon::Restart, GOLD).draw(vec2(left + 7., r.y + 15.), 14., 1.);
            name_x += 19.;
        }
        let name = self.round.course.name;
        draw_text_styled(
            name,
            name_x,
            r.y + 22.,
            fit(modern(16., PAPER), name, right - name_x).with_shadow(ink_shadow(0.9)),
        );
        // Bottom line: hole pips, par and strokes (clay once over par).
        let cy = r.y + 36.;
        let number = classic(16., PAPER).with_shadow(ink_shadow(1.));
        let (par, strokes) = (self.round.course.par, self.round.strokes);
        let stroke_style = number.with_color(if strokes > par { CLAY } else { PAPER });
        let strokes_text = marks::num(strokes.into());
        let sw = marks::mark_value_width(14., strokes_text, stroke_style);
        let sx = right - sw;
        let par_style = number.with_color(DIM);
        let pw = marks::mark_value_width(14., marks::num(par.into()), par_style);
        let px = sx - 12. - pw;
        marks::mark_value(
            Mark::Icon(Icon::Flag, CLAY),
            px,
            cy,
            14.,
            marks::num(par.into()),
            par_style,
        );
        let bump = self.bumps.strokes_scale(pop);
        Mark::Ball.draw(vec2(sx + 7., cy), 14. * bump, 1.);
        let ss = stroke_style.with_size(16. * bump);
        draw_text_styled(strokes_text, sx + 18., ss.centered_baseline(cy), ss);
        let step = ((px - 10. - left) / 12.).clamp(4., 10.);
        marks::hole_pips(
            left,
            cy,
            step,
            &self.round,
            &self.pars,
            1.,
            self.breathe(2.4),
        );
    }

    /// What this hole holds, in marks: par, water (+1), sand (slow), the
    /// raised green, moving copper and your best here.
    fn hole_marks(&self) -> [Option<(Mark, &'static str, Color)>; 6] {
        let course = &self.round.course;
        let has = |s: Surface| course.patches.iter().any(|p| p.surface == s);
        let moving = course
            .walls
            .iter()
            .any(|w| w.spin != 0. || w.travel != Vec2::ZERO);
        let mut out = [None; 6];
        let mut n = 0;
        let mut push = |m: (Mark, &'static str, Color)| {
            out[n] = Some(m);
            n += 1;
        };
        push((
            Mark::Icon(Icon::Flag, CLAY),
            marks::num(course.par.into()),
            PAPER,
        ));
        if has(Surface::Water) {
            push((Mark::Icon(Icon::Drop, marks::POOL), "+1", CLAY));
        }
        if has(Surface::Sand) {
            push((Mark::Icon(Icon::Hourglass, marks::SAND), "", marks::SAND));
        }
        if has(Surface::Slope) {
            push((Mark::Icon(Icon::Mountain, MINT), "", MINT));
        }
        if moving {
            push((Mark::Icon(Icon::Gear, CLAY), "", CLAY));
        }
        match self.hole_best[self.round.hole_index] {
            Some(1) => push((Mark::Icon(Icon::Star, GOLD), "1", GOLD)),
            Some(b) => push((Mark::Icon(Icon::Trophy, MINT), marks::num(b.into()), MINT)),
            None => {}
        }
        out
    }

    /// Hole title card: the hole number and name, then par, hazards and the
    /// best here as marks. Fades once you putt.
    fn title_card(&mut self, w: f32, scene: Scene) {
        let shown = if self.round.strokes > 0 || self.round.phase == Phase::Rolling {
            0.
        } else {
            ((INTRO - self.intro) / 0.4).clamp(0., 1.)
        };
        if shown <= 0. {
            return;
        }
        let look = Entrance::fade_up(10.).sample(self.intro, 0, self.motion());
        let a = shown * look.alpha;
        let cw = (w - 24.).min(420.);
        let roomy = scene.h >= 300.;
        let height = if roomy { 82. } else { 52. };
        // Settles upward from below, never into the fullscreen menu corner.
        let top = scene.y + 6. + look.offset[1].abs();
        let r = Rect::new((w - cw) * 0.5, top, cw, height);
        surface(r, 14., &self.theme, a);
        shapes::fill_rounded_rect(
            Rect::new(r.x + 16., r.y, r.w - 32., 3.),
            1.5,
            fade(COPPER, a),
        );
        let c = r.x + r.w * 0.5;
        let name = self.round.course.name;
        let number_r = 13.;
        let style = fit(classic(24., fade(PAPER, a)), name, cw - 50. - number_r * 3.)
            .with_shadow(fade(Color::new(0., 0., 0., 0.45), a));
        let name_w = text_width(name, style);
        let number = marks::num(self.round.hole_index as u32 + 1);
        let sign_w = marks::hole_number_width(number_r, number);
        let start = c - (sign_w + 10. + name_w) * 0.5;
        let name_y = r.y + 28.;
        marks::hole_number(vec2(start + sign_w * 0.5, name_y), number_r, number, a);
        draw_text_styled(
            name,
            start + sign_w + 10.,
            style.centered_baseline(name_y),
            style,
        );
        if !roomy {
            return;
        }
        let chips = self.hole_marks();
        let total: f32 = chips
            .iter()
            .flatten()
            .map(|(_, text, _)| marks::big_tag_width(text))
            .sum::<f32>()
            + 10. * (chips.iter().flatten().count().saturating_sub(1)) as f32;
        let mut x = c - total * 0.5;
        for (mark, text, color) in chips.into_iter().flatten() {
            x += marks::big_tag(x, r.y + 62., mark, text, color, a) + 10.;
        }
    }

    /// The power meter (a bolt and a bar that warms with power, with a tick at
    /// this hole's previous putt) and, while learning, key caps with marks.
    fn paint_power(&mut self, x: f32, y: f32, dock_w: f32, w: f32) {
        let theme = self.theme;
        // Onboarding: fades over the first few accepted putts, never for demos.
        // Narrow screens get a drag gesture at the ball instead (draw.rs).
        let learn = 1. - self.putts as f32 / LEARNED as f32;
        if learn > 0. && !self.auto && w >= 560. {
            let a = 0.35 + 0.65 * learn;
            let t = theme.faded(a);
            let groups: [(&[&str], Mark); 3] = [
                (&["←", "→"], Mark::Icon(Icon::Target, GOLD)),
                (&["↑", "↓"], Mark::Icon(Icon::Bolt, GOLD)),
                (&["Space"], Mark::Ball),
            ];
            let icon = 18.;
            let total = groups
                .iter()
                .map(|(keys, _)| caps_width(keys, &t) + 2. + icon)
                .sum::<f32>()
                + 22. * (groups.len() - 1) as f32;
            let pill = Rect::new((w - total) * 0.5 - 14., y - 70., total + 28., 34.);
            shapes::fill_rounded_rect(pill, 17., Color::new(0.03, 0.08, 0.06, 0.72 * a));
            let mut cx = pill.x + 14.;
            for (keys, mark) in groups {
                for key in keys {
                    cx += kit::keycap(cx, pill.y + 5., key, &t).w + kit::SPACE[0];
                }
                mark.draw(vec2(cx + 2. + icon * 0.5, pill.y + 16.5), icon, a);
                cx += 2. + icon + 22.;
            }
        }
        let my = y - 22.;
        let track = Rect::new(x, my - 6., dock_w, 20.);
        shapes::fill_rounded_rect(track, 10., Color::new(0.03, 0.08, 0.06, 0.7));
        let color = power_color(self.power);
        Mark::Icon(Icon::Bolt, color).draw(vec2(x + 15., my + 4.), 14., 1.);
        let bar = Rect::new(x + 30., my, dock_w - 30. - 12., 8.);
        kit::meter(bar, self.power, None, color, &theme);
        if let Some(p) = self.last_power {
            let tx = bar.x + bar.w * p;
            shapes::fill_rounded_rect(
                Rect::new(tx - 1., bar.y - 4., 2., bar.h + 8.),
                1.,
                Color::new(1., 0.97, 0.9, 0.75),
            );
        }
    }

    /// Rare, full-width headline: every score name, then it hands off to the sheet.
    fn banner(&mut self, w: f32, scene: Scene) {
        let handoff = self.headline_handoff(w);
        let Some(b) = &self.juice.banner else { return };
        let a = b.alpha() * if b.score { 1. - handoff } else { 1. };
        if a <= 0. {
            return;
        }
        let scale = self.juice.banner_scale();
        let top = if self.sheet_open() {
            if !Self::banner_room(w) {
                return; // No room above the sheet; the sheet says it all.
            }
            14.
        } else {
            scene.y + scene.h * 0.22
        };
        let band = 64.;
        let full = Rect::new(0., top, w, band);
        shapes::fill_rounded_rect(
            Rect::new(0., top, w * 0.5, band),
            0.,
            Fill::horizontal(
                full,
                Color::new(0.03, 0.08, 0.06, 0.),
                Color::new(0.03, 0.08, 0.06, 0.9 * a),
            ),
        );
        shapes::fill_rounded_rect(
            Rect::new(w * 0.5, top, w * 0.5, band),
            0.,
            Fill::horizontal(
                Rect::new(w * 0.5, top, w * 0.5, band),
                Color::new(0.03, 0.08, 0.06, 0.9 * a),
                Color::new(0.03, 0.08, 0.06, 0.),
            ),
        );
        let area = banner_text_rect(w, top, 0.);
        let cx = area.min[0] + area.size[0] * 0.5;
        let (head, color) = (b.head.as_str(), b.color);
        let mid = top + band * 0.5;
        shapes::glow_disc(vec2(cx, mid), 120. * scale, fade(color, 0.22 * a));
        let size = if head.len() > 12 { 24. } else { 32. };
        let head_style = fit(
            classic(size, fade(color, a))
                .with_outline(fade(DARK, a))
                .with_shadow(Color::new(0., 0., 0., 0.5 * a))
                .with_align(TextAlign::Center),
            head,
            area.size[0],
        );
        // The bounce scales the headline about its center (motion only).
        let head_style = head_style.with_size((head_style.size * scale).max(8.));
        draw_text_styled(head, cx, head_style.centered_baseline(mid), head_style);
    }
    /// Whether a headline fits above the between-hole sheet while keeping a
    /// strip of the re-fitted course visible.
    pub(super) fn banner_room(w: f32) -> bool {
        let (_, sheet_h, _) = Self::sheet_layout(w);
        14. + 34. + 30. + 140. <= screen_height() - 12. - sheet_h
    }
    /// 0..=1: how much of the score headline the sheet shows. Without room
    /// for the top headline the sheet shows it immediately.
    pub(super) fn headline_handoff(&self, w: f32) -> f32 {
        if self.sheet_open() && !Self::banner_room(w) {
            1.
        } else {
            self.juice.handoff()
        }
    }
    pub(super) fn sheet_layout(w: f32) -> (f32, f32, usize) {
        let sw = (w - 24.).min(540.);
        let tw = sw - 32.;
        let cols = if tw >= 400. { 12 } else { 6 };
        let rows = 12 / cols;
        let strip = rows as f32 * 34. + (rows - 1) as f32 * 4.;
        (sw, 86. + strip + 32. + 44. + 14., cols)
    }
    /// Between-hole sheet: the course stays visible above it.
    fn hole_sheet(&mut self, paint: bool, w: f32, h: f32) {
        let (sw, sh, cols) = Self::sheet_layout(w);
        self.sheet_h = sh;
        let Some(o) = self.outcome else { return };
        let handoff = self.headline_handoff(w);
        let slide = ease_out(self.sheet);
        let x = (w - sw) * 0.5;
        let y = (h - 12. - sh) + (1. - slide) * (sh + 16.);
        let tx = x + 16.;
        let tw = sw - 32.;
        if !paint {
            self.ui.modal(900).expect("one modal");
        }
        let rows = 12 / cols;
        let after = y + 86. + rows as f32 * 34. + (rows - 1) as f32 * 4. + 10.;
        let by = after + 22.;
        let next_w = (tw - 8.) * 0.62;
        if paint {
            let tone = o.name.tone();
            let r = self.px(rect(x, y, sw, sh));
            surface(r, 18., &self.theme, 1.);
            shapes::fill_rounded_rect(
                Rect::new(r.x + 18., r.y, r.w - 36., 3.),
                1.5,
                tone_color(tone),
            );
            // Strokes (ball) and par (flag) on the right.
            let side = tx + tw;
            let big = classic(24., if o.strokes > o.par { CLAY } else { PAPER })
                .with_shadow(ink_shadow(1.));
            let strokes = marks::num(o.strokes.into());
            let sw_ = marks::mark_value_width(20., strokes, big);
            let sx = side - sw_;
            marks::mark_value(Mark::Ball, sx, y + 30., 20., strokes, big);
            let par = classic(16., DIM);
            let pw = marks::mark_value_width(14., marks::num(o.par.into()), par);
            let px = sx - 14. - pw;
            marks::mark_value(
                Mark::Icon(Icon::Flag, CLAY),
                px,
                y + 30.,
                14.,
                marks::num(o.par.into()),
                par,
            );
            let head = o.name.headline();
            draw_text_styled(
                &head,
                tx,
                y + 40.,
                fit(
                    classic(24., fade(tone_color(tone), handoff)),
                    &head,
                    px - tx - 10.,
                )
                .with_shadow(fade(Color::new(0., 0., 0., 0.4), handoff)),
            );
            let quip = quips::hole(&o);
            draw_text_styled(
                quip,
                tx,
                y + 60.,
                fit(modern(16., fade(PAPER, 0.85 * handoff)), quip, tw),
            );
            self.sheet_tags(&o, tx, y + 76., tw);
            self.score_strip(tx, y + 86., tw, cols);
            self.round_line(tx, after + 6., 1.);
        }
        let finish = self.round.hole_index == 11;
        self.kit_button(
            paint,
            24,
            rect(tx, by, next_w, 44.),
            if finish { "Finish" } else { "Next hole" },
            ButtonKind::Primary,
            true,
            Appearance::SHOWN,
        );
        self.kit_button(
            paint,
            23,
            rect(tx + next_w + 8., by, tw - next_w - 8., 44.),
            "Card",
            ButtonKind::Secondary,
            true,
            Appearance::SHOWN,
        );
    }
    /// Tag pills for the finished hole: bank or trick shot, the hidden
    /// copper assist, a par-or-better streak and the hole best.
    fn sheet_tags(&mut self, o: &HoleOutcome, x: f32, cy: f32, w: f32) {
        let mut s = std::mem::take(&mut self.scribe);
        let mut at = x;
        let put = |at: &mut f32, mark: Option<Mark>, text: &str, color: Color| {
            if *at + marks::tag_width(mark.is_some(), text) <= x + w {
                *at += marks::tag(*at, cy, mark, text, color, 1.) + 6.;
            }
        };
        match o.style {
            Style::Straight => {}
            Style::Bank(1) => put(&mut at, None, "BANK", GOLD),
            Style::Bank(n) => put(&mut at, None, s.f(format_args!("BANK ×{n}")), GOLD),
            Style::Trick(n) => put(&mut at, None, s.f(format_args!("TRICK ×{n}")), GOLD),
        }
        if o.assist {
            put(&mut at, Some(Mark::Icon(Icon::Gear, GOLD)), "ASSIST", GOLD);
        }
        if o.streak >= 2 {
            put(
                &mut at,
                Some(Mark::Icon(Icon::Flame, CLAY)),
                s.f(format_args!("×{}", o.streak)),
                CLAY,
            );
        }
        if o.name != ScoreName::PickedUp && !self.auto {
            match self.hole_best_before {
                None => {}
                Some(b) if o.strokes < b => put(
                    &mut at,
                    Some(Mark::Icon(Icon::Trophy, MINT)),
                    "NEW BEST",
                    MINT,
                ),
                Some(b) => put(
                    &mut at,
                    Some(Mark::Icon(Icon::Trophy, DIM)),
                    marks::num(b.into()),
                    DIM,
                ),
            }
        }
        self.scribe = s;
    }
    /// The round so far: its to-par badge and total strokes.
    fn round_line(&mut self, x: f32, cy: f32, a: f32) -> f32 {
        let to_par = self.round_to_par();
        let mut s = std::mem::take(&mut self.scribe);
        let mut at = x;
        if self.round.practice {
            Mark::Icon(Icon::Restart, GOLD).draw(vec2(at + 8., cy), 16., a);
            at += 22.;
        }
        let text = if to_par == 0 {
            "E"
        } else {
            s.f(format_args!("{to_par:+}"))
        };
        marks::to_par_badge(Rect::new(at, cy - 10., 44., 20.), to_par, text, 16., a, 1.);
        at += 52.;
        let total = s.f(format_args!("{}", self.round.total()));
        at += marks::mark_value(
            Mark::Ball,
            at,
            cy,
            16.,
            total,
            classic(16., fade(PAPER, a)).with_shadow(ink_shadow(a)),
        );
        self.scribe = s;
        at - x
    }
    /// Twelve compact cells popping in one after another: played holes show
    /// their score in scorecard notation, holes ahead their number.
    fn score_strip(&mut self, x: f32, y: f32, w: f32, cols: usize) {
        let gap = 4.;
        let cw = (w - gap * (cols - 1) as f32) / cols as f32;
        let entrance = Entrance::pop(0.6).with_stagger(0.035);
        let mut s = std::mem::take(&mut self.scribe);
        for i in 0..12 {
            let look = entrance.sample(self.menu_time, i, self.motion());
            let cell = kit::appear(
                Rect::new(
                    x + (i % cols) as f32 * (cw + gap),
                    y + (i / cols) as f32 * (34. + gap),
                    cw,
                    34.,
                ),
                look,
            );
            let a = look.alpha;
            let current = i == self.round.hole_index;
            shapes::fill_rounded_rect(
                cell,
                7.,
                Fill::vertical(
                    cell,
                    fade(Color::from_rgba(46, 82, 66, 255), a),
                    fade(Color::from_rgba(34, 64, 52, 255), a),
                ),
            );
            if current {
                shapes::stroke_rounded_rect(cell, 7., 2., fade(GOLD, a));
                shapes::glow_disc(cell.center(), cw * 0.7, fade(GOLD, 0.12 * a));
            }
            let c = cell.center();
            match self.round.scores[i] {
                Some(v) => {
                    let name = ScoreName::of(v, self.pars[i], self.round.picked_up[i]);
                    let ink = marks::notation(c, 24., name, a);
                    let text = s.f(format_args!("{v}"));
                    let style =
                        classic(if v >= 10 { 8. } else { 16. }, ink).with_align(TextAlign::Center);
                    draw_text_styled(text, c.x, style.centered_baseline(c.y), style);
                }
                None => {
                    let style = modern(8., fade(DIM, 0.8 * a)).with_align(TextAlign::Center);
                    draw_text_styled(
                        marks::num(i as u32 + 1),
                        c.x,
                        style.centered_baseline(c.y),
                        style,
                    );
                }
            }
        }
        self.scribe = s;
    }

    /// Centered menus: welcome (front door), pause, practice and scorecard.
    fn modal(&mut self, paint: bool, w: f32, h: f32) {
        if self.menu == Menu::None && !self.manual_pause && self.round.phase == Phase::Finished {
            self.final_card(paint, w, h);
            return;
        }
        let width = (w - 32.).min(420.);
        let height = if self.menu == Menu::Practice {
            268.
        } else if self.menu == Menu::Score {
            276.
        } else if self.manual_pause {
            264.
        } else {
            212.
        };
        let x = (w - width) * 0.5;
        let y = ((h - height) * 0.5).max(8.);
        let tx = x + 16.;
        let tw = width - 32.;
        let look = Entrance::fade_up(14.).sample(self.menu_time, 0, self.motion());
        if paint {
            let full = Rect::new(0., 0., w, h);
            shapes::fill_rounded_rect(full, 0., fade(self.theme.backdrop, look.alpha));
            let r = kit::appear(self.px(rect(x, y, width, height)), look);
            surface(r, 18., &self.theme, look.alpha);
            shapes::fill_rounded_rect(
                Rect::new(r.x + 18., r.y, r.w - 36., 3.),
                1.5,
                fade(COPPER, look.alpha),
            );
        } else {
            self.ui.modal(900).expect("one modal");
            // Transparent layout anchor in the card's exact rectangle; browser
            // tests use its corner to find the menu (it paints nothing).
            self.ui
                .panel(
                    rect(x, y, width, height),
                    Color::new(DARK.r, DARK.g, DARK.b, 0.),
                )
                .expect("menu anchor");
        }
        let dy = look.offset[1];
        if self.menu == Menu::Practice {
            if paint {
                self.card_title(tx, y + 34. + dy, "Practice", look.alpha);
            }
            let cw = (tw - 18.) / 4.;
            for (i, label) in PRACTICE_LABELS.into_iter().enumerate() {
                let r = rect(
                    tx + (i % 4) as f32 * (cw + 6.),
                    y + 54. + (i / 4) as f32 * 50.,
                    cw,
                    44.,
                );
                self.kit_button(
                    paint,
                    100 + i as u64,
                    r,
                    label,
                    ButtonKind::Secondary,
                    true,
                    shift(look, 0.),
                );
                if paint {
                    self.practice_marks(self.px(r), i, look);
                }
            }
            self.kit_button(
                paint,
                20,
                rect(tx, y + 208., tw, 44.),
                "Back",
                ButtonKind::Primary,
                true,
                shift(look, 0.),
            );
            return;
        }
        if self.menu == Menu::Score {
            self.score_modal(paint, tx, y, tw, look);
            return;
        }
        if paint {
            let a = look.alpha;
            if self.manual_pause {
                self.card_title(tx, y + 36. + dy, "Paused", a);
                self.pause_line(tx, y + 54. + dy, tw, a);
                let quip = quips::pause(self.pause_seed);
                draw_text_styled(
                    quip,
                    tx,
                    y + 80. + dy,
                    fit(modern(8., fade(DIM, a)), quip, tw),
                );
            } else {
                self.front_door(x, y + dy, width, tw, a, w, h);
            }
        }
        let action = if self.manual_pause { "Resume" } else { "Play" };
        self.kit_button(
            paint,
            20,
            rect(tx, y + 96., tw, 44.),
            action,
            ButtonKind::Primary,
            true,
            shift(look, 0.),
        );
        let half = (tw - 8.) * 0.5;
        self.kit_button(
            paint,
            22,
            rect(tx, y + 152., half, 44.),
            "Practice",
            ButtonKind::Secondary,
            true,
            shift(look, 0.),
        );
        self.kit_button(
            paint,
            23,
            rect(tx + half + 8., y + 152., half, 44.),
            "Card",
            ButtonKind::Secondary,
            true,
            shift(look, 0.),
        );
        if self.manual_pause {
            self.kit_button(
                paint,
                25,
                rect(tx, y + 204., half, 44.),
                "Take 12",
                ButtonKind::Ghost,
                matches!(self.round.phase, Phase::Aiming | Phase::Rolling),
                shift(look, 0.),
            );
            self.kit_button(
                paint,
                21,
                rect(tx + half + 8., y + 204., half, 44.),
                "Restart",
                ButtonKind::Danger,
                true,
                shift(look, 0.),
            );
        }
    }

    /// Par as dots under a practice hole's number, plus a medal in the
    /// result color of your best there.
    fn practice_marks(&self, r: Rect, i: usize, look: Appearance) {
        let r = kit::appear(r, shift(look, 0.));
        let a = look.alpha;
        let par = self.pars[i];
        let spread = 7.;
        let x0 = r.x + r.w * 0.5 - spread * (par as f32 - 1.) * 0.5;
        for k in 0..par {
            shapes::fill_circle(
                vec2(x0 + k as f32 * spread, r.y + r.h - 8.),
                1.8,
                fade(DIM, 0.85 * a),
            );
        }
        if let Some(best) = self.hole_best[i] {
            let name = ScoreName::of(best, par, false);
            shapes::fill_circle(
                vec2(r.x + r.w - 9., r.y + 9.),
                3.5,
                fade(tone_color(name.tone()), a),
            );
        }
    }

    /// Pause context: the hole's number and name, par and strokes so far.
    fn pause_line(&mut self, x: f32, cy: f32, w: f32, a: f32) {
        marks::hole_number(
            vec2(x + 9., cy),
            9.,
            marks::num(self.round.hole_index as u32 + 1),
            a,
        );
        let number = classic(16., fade(PAPER, a)).with_shadow(ink_shadow(a));
        let strokes = marks::num(self.round.strokes.into());
        let par = marks::num(self.round.course.par.into());
        let tail = marks::mark_value_width(14., par, number)
            + 12.
            + marks::mark_value_width(14., strokes, number);
        let name = self.round.course.name;
        let style = fit(modern(16., fade(GOLD, a)), name, w - 26. - tail - 12.);
        let nw = draw_text_styled(name, x + 26., style.centered_baseline(cy), style).width;
        let mut at = x + 26. + nw + 12.;
        at += marks::mark_value(
            Mark::Icon(Icon::Flag, CLAY),
            at,
            cy,
            14.,
            par,
            number.with_color(fade(DIM, a)),
        ) + 12.;
        marks::mark_value(Mark::Ball, at, cy, 14., strokes, number);
    }

    fn card_title(&self, x: f32, baseline: f32, title: &str, a: f32) {
        draw_text_styled(
            title,
            x,
            baseline,
            modern(24., fade(PAPER, a)).with_shadow(Color::new(0., 0., 0., 0.35 * a)),
        );
    }

    /// Title screen: logo, the course as pips and par, the best round and
    /// attract-mode motion.
    #[allow(clippy::too_many_arguments)]
    fn front_door(&mut self, x: f32, y: f32, width: f32, tw: f32, a: f32, w: f32, h: f32) {
        let cx = x + width * 0.5;
        let t = self.ambient;
        let bob = (t * 2.).sin() * 2. * self.juice.motion();
        shapes::glow_disc(vec2(cx, y + 30.), width * 0.45, fade(GOLD, 0.12 * a));
        let logo = "COPPER GREENS";
        draw_text_styled(
            logo,
            cx,
            y + 40. + bob,
            fit(
                classic(32., fade(GOLD, a))
                    .with_outline(fade(Color::from_rgba(60, 30, 14, 255), a))
                    .with_shadow(Color::new(0., 0., 0., 0.5 * a))
                    .with_align(TextAlign::Center),
                logo,
                tw,
            ),
        );
        // Twelve holes as pips, then the course par on its flag.
        let mut s = std::mem::take(&mut self.scribe);
        let par_style = modern(16., fade(DIM, a));
        let par_text = s.f(format_args!("{}", card::course_par()));
        let par_w = marks::mark_value_width(16., par_text, par_style);
        let row = 12. * 11. + 14. + par_w;
        let mut at = cx - row * 0.5;
        for i in 0..12 {
            let lit = 0.55 + 0.45 * ((t * 3. - i as f32 * 0.5).sin() * 0.5 + 0.5);
            shapes::fill_circle(vec2(at + 5.5, y + 62.), 3.2, fade(GOLD, lit * a));
            at += 11.;
        }
        at += 14.;
        marks::mark_value(
            Mark::Icon(Icon::Flag, CLAY),
            at,
            y + 62.,
            16.,
            par_text,
            par_style,
        );
        if let Some(b) = self.best {
            let to_par = b as i16 - card::course_par() as i16;
            let style = classic(16., fade(GOLD, a)).with_shadow(ink_shadow(a));
            let best = s.f(format_args!("{b}"));
            let bw = marks::mark_value_width(14., best, style);
            let left = cx - (bw + 8. + 40.) * 0.5;
            marks::mark_value(
                Mark::Icon(Icon::Trophy, GOLD),
                left,
                y + 80.,
                14.,
                best,
                style,
            );
            let text = if to_par == 0 {
                "E"
            } else {
                s.f(format_args!("{to_par:+}"))
            };
            marks::to_par_badge(
                Rect::new(left + bw + 8., y + 72., 40., 16.),
                to_par,
                text,
                8.,
                a,
                1.,
            );
        }
        self.scribe = s;
        // Attract mode above the card: a ball rolls into a glowing cup forever.
        if y > 96. {
            let lane_y = y - 34.;
            let span = (width * 0.7).min(w - 48.);
            let start = cx - span * 0.5;
            let cup = vec2(start + span, lane_y);
            let cycle = 2.6;
            let k = if self.juice.motion() == 0. {
                0.6
            } else {
                (t % cycle) / cycle
            };
            let roll = ease_out((k / 0.75).min(1.));
            shapes::polyline(
                &[vec2(start, lane_y + 7.), vec2(cup.x + 14., lane_y + 7.)],
                Stroke::new(2., Color::new(0.4, 0.62, 0.42, 0.5 * a)),
            );
            shapes::glow_disc(cup, 26., fade(GOLD, 0.3 * a));
            shapes::fill_circle(
                cup + vec2(0., 4.),
                9.,
                fade(Color::from_rgba(10, 22, 18, 255), a),
            );
            shapes::polyline(
                &[cup + vec2(0., 2.), cup - vec2(0., 26.)],
                Stroke::new(2., fade(Color::from_rgba(240, 236, 226, 255), a)),
            );
            draw_triangle(
                cup - vec2(0., 26.),
                cup + vec2(15., -21. + bob),
                cup - vec2(0., 16.),
                fade(COPPER, a),
            );
            if k < 0.78 {
                let ball = vec2(start + roll * (span - 2.), lane_y);
                shapes::fill_circle(ball + vec2(2., 5.), 6., Color::new(0., 0., 0., 0.3 * a));
                shapes::fill_circle(ball, 6., fade(Color::new(1., 1., 1., 1.), a));
            } else {
                let pop = (k - 0.78) / 0.22;
                shapes::glow_ring(cup, 10. + 30. * pop, 3., fade(GOLD, (1. - pop) * a));
            }
        }
        // Clear "press to start" below the card when there is room.
        if h - (y + 212.) > 52. {
            let prompt_theme = KitTheme {
                ink: GOLD,
                ..self.theme.faded(a)
            };
            let pw = prompt_width(&["Enter"], "Tee off", &prompt_theme);
            kit::key_prompt(
                cx - pw * 0.5,
                y + 226.,
                &["Enter"],
                "Tee off",
                &prompt_theme,
            );
        }
    }

    /// Scorecard: each hole's number, par and result in golf notation, then
    /// the round's total and to-par.
    fn score_modal(&mut self, paint: bool, tx: f32, y: f32, tw: f32, look: Appearance) {
        if paint {
            let a = look.alpha;
            let dy = look.offset[1];
            self.card_title(tx, y + 36. + dy, "Scorecard", a);
            if self.round.practice {
                Mark::Icon(Icon::Restart, GOLD).draw(vec2(tx + tw - 8., y + 28. + dy), 16., a);
            }
            let cw = (tw - 12.) * 0.5;
            let mut s = std::mem::take(&mut self.scribe);
            for i in 0..12 {
                let row = Entrance::fade_up(6.).with_stagger(0.02).sample(
                    self.menu_time,
                    i,
                    self.motion(),
                );
                let ra = a * row.alpha;
                let rx = tx + (i / 6) as f32 * (cw + 12.);
                let cy = y + 61. + (i % 6) as f32 * 22. + dy + row.offset[1];
                let par = self.pars[i];
                if i == self.round.hole_index && self.round.phase != Phase::Finished {
                    shapes::fill_rounded_rect(
                        Rect::new(rx - 6., cy - 10., cw + 12., 20.),
                        6.,
                        fade(Color::from_rgba(52, 92, 72, 255), ra),
                    );
                }
                marks::hole_number(vec2(rx + 8., cy), 8., marks::num(i as u32 + 1), ra);
                marks::mark_value(
                    Mark::Icon(Icon::Flag, CLAY),
                    rx + 26.,
                    cy,
                    12.,
                    marks::num(par.into()),
                    modern(8., fade(DIM, ra)),
                );
                let sc = vec2(rx + cw - 16., cy);
                match self.round.scores[i] {
                    Some(v) => {
                        let name = ScoreName::of(v, par, self.round.picked_up[i]);
                        let ink = marks::notation(sc, 19., name, ra);
                        let style = classic(if v >= 10 { 8. } else { 16. }, ink)
                            .with_align(TextAlign::Center);
                        draw_text_styled(
                            s.f(format_args!("{v}")),
                            sc.x,
                            style.centered_baseline(sc.y),
                            style,
                        );
                    }
                    None => {
                        shapes::fill_rounded_rect(
                            Rect::new(sc.x - 5., sc.y - 1., 10., 2.),
                            1.,
                            fade(DIM, 0.6 * ra),
                        );
                    }
                }
            }
            self.scribe = s;
            self.round_line(tx, y + 196. + dy, a);
        }
        let half = (tw - 8.) * 0.5;
        self.kit_button(
            paint,
            20,
            rect(tx, y + 216., half, 44.),
            "Back",
            ButtonKind::Primary,
            true,
            shift(look, 0.),
        );
        self.kit_button(
            paint,
            25,
            rect(tx + half + 8., y + 216., half, 44.),
            "Take 12",
            ButtonKind::Ghost,
            self.round.phase == Phase::Aiming,
            shift(look, 0.),
        );
    }

    /// Results: rank title, the total and to-par, personal best, the next
    /// title to chase and round stats as marks.
    fn final_card(&mut self, paint: bool, w: f32, h: f32) {
        let width = (w - 32.).min(460.);
        let stats = h >= 520.;
        let assists = self.stats.assist_count();
        let height = final_card_height(h, assists > 0);
        let grid = height - (156. + 44. + 8. + 44. + 16.);
        let x = (w - width) * 0.5;
        let y = ((h - height) * 0.5).max(8.);
        let tx = x + 16.;
        let tw = width - 32.;
        let look = Entrance::fade_up(16.).sample(self.menu_time, 0, self.motion());
        let practice = self.round.practice;
        if !paint {
            self.ui.modal(900).expect("one modal");
            self.ui
                .panel(
                    rect(x, y, width, height),
                    Color::new(DARK.r, DARK.g, DARK.b, 0.),
                )
                .expect("results anchor");
        } else {
            let a = look.alpha;
            let full = Rect::new(0., 0., w, h);
            shapes::fill_rounded_rect(full, 0., fade(self.theme.backdrop, a * 0.8));
            let r = kit::appear(self.px(rect(x, y, width, height)), look);
            let to_par = self.round_to_par();
            let accent = if practice {
                PAPER
            } else if to_par < 0 {
                GOLD
            } else {
                COPPER
            };
            surface(r, 20., &self.theme, a);
            shapes::fill_rounded_rect(
                Rect::new(r.x + 20., r.y, r.w - 40., 3.),
                1.5,
                fade(accent, a),
            );
            let cx = x + width * 0.5;
            let dy = look.offset[1];
            let rank = card::rank(to_par);
            let lines = Entrance::fade_up(10.).with_stagger(0.07);
            let menu_time = self.menu_time;
            let motion = self.motion();
            let la = |i: usize| a * lines.sample(menu_time, i, motion).alpha;
            let pulse = if self.juice.motion() > 0. {
                self.breathe(2.5)
            } else {
                0.
            };
            let crest = if practice {
                Mark::Icon(Icon::Restart, PAPER)
            } else {
                Mark::Icon(Icon::Trophy, GOLD)
            };
            crest.draw(vec2(cx, y + 22. + dy), 24., la(0));
            let title = if practice {
                "PRACTICE MAKES PUTTS"
            } else {
                rank.title
            };
            shapes::glow_disc(
                vec2(cx, y + 48. + dy),
                width * 0.4,
                fade(GOLD, (0.1 + 0.06 * pulse) * la(1)),
            );
            draw_text_styled(
                title,
                cx,
                y + 60. + dy,
                fit(
                    classic(24., fade(if practice { PAPER } else { GOLD }, la(1)))
                        .with_outline(fade(Color::from_rgba(60, 30, 14, 255), la(1)))
                        .with_align(TextAlign::Center),
                    title,
                    tw,
                ),
            );
            if !practice {
                draw_text_styled(
                    rank.quip,
                    cx,
                    y + 78. + dy,
                    fit(
                        modern(8., fade(PAPER, la(2))).with_align(TextAlign::Center),
                        rank.quip,
                        tw,
                    ),
                );
            }
            // Total strokes rolling up beside the to-par badge.
            let mut s = std::mem::take(&mut self.scribe);
            let mut d = std::mem::take(&mut self.scribe2);
            let big = classic(24., fade(PAPER, la(3))).with_shadow(ink_shadow(la(3)));
            let shown = s.f(format_args!("{}", self.roll.shown()));
            let total_w = marks::mark_value_width(22., shown, big);
            let left = cx - (total_w + 12. + 52.) * 0.5;
            marks::mark_value(Mark::Ball, left, y + 104. + dy, 22., shown, big);
            let text = if to_par == 0 {
                "E"
            } else {
                s.f(format_args!("{to_par:+}"))
            };
            marks::to_par_badge(
                Rect::new(left + total_w + 12., y + 92. + dy, 52., 24.),
                to_par,
                text,
                16.,
                la(3),
                1.,
            );
            self.best_line(cx, y + 128. + dy, la(4), &mut s, &mut d);
            self.goal_line(cx, y + 148. + dy, to_par, la(5), &mut s);
            if stats {
                self.stats_grid(tx, y + 162. + dy, tw, a, &mut s);
            }
            self.scribe = s;
            self.scribe2 = d;
        }
        let by = y + 156. + grid;
        self.kit_button(
            paint,
            21,
            rect(tx, by, tw, 44.),
            "Play again",
            ButtonKind::Primary,
            true,
            shift(look, 0.),
        );
        let half = (tw - 8.) * 0.5;
        self.kit_button(
            paint,
            22,
            rect(tx, by + 52., half, 44.),
            "Practice",
            ButtonKind::Secondary,
            true,
            shift(look, 0.),
        );
        self.kit_button(
            paint,
            23,
            rect(tx + half + 8., by + 52., half, 44.),
            "Card",
            ButtonKind::Secondary,
            true,
            shift(look, 0.),
        );
    }

    /// Personal best on the results card: a trophy with the comparison and
    /// the stroke difference (`d` formats the second number).
    fn best_line(&self, cx: f32, cy: f32, a: f32, s: &mut Scribe, d: &mut Scribe) {
        let shown_only = self.round.practice || self.auto && !self.capture.player_card;
        let (mark, head, delta, color): (Mark, &str, Option<&str>, Color) = if shown_only {
            match self.best {
                Some(b) => (
                    Mark::Icon(Icon::Trophy, DIM),
                    s.f(format_args!("{b}")),
                    None,
                    DIM,
                ),
                None => return,
            }
        } else {
            match self.round_best {
                Some(b @ (Best::First | Best::Tied)) => {
                    (Mark::Icon(Icon::Trophy, MINT), b.line(), None, MINT)
                }
                Some(b @ Best::New(n)) => (
                    Mark::Icon(Icon::Trophy, GOLD),
                    b.line(),
                    Some(d.f(format_args!("-{n}"))),
                    GOLD,
                ),
                Some(b @ Best::Short(n)) => (
                    Mark::Icon(Icon::Trophy, DIM),
                    match self.best {
                        Some(previous) => s.f(format_args!("{previous}")),
                        None => b.line(),
                    },
                    Some(d.f(format_args!("+{n}"))),
                    CLAY,
                ),
                None => return,
            }
        };
        let w =
            marks::tag_width(true, head) + delta.map_or(0., |d| 6. + marks::tag_width(false, d));
        let mut at = cx - w * 0.5;
        at += marks::tag(at, cy, Some(mark), head, color, a) + 6.;
        if let Some(d) = delta {
            marks::tag(at, cy, None, d, color, a);
        }
    }

    /// The next title to chase and the strokes it needs, or the crown.
    fn goal_line(&self, cx: f32, cy: f32, to_par: i16, a: f32, s: &mut Scribe) {
        if self.round.practice {
            return;
        }
        match card::next_rank(to_par) {
            Some((better, strokes)) => {
                let need = s.f(format_args!("-{strokes}"));
                let w = marks::tag_width(true, better.title) + 6. + marks::tag_width(true, need);
                let mut at = cx - w * 0.5;
                at += marks::tag(
                    at,
                    cy,
                    Some(Mark::Icon(Icon::ArrowUp, GOLD)),
                    better.title,
                    GOLD,
                    a,
                ) + 6.;
                marks::tag(at, cy, Some(Mark::Ball), need, MINT, a);
            }
            None => {
                let w = marks::tag_width(true, "TOP TITLE");
                marks::tag(
                    cx - w * 0.5,
                    cy,
                    Some(Mark::Icon(Icon::Crown, GOLD)),
                    "TOP TITLE",
                    GOLD,
                    a,
                );
            }
        }
    }

    /// Round stats as mark + number tiles, plus the secret assists as pips.
    fn stats_grid(&self, x: f32, y: f32, w: f32, a: f32, s: &mut Scribe) {
        let st = self.stats;
        let rows = 2.;
        let area = Rect::new(
            x,
            y - 4.,
            w,
            rows * 26. + 8. + if st.assists != 0 { 20. } else { 0. },
        );
        shapes::fill_rounded_rect(area, 10., Color::new(0., 0., 0., 0.2 * a));
        let cells: [(StatMark, u8); 8] = [
            (StatMark::Mark(Mark::Icon(Icon::Star, GOLD)), st.aces),
            (StatMark::Score(ScoreName::Birdie), st.under),
            (StatMark::Mark(Mark::Icon(Icon::Flag, CLAY)), st.pars),
            (StatMark::Score(ScoreName::Bogey), st.over),
            (StatMark::Bank, st.bank_shots + st.trick_shots),
            (
                StatMark::Mark(Mark::Icon(Icon::Drop, marks::POOL)),
                st.splashes,
            ),
            (
                StatMark::Mark(Mark::Icon(Icon::Flame, CLAY)),
                st.best_streak,
            ),
            (StatMark::Lip, st.lip_outs),
        ];
        let col = w / 4.;
        let lines = Entrance::fade_up(10.).with_stagger(0.07);
        for (i, (mark, value)) in cells.into_iter().enumerate() {
            let ca = a * lines.sample(self.menu_time, 6 + i, self.motion()).alpha;
            let ca = if value == 0 { ca * 0.45 } else { ca };
            let c = vec2(
                x + (i % 4) as f32 * col + col * 0.5,
                y + 9. + (i / 4) as f32 * 26.,
            );
            let icon = c - vec2(10., 0.);
            match mark {
                StatMark::Mark(m) => m.draw(icon, 16., ca),
                StatMark::Score(name) => {
                    marks::notation(icon, 14., name, ca);
                }
                StatMark::Bank => marks::bank(icon, 14., fade(GOLD, ca)),
                StatMark::Lip => marks::lip(icon, 16., ca),
            }
            let style = classic(16., fade(PAPER, ca));
            draw_text_styled(
                s.f(format_args!("{value}")),
                c.x + 2.,
                style.centered_baseline(c.y),
                style,
            );
        }
        if st.assists != 0 {
            let total = card::assist_holes();
            let cy = y + 9. + rows * 26.;
            let span = 16. + 8. + total as f32 * 12.;
            let mut at = x + w * 0.5 - span * 0.5;
            Mark::Icon(Icon::Gear, GOLD).draw(vec2(at + 8., cy), 16., a);
            at += 24.;
            for k in 0..total {
                let c = vec2(at + 6., cy);
                if k < self.stats.assist_count() {
                    shapes::glow_disc(c, 9., fade(GOLD, 0.25 * a));
                    shapes::fill_circle(c, 4., fade(GOLD, a));
                } else {
                    shapes::stroke_circle(c, 3.6, 1.2, fade(DIM, a));
                }
                at += 12.;
            }
        }
    }
}

/// What a results stat tile shows before its number.
#[derive(Clone, Copy)]
enum StatMark {
    Mark(Mark),
    Score(ScoreName),
    Bank,
    Lip,
}

const PRACTICE_LABELS: [&str; 12] = [
    "01", "02", "03", "04", "05", "06", "07", "08", "09", "10", "11", "12",
];

/// Entrance appearance without scale, so button hit areas match their paint.
fn shift(look: Appearance, extra: f32) -> Appearance {
    Appearance {
        alpha: look.alpha,
        offset: [look.offset[0], look.offset[1] + extra],
        scale: 1.,
    }
}

/// Width of `kit::keycap`s for `keys`, each followed by the kit's small gap.
fn caps_width(keys: &[&str], theme: &KitTheme) -> f32 {
    let style = Type::modern(theme.body, theme.ink);
    let h = theme.body * 1.5 + 2.;
    keys.iter()
        .map(|k| (text_width(k, style) + kit::SPACE[2] * 2.).max(h - 3.) + kit::SPACE[0])
        .sum()
}

/// Width `kit::key_prompt` will use, for centering before drawing.
fn prompt_width(keys: &[&str], action: &str, theme: &KitTheme) -> f32 {
    caps_width(keys, theme)
        + kit::SPACE[0]
        + text_width(action, Type::modern(theme.body, theme.muted))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hud_bumps_follow_new_strokes_and_round_changes_only() {
        let mut b = Bumps::default();
        assert_eq!((b.strokes_scale(1.), b.badge_scale(1.)), (1., 1.));
        b.track(1, 0, 0.016);
        assert!(b.strokes_scale(1.) > 1.3, "a putt bumps the stroke count");
        assert_eq!(b.strokes_scale(0.), 1., "reduced motion holds still");
        assert_eq!(b.badge_scale(1.), 1.);
        b.track(1, 0, 1.);
        assert_eq!(b.strokes_scale(1.), 1.);
        // A new hole resets strokes (no bump); the finished hole moved to-par.
        b.track(0, -1, 0.016);
        assert_eq!(b.strokes_scale(1.), 1.);
        assert!(b.badge_scale(1.) > 1.3);
    }

    #[test]
    fn key_cap_widths_match_the_kit() {
        let theme = golf_theme();
        let one = caps_width(&["Space"], &theme);
        assert!(one > 0. && caps_width(&["←", "→"], &theme) > caps_width(&["←"], &theme));
        assert!(prompt_width(&["Enter"], "Tee off", &theme) > caps_width(&["Enter"], &theme));
    }
}
