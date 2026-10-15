//! Original immediate/retained renderer and platform ABI. Game code, input, geometry and
//! scheduling live in Rust; hosts only bind OS events and GPU submission.
pub use ferravex_math::{Color, Mat4, Quat, Rect, Vec2, Vec3, Vec4, vec2, vec3, vec4};
use std::{
    cell::RefCell,
    collections::BTreeSet,
    future::Future,
    pin::Pin,
    task::{Context, Poll, Waker},
};
pub mod ao;
pub mod audio;
pub mod canvas;
pub mod demo;
pub mod effects;
mod font;
pub mod input;
pub mod lighting;
pub mod material;
pub mod mesh;
#[cfg(not(target_arch = "wasm32"))]
pub mod native;
pub mod retained;
pub mod shaders;
pub mod skinning;
pub mod ui;
pub mod visibility;
pub use ferravex_core::telemetry;

pub mod prelude {
    pub use crate::{
        BLACK, BLUE, Camera, Camera2D, Camera3D, Color, Conf, DARKGRAY, Font, GRAY, GREEN, KeyCode,
        LIGHTGRAY, Mat4, Mesh, MouseButton, ORANGE, Quat, RED, Rect, TextDimensions, TextParams,
        Touch, TouchPhase, Vec2, Vec3, Vec4, Vertex, WHITE, YELLOW, clear_background, draw_circle,
        draw_circle_lines, draw_cube, draw_ellipse, draw_line, draw_line_3d, draw_mesh,
        draw_mesh_transformed, draw_poly, draw_rectangle, draw_rectangle_lines, draw_text,
        draw_text_ex, draw_triangle, get_default_font, get_frame_time, get_screen_data, get_time,
        is_key_down, is_key_pressed, is_mouse_button_down, is_mouse_button_pressed, measure_text,
        mouse_delta_position, mouse_position, mouse_wheel, next_frame, screen_dpi_scale,
        screen_height, screen_width, set_camera, set_cursor_grab, set_default_camera, show_mouse,
        simulate_mouse_with_touch, touches, vec2, vec3, vec4,
    };
}
pub const WHITE: Color = Color::new(1., 1., 1., 1.);
pub const BLACK: Color = Color::new(0., 0., 0., 1.);
pub const RED: Color = Color::new(0.9, 0.2, 0.2, 1.);
pub const GREEN: Color = Color::new(0.2, 0.8, 0.3, 1.);
pub const BLUE: Color = Color::new(0.2, 0.4, 0.9, 1.);
pub const YELLOW: Color = Color::new(1., 0.85, 0.2, 1.);
pub const GRAY: Color = Color::new(0.5, 0.5, 0.5, 1.);
pub const DARKGRAY: Color = Color::new(0.2, 0.2, 0.2, 1.);
pub const LIGHTGRAY: Color = Color::new(0.8, 0.8, 0.8, 1.);
pub const ORANGE: Color = Color::new(1., 0.5, 0.2, 1.);
#[derive(Clone, Debug)]
pub struct Conf {
    pub window_title: String,
    pub window_width: i32,
    pub window_height: i32,
    pub high_dpi: bool,
    pub sample_count: i32,
}
impl Default for Conf {
    fn default() -> Self {
        window_conf("Ferravex")
    }
}
pub fn window_conf(title: &str) -> Conf {
    Conf {
        window_title: title.into(),
        window_width: 1280,
        window_height: 800,
        high_dpi: true,
        sample_count: 4,
    }
}
#[derive(Clone, Debug, Default)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u16>,
    pub texture: Option<()>,
}
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Vertex {
    pub position: Vec3,
    pub uv: Vec2,
    pub color: [u8; 4],
    pub normal: Vec4,
}
impl Vertex {
    pub fn new2(position: Vec3, uv: Vec2, color: Color) -> Self {
        Self {
            position,
            uv,
            color: color.to_rgba(),
            normal: Vec4::Y,
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Batch {
    pub start: u32,
    pub count: u32,
    pub depth: bool,
}
#[derive(Clone, Copy, Debug)]
pub struct Camera2D {
    pub target: Vec2,
    pub zoom: Vec2,
    pub offset: Vec2,
    pub rotation: f32,
}
impl Default for Camera2D {
    fn default() -> Self {
        Self {
            target: Vec2::ZERO,
            zoom: Vec2::ONE,
            offset: Vec2::ZERO,
            rotation: 0.,
        }
    }
}
impl Camera2D {
    pub fn from_display_rect(r: Rect) -> Self {
        Self {
            target: r.center(),
            zoom: vec2(2. / r.w, 2. / r.h),
            ..Self::default()
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Camera3D {
    pub position: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    pub fovy: f32,
    pub aspect: Option<f32>,
    pub z_near: f32,
    pub z_far: f32,
}
impl Default for Camera3D {
    fn default() -> Self {
        Self {
            position: vec3(0., 5., 10.),
            target: Vec3::ZERO,
            up: Vec3::Y,
            fovy: 45f32.to_radians(),
            aspect: None,
            z_near: 0.1,
            z_far: 1000.,
        }
    }
}
pub trait Camera {
    fn matrix(&self, w: f32, h: f32) -> Mat4;
    fn depth(&self) -> bool;
    fn position(&self) -> Option<Vec3> {
        None
    }
}
impl Camera for Camera2D {
    fn matrix(&self, _: f32, _: f32) -> Mat4 {
        Mat4::from_translation(self.offset.extend(0.))
            * Mat4::from_scale(vec3(self.zoom.x, -self.zoom.y, 1.))
            * Mat4::from_rotation_z(-self.rotation.to_radians())
            * Mat4::from_translation((-self.target).extend(0.))
    }
    fn depth(&self) -> bool {
        false
    }
}
impl Camera for Camera3D {
    fn position(&self) -> Option<Vec3> {
        Some(self.position)
    }
    fn matrix(&self, w: f32, h: f32) -> Mat4 {
        Mat4::perspective_rh_gl(
            self.fovy,
            self.aspect.unwrap_or(w / h),
            self.z_near,
            self.z_far,
        ) * Mat4::look_at_rh(self.position, self.target, self.up)
    }
    fn depth(&self) -> bool {
        true
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum KeyCode {
    F1 = 0xffbe,
    F2 = 0xffbf,
    F3 = 0xffc0,
    Space = 32,
    A = 97,
    B = 98,
    C = 99,
    D = 100,
    E = 101,
    F = 102,
    G = 103,
    H = 104,
    I = 105,
    J = 106,
    K = 107,
    L = 108,
    M = 109,
    N = 110,
    O = 111,
    P = 112,
    Q = 113,
    R = 114,
    S = 115,
    T = 116,
    U = 117,
    V = 118,
    W = 119,
    X = 120,
    Y = 121,
    Z = 122,
    Key0 = 48,
    Key1 = 49,
    Key2 = 50,
    Key3 = 51,
    Key4 = 52,
    Escape = 0xff1b,
    Enter = 0xff0d,
    Tab = 0xff09,
    Backspace = 0xff08,
    Left = 0xff51,
    Up = 0xff52,
    Right = 0xff53,
    Down = 0xff54,
    LeftShift = 0xffe1,
    RightShift = 0xffe2,
    LeftControl = 0xffe3,
    RightControl = 0xffe4,
}
#[derive(Clone, Copy, Debug)]
#[repr(u32)]
pub enum MouseButton {
    Left = 1,
    Middle = 2,
    Right = 3,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TouchPhase {
    Started,
    Moved,
    Stationary,
    Ended,
    Cancelled,
}
#[derive(Clone, Copy, Debug)]
pub struct Touch {
    pub id: u64,
    pub position: Vec2,
    pub phase: TouchPhase,
}
struct State {
    w: u32,
    h: u32,
    dt: f32,
    time: f64,
    matrix: Mat4,
    depth: bool,
    frustum: Option<ferravex_graphics::Frustum>,
    eye: Option<Vec3>,
    visibility: visibility::VisibilitySettings,
    visibility_stats: visibility::VisibilityStats,
    clear: Color,
    vertices: Vec<[f32; 8]>,
    batches: Vec<Batch>,
    retained: retained::Frame,
    effect_stats: effects::EffectStats,
    lighting: Option<shaders::LitSettings>,
    shader: Option<material::ShaderMaterial>,
    shader_params: [f32; 16],
    quality: lighting::Quality,
    quality_override: Option<lighting::Quality>,
    points: Vec<lighting::PointLight>,
    sun: Option<lighting::SunShadow>,
    input: input::PlatformInput,
    input_enabled: bool,
    controls_open: bool,
    keys: BTreeSet<u32>,
    pressed: BTreeSet<u32>,
    buttons: BTreeSet<u32>,
    touch_buttons: BTreeSet<u32>,
    clicks: BTreeSet<u32>,
    mouse: Vec2,
    delta: Vec2,
    wheel: f32,
    wheel_x: f32,
    touches: Vec<Touch>,
    grab: bool,
    visible: bool,
    touch_mouse: bool,
    #[cfg(not(target_arch = "wasm32"))]
    screenshots: Vec<String>,
    future: Option<Pin<Box<dyn Future<Output = ()>>>>,
    autoplay: bool,
    frame: u64,
    in_frame: bool,
}
impl Default for State {
    fn default() -> Self {
        let mut s = Self {
            w: 1280,
            h: 800,
            dt: 1. / 60.,
            time: 0.,
            matrix: Mat4::IDENTITY,
            depth: false,
            frustum: None,
            eye: None,
            visibility: Default::default(),
            visibility_stats: Default::default(),
            clear: BLACK,
            vertices: Vec::new(),
            batches: Vec::new(),
            retained: retained::Frame::default(),
            effect_stats: effects::EffectStats::default(),
            lighting: None,
            shader: None,
            shader_params: [0.; 16],
            quality: Default::default(),
            quality_override: None,
            points: Vec::new(),
            sun: None,
            input: Default::default(),
            input_enabled: false,
            controls_open: false,
            keys: BTreeSet::new(),
            pressed: BTreeSet::new(),
            buttons: BTreeSet::new(),
            touch_buttons: BTreeSet::new(),
            clicks: BTreeSet::new(),
            mouse: Vec2::ZERO,
            delta: Vec2::ZERO,
            wheel: 0.,
            wheel_x: 0.,
            touches: Vec::new(),
            grab: false,
            visible: true,
            touch_mouse: true,
            #[cfg(not(target_arch = "wasm32"))]
            screenshots: Vec::new(),
            future: None,
            autoplay: false,
            frame: 0,
            in_frame: false,
        };
        s.default_camera();
        s
    }
}
impl State {
    fn refresh_environment(&mut self) {
        self.retained
            .environment
            .update(&self.points, self.sun, self.quality, self.eye);
    }
    fn submit(
        &mut self,
        mesh: &retained::ResidentMesh,
        model: Mat4,
        bounds: ferravex_graphics::Bounds,
        skin: Option<&skinning::SkinPalette>,
    ) -> bool {
        let visible = self.test_visibility(bounds, model);
        let casts = self.depth
            && mesh.is_opaque()
            && self.shader.as_ref().is_none_or(|m| {
                m.0.options.blend == material::BlendMode::Opaque && m.0.options.casts_shadows
            })
            && self
                .retained
                .environment
                .frustum()
                .is_some_and(|f| f.intersects_bounds(bounds, model));
        if !visible && !casts {
            return false;
        }
        let recorded = self.retained.record_style(
            mesh,
            model,
            self.matrix,
            self.depth,
            retained::DrawStyle {
                light: self.lighting,
                material: self.shader.as_ref(),
                params: self.shader_params,
                shadow_only: !visible,
                skip_shadow: !casts,
                skin,
            },
        );
        visible && recorded
    }
    fn test_visibility(&mut self, bounds: ferravex_graphics::Bounds, model: Mat4) -> bool {
        use visibility::Visibility;
        self.visibility_stats.tested += 1;
        match visibility::classify(bounds, model, self.frustum, self.eye, self.visibility) {
            Visibility::Visible => {
                self.visibility_stats.visible += 1;
                true
            }
            Visibility::OutsideFrustum => {
                self.visibility_stats.frustum_culled += 1;
                false
            }
            Visibility::TooDistant => {
                self.visibility_stats.distance_culled += 1;
                false
            }
            Visibility::InvalidTransform => false,
        }
    }
    fn default_camera(&mut self) {
        self.matrix = Mat4::orthographic_rh_gl(0., self.w as f32, self.h as f32, 0., -1., 1.);
        self.depth = false;
        self.frustum = None;
        self.eye = None;
    }
    fn push(&mut self, vertices: impl IntoIterator<Item = (Vec3, Vec3, Color)>, model: Mat4) {
        let start = self.vertices.len() as u32;
        let normal_matrix = if self.lighting.is_some() {
            model
                .try_inverse()
                .map(|m| m.transpose())
                .unwrap_or(Mat4::IDENTITY)
        } else {
            Mat4::IDENTITY
        };
        for (p, n, mut c) in vertices {
            let p = model.transform_point3(p);
            if let Some(l) = self.lighting {
                c = l.shade_vertex(p, normal_matrix.transform_vector3(n), c);
            }
            let v = self.matrix * p.extend(1.);
            self.vertices.push([v.x, v.y, v.z, v.w, c.r, c.g, c.b, c.a]);
        }
        let count = self.vertices.len() as u32 - start;
        if count == 0 {
            return;
        }
        self.record_immediate(start, count, self.depth);
    }
    fn record_immediate(&mut self, start: u32, count: u32, depth: bool) {
        if let Some(last) = self.batches.last_mut().filter(|b| {
            b.depth == depth && self.retained.commands.last().is_some_and(|c| c.kind == 0)
        }) {
            last.count += count;
        } else {
            self.retained.commands.push(retained::Command {
                kind: 0,
                index: self.batches.len() as u32,
            });
            self.batches.push(Batch {
                start,
                count,
                depth,
            });
        }
    }
}
thread_local! {static STATE:RefCell<State>=RefCell::new(State::default());}
fn read<T>(f: impl FnOnce(&State) -> T) -> T {
    STATE.with(|s| f(&s.borrow()))
}
fn edit<T>(f: impl FnOnce(&mut State) -> T) -> T {
    STATE.with(|s| f(&mut s.borrow_mut()))
}
pub fn set_camera(camera: &impl Camera) {
    edit(|s| {
        s.matrix = camera.matrix(s.w as f32, s.h as f32);
        s.depth = camera.depth();
        s.eye = camera.position();
        if s.eye.is_some() {
            s.refresh_environment();
        }
        s.frustum = if s.depth {
            ferravex_graphics::Frustum::from_matrix(
                s.matrix,
                ferravex_graphics::ClipDepth::NegativeOneToOne,
            )
        } else {
            None
        };
    });
}
pub fn set_default_camera() {
    edit(State::default_camera)
}
pub fn set_lighting(settings: Option<shaders::LitSettings>) {
    edit(|s| s.lighting = settings)
}
pub fn clear_background(c: Color) {
    edit(|s| {
        s.clear = c;
        s.vertices.clear();
        s.batches.clear();
        s.retained.clear();
        s.effect_stats = effects::EffectStats::default();
        s.points.clear();
        s.sun = None;
        s.refresh_environment();
        s.visibility_stats = Default::default();
    });
}
pub fn draw_mesh(m: &Mesh) {
    draw_mesh_transformed(m, Mat4::IDENTITY)
}
/// Submit an immutable mesh snapshot. Compatible adjacent draws share one instanced call.
/// Returns false when culled, for invalid transforms or a full frame instance budget.
pub(crate) fn set_shader(shader: Option<material::ShaderMaterial>, params: [f32; 16]) {
    edit(|s| {
        s.shader = shader;
        s.shader_params = params;
    });
}
pub fn draw_resident(mesh: &retained::ResidentMesh, model: Mat4) -> bool {
    edit(|s| {
        let bounds = s
            .shader
            .as_ref()
            .map_or(mesh.bounds(), |m| m.padded(mesh.bounds()));
        s.submit(mesh, model, bounds, None)
    })
}
/// Submit a hidden opaque caster, for example a first-person character body.
pub fn draw_shadow(mesh: &retained::ResidentMesh, model: Mat4) -> bool {
    draw_shadow_pose(mesh, model, mesh.bounds(), None)
}
/// Record a GPU-skinned mesh using conservative deformed bounds. Position and
/// inverse-transpose normal palettes are copied into this frame; later palette
/// edits cannot change already recorded draws or their matching shadow pass.
pub fn draw_skinned(
    mesh: &skinning::SkinnedMesh,
    model: Mat4,
    palette: &skinning::SkinPalette,
) -> bool {
    let Ok(bounds) = mesh.bounds(palette) else {
        return false;
    };
    edit(|s| {
        let bounds = s.shader.as_ref().map_or(bounds, |m| m.padded(bounds));
        s.submit(mesh.resident(), model, bounds, Some(palette))
    })
}
pub fn draw_skinned_shadow(
    mesh: &skinning::SkinnedMesh,
    model: Mat4,
    palette: &skinning::SkinPalette,
) -> bool {
    let Ok(bounds) = mesh.bounds(palette) else {
        return false;
    };
    draw_shadow_pose(mesh.resident(), model, bounds, Some(palette))
}
fn draw_shadow_pose(
    mesh: &retained::ResidentMesh,
    model: Mat4,
    bounds: ferravex_graphics::Bounds,
    skin: Option<&skinning::SkinPalette>,
) -> bool {
    edit(|s| {
        let bounds = s.shader.as_ref().map_or(bounds, |m| m.padded(bounds));
        if !s.depth
            || !mesh.is_opaque()
            || s.shader.as_ref().is_some_and(|m| {
                !m.0.options.casts_shadows || m.0.options.blend != material::BlendMode::Opaque
            })
            || !s
                .retained
                .environment
                .frustum()
                .is_some_and(|f| f.intersects_bounds(bounds, model))
        {
            return false;
        }
        s.retained.record_style(
            mesh,
            model,
            s.matrix,
            s.depth,
            retained::DrawStyle {
                light: s.lighting,
                material: s.shader.as_ref(),
                params: s.shader_params,
                shadow_only: true,
                skip_shadow: false,
                skin,
            },
        )
    })
}
pub fn set_quality(quality: lighting::Quality) {
    edit(|s| {
        s.quality = s.quality_override.unwrap_or(quality);
        s.refresh_environment();
    });
}
pub fn quality() -> lighting::Quality {
    read(|s| s.quality)
}
pub fn set_point_lights(lights: &[lighting::PointLight]) -> Result<(), &'static str> {
    if lights.len() > lighting::MAX_POINT_LIGHTS || lights.iter().any(|l| !l.valid()) {
        return Err("invalid point lights or capacity exceeded");
    }
    edit(|s| {
        s.points.clear();
        s.points.extend_from_slice(lights);
        s.refresh_environment();
    });
    Ok(())
}
pub fn set_sun_shadow(sun: Option<lighting::SunShadow>) -> Result<(), &'static str> {
    if sun.is_some_and(|s| !s.valid()) {
        return Err("invalid sun shadow");
    }
    edit(|s| {
        s.sun = sun;
        s.refresh_environment();
    });
    Ok(())
}
fn quality_override(quality: lighting::Quality) {
    edit(|s| {
        s.quality_override = Some(quality);
        s.quality = quality;
        s.refresh_environment();
    });
}
pub fn set_visibility(settings: visibility::VisibilitySettings) -> bool {
    if !settings.valid() {
        return false;
    }
    edit(|s| s.visibility = settings);
    true
}
pub fn visibility_stats() -> visibility::VisibilityStats {
    read(|s| s.visibility_stats)
}
pub fn draw_lod(
    chain: &visibility::LodChain,
    model: Mat4,
    state: &mut visibility::LodState,
) -> bool {
    edit(|s| {
        let distance = s.eye.map_or(0., |eye| {
            visibility::distance_to_bounds(chain.bounds(), model, eye)
        });
        let mesh = chain.select(distance, state);
        let bounds = s
            .shader
            .as_ref()
            .map_or(chain.bounds(), |m| m.padded(chain.bounds()));
        s.submit(mesh, model, bounds, None)
    })
}
pub fn draw_mesh_transformed(m: &Mesh, model: Mat4) {
    edit(|s| {
        s.push(
            m.indices
                .as_chunks::<3>()
                .0
                .iter()
                .filter_map(|[a, b, c]| {
                    let v = [
                        m.vertices.get(*a as usize)?,
                        m.vertices.get(*b as usize)?,
                        m.vertices.get(*c as usize)?,
                    ];
                    v.iter()
                        .all(|v| v.position.is_finite() && v.normal.is_finite())
                        .then_some(v)
                })
                .flatten()
                .map(|v| {
                    (
                        v.position,
                        v.normal.truncate(),
                        Color::from_rgba(v.color[0], v.color[1], v.color[2], v.color[3]),
                    )
                }),
            model,
        )
    })
}
pub fn draw_triangle(a: Vec2, b: Vec2, c: Vec2, color: Color) {
    edit(|s| {
        s.push(
            [a, b, c].map(|p| (p.extend(0.), Vec3::Z, color)),
            Mat4::IDENTITY,
        )
    })
}
pub fn draw_rectangle(x: f32, y: f32, w: f32, h: f32, c: Color) {
    let a = vec2(x, y);
    let b = vec2(x + w, y);
    let d = vec2(x, y + h);
    let e = vec2(x + w, y + h);
    draw_triangle(a, b, d, c);
    draw_triangle(d, b, e, c);
}
pub fn draw_line(x1: f32, y1: f32, x2: f32, y2: f32, width: f32, c: Color) {
    let a = vec2(x1, y1);
    let b = vec2(x2, y2);
    let d = (b - a).normalize_or_zero();
    let n = vec2(-d.y, d.x) * width * 0.5;
    draw_triangle(a + n, a - n, b + n, c);
    draw_triangle(b + n, a - n, b - n, c);
}
pub fn draw_rectangle_lines(x: f32, y: f32, w: f32, h: f32, t: f32, c: Color) {
    draw_line(x, y, x + w, y, t, c);
    draw_line(x + w, y, x + w, y + h, t, c);
    draw_line(x + w, y + h, x, y + h, t, c);
    draw_line(x, y + h, x, y, t, c);
}
pub fn draw_poly(x: f32, y: f32, sides: u8, r: f32, rotation: f32, c: Color) {
    let sides = sides.max(3);
    for i in 0..sides {
        let a = rotation.to_radians() + i as f32 * std::f32::consts::TAU / sides as f32;
        let b = a + std::f32::consts::TAU / sides as f32;
        draw_triangle(
            vec2(x, y),
            vec2(x + r * a.cos(), y + r * a.sin()),
            vec2(x + r * b.cos(), y + r * b.sin()),
            c,
        );
    }
}
pub fn draw_circle(x: f32, y: f32, r: f32, c: Color) {
    draw_poly(x, y, 32, r, 0., c)
}
pub fn draw_circle_lines(x: f32, y: f32, r: f32, t: f32, c: Color) {
    for i in 0..40 {
        let a = i as f32 * std::f32::consts::TAU / 40.;
        let b = (i + 1) as f32 * std::f32::consts::TAU / 40.;
        draw_line(
            x + r * a.cos(),
            y + r * a.sin(),
            x + r * b.cos(),
            y + r * b.sin(),
            t,
            c,
        );
    }
}
pub fn draw_ellipse(x: f32, y: f32, rx: f32, ry: f32, rotation: f32, c: Color) {
    let angle = rotation.to_radians();
    let point = |a: f32| {
        let p = vec2(rx * a.cos(), ry * a.sin());
        vec2(
            x + p.x * angle.cos() - p.y * angle.sin(),
            y + p.x * angle.sin() + p.y * angle.cos(),
        )
    };
    for i in 0..32 {
        draw_triangle(
            vec2(x, y),
            point(i as f32 * std::f32::consts::TAU / 32.),
            point((i + 1) as f32 * std::f32::consts::TAU / 32.),
            c,
        );
    }
}
pub fn draw_line_3d(a: Vec3, b: Vec3, c: Color) {
    let d = (b - a).normalize_or_zero();
    let n = d
        .cross(if d.y.abs() < 0.9 { Vec3::Y } else { Vec3::X })
        .normalize_or_zero()
        * 0.015;
    edit(|s| {
        s.push(
            [a + n, a - n, b + n, b + n, a - n, b - n].map(|p| (p, Vec3::Y, c)),
            Mat4::IDENTITY,
        )
    });
}
pub fn draw_cube(position: Vec3, size: Vec3, _texture: Option<()>, color: Color) {
    mesh::Mesh3d::cuboid(size, color).draw(position, Quat::IDENTITY);
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Font;
impl Font {
    pub fn populate_font_cache(&self, _chars: &[char], _size: u16) { /* Static glyphs need no atlas allocation. */
    }
}
pub fn get_default_font() -> Font {
    Font
}
#[derive(Clone, Copy, Debug)]
pub struct TextParams {
    pub font_size: u16,
    pub font_scale: f32,
    pub color: Color,
}
impl Default for TextParams {
    fn default() -> Self {
        Self {
            font_size: 20,
            font_scale: 1.,
            color: WHITE,
        }
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub struct TextDimensions {
    pub width: f32,
    pub height: f32,
    pub offset_y: f32,
}
pub fn measure_text(t: &str, _font: Option<&Font>, size: u16, scale: f32) -> TextDimensions {
    let px = size as f32 * scale / 8.;
    TextDimensions {
        width: t.chars().count() as f32 * 6. * px,
        height: 7. * px,
        offset_y: 7. * px,
    }
}
pub fn draw_text(t: &str, x: f32, y: f32, size: f32, c: Color) -> TextDimensions {
    draw_text_ex(
        t,
        x,
        y,
        TextParams {
            font_size: 8,
            font_scale: size / 8.,
            color: c,
        },
    )
}
pub fn draw_text_ex(t: &str, mut x: f32, y: f32, p: TextParams) -> TextDimensions {
    let px = p.font_size as f32 * p.font_scale / 8.;
    for c in t.chars() {
        for (row, bits) in font::glyph(c.to_ascii_uppercase()).into_iter().enumerate() {
            for col in 0..5 {
                if bits & (1 << (4 - col)) != 0 {
                    draw_rectangle(
                        x + col as f32 * px,
                        y - 7. * px + row as f32 * px,
                        px,
                        px,
                        p.color,
                    );
                }
            }
        }
        x += 6. * px;
    }
    measure_text(t, None, p.font_size, p.font_scale)
}
pub fn screen_width() -> f32 {
    read(|s| s.w as f32)
}
pub fn screen_height() -> f32 {
    read(|s| s.h as f32)
}
pub fn screen_dpi_scale() -> f32 {
    1.
}
pub fn get_frame_time() -> f32 {
    read(|s| s.dt)
}
pub fn get_time() -> f64 {
    read(|s| s.time)
}
pub fn is_key_down(k: KeyCode) -> bool {
    read(|s| !s.controls_open && s.keys.contains(&(k as u32)))
}
pub fn is_key_pressed(k: KeyCode) -> bool {
    read(|s| !s.controls_open && s.pressed.contains(&(k as u32)))
}
pub fn is_mouse_button_down(k: MouseButton) -> bool {
    read(|s| {
        !s.controls_open
            && (s.buttons.contains(&(k as u32)) || s.touch_buttons.contains(&(k as u32)))
    })
}
pub fn is_mouse_button_pressed(k: MouseButton) -> bool {
    read(|s| !s.controls_open && s.clicks.contains(&(k as u32)))
}
pub fn mouse_position() -> (f32, f32) {
    read(|s| s.mouse.into())
}
pub fn mouse_delta_position() -> Vec2 {
    read(|s| -s.delta / vec2(s.w as f32, s.h as f32))
}
pub fn mouse_wheel() -> (f32, f32) {
    read(|s| (s.wheel_x, s.wheel))
}
pub fn touches() -> Vec<Touch> {
    read(|s| {
        if s.controls_open {
            Vec::new()
        } else {
            s.touches.clone()
        }
    })
}
pub fn set_cursor_grab(grab: bool) {
    edit(|s| s.grab = grab)
}
pub fn simulate_mouse_with_touch(enabled: bool) {
    edit(|s| s.touch_mouse = enabled)
}
pub fn show_mouse(visible: bool) {
    edit(|s| s.visible = visible)
}
pub struct ScreenImage;
pub fn get_screen_data() -> ScreenImage {
    ScreenImage
}
impl ScreenImage {
    /// Captures this frame after GPU submission, including draws queued before this call.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn export_png(&self, path: &str) {
        edit(|s| {
            if s.screenshots.len() < 8 {
                s.screenshots.push(path.into());
            }
        });
    }
}
pub async fn next_frame() {
    struct Yield(bool);
    impl Future for Yield {
        type Output = ();
        fn poll(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<()> {
            if self.0 {
                Poll::Ready(())
            } else {
                self.0 = true;
                Poll::Pending
            }
        }
    }
    Yield(false).await
}
pub fn start(conf: Conf, future: impl Future<Output = ()> + 'static) {
    audio::reset();
    edit(|s| {
        *s = State::default();
        s.w = conf.window_width.max(1) as u32;
        s.h = conf.window_height.max(1) as u32;
        s.default_camera();
        s.future = Some(Box::pin(future));
    });
}
fn begin_host_frame(dt: f32, w: u32, h: u32) {
    edit(|s| {
        assert!(!s.in_frame, "one input/render frame at a time");
        s.in_frame = true;
        s.dt = if dt.is_finite() {
            dt.clamp(0., 0.1)
        } else {
            0.
        };
        s.time += s.dt as f64;
        s.w = w.max(1);
        s.h = h.max(1);
        s.vertices.clear();
        s.batches.clear();
        s.retained.clear();
        s.effect_stats = effects::EffectStats::default();
        s.points.clear();
        s.sun = None;
        s.refresh_environment();
        s.visibility_stats = Default::default();
        s.default_camera();
    });
}
fn frame(dt: f32, w: u32, h: u32) -> bool {
    begin_host_frame(dt, w, h);
    let mut future = edit(|s| s.future.take());
    let finished = future.as_mut().is_none_or(|f| {
        f.as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_ready()
    });
    edit(|s| {
        s.future = future;
    });
    end_host_frame();
    !finished
}
fn end_host_frame() {
    let dt = read(|s| s.dt);
    audio::end_frame(dt);
    edit(|s| {
        s.retained.finalize();
        s.frame += 1;
        s.in_frame = false;
        s.pressed.clear();
        s.clicks.clear();
        s.input.finish_frame();
        s.delta = Vec2::ZERO;
        s.wheel = 0.;
        s.wheel_x = 0.;
        if !s.touches.is_empty() {
            let mut ended = [0_u64; 64];
            let mut len = 0;
            for touch in &s.touches {
                if matches!(touch.phase, TouchPhase::Ended | TouchPhase::Cancelled) {
                    ended[len] = touch.id;
                    len += 1;
                }
            }
            s.touches.retain(|touch| !ended[..len].contains(&touch.id));
        }
        for t in &mut s.touches {
            t.phase = TouchPhase::Stationary;
        }
    });
}
fn clear_held(s: &mut State) {
    s.keys.clear();
    s.pressed.clear();
    s.buttons.clear();
    s.touch_buttons.clear();
    s.clicks.clear();
    s.touches.clear();
    s.delta = Vec2::ZERO;
    s.wheel = 0.;
    s.wheel_x = 0.;
}
fn focus(focused: bool) {
    edit(|s| {
        s.input.focus(focused);
        if !focused {
            clear_held(s);
        }
    });
    if !focused {
        audio::focus_lost();
    }
}
fn key(code: u32, down: bool) {
    if code == 0 || code > u16::MAX as u32 {
        return;
    }
    edit(|s| {
        if s.keys.contains(&code) == down {
            return;
        }
        if !s.input.push(input::PlatformEvent::Value(
            input::InputCode::Key(code as u16),
            f32::from(down),
        )) {
            clear_held(s);
            return;
        }
        if down {
            if s.keys.len() < input::MAX_ACTIVE_CODES {
                s.keys.insert(code);
                s.pressed.insert(code);
            }
        } else {
            s.keys.remove(&code);
        }
    });
}
fn button(code: u32, down: bool) {
    button_source(code, down, false)
}
fn button_source(code: u32, down: bool, touch: bool) {
    if !(1..=5).contains(&code) {
        return;
    }
    edit(|s| {
        if !s.input.focused || s.input.overflow {
            clear_held(s);
            return;
        }
        let held = if touch { &s.touch_buttons } else { &s.buttons };
        if held.contains(&code) == down {
            return;
        }
        if !touch
            && !s.input.push(input::PlatformEvent::Value(
                input::InputCode::Mouse(code as u8),
                f32::from(down),
            ))
        {
            clear_held(s);
            return;
        }
        let held = if touch {
            &mut s.touch_buttons
        } else {
            &mut s.buttons
        };
        if down {
            held.insert(code);
            s.clicks.insert(code);
        } else {
            held.remove(&code);
        }
    });
}
fn pointer(x: f32, y: f32, dx: f32, dy: f32) {
    if ![x, y, dx, dy]
        .iter()
        .all(|v| v.is_finite() && v.abs() <= 1e6)
    {
        return;
    }
    edit(|s| {
        if (dx != 0. || dy != 0.)
            && !s
                .input
                .push(input::PlatformEvent::MouseMotion(vec2(dx, dy)))
        {
            clear_held(s);
            return;
        }
        s.mouse = vec2(x, y);
        s.delta += vec2(dx, dy);
    });
}
fn wheel(delta: f32) {
    wheel_axis(1, delta)
}
fn wheel_axis(axis: u8, delta: f32) {
    if !delta.is_finite() || axis > 1 {
        return;
    }
    edit(|s| {
        let delta = delta.clamp(-1000., 1000.);
        if !s.input.push(input::PlatformEvent::Value(
            input::InputCode::Wheel(axis),
            delta,
        )) {
            clear_held(s);
            return;
        }
        if axis == 0 {
            s.wheel_x += delta;
        } else {
            s.wheel += delta;
        }
    });
}
fn touch(id: u64, phase: u32, x: f32, y: f32) {
    if !x.is_finite() || !y.is_finite() {
        return;
    }
    edit(|s| {
        let position = vec2(x, y);
        if position.abs().max_element() > 1e6 {
            return;
        }
        let event = ferravex_play::input::touch::Event {
            id,
            position,
            phase: match phase {
                0 => ferravex_play::input::touch::Phase::Start,
                1 => ferravex_play::input::touch::Phase::Move,
                2 => ferravex_play::input::touch::Phase::End,
                _ => ferravex_play::input::touch::Phase::Cancel,
            },
        };
        if !s.input.push(input::PlatformEvent::Touch(event)) {
            clear_held(s);
            return;
        }
        if phase == 1
            && let Some(t) = s
                .touches
                .iter_mut()
                .find(|t| t.id == id && t.phase == TouchPhase::Started)
        {
            t.position = position;
            return;
        }
        let preserve_start = phase == 2
            && s.touches
                .iter()
                .any(|t| t.id == id && t.phase == TouchPhase::Started);
        if !preserve_start {
            s.touches.retain(|t| t.id != id);
        }
        if s.touches.len() < 64 {
            s.touches.push(Touch {
                id,
                position,
                phase: match phase {
                    0 => TouchPhase::Started,
                    1 => TouchPhase::Moved,
                    2 => TouchPhase::Ended,
                    _ => TouchPhase::Cancelled,
                },
            });
        }
    });
}
#[cfg(not(target_arch = "wasm32"))]
pub fn run(conf: Conf, future: impl Future<Output = ()> + 'static) {
    let mut window = native::NativeWindow::new(&conf)
        .unwrap_or_else(|e| panic!("Ferravex native platform: {e}"));
    start(conf, future);
    if let Ok(value) = std::env::var("FERRAVEX_QUALITY") {
        quality_override(
            lighting::Quality::from_name(&value)
                .expect("FERRAVEX_QUALITY must be low, medium or high"),
        );
    }
    let fixed = std::env::args().any(|s| s == "--smoke-frames");
    audio::boot_native(fixed);
    let mut performance = telemetry::Session::from_env();
    if let Some(session) = &mut performance {
        window.set_swap_interval(0);
        if window.enable_gpu_timing() {
            session.gpu_source("OpenGL TIME_ELAPSED");
        }
    }
    let mut last = std::time::Instant::now();
    loop {
        let now = std::time::Instant::now();
        let dt = if fixed {
            1. / 60.
        } else {
            (now - last).as_secs_f32()
        };
        last = now;
        for e in window.poll() {
            use native::Event;
            match e {
                Event::Key(c, d) => key(c, d),
                Event::MouseButton(c, d) => button(c, d),
                Event::MouseMove(x, y) => {
                    let old = read(|s| s.mouse);
                    pointer(x, y, x - old.x, y - old.y);
                }
                Event::MouseDelta(dx, dy) => {
                    let old = read(|s| s.mouse);
                    pointer(old.x, old.y, dx, dy);
                }
                Event::Wheel(v) => wheel(v),
                Event::Focus(value) => focus(value),
                Event::Resize(_, _) => {}
                Event::Quit => {
                    audio::shutdown_native();
                    return;
                }
            }
        }
        input::poll_gamepads();
        let (w, h) = window.size();
        let token = performance.as_ref().map(|_| telemetry::FrameToken::begin());
        let active = frame(dt, w, h);
        read(|s| window.set_cursor(s.grab, s.visible));
        let cpu_us = token.as_ref().map_or(0., |t| t.elapsed_us());
        let submit_start = std::time::Instant::now();
        read(|s| window.present_scene(&s.vertices, &s.batches, s.clear, Some(&s.retained)));
        if let (Some(session), Some(token)) = (&mut performance, token) {
            session.record(token.finish(read(|s| telemetry::FrameSample {
                cpu_us,
                submit_us: Some(submit_start.elapsed().as_secs_f64() * 1e6),
                draw_calls: Some(window.render_counts().0),
                vertices: Some(window.render_counts().1),
                upload_bytes: Some(
                    (s.vertices.len() * 32 + s.retained.instances.len() * 112) as u64
                        + window.last_mesh_upload_bytes()
                        + window.skin_upload_bytes()
                        + window.sprite_upload_bytes(),
                ),
                ..Default::default()
            })));
        }
        if let Some(session) = &mut performance {
            window.drain_gpu_results(|frame, time| session.gpu_result(frame, time));
        }
        let paths = edit(|s| std::mem::take(&mut s.screenshots));
        if !paths.is_empty() {
            let pixels = window.pixels();
            match ferravex_graphics::Framebuffer::new(w, h) {
                Ok(mut fb) if pixels.len() == w as usize * h as usize * 4 => {
                    for (dst, src) in fb.pixels_mut().iter_mut().zip(pixels.as_chunks::<4>().0) {
                        *dst = *src;
                    }
                    for path in paths {
                        if let Err(error) = fb.save_png(&path) {
                            eprintln!("Screenshot {path}: {error}");
                        }
                    }
                }
                _ => eprintln!("Screenshot unavailable: invalid dimensions or GPU readback"),
            }
        }
        if !active {
            break;
        }
        if !fixed {
            let elapsed = now.elapsed();
            if elapsed < std::time::Duration::from_millis(16) {
                std::thread::sleep(std::time::Duration::from_millis(16) - elapsed);
            }
        }
    }
    audio::shutdown_native();
    if let Some(mut session) = performance {
        session.render_quality(quality() as u32);
        window.finish_gpu_timing();
        window.drain_gpu_results(|frame, time| session.gpu_result(frame, time));
        let (w, h) = window.size();
        session
            .write(&window.driver_info(), w, h)
            .expect("write performance report");
    }
}
#[macro_export]
macro_rules! game {
    ($conf:path,$game:path) => {
        #[cfg(not(target_arch = "wasm32"))]
        #[global_allocator]
        static FERRAVEX_ALLOCATOR: $crate::telemetry::TrackingAllocator =
            $crate::telemetry::TrackingAllocator;
        #[cfg(not(target_arch = "wasm32"))]
        pub fn run() {
            $crate::run($conf(), $game());
        }
        #[cfg(target_arch = "wasm32")]
        #[unsafe(no_mangle)]
        pub extern "C" fn ferravex_init() {
            $crate::start($conf(), $game());
        }
    };
}
#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::*;
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_light_environment() -> *const lighting::Environment {
        read(|s| &s.retained.environment as *const lighting::Environment)
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_quality_override(value: u32) -> u32 {
        let q = match value {
            0 => lighting::Quality::Low,
            1 => lighting::Quality::Medium,
            2 => lighting::Quality::High,
            _ => return 0,
        };
        quality_override(q);
        1
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_retained_batch_count() -> u32 {
        read(|s| s.retained.batches.len() as u32)
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_culled_count() -> u32 {
        let s = visibility_stats();
        s.frustum_culled + s.distance_culled
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_visible_count() -> u32 {
        visibility_stats().visible
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_commands() -> *const retained::Command {
        read(|s| s.retained.commands.as_ptr())
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_command_count() -> u32 {
        read(|s| s.retained.commands.len() as u32)
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_sprite_batches() -> *const effects::SpriteBatch {
        read(|s| s.retained.sprite_batches.as_ptr())
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_sprite_batch_count() -> u32 {
        read(|s| s.retained.sprite_batches.len() as u32)
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_sprite_instances() -> *const [f32; 16] {
        read(|s| s.retained.sprites.as_ptr())
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_sprite_instance_count() -> u32 {
        read(|s| s.retained.sprites.len() as u32)
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_effect_batches() -> *const effects::EffectBatch {
        read(|s| s.retained.effects.as_ptr())
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_effect_batch_count() -> u32 {
        read(|s| s.retained.effects.len() as u32)
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_effect_vertex_count() -> u32 {
        effects::stats().vertices
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_retained_batches() -> *const retained::RetainedBatch {
        read(|s| s.retained.batches.as_ptr())
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_retained_instances() -> *const [f32; 28] {
        read(|s| s.retained.instances.as_ptr())
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_retained_instance_count() -> u32 {
        read(|s| s.retained.instances.len() as u32)
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_mesh_vertices(id: u32) -> *const [f32; 10] {
        retained::geometry(id).map_or(std::ptr::null(), |m| m.vertices.as_ptr())
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_mesh_skin(id: u32) -> *const [f32; 8] {
        retained::geometry(id)
            .and_then(|m| m.skin.as_ref().map(|v| v.as_ptr()))
            .unwrap_or(std::ptr::null())
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_skin_matrices() -> *const [f32; 16] {
        read(|s| s.retained.skin_matrices.as_ptr())
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_skin_normals() -> *const [f32; 9] {
        read(|s| s.retained.skin_normals.as_ptr())
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_skin_joint_count() -> u32 {
        read(|s| s.retained.skin_matrices.len() as u32)
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_mesh_vertex_count(id: u32) -> u32 {
        retained::geometry(id).map_or(0, |m| m.vertices.len() as u32)
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_frame(dt: f32, w: u32, h: u32) -> u32 {
        frame(dt, w, h) as u32
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_vertices() -> *const [f32; 8] {
        read(|s| s.vertices.as_ptr())
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_vertex_count() -> u32 {
        read(|s| s.vertices.len() as u32)
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_batches() -> *const Batch {
        read(|s| s.batches.as_ptr())
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_batch_count() -> u32 {
        read(|s| s.batches.len() as u32)
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_clear(channel: u32) -> f32 {
        read(|s| [s.clear.r, s.clear.g, s.clear.b, s.clear.a][channel.min(3) as usize])
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_key(code: u32, down: u32) {
        key(code, down != 0)
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_button(code: u32, down: u32) {
        button(code, down != 0)
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_pointer(x: f32, y: f32, dx: f32, dy: f32) {
        pointer(x, y, dx, dy)
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_wheel(delta: f32) {
        wheel(delta);
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_wheel_axis(axis: u32, delta: f32) {
        if axis < 2 {
            wheel_axis(axis as u8, delta);
        }
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_blur() {
        focus(false)
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_focus(value: u32) {
        focus(value != 0)
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_touch_button(code: u32, down: u32) {
        button_source(code, down != 0, true)
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_touch_pointer(x: f32, y: f32) {
        if x.is_finite() && y.is_finite() {
            edit(|s| s.mouse = vec2(x, y));
        }
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_pad_connected(index: u32, connected: u32) {
        edit(|s| s.input.pad_connected(index as usize, connected != 0));
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_pad_axis(index: u32, axis: u32, value: f32) {
        edit(|s| s.input.pad_axis(index as usize, axis as usize, value));
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_pad_button(index: u32, button: u32, value: f32) {
        edit(|s| s.input.pad_button(index as usize, button as usize, value));
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_controls_open() -> u32 {
        read(|s| s.controls_open as u32)
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_input_settings() {
        edit(|s| s.input.settings_requested = true);
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_touch_mouse() -> u32 {
        read(|s| s.touch_mouse as u32)
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_grab() -> u32 {
        read(|s| s.grab as u32)
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_touch(id: u32, phase: u32, x: f32, y: f32) {
        touch(id as u64, phase, x, y);
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_autoplay(enabled: u32) {
        edit(|s| s.autoplay = enabled != 0)
    }
}
pub fn autoplay_enabled() -> bool {
    read(|s| s.autoplay)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn reset() {
        edit(|s| *s = State::default());
    }
    #[test]
    fn camera_culled_meshes_remain_available_to_the_shadow_pass() {
        reset();
        set_quality(lighting::Quality::High);
        set_sun_shadow(Some(lighting::SunShadow {
            extent: 30.,
            ..Default::default()
        }))
        .unwrap();
        set_camera(&Camera3D {
            position: vec3(0., 0., 5.),
            target: Vec3::ZERO,
            z_far: 20.,
            ..Default::default()
        });
        let mesh = mesh::Mesh3d::cuboid(Vec3::ONE, WHITE);
        assert!(!draw_resident(
            mesh.resident(),
            Mat4::from_translation(Vec3::X * 10.)
        ));
        read(|s| {
            assert_eq!(s.retained.batches.len(), 1);
            assert_ne!(s.retained.batches[0].depth & 32, 0);
            assert_eq!(s.retained.batches[0].depth & 8, 0);
        });
    }
    #[test]
    fn shared_camera_culls_residents_and_default_ui_camera_does_not() {
        reset();
        let mesh = mesh::Mesh3d::cuboid(Vec3::ONE, WHITE);
        set_camera(&Camera3D {
            position: vec3(0., 0., 5.),
            target: Vec3::ZERO,
            z_far: 20.,
            ..Default::default()
        });
        assert!(draw_resident(mesh.resident(), Mat4::IDENTITY));
        assert!(!draw_resident(
            mesh.resident(),
            Mat4::from_translation(Vec3::X * 100.)
        ));
        let stats = visibility_stats();
        assert_eq!(
            (stats.tested, stats.visible, stats.frustum_culled),
            (2, 1, 1)
        );
        assert!(!set_visibility(visibility::VisibilitySettings {
            max_distance: Some(-1.),
            ..Default::default()
        }));
        set_default_camera();
        assert!(draw_resident(
            mesh.resident(),
            Mat4::from_translation(Vec3::X * 100.)
        ));
    }
    #[test]
    fn frame_yields_once_and_keeps_game_local_state() {
        reset();
        let ticks = std::rc::Rc::new(std::cell::Cell::new(0));
        let count = ticks.clone();
        start(Conf::default(), async move {
            for _ in 0..3 {
                count.set(count.get() + 1);
                next_frame().await;
            }
        });
        for expected in 1..=3 {
            assert!(frame(1. / 60., 640, 480));
            assert_eq!(ticks.get(), expected);
        }
        assert!(!frame(1. / 60., 640, 480));
        assert_eq!(ticks.get(), 3);
    }
    #[test]
    fn key_edges_survive_until_poll_and_release_on_blur() {
        reset();
        key(97, true);
        key(97, true);
        assert!(is_key_down(KeyCode::A));
        assert!(is_key_pressed(KeyCode::A));
        start(Conf::default(), async {
            assert!(is_key_pressed(KeyCode::D));
            next_frame().await;
            assert!(!is_key_pressed(KeyCode::D));
            assert!(is_key_down(KeyCode::D));
        });
        key(100, true);
        assert!(frame(0.01, 1280, 800));
        assert!(!frame(0.01, 1280, 800));
        key(100, false);
        assert!(!is_key_down(KeyCode::D));
    }
    #[test]
    fn screen_camera_projects_top_left_and_bottom_right() {
        reset();
        edit(|s| {
            s.w = 640;
            s.h = 480;
            s.default_camera()
        });
        read(|s| {
            assert!(
                s.matrix
                    .project_point3(Vec3::ZERO)
                    .abs_diff_eq(vec3(-1., 1., 0.), 1e-6)
            );
            assert!(
                s.matrix
                    .project_point3(vec3(640., 480., 0.))
                    .abs_diff_eq(vec3(1., -1., 0.), 1e-6)
            );
        });
        let c = Camera2D::from_display_rect(Rect::new(10., 20., 100., 50.));
        let m = c.matrix(640., 480.);
        assert!(
            m.project_point3(vec3(10., 20., 0.))
                .abs_diff_eq(vec3(-1., 1., 0.), 1e-6)
        );
    }
    #[test]
    fn draws_batch_only_adjacent_depth_modes_and_retain_their_camera() {
        reset();
        draw_rectangle(0., 0., 20., 20., WHITE);
        set_camera(&Camera3D::default());
        draw_triangle(Vec2::ZERO, Vec2::X, Vec2::Y, RED);
        set_default_camera();
        draw_triangle(Vec2::ZERO, Vec2::X, Vec2::Y, BLUE);
        read(|s| {
            assert_eq!(s.vertices.len(), 12);
            assert_eq!(
                s.batches
                    .iter()
                    .map(|b| (b.start, b.count, b.depth))
                    .collect::<Vec<_>>(),
                [(0, 6, false), (6, 3, true), (9, 3, false)]
            );
            assert_eq!(std::mem::size_of::<Batch>(), 12);
            assert_eq!(std::mem::offset_of!(Batch, depth), 8);
        });
    }
    #[test]
    fn fast_touch_taps_preserve_start_and_end_without_sticking() {
        reset();
        start(Conf::default(), async {
            next_frame().await;
        });
        touch(7, 0, 10., 20.);
        touch(7, 1, 12., 22.);
        assert_eq!(touches()[0].phase, TouchPhase::Started);
        touch(7, 2, 12., 22.);
        assert_eq!(touches().len(), 2);
        frame(f32::NAN, 640, 480);
        assert!(touches().is_empty());
        assert_eq!(get_frame_time(), 0.);
        assert!(get_time().is_finite());
        touch(8, 0, 10., 20.);
        touch(8, 3, 10., 20.);
        assert_eq!(touches().len(), 1);
        assert_eq!(touches()[0].phase, TouchPhase::Cancelled);
        pointer(f32::NAN, 0., 0., 0.);
        assert!(read(|s| s.mouse.is_finite()));
    }
    #[test]
    fn invalid_mesh_triangles_cannot_corrupt_adjacent_batches() {
        reset();
        let v = Vertex::new2(Vec3::ZERO, Vec2::ZERO, WHITE);
        draw_mesh(&Mesh {
            vertices: vec![v; 3],
            indices: vec![0, 1, 9, 0, 1, 2, 0],
            texture: None,
        });
        read(|s| {
            assert_eq!(s.vertices.len(), 3);
            assert_eq!(s.batches[0].count, 3);
        });
    }
    #[test]
    fn nonuniform_scale_uses_inverse_transpose_for_normals() {
        reset();
        set_lighting(Some(shaders::LitSettings {
            ambient: 0.,
            light_direction: Vec3::Y,
            fog_start: 1e5,
            fog_end: 2e5,
            ..Default::default()
        }));
        let n = vec3(1., 1., 0.).normalize();
        let model = Mat4::from_scale(vec3(4., 0.5, 1.));
        edit(|s| s.push([(Vec3::ZERO, n, WHITE); 3], model));
        read(|s| {
            assert!(s.vertices[0][4] > 0.99);
        });
    }
}
