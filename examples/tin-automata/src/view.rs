//! Read-only workshop art, the program tape editor and room results.
use crate::model::{COLS, Game, Op, Phase, ROWS};
use ferravex_arcade::shapes::{self, Fill, Stroke};
use ferravex_arcade::typography::{TextAlign, TextStyle, draw_text_styled, text_width, wrap_text};
use ferravex_arcade::*;
use ferravex_play::navigation::Cell;

const MINT: Color = Color::new(0.43, 0.86, 0.77, 1.);
const CREAM: Color = Color::new(0.98, 0.88, 0.65, 1.);
const COPPER: Color = Color::new(0.89, 0.55, 0.3, 1.);
const RED: Color = Color::new(0.97, 0.42, 0.4, 1.);
const BLUE: Color = Color::new(0.44, 0.67, 0.96, 1.);
const PLUM: Color = Color::new(0.24, 0.16, 0.27, 1.);
const SOFT: Color = Color::new(0.72, 0.78, 0.80, 1.);
const BENCH: Color = Color::new(0.075, 0.1, 0.135, 1.);
const TILE: f32 = 52.;

/// Flat text: every label here sits on a solid surface, so it skips the
/// drop-shadow layer that would double its glyph cost.
fn ink(size: f32, color: Color) -> TextStyle {
    TextStyle::modern(size, color)
}
fn label(value: &str, x: f32, y: f32, size: f32, color: Color) -> f32 {
    draw_text_styled(value, x, y, ink(size, color)).width
}
fn label_c(value: &str, cx: f32, y: f32, size: f32, color: Color) {
    draw_text_styled(value, cx, y, ink(size, color).with_align(TextAlign::Center));
}
fn label_r(value: &str, right: f32, y: f32, size: f32, color: Color) {
    draw_text_styled(
        value,
        right,
        y,
        ink(size, color).with_align(TextAlign::Right),
    );
}
/// Label shrunk to fit `width`, never enlarged.
fn label_fit(value: &str, x: f32, y: f32, width: f32, size: f32, color: Color) {
    let s = ink(size, color);
    let actual = text_width(value, s);
    let s = if actual > width {
        s.with_size(size * width / actual)
    } else {
        s
    };
    draw_text_styled(value, x, y, s);
}
/// Wrapped flat paragraph; returns the next baseline.
fn paragraph(value: &str, x: f32, y: f32, width: f32, size: f32, color: Color) -> f32 {
    let s = ink(size, color);
    let mut baseline = y;
    for line in wrap_text(value, width, s) {
        draw_text_styled(line, x, baseline, s);
        baseline += size * 1.3;
    }
    baseline
}
fn eyebrow_style(color: Color) -> TextStyle {
    ink(10.5, color).with_letter_spacing(1.4)
}
fn eyebrow(value: &str, x: f32, y: f32, color: Color) -> f32 {
    draw_text_styled(value, x, y, eyebrow_style(color)).width
}
/// Compact key hint: a small cap followed by its action; returns the width.
fn mini_key(key: &str, action: &str, x: f32, y: f32, color: Color) -> f32 {
    let ks = ink(10.5, CREAM);
    let w = (text_width(key, ks) + 9.).max(16.);
    rounded(x, y + 1.5, w, 15., 3.5, Color::new(0., 0., 0., 0.45));
    rounded_gradient(
        x,
        y,
        w,
        15.,
        3.5,
        Color::new(0.21, 0.25, 0.31, 1.),
        Color::new(0.14, 0.17, 0.22, 1.),
    );
    draw_text_styled(
        key,
        x + w * 0.5,
        ks.centered_baseline(y + 7.5),
        ks.with_align(TextAlign::Center),
    );
    let a = draw_text_styled(
        action,
        x + w + 5.,
        ink(12., color).centered_baseline(y + 7.5),
        ink(12., color),
    );
    w + 5. + a.width
}

/// Score with thousands separators, matching the host header.
fn grouped(value: u32) -> String {
    let digits = value.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}
