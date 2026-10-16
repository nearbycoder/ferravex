//! A small precision platformer: no assets, a fixed simulation, and forgiving jumps.
mod animation;
use animation::EmberAnimation;
use ferravex_core::FixedClock;
use ferravex_physics::character::{
    Character, CharacterInput, CharacterSettings, CharacterShape, Obstacle, QueryBounds, Terrain,
};
use ferravex_play::camera::{Follow2, Follow2Settings, Limits2};
use ferravex_render::audio::{Cue, GameAudio, Surface};
use ferravex_render::input::{
    PlayerInput, Preset,
    touch::{Control, Output, Region},
};
use ferravex_render::{demo::Demo, prelude::*};

const WIDTH: f32 = 1280.0;
const HEIGHT: f32 = 720.0;
const WORLD_END: f32 = 2700.0;
const PLAYER: Vec2 = Vec2::new(24.0, 40.0);
const SPEED: f32 = 280.0;
const GRAVITY: f32 = 1600.0;
const JUMP: f32 = 660.0;
const COYOTE: f32 = 0.10;
const BUFFER: f32 = 0.12;
const ORANGE: Color = Color::new(1.0, 0.54, 0.25, 1.0);
const CREAM: Color = Color::new(0.96, 0.88, 0.72, 1.0);
const GREEN: Color = Color::new(0.48, 0.68, 0.48, 1.0);

#[derive(Default, Clone, Copy)]
struct Input {
    direction: f32,
    jump: bool,
    jump_held: bool,
}

struct Game {
    motor: Character,
    platforms: Vec<Rect>,
    hazards: Vec<Rect>,
    embers: Vec<(Vec2, bool)>,
    checkpoint: Vec2,
    checkpoint_lit: bool,
    won: bool,
    deaths: u32,
    score: ferravex_play::gameplay::combo::ScoreChain,
    jumps: u32,
    ember_inventory: ferravex_play::gameplay::inventory::Inventory<1>,
    air_jump_used: bool,
    jump_down: bool,
    springs: [ferravex_play::gameplay::platforms::LaunchPad; 3],
    spring_flight: bool,
    crumbles: [ferravex_play::gameplay::platforms::CrumblePlatform; 2],
    elapsed: f32,
    trial: ferravex_play::gameplay::trial::Trial<3>,
    respawn_flash: f32,
}

impl Game {
    fn new() -> Self {
        Self {
            motor: Character::new(feet(vec2(90.0, 520.0))),
            platforms: vec![
                Rect::new(-100.0, 560.0, 680.0, 250.0),
                Rect::new(700.0, 560.0, 500.0, 250.0),
                Rect::new(1320.0, 560.0, 500.0, 250.0),
                Rect::new(1940.0, 560.0, 850.0, 250.0),
                Rect::new(140.0, 445.0, 80.0, 18.0),
                Rect::new(770.0, 435.0, 130.0, 18.0),
                Rect::new(1350.0, 430.0, 120.0, 18.0),
                Rect::new(2010.0, 430.0, 120.0, 18.0),
                Rect::new(380., 330., 110., 18.),
                Rect::new(1640., 315., 110., 18.),
            ],
            hazards: vec![
                Rect::new(350.0, 542.0, 44.0, 18.0),
                Rect::new(1040.0, 542.0, 56.0, 18.0),
                Rect::new(1600.0, 542.0, 52.0, 18.0),
                Rect::new(2230.0, 542.0, 56.0, 18.0),
            ],
            embers: [
                (180.0, 406.0),
                (375.0, 455.0),
                (640.0, 470.0),
                (835.0, 395.0),
                (1070.0, 455.0),
                (1260.0, 470.0),
                (1410.0, 390.0),
                (1880.0, 470.0),
                (2070.0, 390.0),
                (2430.0, 520.0),
                (425., 300.),
                (1695., 285.),
            ]
            .into_iter()
            .map(|(x, y)| (vec2(x, y), false))
            .collect(),
            checkpoint: vec2(90.0, 520.0),
            checkpoint_lit: false,
            won: false,
            deaths: 0,
            score: ferravex_play::gameplay::combo::ScoreChain::new(5., 2, 4).unwrap(),
            jumps: 0,
            ember_inventory: ferravex_play::gameplay::inventory::Inventory::new().unwrap(),
            air_jump_used: false,
            jump_down: false,
            springs: [ferravex_play::gameplay::platforms::LaunchPad::new(780., 0.25).unwrap(); 3],
            spring_flight: false,
            crumbles: [ferravex_play::gameplay::platforms::CrumblePlatform::new(0.65, 3.).unwrap();
                2],
            elapsed: 0.0,
            trial: {
                let mut t = ferravex_play::gameplay::trial::Trial::new([12., 18., 30.]).unwrap();
                t.start();
                t
            },
            respawn_flash: 0.0,
        }
    }

