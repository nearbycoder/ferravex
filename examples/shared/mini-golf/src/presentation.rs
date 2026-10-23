//! Shared input/HUD with distinct top-down and actual mesh-based 3D renderers.
use crate::{
    course::{self, RADIUS, SIZE, Surface},
    game::{Phase, Round},
};
use ferravex_core::session::GameSession;
use ferravex_render::{
    audio::{Cue, GameAudio, Surface as SoundSurface},
    input::{ActionKind, Binding, InputCode, PlayerInput},
    mesh::Mesh3d,
    prelude::*,
    ui::{ButtonStyle, TextStyle, Ui, UiRect, Viewport},
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
const GREEN: Color = Color::from_rgba(100, 153, 107, 255);
const COPPER: Color = Color::from_rgba(199, 111, 65, 255);
const GOLD: Color = Color::from_rgba(232, 193, 109, 255);
fn rect(x: f32, y: f32, w: f32, h: f32) -> UiRect {
    UiRect {
        min: [x, y],
        size: [w, h],
    }
}
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
#[derive(Clone, Copy)]
struct GolfCamera {
    matrix: Mat4,
    eye: Vec3,
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
    }
}
fn top_map(scene: Scene, p: Vec2) -> Vec2 {
    let scale = (scene.w / 20.).min(scene.h / 14.);
    vec2(scene.x + scene.w * 0.5, scene.y + scene.h * 0.5) + (p - SIZE * 0.5) * scale
}
fn world_pointer(view: View, scene: Scene, p: Vec2, w: f32, h: f32) -> Vec2 {
    if view == View::TwoD {
        let scale = (scene.w / 20.).min(scene.h / 14.);
        (p - vec2(scene.x + scene.w * 0.5, scene.y + scene.h * 0.5)) / scale + SIZE * 0.5
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
    notice: &'static str,
    notice_time: f32,
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
        let best = records
            .as_ref()
            .and_then(|r| r.get("round", 12))
            .map(|r| (1000 - r.score.min(1000)) as u16);
        Self {
            round: Round::new(),
            view,
            input,
            ui: Ui::new(),
            audio: GameAudio::new(),
            menu: if autoplay { Menu::None } else { Menu::Welcome },
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
            notice: "",
            notice_time: 0.,
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
        }
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
    }
    /// Sample input and prepare screen-space UI once per frame. Physics edges
    /// remain pending until `tick`; UI and shared Controls own their gestures.
    pub fn prepare(&mut self, session: &mut GameSession, dt: f32) {
        self.input
            .sample(self.menu == Menu::None && !self.manual_pause);
        if self.input.take("pause").pressed {
            if self.menu == Menu::None {
                self.manual_pause = !self.manual_pause
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
        let w = screen_width();
        let h = screen_height();
        let scene = Scene::new(w, h);
        self.ui
            .begin(Viewport::responsive([w, h], [0.; 4]).expect("positive screen"));
        // Only the score and the shot belong in the permanent HUD. Course notes,
        // bindings and player preferences live in the on-demand game menu.
        let modal = self.menu != Menu::None
            || self.manual_pause
            || matches!(self.round.phase, Phase::Holed | Phase::Finished);
        if !modal {
            self.ui
                .button(
                    15,
                    rect(10., 10., (w - 80.).min(350.), 44.),
                    "",
                    true,
                    ButtonStyle {
                        fill: DARK,
                        focus: GOLD,
                        ..Default::default()
                    },
                )
                .unwrap();
            label(
                &mut self.ui,
                rect(20., 15., w - 110., 18.),
                &format!(
                    "{:02}/12   PAR {}",
                    self.round.hole_index + 1,
                    self.round.course.par
                ),
                14.,
                PAPER,
            );
            label(
                &mut self.ui,
                rect(20., 35., w - 110., 17.),
                &format!(
                    "{} SHOTS / {} TOTAL",
                    self.round.strokes,
                    self.round.total()
                ),
                13.,
                GOLD,
            );
            let dock_w = (w - 24.).min(420.);
            let x = (w - dock_w) * 0.5;
            let y = h - 64.;
            let gap = 6.;
            let bw = (dock_w - gap * 4.) / 5.;
            if self.round.phase == Phase::Aiming {
                self.ui.panel(rect(x, y - 12., dock_w, 4.), DARK).unwrap();
                self.ui
                    .panel(rect(x, y - 12., dock_w * self.power, 4.), COPPER)
                    .unwrap();
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
            if self.notice_time > 0. {
                label(
                    &mut self.ui,
                    rect(16., 58., w - 32., 20.),
                    self.notice,
                    13.,
                    GOLD,
                );
            }
        }
        if modal {
            self.modal(w, h);
        }
        let controls_open = self.input.controls_open();
        self.ui.set_auto_focus(modal);
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
            self.round.next_hole();
            self.aim = (self.round.course.cup - self.round.ball).normalize_or_zero();
            self.auto_wait = 0.5;
        }
        if self.ui.activated(25) {
            self.round.pick_up();
        }
        for i in 0..12 {
            if self.ui.activated(100 + i as u64) {
                self.round.practice_hole(i);
                self.menu = Menu::None;
                self.manual_pause = false;
                self.aim = (self.round.course.route[0] - self.round.ball).normalize_or_zero();
                self.saved = false;
            }
        }
        let active = self.menu == Menu::None
            && !self.manual_pause
            && self.input.gameplay_active()
            && !matches!(self.round.phase, Phase::Holed | Phase::Finished);
        session.set_paused(!active);
        if active && self.round.phase == Phase::Aiming && !self.auto {
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
                if !consumed
                    && scene.contains(pointer)
                    && is_mouse_button_pressed(MouseButton::Left)
                {
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
        } else {
            self.drag_start = None;
            self.drag_touch = None;
            if !active {
                self.pending_shot = false;
            }
        }

        if active {
            self.audio.resume();
        } else {
            self.audio.pause();
        }
        if self.auto
            && self.round.phase == Phase::Holed
            && self.input.gameplay_active()
            && self.menu == Menu::None
            && !self.manual_pause
        {
            self.round.next_hole();
            self.auto_wait = 0.8;
        }
        if self.round.phase == Phase::Finished && !self.saved && !self.round.practice && !self.auto
        {
            let total = self.round.total();
            if self.best.is_none_or(|v| total < v) {
                self.best = Some(total);
                if let Some(records) = &mut self.records {
                    let _ = records.attempt("round", 12, 1000 - u64::from(total), None);
                }
            }
            self.saved = true;
        }
        if self.view == View::ThreeD && self.terrain_hole != self.round.hole_index {
            self.terrain = terrain_mesh(&self.round.course);
            self.terrain_hole = self.round.hole_index;
        }
        let _ = dt;
    }
    fn aim_at(&mut self, pointer: Vec2, scene: Scene, w: f32, h: f32) {
        let to = world_pointer(self.view, scene, pointer, w, h) - self.round.ball;
        if to.length() > 0.3 {
            self.aim = to.normalize();
            self.power = (to.length() / 17.).clamp(0.05, 1.);
        }
    }
    fn modal(&mut self, w: f32, h: f32) {
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
        self.ui.panel(rect(x, y, width, height), DARK).unwrap();
        if self.menu == Menu::Practice {
            label(
                &mut self.ui,
                rect(tx, y + 16., tw, 26.),
                "PRACTICE",
                20.,
                PAPER,
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
            label(
                &mut self.ui,
                rect(tx, y + 16., tw, 26.),
                "SCORECARD",
                20.,
                PAPER,
            );
            let cw = (tw - 12.) * 0.5;
            for i in 0..12 {
                let score = self.round.scores[i]
                    .map(|s| format!("{}{}", s, if self.round.picked_up[i] { "*" } else { "" }))
                    .unwrap_or_else(|| "-".into());
                label(
                    &mut self.ui,
                    rect(
                        tx + (i / 6) as f32 * (cw + 12.),
                        y + 54. + (i % 6) as f32 * 20.,
                        cw,
                        21.,
                    ),
                    &format!("{:02}  {}/{}", i + 1, score, course::hole(i).par),
                    14.,
                    PAPER,
                );
            }
            label(
                &mut self.ui,
                rect(tx, y + 184., tw, 23.),
                &format!(
                    "TOTAL {}  ({:+})",
                    self.round.total(),
                    self.round.total() as i16 - self.round.par_played() as i16
                ),
                16.,
                GOLD,
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

            return;
        }
        let heading = if self.manual_pause {
            "PAUSED"
        } else if self.round.phase == Phase::Finished {
            "ROUND COMPLETE"
        } else if self.round.phase == Phase::Holed {
            "IN THE CUP"
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
        let detail = if matches!(self.round.phase, Phase::Holed | Phase::Finished) {
            format!(
                "{} STROKES  /  {:+} TO PAR",
                self.round.total(),
                self.round.total() as i16 - self.round.par_played() as i16
            )
        } else if self.manual_pause {
            "".into()
        } else {
            "12 HOLES. ONE GREAT ROUND.".into()
        };
        label(&mut self.ui, rect(tx, y + 58., tw, 22.), &detail, 13., GOLD);
        let first = if self.round.phase == Phase::Holed {
            24
        } else if self.round.phase == Phase::Finished {
            21
        } else {
            20
        };
        let action = if first == 24 {
            if self.round.hole_index == 11 {
                "FINISH"
            } else {
                "NEXT HOLE"
            }
        } else if first == 21 {
            "PLAY AGAIN"
        } else if self.manual_pause {
            "RESUME"
        } else {
            "PLAY"
        };
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
                true,
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
            if self.pending_shot || actions[0].pressed {
                if self.round.shoot(self.aim, self.power) {
                    self.audio.cue(Cue::Impact(SoundSurface::Wood), 0.6);
                }
                self.pending_shot = false;
            }
        }
        if actions[5].pressed && self.round.drop_ball() {
            self.notice = "RETURNED TO SAFE LIE / +1";
            self.notice_time = 2.;
        }
        let e = self.round.tick();
        self.bank_cooldown -= crate::game::DT;
        self.notice_time -= crate::game::DT;
        if e.bank && self.bank_cooldown <= 0. {
            self.audio.cue(Cue::Impact(SoundSurface::Stone), 0.32);
            self.bank_cooldown = 0.12;
        }
        if e.water {
            self.audio.cue(Cue::Impact(SoundSurface::Water), 0.65);
            self.notice = "WATER / RETURNING / +1";
            self.notice_time = 2.;
        }
        if e.holed {
            self.audio.cue(Cue::Success, 0.7);
        }
    }
    pub fn draw(&mut self) {
        publish(&self.round, self.power);
        let (w, h) = (screen_width(), screen_height());
        let scene = Scene::new(w, h);
        clear_background(Color::from_rgba(32, 62, 49, 255));
        if self.view == View::TwoD {
            self.draw_2d(scene);
        } else {
            self.draw_3d(scene, w, h);
        }
        set_default_camera();
        self.ui.paint();
        self.input.draw_controls();
    }
    fn draw_2d(&self, scene: Scene) {
        let scale = (scene.w / 20.).min(scene.h / 14.);
        let origin = top_map(scene, Vec2::ZERO);
        draw_rectangle(
            origin.x - 6.,
            origin.y - 4.,
            SIZE.x * scale + 12.,
            SIZE.y * scale + 12.,
            Color::from_rgba(18, 39, 31, 255),
        );
        for x in 0..18 {
            draw_rectangle(
                origin.x + x as f32 * scale,
                origin.y,
                scale,
                SIZE.y * scale,
                if x % 2 == 0 {
                    GREEN
                } else {
                    Color::from_rgba(91, 143, 98, 255)
                },
            );
        }
        for p in &self.round.course.patches {
            let a = top_map(scene, p.min);
            let col = match p.surface {
                Surface::Water => Color::from_rgba(62, 134, 157, 255),
                Surface::Sand => GOLD,
                Surface::Slope => Color::from_rgba(122, 169, 105, 255),
            };
            draw_rectangle(a.x, a.y, p.size.x * scale, p.size.y * scale, col);
            for j in 1..5 {
                let yy = a.y + p.size.y * scale * j as f32 / 5.;
                draw_line(
                    a.x + 3.,
                    yy,
                    a.x + p.size.x * scale - 3.,
                    yy,
                    1.,
                    Color::new(1., 1., 1., 0.12),
                );
            }
        }
        for i in 0..12 {
            let p = top_map(scene, vec2(0.6 + i as f32 * 1.5, -0.55));
            draw_circle(p.x, p.y, scale * 0.19, Color::from_rgba(73, 102, 66, 255));
            let p = top_map(scene, vec2(0.6 + i as f32 * 1.5, 12.55));
            draw_circle(p.x, p.y, scale * 0.19, Color::from_rgba(73, 102, 66, 255));
        }
        for z in [-0.75, 12.75] {
            for x in [2., 5., 8., 11., 14., 17.] {
                let p = top_map(scene, vec2(x, z));
                draw_circle(p.x, p.y, scale * 0.28, Color::from_rgba(47, 82, 54, 255));
                for v in [vec2(-0.1, -0.05), vec2(0.12, 0.1)] {
                    let q = p + v * scale;
                    draw_circle(q.x, q.y, (scale * 0.075).max(1.2), GOLD);
                }
            }
        }
        let tee = top_map(scene, self.round.course.tee);
        draw_circle_lines(
            tee.x,
            tee.y,
            scale * 0.34,
            1.,
            Color::new(0.96, 0.94, 0.83, 0.4),
        );
        for &wall in &self.round.course.walls {
            let (c, a) = wall.pose(self.round.time);
            let r = Vec2::from_angle(a);
            let v = [
                vec2(-wall.half.x, -wall.half.y),
                vec2(wall.half.x, -wall.half.y),
                vec2(wall.half.x, wall.half.y),
                vec2(-wall.half.x, wall.half.y),
            ]
            .map(|q| top_map(scene, c + r.rotate(q)));
            let col = if wall.spin != 0. || wall.travel != Vec2::ZERO {
                COPPER
            } else {
                Color::from_rgba(217, 207, 175, 255)
            };
            draw_triangle(
                v[0] + vec2(2., 3.),
                v[1] + vec2(2., 3.),
                v[2] + vec2(2., 3.),
                DARK,
            );
            draw_triangle(v[0], v[1], v[2], col);
            draw_triangle(v[0], v[2], v[3], col);
        }
        let cup = top_map(scene, self.round.course.cup);
        draw_circle(cup.x, cup.y, scale * 0.4, PAPER);
        draw_circle(cup.x, cup.y, scale * 0.3, DARK);
        draw_line(cup.x, cup.y, cup.x, cup.y - 22., 2., PAPER);
        draw_triangle(
            vec2(cup.x, cup.y - 22.),
            vec2(cup.x + 12., cup.y - 18.),
            vec2(cup.x, cup.y - 13.),
            COPPER,
        );
        if self.round.phase == Phase::Aiming {
            for i in 1..=14 {
                let d = i as f32 * 0.23;
                let p = top_map(scene, self.round.ball + self.aim * d);
                draw_circle(p.x, p.y, if i % 3 == 0 { 2.2 } else { 1.5 }, PAPER);
            }
            let p = top_map(scene, self.round.ball - self.aim * 0.7);
            let t = vec2(-self.aim.y, self.aim.x) * scale * 0.35;
            draw_line(p.x - t.x, p.y - t.y, p.x + t.x, p.y + t.y, 4., COPPER);
        }
        if self.round.phase != Phase::Water {
            let b = top_map(scene, self.round.ball);
            draw_circle(
                b.x + 2.,
                b.y + 3.,
                (RADIUS * scale).max(3.),
                Color::new(0., 0., 0., 0.24),
            );
            draw_circle(b.x, b.y, (RADIUS * scale).max(3.5), PAPER);
            draw_circle(b.x - 1., b.y - 1., 1.2, WHITE);
        }
    }
    fn draw_3d(&self, scene: Scene, w: f32, h: f32) {
        let cam = camera(scene, w, h);
        set_camera(&cam);
        self.material.begin(ferravex_render::shaders::LitSettings {
            camera_position: cam.eye,
            ambient: 0.55,
            fog_start: 100.,
            fog_end: 200.,
            ..Default::default()
        });
        draw_cube(vec3(9., -0.42, 6.), vec3(19., 0.7, 13.), None, DARK);
        for x in 0..18 {
            draw_cube(
                vec3(x as f32 + 0.5, -0.05, 6.),
                vec3(1., 0.1, 12.),
                None,
                if x % 2 == 0 {
                    GREEN
                } else {
                    Color::from_rgba(91, 143, 98, 255)
                },
            );
        }
        // The raised surface is tessellated from the exact shared heightfield.
        for patch in &self.round.course.patches {
            let p = patch;
            let col = match p.surface {
                Surface::Water => Color::from_rgba(53, 121, 146, 255),
                Surface::Sand => GOLD,
                Surface::Slope => Color::from_rgba(122, 169, 105, 255),
            };
            if p.surface == Surface::Slope {
                // Prepared terrain below samples the same heightfield as physics.
            } else {
                draw_cube(
                    vec3(p.min.x + p.size.x * 0.5, 0.015, p.min.y + p.size.y * 0.5),
                    vec3(p.size.x, 0.035, p.size.y),
                    None,
                    col,
                );
            }
        }
        if let Some(terrain) = &self.terrain {
            terrain.draw(Vec3::ZERO, Quat::IDENTITY);
        }
        for (x, z, sx, sz) in [
            (9., -0.16, 18.6, 0.3),
            (9., 12.16, 18.6, 0.3),
            (-0.16, 6., 0.3, 12.),
            (18.16, 6., 0.3, 12.),
        ] {
            draw_cube(
                vec3(x, 0.18, z),
                vec3(sx, 0.36, sz),
                None,
                Color::from_rgba(214, 203, 169, 255),
            );
        }
        for wall in &self.round.course.walls {
            let (c, a) = wall.pose(self.round.time);
            let mesh = if wall.spin != 0. || wall.travel != Vec2::ZERO {
                &self.copper_mesh
            } else {
                &self.stone_mesh
            };
            let model = Mat4::from_scale_rotation_translation(
                vec3(wall.half.x * 2., 0.65, wall.half.y * 2.),
                Quat::from_rotation_y(-a),
                vec3(c.x, 0.325, c.y),
            );
            mesh.draw_transformed(model);
        }
        let cup = self.round.course.cup;
        self.cup_mesh
            .draw(vec3(cup.x, 0.025, cup.y), Quat::IDENTITY);
        self.post_mesh.draw(vec3(cup.x, 0.8, cup.y), Quat::IDENTITY);
        draw_cube(
            vec3(cup.x + 0.28, 1.45, cup.y),
            vec3(0.55, 0.3, 0.04),
            None,
            COPPER,
        );
        for i in 0..10 {
            let x = 0.6 + i as f32 * 1.85;
            for z in [-0.7, 12.7] {
                draw_cube(
                    vec3(x, 0.12, z),
                    vec3(0.4, 0.25, 0.4),
                    None,
                    Color::from_rgba(61, 91, 58, 255),
                );
            }
        }
        if self.round.phase == Phase::Aiming {
            for i in 1..=14 {
                let p = self.round.ball + self.aim * (i as f32 * 0.23);
                draw_cube(
                    vec3(p.x, self.round.course.height(p) + 0.04, p.y),
                    vec3(0.055, 0.025, 0.055),
                    None,
                    PAPER,
                );
            }
        }
        if self.round.phase != Phase::Water {
            let b = self.round.ball;
            let y = self.round.course.height(b);
            draw_cube(
                vec3(b.x, y + 0.025, b.y),
                vec3(0.35, 0.02, 0.35),
                None,
                Color::from_rgba(47, 83, 56, 255),
            );
            self.ball_mesh.draw(
                vec3(b.x, y + RADIUS, b.y),
                Quat::from_rotation_z(self.round.time),
            );
        }
        self.material.end();
        set_default_camera();
    }
}
fn terrain_mesh(course: &course::Hole) -> Option<Mesh3d> {
    let patch = course
        .patches
        .iter()
        .find(|p| p.surface == Surface::Slope)?;
    let mut vertices = Vec::with_capacity(25 * 17);
    let mut indices = Vec::with_capacity(24 * 16 * 6);
    for z in 0..=16 {
        for x in 0..=24 {
            let p = patch.min + patch.size * vec2(x as f32 / 24., z as f32 / 16.);
            let g = course.slope(p);
            let mut v = Vertex::new2(
                vec3(p.x, course.height(p) + 0.025, p.y),
                Vec2::ZERO,
                Color::from_rgba(122, 169, 105, 255),
            );
            v.normal = vec3(-g.x, 1., -g.y).normalize().extend(0.);
            vertices.push(v);
        }
    }
    for z in 0..16 {
        for x in 0..24 {
            let a = (z * 25 + x) as u16;
            indices.extend_from_slice(&[a, a + 25, a + 1, a + 1, a + 25, a + 26]);
        }
    }
    Some(Mesh3d::from_mesh(Mesh {
        vertices,
        indices,
        texture: None,
    }))
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
