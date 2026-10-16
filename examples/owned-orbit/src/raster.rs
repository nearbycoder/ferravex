use crate::game::{Game, HEIGHT, LEFT, PADDLE_Y, Phase, RIGHT, TOP, WIDTH};
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
            self.rect(x, y, w, h, if block.flash > 0.0 { PAPER } else { shade });
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
        let sprite = self
            .pulse
            .frame(g.time, LoopMode::Repeat)
            .expect("finite time");
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
            self.circle(p[0], p[1], 2.0 + t * 3.0, [99, 215, 196, (t * 80.0) as u8]);
        }
        for p in g.particles.iter() {
            let color = p.color.map(|n| (n.clamp(0.0, 1.0) * 255.0) as u8);
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
        self.text(&format!("SALVAGE / SECTOR 0{}", g.sector), 39, 68, 1, MUTED);
        self.text_right(&format!("{:05}", g.score), 600, 32, 3, GOLD);
        self.text_right(
            &format!("SCORE / X{}", g.chain.multiplier()),
            599,
            68,
            1,
            MUTED,
        );
        for i in 0..3 {
            self.circle(
                46.0 + i as f32 * 17.0,
                682.0,
                3.0,
                if i < g.lives { CYAN } else { LINE },
            );
        }
        self.text(&format!("{:02} PANELS", g.remaining()), 104, 679, 1, MUTED);
        self.text_right("A D / ARROWS / DRAG", 600, 679, 1, MUTED);
        match g.phase {
            Phase::Ready => self.overlay(
                if g.lives == 3 && g.sector == 1 && g.clears == 0 {
                    "ORBIT BREAKER"
                } else {
                    "READY FOR LAUNCH"
                },
                "BREAK THE PANELS. CATCH THE RETURN.",
                "SPACE / TAP TO LAUNCH",
            ),
            Phase::Won => self.overlay(
                "FIELD CLEARED",
                &format!("{} POINTS / {:.0} SECONDS", g.score, g.time),
                "SPACE / TAP TO RESTART",
            ),
            Phase::Lost => self.overlay(
                "SIGNAL LOST",
                &format!("{} POINTS / {} PANELS CLEARED", g.score, g.clears),
                "SPACE / TAP TO RESTART",
            ),
            Phase::Transition => self.overlay(
                "SECTOR SECURED",
                "REINFORCED DEBRIS AHEAD",
                "PREPARING SECTOR 02",
            ),
            Phase::Playing => {}
        }
    }
    fn overlay(&mut self, title: &str, detail: &str, action: &str) {
        self.rect(104, 379, 432, 161, [6, 15, 21, 238]);
        self.rect(104, 379, 432, 1, LINE);
        self.rect(104, 539, 432, 1, LINE);
        self.rect(302, 395, 36, 2, CYAN);
        self.text_center(title, 415, 2, PAPER);
        self.text_center(detail, 451, 1, MUTED);
        self.text_center(action, 493, 1, CYAN);
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
    text.chars().count() as i32 * 6 * scale - scale
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
        '-' => [0, 0, 0, 31, 0, 0, 0],
        '.' => [0, 0, 0, 0, 0, 12, 12],
        _ => [0; 7],
    }
}
