//! Shared input/HUD with distinct top-down and actual mesh-based 3D renderers.
use crate::{
    capture::Capture,
    card::{self, Best, HoleOutcome, RoundStats, ScoreName, Style, Tone},
    course::{self, RADIUS, SIZE, Surface},
    game::{Events, MAX_STROKES, Phase, Round},
    juice::{self, CLAY, Juice, MINT, tone_color},
    quips,
};
mod draw;
mod hud;
use draw::terrain_mesh;
use ferravex_core::session::GameSession;
use ferravex_render::{
    audio::{Cue, GameAudio, Surface as SoundSurface},
    input::{ActionKind, Binding, InputCode, PlayerInput},
    mesh::Mesh3d,
    preferences,
    prelude::*,
    ui::{Align, ButtonStyle, TextStyle, Ui, UiRect, Viewport},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    TwoD,
    ThreeD,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Menu {
    Welcome,
    Score,
    Practice,
    None,
}
const PAPER: Color = Color::from_rgba(246, 239, 218, 255);
const DARK: Color = Color::from_rgba(27, 51, 43, 255);
const PANEL: Color = Color::from_rgba(36, 66, 53, 255);
const DIM: Color = Color::from_rgba(170, 186, 160, 255);
const GREEN: Color = Color::from_rgba(100, 153, 107, 255);
const COPPER: Color = Color::from_rgba(199, 111, 65, 255);
const GOLD: Color = Color::from_rgba(232, 193, 109, 255);
/// Seconds the hole title card stays up before fading.
const INTRO: f32 = 3.6;
/// Render seconds the demonstration lingers on each finished hole.
const DEMO_HOLD: f32 = 1.4;
/// Putts after which the control hint has been learned.
const LEARNED: u32 = 3;
fn rect(x: f32, y: f32, w: f32, h: f32) -> UiRect {
    UiRect {
        min: [x, y],
        size: [w.max(1.), h.max(1.)],
    }
}
fn fade(c: Color, a: f32) -> Color {
    Color::new(c.r, c.g, c.b, c.a * a.clamp(0., 1.))
}
fn mix(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0., 1.);
    Color::new(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
        a.a + (b.a - a.a) * t,
    )
}
/// Wrapped copy for paragraphs.
fn label(ui: &mut Ui, r: UiRect, s: &str, size: f32, color: Color) {
    ui.text(
        r,
        s,
        TextStyle {
            size,
            color,
            wrap: true,
            ..Default::default()
        },
    )
    .expect("bounded golf text");
}
/// One line that shrinks to fit its rectangle instead of clipping.
fn line(ui: &mut Ui, r: UiRect, s: &str, size: f32, color: Color, align: Align) {
    ui.text(
        r,
        s,
        TextStyle {
            size,
            color,
            align,
            wrap: false,
            shadow: false,
        },
    )
    .expect("bounded golf text");
}
/// A shadowed line for copy drawn over the course.
fn overlay(ui: &mut Ui, r: UiRect, s: &str, size: f32, color: Color) {
    ui.text(
        r,
        s,
        TextStyle {
            size,
            color,
            align: Align::Center,
            wrap: false,
            shadow: true,
        },
    )
    .expect("bounded golf text");
}
fn button(ui: &mut Ui, id: u64, r: UiRect, s: &str, enabled: bool) {
    ui.button(
        id,
        r,
        s,
        enabled,
        ButtonStyle {
            fill: if matches!(id, 14 | 20 | 21 | 24) {
                COPPER
            } else {
                Color::from_rgba(43, 76, 60, 255)
            },
            ink: PAPER,
            focus: GOLD,
            disabled: Color::from_rgba(68, 81, 71, 255),
            size: 16.,
        },
    )
    .expect("bounded golf button");
}
fn ease_out(t: f32) -> f32 {
    1. - (1. - t.clamp(0., 1.)).powi(3)
}
#[derive(Clone, Copy)]
struct GolfCamera {
    matrix: Mat4,
    eye: Vec3,
    /// Screen pixels per metre in the orthographic view plane.
    scale: f32,
}
impl Camera for GolfCamera {
    fn matrix(&self, _w: f32, _h: f32) -> Mat4 {
        self.matrix
    }
    fn depth(&self) -> bool {
        true
    }
    fn position(&self) -> Option<Vec3> {
        Some(self.eye)
    }
}
/// Course region between the compact score chip and shot dock.
/// On-demand menus may temporarily cover the course.
#[derive(Clone, Copy, Debug)]
pub struct Scene {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}
impl Scene {
    pub fn new(w: f32, h: f32) -> Self {
        Self {
            x: 12.,
            y: 60.,
            w: (w - 24.).max(1.),
            h: (h - 144.).max(1.),
        }
    }
    fn contains(self, p: Vec2) -> bool {
        p.x >= self.x && p.x < self.x + self.w && p.y >= self.y && p.y < self.y + self.h
    }
    fn shifted(self, by: Vec2) -> Self {
        Self {
            x: self.x + by.x,
            y: self.y + by.y,
            ..self
        }
    }
}
fn camera(scene: Scene, w: f32, h: f32) -> GolfCamera {
    let eye = vec3(20., 24., 24.);
    let view = Mat4::look_at_rh(eye, vec3(9., 0., 6.), Vec3::Y);
    let mut lo = Vec2::splat(f32::INFINITY);
    let mut hi = Vec2::splat(f32::NEG_INFINITY);
    for x in [-0.8, 18.8] {
        for z in [-0.8, 12.8] {
            for y in [-0.4, 2.2] {
                let p = view.transform_point3(vec3(x, y, z));
                lo = lo.min(vec2(p.x, p.y));
                hi = hi.max(vec2(p.x, p.y));
            }
        }
    }
    let scale = (scene.w / (hi.x - lo.x)).min(scene.h / (hi.y - lo.y));
    let mid = (hi + lo) * 0.5;
    let cx = scene.x + scene.w * 0.5;
    let cy = scene.y + scene.h * 0.5;
    let sx = 2. * scale / w;
    let sy = 2. * scale / h;
    let projection = Mat4::from_cols(
        vec4(sx, 0., 0., 0.),
        vec4(0., sy, 0., 0.),
        vec4(0., 0., -0.02, 0.),
        vec4(
            2. * cx / w - 1. - sx * mid.x,
            1. - 2. * cy / h - sy * mid.y,
            0.,
            1.,
        ),
    );
    GolfCamera {
        matrix: projection * view,
        eye,
        scale,
    }
}
fn top_scale(scene: Scene) -> f32 {
    (scene.w / 20.).min(scene.h / 14.)
}
fn top_map(scene: Scene, p: Vec2) -> Vec2 {
    vec2(scene.x + scene.w * 0.5, scene.y + scene.h * 0.5) + (p - SIZE * 0.5) * top_scale(scene)
}
fn world_pointer(view: View, scene: Scene, p: Vec2, w: f32, h: f32) -> Vec2 {
    if view == View::TwoD {
        (p - vec2(scene.x + scene.w * 0.5, scene.y + scene.h * 0.5)) / top_scale(scene) + SIZE * 0.5
    } else {
        let inv = camera(scene, w, h).matrix.inverse();
        let a = inv.project_point3(vec3(p.x / w * 2. - 1., 1. - p.y / h * 2., -1.));
        let b = inv.project_point3(vec3(p.x / w * 2. - 1., 1. - p.y / h * 2., 1.));
        let d = b - a;
        let t = if d.y.abs() > 0.00001 { -a.y / d.y } else { 0. };
        let q = a + d * t;
        vec2(q.x, q.z)
    }
}
/// Course point (`x`, height, `z`) to screen pixels, for either view.
struct Projector {
    view: View,
    scene: Scene,
    camera: GolfCamera,
    w: f32,
    h: f32,
}
impl Projector {
    fn new(view: View, scene: Scene, w: f32, h: f32) -> Self {
        Self {
            view,
            scene,
            camera: camera(scene, w, h),
            w,
            h,
        }
    }
    fn ppm(&self) -> f32 {
        if self.view == View::TwoD {
            top_scale(self.scene)
        } else {
            self.camera.scale
        }
    }
    fn screen(&self, p: Vec3) -> Vec2 {
        if self.view == View::TwoD {
            top_map(self.scene, vec2(p.x, p.z)) - vec2(0., p.y * self.ppm())
        } else {
            let q = self.camera.matrix.project_point3(p);
            vec2((q.x + 1.) * self.w * 0.5, (1. - q.y) * self.h * 0.5)
        }
    }
}
/// Browser fullscreen pins a 44 px game-menu button 8 px from the top-right
/// corner. HUD text and controls stay out of this `CORNER.x` × `CORNER.y`
/// region; only full-bleed background bars and the course may pass under it.
const CORNER: Vec2 = vec2(72., 60.);
/// The score chip (a button) ends left of the menu corner.
fn chip_width(w: f32) -> f32 {
    (w - 10. - CORNER.x - 2.).min(350.)
}
/// Headline copy is inset symmetrically so it clears the menu corner.
fn banner_text_rect(w: f32, y: f32, size: f32) -> UiRect {
    rect(CORNER.x, y, w - 2. * CORNER.x, size)
}
/// Results card height: the stats grid only appears on tall screens.
fn final_card_height(h: f32, assists: bool) -> f32 {
    let grid = if h >= 520. {
        4. * 18. + 12. + if assists { 18. } else { 0. }
    } else {
        0.
    };
    156. + grid + 44. + 8. + 44. + 16.
}
/// Rough flat-green roll distance for the aim preview, in metres.
fn preview_length(power: f32) -> f32 {
    0.8 + power * 4.4
}
fn power_color(power: f32) -> Color {
    if power < 0.55 {
        mix(MINT, GOLD, power / 0.55)
    } else {
        mix(GOLD, CLAY, (power - 0.55) / 0.45)
    }
}
pub struct App {
    pub round: Round,
    view: View,
    input: PlayerInput,
    ui: Ui,
    audio: GameAudio,
    menu: Menu,
    manual_pause: bool,
    aim: Vec2,
    power: f32,
    pending_shot: bool,
    drag_start: Option<Vec2>,
    drag_touch: Option<u64>,
    aim_pointer: Vec2,
    auto: bool,
    auto_wait: f32,
    bank_cooldown: f32,
    juice: Juice,
    stats: RoundStats,
    /// The finished hole currently being celebrated.
    outcome: Option<HoleOutcome>,
    /// Per-hole personal bests (strokes), from local records.
    hole_best: [Option<u8>; 12],
    /// The best on this hole before the outcome above was recorded.
    hole_best_before: Option<u8>,
    /// Full-round comparison, fixed when the round finishes.
    round_best: Option<Best>,
    /// Render seconds since this hole's title card appeared.
    intro: f32,
    /// Putts accepted this session, for onboarding hints.
    putts: u32,
    /// Session counters that keep quips from repeating.
    splashes: u32,
    lips: u32,
    nears: u32,
    returns: u32,
    sands: u32,
    pause_seed: u32,
    /// Between-hole sheet slide progress, 0..=1.
    sheet: f32,
    sheet_h: f32,
    ball_mesh: Mesh3d,
    terrain: Option<Mesh3d>,
    terrain_hole: usize,
    cup_mesh: Mesh3d,
    post_mesh: Mesh3d,
    stone_mesh: Mesh3d,
    copper_mesh: Mesh3d,
    records: Option<ferravex_render::storage::Records>,
    best: Option<u16>,
    saved: bool,
    material: ferravex_render::shaders::LitMaterial,
    capture: Capture,
}
impl App {
    pub fn new(view: View, autoplay: bool) -> Self {
        let mut input = PlayerInput::new(if view == View::TwoD {
            "mini-golf-2d"
        } else {
            "mini-golf-3d"
        });
        for (name, title, codes, global) in [
            (
                "shoot",
                "Putt",
                vec![
                    InputCode::Key(KeyCode::Space as u16),
                    InputCode::PadButton(0),
                ],
                false,
            ),
            (
                "left",
                "Aim left",
                vec![
                    InputCode::Key(0xff51),
                    InputCode::Key(KeyCode::A as u16),
                    InputCode::PadButton(14),
                ],
                false,
            ),
            (
                "right",
                "Aim right",
                vec![
                    InputCode::Key(0xff53),
                    InputCode::Key(KeyCode::D as u16),
                    InputCode::PadButton(15),
                ],
                false,
            ),
            (
                "more",
                "More power",
                vec![
                    InputCode::Key(0xff52),
                    InputCode::Key(KeyCode::W as u16),
                    InputCode::PadButton(12),
                ],
                false,
            ),
            (
                "less",
                "Less power",
                vec![
                    InputCode::Key(0xff54),
                    InputCode::Key(KeyCode::S as u16),
                    InputCode::PadButton(13),
                ],
                false,
            ),
            (
                "pause",
                "Pause",
                vec![
                    InputCode::Key(0xff1b),
                    InputCode::Key(KeyCode::P as u16),
                    InputCode::PadButton(9),
                ],
                true,
            ),
            ("confirm", "Continue", vec![InputCode::Key(0xff0d)], true),
            (
                "score",
                "Scorecard",
                vec![InputCode::Key(KeyCode::C as u16)],
                true,
            ),
            (
                "drop",
                "Return ball (+1)",
                vec![InputCode::Key(KeyCode::R as u16)],
                false,
            ),
        ] {
            let binds = codes.into_iter().map(Binding::digital).collect();
            if global {
                input.global_action(name, title, ActionKind::Button, binds)
            } else {
                input.action(name, title, ActionKind::Button, binds)
            }
            .expect("golf default action");
        }
        let records = ferravex_render::storage::Records::new(if view == View::TwoD {
            "mini-golf-2d"
        } else {
            "mini-golf-3d"
        })
        .ok();
        let capture = Capture::from_env();
        let best = capture.fake_best.or_else(|| {
            records
                .as_ref()
                .and_then(|r| r.get("round", 12))
                .map(|r| (1000 - r.score.min(1000)) as u16)
        });
        let hole_best = std::array::from_fn(|i| {
            records
                .as_ref()
                .and_then(|r| r.get("hole", i as u64))
                .map(|r| (100 - r.score.min(100)) as u8)
                .filter(|&s| (1..MAX_STROKES).contains(&s))
        });
        let mut round = Round::new();
        if let Some(hole) = capture.hole {
            round.load(hole);
        }
        let mut app = Self {
            round,
            view,
            input,
            ui: Ui::new(),
            audio: GameAudio::new(),
            menu: if autoplay || capture.skip_welcome {
                Menu::None
            } else {
                Menu::Welcome
            },
            manual_pause: false,
            aim: Vec2::X,
            power: 0.56,
            pending_shot: false,
            drag_start: None,
            drag_touch: None,
            aim_pointer: Vec2::ZERO,
            auto: autoplay,
            auto_wait: 0.5,
            bank_cooldown: 0.,
            juice: Juice::new(),
            stats: RoundStats::default(),
            outcome: None,
            hole_best,
            hole_best_before: None,
            round_best: None,
            intro: 0.,
            putts: 0,
            splashes: 0,
            lips: 0,
            nears: 0,
            returns: 0,
            sands: 0,
            pause_seed: 0,
            sheet: 0.,
            sheet_h: 0.,
            terrain: None,
            terrain_hole: usize::MAX,
            cup_mesh: Mesh3d::cylinder(0.32, 0.035, DARK, 24),
            ball_mesh: Mesh3d::sphere(RADIUS, PAPER, 16, 10),
            post_mesh: Mesh3d::cylinder(0.035, 1.55, PAPER, 8),
            stone_mesh: Mesh3d::cuboid(Vec3::ONE, Color::from_rgba(225, 214, 184, 255)),
            copper_mesh: Mesh3d::cuboid(Vec3::ONE, COPPER),
            records,
            best,
            saved: false,
            material: ferravex_render::shaders::LitMaterial::new().expect("built-in lighting"),
            capture,
        };
        if app.capture.hole.is_some() {
            app.aim = (app.round.course.cup - app.round.ball).normalize_or_zero();
        }
        app
    }
    fn reset(&mut self) {
        self.round = Round::new();
        self.aim = Vec2::X;
        self.power = 0.56;
        self.saved = false;
        self.manual_pause = false;
        self.menu = Menu::None;
        self.pending_shot = false;
        self.drag_start = None;
        self.drag_touch = None;
        self.auto_wait = 0.6;
        self.stats.reset();
        self.juice.reset();
        self.begin_hole();
    }
    /// Presentation state for a freshly loaded hole.
    fn begin_hole(&mut self) {
        self.outcome = None;
        self.round_best = None;
        self.hole_best_before = None;
        self.juice.new_hole();
        self.intro = 0.;
        self.sheet = 0.;
    }
    /// Platform focus and Controls state; debug captures may pin focus.
    fn gameplay_focus(&self) -> bool {
        self.input.gameplay_active() || self.capture.force_focus && !self.input.controls_open()
    }
    fn modal_open(&self) -> bool {
        self.menu != Menu::None || self.manual_pause || self.round.phase == Phase::Finished
    }
    fn sheet_open(&self) -> bool {
        !self.modal_open() && self.round.phase == Phase::Holed
    }
    /// Sample input and prepare screen-space UI once per frame. Physics edges
    /// remain pending until `tick`; UI and shared Controls own their gestures.
    pub fn prepare(&mut self, session: &mut GameSession, dt: f32) {
        self.juice.apply_policy(preferences::feedback_policy());
        self.input
            .sample(self.menu == Menu::None && !self.manual_pause);
        if self.input.take("pause").pressed {
            if self.menu == Menu::None {
                self.manual_pause = !self.manual_pause;
                self.pause_seed = self.pause_seed.wrapping_add(1);
            } else if self.menu != Menu::Welcome {
                self.menu = Menu::None;
            }
        }
        if self.input.take("score").pressed {
            self.menu = if self.menu == Menu::Score {
                Menu::None
            } else {
                Menu::Score
            };
        }
        if let Some(pause) = self.capture.frame_actions() {
            match pause {
                crate::capture::Action::Pause => {
                    self.manual_pause = true;
                    self.pause_seed = self.pause_seed.wrapping_add(1);
                }
                crate::capture::Action::Card => self.menu = Menu::Score,
            }
        }
        let w = screen_width();
        let h = screen_height();
        let scene = Scene::new(w, h);
        let running = self.menu == Menu::None && !self.manual_pause && self.gameplay_focus();
        if running && self.round.phase == Phase::Finished {
            // Settle the last hole's confetti and callouts behind the results.
            self.juice.advance_celebration(dt);
        }
        if self.sheet_open() && running {
            self.juice.advance_celebration(dt);
            self.sheet = if self.juice.motion() == 0. {
                1.
            } else {
                (self.sheet + dt.clamp(0., 0.1) * 3.5).min(1.)
            };
        }
        if running && matches!(self.round.phase, Phase::Aiming | Phase::Rolling) {
            self.intro += dt.clamp(0., 0.1);
        }
        self.ui
            .begin(Viewport::responsive([w, h], [0.; 4]).expect("positive screen"));
        // Only the score and the shot belong in the permanent HUD. Course notes,
        // bindings and player preferences live in the on-demand game menu.
        let modal = self.modal_open();
        let sheet = self.sheet_open();
        if !modal && !sheet {
            self.hud(w, h, scene);
        }
        if !modal {
            self.banner(w, scene);
        }
        if sheet {
            self.hole_sheet(w, h);
        } else if modal {
            self.modal(w, h);
        }
        let controls_open = self.input.controls_open();
        self.ui.set_auto_focus(modal || sheet);
        self.ui.finish_player(&mut self.input, !controls_open);
        let consumed = self.ui.interaction().pointer_consumed;
        for id in 10..=15 {
            if self.ui.activated(id) {
                match id {
                    10 => self.aim = Vec2::from_angle(-0.10).rotate(self.aim),
                    11 => self.aim = Vec2::from_angle(0.10).rotate(self.aim),
                    12 => self.power = (self.power - 0.05).max(0.05),
                    13 => self.power = (self.power + 0.05).min(1.),
                    14 => self.pending_shot = true,
                    15 => self.menu = Menu::Score,
                    _ => {}
                }
            }
        }
        if self.ui.activated(20) {
            if self.menu == Menu::Welcome || self.menu == Menu::None {
                self.manual_pause = false;
            }
            self.menu = Menu::None;
        }
        if self.ui.activated(21) {
            self.reset();
        }
        if self.ui.activated(22) {
            self.menu = Menu::Practice;
        }
        if self.ui.activated(23) {
            self.menu = Menu::Score;
        }
        if self.ui.activated(24) {
            self.advance_hole();
        }
        if self.ui.activated(25) {
            self.round.pick_up();
            self.settle();
        }
        for i in 0..12 {
            if self.ui.activated(100 + i as u64) {
                self.round.practice_hole(i);
                self.menu = Menu::None;
                self.manual_pause = false;
                self.aim = (self.round.course.route[0] - self.round.ball).normalize_or_zero();
                self.saved = false;
                self.stats.reset();
                self.begin_hole();
            }
        }
        let active = self.menu == Menu::None
            && !self.manual_pause
            && self.gameplay_focus()
            && !matches!(self.round.phase, Phase::Holed | Phase::Finished);
        session.set_paused(!active);
        if active && self.round.phase == Phase::Aiming && !self.auto {
            self.pointer_aim(scene, w, h, consumed);
        } else {
            self.drag_start = None;
            self.drag_touch = None;
            if !active {
                self.pending_shot = false;
            }
        }

        if active || self.sheet_open() && running {
            self.audio.resume();
        } else {
            self.audio.pause();
        }
        // The demonstration lingers on each result long enough to read it.
        if self.auto
            && self.round.phase == Phase::Holed
            && running
            && self.juice.celebrate > DEMO_HOLD
        {
            self.advance_hole();
        }
        if self.round.phase == Phase::Finished && !self.saved {
            self.finish_round();
        }
        if self.view == View::ThreeD && self.terrain_hole != self.round.hole_index {
            self.terrain = terrain_mesh(&self.round.course);
            self.terrain_hole = self.round.hole_index;
        }
        self.capture.end_frame();
    }
    fn advance_hole(&mut self) {
        if self.round.phase != Phase::Holed {
            return;
        }
        self.round.next_hole();
        if self.round.phase != Phase::Finished {
            self.aim = (self.round.course.cup - self.round.ball).normalize_or_zero();
            self.begin_hole();
        }
        self.auto_wait = 0.5;
    }
    fn finish_round(&mut self) {
        let total = self.round.total();
        let best = Best::compare(total, self.best);
        self.round_best = Some(best);
        let to_par = self.round_to_par();
        if !self.round.practice && (to_par < 0 || matches!(best, Best::New(_))) {
            self.juice.finale(self.round.course.cup);
        }
        if !self.round.practice && !self.auto {
            if self.best.is_none_or(|v| total < v) {
                self.best = Some(total);
            }
            if let Some(records) = &mut self.records {
                let _ = records.attempt("round", 12, 1000 - u64::from(total.min(1000)), None);
            }
        }
        self.saved = true;
    }
    fn pointer_aim(&mut self, scene: Scene, w: f32, h: f32, consumed: bool) {
        let contacts = touches();
        if !contacts.is_empty() || self.drag_touch.is_some() {
            if self.drag_touch.is_none()
                && !consumed
                && let Some(t) = contacts
                    .iter()
                    .find(|t| t.phase == TouchPhase::Started && scene.contains(t.position))
            {
                self.drag_touch = Some(t.id);
                self.drag_start = Some(t.position);
            }
            if let Some(id) = self.drag_touch {
                if let Some(t) = contacts.iter().find(|t| t.id == id) {
                    if scene.contains(t.position) && t.phase != TouchPhase::Cancelled {
                        self.aim_at(t.position, scene, w, h);
                    }
                    if matches!(t.phase, TouchPhase::Ended | TouchPhase::Cancelled) {
                        if t.phase == TouchPhase::Ended
                            && !consumed
                            && scene.contains(t.position)
                            && self.drag_start.is_some_and(|p| p.distance(t.position) > 8.)
                        {
                            self.pending_shot = true;
                        }
                        self.drag_touch = None;
                        self.drag_start = None;
                    }
                } else {
                    self.drag_touch = None;
                    self.drag_start = None;
                }
            }
        } else {
            let pointer = Vec2::from(mouse_position());
            self.aim_pointer = pointer;
            if !consumed && scene.contains(pointer) && is_mouse_button_pressed(MouseButton::Left) {
                self.drag_start = Some(pointer);
            }
            if self.drag_start.is_some()
                && is_mouse_button_down(MouseButton::Left)
                && scene.contains(pointer)
            {
                self.aim_at(pointer, scene, w, h);
            }
            if !is_mouse_button_down(MouseButton::Left)
                && let Some(start) = self.drag_start.take()
                && !consumed
                && scene.contains(pointer)
                && start.distance(pointer) > 8.
            {
                self.pending_shot = true;
            }
        }
    }
    fn aim_at(&mut self, pointer: Vec2, scene: Scene, w: f32, h: f32) {
        let to = world_pointer(self.view, scene, pointer, w, h) - self.round.ball;
        if to.length() > 0.3 {
            self.aim = to.normalize();
            self.power = (to.length() / 17.).clamp(0.05, 1.);
        }
    }
    fn round_to_par(&self) -> i16 {
        self.round.total() as i16 - self.round.par_played() as i16
    }
    /// The hole just ended (cup, cap or concession): score it once.
    fn settle(&mut self) {
        if self.round.phase != Phase::Holed || self.outcome.is_some() {
            return;
        }
        let i = self.round.hole_index;
        let picked = self.round.picked_up[i];
        let strokes = self.round.strokes;
        let o = self.stats.finish_hole(
            i,
            strokes,
            self.round.course.par,
            picked,
            self.round.stroke_banks,
            self.round.stroke_assist,
        );
        self.hole_best_before = self.hole_best[i];
        if !picked && !self.auto {
            if self.hole_best[i].is_none_or(|b| strokes < b) {
                self.hole_best[i] = Some(strokes);
            }
            if let Some(records) = &mut self.records {
                let _ = records.attempt("hole", i as u64, 100 - u64::from(strokes), None);
            }
        }
        self.juice.hole_done(self.round.course.cup, &o);
        match o.name.tone() {
            Tone::Ace => {
                self.audio.cue(Cue::Success, 0.9);
                self.audio.cue(Cue::Pickup, 0.7);
            }
            Tone::Under => {
                self.audio.cue(Cue::Success, 0.75);
                self.audio.cue(Cue::Pickup, 0.4);
            }
            Tone::Even => self.audio.cue(Cue::Pickup, 0.6),
            Tone::Over => self.audio.cue(Cue::Pickup, 0.4),
            Tone::Rough => self.audio.cue(Cue::Failure, 0.3),
        }
        self.outcome = Some(o);
        self.sheet = 0.;
    }
    pub fn tick(&mut self) {
        let actions = self
            .input
            .take_tick(["shoot", "left", "right", "more", "less", "drop"]);
        if self.round.phase == Phase::Aiming {
            self.aim =
                Vec2::from_angle((actions[2].value - actions[1].value) * 1.1 * crate::game::DT)
                    .rotate(self.aim);
            self.power = (self.power
                + (actions[3].value - actions[4].value) * 0.4 * crate::game::DT)
                .clamp(0.05, 1.);
            self.auto_wait -= crate::game::DT;
            if self.auto && self.auto_wait <= 0. {
                let (d, p) = self.round.suggested_shot();
                self.aim = d;
                self.power = p;
                self.pending_shot = true;
                self.auto_wait = 0.6;
            }
            if let Some((aim, power)) = self.capture.scripted_shot() {
                self.aim = aim;
                self.power = power;
                self.pending_shot = true;
            }
            if self.pending_shot || actions[0].pressed {
                if self.round.shoot(self.aim, self.power) {
                    self.audio
                        .cue(Cue::Impact(SoundSurface::Wood), 0.35 + 0.4 * self.power);
                    self.juice.putt(self.round.ball, self.aim, self.power);
                    self.putts = self.putts.saturating_add(1);
                }
                self.pending_shot = false;
            }
        }
        if actions[5].pressed && self.round.drop_ball() {
            self.returns = self.returns.wrapping_add(1);
            self.juice.returned(self.round.ball, self.returns);
            self.audio.cue(Cue::Click, 0.5);
        }
        let e = self.round.tick();
        self.feedback(e);
        self.juice.advance(crate::game::DT);
        self.settle();
    }
    /// Route one accepted tick's events to sound and juice.
    fn feedback(&mut self, e: Events) {
        let ball = self.round.ball;
        self.bank_cooldown -= crate::game::DT;
        if self.round.phase == Phase::Rolling {
            self.juice.roll(ball);
        }
        if e.bank {
            self.juice
                .bank(ball, self.round.velocity, e.impact_cms, e.moving_hit);
            if self.bank_cooldown <= 0. {
                let gain = 0.15 + 0.35 * (f32::from(e.impact_cms) / 700.).min(1.);
                let surface = if e.moving_hit {
                    SoundSurface::Metal
                } else {
                    SoundSurface::Stone
                };
                self.audio.cue(Cue::Impact(surface), gain);
                self.bank_cooldown = 0.12;
            }
        }
        if e.sand {
            self.sands = self.sands.wrapping_add(1);
            self.juice.sand(ball, self.sands);
            self.audio.cue(Cue::Footstep(SoundSurface::Sand), 0.5);
        }
        if e.water {
            self.splashes = self.splashes.wrapping_add(1);
            self.stats.splashes = self.stats.splashes.saturating_add(1);
            self.juice.water(ball, self.splashes);
            self.audio.cue(Cue::Impact(SoundSurface::Water), 0.65);
        }
        if e.lip_out {
            self.lips = self.lips.wrapping_add(1);
            self.stats.lip_outs = self.stats.lip_outs.saturating_add(1);
            self.juice.lip_out(self.round.course.cup, self.lips);
            self.audio.cue(Cue::Impact(SoundSurface::Metal), 0.4);
        }
        if e.stopped
            && self.round.phase == Phase::Aiming
            && ball.distance(self.round.course.cup) < 0.9
        {
            self.nears = self.nears.wrapping_add(1);
            self.juice.near_miss(ball, self.nears);
        }
    }
    pub fn draw(&mut self) {
        publish(&self.round, self.power);
        let (w, h) = (screen_width(), screen_height());
        let base = Scene::new(w, h);
        // While the between-hole sheet is up, the course re-fits above it so
        // the cup drop and confetti stay visible. Pointer mapping is unchanged.
        let mut scene = base;
        if self.sheet_open() {
            let t = ease_out(self.sheet);
            let bottom = h - 12. - self.sheet_h - 10.;
            if bottom - 14. > 120. {
                scene.y += (14. - scene.y) * t;
                scene.h += (bottom - 14. - scene.h) * t;
            }
        }
        let scene = scene.shifted(self.juice.shake());
        clear_background(Color::from_rgba(32, 62, 49, 255));
        if self.view == View::TwoD {
            self.draw_2d(scene);
        } else {
            self.draw_3d(scene, w, h);
        }
        set_default_camera();
        self.draw_juice(&Projector::new(self.view, scene, w, h));
        let flash = self.juice.flash_level();
        if flash > 0. {
            draw_rectangle(0., 0., w, h, fade(GOLD, 0.3 * flash));
        }
        self.ui.paint();
        self.input.draw_controls();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn projections_preserve_pointer_aim() {
        for (w, h) in [
            (320., 480.),
            (320., 640.),
            (390., 640.),
            (844., 390.),
            (400., 300.),
            (1280., 720.),
        ] {
            let s = Scene::new(w, h);
            for p in [vec2(2., 2.), vec2(9., 6.), vec2(16., 10.)] {
                let q = top_map(s, p);
                assert!(world_pointer(View::TwoD, s, q, w, h).distance(p) < 0.001);
                let q = camera(s, w, h).matrix.project_point3(vec3(p.x, 0., p.y));
                let screen = vec2((q.x + 1.) * w * 0.5, (1. - q.y) * h * 0.5);
                assert!(s.contains(screen));
                assert!(world_pointer(View::ThreeD, s, screen, w, h).distance(p) < 0.001);
            }
        }
    }
    #[test]
    fn demonstration_round_fits_the_browser_frame_budget() {
        // Browser regression: 2200 frames of 0.1 s (12 fixed ticks each) must
        // finish all twelve holes including the per-hole result linger.
        let mut r = Round::new();
        let mut ticks = 0usize;
        for _ in 0..12 {
            ticks += 60; // 0.5 s before the first demonstration putt.
            while r.phase != Phase::Holed {
                ticks += 72; // 0.6 s between demonstration putts.
                let (d, p) = r.suggested_shot();
                assert!(r.shoot(d, p));
                while r.phase == Phase::Rolling || r.phase == Phase::Water {
                    r.tick();
                    ticks += 1;
                }
            }
            r.next_hole();
        }
        assert_eq!(r.phase, Phase::Finished);
        let frames = ticks.div_ceil(12) + 12 * ((DEMO_HOLD / 0.1).ceil() as usize + 2);
        assert!(
            frames < 1600,
            "{frames} frames leaves too little margin under 2200"
        );
    }
    #[test]
    fn fullscreen_menu_corner_stays_clear_of_hud_text_and_controls() {
        let clear = |r: UiRect, w: f32, what: &str| {
            assert!(
                r.min[0] + r.size[0] <= w - CORNER.x || r.min[1] >= CORNER.y,
                "{what} {:?} enters the {w}px menu corner",
                r
            );
        };
        for (w, h) in [
            (1280., 800.),
            (1600., 1000.),
            (390., 844.),
            (844., 390.),
            (320., 480.),
        ] {
            let scene = Scene::new(w, h);
            // Score chip (button), its text and round badge sit inside it.
            clear(rect(10., 10., chip_width(w), 44.), w, "score chip");
            // Hole title card: spans the width but starts below the corner,
            // including its settle-in offset.
            assert!(scene.y + 6. >= CORNER.y, "title card at {w}x{h}");
            // Headline banner over the sheet starts at y 14: inset copy.
            for size in [12., 26., 34.] {
                clear(banner_text_rect(w, 20., size), w, "banner copy");
            }
            // Between-hole sheet (text at its top-right).
            let (sw, sh, _) = App::sheet_layout(w);
            clear(rect((w - sw) * 0.5, h - 12. - sh, sw, sh), w, "sheet");
            // Centered menus: welcome, pause, practice, card and results.
            let width = (w - 32.).min(420.);
            for height in [212., 264., 268., 276.] {
                let y = ((h - height) * 0.5).max(8.);
                clear(rect((w - width) * 0.5, y, width, height), w, "menu");
            }
            let width = (w - 32.).min(460.);
            for assists in [false, true] {
                let height = final_card_height(h, assists);
                let y = ((h - height) * 0.5).max(8.);
                clear(rect((w - width) * 0.5, y, width, height), w, "results");
            }
        }
    }
    #[test]
    fn effect_projection_matches_the_course_in_both_views() {
        for (w, h) in [(320., 640.), (1280., 800.), (1600., 1000.)] {
            let s = Scene::new(w, h);
            for view in [View::TwoD, View::ThreeD] {
                let p = Projector::new(view, s, w, h);
                for g in [vec2(2., 2.), vec2(14.5, 6.)] {
                    let q = p.screen(vec3(g.x, 0., g.y));
                    assert!(world_pointer(view, s, q, w, h).distance(g) < 0.001);
                    // Height always lifts an effect up the screen.
                    assert!(p.screen(vec3(g.x, 1., g.y)).y < q.y);
                }
                assert!(p.ppm() > 1.);
            }
        }
    }
    #[test]
    fn sheet_fits_every_supported_viewport_and_aim_preview_grows_with_power() {
        for (w, h) in [(320., 480.), (390., 640.), (400., 300.), (1280., 800.)] {
            let (sw, sh, cols) = App::sheet_layout(w);
            assert!(sw <= w - 24. && sh + 24. <= h, "{w}x{h}");
            assert!(cols == 6 || cols == 12);
        }
        assert!(preview_length(1.) > preview_length(0.3));
        assert!(preview_length(0.05) > 0.);
    }
}

thread_local! { static STATUS: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) }; }
fn publish(r: &Round, power: f32) {
    STATUS.with(|s| { use std::fmt::Write; let mut s=s.borrow_mut();s.clear();let _=write!(s,"{{\"version\":1,\"hole\":{},\"phase\":\"{:?}\",\"strokes\":{},\"total\":{},\"practice\":{},\"time\":{},\"power\":{},\"x\":{},\"y\":{},\"scores\":{:?}}}",r.hole_index+1,r.phase,r.strokes,r.total(),r.practice,r.time,power,r.ball.x,r.ball.y,r.scores.map(|v|v.map(i16::from).unwrap_or(-1))); });
}
/// Module-owned UTF-8 diagnostic snapshot; read before calling `status_len`.
/// Both views publish the same version-1 read-only representation.
pub fn status_ptr() -> *const u8 {
    STATUS.with(|s| s.borrow().as_ptr())
}
pub fn status_len() -> u32 {
    STATUS.with(|s| s.borrow().len() as u32)
}
