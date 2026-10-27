//! Screen-space UI: score chip, hole title card, power dock, headline
//! banner, the between-hole sheet, menus and the results card.
use super::*;

impl App {
    /// Score chip, hole title card, onboarding hint, power meter and shot dock.
    pub(super) fn hud(&mut self, w: f32, h: f32, scene: Scene) {
        let chip_w = chip_width(w);
        self.ui
            .button(
                15,
                rect(10., 10., chip_w, 44.),
                "",
                true,
                ButtonStyle {
                    fill: DARK,
                    focus: GOLD,
                    ..Default::default()
                },
            )
            .unwrap();
        let badge = 54.;
        let text_w = chip_w - 20. - badge;
        let name = self.round.course.name.to_uppercase();
        let hole = self.round.hole_index + 1;
        let title = if text_w >= 230. {
            format!("HOLE {hole}/12 · {name}")
        } else {
            format!("{hole} · {name}")
        };
        line(
            &mut self.ui,
            rect(20., 15., text_w, 15.),
            &title,
            15.,
            PAPER,
            Align::Left,
        );
        let strokes = self.round.strokes;
        let detail = format!(
            "{}PAR {} · {} {}",
            if self.round.practice {
                "PRACTICE · "
            } else {
                ""
            },
            self.round.course.par,
            strokes,
            if strokes == 1 { "STROKE" } else { "STROKES" }
        );
        line(
            &mut self.ui,
            rect(20., 35., text_w, 12.),
            &detail,
            12.,
            GOLD,
            Align::Left,
        );
        let to_par = self.round_to_par();
        let badge_x = 10. + chip_w - badge - 6.;
        self.ui
            .panel(rect(badge_x, 15., badge, 34.), PANEL)
            .unwrap();
        line(
            &mut self.ui,
            rect(badge_x, 18., badge, 9.),
            "ROUND",
            9.,
            DIM,
            Align::Center,
        );
        let color = match to_par {
            i16::MIN..=-1 => MINT,
            0 => PAPER,
            _ => CLAY,
        };
        line(
            &mut self.ui,
            rect(badge_x, 30., badge, 16.),
            &card::to_par_text(to_par),
            16.,
            color,
            Align::Center,
        );

        // Hole title card: name, par, the hole's read and a personal best hook.
        let shown = if self.round.strokes > 0 || self.round.phase == Phase::Rolling {
            0.
        } else {
            ((INTRO - self.intro) / 0.4).clamp(0., 1.)
        };
        if shown > 0. {
            let cw = (w - 24.).min(460.);
            let cx = (w - cw) * 0.5;
            let cy = scene.y + 6.;
            let best = self.hole_best[self.round.hole_index];
            // Short landscape screens keep the name and par; the hint waits
            // for a taller view rather than covering the green.
            let roomy = scene.h >= 300.;
            let mut height = if roomy { 74. } else { 54. };
            let best_y = height - 4.;
            if best.is_some() {
                height += 16.;
            }
            let rise = if self.juice.motion() > 0. {
                // Settles upward from below, never into the menu corner.
                (1. - ease_out(self.intro / 0.35)) * 10.
            } else {
                0.
            };
            let cy = cy + rise;
            self.ui
                .panel(rect(cx, cy, cw, height), fade(DARK, 0.88 * shown))
                .unwrap();
            self.ui
                .panel(rect(cx, cy, cw, 3.), fade(COPPER, shown))
                .unwrap();
            line(
                &mut self.ui,
                rect(cx + 12., cy + 10., cw - 24., 11.),
                &format!("HOLE {} OF 12 · PAR {}", hole, self.round.course.par),
                11.,
                fade(GOLD, shown),
                Align::Center,
            );
            line(
                &mut self.ui,
                rect(cx + 12., cy + 25., cw - 24., 22.),
                &name,
                22.,
                fade(PAPER, shown),
                Align::Center,
            );
            if roomy {
                line(
                    &mut self.ui,
                    rect(cx + 12., cy + 52., cw - 24., 12.),
                    &self.round.course.hint.to_uppercase(),
                    12.,
                    fade(PAPER, shown * 0.9),
                    Align::Center,
                );
            }
            if let Some(best) = best {
                let ace = if best == 1 { " · ACED IT" } else { "" };
                line(
                    &mut self.ui,
                    rect(cx + 12., cy + best_y, cw - 24., 11.),
                    &format!("YOUR BEST HERE: {best}{ace}"),
                    11.,
                    fade(MINT, shown),
                    Align::Center,
                );
            }
        }

        let dock_w = (w - 24.).min(420.);
        let x = (w - dock_w) * 0.5;
        let y = h - 64.;
        let gap = 6.;
        let bw = (dock_w - gap * 4.) / 5.;
        if self.round.phase == Phase::Aiming {
            // Onboarding: fades over the first few accepted putts, never for demos.
            let learn = 1. - self.putts as f32 / LEARNED as f32;
            if learn > 0. && !self.auto {
                let hint = if w >= 560. {
                    "DRAG TOWARD THE TARGET, RELEASE TO PUTT  ·  OR ← → AIM, ↑ ↓ POWER, SPACE"
                } else {
                    "DRAG TO AIM, RELEASE TO PUTT"
                };
                let a = 0.35 + 0.65 * learn;
                let pill = (hint.chars().count() as f32 * 9. + 28.).min(w - 16.);
                self.ui
                    .panel(
                        rect((w - pill) * 0.5, y - 46., pill, 24.),
                        fade(DARK, 0.85 * a),
                    )
                    .unwrap();
                overlay(
                    &mut self.ui,
                    rect(8., y - 40., w - 16., 12.),
                    hint,
                    12.,
                    fade(PAPER, a),
                );
            }
            // Power meter: label, gradient fill and a readable percentage.
            let my = y - 18.;
            self.ui.panel(rect(x, my - 3., dock_w, 14.), DARK).unwrap();
            line(
                &mut self.ui,
                rect(x + 6., my, 44., 9.),
                "POWER",
                9.,
                DIM,
                Align::Left,
            );
            let bar_x = x + 52.;
            let bar_w = dock_w - 52. - 46.;
            self.ui
                .panel(rect(bar_x, my + 1., bar_w, 6.), PANEL)
                .unwrap();
            self.ui
                .gradient(
                    rect(bar_x, my + 1., bar_w * self.power, 6.),
                    MINT,
                    power_color(self.power),
                )
                .unwrap();
            line(
                &mut self.ui,
                rect(x + dock_w - 44., my - 1., 38., 11.),
                &format!("{:.0}%", self.power * 100.),
                11.,
                power_color(self.power),
                Align::Right,
            );
            for (i, id, text) in [
                (0, 10, "<"),
                (1, 11, ">"),
                (2, 12, "-"),
                (3, 13, "+"),
                (4, 14, "PUTT"),
            ] {
                button(
                    &mut self.ui,
                    id,
                    rect(x + i as f32 * (bw + gap), y, bw, 48.),
                    text,
                    true,
                );
            }
        }
    }
    /// Rare, full-width headline: aces, splashes and the hidden trick.
    pub(super) fn banner(&mut self, w: f32, scene: Scene) {
        let handoff = self.headline_handoff(w);
        let Some(b) = &self.juice.banner else { return };
        let a = b.alpha() * if b.score { 1. - handoff } else { 1. };
        if a <= 0. {
            return;
        }
        let scale = self.juice.banner_scale();
        let size = if b.head.len() > 12 { 26. } else { 34. } * scale;
        let top = if self.sheet_open() {
            if !Self::banner_room(w) {
                return; // No room above the sheet; the sheet says it all.
            }
            14.
        } else {
            scene.y + scene.h * 0.22
        };
        let band = size + 30.;
        self.ui
            .gradient(
                rect(0., top, w * 0.5, band),
                fade(DARK, 0.),
                fade(DARK, 0.92 * a),
            )
            .unwrap();
        self.ui
            .gradient(
                rect(w * 0.5, top, w * 0.5, band),
                fade(DARK, 0.92 * a),
                fade(DARK, 0.),
            )
            .unwrap();
        let (head, sub, color) = (b.head.clone(), b.sub.clone(), b.color);
        overlay(
            &mut self.ui,
            banner_text_rect(w, top + 6. + (34. - size) * 0.25, size),
            &head,
            size,
            fade(color, a),
        );
        overlay(
            &mut self.ui,
            banner_text_rect(w, top + band - 17., 12.),
            &sub,
            12.,
            fade(PAPER, a),
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
    pub(super) fn hole_sheet(&mut self, w: f32, h: f32) {
        let (sw, sh, cols) = Self::sheet_layout(w);
        self.sheet_h = sh;
        let Some(o) = self.outcome else { return };
        let handoff = self.headline_handoff(w);
        let slide = ease_out(self.sheet);
        let x = (w - sw) * 0.5;
        let y = (h - 12. - sh) + (1. - slide) * (sh + 16.);
        let tx = x + 16.;
        let tw = sw - 32.;
        self.ui.modal(900).expect("one modal");
        self.ui.panel(rect(x, y, sw, sh), DARK).unwrap();
        let tone = o.name.tone();
        self.ui.panel(rect(x, y, sw, 4.), tone_color(tone)).unwrap();
        let head_w = tw * 0.6;
        let side_x = tx + tw * 0.64;
        let side_w = tw * 0.36;
        line(
            &mut self.ui,
            rect(tx, y + 16., head_w, 24.),
            &o.name.headline(),
            24.,
            fade(tone_color(tone), handoff),
            Align::Left,
        );
        line(
            &mut self.ui,
            rect(side_x, y + 16., side_w, 11.),
            &format!("HOLE {} · PAR {}", o.hole + 1, o.par),
            11.,
            DIM,
            Align::Right,
        );
        line(
            &mut self.ui,
            rect(side_x, y + 30., side_w, 12.),
            &format!(
                "{} {}",
                o.strokes,
                if o.strokes == 1 { "STROKE" } else { "STROKES" }
            ),
            12.,
            PAPER,
            Align::Right,
        );
        line(
            &mut self.ui,
            rect(tx, y + 48., tw, 13.),
            quips::hole(&o),
            13.,
            fade(PAPER, handoff),
            Align::Left,
        );
        let mut tags: Vec<String> = Vec::with_capacity(4);
        match o.style {
            Style::Straight => {}
            Style::Bank(1) => tags.push("BANK SHOT".into()),
            Style::Bank(n) => tags.push(format!("BANK SHOT ×{n}")),
            Style::Trick(n) => tags.push(format!("TRICK SHOT ×{n}")),
        }
        if o.assist {
            tags.push("COPPER ASSIST".into());
        }
        if o.streak >= 2 {
            tags.push(format!("STREAK ×{}", o.streak));
        }
        if o.name != ScoreName::PickedUp && !self.auto {
            match self.hole_best_before {
                None => {}
                Some(b) if o.strokes < b => tags.push("NEW HOLE BEST!".into()),
                Some(b) => tags.push(format!("HOLE BEST {b}")),
            }
        }
        line(
            &mut self.ui,
            rect(tx, y + 66., tw, 11.),
            &tags[..tags.len().min(3)].join("  ·  "),
            11.,
            GOLD,
            Align::Left,
        );
        let strip_y = y + 86.;
        self.score_strip(tx, strip_y, tw, cols);
        let rows = 12 / cols;
        let after = strip_y + rows as f32 * 34. + (rows - 1) as f32 * 4. + 10.;
        let to_par = self.round_to_par();
        let thru = self.round.scores.iter().flatten().count();
        let summary = if self.round.practice {
            format!("PRACTICE · TOTAL {} · THRU {thru}", self.round.total())
        } else {
            format!(
                "ROUND {} · TOTAL {} · THRU {thru}",
                card::to_par_text(to_par),
                self.round.total()
            )
        };
        line(
            &mut self.ui,
            rect(tx, after, tw, 12.),
            &summary,
            12.,
            GOLD,
            Align::Left,
        );
        let by = after + 22.;
        let next_w = (tw - 8.) * 0.62;
        button(
            &mut self.ui,
            24,
            rect(tx, by, next_w, 44.),
            if self.round.hole_index == 11 {
                "FINISH"
            } else {
                "NEXT HOLE"
            },
            true,
        );
        button(
            &mut self.ui,
            23,
            rect(tx + next_w + 8., by, tw - next_w - 8., 44.),
            "CARD",
            true,
        );
    }
    /// Twelve compact cells, colored by result; the current hole is outlined.
    pub(super) fn score_strip(&mut self, x: f32, y: f32, w: f32, cols: usize) {
        let gap = 4.;
        let cw = (w - gap * (cols - 1) as f32) / cols as f32;
        for i in 0..12 {
            let cx = x + (i % cols) as f32 * (cw + gap);
            let cy = y + (i / cols) as f32 * (34. + gap);
            let current = i == self.round.hole_index;
            if current {
                self.ui
                    .panel(rect(cx - 1., cy - 1., cw + 2., 36.), GOLD)
                    .unwrap();
            }
            self.ui.panel(rect(cx, cy, cw, 34.), PANEL).unwrap();
            line(
                &mut self.ui,
                rect(cx, cy + 4., cw, 8.),
                &format!("{}", i + 1),
                8.,
                DIM,
                Align::Center,
            );
            let par = course::hole(i).par;
            let (text, color) = match self.round.scores[i] {
                Some(s) => {
                    let name = ScoreName::of(s, par, self.round.picked_up[i]);
                    (
                        if self.round.picked_up[i] {
                            format!("{s}*")
                        } else {
                            format!("{s}")
                        },
                        tone_color(name.tone()),
                    )
                }
                None => ("·".into(), DIM),
            };
            line(
                &mut self.ui,
                rect(cx, cy + 15., cw, 14.),
                &text,
                14.,
                color,
                Align::Center,
            );
        }
    }
    pub(super) fn modal(&mut self, w: f32, h: f32) {
        if self.menu == Menu::None && !self.manual_pause && self.round.phase == Phase::Finished {
            self.final_card(w, h);
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
        self.ui.modal(900).expect("one modal");
        self.ui.panel(rect(0., 0., w, h), fade(DARK, 0.5)).unwrap();
        self.ui.panel(rect(x, y, width, height), DARK).unwrap();
        self.ui.panel(rect(x, y, width, 4.), COPPER).unwrap();
        if self.menu == Menu::Practice {
            label(
                &mut self.ui,
                rect(tx, y + 16., tw, 26.),
                "PRACTICE",
                20.,
                PAPER,
            );
            line(
                &mut self.ui,
                rect(tx + 120., y + 21., tw - 120., 11.),
                "PICK ANY GREEN",
                11.,
                DIM,
                Align::Right,
            );
            let cw = (tw - 18.) / 4.;
            for i in 0..12 {
                button(
                    &mut self.ui,
                    100 + i as u64,
                    rect(
                        tx + (i % 4) as f32 * (cw + 6.),
                        y + 54. + (i / 4) as f32 * 50.,
                        cw,
                        44.,
                    ),
                    &format!("{:02}", i + 1),
                    true,
                );
            }
            button(&mut self.ui, 20, rect(tx, y + 208., tw, 44.), "BACK", true);
            return;
        }
        if self.menu == Menu::Score {
            self.score_modal(tx, y, tw);
            return;
        }
        let heading = if self.manual_pause {
            "PAUSED"
        } else {
            "COPPER GREENS"
        };
        label(
            &mut self.ui,
            rect(tx, y + 20., tw, 28.),
            heading,
            20.,
            PAPER,
        );
        if self.manual_pause {
            line(
                &mut self.ui,
                rect(tx, y + 50., tw, 12.),
                &format!(
                    "HOLE {} · {} · {} {}",
                    self.round.hole_index + 1,
                    self.round.course.name.to_uppercase(),
                    self.round.strokes,
                    if self.round.strokes == 1 {
                        "STROKE"
                    } else {
                        "STROKES"
                    }
                ),
                12.,
                GOLD,
                Align::Left,
            );
            line(
                &mut self.ui,
                rect(tx, y + 70., tw, 12.),
                quips::pause(self.pause_seed),
                12.,
                DIM,
                Align::Left,
            );
        } else {
            line(
                &mut self.ui,
                rect(tx, y + 50., tw, 12.),
                &format!("12 HOLES · PAR {}", card::course_par()),
                12.,
                GOLD,
                Align::Left,
            );
            let best = match self.best {
                Some(b) => format!(
                    "BEST ROUND {b} ({})",
                    card::to_par_text(b as i16 - card::course_par() as i16)
                ),
                None => "NO BEST ROUND YET".into(),
            };
            line(
                &mut self.ui,
                rect(tx, y + 70., tw, 12.),
                &best,
                12.,
                MINT,
                Align::Left,
            );
        }
        let first = 20;
        let action = if self.manual_pause { "RESUME" } else { "PLAY" };
        button(
            &mut self.ui,
            first,
            rect(tx, y + 96., tw, 44.),
            action,
            true,
        );
        let half = (tw - 8.) * 0.5;
        button(
            &mut self.ui,
            22,
            rect(tx, y + 152., half, 44.),
            "PRACTICE",
            true,
        );
        button(
            &mut self.ui,
            23,
            rect(tx + half + 8., y + 152., half, 44.),
            "CARD",
            true,
        );
        if self.manual_pause {
            button(
                &mut self.ui,
                25,
                rect(tx, y + 204., half, 44.),
                "TAKE 12",
                matches!(self.round.phase, Phase::Aiming | Phase::Rolling),
            );
            button(
                &mut self.ui,
                21,
                rect(tx + half + 8., y + 204., half, 44.),
                "RESTART",
                true,
            );
        }
    }
    pub(super) fn score_modal(&mut self, tx: f32, y: f32, tw: f32) {
        label(
            &mut self.ui,
            rect(tx, y + 16., tw, 26.),
            "SCORECARD",
            20.,
            PAPER,
        );
        line(
            &mut self.ui,
            rect(tx + 160., y + 21., tw - 160., 11.),
            if self.round.practice {
                "PRACTICE"
            } else {
                "FULL ROUND"
            },
            11.,
            DIM,
            Align::Right,
        );
        let cw = (tw - 12.) * 0.5;
        for i in 0..12 {
            let rx = tx + (i / 6) as f32 * (cw + 12.);
            let ry = y + 50. + (i % 6) as f32 * 21.;
            let par = course::hole(i).par;
            if i == self.round.hole_index && self.round.phase != Phase::Finished {
                self.ui
                    .panel(rect(rx - 4., ry - 2., cw + 8., 19.), PANEL)
                    .unwrap();
            }
            line(
                &mut self.ui,
                rect(rx, ry + 1., 26., 14.),
                &format!("{:02}", i + 1),
                14.,
                DIM,
                Align::Left,
            );
            let (score, color) = match self.round.scores[i] {
                Some(s) => (
                    format!("{s}{}", if self.round.picked_up[i] { "*" } else { "" }),
                    tone_color(ScoreName::of(s, par, self.round.picked_up[i]).tone()),
                ),
                None => ("-".into(), DIM),
            };
            line(
                &mut self.ui,
                rect(rx + 30., ry + 1., 36., 14.),
                &score,
                14.,
                color,
                Align::Center,
            );
            line(
                &mut self.ui,
                rect(rx + 70., ry + 3., cw - 70., 10.),
                &format!("PAR {par}"),
                10.,
                DIM,
                Align::Left,
            );
        }
        let total = format!(
            "TOTAL {}  ({})",
            self.round.total(),
            card::to_par_text(self.round_to_par())
        );
        line(
            &mut self.ui,
            rect(tx, y + 184., tw, 16.),
            &total,
            16.,
            GOLD,
            Align::Left,
        );
        let half = (tw - 8.) * 0.5;
        button(
            &mut self.ui,
            20,
            rect(tx, y + 216., half, 44.),
            "BACK",
            true,
        );
        button(
            &mut self.ui,
            25,
            rect(tx + half + 8., y + 216., half, 44.),
            "TAKE 12",
            self.round.phase == Phase::Aiming,
        );
    }
    /// Results: rank title, personal best, the next goal and round stats.
    pub(super) fn final_card(&mut self, w: f32, h: f32) {
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
        self.ui.modal(900).expect("one modal");
        self.ui.panel(rect(0., 0., w, h), fade(DARK, 0.5)).unwrap();
        self.ui.panel(rect(x, y, width, height), DARK).unwrap();
        let total = self.round.total();
        let to_par = self.round_to_par();
        let rank = card::rank(to_par);
        let practice = self.round.practice;
        let accent = if practice {
            PAPER
        } else if to_par < 0 {
            GOLD
        } else {
            COPPER
        };
        self.ui.panel(rect(x, y, width, 4.), accent).unwrap();
        line(
            &mut self.ui,
            rect(tx, y + 16., tw, 11.),
            if practice {
                "PRACTICE COMPLETE"
            } else {
                "ROUND COMPLETE · YOUR TITLE"
            },
            11.,
            GOLD,
            Align::Center,
        );
        let pulse = if self.juice.motion() > 0. {
            1. + 0.04 * (self.juice.time * 3.).sin()
        } else {
            1.
        };
        line(
            &mut self.ui,
            rect(tx, y + 32., tw, 26. * pulse),
            if practice {
                "PRACTICE MAKES PUTTS"
            } else {
                rank.title
            },
            26. * pulse,
            if practice { PAPER } else { GOLD },
            Align::Center,
        );
        line(
            &mut self.ui,
            rect(tx, y + 64., tw, 12.),
            if practice {
                "PRACTICE NEVER TOUCHES YOUR RECORD"
            } else {
                rank.quip
            },
            12.,
            PAPER,
            Align::Center,
        );
        line(
            &mut self.ui,
            rect(tx, y + 86., tw, 18.),
            &format!("{total} STROKES · {}", card::to_par_text(to_par)),
            18.,
            PAPER,
            Align::Center,
        );
        let (best_line, best_color) = match self.round_best {
            _ if practice || self.auto && !self.capture.player_card => (
                match self.best {
                    Some(b) => format!("YOUR BEST FULL ROUND: {b}"),
                    None => "PLAY A FULL ROUND TO SET A BEST".into(),
                },
                DIM,
            ),
            Some(b @ Best::New(_)) | Some(b @ Best::First) => (b.line(), MINT),
            Some(b) => (b.line(), CLAY),
            None => (String::new(), DIM),
        };
        line(
            &mut self.ui,
            rect(tx, y + 112., tw, 13.),
            &best_line,
            13.,
            best_color,
            Align::Center,
        );
        let goal = match card::next_rank(to_par) {
            Some((better, strokes)) if !practice => format!(
                "NEXT TITLE: {} · {strokes} {} TO GO",
                better.title,
                if strokes == 1 { "STROKE" } else { "STROKES" }
            ),
            None if !practice => "TOP TITLE. THE WINDMILL SALUTES YOU.".into(),
            _ => "TRY A FULL ROUND FOR A TITLE".into(),
        };
        line(
            &mut self.ui,
            rect(tx, y + 132., tw, 12.),
            &goal,
            12.,
            GOLD,
            Align::Center,
        );
        let mut by = y + 156.;
        if stats_rows > 0 {
            let s = self.stats;
            let cells = [
                ("ACES", s.aces),
                ("UNDER PAR", s.under),
                ("PARS", s.pars),
                ("OVER PAR", s.over),
                ("BANK SHOTS", s.bank_shots + s.trick_shots),
                ("SPLASHES", s.splashes),
                ("BEST STREAK", s.best_streak),
                ("LIP-OUTS", s.lip_outs),
            ];
            self.ui
                .panel(
                    rect(tx, by - 6., tw, stats_rows as f32 * 18. + 6. + extra),
                    PANEL,
                )
                .unwrap();
            let half = tw * 0.5;
            for (i, (name, value)) in cells.into_iter().enumerate() {
                let cx = tx + (i % 2) as f32 * half + 10.;
                let cy = by + (i / 2) as f32 * 18.;
                line(
                    &mut self.ui,
                    rect(cx, cy, half - 60., 11.),
                    name,
                    11.,
                    DIM,
                    Align::Left,
                );
                line(
                    &mut self.ui,
                    rect(cx + half - 64., cy, 40., 12.),
                    &value.to_string(),
                    12.,
                    PAPER,
                    Align::Right,
                );
            }
            if assists > 0 {
                line(
                    &mut self.ui,
                    rect(tx, by + stats_rows as f32 * 18., tw, 11.),
                    &format!(
                        "* SECRET COPPER ASSISTS {assists}/{} *",
                        card::assist_holes()
                    ),
                    11.,
                    GOLD,
                    Align::Center,
                );
            }
            by += grid;
        }
        button(&mut self.ui, 21, rect(tx, by, tw, 44.), "PLAY AGAIN", true);
        let half = (tw - 8.) * 0.5;
        button(
            &mut self.ui,
            22,
            rect(tx, by + 52., half, 44.),
            "PRACTICE",
            true,
        );
        button(
            &mut self.ui,
            23,
            rect(tx + half + 8., by + 52., half, 44.),
            "CARD",
            true,
        );
    }
}
