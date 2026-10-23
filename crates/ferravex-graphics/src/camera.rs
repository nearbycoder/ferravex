use crate::{Bounds, Mat4, Vec3, Vec4};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipDepth {
    NegativeOneToOne,
    ZeroToOne,
}
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub view_projection: Mat4,
    pub position: Vec3,
    pub clip_depth: ClipDepth,
}
impl Camera {
    pub fn perspective(
        eye: Vec3,
        target: Vec3,
        up: Vec3,
        fov_y: f32,
        aspect: f32,
        near: f32,
        far: f32,
    ) -> Self {
        Self {
            view_projection: Mat4::perspective_rh(fov_y, aspect, near, far)
                * Mat4::look_at_rh(eye, target, up),
            position: eye,
            clip_depth: ClipDepth::ZeroToOne,
        }
    }
    pub fn frustum(self) -> Option<Frustum> {
        Frustum::from_matrix(self.view_projection, self.clip_depth)
    }

    /// Cache projection and picking using this camera's actual clip-depth convention.
    ///
    /// Coordinates share the viewport's units (normally logical pixels). Rays
    /// start at the near plane; callers declare finite/infinite far intent because
    /// it cannot reliably be recovered from the composed matrix. Infinite far is
    /// currently rejected. Camera position and rendering remain unchanged.
    ///
    /// ```
    /// use ferravex_graphics::{Camera, Vec3, vec2, vec3};
    /// use ferravex_math::projection::{FarPlane, ScreenYAxis, Viewport};
    /// let camera = Camera::perspective(
    ///     vec3(0., 0., 5.), Vec3::ZERO, Vec3::Y, 1., 2., 0.5, 50.,
    /// );
    /// let projector = camera.prepare_projector(
    ///     Viewport { origin: vec2(20., 80.), size: vec2(800., 400.) },
    ///     ScreenYAxis::Down, FarPlane::Finite,
    /// )?;
    /// let anchor = projector.project(Vec3::ZERO)?;
    /// assert!(anchor.screen.abs_diff_eq(vec2(420., 280.), 0.001));
    /// let ray = projector.pick_ray(anchor.screen)?;
    /// assert!(ray.origin.abs_diff_eq(vec3(0., 0., 4.5), 0.001));
    /// assert!(ray.direction.abs_diff_eq(Vec3::NEG_Z, 0.001));
    /// # Ok::<(), ferravex_math::projection::ProjectionError>(())
    /// ```
    pub fn prepare_projector(
        self,
        viewport: ferravex_math::projection::Viewport,
        y_axis: ferravex_math::projection::ScreenYAxis,
        far_plane: ferravex_math::projection::FarPlane,
    ) -> Result<
        ferravex_math::projection::PreparedProjector3D,
        ferravex_math::projection::ProjectionError,
    > {
        use ferravex_math::projection::{NdcDepth, PreparedProjector3D};
        let depth = match self.clip_depth {
            ClipDepth::NegativeOneToOne => NdcDepth::NegativeOneToOne,
            ClipDepth::ZeroToOne => NdcDepth::ZeroToOne,
        };
        PreparedProjector3D::try_new(self.view_projection, viewport, y_axis, depth, far_plane)
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Frustum {
    planes: [Vec4; 6],
}
impl Frustum {
    pub fn from_matrix(matrix: Mat4, depth: ClipDepth) -> Option<Self> {
        if !matrix.is_finite() {
            return None;
        }
        let r0 = matrix.row(0);
        let r1 = matrix.row(1);
        let r2 = matrix.row(2);
        let r3 = matrix.row(3);
        let mut planes = [
            r3 + r0,
            r3 - r0,
            r3 + r1,
            r3 - r1,
            if depth == ClipDepth::ZeroToOne {
                r2
            } else {
                r3 + r2
            },
            r3 - r2,
        ];
        for plane in &mut planes {
            let length = plane.truncate().length();
            if !length.is_finite() {
                return None;
            }
            if length > 0.0 {
                *plane /= length;
            }
        }
        Some(Self { planes })
    }
    /// Exact transformed-box support test; conservative under nonuniform scale and shear.
    pub fn intersects_bounds(self, bounds: Bounds, transform: Mat4) -> bool {
        let center = transform.transform_point3(bounds.center());
        let half = bounds.half_extents();
        self.planes.into_iter().all(|plane| {
            let normal = plane.truncate();
            let radius = normal.dot(transform.x_axis.truncate()).abs() * half.x
                + normal.dot(transform.y_axis.truncate()).abs() * half.y
                + normal.dot(transform.z_axis.truncate()).abs() * half.z;
            normal.dot(center) + plane.w + radius >= 0.0
        })
    }
    pub fn contains_point(self, point: Vec3) -> bool {
        self.planes
            .into_iter()
            .all(|plane| plane.truncate().dot(point) + plane.w >= 0.0)
    }
}
