use crate::{Camera, ClipDepth, Color, MeshCache, RenderItem, Vec3, Vec4, vec2};

/// Resource bounds prevent accidental allocations or unbounded raster loops.
pub const MAX_DIMENSION: u32 = 8192;
pub const MAX_PIXELS: usize = 16_777_216;
const CLIP_VERTEX_CAPACITY: usize = 12;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderError {
    InvalidDimensions,
    InvalidCamera,
    InvalidSettings,
}
impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "software renderer: {self:?}")
    }
}
impl std::error::Error for RenderError {}

pub struct Framebuffer {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pixels: Vec<[u8; 4]>,
    depth: Vec<f32>,
}
impl Framebuffer {
    pub fn new(width: u32, height: u32) -> Result<Self, RenderError> {
        let count = (width as usize)
            .checked_mul(height as usize)
            .ok_or(RenderError::InvalidDimensions)?;
        if width == 0
            || height == 0
            || width > MAX_DIMENSION
            || height > MAX_DIMENSION
            || count > MAX_PIXELS
        {
            return Err(RenderError::InvalidDimensions);
        }
        Ok(Self {
            width,
            height,
            pixels: vec![[0, 0, 0, 255]; count],
            depth: vec![f32::INFINITY; count],
        })
    }
    pub const fn width(&self) -> u32 {
        self.width
    }
    pub const fn height(&self) -> u32 {
        self.height
    }
    /// Mutable color storage for 2D rasterizers; dimensions and depth storage remain fixed.
    pub fn pixels_mut(&mut self) -> &mut [[u8; 4]] {
        &mut self.pixels
    }
    pub fn pixels(&self) -> &[[u8; 4]] {
        &self.pixels
    }
    pub fn depths(&self) -> &[f32] {
        &self.depth
    }
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            None
        } else {
            Some(self.pixels[(y * self.width + x) as usize])
        }
    }
    pub fn clear(&mut self, color: Color) {
        self.pixels.fill(color.to_rgba());
        self.depth.fill(f32::INFINITY);
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Fog {
    pub color: Color,
    pub start: f32,
    pub end: f32,
}
#[derive(Clone, Copy, Debug)]
pub struct RenderSettings {
    pub look: crate::Look,
    pub clear_color: Color,
    /// Direction from the surface toward the light; normalized once each frame.
    pub light_direction: Vec3,
    pub ambient: f32,
    pub diffuse: f32,
    pub fog: Option<Fog>,
    pub cull_backfaces: bool,
}
impl Default for RenderSettings {
    fn default() -> Self {
        Self {
            look: crate::Look::Authored,
            clear_color: Color::from_hex(0x293945),
            light_direction: Vec3::new(-0.4, 0.8, 0.3),
            ambient: 0.35,
            diffuse: 0.65,
            fog: None,
            cull_backfaces: true,
        }
    }
}
impl RenderSettings {
    fn valid(self) -> bool {
        self.clear_color.to_vec().is_finite()
            && self.light_direction.is_finite()
            && self.ambient.is_finite()
            && self.diffuse.is_finite()
            && self.ambient >= 0.0
            && self.diffuse >= 0.0
            && self.fog.is_none_or(|f| {
                f.color.to_vec().is_finite()
                    && f.start.is_finite()
                    && f.end.is_finite()
                    && f.start >= 0.0
                    && f.end > f.start
            })
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RenderStats {
    pub submitted_instances: u64,
    pub visible_instances: u64,
    pub culled_instances: u64,
    pub invalid_instances: u64,
    pub submitted_triangles: u64,
    pub clipped_triangles: u64,
    pub rasterized_triangles: u64,
    pub shaded_pixels: u64,
}
#[derive(Clone, Copy, Default)]
struct ClipVertex {
    clip: Vec4,
    world: Vec3,
    color: Vec4,
}
impl ClipVertex {
    fn lerp(self, other: Self, t: f32) -> Self {
        Self {
            clip: self.clip.lerp(other.clip, t),
            world: self.world.lerp(other.world, t),
            color: self.color.lerp(other.color, t),
        }
    }
}

pub struct SoftwareRenderer {
    pub framebuffer: Framebuffer,
    pub settings: RenderSettings,
    scratch: Vec<ClipVertex>,
}
impl SoftwareRenderer {
    pub fn new(width: u32, height: u32) -> Result<Self, RenderError> {
        Ok(Self {
            framebuffer: Framebuffer::new(width, height)?,
            settings: RenderSettings::default(),
            scratch: Vec::new(),
        })
    }
    /// Clears the target and renders the complete opaque scene.
    /// Stale mesh handles, singular/non-affine transforms and degenerate triangles are skipped.
    pub fn render(
        &mut self,
        cache: &MeshCache,
        items: &[RenderItem],
        camera: Camera,
    ) -> Result<RenderStats, RenderError> {
        if !self.settings.valid() {
            return Err(RenderError::InvalidSettings);
        }
        if !camera.position.is_finite() {
            return Err(RenderError::InvalidCamera);
        }
        let frustum = camera.frustum().ok_or(RenderError::InvalidCamera)?;
        self.framebuffer
            .clear(self.settings.look.shade(self.settings.clear_color));
        let mut stats = RenderStats::default();
        let light = self.settings.light_direction.normalize_or_zero();
        let mut raster_settings = self.settings;
        raster_settings.fog = raster_settings.fog.map(|mut fog| {
            fog.color = self.settings.look.shade(fog.color);
            fog
        });
        for item in items {
            stats.submitted_instances += 1;
            let Some(mesh) = cache.get(item.mesh) else {
                stats.invalid_instances += 1;
                continue;
            };
            if !item.transform.is_finite() || !item.transform.row(3).abs_diff_eq(Vec4::W, 1.0e-5) {
                stats.invalid_instances += 1;
                continue;
            }
            let Some(inverse) = item.transform.try_inverse() else {
                stats.invalid_instances += 1;
                continue;
            };
            let bounds = cache.bounds(item.mesh).expect("same validated mesh handle");
            if !frustum.intersects_bounds(bounds, item.transform) {
                stats.culled_instances += 1;
                continue;
            }
            stats.visible_instances += 1;
            let normal_matrix = inverse.transpose();
            self.scratch.clear();
            self.scratch.reserve(mesh.vertices.len());
            for vertex in &mesh.vertices {
                let world = item.transform.transform_point3(vertex.position);
                let normal = normal_matrix
                    .transform_vector3(vertex.normal)
                    .normalize_or_zero();
                let lighting =
                    self.settings.ambient + self.settings.diffuse * normal.dot(light).max(0.0);
                let color = Vec4::new(
                    vertex.color.r * lighting,
                    vertex.color.g * lighting,
                    vertex.color.b * lighting,
                    vertex.color.a,
                );
                let color = self.settings.look.shade(Color::from(color)).to_vec();
                self.scratch.push(ClipVertex {
                    clip: camera.view_projection * world.extend(1.0),
                    world,
                    color,
                });
            }
            let mirrored = item.transform.determinant() < 0.0;
            for indices in mesh.indices.as_chunks::<3>().0 {
                stats.submitted_triangles += 1;
                let triangle = [
                    self.scratch[indices[0] as usize],
                    self.scratch[indices[1] as usize],
                    self.scratch[indices[2] as usize],
                ];
                let (mut polygon, count) = clip_triangle(triangle, camera.clip_depth);
                if count < 3 {
                    continue;
                }
                if mirrored {
                    polygon[..count].reverse();
                }
                stats.clipped_triangles += (count - 2) as u64;
                for i in 1..count - 1 {
                    raster_triangle(
                        &mut self.framebuffer,
                        [polygon[0], polygon[i], polygon[i + 1]],
                        raster_settings,
                        camera,
                        &mut stats,
                    );
                }
            }
        }
        Ok(stats)
    }
}
fn clip_distance(vertex: ClipVertex, plane: usize, depth: ClipDepth) -> f32 {
    let p = vertex.clip;
    match plane {
        0 => p.w + p.x,
        1 => p.w - p.x,
        2 => p.w + p.y,
        3 => p.w - p.y,
        4 => {
            if depth == ClipDepth::ZeroToOne {
                p.z
            } else {
                p.w + p.z
            }
        }
        5 => p.w - p.z,
        _ => p.w - 1.0e-6,
    }
}
/// Seven halfspaces: six clip planes plus w >= epsilon. A clipped triangle needs <= 10 vertices.
fn clip_triangle(
    triangle: [ClipVertex; 3],
    depth: ClipDepth,
) -> ([ClipVertex; CLIP_VERTEX_CAPACITY], usize) {
    let mut a = [ClipVertex::default(); CLIP_VERTEX_CAPACITY];
    let mut b = a;
    a[..3].copy_from_slice(&triangle);
    let mut count = 3;
    if triangle
        .iter()
        .any(|v| !v.clip.is_finite() || !v.world.is_finite() || !v.color.is_finite())
    {
        return (a, 0);
    }
    for plane in 0..7 {
        let mut next = 0;
        for i in 0..count {
            let current = a[i];
            let previous = a[(i + count - 1) % count];
            let dc = clip_distance(current, plane, depth);
            let dp = clip_distance(previous, plane, depth);
            if (dc >= 0.0) != (dp >= 0.0) {
                let ratio = (f64::from(dp) / (f64::from(dp) - f64::from(dc))) as f32;
                if next >= CLIP_VERTEX_CAPACITY {
                    return (a, 0);
                }
                b[next] = previous.lerp(current, ratio.clamp(0.0, 1.0));
                next += 1;
            }
            if dc >= 0.0 {
                if next >= CLIP_VERTEX_CAPACITY {
                    return (a, 0);
                }
                b[next] = current;
                next += 1;
            }
        }
        if next < 3 {
            return (a, 0);
        }
        std::mem::swap(&mut a, &mut b);
        count = next;
    }
    (a, count)
}
fn raster_triangle(
    frame: &mut Framebuffer,
    triangle: [ClipVertex; 3],
    settings: RenderSettings,
    camera: Camera,
    stats: &mut RenderStats,
) {
    let inverse_w = triangle.map(|v| v.clip.w.recip());
    let ndc = std::array::from_fn::<_, 3, _>(|i| triangle[i].clip.truncate() * inverse_w[i]);
    let screen = ndc.map(|p| {
        vec2(
            (p.x * 0.5 + 0.5) * frame.width as f32,
            (0.5 - p.y * 0.5) * frame.height as f32,
        )
    });
    let area = (screen[1] - screen[0]).perp_dot(screen[2] - screen[0]);
    if !area.is_finite() || area.abs() < 1.0e-6 || (settings.cull_backfaces && area >= 0.0) {
        return;
    }
    let min = screen[0].min(screen[1]).min(screen[2]).floor();
    let max = screen[0].max(screen[1]).max(screen[2]).ceil();
    let min_x = (min.x as i64).clamp(0, i64::from(frame.width) - 1) as u32;
    let max_x = (max.x as i64).clamp(0, i64::from(frame.width) - 1) as u32;
    let min_y = (min.y as i64).clamp(0, i64::from(frame.height) - 1) as u32;
    let max_y = (max.y as i64).clamp(0, i64::from(frame.height) - 1) as u32;
    // A sphere is convex: if all vertices are before fog start, every
    // perspective-correct surface sample is too. Keep the general fog path for
    // crossing triangles rather than approximating distance at the vertices.
    let fog = settings
        .fog
        .filter(|fog| {
            triangle
                .iter()
                .any(|v| v.world.distance_squared(camera.position) > fog.start * fog.start)
        })
        .map(|f| (f, (f.end - f.start).recip()));
    let flat_color = if fog.is_none()
        && triangle[0].color == triangle[1].color
        && triangle[0].color == triangle[2].color
    {
        let mut opaque = triangle[0].color;
        opaque.w = 1.;
        Some((triangle[0].color.w, Color::from(opaque).to_rgba()))
    } else {
        None
    };
    stats.rasterized_triangles += 1;
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let sample = vec2(x as f32 + 0.5, y as f32 + 0.5);
            let bary = [
                (screen[2] - screen[1]).perp_dot(sample - screen[1]) / area,
                (screen[0] - screen[2]).perp_dot(sample - screen[2]) / area,
                (screen[1] - screen[0]).perp_dot(sample - screen[0]) / area,
            ];
            if bary.iter().any(|&w| w < 0.0) {
                continue;
            }
            let depth = bary[0] * ndc[0].z + bary[1] * ndc[1].z + bary[2] * ndc[2].z;
            let depth = if camera.clip_depth == ClipDepth::NegativeOneToOne {
                depth * 0.5 + 0.5
            } else {
                depth
            };
            let index = (y * frame.width + x) as usize;
            if !depth.is_finite()
                || !(0.0..=1.00001).contains(&depth)
                || depth >= frame.depth[index]
            {
                continue;
            }
            if let Some((alpha, rgba)) = flat_color {
                if alpha > 0. {
                    frame.pixels[index] = rgba;
                    frame.depth[index] = depth.clamp(0.0, 1.0);
                    stats.shaded_pixels += 1;
                }
                continue;
            }
            let weights = std::array::from_fn::<_, 3, _>(|i| bary[i] * inverse_w[i]);
            let sum = weights.iter().sum::<f32>();
            if !sum.is_finite() || sum <= 0.0 {
                continue;
            }
            let mut color = Vec4::ZERO;
            // One reciprocal serves every perspective-correct attribute. World
            // position is only needed for fog, so un-fogged draws skip it entirely.
            let inverse_sum = sum.recip();
            let weights = weights.map(|weight| weight * inverse_sum);
            for i in 0..3 {
                color += triangle[i].color * weights[i];
            }
            if color.w <= 0.0 {
                continue;
            }
            if let Some((fog, inverse_range)) = fog {
                let position = triangle[0].world * weights[0]
                    + triangle[1].world * weights[1]
                    + triangle[2].world * weights[2];
                let amount = ((position.distance(camera.position) - fog.start) * inverse_range)
                    .clamp(0.0, 1.0);
                color = color.lerp(fog.color.to_vec(), amount);
            }
            color.w = 1.0;
            frame.pixels[index] = Color::from(color).to_rgba();
            frame.depth[index] = depth.clamp(0.0, 1.0);
            stats.shaded_pixels += 1;
        }
    }
}
