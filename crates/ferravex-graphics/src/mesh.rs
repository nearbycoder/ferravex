use ferravex_math::{Color, Vec3, vec3};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vertex {
    pub position: Vec3,
    pub normal: Vec3,
    pub color: Color,
}
impl Vertex {
    pub const fn new(position: Vec3, normal: Vec3, color: Color) -> Self {
        Self {
            position,
            normal,
            color,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub min: Vec3,
    pub max: Vec3,
}
impl Bounds {
    pub fn center(self) -> Vec3 {
        (self.min + self.max) * 0.5
    }
    pub fn half_extents(self) -> Vec3 {
        (self.max - self.min) * 0.5
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeshError {
    Empty,
    InvalidTriangleCount,
    IndexOutOfBounds,
    NonFiniteVertex,
    TooLarge,
}
impl std::fmt::Display for MeshError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid mesh: {self:?}")
    }
}
impl std::error::Error for MeshError {}
impl Mesh {
    pub fn validate(&self) -> Result<Bounds, MeshError> {
        if self.vertices.is_empty() || self.indices.is_empty() {
            return Err(MeshError::Empty);
        }
        if self.vertices.len() > 1_000_000 || self.indices.len() > 6_000_000 {
            return Err(MeshError::TooLarge);
        }
        if !self.indices.len().is_multiple_of(3) {
            return Err(MeshError::InvalidTriangleCount);
        }
        let mut bounds = Bounds {
            min: Vec3::INFINITY,
            max: Vec3::NEG_INFINITY,
        };
        for v in &self.vertices {
            if !v.position.is_finite() || !v.normal.is_finite() || !v.color.to_vec().is_finite() {
                return Err(MeshError::NonFiniteVertex);
            }
            bounds.min = bounds.min.min(v.position);
            bounds.max = bounds.max.max(v.position);
        }
        if self
            .indices
            .iter()
            .any(|&i| i as usize >= self.vertices.len())
        {
            return Err(MeshError::IndexOutOfBounds);
        }
        Ok(bounds)
    }
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }
    /// Each face has its own vertices, preserving hard low-poly normals.
    pub fn cuboid(size: Vec3, color: Color) -> Self {
        assert!(size.is_finite() && size.cmpgt(Vec3::ZERO).all());
        let half = size * 0.5;
        let mut mesh = Self {
            vertices: Vec::with_capacity(24),
            indices: Vec::with_capacity(36),
        };
        for (n, u, v) in [
            (Vec3::X, Vec3::Y, Vec3::Z),
            (Vec3::NEG_X, Vec3::Y, Vec3::NEG_Z),
            (Vec3::Y, Vec3::Z, Vec3::X),
            (Vec3::NEG_Y, Vec3::Z, Vec3::NEG_X),
            (Vec3::Z, Vec3::X, Vec3::Y),
            (Vec3::NEG_Z, Vec3::NEG_X, Vec3::Y),
        ] {
            let start = mesh.vertices.len() as u32;
            for (a, b) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                mesh.vertices
                    .push(Vertex::new((n + u * a + v * b) * half, n, color));
            }
            mesh.indices
                .extend([start, start + 1, start + 2, start, start + 2, start + 3]);
        }
        mesh
    }
    /// A subdivided XZ floor with normal +Y.
    pub fn plane(width: f32, depth: f32, subdivisions: u16, color: Color) -> Self {
        assert!(width.is_finite() && depth.is_finite() && width > 0.0 && depth > 0.0);
        let n = usize::from(subdivisions.clamp(1, 128));
        let mut mesh = Self {
            vertices: Vec::with_capacity((n + 1) * (n + 1)),
            indices: Vec::with_capacity(n * n * 6),
        };
        for z in 0..=n {
            for x in 0..=n {
                mesh.vertices.push(Vertex::new(
                    vec3(
                        (x as f32 / n as f32 - 0.5) * width,
                        0.0,
                        (z as f32 / n as f32 - 0.5) * depth,
                    ),
                    Vec3::Y,
                    color,
                ));
            }
        }
        for z in 0..n {
            for x in 0..n {
                let a = (z * (n + 1) + x) as u32;
                let b = a + (n + 1) as u32;
                mesh.indices.extend([a, b, b + 1, a, b + 1, a + 1]);
            }
        }
        mesh
    }
    /// Faceted sphere. Duplicate per-triangle vertices keep normals flat.
    pub fn sphere(radius: f32, rings: u16, sectors: u16, color: Color) -> Self {
        assert!(radius.is_finite() && radius > 0.0);
        let rings = rings.clamp(2, 64);
        let sectors = sectors.clamp(3, 128);
        let mut mesh = Self {
            vertices: Vec::new(),
            indices: Vec::new(),
        };
        let point = |ring: u16, sector: u16| {
            let phi = std::f32::consts::PI * ring as f32 / rings as f32;
            let theta = std::f32::consts::TAU * sector as f32 / sectors as f32;
            vec3(phi.sin() * theta.cos(), phi.cos(), phi.sin() * theta.sin()) * radius
        };
        for ring in 0..rings {
            for sector in 0..sectors {
                let a = point(ring, sector);
                let b = point(ring + 1, sector);
                let c = point(ring + 1, sector + 1);
                let d = point(ring, sector + 1);
                if ring > 0 {
                    mesh.push_outward_triangle(a, c, d, color);
                }
                if ring + 1 < rings {
                    mesh.push_outward_triangle(a, b, c, color);
                }
            }
        }
        mesh
    }
    /// Faceted Y-axis cylinder, including closed caps.
    pub fn cylinder(radius: f32, height: f32, sides: u16, color: Color) -> Self {
        assert!(radius.is_finite() && height.is_finite() && radius > 0.0 && height > 0.0);
        let sides = sides.clamp(3, 128);
        let half = height * 0.5;
        let mut mesh = Self {
            vertices: Vec::new(),
            indices: Vec::new(),
        };
        for i in 0..sides {
            let a = std::f32::consts::TAU * i as f32 / sides as f32;
            let b = std::f32::consts::TAU * (i + 1) as f32 / sides as f32;
            let p = vec3(a.cos() * radius, -half, a.sin() * radius);
            let q = vec3(b.cos() * radius, -half, b.sin() * radius);
            let r = q + Vec3::Y * height;
            let s = p + Vec3::Y * height;
            mesh.push_outward_triangle(p, q, r, color);
            mesh.push_outward_triangle(p, r, s, color);
            mesh.push_outward_triangle(vec3(0.0, -half, 0.0), q, p, color);
            mesh.push_outward_triangle(vec3(0.0, half, 0.0), s, r, color);
        }
        mesh
    }
    fn push_outward_triangle(&mut self, a: Vec3, mut b: Vec3, mut c: Vec3, color: Color) {
        if (b - a).cross(c - a).dot(a + b + c) < 0.0 {
            std::mem::swap(&mut b, &mut c);
        }
        let normal = (b - a).cross(c - a).normalize_or_zero();
        let index = self.vertices.len() as u32;
        for point in [a, b, c] {
            self.vertices.push(Vertex::new(point, normal, color));
        }
        self.indices.extend([index, index + 1, index + 2]);
    }
    /// Bakes a caller-computed AO factor into RGB once. Alpha is preserved.
    pub fn bake_vertex_ao(&mut self, mut occlusion: impl FnMut(Vec3, Vec3) -> f32) {
        for vertex in &mut self.vertices {
            let factor = occlusion(vertex.position, vertex.normal).clamp(0.0, 1.0);
            let factor = if factor.is_finite() { factor } else { 1.0 };
            vertex.color.r *= factor;
            vertex.color.g *= factor;
            vertex.color.b *= factor;
        }
    }
}