    fn position(&self) -> Vec2 {
        vec2(
            self.motor.position.x - PLAYER.x * 0.5,
            -self.motor.position.y - PLAYER.y,
        )
    }
    fn velocity(&self) -> Vec2 {
        vec2(self.motor.velocity.x, -self.motor.velocity.y)
    }
    fn body(&self) -> Rect {
        Rect::new(self.position().x, self.position().y, PLAYER.x, PLAYER.y)
    }

    fn respawn(&mut self) {
        self.motor = Character::new(feet(self.checkpoint));
        self.air_jump_used = false;
        self.jump_down = false;
        self.spring_flight = false;
        self.deaths += 1;
        self.score.break_chain();
        self.respawn_flash = 0.6;
    }

    fn update(&mut self, input: Input, dt: f32) {
        if self.won {
            return;
        }
        self.elapsed += dt;
        self.trial.advance(dt).unwrap();
        self.score.advance(dt).unwrap();
        self.respawn_flash = (self.respawn_flash - dt).max(0.0);
        for (mechanism, surface) in self.crumbles.iter_mut().zip(&self.platforms[8..10]) {
            let contact = self.motor.grounded
                && (-self.motor.position.y - surface.y).abs() < 1.
                && (self.motor.position.x - surface.center().x).abs() < surface.w * 0.5;
            mechanism.step(dt, contact).unwrap();
        }
        // A side platform moves before the motor, so standing passengers inherit its pose.
        self.platforms[5].x = 770.0 + (self.elapsed * 1.2).sin() * 28.0;
        let jump_edge = input.jump && !self.jump_down;
        self.jump_down = input.jump;
        let result = self
            .motor
            .step(
                &EmberTerrain(&self.platforms, self.crumbles.map(|c| !c.solid())),
                CharacterShape::Box2 { half: PLAYER * 0.5 },
                CharacterSettings {
                    speed: SPEED,
                    gravity: GRAVITY,
                    jump_speed: JUMP,
                    terminal_speed: 1000.,
                    ground_acceleration: 2400.,
                    air_acceleration: 1600.,
                    coyote_time: COYOTE,
                    jump_buffer: BUFFER,
                    ground_snap: 0.1,
                    step_height: 0.,
                    max_recovery: 24.,
                    ..Default::default()
                },
                CharacterInput {
                    direction: vec3(input.direction, 0., 0.),
                    jump: input.jump,
                },
                dt,
            )
            .expect("authored Emberline character terrain");
        self.jumps += u32::from(result.jumped);
        if self.motor.grounded {
            self.air_jump_used = false;
        }
        if jump_edge
            && !result.jumped
            && !self.motor.grounded
            && self.motor.jump.coyote == 0.
            && !self.air_jump_used
            && self.ember_inventory.count(1) >= 3
        {
            self.motor.velocity.y = JUMP * 0.85;
            self.motor.jump.buffer = 0.;
            self.air_jump_used = true;
            self.jumps += 1;
        }
        if self.motor.grounded {
            self.spring_flight = false;
        }
        for (pad, platform) in self.springs.iter_mut().zip([4, 6, 7]) {
            let surface = self.platforms[platform];
            let contact = self.motor.grounded
                && self.motor.velocity.y <= 0.
                && (self.motor.position.x - surface.center().x).abs() < 25.
                && (-self.motor.position.y - surface.y).abs() < 1.;
            if let Some(speed) = pad.step(dt, contact).unwrap() {
                self.motor.velocity.y = speed;
                self.motor.grounded = false;
                self.motor.jump.coyote = 0.;
                self.motor.jump.buffer = 0.;
                self.spring_flight = true;
            }
        }
        // Release makes a short hop; holding keeps the full gap-clearing arc.
        // Cutting velocity once avoids applying a frame-rate-dependent force.
        if !self.spring_flight
            && !result.jumped
            && !input.jump_held
            && !input.jump
            && self.motor.velocity.y > JUMP * 0.45
        {
            self.motor.velocity.y = JUMP * 0.45;
        }
        self.motor.position.x = self
            .motor
            .position
            .x
            .clamp(PLAYER.x * 0.5, WORLD_END - PLAYER.x * 0.5);
        if self.position().y > 780.0 || self.hazards.iter().any(|h| self.body().overlaps(h)) {
            self.respawn();
            return;
        }
        let body = self.body();
        for (position, collected) in &mut self.embers {
            if !*collected
                && body.overlaps(&Rect::new(position.x - 12.0, position.y - 12.0, 24.0, 24.0))
            {
                *collected = true;
                self.score.award(100);
                self.ember_inventory
                    .add(ferravex_play::gameplay::inventory::Stack {
                        item: 1,
                        amount: 1,
                        limit: 32,
                    })
                    .unwrap();
            }
        }
        if self.position().x > 1380.0 && self.position().x < 1490.0 && self.motor.grounded {
            self.checkpoint_lit = true;
            self.checkpoint = vec2(1410.0, 520.0);
        }
        if self.position().x >= 900. && self.trial.splits()[0].is_none() {
            self.trial.split(0).unwrap();
        }
        if self.position().x >= 1800. && self.trial.splits()[1].is_none() {
            self.trial.split(1).unwrap();
        }
        if self.position().x > 2560.0 && self.motor.grounded {
            self.trial.split(2).unwrap();
            self.won = true;
        }
    }

