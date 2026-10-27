//! Screen-space UI built with the modern kit: score chip, hole title card,
//! power dock, headline banner, the between-hole sheet, menus and results.
//!
//! Every function runs twice per frame. The layout pass (`paint == false`,
//! during `prepare`) submits only `Ui` input: invisible buttons, the modal
//! blocker and a transparent anchor rectangle for each centered card. The
//! paint pass (`paint == true`, during `draw`, after the post-processing HUD
//! boundary) draws the same rectangles with the kit. One function owns both,
//! so layout and visuals cannot drift apart.
use super::*;
use ferravex_play::tween::{Appearance, Entrance};
use ferravex_render::{
    post,
    shapes::{self, Fill, Shadow, Stroke},
    text::{FontFace, TextAlign, TextStyle as Type, draw_text_styled, text_width, wrap_text},
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
fn faded(style: Type, a: f32) -> Type {
    let mut s = style;
    s.color = fade(s.color, a);
    s.shadow = s.shadow.map(|c| fade(c, a));
    s.outline = s.outline.map(|c| fade(c, a));
    s
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
fn tone_label(tone: Tone) -> Color {
    tone_color(tone)
}

impl App {
    pub(super) fn px(&self, r: UiRect) -> Rect {
        kit::ui_rect(self.ui.viewport(), r)
    }
    pub(super) fn motion(&self) -> Motion {
        Motion::from_reduced(self.juice.motion() == 0.)
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

    /// Score chip, hole title card, onboarding hint, power meter and shot dock.
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
        let badge = Rect::new(r.x + r.w - 62., r.y + 6., 56., r.h - 12.);
        shapes::fill_rounded_rect(badge, 8., Color::new(0., 0., 0., 0.24));
        let c = badge.center();
        draw_text_styled(
            "Round",
            c.x,
            badge.y + 12.,
            modern(8., DIM).with_align(TextAlign::Center),
        );
        let to_par = self.round_to_par();
        let color = match to_par {
            i16::MIN..=-1 => MINT,
            0 => PAPER,
            _ => CLAY,
        };
        let mut s = std::mem::take(&mut self.scribe);
        let text = if to_par == 0 {
            "E"
        } else {
            s.f(format_args!("{to_par:+}"))
        };
        draw_text_styled(
            text,
            c.x,
            badge.y + badge.h - 6.,
            classic(16., color)
                .with_align(TextAlign::Center)
                .with_shadow(Color::new(0., 0., 0., 0.4)),
        );
        let text_w = badge.x - r.x - 20.;
        let name = self.round.course.name;
        draw_text_styled(
            name,
            r.x + 12.,
            r.y + 24.,
            fit(modern(16., PAPER), name, text_w).with_shadow(Color::new(0., 0., 0., 0.35)),
        );
        let strokes = self.round.strokes;
        let caption = s.f(format_args!(
            "{}Hole {}/12 · Par {} · {} {}",
            if self.round.practice {
                "Practice · "
            } else {
                ""
            },
            self.round.hole_index + 1,
            self.round.course.par,
            strokes,
            if strokes == 1 { "stroke" } else { "strokes" }
        ));
        draw_text_styled(
            caption,
            r.x + 12.,
            r.y + 39.,
            fit(modern(8., GOLD), caption, text_w),
        );
        self.scribe = s;
    }

    /// Hole title card: name, par, the hole's read and a personal best hook.
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
        let cw = (w - 24.).min(460.);
        let cx = (w - cw) * 0.5;
        let roomy = scene.h >= 300.;
        let hint = self.round.course.hint;
        let hint_style = modern(8., PAPER).with_align(TextAlign::Center);
        let lines = if roomy {
            wrap_text(hint, cw - 32., hint_style).count().min(2)
        } else {
            0
        };
        let best = self.hole_best[self.round.hole_index];
        let height = 54. + lines as f32 * 12. + if best.is_some() { 14. } else { 0. } + 6.;
        // Settles upward from below, never into the fullscreen menu corner.
        let top = scene.y + 6. + look.offset[1].abs();
        let r = Rect::new(cx, top, cw, height);
        surface(r, 14., &self.theme, a);
        shapes::fill_rounded_rect(
            Rect::new(r.x + 16., r.y, r.w - 32., 3.),
            1.5,
            fade(COPPER, a),
        );
        let c = r.x + r.w * 0.5;
        let mut s = std::mem::take(&mut self.scribe);
        draw_text_styled(
            s.f(format_args!(
                "Hole {} of 12 · Par {}",
                self.round.hole_index + 1,
                self.round.course.par
            )),
            c,
            r.y + 18.,
            modern(8., fade(GOLD, a)).with_align(TextAlign::Center),
        );
        let name = self.round.course.name;
        draw_text_styled(
            name,
            c,
            r.y + 44.,
            fit(classic(24., fade(PAPER, a)), name, cw - 32.)
                .with_align(TextAlign::Center)
                .with_shadow(fade(Color::new(0., 0., 0., 0.45), a)),
        );
        let mut y = r.y + 60.;
        for line in wrap_text(hint, cw - 32., hint_style).take(lines) {
            draw_text_styled(line, c, y, faded(hint_style, 0.9 * a));
            y += 12.;
        }
        if let Some(best) = best {
            let text = if best == 1 {
                s.f(format_args!("Your best here: {best} · aced it ★"))
            } else {
                s.f(format_args!("Your best here: {best}"))
            };
            draw_text_styled(
                text,
                c,
                y + 2.,
                modern(8., fade(MINT, a)).with_align(TextAlign::Center),
            );
        }
        self.scribe = s;
    }

    fn paint_power(&mut self, x: f32, y: f32, dock_w: f32, w: f32) {
        let theme = self.theme;
        // Onboarding: fades over the first few accepted putts, never for demos.
        let learn = 1. - self.putts as f32 / LEARNED as f32;
        if learn > 0. && !self.auto {
            let a = 0.35 + 0.65 * learn;
            let t = theme.faded(a);
            if w >= 560. {
                let prompts: [(&[&str], &str); 3] = [
                    (&["←", "→"], "Aim"),
                    (&["↑", "↓"], "Power"),
                    (&["Space"], "Putt"),
                ];
                let total: f32 = prompts
                    .iter()
                    .map(|(keys, action)| prompt_width(keys, action, &t) + 18.)
                    .sum::<f32>()
                    - 18.;
                let pill = Rect::new((w - total) * 0.5 - 14., y - 70., total + 28., 34.);
                shapes::fill_rounded_rect(pill, 17., Color::new(0.03, 0.08, 0.06, 0.72 * a));
                let mut cx = pill.x + 14.;
                for (keys, action) in prompts {
                    cx += kit::key_prompt(cx, pill.y + 5., keys, action, &t) + 18.;
                }
            } else {
                let style = modern(8., fade(PAPER, a)).with_align(TextAlign::Center);
                let text = "Drag toward the target · release to putt";
                let tw = text_width(text, style);
                let pill = Rect::new((w - tw) * 0.5 - 14., y - 56., tw + 28., 22.);
                shapes::fill_rounded_rect(pill, 11., Color::new(0.03, 0.08, 0.06, 0.72 * a));
                draw_text_styled(text, w * 0.5, pill.y + 15., style);
            }
        }
        let my = y - 22.;
        let track = Rect::new(x, my - 6., dock_w, 20.);
        shapes::fill_rounded_rect(track, 10., Color::new(0.03, 0.08, 0.06, 0.7));
        draw_text_styled("Power", x + 10., my + 8., modern(8., DIM));
        let bar = Rect::new(x + 52., my, dock_w - 52. - 48., 8.);
        let color = power_color(self.power);
        kit::meter(bar, self.power, None, color, &theme);
        let mut s = std::mem::take(&mut self.scribe);
        draw_text_styled(
            s.f(format_args!("{:.0}%", self.power * 100.)),
            x + dock_w - 10.,
            my + 8.,
            modern(8., color).with_align(TextAlign::Right),
        );
        self.scribe = s;
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
        let band = 78.;
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
        let (head, sub, color) = (b.head.as_str(), b.sub.as_str(), b.color);
        shapes::glow_disc(vec2(cx, top + 30.), 120. * scale, fade(color, 0.22 * a));
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
        draw_text_styled(head, cx, top + 26. + head_style.size * 0.5, head_style);
        draw_text_styled(
            sub,
            cx,
            top + band - 10.,
            fit(
                modern(16., fade(PAPER, a)).with_align(TextAlign::Center),
                sub,
                area.size[0],
            ),
        );
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
                tone_label(tone),
            );
            let mut s = std::mem::take(&mut self.scribe);
            let head = o.name.headline();
            let head_w = tw * 0.6;
            draw_text_styled(
                &head,
                tx,
                y + 38.,
                fit(classic(24., fade(tone_label(tone), handoff)), &head, head_w)
                    .with_shadow(fade(Color::new(0., 0., 0., 0.4), handoff)),
            );
            let side = tx + tw;
            draw_text_styled(
                s.f(format_args!("Hole {} · Par {}", o.hole + 1, o.par)),
                side,
                y + 22.,
                modern(8., DIM).with_align(TextAlign::Right),
            );
            draw_text_styled(
                s.f(format_args!(
                    "{} {}",
                    o.strokes,
                    if o.strokes == 1 { "stroke" } else { "strokes" }
                )),
                side,
                y + 40.,
                modern(16., PAPER).with_align(TextAlign::Right),
            );
            let quip = quips::hole(&o);
            draw_text_styled(
                quip,
                tx,
                y + 62.,
                fit(modern(16., fade(PAPER, handoff)), quip, tw),
            );
            // Tags: style, the hidden assist, a streak and the hole-best hook.
            let mut tags = std::mem::take(&mut s.0);
            tags.clear();
            let mut push = |t: std::fmt::Arguments<'_>| {
                if !tags.is_empty() {
                    tags.push_str("  ·  ");
                }
                let _ = tags.write_fmt(t);
            };
            match o.style {
                Style::Straight => {}
                Style::Bank(1) => push(format_args!("Bank shot")),
                Style::Bank(n) => push(format_args!("Bank shot ×{n}")),
                Style::Trick(n) => push(format_args!("Trick shot ×{n}")),
            }
            if o.assist {
                push(format_args!("Copper assist ★"));
            }
            if o.streak >= 2 {
                push(format_args!("Streak ×{}", o.streak));
            }
            if o.name != ScoreName::PickedUp && !self.auto {
                match self.hole_best_before {
                    None => {}
                    Some(b) if o.strokes < b => push(format_args!("New hole best!")),
                    Some(b) => push(format_args!("Hole best {b}")),
                }
            }
            draw_text_styled(&tags, tx, y + 78., fit(modern(8., GOLD), &tags, tw));
            s.0 = tags;
            self.score_strip(tx, y + 86., tw, cols);
            let to_par = self.round_to_par();
            let thru = self.round.scores.iter().flatten().count();
            let summary = if self.round.practice {
                s.f(format_args!(
                    "Practice · total {} · thru {thru}",
                    self.round.total()
                ))
            } else if to_par == 0 {
                s.f(format_args!(
                    "Round E · total {} · thru {thru}",
                    self.round.total()
                ))
            } else {
                s.f(format_args!(
                    "Round {to_par:+} · total {} · thru {thru}",
                    self.round.total()
                ))
            };
            draw_text_styled(summary, tx, after + 10., modern(8., GOLD));
            self.scribe = s;
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
    /// Twelve compact cells, colored by result, popping in one after another.
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
            let par = course::hole(i).par;
            let (text, color) = match self.round.scores[i] {
                Some(v) => {
                    let name = ScoreName::of(v, par, self.round.picked_up[i]);
                    (
                        if self.round.picked_up[i] {
                            s.f(format_args!("{v}*"))
                        } else {
                            s.f(format_args!("{v}"))
                        },
                        tone_color(name.tone()),
                    )
                }
                None => ("·", DIM),
            };
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
            draw_text_styled(
                text,
                c.x,
                cell.y + 28.,
                classic(16., fade(color, a)).with_align(TextAlign::Center),
            );
            draw_text_styled(
                s.f(format_args!("{}", i + 1)),
                c.x,
                cell.y + 11.,
                modern(8., fade(DIM, a)).with_align(TextAlign::Center),
            );
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
                self.card_title(
                    tx,
                    y + 34. + dy,
                    "Practice",
                    "Pick any green",
                    tw,
                    look.alpha,
                );
            }
            let cw = (tw - 18.) / 4.;
            for (i, &text) in PRACTICE_LABELS.iter().enumerate() {
                let cell = Entrance::fade_up(8.).with_stagger(0.02).sample(
                    self.menu_time,
                    i + 1,
                    self.motion(),
                );
                let _ = cell;
                self.kit_button(
                    paint,
                    100 + i as u64,
                    rect(
                        tx + (i % 4) as f32 * (cw + 6.),
                        y + 54. + (i / 4) as f32 * 50.,
                        cw,
                        44.,
                    ),
                    text,
                    ButtonKind::Secondary,
                    true,
                    shift(look, 0.),
                );
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
                self.card_title(tx, y + 36. + dy, "Paused", "", tw, a);
                let mut s = std::mem::take(&mut self.scribe);
                let info = s.f(format_args!(
                    "Hole {} · {} · {} {}",
                    self.round.hole_index + 1,
                    self.round.course.name,
                    self.round.strokes,
                    if self.round.strokes == 1 {
                        "stroke"
                    } else {
                        "strokes"
                    }
                ));
                draw_text_styled(
                    info,
                    tx,
                    y + 56. + dy,
                    fit(modern(8., fade(GOLD, a)), info, tw),
                );
                self.scribe = s;
                let quip = quips::pause(self.pause_seed);
                draw_text_styled(
                    quip,
                    tx,
                    y + 76. + dy,
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

    fn card_title(&self, x: f32, baseline: f32, title: &str, aside: &str, tw: f32, a: f32) {
        draw_text_styled(
            title,
            x,
            baseline,
            modern(24., fade(PAPER, a)).with_shadow(Color::new(0., 0., 0., 0.35 * a)),
        );
        if !aside.is_empty() {
            draw_text_styled(
                aside,
                x + tw,
                baseline - 2.,
                modern(8., fade(DIM, a)).with_align(TextAlign::Right),
            );
        }
    }

    /// Title screen: logo, course facts, best round and attract-mode motion.
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
        draw_text_styled(
            "12 holes · par 40 · 2 views, 1 course",
            cx,
            y + 62.,
            fit(
                modern(8., fade(DIM, a)).with_align(TextAlign::Center),
                "12 holes · par 40 · 2 views, 1 course",
                tw,
            ),
        );
        let mut s = std::mem::take(&mut self.scribe);
        let best = match self.best {
            Some(b) => {
                let to_par = b as i16 - card::course_par() as i16;
                if to_par == 0 {
                    s.f(format_args!("Best round {b} (E)"))
                } else {
                    s.f(format_args!("Best round {b} ({to_par:+})"))
                }
            }
            None => "No best round yet: set one!",
        };
        draw_text_styled(
            best,
            cx,
            y + 80.,
            modern(8., fade(MINT, a)).with_align(TextAlign::Center),
        );
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

    fn score_modal(&mut self, paint: bool, tx: f32, y: f32, tw: f32, look: Appearance) {
        if paint {
            let a = look.alpha;
            let dy = look.offset[1];
            self.card_title(
                tx,
                y + 36. + dy,
                "Scorecard",
                if self.round.practice {
                    "Practice"
                } else {
                    "Full round"
                },
                tw,
                a,
            );
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
                let ry = y + 50. + (i % 6) as f32 * 21. + dy + row.offset[1];
                let par = course::hole(i).par;
                if i == self.round.hole_index && self.round.phase != Phase::Finished {
                    shapes::fill_rounded_rect(
                        Rect::new(rx - 6., ry - 2., cw + 12., 20.),
                        6.,
                        fade(Color::from_rgba(52, 92, 72, 255), ra),
                    );
                }
                draw_text_styled(
                    s.f(format_args!("{:02}", i + 1)),
                    rx,
                    ry + 13.,
                    modern(8., fade(DIM, ra)),
                );
                let (score, color) = match self.round.scores[i] {
                    Some(v) => (
                        if self.round.picked_up[i] {
                            s.f(format_args!("{v}*"))
                        } else {
                            s.f(format_args!("{v}"))
                        },
                        tone_color(ScoreName::of(v, par, self.round.picked_up[i]).tone()),
                    ),
                    None => ("-", DIM),
                };
                draw_text_styled(
                    score,
                    rx + 48.,
                    ry + 14.,
                    classic(16., fade(color, ra)).with_align(TextAlign::Center),
                );
                draw_text_styled(
                    s.f(format_args!("par {par}")),
                    rx + 72.,
                    ry + 13.,
                    modern(8., fade(DIM, ra)),
                );
            }
            let to_par = self.round_to_par();
            let total = if to_par == 0 {
                s.f(format_args!("Total {}  (E)", self.round.total()))
            } else {
                s.f(format_args!("Total {}  ({to_par:+})", self.round.total()))
            };
            draw_text_styled(total, tx, y + 200. + dy, modern(16., fade(GOLD, a)));
            self.scribe = s;
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

    /// Results: rank title, personal best, the next goal and round stats.
    fn final_card(&mut self, paint: bool, w: f32, h: f32) {
        let width = (w - 32.).min(460.);
        let stats_rows = if h >= 520. { 4 } else { 0 };
        let assists = self.stats.assist_count();
        let extra = if stats_rows > 0 && assists > 0 {
            18.
        } else {
            0.
        };
        let grid = if stats_rows > 0 {
            stats_rows as f32 * 18. + 12. + extra
        } else {
            0.
        };
        let height = final_card_height(h, assists > 0);
        debug_assert_eq!(height, 156. + grid + 44. + 8. + 44. + 16.);
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
            draw_text_styled(
                if practice {
                    "Practice complete"
                } else {
                    "Round complete · your title"
                },
                cx,
                y + 24. + dy,
                modern(8., fade(GOLD, la(0))).with_align(TextAlign::Center),
            );
            let title = if practice {
                "PRACTICE MAKES PUTTS"
            } else {
                rank.title
            };
            let pulse = if self.juice.motion() > 0. {
                0.5 + 0.5 * (self.ambient * 2.5).sin()
            } else {
                0.
            };
            shapes::glow_disc(
                vec2(cx, y + 48. + dy),
                width * 0.4,
                fade(GOLD, (0.1 + 0.06 * pulse) * la(1)),
            );
            draw_text_styled(
                title,
                cx,
                y + 58. + dy,
                fit(
                    classic(24., fade(if practice { PAPER } else { GOLD }, la(1)))
                        .with_outline(fade(Color::from_rgba(60, 30, 14, 255), la(1)))
                        .with_align(TextAlign::Center),
                    title,
                    tw,
                ),
            );
            let quip = if practice {
                "Practice never touches your record."
            } else {
                rank.quip
            };
            draw_text_styled(
                quip,
                cx,
                y + 76. + dy,
                fit(
                    modern(8., fade(PAPER, la(2))).with_align(TextAlign::Center),
                    quip,
                    tw,
                ),
            );
            let mut s = std::mem::take(&mut self.scribe);
            let shown = self.roll.shown();
            let big = if to_par == 0 {
                s.f(format_args!("{shown} strokes · E"))
            } else {
                s.f(format_args!("{shown} strokes · {to_par:+}"))
            };
            draw_text_styled(
                big,
                cx,
                y + 104. + dy,
                classic(16., fade(PAPER, la(3))).with_align(TextAlign::Center),
            );
            let (best_line, best_color) = match self.round_best {
                _ if practice || self.auto && !self.capture.player_card => (
                    match self.best {
                        Some(b) => s.f(format_args!("Your best full round: {b}")),
                        None => "Play a full round to set a best.",
                    },
                    DIM,
                ),
                Some(b @ Best::New(_)) | Some(b @ Best::First) => {
                    (s.f(format_args!("{}", b.line())), MINT)
                }
                Some(b) => (s.f(format_args!("{}", b.line())), CLAY),
                None => ("", DIM),
            };
            draw_text_styled(
                best_line,
                cx,
                y + 122. + dy,
                fit(
                    modern(8., fade(best_color, la(4))).with_align(TextAlign::Center),
                    best_line,
                    tw,
                ),
            );
            let goal = match card::next_rank(to_par) {
                Some((better, strokes)) if !practice => s.f(format_args!(
                    "Next title: {} · {strokes} {} to go",
                    better.title,
                    if strokes == 1 { "stroke" } else { "strokes" }
                )),
                None if !practice => "Top title. The windmill salutes you.",
                _ => "Try a full round for a title.",
            };
            draw_text_styled(
                goal,
                cx,
                y + 140. + dy,
                fit(
                    modern(8., fade(GOLD, la(5))).with_align(TextAlign::Center),
                    goal,
                    tw,
                ),
            );
            if stats_rows > 0 {
                let st = self.stats;
                let cells: [(&str, u8); 8] = [
                    ("Aces", st.aces),
                    ("Under par", st.under),
                    ("Pars", st.pars),
                    ("Over par", st.over),
                    ("Bank shots", st.bank_shots + st.trick_shots),
                    ("Splashes", st.splashes),
                    ("Best streak", st.best_streak),
                    ("Lip-outs", st.lip_outs),
                ];
                let by = y + 156. + dy;
                shapes::fill_rounded_rect(
                    Rect::new(tx, by - 6., tw, stats_rows as f32 * 18. + 6. + extra),
                    10.,
                    Color::new(0., 0., 0., 0.2 * a),
                );
                let half = tw * 0.5;
                for (i, (name, value)) in cells.into_iter().enumerate() {
                    let ca = a * lines.sample(self.menu_time, 6 + i, self.motion()).alpha;
                    let cx = tx + (i % 2) as f32 * half + 12.;
                    let cy = by + (i / 2) as f32 * 18. + 9.;
                    draw_text_styled(name, cx, cy, modern(8., fade(DIM, ca)));
                    draw_text_styled(
                        s.f(format_args!("{value}")),
                        cx + half - 28.,
                        cy,
                        modern(8., fade(PAPER, ca)).with_align(TextAlign::Right),
                    );
                }
                if assists > 0 {
                    draw_text_styled(
                        s.f(format_args!(
                            "★ Secret copper assists {assists}/{} ★",
                            card::assist_holes()
                        )),
                        x + width * 0.5,
                        by + stats_rows as f32 * 18. + 9.,
                        modern(8., fade(GOLD, a)).with_align(TextAlign::Center),
                    );
                }
            }
            self.scribe = s;
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

/// Width `kit::key_prompt` will use, for centering before drawing.
fn prompt_width(keys: &[&str], action: &str, theme: &KitTheme) -> f32 {
    let style = Type::modern(theme.body, theme.ink);
    let h = theme.body * 1.5 + 2.;
    let caps: f32 = keys
        .iter()
        .map(|k| (text_width(k, style) + kit::SPACE[2] * 2.).max(h - 3.) + kit::SPACE[0])
        .sum();
    caps + kit::SPACE[0] + text_width(action, Type::modern(theme.body, theme.muted))
}

#[allow(dead_code)]
fn _face(_: FontFace) {}
#[allow(dead_code)]
fn _glow() {
    let _ = post::post_fx;
}