fn center(cell: Cell) -> Vec2 {
    vec2(
        28. + cell.x as f32 * TILE + 26.,
        168. + cell.y as f32 * TILE + 26.,
    )
}
fn parcel_color(index: usize) -> Color {
    [COPPER, BLUE, MINT][index]
}
fn op_color(op: Op) -> Color {
    match op {
        Op::Forward => MINT,
        Op::Left | Op::Right => BLUE,
        Op::Grab | Op::Drop => COPPER,
        Op::Repeat2 | Op::Repeat3 => Color::new(0.78, 0.6, 0.95, 1.),
    }
}
/// Square opcode badge with the instruction's tape letter.
fn op_badge(op: Op, x: f32, y: f32, size: f32, on_light: bool) {
    let color = op_color(op);
    if on_light {
        panel(x, y, size, size, Color::new(0.04, 0.05, 0.07, 0.9));
    } else {
        panel(x, y, size, size, darken(color, 0.35));
        panel(x, y, size, size * 0.5, darken(color, 0.25));
    }
    let s = ink(size * 0.72, lighten(color, 0.3)).with_align(TextAlign::Center);
    draw_text_styled(
        &op.code().to_string(),
        x + size * 0.5 + 0.5,
        s.centered_baseline(y + size * 0.5),
        s,
    );
}
/// Five-pointed star from ten triangles.
fn star(c: Vec2, outer: f32, color: Color) {
    let inner = outer * 0.45;
    let point = |i: usize| {
        let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.;
        c + vec2(a.cos(), a.sin()) * if i.is_multiple_of(2) { outer } else { inner }
    };
    for i in 0..10 {
        draw_triangle(c, point(i), point(i + 1), color);
    }
}
fn parcel(pos: Vec2, index: usize, small: bool) {
    let r = if small { 10. } else { 15. };
    let color = parcel_color(index);
    rounded(
        pos.x - r + 2.,
        pos.y - r + 4.,
        r * 2.,
        r * 2.,
        4.,
        Color::new(0.01, 0.02, 0.03, 0.55),
    );
    rounded_gradient(
        pos.x - r,
        pos.y - r,
        r * 2.,
        r * 2.,
        4.,
        lighten(color, 0.18),
        darken(color, 0.12),
    );
    panel(
        pos.x - r + 3.,
        pos.y - r + 1.,
        r * 2. - 6.,
        1.5,
        lighten(color, 0.4),
    );
    panel(
        pos.x - 3.,
        pos.y - r,
        6.,
        r * 2.,
        Color::new(0.97, 0.9, 0.68, 0.7),
    );
    let s = ink(if small { 10. } else { 13. }, INK).with_align(TextAlign::Center);
    draw_text_styled(
        &(index + 1).to_string(),
        pos.x + 0.5,
        s.centered_baseline(pos.y),
        s,
    );
}