    fn autoplay(&self) -> Input {
        let front = self.position().x + PLAYER.x;
        let hazard = self
            .hazards
            .iter()
            .any(|h| h.x >= front - 2.0 && h.x - front < 74.0);
        let gap = self
            .platforms
            .iter()
            .filter(|p| p.y == 560.0)
            .any(|p| front > p.x && front <= p.right() && p.right() - front < 58.0);
        Input {
            direction: 1.0,
            jump: self.motor.grounded && (hazard || gap),
            jump_held: true,
        }
    }
}

fn feet(position: Vec2) -> Vec3 {
    vec3(position.x + PLAYER.x * 0.5, -position.y - PLAYER.y, 0.)
}
struct EmberTerrain<'a>(&'a [Rect], [bool; 2]);
impl EmberTerrain<'_> {
    fn obstacle_at(&self, i: usize) -> Obstacle {
        let p = self.0[i];
        let mut obstacle = Obstacle::cuboid(
            i as u64,
            vec3(p.x + p.w * 0.5, -p.y - p.h * 0.5, 0.),
            vec3(p.w * 0.5, p.h * 0.5, 1.),
        );
        obstacle.sensor = (8..10).contains(&i) && self.1[i - 8];
        obstacle
    }
}
impl Terrain for EmberTerrain<'_> {
    fn visit(&self, bounds: QueryBounds, visitor: &mut dyn FnMut(Obstacle)) {
        for i in 0..self.0.len() {
            let o = self.obstacle_at(i);
            if bounds.intersects(o.bounds()) {
                visitor(o);
            }
        }
    }
    fn obstacle(&self, id: u64) -> Option<Obstacle> {
        ((id as usize) < self.0.len()).then(|| self.obstacle_at(id as usize))
    }
}

fn diamond(position: Vec2, radius: f32, color: Color) {
    draw_triangle(
        position + vec2(0.0, -radius),
        position + vec2(radius * 0.65, 0.0),
        position + vec2(0.0, radius),
        color,
    );
    draw_triangle(
        position + vec2(0.0, -radius),
        position + vec2(0.0, radius),
        position - vec2(radius * 0.65, 0.0),
        Color::new(color.r * 0.7, color.g * 0.7, color.b * 0.7, color.a),
    );
}

fn tree(x: f32, y: f32, size: f32, color: Color) {
    draw_rectangle(
        x - size * 0.04,
        y - size * 0.45,
        size * 0.08,
        size * 0.45,
        color,
    );
    draw_triangle(
        vec2(x, y - size),
        vec2(x - size * 0.35, y - size * 0.2),
        vec2(x + size * 0.3, y - size * 0.2),
        color,
    );
    draw_triangle(
        vec2(x, y - size * 1.2),
        vec2(x - size * 0.25, y - size * 0.5),
        vec2(x + size * 0.25, y - size * 0.5),
        color,
    );
}

