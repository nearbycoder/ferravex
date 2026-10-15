//! Batched low-poly particle billboards and ribbons over the shared color stream.
//! No per-particle mesh, material, texture, GPU buffer or shader allocation.
use crate::{Camera3D, Vec3, retained::Command};
pub use ferravex_play::particles::{
    BeamPool, ParticlePool, ParticleView, RibbonFacing, RibbonPoint, RibbonSource, RibbonTriangle,
    RibbonTriangles, RibbonVertex, Trail,
};
pub const MAX_EFFECT_VERTICES: usize = 196608;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EffectBlend {
    #[default]
    Alpha,
    Additive,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EffectStyle {
    pub blend: EffectBlend,
    /// None follows the selected camera; false provides an overlay effect.
    pub depth_test: Option<bool>,
    /// Transparent effects normally test against opaque surfaces without writing.
    pub depth_write: bool,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EffectBatch {
    pub start: u32,
    pub count: u32,
    pub flags: u32,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DrawReport {
    pub triangles: u32,
    pub invalid: u32,
    pub dropped_samples: u32,
    pub culled_samples: u32,
    pub truncated: bool,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EffectStats {
    pub vertices: u32,
    pub processed_triangles: u32,
    pub invalid_triangles: u32,
    pub truncated_calls: u32,
    pub instanced_vertices: u32,
    pub culled_sprites: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectError {
    InvalidView,
    InvalidStyle,
    InvalidParticle,
    InvalidCapacity,
    Ribbon(ferravex_play::particles::RibbonError),
}
impl From<ferravex_play::particles::RibbonError> for EffectError {
    fn from(error: ferravex_play::particles::RibbonError) -> Self {
        Self::Ribbon(error)
    }
}
pub fn stats() -> EffectStats {
    crate::read(|s| s.effect_stats)
}
/// Record unlit, normalized RGBA triangles with the current camera. This color
/// stream is shared with ordinary geometry; flags preserve blend/depth/HUD order.
/// At most 65,536 triangles are examined per frame, including malformed input.
pub fn draw_triangles(
    triangles: impl IntoIterator<Item = RibbonTriangle>,
    style: EffectStyle,
) -> DrawReport {
    crate::edit(|s| {
        let start = s.vertices.len() as u32;
        let mut input = triangles.into_iter();
        let mut report = DrawReport::default();
        let budget =
            (MAX_EFFECT_VERTICES / 3).saturating_sub(s.effect_stats.processed_triangles as usize);
        for _ in 0..budget {
            let Some(triangle) = input.next() else {
                break;
            };
            s.effect_stats.processed_triangles += 1;
            let mut projected = [[0.; 8]; 3];
            let mut valid = true;
            for (out, v) in projected.iter_mut().zip(triangle.vertices) {
                if !v.position.into_iter().chain(v.color).all(f32::is_finite) {
                    valid = false;
                    break;
                }
                let p = s.matrix * Vec3::from_array(v.position).extend(1.);
                if !p.is_finite() {
                    valid = false;
                    break;
                }
                *out = [
                    p.x,
                    p.y,
                    p.z,
                    p.w,
                    v.color[0].clamp(0., 1.),
                    v.color[1].clamp(0., 1.),
                    v.color[2].clamp(0., 1.),
                    v.color[3].clamp(0., 1.),
                ];
            }
            if valid {
                s.vertices.extend_from_slice(&projected);
                report.triangles += 1;
            } else {
                report.invalid += 1;
            }
        }
        // One bounded lookahead detects truncation; it does not walk discarded work.
        if s.effect_stats.processed_triangles as usize == MAX_EFFECT_VERTICES / 3
            && input.next().is_some()
        {
            report.truncated = true;
            s.effect_stats.truncated_calls = s.effect_stats.truncated_calls.saturating_add(1);
        }
        s.effect_stats.invalid_triangles = s
            .effect_stats
            .invalid_triangles
            .saturating_add(report.invalid);
        let count = report.triangles * 3;
        s.effect_stats.vertices += count;
        if count > 0 {
            let depth = style.depth_test.unwrap_or(s.depth);
            let flags = depth as u32
                | if style.depth_write { 0 } else { 2 }
                | if style.blend == EffectBlend::Additive {
                    4
                } else {
                    0
                };
            if style.blend == EffectBlend::Alpha && (!depth || style.depth_write) {
                // 2D alpha effects have exactly the legacy stream state and can
                // share its existing draw rather than inserting a new barrier.
                s.record_immediate(start, count, depth);
            } else if let Some(last) = s.retained.effects.last_mut().filter(|b| {
                b.flags == flags && s.retained.commands.last().is_some_and(|c| c.kind == 2)
            }) {
                last.count += count;
            } else {
                s.retained.commands.push(Command {
                    kind: 2,
                    index: s.retained.effects.len() as u32,
                });
                s.retained.effects.push(EffectBatch {
                    start,
                    count,
                    flags,
                });
            }
        }
        report
    })
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BillboardView {
    pub eye: Vec3,
    pub right: Vec3,
    pub up: Vec3,
}
impl BillboardView {
    pub fn new(eye: Vec3, right: Vec3, up: Vec3) -> Result<Self, EffectError> {
        if !eye.is_finite() {
            return Err(EffectError::InvalidView);
        }
        let right = right.try_normalize().ok_or(EffectError::InvalidView)?;
        let up = (up - right * up.dot(right))
            .try_normalize()
            .ok_or(EffectError::InvalidView)?;
        Ok(Self { eye, right, up })
    }
    pub fn from_camera(camera: &Camera3D) -> Result<Self, EffectError> {
        let forward = (camera.target - camera.position)
            .try_normalize()
            .ok_or(EffectError::InvalidView)?;
        let right = forward.cross(camera.up);
        Self::new(camera.position, right, camera.up)
    }
    fn normalized(self) -> Result<Self, EffectError> {
        Self::new(self.eye, self.right, self.up)
    }
    fn depth(self, position: [f32; 3]) -> f32 {
        (Vec3::from_array(position) - self.eye).dot(-self.right.cross(self.up))
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ParticleShape {
    Square,
    #[default]
    Diamond,
    Triangle,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParticleStyle {
    pub draw: EffectStyle,
    pub shape: ParticleShape,
    /// Stretch along projected velocity; zero keeps authored rotation/size.
    pub stretch: f32,
    pub max_stretch: f32,
    pub sort_back_to_front: bool,
}
impl Default for ParticleStyle {
    fn default() -> Self {
        Self {
            draw: EffectStyle::default(),
            shape: ParticleShape::Diamond,
            stretch: 0.,
            max_stretch: 8.,
            sort_back_to_front: true,
        }
    }
}
impl ParticleStyle {
    fn validate(self) -> Result<Self, EffectError> {
        if !self.stretch.is_finite()
            || !(0.0..=100.).contains(&self.stretch)
            || !self.max_stretch.is_finite()
            || !(1.0..=32.).contains(&self.max_stretch)
        {
            Err(EffectError::InvalidStyle)
        } else {
            Ok(self)
        }
    }
}
#[derive(Clone, Copy)]
struct Entry {
    particle: ParticleView,
    depth: f32,
    order: u32,
}
/// Scratch is allocated once. Alpha 3D particles sort by camera depth with an
/// allocation-free unstable sort and explicit tie order. Oversized pools are
/// truncated to this renderer's capacity, independently of the geometry budget.
pub struct ParticleRenderer {
    scratch: Vec<Entry>,
    capacity: usize,
}
impl ParticleRenderer {
    pub fn new(capacity: usize) -> Result<Self, EffectError> {
        if capacity > ferravex_play::particles::MAX_PARTICLES {
            return Err(EffectError::InvalidCapacity);
        }
        Ok(Self {
            scratch: Vec::with_capacity(capacity),
            capacity,
        })
    }
    pub fn capacity(&self) -> usize {
        self.capacity
    }
    pub fn draw_2d(
        &mut self,
        pool: &ParticlePool,
        style: ParticleStyle,
    ) -> Result<DrawReport, EffectError> {
        let style = style.validate()?;
        self.prepare(pool, None, false);
        let view = BillboardView {
            eye: Vec3::Z,
            right: Vec3::X,
            up: Vec3::Y,
        };
        Ok(self.draw_prepared(pool, view, style))
    }
    pub fn draw_3d(
        &mut self,
        pool: &ParticlePool,
        view: BillboardView,
        style: ParticleStyle,
    ) -> Result<DrawReport, EffectError> {
        let style = style.validate()?;
        let view = view.normalized()?;
        self.prepare(
            pool,
            Some(view),
            style.sort_back_to_front && style.draw.blend == EffectBlend::Alpha,
        );
        Ok(self.draw_prepared(pool, view, style))
    }
    fn prepare(&mut self, pool: &ParticlePool, view: Option<BillboardView>, sort: bool) {
        self.scratch.clear();
        self.scratch.extend(
            pool.iter()
                .take(self.capacity)
                .enumerate()
                .map(|(i, particle)| Entry {
                    depth: view.map_or(0., |v| v.depth(particle.position)),
                    particle,
                    order: i as u32,
                }),
        );
        if sort {
            self.scratch.sort_unstable_by(|a, b| {
                b.depth
                    .total_cmp(&a.depth)
                    .then_with(|| a.order.cmp(&b.order))
            });
        }
    }
    fn draw_prepared(
        &self,
        pool: &ParticlePool,
        view: BillboardView,
        style: ParticleStyle,
    ) -> DrawReport {
        let mut report = crate::edit(|s| {
            let mut report = DrawReport::default();
            let triangles = if style.shape == ParticleShape::Triangle {
                1
            } else {
                2
            };
            let depth = style.draw.depth_test.unwrap_or(s.depth);
            let flags = depth as u32
                | if style.draw.depth_write { 0 } else { 2 }
                | if style.draw.blend == EffectBlend::Additive {
                    4
                } else {
                    0
                };
            for entry in &self.scratch {
                if s.effect_stats.processed_triangles as usize + triangles > MAX_EFFECT_VERTICES / 3
                {
                    report.truncated = true;
                    s.effect_stats.truncated_calls =
                        s.effect_stats.truncated_calls.saturating_add(1);
                    break;
                }
                s.effect_stats.processed_triangles += triangles as u32;
                let p = entry.particle;
                let (mut right, mut up) = sprite_axes(p, view, style);
                if style.shape == ParticleShape::Diamond {
                    let r = (right + up) * 0.5;
                    up = (up - right) * 0.5;
                    right = r;
                }
                let center = s.matrix * Vec3::from_array(p.position).extend(1.);
                let right = s.matrix * right.extend(0.);
                let up = s.matrix * up.extend(0.);
                if !center.is_finite() || !right.is_finite() || !up.is_finite() {
                    report.invalid += triangles as u32;
                    continue;
                }
                if outside_clip(center.to_array(), right.to_array(), up.to_array()) {
                    report.culled_samples += 1;
                    continue;
                }
                let mut instance = [0.; 16];
                instance[..4].copy_from_slice(&center.to_array());
                instance[4..8].copy_from_slice(&right.to_array());
                instance[8..12].copy_from_slice(&up.to_array());
                instance[12..].copy_from_slice(&p.color.map(|v| v.clamp(0., 1.)));
                let first = s.retained.sprites.len() as u32;
                if let Some(last) = s.retained.sprite_batches.last_mut().filter(|b| {
                    b.flags == flags
                        && b.triangles == triangles as u32
                        && s.retained.commands.last().is_some_and(|c| c.kind == 3)
                }) {
                    last.count += 1;
                } else {
                    s.retained.commands.push(Command {
                        kind: 3,
                        index: s.retained.sprite_batches.len() as u32,
                    });
                    s.retained.sprite_batches.push(SpriteBatch {
                        first,
                        count: 1,
                        flags,
                        triangles: triangles as u32,
                    });
                }
                s.retained.sprites.push(instance);
                report.triangles += triangles as u32;
            }
            s.effect_stats.invalid_triangles = s
                .effect_stats
                .invalid_triangles
                .saturating_add(report.invalid);
            s.effect_stats.culled_sprites = s
                .effect_stats
                .culled_sprites
                .saturating_add(report.culled_samples);
            s.effect_stats.vertices += report.triangles * 3;
            s.effect_stats.instanced_vertices += report.triangles * 3;
            report
        });
        report.dropped_samples = pool.len().saturating_sub(self.scratch.len()) as u32;
        report
    }
}
fn outside_clip(center: [f32; 4], right: [f32; 4], up: [f32; 4]) -> bool {
    let epsilon = center
        .into_iter()
        .chain(right)
        .chain(up)
        .map(|v| f64::from(v).abs())
        .fold(1., f64::max)
        * 1e-6;
    for axis in 0..3 {
        for sign in [-1., 1.] {
            let plane = |v: [f32; 4]| f64::from(v[3]) + sign * f64::from(v[axis]);
            if plane(center) + plane(right).abs() + plane(up).abs() < -epsilon {
                return true;
            }
        }
    }
    false
}
fn sprite_axes(p: ParticleView, view: BillboardView, style: ParticleStyle) -> (Vec3, Vec3) {
    let mut right = view.right;
    let mut up = view.up;
    let mut stretch = 1.;
    if style.stretch > 0. {
        let v = Vec3::from_array(p.velocity);
        let x = v.dot(right);
        let y = v.dot(up);
        let speed = x.hypot(y);
        if speed > 0.0001 {
            right = (right * x + up * y) / speed;
            up = view.right.cross(view.up).cross(right);
            stretch = (1. + speed * style.stretch).min(style.max_stretch);
        }
    }
    let (sin, cos) = p.rotation.sin_cos();
    let r = (right * cos + up * sin) * (p.size * 0.5 * stretch);
    let u = (up * cos - right * sin) * (p.size * 0.5);
    (r, u)
}

/// CPU reference geometry for software hosts and GPU comparison tests.
pub fn particle_triangles(
    p: ParticleView,
    view: BillboardView,
    style: ParticleStyle,
) -> Result<[Option<RibbonTriangle>; 2], EffectError> {
    if !p
        .position
        .into_iter()
        .chain(p.velocity)
        .chain(p.color)
        .chain([p.size, p.rotation, p.normalized_age])
        .all(f32::is_finite)
        || p.size < 0.
        || !(0.0..=1.).contains(&p.normalized_age)
    {
        return Err(EffectError::InvalidParticle);
    }
    let style = style.validate()?;
    let view = view.normalized()?;
    let (r, u) = sprite_axes(p, view, style);
    let center = Vec3::from_array(p.position);
    let vertex = |position: Vec3, uv| RibbonVertex {
        position: position.to_array(),
        color: p.color,
        uv,
    };
    if style.shape == ParticleShape::Triangle {
        return Ok([
            Some(RibbonTriangle {
                vertices: [
                    vertex(center + u, [0.5, 1.]),
                    vertex(center - r - u, [0., 0.]),
                    vertex(center + r - u, [1., 0.]),
                ],
            }),
            None,
        ]);
    }
    let points = if style.shape == ParticleShape::Diamond {
        [center - r, center + u, center - u, center + r]
    } else {
        [
            center - r + u,
            center - r - u,
            center + r + u,
            center + r - u,
        ]
    };
    let v = [
        vertex(points[0], [0., 0.]),
        vertex(points[1], [0., 1.]),
        vertex(points[2], [1., 0.]),
        vertex(points[3], [1., 1.]),
    ];
    Ok([
        Some(RibbonTriangle {
            vertices: [v[0], v[1], v[2]],
        }),
        Some(RibbonTriangle {
            vertices: [v[2], v[1], v[3]],
        }),
    ])
}
pub fn draw_ribbon<S: RibbonSource + ?Sized>(
    source: &S,
    facing: RibbonFacing,
    miter_limit: f32,
    style: EffectStyle,
) -> Result<DrawReport, EffectError> {
    Ok(draw_triangles(
        RibbonTriangles::new(source, facing, miter_limit)?,
        style,
    ))
}
pub fn draw_beams(
    beams: &BeamPool,
    facing: RibbonFacing,
    style: EffectStyle,
) -> Result<DrawReport, EffectError> {
    RibbonTriangles::new(&[] as &[RibbonPoint], facing, 1.)?;
    let triangles = beams.iter().flat_map(|beam| {
        let points = beam.points();
        let mut mesh = RibbonTriangles::new(&points, facing, 1.).expect("validated beam facing");
        [mesh.next(), mesh.next()].into_iter().flatten()
    });
    Ok(draw_triangles(triangles, style))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferravex_play::particles::ParticleDesc;
    fn triangle() -> RibbonTriangle {
        RibbonTriangle {
            vertices: [
                RibbonVertex {
                    position: [0., 0., 0.],
                    color: [1.; 4],
                    uv: [0.; 2],
                },
                RibbonVertex {
                    position: [10., 0., 0.],
                    color: [1.; 4],
                    uv: [0.; 2],
                },
                RibbonVertex {
                    position: [0., 10., 0.],
                    color: [1.; 4],
                    uv: [0.; 2],
                },
            ],
        }
    }
    #[test]
    fn alpha_2d_shares_geometry_but_additive_and_following_hud_preserve_order() {
        crate::input::host_frame(1. / 60., 960, 600, || {
            crate::clear_background(crate::BLACK);
            crate::draw_triangle(
                crate::Vec2::ZERO,
                crate::Vec2::X,
                crate::Vec2::Y,
                crate::WHITE,
            );
            assert_eq!(
                draw_triangles([triangle()], EffectStyle::default()).triangles,
                1
            );
            assert_eq!(
                draw_triangles(
                    [triangle()],
                    EffectStyle {
                        blend: EffectBlend::Additive,
                        ..Default::default()
                    }
                )
                .triangles,
                1
            );
            crate::draw_triangle(
                crate::Vec2::ZERO,
                crate::Vec2::X,
                crate::Vec2::Y,
                crate::GREEN,
            );
        });
        crate::read(|s| {
            assert_eq!(
                s.retained
                    .commands
                    .iter()
                    .map(|c| c.kind)
                    .collect::<Vec<_>>(),
                [0, 2, 0]
            );
            assert_eq!(s.batches[0].count, 6);
            assert_eq!(s.retained.effects[0].flags, 6);
            assert_eq!(s.effect_stats.vertices, 6);
        });
        assert_eq!(std::mem::size_of::<EffectBatch>(), 12);
    }
    #[test]
    fn sprite_culling_is_conservative_at_clip_edges_and_keeps_simulation_alive() {
        let mut pool = ParticlePool::new(3);
        for x in [0., 1.1, 100.] {
            pool.spawn(ParticleDesc {
                position: [x, 0., 0.],
                start_size: 0.4,
                ..Default::default()
            })
            .unwrap();
        }
        let mut renderer = ParticleRenderer::new(3).unwrap();
        crate::input::host_frame(1. / 60., 960, 600, || {
            crate::edit(|s| s.matrix = crate::Mat4::IDENTITY);
            let report = renderer.draw_2d(&pool, ParticleStyle::default()).unwrap();
            assert_eq!(report.culled_samples, 1);
            assert_eq!(report.triangles, 4);
        });
        assert_eq!(pool.len(), 3);
        assert_eq!(stats().culled_sprites, 1);
    }
    #[test]
    fn malformed_triangles_and_frame_budget_cannot_poison_or_grow_the_stream() {
        crate::input::host_frame(1. / 60., 960, 600, || {
            let mut bad = triangle();
            bad.vertices[0].position[0] = f32::NAN;
            let r = draw_triangles([bad, triangle()], EffectStyle::default());
            assert_eq!(r.invalid, 1);
            assert_eq!(r.triangles, 1);
            let r = draw_triangles(
                std::iter::repeat_n(triangle(), MAX_EFFECT_VERTICES),
                EffectStyle::default(),
            );
            assert!(r.truncated);
            assert!(draw_triangles([triangle()], EffectStyle::default()).truncated);
        });
        crate::read(|s| {
            assert_eq!(
                s.effect_stats.processed_triangles as usize,
                MAX_EFFECT_VERTICES / 3
            );
            assert_eq!(s.vertices.len(), MAX_EFFECT_VERTICES - 3);
            assert!(s.vertices.iter().flatten().all(|v| v.is_finite()));
        });
    }
    #[test]
    fn alpha_billboards_sort_by_camera_depth_and_report_scratch_overflow() {
        let mut pool = ParticlePool::new(3);
        for (z, color) in [
            (2., [0., 1., 0., 0.5]),
            (0., [1., 0., 0., 0.5]),
            (1., [0., 0., 1., 0.5]),
        ] {
            pool.spawn(ParticleDesc {
                position: [0., 0., z],
                start_color: color,
                ..Default::default()
            })
            .unwrap();
        }
        let mut renderer = ParticleRenderer::new(2).unwrap();
        crate::input::host_frame(1. / 60., 960, 600, || {
            crate::edit(|s| s.matrix = crate::Mat4::from_scale(Vec3::new(1., 1., 0.1)));
            let r = renderer
                .draw_3d(
                    &pool,
                    BillboardView::new(Vec3::new(0., 0., 5.), Vec3::X, Vec3::Y).unwrap(),
                    ParticleStyle::default(),
                )
                .unwrap();
            assert_eq!(r.triangles, 4);
            assert_eq!(r.dropped_samples, 1);
        });
        crate::read(|s| {
            assert_eq!(&s.retained.sprites[0][12..], [1., 0., 0., 0.5]);
            assert_eq!(&s.retained.sprites[1][12..], [0., 1., 0., 0.5]);
        });
        assert!(BillboardView::new(Vec3::ZERO, Vec3::X, Vec3::X).is_err());
        assert!(
            renderer
                .draw_2d(
                    &pool,
                    ParticleStyle {
                        stretch: f32::NAN,
                        ..Default::default()
                    }
                )
                .is_err()
        );
    }
}

/// ABI: 16 bytes; triangles is 1 for a triangle or 2 for a quad/diamond.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpriteBatch {
    pub first: u32,
    pub count: u32,
    pub flags: u32,
    pub triangles: u32,
}
pub const SPRITE_VERTEX_BODY: &str = r#"
attribute vec2 a_corner;
attribute vec4 a_center;
attribute vec4 a_right;
attribute vec4 a_up;
attribute vec4 a_tint;
varying vec4 v_color;
void main(){gl_Position=a_center+a_right*a_corner.x+a_up*a_corner.y;v_color=a_tint;}
"#;
pub const SPRITE_FRAGMENT_BODY: &str = r#"
varying vec4 v_color;
void main(){gl_FragColor=v_color;}
"#;
