use crate::game::{Feedback, Game, HEIGHT, LEFT, PADDLE_Y, Phase, RIGHT, TOP, WIDTH};
use ferravex_engine::{
    graphics::{Color, Framebuffer},
    play::animation::{LoopMode, SpriteClip, SpriteFrame},
};

const INK: [u8; 4] = [9, 18, 25, 255];
const LINE: [u8; 4] = [42, 65, 72, 255];
const PAPER: [u8; 4] = [225, 234, 218, 255];
const MUTED: [u8; 4] = [120, 148, 151, 255];
const CYAN: [u8; 4] = [99, 215, 196, 255];
const GOLD: [u8; 4] = [216, 173, 109, 255];

pub struct Canvas {
    pub frame: Framebuffer,
    pulse: SpriteClip,
    look: ferravex_engine::graphics::Look,
}
impl Default for Canvas {
    fn default() -> Self {
        Self::new()
    }
}
impl Canvas {
    pub fn new() -> Self {
        Self {
            look: Default::default(),
            frame: Framebuffer::new(WIDTH, HEIGHT).expect("fixed canvas"),
            pulse: SpriteClip::new(
                (0..4)
                    .map(|index| SpriteFrame {
                        index,
                        duration: 0.12,
                    })
                    .collect(),
            )
            .expect("valid clip"),
        }
    }
    pub fn render(&mut self, g: &Game) {
        self.render_session(g, false, &crate::presentation::Presentation::default());
    }
    pub(crate) fn render_session(
        &mut self,
        g: &Game,
        paused: bool,
        presentation: &crate::presentation::Presentation,
    ) {
        self.look = ferravex_engine::render::look();
        self.frame.clear(self.look.shade(Color::from_hex(0x091219)));
        for y in TOP as i32..654 {
            let tint = ((y - TOP as i32) as f32 / 550.0 * 5.0) as u8;
            self.rect(
                LEFT as i32,
                y,
                (RIGHT - LEFT) as i32,
                1,
                [13 + tint, 26 + tint, 33 + tint, 255],
            );
        }
        for i in 0..80 {
            let x = 49 + (i * 149 + 43) % 542;
            let y = 116 + (i * 97 + 19) % 522;
            self.rect(x, y, 1, 1, [54, 75, 79, if i % 7 == 0 { 170 } else { 85 }]);
        }
        for r in [100.0, 180.0, 260.0] {
            self.ring(320.0, 405.0, r, 1.0, [36, 57, 64, 150]);
        }
        self.line(320.0, TOP, 320.0, 653.0, [34, 52, 58, 100]);
        self.line(LEFT, 405.0, RIGHT, 405.0, [34, 52, 58, 100]);
        self.line(LEFT, TOP, LEFT, 653.0, LINE);
        self.line(RIGHT, TOP, RIGHT, 653.0, LINE);
        self.line(LEFT, TOP, RIGHT, TOP, LINE);
        for x in [LEFT, RIGHT] {
            self.rect(x as i32 - 2, TOP as i32, 4, 24, CYAN);
            self.rect(x as i32 - 2, 630, 4, 24, CYAN);
            for y in (360..420).step_by(12) {
                self.line(x - 3.0, y as f32, x + 3.0, y as f32 + 5.0, GOLD);
            }
        }
        for block in g.blocks.iter().filter(|b| b.health > 0) {
            let x = (block.center[0] - block.half[0]) as i32;
            let y = (block.center[1] - block.half[1]) as i32;
            let w = (block.half[0] * 2.0) as i32;
            let h = (block.half[1] * 2.0) as i32;
            let shade = match block.row {
                0 => [211, 173, 117, 255],
                1 => [183, 153, 111, 255],
                2 => [139, 156, 141, 255],
                3 => [100, 157, 152, 255],
                _ => [80, 133, 140, 255],
            };
            self.rect(x + 2, y + 4, w, h, [3, 10, 15, 180]);
            let mut panel = shade;
            if block.flash > 0.0 && !g.policy.reduced_flash() {
                for channel in 0..3 {
                    panel[channel] = (f32::from(shade[channel])
                        + (f32::from(PAPER[channel]) - f32::from(shade[channel]))
                            * g.policy.intensity()) as u8;
                }
            }
            self.rect(x, y, w, h, panel);
            self.rect(x + 1, y + 1, w - 2, 2, [240, 240, 211, 80]);
            self.rect(x + 2, y + h - 5, w - 4, 4, [3, 18, 22, 45]);
            self.rect(x + 5, y + 5, 2, 2, [9, 27, 30, 120]);
            self.rect(x + w - 7, y + 5, 2, 2, [9, 27, 30, 120]);
            if block.original_health == 2 {
                self.line(
                    x as f32 + 12.0,
                    y as f32 + 8.0,
                    x as f32 + 20.0,
                    y as f32 + 19.0,
                    [20, 40, 44, 150],
                );
                self.line(
                    x as f32 + w as f32 - 12.0,
                    y as f32 + 8.0,
                    x as f32 + w as f32 - 20.0,
                    y as f32 + 19.0,
                    [20, 40, 44, 150],
                );
                if block.health == 1 {
                    self.line(
                        block.center[0],
                        y as f32 + 2.0,
                        block.center[0] - 4.0,
                        y as f32 + 12.0,
                        INK,
                    );
                    self.line(
                        block.center[0] - 4.0,
                        y as f32 + 12.0,
                        block.center[0] + 3.0,
                        y as f32 + h as f32 - 1.0,
                        INK,
                    );
                }
            }
        }
        let sprite = if g.policy.reduced_motion() || g.policy.intensity() == 0. {
            0
        } else {
            self.pulse
                .frame(g.time, LoopMode::Repeat)
                .expect("finite time")
        };
        for drop in &g.drops {
            let [x, y] = drop.position;
            self.rect(x as i32 - 9, y as i32 - 9, 18, 18, [30, 76, 70, 255]);
            self.rect(x as i32 - 6, y as i32 - 2, 12, 4, CYAN);
            let shift = sprite as f32;
            self.line(x - 11.0 - shift, y - 4.0, x - 15.0 - shift, y, CYAN);
            self.line(x - 15.0 - shift, y, x - 11.0 - shift, y + 4.0, CYAN);
            self.line(x + 11.0 + shift, y - 4.0, x + 15.0 + shift, y, CYAN);
            self.line(x + 15.0 + shift, y, x + 11.0 + shift, y + 4.0, CYAN);
        }
        for (i, p) in g.trail.iter().enumerate() {
            let t = (i + 1) as f32 / (g.trail.len() + 1) as f32;
            self.circle(
                p[0],
                p[1],
                2.0 + t * 3.0,
                [99, 215, 196, (t * 80.0 * g.policy.intensity()) as u8],
            );
        }
        for p in g.particles.iter() {
            let mut color = p.color.map(|n| (n.clamp(0.0, 1.0) * 255.0) as u8);
            color[3] = (f32::from(color[3]) * g.policy.intensity()) as u8;
            self.rect(
                p.position[0] as i32,
                p.position[1] as i32,
                p.size.ceil() as i32,
                p.size.ceil() as i32,
                color,
            );
        }
        let x = (g.paddle_x - g.paddle_half) as i32;
        let w = (g.paddle_half * 2.0) as i32;
        self.rect(x + 3, PADDLE_Y as i32 - 2, w, 15, [3, 9, 13, 200]);
        self.rect(x, PADDLE_Y as i32 - 8, w, 16, [51, 81, 85, 255]);
        self.rect(x + 2, PADDLE_Y as i32 - 8, w - 4, 3, CYAN);
        self.rect(x + 9, PADDLE_Y as i32 - 4, w - 18, 5, [188, 214, 201, 255]);
        self.rect(x + 5, PADDLE_Y as i32 + 3, 4, 3, GOLD);
        self.rect(x + w - 9, PADDLE_Y as i32 + 3, 4, 3, GOLD);
        let emphasis = presentation.feedback.impact().emphasis;
        if emphasis > 0.01 {
            self.rect(
                x,
                PADDLE_Y as i32 - 11,
                w,
                (emphasis * 6.).ceil() as i32,
                [99, 215, 196, 130],
            );
        }
        if g.wide_time > 0.0 {
            self.rect(
                x,
                PADDLE_Y as i32 + 11,
                (w as f32 * g.wide_time / 9.0) as i32,
                2,
                CYAN,
            );
        }
        if !matches!(g.phase, Phase::Won | Phase::Lost | Phase::Transition) {
            self.circle(g.ball[0], g.ball[1], 12.0, [99, 215, 196, 16]);
            self.circle(g.ball[0], g.ball[1], 8.0, [99, 215, 196, 65]);
            self.circle(g.ball[0], g.ball[1], 5.8, PAPER);
            self.circle(g.ball[0] - 1.0, g.ball[1] - 1.0, 2.5, [255, 255, 241, 255]);
        }
        self.look = Default::default();
        self.text("ORBIT BREAKER", 38, 33, 3, PAPER);
        self.text(
            &format!("SECTOR 0{} / PRECISE {}", g.sector, g.precision.streak()),
            39,
            68,
            1,
            MUTED,
        );
        self.text_right(&format!("{:05}", g.score), 600, 32, 3, GOLD);
        self.text_right(
            &format!("SCORE / X{}", g.chain.multiplier()),
            599,
            68,
            1,
            MUTED,
        );
        if g.armor_piercing() {
            self.text("ARMOR PIERCING / UNTIL PADDLE RETURN", 39, 88, 1, GOLD);
        }
        if g.wide_time > 0. {
            self.text_right(&format!("WIDE {:.1}S", g.wide_time), 600, 88, 1, CYAN);
        }
        for i in 0..3 {
            self.circle(
                46.0 + i as f32 * 17.0,
                682.0,
                3.0,
                if i < g.lives { CYAN } else { LINE },
            );
        }
        self.text(&format!("{:02} PANELS", g.remaining()), 104, 679, 1, MUTED);
        if g.ball_save > 0. && g.phase == Phase::Playing {
            self.text("BALL SAVE", 270, 679, 1, CYAN);
        }
        match g.phase {
            Phase::Ready => {
                let title = if g.feedback() == Some(Feedback::LifeLost) {
                    format!(
                        "BALL LOST / {} {}",
                        g.lives,
                        if g.lives == 1 { "LIFE" } else { "LIVES" }
                    )
                } else if g.feedback() == Some(Feedback::Restarted) {
                    "FRESH RUN / 3 LIVES".into()
                } else if g.lives == 3 && g.sector == 1 && g.clears == 0 {
                    "ORBIT BREAKER".into()
                } else {
                    "READY FOR LAUNCH".into()
                };
                self.overlay(
                    &title,
                    &format!("CHARGE {:03}%", (g.serve_charge.power() * 100.) as u32),
                    &format!("{} TO LAUNCH", presentation.hints.launch),
                );
                self.rect(180, 472, 280, 3, LINE);
                self.rect(180, 472, (280. * g.serve_charge.power()) as i32, 3, CYAN);
            }
            Phase::Won => self.overlay(
                "FIELD CLEARED",
                &format!("{} POINTS / {:.0} SECONDS", g.score, g.time),
                &format!("{} TO RETRY", presentation.hints.launch),
            ),
            Phase::Lost => self.overlay(
                "SIGNAL LOST",
                &format!("{} POINTS / {} PANELS CLEARED", g.score, g.clears),
                &format!("{} TO RETRY", presentation.hints.launch),
            ),
            Phase::Transition => {
                self.overlay(
                    "SECTOR 01 SECURED",
                    "REINFORCED PANELS NEED TWO HITS",
                    &format!("SECTOR 02 IN {:.1} SECONDS", g.transition_remaining()),
                );
                self.rect(180, 517, 280, 3, LINE);
                let progress = (1. - g.transition_remaining() / 1.4).clamp(0., 1.);
                self.rect(180, 517, (280. * progress) as i32, 3, CYAN);
            }
            Phase::Playing => {
                let message = match g.feedback() {
                    Some(Feedback::Launched) => "",
                    Some(Feedback::BallSaved) => "BALL SAVED",
                    _ => "",
                };
                self.text_center(message, 553, 1, CYAN);
                if let Some((label, points)) = presentation.notice {
                    self.text_center(&format!("{label} +{points}"), 573, 1, GOLD);
                }
            }
        }
        if matches!(g.phase, Phase::Won | Phase::Lost) {
            self.text_center(
                &format!(
                    "BEST PRECISE {} / BALL SAVES {}",
                    g.precision.best(),
                    g.saves
                ),
                474,
                1,
                GOLD,
            );
        }
        if paused {
            self.overlay(
                "PAUSED",
                &format!(
                    "SECTOR {} / {} POINTS / {} LIVES",
                    g.sector, g.score, g.lives
                ),
                &format!("{} TO RESUME", presentation.hints.pause),
            );
        }
    }
    fn overlay(&mut self, title: &str, detail: &str, action: &str) {
        self.rect(104, 379, 432, 161, [6, 15, 21, 238]);
        self.rect(104, 379, 432, 1, LINE);
        self.rect(104, 539, 432, 1, LINE);
        self.rect(302, 395, 36, 2, CYAN);
        self.text_fit_center(title, 415, 2, 408, PAPER);
        self.text_fit_center(detail, 451, 1, 408, MUTED);
        self.text_fit_center(action, 493, 1, 408, CYAN);
    }
    fn text_fit_center(&mut self, text: &str, y: i32, scale: i32, width: i32, color: [u8; 4]) {
        let scale = if scale > 1 && text_width(text, scale) > width {
            1
        } else {
            scale
        };
        let fitted = fit_label(text, scale, width);
        self.text_center(&fitted, y, scale, color);
    }
    fn text_center(&mut self, t: &str, y: i32, s: i32, c: [u8; 4]) {
        self.text(t, (WIDTH as i32 - text_width(t, s)) / 2, y, s, c);
    }
    fn text_right(&mut self, t: &str, x: i32, y: i32, s: i32, c: [u8; 4]) {
        self.text(t, x - text_width(t, s), y, s, c);
    }
    fn text(&mut self, text: &str, mut x: i32, y: i32, scale: i32, color: [u8; 4]) {
        let look = self.look;
        self.look = Default::default();
        for letter in text.chars() {
            let rows = glyph(letter);
            for (row, bits) in rows.into_iter().enumerate() {
                for col in 0..5 {
                    if bits & (1 << (4 - col)) != 0 {
                        self.rect(x + col * scale, y + row as i32 * scale, scale, scale, color);
                    }
                }
            }
            x += 6 * scale;
        }
        self.look = look;
    }
    fn tint(&self, color: [u8; 4]) -> [u8; 4] {
        self.look
            .shade(Color::from_rgba(color[0], color[1], color[2], color[3]))
            .to_rgba()
    }
    fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: [u8; 4]) {
        let color = self.tint(color);
        if color[3] == 255 {
            let left = x.clamp(0, WIDTH as i32) as usize;
            let right = x.saturating_add(w).clamp(left as i32, WIDTH as i32) as usize;
            for py in y.max(0)..y.saturating_add(h).min(HEIGHT as i32) {
                self.frame.pixels_mut()
                    [py as usize * WIDTH as usize + left..py as usize * WIDTH as usize + right]
                    .fill(color);
            }
            return;
        }
        for py in y.max(0)..(y + h).min(HEIGHT as i32) {
            for px in x.max(0)..(x + w).min(WIDTH as i32) {
                self.pixel(px, py, color);
            }
        }
    }
    fn pixel(&mut self, x: i32, y: i32, color: [u8; 4]) {
        if !(0..WIDTH as i32).contains(&x) || !(0..HEIGHT as i32).contains(&y) {
            return;
        }
        let dst = &mut self.frame.pixels_mut()[(y as u32 * WIDTH + x as u32) as usize];
        let alpha = u32::from(color[3]);
        for i in 0..3 {
            dst[i] =
                ((u32::from(color[i]) * alpha + u32::from(dst[i]) * (255 - alpha)) / 255) as u8;
        }
        dst[3] = 255;
    }
    fn circle(&mut self, x: f32, y: f32, r: f32, color: [u8; 4]) {
        let color = self.tint(color);
        for py in (y - r - 1.0) as i32..=(y + r + 1.0) as i32 {
            for px in (x - r - 1.0) as i32..=(x + r + 1.0) as i32 {
                let d = (px as f32 + 0.5 - x).hypot(py as f32 + 0.5 - y);
                let coverage = (r + 0.5 - d).clamp(0.0, 1.0);
                if coverage > 0.0 {
                    let mut c = color;
                    c[3] = (f32::from(c[3]) * coverage) as u8;
                    self.pixel(px, py, c);
                }
            }
        }
    }
    fn ring(&mut self, x: f32, y: f32, r: f32, thickness: f32, color: [u8; 4]) {
        let steps = (r * 7.0) as i32;
        for i in 0..steps {
            let a = i as f32 / steps as f32 * std::f32::consts::TAU;
            self.circle(x + a.cos() * r, y + a.sin() * r, thickness * 0.5, color);
        }
    }
    fn line(&mut self, x: f32, y: f32, end_x: f32, end_y: f32, color: [u8; 4]) {
        let color = self.tint(color);
        let steps = (end_x - x).abs().max((end_y - y).abs()).ceil() as i32;
        if steps == 0 {
            self.pixel(x as i32, y as i32, color);
            return;
        }
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            self.pixel(
                (x + (end_x - x) * t).round() as i32,
                (y + (end_y - y) * t).round() as i32,
                color,
            );
        }
    }
}
fn text_width(text: &str, scale: i32) -> i32 {
    let metrics = ferravex_engine::render::get_default_font()
        .try_metrics(8, scale as f32)
        .expect("positive bitmap scale");
    (metrics.measure_line(text).expect("bounded HUD label").width as i32 - scale).max(0)
}
fn fit_label(text: &str, scale: i32, width: i32) -> std::borrow::Cow<'_, str> {
    if text_width(text, scale) <= width {
        return std::borrow::Cow::Borrowed(text);
    }
    let cells = ((width + scale) / (6 * scale)).max(3) as usize;
    let mut fitted: String = text.chars().take(cells - 3).collect();
    fitted.push_str("...");
    std::borrow::Cow::Owned(fitted)
}
fn glyph(c: char) -> [u8; 7] {
    match c {
        'A' => [14, 17, 17, 31, 17, 17, 17],
        'B' => [30, 17, 17, 30, 17, 17, 30],
        'C' => [14, 17, 16, 16, 16, 17, 14],
        'D' => [30, 17, 17, 17, 17, 17, 30],
        'E' => [31, 16, 16, 30, 16, 16, 31],
        'F' => [31, 16, 16, 30, 16, 16, 16],
        'G' => [14, 17, 16, 23, 17, 17, 15],
        'H' => [17, 17, 17, 31, 17, 17, 17],
        'I' => [31, 4, 4, 4, 4, 4, 31],
        'J' => [7, 2, 2, 2, 18, 18, 12],
        'K' => [17, 18, 20, 24, 20, 18, 17],
        'L' => [16, 16, 16, 16, 16, 16, 31],
        'M' => [17, 27, 21, 21, 17, 17, 17],
        'N' => [17, 25, 21, 19, 17, 17, 17],
        'O' => [14, 17, 17, 17, 17, 17, 14],
        'P' => [30, 17, 17, 30, 16, 16, 16],
        'Q' => [14, 17, 17, 17, 21, 18, 13],
        'R' => [30, 17, 17, 30, 20, 18, 17],
        'S' => [15, 16, 16, 14, 1, 1, 30],
        'T' => [31, 4, 4, 4, 4, 4, 4],
        'U' => [17, 17, 17, 17, 17, 17, 14],
        'V' => [17, 17, 17, 17, 17, 10, 4],
        'W' => [17, 17, 17, 21, 21, 21, 10],
        'X' => [17, 17, 10, 4, 10, 17, 17],
        'Y' => [17, 17, 10, 4, 4, 4, 4],
        'Z' => [31, 1, 2, 4, 8, 16, 31],
        '0' => [14, 17, 19, 21, 25, 17, 14],
        '1' => [4, 12, 4, 4, 4, 4, 14],
        '2' => [14, 17, 1, 2, 4, 8, 31],
        '3' => [30, 1, 1, 14, 1, 1, 30],
        '4' => [2, 6, 10, 18, 31, 2, 2],
        '5' => [31, 16, 16, 30, 1, 1, 30],
        '6' => [14, 16, 16, 30, 17, 17, 14],
        '7' => [31, 1, 2, 4, 8, 8, 8],
        '8' => [14, 17, 17, 14, 17, 17, 14],
        '9' => [14, 17, 17, 15, 1, 1, 14],
        '/' => [1, 1, 2, 4, 8, 16, 16],
        '+' => [0, 4, 4, 31, 4, 4, 0],
        ':' => [0, 12, 12, 0, 12, 12, 0],
        '-' => [0, 0, 0, 31, 0, 0, 0],
        '.' => [0, 0, 0, 0, 0, 12, 12],
        '%' => [25, 26, 2, 4, 8, 11, 19],
        _ => [0; 7],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presentation::Presentation;
    use ferravex_engine::play::feedback::FeedbackPolicy;

    #[test]
    fn legacy_retry_glyph_geometry_explains_why_narrow_status_uses_native_dom_text() {
        let mut canvas = Canvas::new();
        canvas.frame.clear(Color::from_hex(0x091219));
        canvas.text("J", 0, 0, 1, PAPER);
        let lit: Vec<_> = canvas
            .frame
            .pixels()
            .iter()
            .enumerate()
            .filter(|(_, pixel)| **pixel == PAPER)
            .map(|(index, _)| (index % WIDTH as usize, index / WIDTH as usize))
            .collect();
        assert_eq!(lit.iter().map(|p| p.0).max(), Some(4));
        assert_eq!(lit.iter().map(|p| p.1).max(), Some(6));
        assert_eq!(glyph('J'), [7, 2, 2, 2, 18, 18, 12]);
        for css_width in [300.796875_f64, 320., 327.109375, 366.59375, 390.] {
            let glyph_height = 7. * css_width / f64::from(WIDTH);
            assert!(glyph_height < 4.3, "{glyph_height}");
        }
        assert_eq!((canvas.frame.width(), canvas.frame.height()), (640, 720));
    }

    #[test]
    fn shared_measurements_fit_labels_inside_the_panel_and_empty_labels_have_zero_width() {
        assert_eq!(text_width("", 1), 0);
        assert_eq!(text_width("ORBIT BREAKER", 3), 231);
        for label in [
            "SPACE / ENTER / MOUSE 1 TO LAUNCH".to_owned(),
            "PAD 10 / ".repeat(64),
        ] {
            let fitted = fit_label(&label, 1, 408);
            assert!(text_width(&fitted, 1) <= 408);
            let mut canvas = Canvas::new();
            canvas.frame.clear(Color::from_hex(0x091219));
            canvas.text_center(&fitted, 493, 1, CYAN);
            for (i, pixel) in canvas.frame.pixels().iter().enumerate() {
                if *pixel == CYAN {
                    assert!((116..524).contains(&(i % WIDTH as usize)));
                }
            }
        }
    }

    #[test]
    fn reduced_motion_stops_capsule_pulses_and_pause_is_visible_in_the_software_frame() {
        let mut game = Game::new();
        game.drops.push(crate::game::Drop {
            position: [320., 340.],
        });
        game.apply_policy(FeedbackPolicy::new(true, false, 1.).unwrap());
        let mut canvas = Canvas::new();
        let presentation = Presentation::default();
        canvas.render_session(&game, false, &presentation);
        let still = canvas.frame.pixels().to_vec();
        game.time = 0.25;
        canvas.render_session(&game, false, &presentation);
        assert_eq!(canvas.frame.pixels(), still);
        canvas.render_session(&game, true, &presentation);
        assert_ne!(canvas.frame.pixels(), still);
        assert!(canvas.frame.pixels().iter().all(|p| p[3] == 255));
        game.apply_policy(FeedbackPolicy::default());
        canvas.render_session(&game, false, &presentation);
        assert_ne!(canvas.frame.pixels(), still);
    }
}