fn world(game: &Game, camera: f32, time: f32, motion: &EmberAnimation) {
    clear_background(Color::from_hex(0x242c2b));
    draw_circle(
        1040.0 - camera * 0.12,
        160.0,
        86.0,
        Color::from_hex(0xa97043),
    );
    for i in 0..12 {
        let x = i as f32 * 330.0 - camera * 0.22 - 140.0;
        draw_triangle(
            vec2(x, 510.0),
            vec2(x + 210.0, 175.0 + (i % 3) as f32 * 50.0),
            vec2(x + 450.0, 510.0),
            Color::from_hex(0x34413b),
        );
    }
    for i in 0..32 {
        tree(
            i as f32 * 130.0 - camera * 0.52,
            570.0,
            140.0 + (i % 4) as f32 * 40.0,
            Color::from_hex(0x28352f),
        );
    }
    for i in 0..8 {
        let x = 800.0 + i as f32 * 290.0 - camera * 0.8;
        draw_rectangle(x, 260.0, 12.0, 300.0, Color::from_hex(0x1e2926));
        draw_line(x, 268.0, x + 280.0, 290.0, 4.0, Color::from_hex(0x1e2926));
        draw_line(
            x + 100.0,
            275.0,
            x + 100.0,
            325.0,
            2.0,
            Color::from_hex(0x1e2926),
        );
        draw_circle(x + 100.0, 328.0, 4.0, ORANGE);
    }
    // Deep hot fissures provide an unmistakable landing boundary.
    draw_rectangle(0.0, 645.0, WIDTH, 75.0, Color::from_hex(0x8c452c));
    for i in 0..40 {
        let x = (i as f32 * 119.0 + time * 12.0).rem_euclid(WIDTH);
        let y = 675.0 - ((time * 25.0 + i as f32 * 39.0) % 180.0);
        draw_circle(x, y, 1.5, Color::new(1.0, 0.52, 0.22, 0.45));
    }
    for (index, p) in game.platforms.iter().enumerate() {
        if index >= 8 && !game.crumbles[index - 8].solid() {
            continue;
        }
        let x = p.x - camera;
        if index >= 8 {
            let warning = game.crumbles[index - 8].phase()
                == ferravex_play::gameplay::platforms::PlatformPhase::Warning;
            draw_rectangle_lines(
                x - 2.,
                p.y - 2.,
                p.w + 4.,
                p.h + 4.,
                2.,
                if warning { ORANGE } else { GREEN },
            );
        }
        draw_rectangle(x, p.y, p.w, p.h, Color::from_hex(0x383c32));
        draw_rectangle(x, p.y, p.w, 7.0, Color::from_hex(0x8d8962));
        draw_rectangle(x, p.y + 7.0, p.w, 8.0, Color::from_hex(0x545942));
        if p.h > 100.0 {
            for i in 0..(p.w / 64.0) as usize {
                let rx = x + i as f32 * 64.0;
                draw_triangle(
                    vec2(rx, 598.0),
                    vec2(rx + 50.0, 585.0),
                    vec2(rx + 33.0, 684.0),
                    Color::from_hex(0x30372f),
                );
            }
        } else {
            draw_line(
                x + 8.0,
                p.y + p.h,
                x + 8.0,
                560.0,
                3.0,
                Color::from_hex(0x62634d),
            );
            draw_line(
                x + p.w - 8.0,
                p.y + p.h,
                x + p.w - 8.0,
                560.0,
                3.0,
                Color::from_hex(0x62634d),
            );
        }
    }
    for index in [4, 6, 7] {
        let p = game.platforms[index];
        draw_rectangle(p.center().x - 25. - camera, p.y - 5., 50., 5., GREEN);
        label("^", p.center().x - 6. - camera, p.y - 10., 19., GREEN);
    }
    for h in &game.hazards {
        for i in 0..4 {
            let x = h.x - camera + i as f32 * h.w / 4.0;
            draw_triangle(
                vec2(x, h.bottom()),
                vec2(x + h.w / 8.0, h.y),
                vec2(x + h.w / 4.0, h.bottom()),
                ORANGE,
            );
        }
    }
    for (position, collected) in &game.embers {
        if !collected {
            diamond(
                *position - vec2(camera, 0.0) + vec2(0.0, (time * 3.0 + position.x).sin() * 3.0),
                10.0,
                ORANGE,
            );
        }
    }
    // Checkpoint lantern and the final lift share the same visual language.
    let checkpoint_color = if game.checkpoint_lit { GREEN } else { CREAM };
    let cx = 1435.0 - camera;
    draw_rectangle(cx - 3.0, 471.0, 6.0, 89.0, Color::from_hex(0x70715a));
    draw_rectangle(cx - 14.0, 459.0, 28.0, 30.0, checkpoint_color);
    draw_rectangle(cx - 8.0, 466.0, 16.0, 16.0, Color::from_hex(0x324237));
    let exit = 2590.0 - camera;
    draw_rectangle(exit - 35.0, 450.0, 70.0, 110.0, Color::from_hex(0x1a2925));
    draw_rectangle_lines(exit - 35.0, 450.0, 70.0, 110.0, 5.0, GREEN);
    draw_triangle(
        vec2(exit - 8.0, 495.0),
        vec2(exit + 9.0, 505.0),
        vec2(exit - 8.0, 515.0),
        GREEN,
    );
    let p = game.position() - vec2(camera, 0.0);
    let (left, right, body) = match motion.frame() {
        1 => (0., 0., 1.),
        2 => (-3., 3., 0.),
        3 | 5 => (0., 0., -1.),
        4 => (3., -3., 0.),
        8 => (-3., -1., 0.),
        9 => (1., 2., 0.),
        10 => (-3., -3., 3.),
        11 => (-1., -1., 1.),
        _ => (0., 0., 0.),
    };
    draw_ellipse(
        p.x + 12.0,
        p.y + 42.0,
        15.0,
        3.0,
        0.0,
        Color::new(0.0, 0.0, 0.0, 0.20),
    );
    draw_rectangle(p.x + 3.0, p.y + 15.0 + body, 18.0, 19.0, ORANGE);
    draw_rectangle(
        p.x + 4.0,
        p.y + 32.0,
        6.0,
        8.0 + left,
        Color::from_hex(0x1b2524),
    );
    draw_rectangle(
        p.x + 15.0,
        p.y + 32.0,
        6.0,
        8.0 + right,
        Color::from_hex(0x1b2524),
    );
    draw_rectangle(p.x + 4.0, p.y + 1.0 + body, 17.0, 15.0, CREAM);
    draw_rectangle(p.x + 1.0, p.y + body, 24.0, 6.0, Color::from_hex(0xc79d57));
    let face = if motion.facing() < 0. { 5.0 } else { 14.0 };
    draw_rectangle(
        p.x + face,
        p.y + 7.0 + body,
        5.0,
        3.0,
        Color::from_hex(0x26312b),
    );
    draw_triangle(
        vec2(p.x + 5.0, p.y + 18.0),
        vec2(
            p.x + 12.0 - motion.facing() * 25.0,
            p.y + 22.0 + left * 0.4 + body,
        ),
        vec2(p.x + 5.0, p.y + 26.0),
        Color::from_hex(0x9e5033),
    );
}