impl Game {
    fn draw_floor(&self, cell: Cell, p: Vec2) {
        let shade = if (cell.x + cell.y).is_multiple_of(2) {
            0.015
        } else {
            0.
        };
        let r = Rect::new(p.x - 25., p.y - 25., 50., 50.);
        shapes::fill_rounded_rect(
            r,
            0.,
            Fill::vertical(
                r,
                Color::new(0.18 + shade, 0.28 + shade, 0.29 + shade, 1.),
                Color::new(0.15 + shade, 0.24 + shade, 0.25 + shade, 1.),
            ),
        );
        panel(p.x - 25., p.y - 25., 50., 1., Color::new(1., 1., 1., 0.07));
        panel(p.x - 25., p.y + 24., 50., 1., Color::new(0., 0., 0., 0.18));
        panel(
            p.x - 20.,
            p.y + 18.,
            2.,
            2.,
            Color::new(0.38, 0.5, 0.5, 0.7),
        );
    }
    fn draw_wall(&self, cell: Cell, p: Vec2) {
        let below = self.tile(Cell {
            x: cell.x,
            y: cell.y + 1,
        });
        if below != b'#' && cell.y + 1 < ROWS {
            panel(
                p.x - 25.,
                p.y + 25.,
                50.,
                7.,
                Color::new(0., 0.01, 0.02, 0.4),
            );
        }
        let top = Rect::new(p.x - 25., p.y - 25., 50., 44.);
        shapes::fill_rounded_rect(
            top,
            0.,
            Fill::vertical(
                top,
                Color::new(0.17, 0.19, 0.24, 1.),
                Color::new(0.12, 0.135, 0.17, 1.),
            ),
        );
        panel(
            p.x - 25.,
            p.y + 19.,
            50.,
            6.,
            Color::new(0.06, 0.065, 0.085, 1.),
        );
        panel(p.x - 25., p.y - 25., 50., 1.5, Color::new(1., 1., 1., 0.12));
        for (dx, dy) in [(-19., -19.), (16., -19.), (-19., 13.), (16., 13.)] {
            panel(p.x + dx, p.y + dy, 3., 3., Color::new(0.3, 0.33, 0.39, 1.));
        }
    }
    fn draw_pit(&self, p: Vec2) {
        let r = Rect::new(p.x - 25., p.y - 25., 50., 50.);
        shapes::fill_rounded_rect(
            r,
            0.,
            Fill::vertical(
                r,
                Color::new(0.005, 0.01, 0.02, 1.),
                Color::new(0.03, 0.05, 0.07, 1.),
            ),
        );
        panel(
            p.x - 25.,
            p.y - 25.,
            50.,
            6.,
            Color::new(0.16, 0.1, 0.05, 1.),
        );
        for i in 0..4 {
            let x = p.x - 23. + i as f32 * 12.;
            draw_line(x, p.y - 19., x + 6., p.y - 25., 3., COPPER);
        }
    }
    fn draw_board(&self) {
        // Workbench header: room name and its budget.
        label(self.content().name, 28., 140., 20., CREAM);
        let budget = format!(
            "PAR {}  ·  BUDGET {}",
            self.content().solution.len(),
            self.content().budget
        );
        draw_text_styled(
            &budget,
            496.,
            138.,
            eyebrow_style(SOFT).with_align(TextAlign::Right),
        );
        // Recessed bench plate.
        rounded(16., 150., 490., 390., 10., Color::new(0., 0., 0., 0.3));
        rounded_gradient(
            17.,
            149.,
            488.,
            387.,
            10.,
            Color::new(0.06, 0.09, 0.11, 1.),
            Color::new(0.045, 0.07, 0.085, 1.),
        );
        for x in 0..COLS {
            label_c(
                &char::from(b'A' + x as u8).to_string(),
                54. + x as f32 * TILE,
                163.,
                10.,
                MUTED,
            );
        }
        for y in 0..ROWS {
            label_c(
                &(y + 1).to_string(),
                22.5,
                198. + y as f32 * TILE,
                10.,
                MUTED,
            );
        }
        // Floors, pits and markings first; walls cast onto the row below.
        for y in 0..ROWS {
            for x in 0..COLS {
                let cell = Cell { x, y };
                let p = center(cell);
                match self.tile(cell) {
                    b'#' => {}
                    b'~' => self.draw_pit(p),
                    _ => self.draw_floor(cell, p),
                }
            }
        }
        for y in 0..ROWS {
            for x in 0..COLS {
                let cell = Cell { x, y };
                let p = center(cell);
                let tile = self.tile(cell);
                match tile {
                    b'#' => self.draw_wall(cell, p),
                    b'A' | b'B' => {
                        let open = self.switches & (1 << (tile - b'A')) != 0;
                        let color = if open { MINT } else { RED };
                        if open {
                            outline(
                                p.x - 21.,
                                p.y - 21.,
                                42.,
                                42.,
                                6.,
                                1.5,
                                with_alpha(MINT, 0.6),
                            );
                            for dx in [-17., 13.] {
                                panel(p.x + dx, p.y - 20., 4., 40., with_alpha(MINT, 0.25));
                            }
                        } else {
                            rounded(p.x - 22., p.y - 22., 44., 44., 6., darken(RED, 0.72));
                            for i in -1..=1 {
                                let bx = p.x + i as f32 * 12. - 2.;
                                panel(bx, p.y - 20., 5., 40., RED);
                                panel(bx, p.y - 20., 2., 40., lighten(RED, 0.3));
                            }
                            outline(p.x - 22., p.y - 22., 44., 44., 6., 2., color);
                        }
                        let s =
                            ink(16., if open { MINT } else { CREAM }).with_align(TextAlign::Center);
                        rounded(
                            p.x - 9.,
                            p.y - 9.,
                            18.,
                            18.,
                            4.,
                            Color::new(0.04, 0.05, 0.07, 0.85),
                        );
                        draw_text_styled(
                            &char::from(tile).to_string(),
                            p.x + 0.5,
                            s.centered_baseline(p.y),
                            s,
                        );
                    }
                    b'a' | b'b' => {
                        let active = self.switches & (1 << (tile - b'a')) != 0;
                        let rim = if active { MINT } else { CREAM };
                        shapes::fill_circle(p + vec2(0., 2.), 19., Color::new(0., 0., 0., 0.4));
                        shapes::fill_circle(p, 19., darken(rim, 0.45));
                        shapes::fill_circle(
                            p,
                            15.,
                            if active {
                                Fill::radial(p - vec2(4., 4.), 20., lighten(MINT, 0.3), MINT)
                            } else {
                                Fill::radial(p - vec2(4., 4.), 20., lighten(PLUM, 0.2), PLUM)
                            },
                        );
                        let s = ink(15., if active { INK } else { CREAM })
                            .with_align(TextAlign::Center);
                        draw_text_styled(
                            &char::from(tile).to_string(),
                            p.x + 0.5,
                            s.centered_baseline(p.y - 1.),
                            s,
                        );
                    }
                    b'x'..=b'z' => {
                        let i = (tile - b'x') as usize;
                        let color = parcel_color(i);
                        let done = self.parcels[i].delivered;
                        rounded(
                            p.x - 21.,
                            p.y - 21.,
                            42.,
                            42.,
                            7.,
                            with_alpha(color, if done { 0.28 } else { 0.1 }),
                        );
                        outline(p.x - 21., p.y - 21., 42., 42., 7., 2., color);
                        for (cx, cy) in [(-1., -1.), (1., -1.), (-1., 1.), (1., 1.)] {
                            panel(
                                p.x + cx * 15. - 2.,
                                p.y + cy * 15. - 2.,
                                4.,
                                4.,
                                with_alpha(color, 0.7),
                            );
                        }
                        if done {
                            shapes::polyline(
                                &[p + vec2(-10., 1.), p + vec2(-3., 8.), p + vec2(11., -8.)],
                                Stroke::new(4., CREAM)
                                    .join(shapes::Join::Round)
                                    .cap(shapes::Cap::Round),
                            );
                        } else {
                            let s = ink(18., color).with_align(TextAlign::Center);
                            draw_text_styled(
                                &(i + 1).to_string(),
                                p.x + 0.5,
                                s.centered_baseline(p.y),
                                s,
                            );
                        }
                    }
                    _ => {}
                }
            }
        }
        for (i, p) in self.parcels.iter().enumerate() {
            if p.present && !p.delivered && self.carrying != Some(i) {
                parcel(center(p.pos), i, false);
            }
        }
        self.draw_robot();
        let fault = self.phase == Phase::Fault;
        let tone = if fault { RED } else { MINT };
        shapes::fill_circle(vec2(25., 554.), 3.5, tone);
        label_fit(
            self.notice,
            35.,
            558.,
            466.,
            12.5,
            if fault { RED } else { SOFT },
        );
    }
    fn draw_robot(&self) {
        let p = center(self.robot);
        let running = self.phase == Phase::Running;
        // Gentle bob in step with the execution cadence.
        let bob = if running {
            -(self.timer as f32 / 18. * std::f32::consts::PI).sin() * 2.
        } else {
            0.
        };
        if self.phase == Phase::Fault {
            outline(p.x - 24., p.y - 24., 48., 48., 7., 2., RED);
            rounded(p.x - 24., p.y - 24., 48., 48., 7., with_alpha(RED, 0.12));
        }
        let d = [vec2(0., -1.), vec2(1., 0.), vec2(0., 1.), vec2(-1., 0.)][self.facing as usize];
        let across = vec2(-d.y, d.x);
        // Facing indicator under the robot.
        let tip = p + d * 35.;
        draw_triangle(
            tip + vec2(1., 2.),
            p + d * 25. + across * 6. + vec2(1., 2.),
            p + d * 25. - across * 6. + vec2(1., 2.),
            Color::new(0., 0., 0., 0.45),
        );
        draw_triangle(
            tip,
            p + d * 25. + across * 6.,
            p + d * 25. - across * 6.,
            CREAM,
        );
        draw_ellipse(
            p.x + 1.,
            p.y + 19.,
            18.,
            6.,
            0.,
            Color::new(0.01, 0.02, 0.03, 0.6),
        );
        let p = p + vec2(0., bob);
        for dx in [-10., 10.] {
            rounded(p.x + dx - 4., p.y + 11., 8., 9., 2.5, darken(COPPER, 0.15));
        }
        rounded_gradient(
            p.x - 14.,
            p.y - 6.,
            28.,
            21.,
            5.,
            lighten(MINT, 0.12),
            darken(MINT, 0.25),
        );
        panel(
            p.x - 6.,
            p.y + 2.,
            12.,
            3.,
            Color::new(0.06, 0.16, 0.16, 0.6),
        );
        rounded_gradient(
            p.x - 17.,
            p.y - 23.,
            34.,
            21.,
            6.,
            Color::new(1., 0.95, 0.8, 1.),
            Color::new(0.88, 0.76, 0.55, 1.),
        );
        rounded(p.x - 12., p.y - 18., 24., 9., 3., INK);
        let gaze = d * 2.;
        let eye = if self.phase == Phase::Fault {
            RED
        } else {
            MINT
        };
        for ex in [-5.5, 5.5] {
            draw_rectangle(
                p.x + ex - 2. + gaze.x,
                p.y - 15.5 + gaze.y * 0.5,
                4.,
                4.,
                eye,
            );
        }
        draw_line(p.x, p.y - 23., p.x, p.y - 29., 2., COPPER);
        let bulb = vec2(p.x, p.y - 30.);
        let lit = running && self.timer < 9;
        let bulb_color = if lit { lighten(RED, 0.25) } else { RED };
        if lit {
            shapes::fill_circle(
                bulb,
                8.,
                Fill::radial(bulb, 8., with_alpha(RED, 0.55), with_alpha(RED, 0.)),
            );
        }
        draw_circle(bulb.x, bulb.y, 3., bulb_color);
        if let Some(i) = self.carrying {
            // Held against the chest between both arms.
            parcel(p + vec2(0., 6.), i, true);
            for side in [-1., 1.] {
                rounded(
                    p.x + side * 12. - 2.5,
                    p.y + 1.,
                    5.,
                    10.,
                    2.5,
                    darken(COPPER, 0.1),
                );
            }
        }
    }
    fn draw_editor(&self) {
        surface(516., 116., 428., 452.);
        let editing = self.phase == Phase::Editing;
        let w = label("Program", 530., 145., 18., CREAM);
        label(
            &format!("{} of {} cells", self.tape.len, self.content().budget),
            530. + w + 10.,
            145.,
            12.,
            MUTED,
        );
        let (state, tone) = match self.phase {
            Phase::Editing => ("Editing", MINT),
            Phase::Running => ("Running", COPPER),
            Phase::Paused => ("Paused", CREAM),
            Phase::Fault => ("Fault · C to edit", RED),
            _ => ("Ready", MINT),
        };
        let cw = text_width(state, style(12., tone)) + 12. * 1.3;
        chip(state, 930. - cw, 129., 12., tone);
        for index in 0..32 {
            let x = 526. + (index / 16) as f32 * 205.;
            let y = 160. + (index % 16) as f32 * 18.;
            let valid = index < self.content().budget;
            let selected = editing && index == self.tape.cursor;
            let executed = !editing && self.last_pc == Some(index);
            let base = y + 12.5;
            if executed {
                rounded(x, y, 198., 17., 4., COPPER);
            } else if selected {
                rounded(x, y, 198., 17., 4., PLUM);
                panel(x, y + 2., 2.5, 13., MINT);
            } else if valid {
                panel(
                    x,
                    y,
                    198.,
                    17.,
                    Color::new(1., 1., 1., if index % 2 == 0 { 0.045 } else { 0.03 }),
                );
            } else {
                panel(x, y + 8., 198., 1., Color::new(1., 1., 1., 0.035));
            }
            if !valid {
                continue;
            }
            label(
                &format!("{:02}", index + 1),
                x + 7.,
                base,
                10.,
                if executed { INK } else { MUTED },
            );
            if index < self.tape.len {
                let op = self.tape.ops[index];
                op_badge(op, x + 30., y + 2., 13., executed);
                label(
                    op.name(),
                    x + 50.,
                    base,
                    12.,
                    if executed { INK } else { PAPER },
                );
            } else if selected {
                label("+ Write here", x + 30., base, 11., CREAM);
            }
        }
        // Run statistics.
        let mut sx = 530.;
        for (name, value) in [
            ("PAR", self.content().solution.len().to_string()),
            ("STEPS", format!("{}/160", self.steps)),
            ("FAULTS", self.failures.to_string()),
        ] {
            sx += eyebrow(name, sx, 463., MUTED) + 6.;
            sx += label(&value, sx, 464., 13., CREAM) + 18.;
        }
        // Instruction picker (click to cycle).
        let op = Op::ALL[self.palette];
        let bar = Rect::new(526., 472., 410., 32.);
        shapes::fill_rounded_rect(
            bar,
            8.,
            Fill::vertical(bar, lighten(PLUM, 0.08), darken(PLUM, 0.1)),
        );
        if editing {
            shapes::stroke_rounded_rect(bar, 8., 1., with_alpha(op_color(op), 0.5));
        }
        let arrows = if editing {
            CREAM
        } else {
            with_alpha(CREAM, 0.3)
        };
        label("◀", 538., 493., 13., arrows);
        label_r("▶", 924., 493., 13., arrows);
        let name_w = text_width(op.name(), ink(17., CREAM));
        let left = 731. - (name_w + 26.) * 0.5;
        op_badge(op, left, 479., 18., false);
        label(op.name(), left + 26., 495., 17., CREAM);
        // Context key hints.
        let mut kx = 528.;
        if editing {
            kx += mini_key("←→", "Choose", kx, 509., SOFT) + 14.;
            kx += mini_key("Space", "Write", kx, 509., SOFT) + 14.;
            kx += mini_key("↑↓", "Cell", kx, 509., SOFT) + 14.;
            mini_key("X", "Delete", kx, 509., SOFT);
        } else if self.repeats > 0 {
            let w = label("Repeat active", kx, 521., 12., COPPER);
            mini_key("C", "Restore board", kx + w + 14., 509., SOFT);
        } else {
            kx += mini_key("1", "Run / pause", kx, 509., SOFT) + 14.;
            kx += mini_key("2", "Step", kx, 509., SOFT) + 14.;
            mini_key("C", "Back to editor", kx, 509., SOFT);
        }
        // Slot buttons 1–5.
        for (i, name) in [
            if self.phase == Phase::Running {
                "Pause"
            } else {
                "Run"
            },
            "Step",
            "Undo",
            "Clear",
            "Hint",
        ]
        .iter()
        .enumerate()
        {
            let x = 526. + i as f32 * 82.;
            let active = (i == 0 && self.phase == Phase::Running) || (i == 4 && self.show_hint);
            let dim = matches!(i, 2 | 3) && !editing;
            let a = if dim { 0.45 } else { 1. };
            let (top, bottom) = if active {
                (darken(MINT, 0.45), darken(MINT, 0.62))
            } else {
                (
                    Color::new(0.17, 0.24, 0.27, 1.),
                    Color::new(0.11, 0.16, 0.19, 1.),
                )
            };
            panel(x + 3., 556., 71., 3., Color::new(0., 0., 0., 0.4));
            rounded_gradient(
                x,
                531.,
                77.,
                27.,
                6.,
                with_alpha(top, a),
                with_alpha(bottom, a),
            );
            rounded(
                x + 6.,
                537.,
                15.,
                15.,
                3.5,
                Color::new(0., 0., 0., 0.35 * a),
            );
            let ks = ink(11., with_alpha(if active { MINT } else { CREAM }, a))
                .with_align(TextAlign::Center);
            draw_text_styled(
                &(i + 1).to_string(),
                x + 13.5,
                ks.centered_baseline(544.5),
                ks,
            );
            label(name, x + 27., 549., 12., with_alpha(PAPER, a));
        }
    }
    fn draw_hint(&self) {
        let (x, y, w, h) = (36., 176., 452., 346.);
        rounded(
            16.,
            150.,
            490.,
            390.,
            10.,
            Color::new(0.01, 0.02, 0.03, 0.55),
        );
        card(x, y, w, h, BENCH);
        rounded(x, y, w, 4., 2., COPPER);
        let left = x + 22.;
        eyebrow("ENGINEER'S NOTE", left, y + 32., COPPER);
        label(self.content().name, left, y + 60., 22., CREAM);
        let next = paragraph(self.content().hint, left, y + 92., w - 44., 15., PAPER);
        let op = Op::ALL[self.palette];
        let box_top = next + 6.;
        let lines = wrap_text(op.description(), w - 80., ink(13., SOFT)).count() as f32;
        let box_h = 38. + lines * 17.;
        rounded(
            left,
            box_top,
            w - 44.,
            box_h,
            8.,
            Color::new(1., 1., 1., 0.04),
        );
        op_badge(op, left + 12., box_top + 10., 16., false);
        label(op.name(), left + 36., box_top + 23., 14., op_color(op));
        paragraph(
            op.description(),
            left + 12.,
            box_top + 44.,
            w - 68.,
            13.,
            SOFT,
        );
        let mut kx = left;
        kx += keycap("5", "Close", kx, y + h - 38., SOFT) + 20.;
        keycap("H", "Complete manual", kx, y + h - 38., SOFT);
    }
    fn draw_solved(&self) {
        let (x, y, w, h) = (516., 116., 428., 452.);
        surface(x, y, w, h);
        let complete = self.phase == Phase::Complete;
        rounded(x, y, w, 4., 2., MINT);
        let left = x + 22.;
        eyebrow(
            &if complete {
                "ALL TWELVE ROOMS".to_owned()
            } else {
                format!("WORKBENCH {:02} OF 12", self.room())
            },
            left,
            y + 34.,
            MINT,
        );
        label(
            if complete {
                "The factory wakes"
            } else {
                "Workbench restored"
            },
            left,
            y + 66.,
            25.,
            CREAM,
        );
        paragraph("All parcels delivered.", left, y + 92., w - 44., 15., SOFT);
        // Stars land one after another.
        let earned = self.stars[self.level];
        for i in 0..3_u8 {
            let c = vec2(left + 34. + f32::from(i) * 72., y + 152.);
            star(c + vec2(2., 3.), 26., Color::new(0., 0., 0., 0.35));
            star(c, 26., Color::new(0.2, 0.24, 0.28, 1.));
            if i < earned {
                let start = 8 + u16::from(i) * 12;
                let t = if complete {
                    1.
                } else {
                    (self.solved_ticks.saturating_sub(start) as f32 / 10.).min(1.)
                };
                if t > 0. {
                    let pop = 1. + (t * std::f32::consts::PI).sin() * 0.25;
                    shapes::fill_circle(
                        c,
                        40. * t,
                        Fill::radial(
                            c,
                            40. * t,
                            with_alpha(CREAM, 0.25 * t),
                            with_alpha(CREAM, 0.),
                        ),
                    );
                    star(c, 26. * t * pop, CREAM);
                    star(c - vec2(0., 2.), 13. * t * pop, lighten(CREAM, 0.6));
                }
            }
        }
        label(
            match earned {
                3 => "Perfect program",
                2 => "Tidy program",
                _ => "Working program",
            },
            left + 228.,
            y + 150.,
            15.,
            CREAM,
        );
        label(
            &format!("{earned} of 3 stars"),
            left + 228.,
            y + 168.,
            12.,
            MUTED,
        );
        divider(left, y + 204., w - 44., Color::new(1., 1., 1., 0.12));
        let stats = [
            (
                "TAPE CELLS",
                format!("{}", self.tape.len),
                format!("par {}", self.content().solution.len()),
            ),
            ("STEPS RUN", format!("{}", self.steps), "of 160".to_owned()),
            (
                "FAULTS",
                format!("{}", self.failures),
                "this room".to_owned(),
            ),
        ];
        let col = (w - 44.) / 3.;
        for (i, (name, value, note)) in stats.iter().enumerate() {
            let sx = left + i as f32 * col;
            eyebrow(name, sx, y + 234., MUTED);
            label(value, sx, y + 264., 24., CREAM);
            label(note, sx, y + 284., 12., MUTED);
        }
        rounded(
            left,
            y + 306.,
            w - 44.,
            52.,
            8.,
            Color::new(1., 1., 1., 0.04),
        );
        let mid = y + 332.;
        eyebrow(
            "FACTORY TOTAL",
            left + 16.,
            eyebrow_style(MUTED).centered_baseline(mid),
            MUTED,
        );
        label_r(
            &grouped(self.score()),
            x + w - 38.,
            ink(24., MINT).centered_baseline(mid),
            24.,
            MINT,
        );
        // Matches the model's click area (540..930, 494..558).
        let button = Rect::new(540., 500., 390., 50.);
        let base = if complete {
            Color::new(0.16, 0.3, 0.28, 1.)
        } else {
            Color::new(0.2, 0.42, 0.37, 1.)
        };
        shapes::fill_rounded_rect(
            Rect::new(button.x, button.y + 3., button.w, button.h),
            10.,
            Color::new(0., 0., 0., 0.4),
        );
        shapes::fill_rounded_rect(
            button,
            10.,
            Fill::vertical(button, lighten(base, 0.12), base),
        );
        if complete {
            label_c("Twelve workbenches restored", 735., 531., 17., CREAM);
        } else {
            let s = ink(17., CREAM);
            let text_w = text_width("Next workbench", s);
            let total = 52. + 10. + text_w;
            let kx = 735. - total * 0.5;
            keycap("Space", "", kx, 515., CREAM);
            draw_text_styled("Next workbench", kx + 62., s.centered_baseline(525.), s);
        }
    }
}
impl ArcadeGame for Game {
    fn new() -> Self {
        Game::new()
    }
    fn update(&mut self, input: &Input) -> Cue {
        self.step(input)
    }
    fn draw(&self) {
        self.draw_board();
        if matches!(self.phase, Phase::Solved | Phase::Complete) {
            self.draw_solved();
        } else {
            self.draw_editor();
        }
        if self.show_hint {
            self.draw_hint();
        }
    }
    fn hud(&self) -> Hud {
        Hud {
            stage: format!("Room {} / 12", self.room()),
            objective: self.content().lesson.into(),
            status: format!(
                "{}/{} delivered · {} faults · {} stars",
                self.delivered(),
                self.goals(),
                self.failures,
                self.stars.iter().map(|&s| u32::from(s)).sum::<u32>()
            ),
            score: self.score(),
        }
    }
    fn outcome(&self) -> Outcome {
        Game::outcome(self)
    }
    fn summary(&self) -> String {
        Game::summary(self)
    }
    fn bot_input(&self, tick: u64) -> Input {
        self.pilot(tick)
    }
}