fn label(text: &str, x: f32, y: f32, size: f32, color: Color) {
    draw_text(
        text,
        x + 1.0,
        y + 2.0,
        size,
        Color::new(0.08, 0.12, 0.10, 0.8),
    );
    draw_text(text, x, y, size, color);
}

fn config() -> Conf {
    ferravex_render::window_conf("EMBERLINE | Ferravex")
}

ferravex_render::game!(config, game);
async fn game() {
    let mut demo = Demo::new();
    let mut sound = GameAudio::new();
    let mut game = Game::new();
    let mut motion = EmberAnimation::new();
    let mut clock = FixedClock::new(120.0, 12);
    let mut player_input = PlayerInput::preset("emberline", Preset::Platformer);
    let mut camera = Follow2::new(Follow2Settings {
        offset: vec2(WIDTH * 0.15, 0.),
        dead_zone: vec2(28., 0.),
        half_life: std::f32::consts::LN_2 / 9.,
        look_ahead_time: 0.05,
        max_look_ahead: 28.,
        teleport_distance: Some(600.),
        limits: Some(Limits2 {
            min: vec2(WIDTH * 0.5, HEIGHT * 0.5),
            max: vec2(WORLD_END - WIDTH * 0.5, HEIGHT * 0.5),
        }),
    })
    .unwrap();
    let mut font_dpi = 0.0;
    let mut previous_deaths = 0;
    loop {
        let dpi = screen_dpi_scale();
        if font_dpi != dpi {
            let characters: Vec<char> = (' '..='~').collect();
            for size in [15_u16, 19, 23, 24, 28, 32, 33] {
                get_default_font()
                    .populate_font_cache(&characters, (f32::from(size) * dpi).ceil() as u16);
            }
            font_dpi = dpi;
        }
        let canvas = ferravex_render::canvas::Canvas2D::fit(
            vec2(WIDTH, HEIGHT),
            vec2(screen_width(), screen_height()),
        );
        let button = |rect: Rect, code| Control {
            region: Region::Rect(Rect::new(
                canvas.project(vec2(rect.x, rect.y)).x,
                canvas.project(vec2(rect.x, rect.y)).y,
                rect.w * canvas.scale,
                rect.h * canvas.scale,
            )),
            output: Output::Button(code),
        };
        let layout = [
            button(Rect::new(0., 590., 120., HEIGHT - 590.), 12),
            button(Rect::new(120., 590., 130., HEIGHT - 590.), 13),
            button(Rect::new(1080., 590., WIDTH - 1080., HEIGHT - 590.), 3),
            button(Rect::new(0., 350., WIDTH, 120.), 9),
        ];
        player_input
            .set_touch_layout(&layout[..if game.won { 4 } else { 3 }])
            .unwrap();
        player_input.sample_play();
        let touch_mode = player_input.touch_seen();
        if player_input.pressed("restart") || game.won && player_input.pressed("confirm") {
            game = Game::new();
            sound.reset();
            motion.reset();
            clock.reset();
            camera.cut();
        }
        let direction = player_input.movement().x;
        let dt = if demo.autoplay {
            demo.dt()
        } else {
            player_input.simulation_dt(demo.dt())
        };
        let frame = clock.advance(f64::from(dt));
        for _ in 0..frame.steps {
            let input = if demo.autoplay {
                game.autoplay()
            } else {
                Input {
                    direction,
                    jump: player_input.pressed("jump"),
                    jump_held: player_input.held("jump"),
                }
            };
            let grounded = game.motor.grounded;
            let collected = game.embers.iter().filter(|(_, found)| *found).count();
            let won = game.won;
            let deaths = game.deaths;
            game.update(input, frame.dt);
            if grounded && !game.motor.grounded && game.velocity().y < 0. {
                sound.cue(Cue::Jump, 0.5);
            }
            if !grounded && game.motor.grounded {
                sound.cue(Cue::Impact(Surface::Wood), 0.4);
            }
            if game.embers.iter().filter(|(_, found)| *found).count() > collected {
                sound.cue(Cue::Pickup, 0.55);
            }
            if game.won && !won {
                sound.cue(Cue::Success, 0.65);
            }
            if game.deaths > deaths {
                sound.cue(Cue::Failure, 0.5);
            }
        }
        if game.deaths != previous_deaths {
            motion.reset();
        }
        motion.update(&game, dt);
        for _ in 0..motion.footsteps() {
            let surface = if game.position().x > 1800. {
                Surface::Metal
            } else if game.position().x > 900. {
                Surface::Wood
            } else {
                Surface::Grass
            };
            sound.cue(Cue::Footstep(surface), 0.32);
        }
        if game.deaths != previous_deaths {
            camera.cut();
        }
        previous_deaths = game.deaths;
        let camera_x = camera
            .update(
                demo.dt(),
                vec2(game.position().x, HEIGHT * 0.5),
                vec2(game.velocity().x, 0.),
            )
            .expect("valid platformer camera")
            .center
            .x
            - WIDTH * 0.5;
        set_camera(&canvas.camera());
        world(&game, camera_x, demo.time(), &motion);
        ferravex_render::set_world_colors(false);
        let collected = game.embers.iter().filter(|(_, found)| *found).count();
        label("EMBERLINE", 32.0, 48.0, 28.0, CREAM);
        let zone = if game.position().x < 900.0 {
            "01 / THE TREELINE"
        } else if game.position().x < 1800.0 {
            "02 / THE OLD MILL"
        } else {
            "03 / THE FURNACE"
        };
        label(zone, 34.0, 72.0, 15.0, Color::from_hex(0xa3ab91));
        label(
            &format!(
                "{:.2}s / SPLITS {}",
                game.trial.elapsed(),
                game.trial.splits().iter().flatten().count()
            ),
            520.,
            48.,
            19.,
            CREAM,
        );
        label(
            &format!(
                "{} POINTS / x{}",
                game.score.total(),
                game.score.multiplier()
            ),
            850.,
            72.,
            15.,
            ORANGE,
        );
        diamond(vec2(WIDTH - 121.0, 38.0), 8.0, ORANGE);
        label(
            &format!("{collected} / {}", game.embers.len()),
            WIDTH - 105.0,
            45.0,
            23.0,
            CREAM,
        );
        if game.ember_inventory.count(1) >= 3 {
            label(
                if game.air_jump_used {
                    "AIR JUMP SPENT"
                } else {
                    "AIR JUMP READY"
                },
                34.,
                105.,
                15.,
                GREEN,
            );
        }
        if game.elapsed < 7.0 && !touch_mode {
            label(
                "A / D  move     SPACE  tap / hold jump     R  restart",
                34.0,
                680.0,
                19.0,
                CREAM,
            );
        }
        if touch_mode && !player_input.touch_customized() {
            for (x, text) in [(68.0, "<"), (182.0, ">"), (1184.0, "^")] {
                draw_circle(x, 651.0, 38.0, Color::new(0.09, 0.14, 0.12, 0.65));
                label(text, x - 11.0, 663.0, 32.0, CREAM);
            }
        }
        if game.respawn_flash > 0.0 {
            label(
                "Back to the lantern",
                WIDTH * 0.5 - 110.0,
                112.0,
                24.0,
                CREAM,
            );
        }
        if game.won {
            draw_rectangle(
                380.0,
                220.0,
                520.0,
                250.0,
                Color::new(0.08, 0.13, 0.11, 0.96),
            );
            label("THE LIGHT CARRIES ON", 422.0, 280.0, 33.0, CREAM);
            label(
                &format!(
                    "{:?} / {:.2}s",
                    game.trial.medal().unwrap(),
                    game.trial.elapsed()
                ),
                505.,
                370.,
                19.,
                GREEN,
            );
            label(
                &format!(
                    "{collected} embers / {} points / {} falls",
                    game.score.total(),
                    game.deaths
                ),
                432.0,
                330.0,
                24.0,
                ORANGE,
            );
            label(
                if touch_mode {
                    "Tap here to run again"
                } else {
                    "Press R to run again"
                },
                504.0,
                417.0,
                23.0,
                CREAM,
            );
        }
        set_default_camera();
        player_input.draw_settings();
        if demo.finish_frame() {
            break;
        }
        next_frame().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tap_and_hold_produce_distinct_controlled_jump_arcs() {
        let apex = |held| {
            let mut game = Game::new();
            for _ in 0..8 {
                game.update(Input::default(), DT);
            }
            let mut rise = 0_f32;
            for tick in 0..130 {
                game.update(
                    Input {
                        jump: tick == 0,
                        jump_held: held,
                        direction: 0.,
                    },
                    DT,
                );
                rise = rise.max(520. - game.position().y);
            }
            assert_eq!(game.jumps, 1);
            assert_eq!(game.deaths, 0);
            assert!(game.motor.grounded);
            rise
        };
        let short = apex(false);
        let full = apex(true);
        assert!(
            short > 20. && full > short + 60.,
            "tap={short}, hold={full}"
        );
    }

    const DT: f32 = 1.0 / 120.0;

    #[test]
    fn floor_stops_player_without_tunneling() {
        let mut game = Game::new();
        game.motor.position = feet(vec2(90., 80.));
        game.motor.grounded = false;
        for _ in 0..240 {
            game.update(Input::default(), DT);
        }
        assert!(game.motor.grounded);
        assert!((game.position().y - 520.).abs() < 0.01);
        assert_eq!(game.deaths, 0);
    }

    #[test]
    fn coyote_jump_and_buffered_landing_work() {
        let mut game = Game::new();
        game.motor.position = feet(vec2(590., 520.));
        game.motor.grounded = false;
        game.motor.jump.coyote = 0.07;
        game.update(
            Input {
                jump: true,
                ..Input::default()
            },
            DT,
        );
        assert!(game.velocity().y < -600.0);
        game.motor.position = feet(vec2(90., 517.));
        game.motor.jump.held = false;
        game.motor.velocity.y = -180.;
        game.motor.jump.coyote = 0.0;
        game.update(
            Input {
                jump: true,
                ..Input::default()
            },
            DT,
        );
        for _ in 0..4 {
            game.update(
                Input {
                    jump_held: true,
                    ..Input::default()
                },
                DT,
            );
        }
        assert!(game.velocity().y < -500.0);
        assert_eq!(game.jumps, 2);
    }

    #[test]
    fn hazards_respawn_at_activated_checkpoint() {
        let mut game = Game::new();
        game.motor.position = feet(vec2(1410., 520.));
        game.update(Input::default(), DT);
        assert!(game.checkpoint_lit);
        game.motor.position = feet(vec2(1605., 520.));
        game.update(Input::default(), DT);
        assert_eq!(game.position(), game.checkpoint);
        assert_eq!(game.deaths, 1);
    }

    #[test]
    fn moving_side_platform_carries_a_standing_player() {
        let mut game = Game::new();
        game.motor = Character::new(feet(vec2(815., 395.)));
        game.update(Input::default(), DT);
        let before = game.position();
        game.update(Input::default(), DT);
        assert!(game.motor.grounded);
        assert!(game.position().x - before.x > 0.27);
        assert!((game.position().y - before.y).abs() < 0.01);
    }
    #[test]
    fn autoplay_completes_all_sections_with_real_physics() {
        let mut game = Game::new();
        for _ in 0..2400 {
            game.update(game.autoplay(), DT);
            if game.won {
                break;
            }
        }
        assert!(
            game.won,
            "position {:?}, deaths {}",
            game.position(),
            game.deaths
        );
        assert!(game.checkpoint_lit);
        assert!(game.jumps >= 7);
        assert!(game.embers.iter().any(|(_, found)| *found));
        assert_eq!(game.deaths, 0);
    }
}

#[cfg(test)]
mod collectible_score_tests {
    use super::*;
    #[test]
    fn each_ember_awards_once_and_falls_break_only_the_chain() {
        let mut g = Game::new();
        g.embers = vec![
            (vec2(102., 540.), false),
            (vec2(108., 540.), false),
            (vec2(114., 540.), false),
        ];
        g.update(Input::default(), 1. / 120.);
        assert_eq!(g.score.total(), 400);
        g.update(Input::default(), 1. / 120.);
        assert_eq!(g.score.total(), 400);
        g.respawn();
        assert_eq!(g.score.chain(), 0);
        assert_eq!(g.score.total(), 400);
    }
}

#[cfg(test)]
mod trial_loop_tests {
    use super::*;
    #[test]
    fn ordinary_completion_records_all_splits_and_freezes_the_medal() {
        let mut g = Game::new();
        for _ in 0..2400 {
            g.update(g.autoplay(), 1. / 120.);
            if g.won {
                break;
            }
        }
        assert!(g.trial.finished());
        let splits = g.trial.splits();
        assert!(splits[0] < splits[1] && splits[1] < splits[2]);
        assert!(g.trial.medal().is_some());
        let time = g.trial.elapsed();
        g.update(Input::default(), 1.);
        assert_eq!(g.trial.elapsed(), time);
        assert!(!Game::new().trial.finished());
    }
}

#[cfg(test)]
mod earned_jump_tests {
    use super::*;
    #[test]
    fn three_real_pickups_unlock_one_air_jump_per_landing() {
        let mut g = Game::new();
        g.embers = vec![(vec2(102., 540.), false); 3];
        g.update(Input::default(), 1. / 120.);
        assert_eq!(g.ember_inventory.count(1), 3);
        g.update(
            Input {
                jump: true,
                jump_held: true,
                ..Input::default()
            },
            1. / 120.,
        );
        for _ in 0..20 {
            g.update(
                Input {
                    jump_held: true,
                    ..Input::default()
                },
                1. / 120.,
            );
        }
        g.update(
            Input {
                jump: true,
                jump_held: true,
                ..Input::default()
            },
            1. / 120.,
        );
        assert!(g.air_jump_used);
        assert!(g.motor.velocity.y > 500.);
        assert_eq!(g.jumps, 2);
        g.update(
            Input {
                jump_held: true,
                ..Input::default()
            },
            1. / 120.,
        );
        g.update(
            Input {
                jump: true,
                jump_held: true,
                ..Input::default()
            },
            1. / 120.,
        );
        assert_eq!(g.jumps, 2);
        for _ in 0..240 {
            g.update(Input::default(), 1. / 120.);
        }
        assert!(g.motor.grounded);
        assert!(!g.air_jump_used);
    }
}

#[cfg(test)]
mod spring_route_tests {
    use super::*;
    #[test]
    fn bonus_route_springboards_launch_from_actual_platform_contact() {
        let mut g = Game::new();
        g.motor = Character::new(feet(vec2(168., 405.)));
        g.update(Input::default(), 1. / 120.);
        assert!(g.spring_flight);
        assert!(!g.motor.grounded);
        assert!(g.motor.velocity.y > 750.);
        g.update(Input::default(), 1. / 120.);
        assert!(g.motor.velocity.y > 700.);
    }
}

#[cfg(test)]
mod crumble_route_tests {
    use super::*;
    #[test]
    fn bonus_platforms_really_stop_supporting_the_player_then_regrow() {
        let mut g = Game::new();
        g.motor = Character::new(feet(vec2(410., 290.)));
        g.update(Input::default(), 1. / 120.);
        assert!(g.motor.grounded);
        for _ in 0..90 {
            g.update(
                Input {
                    jump_held: true,
                    ..Input::default()
                },
                1. / 120.,
            );
        }
        assert!(!g.crumbles[0].solid());
        assert!(g.position().y > 290.);
        for _ in 0..400 {
            g.update(Input::default(), 1. / 120.);
        }
        assert!(g.crumbles[0].solid());
    }
}
