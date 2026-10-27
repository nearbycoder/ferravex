//! Retained low-poly geometry and ordered instanced draw recording.
use crate::{Mat4, Mesh, shaders::LitSettings};
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

const MAX_MESHES: usize = 1024;
const MAX_BYTES: usize = 64 * 1024 * 1024;
const MAX_INSTANCES: usize = 65536;

/// Retained CPU buffer capacities, in elements (not bytes or GPU allocations).
///
/// Buffers keep their allocations when a frame is cleared. Finalization swaps
/// the active instance/draw buffers with their scratch buffers, so inspect both
/// when planning memory use. See [`telemetry_snapshot`] for a usage example.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RetainedBufferCapacities {
    /// Registry slots, including space for expired weak mesh handles.
    pub mesh_slots: usize,
    /// Shared command buffer; includes immediate and effect commands too.
    pub commands: usize,
    /// Active instance records, each containing 28 floats.
    pub instances: usize,
    /// Active [`RetainedBatch`] records.
    pub draws: usize,
    /// Instance records reserved for finalization.
    pub scratch_instances: usize,
    /// Batch records reserved for finalization.
    pub scratch_draws: usize,
    /// Joint position matrices, each containing 16 floats.
    pub skin_matrices: usize,
    /// Joint normal matrices, each containing 9 floats.
    pub skin_normals: usize,
}

/// Rejected retained operations since the last [`reset_telemetry`], on this thread.
///
/// Each failed mesh creation or retained draw validation increments exactly one
/// category, selected by its first error. Counters saturate at `u64::MAX` and
/// survive frame clears. Pure [`validate_mesh`] and [`validate_transform`] calls
/// do not count. Culling and failures in host/skinning code before reaching the
/// retained recorder do not count; [`draw_resident_checked`] also counts its
/// preflight errors, including those for invisible objects. A failed
/// [`draw_residents_checked`] group increments one category once, not once per
/// rejected entry; its report contains the exact rejected entry count.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RetainedRejections {
    /// Invalid geometry or a mismatched skin vertex count.
    pub mesh_validation: u64,
    /// Individual mesh size, live mesh count, or total geometry byte limit.
    pub mesh_capacity: u64,
    /// Exhaustion of the monotonically increasing mesh identity space.
    pub mesh_identity: u64,
    /// Invalid model/camera transform or a mismatched skin palette.
    pub draw_validation: u64,
    /// Frame instance limit reached.
    pub instance_capacity: u64,
    /// Frame joint palette limit reached after attempting adjacent palette reuse.
    pub skin_capacity: u64,
}
thread_local! {
    static REJECTIONS: Cell<RetainedRejections> = Cell::new(RetainedRejections::default());
}
impl RetainedRejections {
    fn mesh(error: MeshError) {
        REJECTIONS.with(|cell| {
            let mut counts = cell.get();
            let count = match error {
                MeshError::MeshTooLarge { .. } | MeshError::StorageCapacity { .. } => {
                    &mut counts.mesh_capacity
                }
                MeshError::IdentityExhausted => &mut counts.mesh_identity,
                _ => &mut counts.mesh_validation,
            };
            *count = count.saturating_add(1);
            cell.set(counts);
        });
    }
    fn draw(error: DrawError) {
        REJECTIONS.with(|cell| {
            let mut counts = cell.get();
            let count = match error {
                DrawError::InstanceCapacity { .. } => &mut counts.instance_capacity,
                DrawError::SkinCapacity { .. } => &mut counts.skin_capacity,
                _ => &mut counts.draw_validation,
            };
            *count = count.saturating_add(1);
            cell.set(counts);
        });
    }
}

/// A copy of retained resource usage on the calling renderer thread.
///
/// Meshes include geometry kept alive by recorded draws, but exclude expired
/// weak handles and do not double-count cloned handles. Instance, draw and joint
/// usage describes the current frame buffers (the last completed frame until
/// the next clear). This is CPU recording telemetry, not a GPU timing or actual
/// driver draw-call count. See [`telemetry_snapshot`] for an example.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RetainedTelemetry {
    pub live_meshes: usize,
    pub mesh_limit: usize,
    /// Expanded vertex and skin payload bytes; excludes metadata and GPU copies.
    pub mesh_bytes: usize,
    pub mesh_byte_limit: usize,
    /// Accepted instances, including shadow-only submissions.
    pub instances: usize,
    pub instance_limit: usize,
    /// Retained batches after adjacent merging or the most recent finalization.
    pub draws: usize,
    /// Upper bound from the instance budget: each batch needs an instance.
    /// There is no separate draw-batch rejection threshold.
    pub draw_limit: usize,
    /// Stored palette joints; adjacent equal palettes share their storage.
    pub skin_joints: usize,
    pub skin_joint_limit: usize,
    pub buffers: RetainedBufferCapacities,
    pub rejections: RetainedRejections,
}

/// Snapshot retained usage, limits, buffer capacities and rejection counters.
///
/// Does not clear counters, prune the registry, finalize draws, or change mesh
/// lifetimes. After thread/renderer initialization this allocates nothing: it
/// scans at most the mesh limit of registry entries and copies frame counters.
///
/// ```
/// use ferravex_render::{mesh::Mesh3d, Mat4, Vec3, WHITE,
///     retained::{ResidentMesh, draw_resident_checked, reset_telemetry,
///                telemetry_snapshot}};
/// reset_telemetry();
/// let mesh = ResidentMesh::try_new(&Mesh3d::cuboid(Vec3::ONE, WHITE).mesh)?;
/// draw_resident_checked(&mesh, Mat4::IDENTITY)?;
/// assert!(draw_resident_checked(&mesh, Mat4::ZERO).is_err());
/// let sample = telemetry_snapshot();
/// assert_eq!(sample.instances, 1);
/// assert_eq!(sample.draws, 1);
/// assert!(sample.buffers.instances >= sample.instances);
/// assert!(sample.mesh_bytes <= sample.mesh_byte_limit);
/// assert_eq!(sample.rejections.draw_validation, 1);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn telemetry_snapshot() -> RetainedTelemetry {
    let (live_meshes, mesh_bytes, mesh_slots) = REGISTRY.with(|registry| {
        let registry = registry.borrow();
        let (live, bytes) = registry.usage();
        (live, bytes, registry.meshes.capacity())
    });
    crate::read(|s| {
        let frame = &s.retained;
        RetainedTelemetry {
            live_meshes,
            mesh_limit: MAX_MESHES,
            mesh_bytes,
            mesh_byte_limit: MAX_BYTES,
            instances: frame.instances.len(),
            instance_limit: MAX_INSTANCES,
            draws: frame.batches.len(),
            draw_limit: MAX_INSTANCES,
            skin_joints: frame.skin_matrices.len(),
            skin_joint_limit: crate::skinning::MAX_FRAME_SKIN_JOINTS,
            buffers: RetainedBufferCapacities {
                mesh_slots,
                commands: frame.commands.capacity(),
                instances: frame.instances.capacity(),
                draws: frame.batches.capacity(),
                scratch_instances: frame.scratch_instances.capacity(),
                scratch_draws: frame.scratch_batches.capacity(),
                skin_matrices: frame.skin_matrices.capacity(),
                skin_normals: frame.skin_normals.capacity(),
            },
            rejections: REJECTIONS.with(Cell::get),
        }
    })
}

/// Return and zero rejection counters for this thread without allocating.
///
/// Leaves geometry, identities, recorded draws and all buffer allocations alone.
/// Frame clears never implicitly reset these counters.
///
/// ```
/// use ferravex_render::{Mesh, retained::{ResidentMesh, RetainedRejections,
///                                      reset_telemetry, telemetry_snapshot}};
/// reset_telemetry();
/// assert!(ResidentMesh::try_new(&Mesh::default()).is_err());
/// assert_eq!(reset_telemetry().mesh_validation, 1);
/// assert_eq!(telemetry_snapshot().rejections, RetainedRejections::default());
/// ```
pub fn reset_telemetry() -> RetainedRejections {
    REJECTIONS.with(Cell::take)
}

/// A retained mesh authoring or storage error. All offsets are zero-based.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum MeshError {
    IncompleteTriangles {
        index_count: usize,
    },
    IndexOutOfBounds {
        index_position: usize,
        index: u16,
        vertex_count: usize,
    },
    NonFiniteVertex {
        vertex: usize,
        /// `position`, `uv`, or `normal`.
        attribute: &'static str,
        component: usize,
    },
    MeshTooLarge {
        index_count: usize,
        max_indices: usize,
    },
    StorageCapacity {
        live_meshes: usize,
        requested_bytes: usize,
        available_bytes: usize,
    },
    IdentityExhausted,
    SkinVertexCountMismatch {
        expected: usize,
        actual: usize,
    },
}
impl std::fmt::Display for MeshError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Self::IncompleteTriangles { index_count } => write!(
                f,
                "mesh has {index_count} indices; supply a nonempty triangle list with a multiple of 3 indices"
            ),
            Self::IndexOutOfBounds {
                index_position,
                index,
                vertex_count,
            } => write!(
                f,
                "mesh index[{index_position}] is {index}, but there are {vertex_count} vertices; use an index below {vertex_count}"
            ),
            Self::NonFiniteVertex {
                vertex,
                attribute,
                component,
            } => write!(
                f,
                "mesh vertex[{vertex}].{attribute}[{component}] is not finite; replace NaN or infinity with a finite value"
            ),
            Self::MeshTooLarge {
                index_count,
                max_indices,
            } => write!(
                f,
                "mesh has {index_count} indices; retained storage allows at most {max_indices} for this vertex layout; split or simplify the mesh"
            ),
            Self::StorageCapacity {
                live_meshes,
                requested_bytes,
                available_bytes,
            } => write!(
                f,
                "retained storage has {live_meshes}/{MAX_MESHES} live meshes and {available_bytes} bytes free; requested {requested_bytes} bytes; drop unused resident handles or simplify geometry"
            ),
            Self::IdentityExhausted => {
                f.write_str("retained mesh identities exhausted; restart the renderer thread")
            }
            Self::SkinVertexCountMismatch { expected, actual } => write!(
                f,
                "mesh has {expected} vertices but {actual} skin records; supply one skin record per source vertex"
            ),
        }
    }
}
impl std::error::Error for MeshError {}
impl MeshError {
    // Keep the original string-error API and its existing error categories.
    fn legacy_message(self) -> &'static str {
        match self {
            Self::IncompleteTriangles { .. } => "mesh requires complete triangles",
            Self::IndexOutOfBounds { .. } | Self::NonFiniteVertex { .. } => {
                "invalid retained mesh geometry"
            }
            Self::MeshTooLarge { .. } => "mesh exceeds retained storage budget",
            Self::StorageCapacity { .. } => "retained storage capacity exceeded",
            Self::IdentityExhausted => "mesh identity exhausted",
            Self::SkinVertexCountMismatch { .. } => "skin source vertex count mismatch",
        }
    }
}

/// Check source geometry without allocating, registering a mesh, or using a GPU.
/// Checks every source vertex, including unused vertices and UVs. Zero normals,
/// degenerate triangles, and UVs outside 0..1 remain valid. This checks the
/// individual static-mesh budget, not currently available registry capacity.
///
/// ```
/// use ferravex_render::{mesh::Mesh3d, retained::validate_mesh, Vec3, WHITE};
/// let mesh = Mesh3d::cuboid(Vec3::ONE, WHITE).mesh;
/// validate_mesh(&mesh)?;
/// # Ok::<(), ferravex_render::retained::MeshError>(())
/// ```
pub fn validate_mesh(mesh: &Mesh) -> Result<(), MeshError> {
    validate_mesh_layout(mesh, 40)
}
fn validate_mesh_layout(mesh: &Mesh, stride: usize) -> Result<(), MeshError> {
    if mesh.indices.is_empty() || !mesh.indices.len().is_multiple_of(3) {
        return Err(MeshError::IncompleteTriangles {
            index_count: mesh.indices.len(),
        });
    }
    if mesh.indices.len() > MAX_BYTES / stride {
        return Err(MeshError::MeshTooLarge {
            index_count: mesh.indices.len(),
            max_indices: MAX_BYTES / stride,
        });
    }
    for (index_position, &index) in mesh.indices.iter().enumerate() {
        if index as usize >= mesh.vertices.len() {
            return Err(MeshError::IndexOutOfBounds {
                index_position,
                index,
                vertex_count: mesh.vertices.len(),
            });
        }
    }
    for (vertex, v) in mesh.vertices.iter().enumerate() {
        let position = v.position.to_array();
        let uv = v.uv.to_array();
        let normal = v.normal.to_array();
        for (attribute, values) in [
            ("position", position.as_slice()),
            ("uv", uv.as_slice()),
            ("normal", normal.as_slice()),
        ] {
            if let Some(component) = values.iter().position(|v| !v.is_finite()) {
                return Err(MeshError::NonFiniteVertex {
                    vertex,
                    attribute,
                    component,
                });
            }
        }
    }
    Ok(())
}

/// Why a retained draw cannot be submitted. Matrix coordinates are zero-based.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DrawError {
    NonFiniteModel {
        column: usize,
        row: usize,
    },
    NonAffineModel {
        column: usize,
    },
    /// Singular, or its inverse cannot be represented with finite f32 values.
    NonInvertibleModel,
    NonFiniteCamera {
        column: usize,
        row: usize,
    },
    InstanceCapacity {
        limit: usize,
    },
    SkinPaletteMismatch {
        expected: usize,
        actual: usize,
    },
    SkinCapacity {
        limit: usize,
    },
}
impl std::fmt::Display for DrawError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Self::NonFiniteModel { column, row } => write!(f,
                "model matrix column {column}, row {row} is not finite; use a finite translation, rotation, and scale"),
            Self::NonAffineModel { column } => write!(f,
                "model matrix column {column}, row 3 is not affine; its last row must be [0, 0, 0, 1]; put projection in the camera matrix"),
            Self::NonInvertibleModel => f.write_str(
                "model matrix is singular or its inverse exceeds finite f32 range; use nonzero scales and independent transform axes"),
            Self::NonFiniteCamera { column, row } => write!(f,
                "camera matrix column {column}, row {row} is not finite; check the viewport, projection, and camera pose"),
            Self::InstanceCapacity { limit } => write!(f,
                "retained frame is full ({limit} instances); submit fewer instances or begin a new frame"),
            Self::SkinPaletteMismatch { expected, actual } => write!(f,
                "mesh needs {expected} skin joints, but draw supplied {actual}; draw skinned geometry with a matching SkinPalette"),
            Self::SkinCapacity { limit } => write!(f,
                "retained frame skin storage is full ({limit} joints); reuse palettes or begin a new frame"),
        }
    }
}
impl std::error::Error for DrawError {}

/// Check a finite affine model matrix and its inverse-transpose normal matrix.
/// Uses only stack storage. Reflections, shear, and nonuniform scale are valid;
/// projection belongs in the camera matrix. Does not evaluate shader arithmetic
/// or guarantee that extreme finite vertex/transform products cannot overflow.
///
/// ```
/// use ferravex_render::{Mat4, Vec3, retained::{validate_transform, DrawError}};
/// validate_transform(Mat4::from_scale(Vec3::new(-2., 3., 4.)))?;
/// assert_eq!(validate_transform(Mat4::from_scale(Vec3::ZERO)),
///            Err(DrawError::NonInvertibleModel));
/// # Ok::<(), DrawError>(())
/// ```
pub fn validate_transform(model: Mat4) -> Result<(), DrawError> {
    prepare_instance(model).map(|_| ())
}
fn prepare_instance(model: Mat4) -> Result<[f32; 28], DrawError> {
    let columns = model.to_cols_array();
    if let Some(i) = columns.iter().position(|v| !v.is_finite()) {
        return Err(DrawError::NonFiniteModel {
            column: i / 4,
            row: i % 4,
        });
    }
    for column in 0..4 {
        if columns[column * 4 + 3] != if column == 3 { 1. } else { 0. } {
            return Err(DrawError::NonAffineModel { column });
        }
    }
    let normal = model
        .try_inverse()
        .ok_or(DrawError::NonInvertibleModel)?
        .transpose()
        .to_cols_array();
    let mut instance = [0.; 28];
    instance[..16].copy_from_slice(&columns);
    for i in 0..3 {
        instance[16 + i * 4..19 + i * 4].copy_from_slice(&normal[i * 4..i * 4 + 3]);
    }
    Ok(instance)
}

/// Checked counterpart of [`crate::draw_resident`], using the current camera and
/// material. Returns `Ok(true)` for a visible recorded draw and `Ok(false)` when
/// culled (a shadow-only instance may still be recorded). Validation runs even
/// for culled draws; an error records nothing. Skinned geometry requires
/// [`crate::draw_skinned`] with its palette.
///
/// Shares the existing resident buffers and frame recording path. Validation
/// uses stack storage; frame buffers retain their capacity across frames.
///
/// ```
/// use ferravex_render::{mesh::Mesh3d, Mat4, Vec3, WHITE,
///     retained::{ResidentMesh, draw_resident_checked}};
/// let mesh = ResidentMesh::try_new(&Mesh3d::cuboid(Vec3::ONE, WHITE).mesh)?;
/// // After selecting your camera/material, in the frame loop:
/// let visible = draw_resident_checked(&mesh, Mat4::IDENTITY)?;
/// # let _ = visible;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn draw_resident_checked(mesh: &ResidentMesh, model: Mat4) -> Result<bool, DrawError> {
    crate::edit(|s| {
        s.retained
            .prepare_draw(mesh, model, s.matrix, None)
            .inspect_err(|error| RetainedRejections::draw(*error))?;
        let bounds = s
            .shader
            .as_ref()
            .map_or(mesh.bounds(), |m| m.padded(mesh.bounds()));
        Ok(s.submit(mesh, model, bounds, None))
    })
}

/// Outcome of one atomic [`draw_residents_checked`] call.
///
/// Counts are exact `usize` entry counts, not saturating activity counters.
/// `accepted + culled + rejected` equals the input slice length. See the
/// function's runnable recipe for handling both success and whole-group failure.
#[must_use]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InstanceGroupReport {
    /// Recorded instances, including shadow-only instances.
    pub accepted: usize,
    /// All input entries on failure; zero on success, including complete culls.
    pub rejected: usize,
    /// Valid entries omitted from both the visible and shadow passes.
    pub culled: usize,
    /// Visible recorded instances; a subset of `accepted`.
    pub visible: usize,
    /// First validation error, or the first entry exceeding aggregate capacity.
    pub error: Option<InstanceGroupError>,
}

/// A group's failure location and existing retained-draw diagnostic.
///
/// No entries were recorded. The index is zero-based in the caller slice.
/// See [`draw_residents_checked`] for validation/capacity precedence and usage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InstanceGroupError {
    pub index: usize,
    pub source: DrawError,
}
impl std::fmt::Display for InstanceGroupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "retained instance group entry[{}]: {}",
            self.index, self.source
        )
    }
}
impl std::error::Error for InstanceGroupError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

/// Atomically submit a caller slice of resident mesh/model pairs using the
/// current camera, material, lighting, visibility and shadow settings.
///
/// All entries are validated before recording, including invisible entries.
/// The first validation error in input order wins over aggregate capacity.
/// Otherwise capacity counts only instances that would actually be recorded
/// (visible or shadow-only); its error index is the first entry that would
/// exceed the remaining frame budget. Empty input succeeds without validation.
/// Skinned residents require palettes and are rejected, as by
/// [`draw_resident_checked`]. Borrowed [`ResidentMesh`] handles guarantee live,
/// already-validated geometry; cloned/duplicate handles are valid and each pair
/// is a separate instance. No raw identity lookup or new geometry is needed.
///
/// Failure leaves every frame buffer's contents, length, capacity, mesh/material
/// lifetimes and visibility counters unchanged. It increments exactly one
/// saturating [`RetainedRejections`] activity counter; `report.rejected` is the
/// entire slice length. Success records in caller order through the existing
/// single-instance path, with its adjacent batching and later opaque regrouping.
/// Valid complete culls are reported separately and are not rejections.
///
/// Preflight uses O(n) work and O(1) stack storage, with no scratch allocation.
/// Commit repeats preparation/classification; the caller bounds work by slice
/// length. Buffers grow normally on success and retain warmed capacity across
/// frames. Atomicity covers reported validation/capacity errors, not allocation
/// failures or unwinding/destructor panics; no panic rollback is promised.
///
/// # Runnable composition recipe
///
/// Retain authored geometry once, then submit each frame's caller-owned model
/// array as a group. This headless recipe uses the same recorder as game hosts;
/// it does not require a window or replace their rendering implementation.
///
/// ```rust
/// use ferravex_math::affine::PreparedAffine3D;
/// use ferravex_render::{Mat4, Vec3, WHITE, BLACK, clear_background,
///     set_default_camera, mesh::Mesh3d,
///     retained::{ResidentMesh, draw_residents_checked, telemetry_snapshot}};
///
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let resident = ResidentMesh::try_new(&Mesh3d::cuboid(Vec3::ONE, WHITE).mesh)?;
///     let clone = resident.clone(); // shares the validated geometry
///     let authored = PreparedAffine3D::try_new(Mat4::from_scale(
///         Vec3::new(-2.0, 3.0, 1.0)))?;
///     clear_background(BLACK);
///     set_default_camera();
///     let instances = [(&resident, authored.model()), (&clone, Mat4::IDENTITY)];
///     let report = draw_residents_checked(&instances);
///     assert_eq!((report.accepted, report.rejected, report.visible), (2, 0, 2));
///     assert_eq!(report.culled, 0);
///     assert!(report.error.is_none());
///     let before = telemetry_snapshot();
///     let failed = draw_residents_checked(&[
///         (&resident, Mat4::IDENTITY), (&resident, Mat4::ZERO)]);
///     assert_eq!((failed.accepted, failed.rejected), (0, 2));
///     assert_eq!(failed.error.unwrap().index, 1);
///     assert_eq!(telemetry_snapshot().instances, before.instances);
///     clear_background(BLACK); // frame reset reuses recording storage
///     let next = draw_residents_checked(&instances);
///     if let Some(error) = next.error { return Err(error.into()); }
///     assert_eq!(next.accepted, 2);
///     Ok(())
/// }
/// ```
pub fn draw_residents_checked(instances: &[(&ResidentMesh, Mat4)]) -> InstanceGroupReport {
    if instances.is_empty() {
        return InstanceGroupReport::default();
    }
    crate::edit(|s| {
        let reject = |index, source| {
            RetainedRejections::draw(source);
            InstanceGroupReport {
                rejected: instances.len(),
                error: Some(InstanceGroupError { index, source }),
                ..Default::default()
            }
        };
        let available = MAX_INSTANCES - s.retained.instances.len();
        let mut required = 0;
        let mut overflow = None;
        for (index, &(mesh, model)) in instances.iter().enumerate() {
            // A hypothetical empty instance buffer bypasses only the per-draw
            // capacity check; the group checks its actual aggregate demand below.
            if let Err(error) = s.retained.prepare_draw_at(mesh, model, s.matrix, None, 0) {
                return reject(index, error);
            }
            let bounds = s
                .shader
                .as_ref()
                .map_or(mesh.bounds(), |m| m.padded(mesh.bounds()));
            let visible =
                crate::visibility::classify(bounds, model, s.frustum, s.eye, s.visibility)
                    == crate::visibility::Visibility::Visible;
            // Mirror State::submit's shadow admission without recording or
            // touching visibility telemetry. Commit still uses State::submit.
            let casts = s.depth
                && mesh.is_opaque()
                && s.shader.as_ref().is_none_or(|m| {
                    m.0.options.blend == crate::material::BlendMode::Opaque
                        && m.0.options.casts_shadows
                })
                && s.shadow_frustum
                    .is_some_and(|f| f.intersects_bounds(bounds, model));
            if visible || casts {
                required += 1;
                if required > available && overflow.is_none() {
                    overflow = Some(index);
                }
            }
        }
        if let Some(index) = overflow {
            return reject(
                index,
                DrawError::InstanceCapacity {
                    limit: MAX_INSTANCES,
                },
            );
        }
        let before = s.retained.instances.len();
        let mut visible = 0;
        for &(mesh, model) in instances {
            let bounds = s
                .shader
                .as_ref()
                .map_or(mesh.bounds(), |m| m.padded(mesh.bounds()));
            visible += usize::from(s.submit(mesh, model, bounds, None));
        }
        let accepted = s.retained.instances.len() - before;
        debug_assert_eq!(accepted, required);
        InstanceGroupReport {
            accepted,
            culled: instances.len() - accepted,
            visible,
            ..Default::default()
        }
    })
}

#[derive(Clone)]
pub struct ResidentMesh(Rc<Geometry>);
pub(crate) struct Geometry {
    pub id: u32,
    pub vertices: Vec<[f32; 10]>,
    pub skin: Option<Vec<[f32; 8]>>,
    pub joint_count: usize,
    opaque: bool,
    bounds: ferravex_graphics::Bounds,
}
#[derive(Default)]
struct Registry {
    next: u32,
    meshes: Vec<(u32, Weak<Geometry>)>,
}
impl Registry {
    fn usage(&self) -> (usize, usize) {
        self.meshes
            .iter()
            .filter_map(|(_, mesh)| mesh.upgrade())
            .fold((0, 0), |(live, bytes), mesh| {
                (
                    live + 1,
                    bytes + mesh.vertices.len() * if mesh.skin.is_some() { 72 } else { 40 },
                )
            })
    }
}
thread_local! {static REGISTRY: RefCell<Registry> = RefCell::new(Registry::default());}
impl ResidentMesh {
    /// Snapshot validated geometry. Changes to the source Mesh require a new snapshot.
    /// Use [`Self::try_new`] for structured errors with source locations.
    pub fn new(mesh: &Mesh) -> Result<Self, &'static str> {
        Self::try_new(mesh).map_err(MeshError::legacy_message)
    }
    /// Snapshot geometry after validating all source vertices and triangle indices.
    /// Validation failure neither registers geometry nor consumes a mesh identity.
    /// Cloning the returned handle shares the snapshot and its GPU buffers.
    ///
    /// ```
    /// use ferravex_render::{mesh::Mesh3d, retained::ResidentMesh, Vec3, WHITE};
    /// let source = Mesh3d::cuboid(Vec3::ONE, WHITE).mesh;
    /// let resident = ResidentMesh::try_new(&source)?;
    /// assert_eq!(resident.vertex_count(), source.indices.len());
    /// # Ok::<(), ferravex_render::retained::MeshError>(())
    /// ```
    pub fn try_new(mesh: &Mesh) -> Result<Self, MeshError> {
        Self::with_skin_checked(mesh, None).inspect_err(|error| RetainedRejections::mesh(*error))
    }
    pub(crate) fn with_skin(
        mesh: &Mesh,
        skin: Option<(&[[f32; 8]], usize)>,
    ) -> Result<Self, &'static str> {
        Self::with_skin_checked(mesh, skin)
            .inspect_err(|error| RetainedRejections::mesh(*error))
            .map_err(MeshError::legacy_message)
    }
    fn with_skin_checked(
        mesh: &Mesh,
        skin: Option<(&[[f32; 8]], usize)>,
    ) -> Result<Self, MeshError> {
        if let Some((weights, _)) = skin.filter(|(weights, _)| weights.len() != mesh.vertices.len())
        {
            return Err(MeshError::SkinVertexCountMismatch {
                expected: mesh.vertices.len(),
                actual: weights.len(),
            });
        }
        let stride = if skin.is_some() { 72 } else { 40 };
        validate_mesh_layout(mesh, stride)?;
        REGISTRY.with(|registry| {
            let mut r = registry.borrow_mut();
            r.meshes.retain(|(_, mesh)| mesh.strong_count() > 0);
            let (_, bytes) = r.usage();
            if r.meshes.len() >= MAX_MESHES || bytes + mesh.indices.len() * stride > MAX_BYTES {
                return Err(MeshError::StorageCapacity {
                    live_meshes: r.meshes.len(),
                    requested_bytes: mesh.indices.len() * stride,
                    available_bytes: MAX_BYTES - bytes,
                });
            }
            r.next = r.next.checked_add(1).ok_or(MeshError::IdentityExhausted)?;
            let data = Rc::new(Geometry {
                id: r.next,
                skin: skin.map(|(weights, _)| {
                    mesh.indices.iter().map(|i| weights[*i as usize]).collect()
                }),
                joint_count: skin.map_or(0, |(_, joints)| joints),
                bounds: mesh
                    .indices
                    .iter()
                    .map(|i| mesh.vertices[*i as usize].position)
                    .fold(
                        ferravex_graphics::Bounds {
                            min: crate::Vec3::splat(f32::INFINITY),
                            max: crate::Vec3::splat(f32::NEG_INFINITY),
                        },
                        |b, p| ferravex_graphics::Bounds {
                            min: b.min.min(p),
                            max: b.max.max(p),
                        },
                    ),
                opaque: mesh
                    .indices
                    .iter()
                    .all(|i| mesh.vertices[*i as usize].color[3] == 255),
                vertices: mesh
                    .indices
                    .iter()
                    .map(|i| {
                        let v = mesh.vertices[*i as usize];
                        [
                            v.position.x,
                            v.position.y,
                            v.position.z,
                            v.normal.x,
                            v.normal.y,
                            v.normal.z,
                            v.color[0] as f32 / 255.,
                            v.color[1] as f32 / 255.,
                            v.color[2] as f32 / 255.,
                            v.color[3] as f32 / 255.,
                        ]
                    })
                    .collect(),
            });
            r.meshes.push((data.id, Rc::downgrade(&data)));
            Ok(Self(data))
        })
    }
    pub fn id(&self) -> u32 {
        self.0.id
    }
    pub fn vertex_count(&self) -> usize {
        self.0.vertices.len()
    }
    pub(crate) fn is_opaque(&self) -> bool {
        self.0.opaque
    }
    pub fn bounds(&self) -> ferravex_graphics::Bounds {
        self.0.bounds
    }
}
// IDs increase monotonically and pruning retains order. Binary search avoids
// walking/upgrading every earlier mesh during each host liveness check.
pub(crate) fn geometry(id: u32) -> Option<Rc<Geometry>> {
    REGISTRY.with(|r| {
        let r = r.borrow();
        let index = r.meshes.binary_search_by_key(&id, |(id, _)| *id).ok()?;
        r.meshes[index].1.upgrade()
    })
}
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn is_live(id: u32) -> bool {
    REGISTRY.with(|r| {
        let r = r.borrow();
        r.meshes
            .binary_search_by_key(&id, |(id, _)| *id)
            .is_ok_and(|index| r.meshes[index].1.strong_count() > 0)
    })
}
/// ABI: kind 0 = immediate, kind 1 = retained, kind 2 = flagged triangles, kind 3 = instanced sprites,
/// kind 4 = post-processing world/HUD boundary (index 0; see `post`). Preserves transparency/UI order.
/// Hosts skip unknown kinds.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Command {
    pub kind: u32,
    pub index: u32,
}
/// ABI: 224 bytes. Lighting: direction.xyz/enabled, eye.xyz/ambient, fog.rgba, start/end/padding.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RetainedBatch {
    pub mesh: u32,
    pub first: u32,
    pub count: u32,
    /// Bit 0: depth test; bit 1: suppress depth writes; bit 2: additive blend.
    pub depth: u32,
    pub camera: [f32; 16],
    pub light: [f32; 16],
    pub params: [f32; 16],
    pub material: u32,
    pub revision: u32,
    /// Skin palette first joint / joint count; count zero disables skinning.
    pub reserved: [u32; 2],
}
#[derive(Default)]
pub(crate) struct Frame {
    pub commands: Vec<Command>,
    pub effects: Vec<crate::effects::EffectBatch>,
    pub sprite_batches: Vec<crate::effects::SpriteBatch>,
    pub sprites: Vec<[f32; 16]>,
    pub batches: Vec<RetainedBatch>,
    pub instances: Vec<[f32; 28]>,
    pub skin_matrices: Vec<[f32; 16]>,
    pub skin_normals: Vec<[f32; 9]>,
    pub environment: crate::lighting::Environment,
    /// Resolved post-processing host block for this frame; index 0 is the tier.
    pub post: [f32; crate::post::POST_FRAME_FLOATS],
    scratch_instances: Vec<[f32; 28]>,
    scratch_batches: Vec<RetainedBatch>,
    keep_alive: Vec<ResidentMesh>,
    keep_materials: Vec<crate::material::ShaderMaterial>,
}
#[derive(Default, Clone, Copy)]
pub(crate) struct DrawStyle<'a> {
    pub look: crate::Look,
    pub light: Option<LitSettings>,
    pub material: Option<&'a crate::material::ShaderMaterial>,
    pub params: [f32; 16],
    pub shadow_only: bool,
    pub skip_shadow: bool,
    pub skin: Option<&'a crate::skinning::SkinPalette>,
}
impl RetainedBatch {
    fn same_pass(&self, other: &Self) -> bool {
        self.camera == other.camera
            && self.light == other.light
            && self.params == other.params
            && self.material == other.material
            && self.revision == other.revision
            && self.depth & 7 == other.depth & 7
    }
    fn compatible(&self, other: &Self) -> bool {
        self.mesh == other.mesh
            && self.depth == other.depth
            && self.reserved == other.reserved
            && self.same_pass(other)
    }
}
impl Frame {
    pub fn clear(&mut self) {
        self.commands.clear();
        self.effects.clear();
        self.sprite_batches.clear();
        self.sprites.clear();
        self.batches.clear();
        self.instances.clear();
        self.skin_matrices.clear();
        self.skin_normals.clear();
        self.keep_alive.clear();
        self.keep_materials.clear();
    }
    #[cfg(test)]
    pub fn record(
        &mut self,
        mesh: &ResidentMesh,
        model: Mat4,
        camera: Mat4,
        depth: bool,
        light: Option<LitSettings>,
    ) -> bool {
        self.record_style(
            mesh,
            model,
            camera,
            depth,
            DrawStyle {
                light,
                ..Default::default()
            },
        )
    }
    pub fn record_style(
        &mut self,
        mesh: &ResidentMesh,
        model: Mat4,
        camera: Mat4,
        depth: bool,
        style: DrawStyle<'_>,
    ) -> bool {
        let (instance, skin_at) = match self.prepare_draw(mesh, model, camera, style.skin) {
            Ok(prepared) => prepared,
            Err(error) => {
                RetainedRejections::draw(error);
                return false;
            }
        };
        let light = style.light;
        let bones = style.skin.map_or(0, |skin| skin.count);
        let l = light.unwrap_or_default().sanitized();
        let lighting = [
            l.light_direction.x,
            l.light_direction.y,
            l.light_direction.z,
            if light.is_some() { 1. } else { 0. },
            l.camera_position.x,
            l.camera_position.y,
            l.camera_position.z,
            l.ambient,
            l.fog_color.r,
            l.fog_color.g,
            l.fog_color.b,
            l.fog_color.a,
            l.fog_start,
            l.fog_end,
            style.look as u32 as f32,
            if mesh.0.opaque
                && style
                    .material
                    .is_none_or(|m| m.0.options.blend == crate::material::BlendMode::Opaque)
            {
                1.
            } else {
                0.
            },
        ];
        let mut depth_flags = depth as u32;
        if !depth {
            depth_flags |= 16;
        }
        if let Some(m) = style.material {
            if m.0.options.blend != crate::material::BlendMode::Opaque {
                depth_flags |= 2;
            }
            if m.0.options.blend == crate::material::BlendMode::Additive {
                depth_flags |= 4;
            }
            if !m.0.options.casts_shadows {
                depth_flags |= 8;
            }
            if !m.0.options.receives_shadows {
                depth_flags |= 16;
            }
        }
        if style.skip_shadow {
            depth_flags |= 8;
        }
        if style.shadow_only {
            depth_flags |= 32;
        }
        if let Some(skin) = style.skin.filter(|_| skin_at == self.skin_matrices.len()) {
            self.skin_matrices
                .extend_from_slice(&skin.matrices[..bones]);
            self.skin_normals.extend_from_slice(&skin.normals[..bones]);
        }
        let batch = RetainedBatch {
            mesh: mesh.id(),
            first: self.instances.len() as u32,
            count: 1,
            depth: depth_flags,
            camera: camera.to_cols_array(),
            light: lighting,
            params: style.params,
            material: style.material.map_or(0, |m| m.id()),
            revision: style.material.map_or(0, |m| m.0.revision.get()),
            reserved: if bones == 0 {
                [0; 2]
            } else {
                [skin_at as u32, bones as u32]
            },
        };
        if let Some(last) = self.batches.last_mut().filter(|last| {
            self.commands.last().is_some_and(|c| c.kind == 1) && last.compatible(&batch)
        }) {
            last.count += 1;
        } else {
            self.commands.push(Command {
                kind: 1,
                index: self.batches.len() as u32,
            });
            self.batches.push(batch);
            self.keep_alive.push(mesh.clone());
            if let Some(material) = style.material {
                self.keep_materials.push(material.clone());
            }
        }
        self.instances.push(instance);
        true
    }
    // No writes or allocation until every draw validation succeeds. The checked
    // public entry point also uses this before visibility can hide author errors.
    fn prepare_draw(
        &self,
        mesh: &ResidentMesh,
        model: Mat4,
        camera: Mat4,
        skin: Option<&crate::skinning::SkinPalette>,
    ) -> Result<([f32; 28], usize), DrawError> {
        self.prepare_draw_at(mesh, model, camera, skin, self.instances.len())
    }
    fn prepare_draw_at(
        &self,
        mesh: &ResidentMesh,
        model: Mat4,
        camera: Mat4,
        skin: Option<&crate::skinning::SkinPalette>,
        instance_count: usize,
    ) -> Result<([f32; 28], usize), DrawError> {
        let instance = prepare_instance(model)?;
        if let Some(i) = camera.to_cols_array().iter().position(|v| !v.is_finite()) {
            return Err(DrawError::NonFiniteCamera {
                column: i / 4,
                row: i % 4,
            });
        }
        if instance_count >= MAX_INSTANCES {
            return Err(DrawError::InstanceCapacity {
                limit: MAX_INSTANCES,
            });
        }
        let bones = skin.map_or(0, |skin| skin.count);
        if bones != mesh.0.joint_count {
            return Err(DrawError::SkinPaletteMismatch {
                expected: mesh.0.joint_count,
                actual: bones,
            });
        }
        let skin_at = if skin.is_some_and(|skin| {
            self.skin_matrices.len() >= bones
                && self.skin_matrices[self.skin_matrices.len() - bones..] == skin.matrices[..bones]
                && self.skin_normals[self.skin_normals.len() - bones..] == skin.normals[..bones]
        }) {
            self.skin_matrices.len() - bones
        } else {
            self.skin_matrices.len()
        };
        if skin_at + bones > crate::skinning::MAX_FRAME_SKIN_JOINTS {
            return Err(DrawError::SkinCapacity {
                limit: crate::skinning::MAX_FRAME_SKIN_JOINTS,
            });
        }
        Ok((instance, skin_at))
    }
    /// Group opaque depth-tested draws within an uninterrupted camera/material pass.
    /// Transparent draws and every immediate command are strict ordering barriers.
    pub fn finalize(&mut self) {
        let opaque = |c: &Command| {
            c.kind == 1
                && self.batches[c.index as usize].depth & 7 == 1
                && self.batches[c.index as usize].light[15] > 0.5
        };
        let mut begin = 0;
        while begin < self.commands.len() {
            if !opaque(&self.commands[begin]) {
                begin += 1;
                continue;
            }
            let first = self.batches[self.commands[begin].index as usize];
            let mut end = begin + 1;
            while end < self.commands.len() && opaque(&self.commands[end]) {
                let b = self.batches[self.commands[end].index as usize];
                if !b.same_pass(&first) {
                    break;
                }
                end += 1;
            }
            self.commands[begin..end].sort_unstable_by_key(|c| {
                let b = self.batches[c.index as usize];
                (b.mesh, b.depth)
            });
            begin = end;
        }
        self.scratch_instances.clear();
        self.scratch_batches.clear();
        let mut written = 0;
        let mut previous_retained = false;
        for i in 0..self.commands.len() {
            let c = self.commands[i];
            if c.kind != 1 {
                self.commands[written] = c;
                written += 1;
                previous_retained = false;
                continue;
            }
            let mut batch = self.batches[c.index as usize];
            let source =
                &self.instances[batch.first as usize..(batch.first + batch.count) as usize];
            batch.first = self.scratch_instances.len() as u32;
            self.scratch_instances.extend_from_slice(source);
            if let Some(last) = self
                .scratch_batches
                .last_mut()
                .filter(|last| previous_retained && last.compatible(&batch))
            {
                last.count += batch.count;
            } else {
                self.commands[written] = Command {
                    kind: 1,
                    index: self.scratch_batches.len() as u32,
                };
                written += 1;
                self.scratch_batches.push(batch);
            }
            previous_retained = true;
        }
        self.commands.truncate(written);
        std::mem::swap(&mut self.instances, &mut self.scratch_instances);
        std::mem::swap(&mut self.batches, &mut self.scratch_batches);
    }
}

/// Same vertex-lighting/fog equations as the CPU immediate path, evaluated by the GPU.
pub const VERTEX_BODY: &str = r#"
attribute vec3 a_position;
attribute vec3 a_normal;
attribute vec4 a_color;
attribute vec4 a_joints; attribute vec4 a_weights;
uniform int u_skin_count;
uniform mat4 u_bones[32];
uniform mat3 u_bone_normals[32];
attribute vec4 a_model0; attribute vec4 a_model1; attribute vec4 a_model2; attribute vec4 a_model3;
attribute vec4 a_normal0; attribute vec4 a_normal1; attribute vec4 a_normal2;
uniform mat4 u_camera;
uniform vec4 u_light[4];
uniform vec4 u_params[4];
varying vec4 v_color;
varying vec3 v_world;
varying vec3 v_normal;
vec3 ferravex_vertex(vec3 position);
vec3 ferravex_normal(vec3 normal);
void main(){
 mat4 model=mat4(a_model0,a_model1,a_model2,a_model3);
 vec3 position=a_position;vec3 posed_normal=a_normal;
 if(u_skin_count>0){
  position=vec3(0.0);posed_normal=vec3(0.0);
  for(int i=0;i<4;i++){int joint=int(a_joints[i]);position+=(u_bones[joint]*vec4(a_position,1.0)).xyz*a_weights[i];posed_normal+=u_bone_normals[joint]*a_normal*a_weights[i];}
 }
 vec4 world=model*vec4(ferravex_vertex(position),1.0);
 vec3 normal=mat3(a_normal0.xyz,a_normal1.xyz,a_normal2.xyz)*ferravex_normal(posed_normal);
 float n=length(normal);normal=n>0.0?normal/n:vec3(0.0);
 gl_Position=u_camera*world;v_color=a_color;v_world=world.xyz;v_normal=normal;
}
"#;
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Color, Vec3, mesh::Mesh3d};
    fn mesh() -> ResidentMesh {
        ResidentMesh::new(&Mesh3d::cuboid(Vec3::ONE, Color::new(1., 1., 1., 1.)).mesh).unwrap()
    }
    #[test]
    fn telemetry_tracks_live_geometry_without_counting_clones_or_expired_entries() {
        reset_telemetry();
        let source = Mesh3d::cuboid(Vec3::ONE, crate::WHITE).mesh;
        let resident = ResidentMesh::try_new(&source).unwrap();
        let clone = resident.clone();
        let packed = vec![[0.; 8]; source.vertices.len()];
        let skin = ResidentMesh::with_skin(&source, Some((&packed, 1))).unwrap();
        let snapshot = telemetry_snapshot();
        assert_eq!(snapshot.live_meshes, 2);
        assert_eq!(snapshot.mesh_bytes, source.indices.len() * (40 + 72));
        assert_eq!(snapshot.mesh_limit, MAX_MESHES);
        assert_eq!(snapshot.mesh_byte_limit, MAX_BYTES);
        assert!(snapshot.buffers.mesh_slots >= snapshot.live_meshes);
        assert_eq!(telemetry_snapshot(), snapshot);

        crate::input::host_frame(0., 800, 600, || {
            assert_eq!(draw_resident_checked(&resident, Mat4::IDENTITY), Ok(true));
        });
        drop((resident, clone, skin));
        let kept = telemetry_snapshot();
        assert_eq!(kept.live_meshes, 1); // Recorded batches keep geometry alive.
        assert_eq!(kept.mesh_bytes, source.indices.len() * 40);
        assert_eq!(kept.instances, 1);
        assert_eq!(kept.draws, 1);
        assert_eq!(reset_telemetry(), RetainedRejections::default());
        assert_eq!(telemetry_snapshot(), kept);

        crate::clear_background(crate::BLACK);
        let cleared = telemetry_snapshot();
        assert_eq!(cleared.live_meshes, 0);
        assert_eq!(cleared.mesh_bytes, 0);
        assert_eq!(cleared.instances, 0);
        assert_eq!(cleared.draws, 0);
        assert_eq!(cleared.buffers, kept.buffers);
        // Snapshotting does not prune expired weak handles.
        REGISTRY.with(|r| assert_eq!(r.borrow().meshes.len(), 2));
    }
    #[test]
    fn telemetry_counts_creation_errors_once_and_does_not_count_pure_validation() {
        reset_telemetry();
        let source = Mesh::default();
        assert!(validate_mesh(&source).is_err());
        assert!(validate_transform(Mat4::ZERO).is_err());
        assert_eq!(
            telemetry_snapshot().rejections,
            RetainedRejections::default()
        );
        assert!(ResidentMesh::try_new(&source).is_err());
        assert!(ResidentMesh::new(&source).is_err());
        let source = Mesh3d::cuboid(Vec3::ONE, crate::WHITE).mesh;
        assert!(ResidentMesh::with_skin(&source, Some((&[], 1))).is_err());
        let counts = telemetry_snapshot().rejections;
        assert_eq!(counts.mesh_validation, 3);
        assert_eq!(counts.mesh_capacity, 0);
        assert_eq!(reset_telemetry(), counts);
        assert_eq!(reset_telemetry(), RetainedRejections::default());
    }
    #[test]
    fn telemetry_reports_mesh_count_limit_and_recovery_without_consuming_identity() {
        reset_telemetry();
        let mut source = Mesh3d::cuboid(Vec3::ONE, crate::WHITE).mesh;
        source.indices.truncate(3);
        let mut meshes: Vec<_> = (0..MAX_MESHES)
            .map(|_| ResidentMesh::try_new(&source).unwrap())
            .collect();
        let before = telemetry_snapshot();
        assert_eq!(before.live_meshes, before.mesh_limit);
        assert_eq!(before.mesh_bytes, MAX_MESHES * 3 * 40);
        assert!(matches!(
            ResidentMesh::try_new(&source),
            Err(MeshError::StorageCapacity {
                live_meshes: MAX_MESHES,
                ..
            })
        ));
        assert_eq!(telemetry_snapshot().rejections.mesh_capacity, 1);
        let last_id = meshes.pop().unwrap().id();
        let next = ResidentMesh::try_new(&source).unwrap();
        assert_eq!(next.id(), last_id + 1);
        assert_eq!(telemetry_snapshot().live_meshes, MAX_MESHES);
    }
    #[test]
    fn telemetry_distinguishes_mesh_byte_limits_from_invalid_geometry() {
        reset_telemetry();
        let mut source = Mesh3d::cuboid(Vec3::ONE, crate::WHITE).mesh;
        let count = (MAX_BYTES / 40 / 3) * 3;
        source.indices = vec![0; count + 3];
        assert!(matches!(
            ResidentMesh::try_new(&source),
            Err(MeshError::MeshTooLarge { .. })
        ));
        source.indices.truncate(count);
        let large = ResidentMesh::try_new(&source).unwrap();
        let full = telemetry_snapshot();
        assert_eq!(full.live_meshes, 1);
        assert_eq!(full.mesh_bytes, count * 40);
        source.indices.truncate(3);
        assert!(matches!(
            ResidentMesh::try_new(&source),
            Err(MeshError::StorageCapacity { live_meshes: 1, available_bytes, .. })
                if available_bytes == MAX_BYTES - full.mesh_bytes
        ));
        let counts = telemetry_snapshot().rejections;
        assert_eq!(counts.mesh_capacity, 2);
        assert_eq!(counts.mesh_validation, 0);
        let id = large.id();
        drop(large);
        assert_eq!(telemetry_snapshot().mesh_bytes, 0);
        assert_eq!(ResidentMesh::try_new(&source).unwrap().id(), id + 1);
    }
    #[test]
    fn telemetry_tracks_batch_merging_and_buffer_swaps_without_resetting_counters() {
        struct IdentityCamera;
        impl crate::Camera for IdentityCamera {
            fn matrix(&self, _: f32, _: f32) -> Mat4 {
                Mat4::IDENTITY
            }
            fn depth(&self) -> bool {
                true
            }
        }
        reset_telemetry();
        let a = mesh();
        let b = mesh();
        crate::input::host_frame(0., 800, 600, || {
            crate::set_camera(&IdentityCamera);
            for mesh in [&a, &b, &a] {
                assert_eq!(draw_resident_checked(mesh, Mat4::IDENTITY), Ok(true));
            }
            let sample = telemetry_snapshot();
            assert_eq!((sample.instances, sample.draws), (3, 3));
            assert_eq!(sample.instance_limit, MAX_INSTANCES);
            assert_eq!(sample.draw_limit, sample.instance_limit);
            let outside = Mat4::from_translation(Vec3::X * 100.);
            assert_eq!(draw_resident_checked(&a, outside), Ok(false));
            assert_eq!(telemetry_snapshot(), sample);
            assert!(draw_resident_checked(&a, outside * Mat4::ZERO).is_err());
            assert_eq!(telemetry_snapshot().rejections.draw_validation, 1);
        });
        let finalized = telemetry_snapshot();
        assert_eq!((finalized.instances, finalized.draws), (3, 2));
        assert!(finalized.buffers.instances >= 3);
        assert!(finalized.buffers.scratch_instances >= 3);
        assert!(finalized.buffers.draws >= 2);
        assert!(finalized.buffers.scratch_draws >= 3);
        crate::input::host_frame(0., 800, 600, || {
            let cleared = telemetry_snapshot();
            assert_eq!((cleared.instances, cleared.draws), (0, 0));
            assert_eq!(cleared.buffers, finalized.buffers);
            assert_eq!(cleared.rejections.draw_validation, 1);
            assert_eq!(reset_telemetry(), cleared.rejections);
            let reset = telemetry_snapshot();
            assert_eq!(reset.rejections, RetainedRejections::default());
            assert_eq!(
                RetainedTelemetry {
                    rejections: cleared.rejections,
                    ..reset
                },
                cleared
            );
        });
    }
    #[test]
    fn telemetry_counts_checked_and_legacy_instance_rejections_once_each() {
        reset_telemetry();
        let mesh = mesh();
        crate::input::host_frame(0., 800, 600, || {
            for _ in 0..MAX_INSTANCES {
                assert!(crate::draw_resident(&mesh, Mat4::IDENTITY));
            }
            let full = telemetry_snapshot();
            assert_eq!(full.instances, full.instance_limit);
            assert_eq!(full.draws, 1); // An instance budget, not a batch budget.
            assert_eq!(
                draw_resident_checked(&mesh, Mat4::IDENTITY),
                Err(DrawError::InstanceCapacity {
                    limit: MAX_INSTANCES
                })
            );
            assert!(!crate::draw_resident(&mesh, Mat4::IDENTITY));
            let rejected = telemetry_snapshot();
            assert_eq!(rejected.rejections.instance_capacity, 2);
            assert_eq!(
                RetainedTelemetry {
                    rejections: full.rejections,
                    ..rejected
                },
                full
            );
            crate::clear_background(crate::BLACK);
            assert_eq!(draw_resident_checked(&mesh, Mat4::IDENTITY), Ok(true));
            assert_eq!(telemetry_snapshot().buffers, full.buffers);
            assert_eq!(telemetry_snapshot().rejections.instance_capacity, 2);
        });
    }
    #[test]
    fn telemetry_counts_skin_capacity_after_palette_reuse_and_preserves_storage() {
        use crate::skinning::{MAX_FRAME_SKIN_JOINTS, MAX_SKIN_JOINTS, SkinPalette};
        reset_telemetry();
        let source = Mesh3d::cuboid(Vec3::ONE, crate::WHITE).mesh;
        let packed = vec![[0.; 8]; source.vertices.len()];
        let mesh = ResidentMesh::with_skin(&source, Some((&packed, MAX_SKIN_JOINTS))).unwrap();
        let a = SkinPalette::new(&[Mat4::IDENTITY.to_cols_array(); MAX_SKIN_JOINTS]).unwrap();
        let b =
            SkinPalette::new(&[Mat4::from_translation(Vec3::X).to_cols_array(); MAX_SKIN_JOINTS])
                .unwrap();
        crate::edit(|s| {
            for i in 0..MAX_FRAME_SKIN_JOINTS / MAX_SKIN_JOINTS {
                assert!(s.retained.record_style(
                    &mesh,
                    Mat4::IDENTITY,
                    Mat4::IDENTITY,
                    true,
                    DrawStyle {
                        skin: Some(if i % 2 == 0 { &a } else { &b }),
                        ..Default::default()
                    }
                ));
            }
            // The last palette can still be reused when joint storage is full.
            assert!(s.retained.record_style(
                &mesh,
                Mat4::IDENTITY,
                Mat4::IDENTITY,
                true,
                DrawStyle {
                    skin: Some(&b),
                    ..Default::default()
                }
            ));
        });
        let full = telemetry_snapshot();
        assert_eq!(full.skin_joints, full.skin_joint_limit);
        assert_eq!(full.rejections, RetainedRejections::default());
        crate::edit(|s| {
            assert!(!s.retained.record_style(
                &mesh,
                Mat4::IDENTITY,
                Mat4::IDENTITY,
                true,
                DrawStyle {
                    skin: Some(&a),
                    ..Default::default()
                }
            ));
        });
        let rejected = telemetry_snapshot();
        assert_eq!(rejected.rejections.skin_capacity, 1);
        assert_eq!(
            RetainedTelemetry {
                rejections: full.rejections,
                ..rejected
            },
            full
        );
        assert!(matches!(
            draw_resident_checked(&mesh, Mat4::IDENTITY),
            Err(DrawError::SkinPaletteMismatch { .. })
        ));
        assert_eq!(telemetry_snapshot().rejections.draw_validation, 1);
        crate::clear_background(crate::BLACK);
        assert_eq!(telemetry_snapshot().skin_joints, 0);
        assert_eq!(telemetry_snapshot().buffers, full.buffers);
    }
    #[test]
    fn telemetry_identity_rejection_and_saturating_counters_are_independent_of_reset() {
        reset_telemetry();
        let source = Mesh3d::cuboid(Vec3::ONE, crate::WHITE).mesh;
        REGISTRY.with(|r| r.borrow_mut().next = u32::MAX);
        assert!(matches!(
            ResidentMesh::try_new(&source),
            Err(MeshError::IdentityExhausted)
        ));
        assert_eq!(reset_telemetry().mesh_identity, 1);
        assert!(matches!(
            ResidentMesh::try_new(&source),
            Err(MeshError::IdentityExhausted)
        ));
        assert_eq!(reset_telemetry().mesh_identity, 1); // Reset never recycles IDs.
        REGISTRY.with(|r| r.borrow_mut().next = 0);
        let mesh = ResidentMesh::try_new(&source).unwrap();
        REJECTIONS.with(|r| {
            r.set(RetainedRejections {
                mesh_validation: u64::MAX,
                draw_validation: u64::MAX,
                ..Default::default()
            })
        });
        assert!(ResidentMesh::try_new(&Mesh::default()).is_err());
        assert!(draw_resident_checked(&mesh, Mat4::ZERO).is_err());
        assert_eq!(
            reset_telemetry(),
            RetainedRejections {
                mesh_validation: u64::MAX,
                draw_validation: u64::MAX,
                ..Default::default()
            }
        );
    }
    #[test]
    fn telemetry_is_thread_local() {
        reset_telemetry();
        let _resident = mesh();
        assert!(ResidentMesh::try_new(&Mesh::default()).is_err());
        std::thread::spawn(|| {
            let sample = telemetry_snapshot();
            assert_eq!(sample.live_meshes, 0);
            assert_eq!(sample.rejections, RetainedRejections::default());
            reset_telemetry();
        })
        .join()
        .unwrap();
        assert_eq!(telemetry_snapshot().live_meshes, 1);
        assert_eq!(reset_telemetry().mesh_validation, 1);
    }
    #[test]
    fn malformed_geometry_is_rejected_before_identity_or_storage_changes() {
        let mut source = Mesh3d::cuboid(Vec3::ONE, crate::WHITE).mesh;
        let before = mesh();
        source.indices[4] = u16::MAX;
        assert!(matches!(
            ResidentMesh::try_new(&source),
            Err(MeshError::IndexOutOfBounds {
                index_position: 4,
                index: u16::MAX,
                ..
            })
        ));
        assert_eq!(
            ResidentMesh::new(&source).err(),
            Some("invalid retained mesh geometry")
        );
        let after = mesh();
        assert_eq!(after.id(), before.id() + 1);
        assert!(geometry(before.id()).is_some());
    }
    #[test]
    fn all_vertex_attributes_and_unused_vertices_must_be_finite() {
        let mut source = Mesh3d::cuboid(Vec3::ONE, crate::WHITE).mesh;
        let unused = source.vertices.len();
        source.vertices.push(source.vertices[0]);
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            for (attribute, count) in [("position", 3), ("uv", 2), ("normal", 4)] {
                for component in 0..count {
                    source.vertices[unused] = source.vertices[0];
                    let vertex = &mut source.vertices[unused];
                    match attribute {
                        "position" => vertex.position[component] = value,
                        "uv" => vertex.uv[component] = value,
                        _ => vertex.normal[component] = value,
                    }
                    let expected = MeshError::NonFiniteVertex {
                        vertex: unused,
                        attribute,
                        component,
                    };
                    assert_eq!(validate_mesh(&source), Err(expected));
                    assert!(
                        matches!(ResidentMesh::try_new(&source), Err(error) if error == expected)
                    );
                    assert!(ResidentMesh::new(&source).is_err());
                }
            }
        }
        source.vertices[unused] = source.vertices[0];
        assert!(ResidentMesh::try_new(&source).is_ok());
    }
    #[test]
    fn triangle_lists_and_individual_storage_budget_are_checked() {
        let mut source = Mesh3d::cuboid(Vec3::ONE, crate::WHITE).mesh;
        for count in [0, 1, 2, 4] {
            source.indices = vec![0; count];
            assert_eq!(
                validate_mesh(&source),
                Err(MeshError::IncompleteTriangles { index_count: count })
            );
        }
        let count = (MAX_BYTES / 40 / 3 + 1) * 3;
        source.indices = vec![0; count];
        assert_eq!(
            validate_mesh(&source),
            Err(MeshError::MeshTooLarge {
                index_count: count,
                max_indices: MAX_BYTES / 40,
            })
        );
        source.indices = vec![0, 0, 0]; // Degenerate authored triangles remain supported.
        assert_eq!(validate_mesh(&source), Ok(()));
    }
    #[test]
    fn invalid_draws_do_not_change_recorded_batches_or_skin_storage() {
        use crate::skinning::{SkinPalette, SkinWeights, SkinnedMesh};
        let source = Mesh3d::cuboid(Vec3::ONE, crate::WHITE).mesh;
        let skin = SkinnedMesh::new(
            &source,
            &vec![SkinWeights::rigid(0); source.vertices.len()],
            1,
        )
        .unwrap();
        let palette = SkinPalette::new(&[Mat4::IDENTITY.to_cols_array()]).unwrap();
        let changed_palette =
            SkinPalette::new(&[Mat4::from_translation(Vec3::X).to_cols_array()]).unwrap();
        let mut f = Frame::default();
        assert!(f.record_style(
            skin.resident(),
            Mat4::IDENTITY,
            Mat4::IDENTITY,
            true,
            DrawStyle {
                skin: Some(&palette),
                ..Default::default()
            }
        ));
        let instances = f.instances.clone();
        let batches = f.batches.clone();
        let matrices = f.skin_matrices.clone();
        let normals = f.skin_normals.clone();
        let mut perspective_model = Mat4::IDENTITY;
        perspective_model.x_axis.w = 0.25;
        for (model, camera) in [
            (Mat4::NAN, Mat4::IDENTITY),
            (Mat4::from_scale(Vec3::ZERO), Mat4::IDENTITY),
            (perspective_model, Mat4::IDENTITY),
            (Mat4::IDENTITY, Mat4::NAN),
            (
                Mat4::from_scale(Vec3::new(f32::from_bits(1), 1., 1.)),
                Mat4::IDENTITY,
            ),
        ] {
            assert!(
                f.prepare_draw(skin.resident(), model, camera, Some(&changed_palette))
                    .is_err()
            );
            assert!(!f.record_style(
                skin.resident(),
                model,
                camera,
                true,
                DrawStyle {
                    skin: Some(&changed_palette),
                    ..Default::default()
                }
            ));
            assert_eq!(f.instances, instances);
            assert_eq!(f.batches, batches);
            assert_eq!(f.skin_matrices, matrices);
            assert_eq!(f.skin_normals, normals);
            assert_eq!(f.commands.len(), 1);
            assert_eq!(f.keep_alive.len(), 1);
        }
        assert!(f.record_style(
            skin.resident(),
            Mat4::IDENTITY,
            Mat4::IDENTITY,
            true,
            DrawStyle {
                skin: Some(&palette),
                ..Default::default()
            }
        ));
        assert_eq!(f.skin_matrices.len(), 1); // Palette reuse survives rejected draws.
        assert_eq!(f.batches[0].count, 2);
    }
    #[test]
    fn frame_limits_are_reported_and_rejection_is_transactional() {
        let mesh = mesh();
        let mut f = Frame::default();
        f.instances.resize(MAX_INSTANCES, [0.; 28]);
        let capacity = f.instances.capacity();
        assert_eq!(
            f.prepare_draw(&mesh, Mat4::IDENTITY, Mat4::IDENTITY, None),
            Err(DrawError::InstanceCapacity {
                limit: MAX_INSTANCES
            })
        );
        assert!(!f.record(&mesh, Mat4::IDENTITY, Mat4::IDENTITY, true, None));
        assert_eq!(f.instances.len(), MAX_INSTANCES);
        assert!(f.commands.is_empty());
        f.clear();
        assert!(f.record(&mesh, Mat4::IDENTITY, Mat4::IDENTITY, true, None));
        assert_eq!(f.instances.capacity(), capacity);
    }
    #[test]
    fn resident_snapshot_and_shared_handle_survive_source_edits() {
        let mut source = Mesh3d::cuboid(Vec3::ONE, crate::WHITE).mesh;
        let mesh = ResidentMesh::try_new(&source).unwrap();
        let shared = mesh.clone();
        let snapshot = geometry(mesh.id()).unwrap();
        source.vertices[0].position.x = f32::NAN;
        assert!(ResidentMesh::try_new(&source).is_err());
        assert_eq!(shared.id(), mesh.id());
        assert_eq!(shared.bounds(), mesh.bounds());
        assert!(Rc::ptr_eq(&snapshot, &geometry(shared.id()).unwrap()));
        assert!(snapshot.vertices.iter().flatten().all(|v| v.is_finite()));
    }
    #[test]
    fn compatible_instances_merge_but_camera_and_order_are_preserved() {
        let mesh = mesh();
        let mut f = Frame::default();
        assert!(f.record(&mesh, Mat4::IDENTITY, Mat4::IDENTITY, true, None));
        f.record(
            &mesh,
            Mat4::from_translation(Vec3::X),
            Mat4::IDENTITY,
            true,
            None,
        );
        assert_eq!(f.batches.len(), 1);
        assert_eq!(f.batches[0].count, 2);
        f.commands.push(Command { kind: 0, index: 0 });
        f.record(&mesh, Mat4::IDENTITY, Mat4::IDENTITY, true, None);
        f.record(
            &mesh,
            Mat4::IDENTITY,
            Mat4::from_translation(Vec3::X),
            true,
            None,
        );
        assert_eq!(f.batches.len(), 3);
        assert_eq!(f.commands.len(), 4);
        assert!(!f.record(
            &mesh,
            Mat4::from_scale(Vec3::ZERO),
            Mat4::IDENTITY,
            true,
            None
        ));
    }
    #[test]
    fn lifetime_keeps_draws_valid_and_ids_never_recycle() {
        let mut f = Frame::default();
        let id;
        {
            let mesh = mesh();
            id = mesh.id();
            f.record(&mesh, Mat4::IDENTITY, Mat4::IDENTITY, true, None);
        }
        assert!(is_live(id));
        f.clear();
        assert!(!is_live(id));
        assert_ne!(id, mesh().id());
    }
    #[test]
    fn lookup_survives_dead_entries_and_registry_pruning() {
        let meshes: Vec<_> = (0..64).map(|_| mesh()).collect();
        let ids: Vec<_> = meshes.iter().map(ResidentMesh::id).collect();
        let live: Vec<_> = meshes
            .into_iter()
            .enumerate()
            .filter_map(|(i, mesh)| (i % 3 == 0).then_some(mesh))
            .collect();
        for (i, &id) in ids.iter().enumerate() {
            assert_eq!(geometry(id).is_some(), i % 3 == 0);
            assert_eq!(is_live(id), i % 3 == 0);
        }
        let next = mesh(); // Removes expired weak entries without recycling IDs.
        assert!(next.id() > *ids.last().unwrap());
        for mesh in &live {
            assert_eq!(geometry(mesh.id()).unwrap().id, mesh.id());
        }
        for (i, &id) in ids.iter().enumerate() {
            assert_eq!(is_live(id), i % 3 == 0);
        }
        assert!(geometry(0).is_none());
        assert!(geometry(u32::MAX).is_none());
    }
    #[test]
    fn wire_layout_and_normal_matrix_are_defined() {
        assert_eq!(std::mem::size_of::<Command>(), 8);
        assert_eq!(std::mem::size_of::<RetainedBatch>(), 224);
        assert_eq!(std::mem::offset_of!(RetainedBatch, camera), 16);
        assert_eq!(std::mem::offset_of!(RetainedBatch, light), 80);
        assert_eq!(std::mem::offset_of!(RetainedBatch, params), 144);
        assert_eq!(std::mem::offset_of!(RetainedBatch, material), 208);
        let mut f = Frame::default();
        f.record(
            &mesh(),
            Mat4::from_scale(Vec3::new(2., 4., 8.)),
            Mat4::IDENTITY,
            true,
            None,
        );
        assert_eq!(f.instances[0][16], 0.5);
        assert_eq!(f.instances[0][21], 0.25);
        assert_eq!(f.instances[0][26], 0.125);
    }
    #[test]
    fn material_and_parameter_changes_cannot_be_merged() {
        use crate::material::{MaterialOptions, ShaderMaterial};
        let mesh = mesh();
        let a = ShaderMaterial::new(
            Default::default(),
            MaterialOptions {
                blend: crate::material::BlendMode::Opaque,
                ..Default::default()
            },
        )
        .unwrap();
        let b = ShaderMaterial::new(
            Default::default(),
            MaterialOptions {
                blend: crate::material::BlendMode::Opaque,
                ..Default::default()
            },
        )
        .unwrap();
        let mut f = Frame::default();
        for (m, v) in [(&a, 1.), (&b, 1.), (&b, 2.)] {
            f.record_style(
                &mesh,
                Mat4::IDENTITY,
                Mat4::IDENTITY,
                true,
                DrawStyle {
                    material: Some(m),
                    params: [v; 16],
                    ..Default::default()
                },
            );
        }
        f.finalize();
        assert_eq!(f.batches.len(), 3);
        assert_eq!(
            f.batches
                .iter()
                .map(|b| (b.material, b.params[0]))
                .collect::<Vec<_>>(),
            [(a.id(), 1.), (b.id(), 1.), (b.id(), 2.)]
        );
    }
    #[test]
    fn opaque_reordering_copies_instances_and_stops_at_ui_barriers() {
        let a = mesh();
        let b = mesh();
        let mut f = Frame::default();
        for (i, m) in [&a, &b, &a].into_iter().enumerate() {
            f.record(
                m,
                Mat4::from_translation(Vec3::X * i as f32),
                Mat4::IDENTITY,
                true,
                None,
            );
        }
        f.commands.push(Command { kind: 0, index: 0 });
        f.record(&a, Mat4::IDENTITY, Mat4::IDENTITY, true, None);
        f.finalize();
        assert_eq!(f.commands.len(), 4);
        assert_eq!(f.batches[0].count, 2);
        assert_eq!(
            f.instances.iter().map(|i| i[12]).collect::<Vec<_>>(),
            [0., 2., 1., 0.]
        );
    }
}

#[cfg(test)]
mod journey400_284 {
    use super::*;
    use crate::{Vec3, mesh::Mesh3d};

    fn mesh() -> ResidentMesh {
        ResidentMesh::try_new(&Mesh3d::cuboid(Vec3::ONE, crate::WHITE).mesh).unwrap()
    }

    #[derive(Debug, PartialEq, Eq)]
    struct BatchBits {
        integers: [u32; 8],
        floats: [[u32; 16]; 3],
    }
    fn batches(values: &[RetainedBatch]) -> Vec<BatchBits> {
        values
            .iter()
            .map(|b| BatchBits {
                integers: [
                    b.mesh,
                    b.first,
                    b.count,
                    b.depth,
                    b.material,
                    b.revision,
                    b.reserved[0],
                    b.reserved[1],
                ],
                floats: [
                    b.camera.map(f32::to_bits),
                    b.light.map(f32::to_bits),
                    b.params.map(f32::to_bits),
                ],
            })
            .collect()
    }
    #[derive(Debug, PartialEq, Eq)]
    struct MeshBits {
        id: u32,
        strong: usize,
        vertices: Vec<[u32; 10]>,
        skin: Option<Vec<[u32; 8]>>,
    }
    #[derive(Debug, PartialEq, Eq)]
    struct FrameBits {
        commands: Vec<[u32; 2]>,
        batches: Vec<BatchBits>,
        instances: Vec<[u32; 28]>,
        scratch_batches: Vec<BatchBits>,
        scratch_instances: Vec<[u32; 28]>,
        skin_matrices: Vec<[u32; 16]>,
        skin_normals: Vec<[u32; 9]>,
        effects: Vec<crate::effects::EffectBatch>,
        sprite_batches: Vec<crate::effects::SpriteBatch>,
        sprites: Vec<[u32; 16]>,
        meshes: Vec<MeshBits>,
        materials: Vec<(u32, usize)>,
        // Length, capacity and allocation address of every frame Vec.
        storage: Vec<(usize, usize, usize)>,
    }
    fn storage<T>(values: &Vec<T>) -> (usize, usize, usize) {
        (values.len(), values.capacity(), values.as_ptr() as usize)
    }
    fn snapshot() -> FrameBits {
        crate::read(|s| {
            let f = &s.retained;
            FrameBits {
                commands: f.commands.iter().map(|c| [c.kind, c.index]).collect(),
                batches: batches(&f.batches),
                instances: f.instances.iter().map(|v| v.map(f32::to_bits)).collect(),
                scratch_batches: batches(&f.scratch_batches),
                scratch_instances: f
                    .scratch_instances
                    .iter()
                    .map(|v| v.map(f32::to_bits))
                    .collect(),
                skin_matrices: f
                    .skin_matrices
                    .iter()
                    .map(|v| v.map(f32::to_bits))
                    .collect(),
                skin_normals: f.skin_normals.iter().map(|v| v.map(f32::to_bits)).collect(),
                effects: f.effects.clone(),
                sprite_batches: f.sprite_batches.clone(),
                sprites: f.sprites.iter().map(|v| v.map(f32::to_bits)).collect(),
                meshes: f
                    .keep_alive
                    .iter()
                    .map(|m| MeshBits {
                        id: m.id(),
                        strong: Rc::strong_count(&m.0),
                        vertices: m.0.vertices.iter().map(|v| v.map(f32::to_bits)).collect(),
                        skin: m
                            .0
                            .skin
                            .as_ref()
                            .map(|skin| skin.iter().map(|v| v.map(f32::to_bits)).collect()),
                    })
                    .collect(),
                materials: f
                    .keep_materials
                    .iter()
                    .map(|m| (m.id(), Rc::strong_count(&m.0)))
                    .collect(),
                storage: vec![
                    storage(&f.commands),
                    storage(&f.batches),
                    storage(&f.instances),
                    storage(&f.scratch_batches),
                    storage(&f.scratch_instances),
                    storage(&f.skin_matrices),
                    storage(&f.skin_normals),
                    storage(&f.effects),
                    storage(&f.sprite_batches),
                    storage(&f.sprites),
                    storage(&f.keep_alive),
                    storage(&f.keep_materials),
                ],
            }
        })
    }
    fn assert_rejected(input: &[(&ResidentMesh, Mat4)], index: usize, source: DrawError) {
        let before = snapshot();
        let telemetry = telemetry_snapshot();
        let visibility = crate::visibility_stats();
        let result = draw_residents_checked(input);
        assert_eq!(
            result,
            InstanceGroupReport {
                rejected: input.len(),
                error: Some(InstanceGroupError { index, source }),
                ..Default::default()
            }
        );
        assert_eq!(snapshot(), before);
        assert_eq!(crate::visibility_stats(), visibility);
        let after = telemetry_snapshot();
        assert_eq!(
            RetainedTelemetry {
                rejections: telemetry.rejections,
                ..after
            },
            telemetry
        );
        let mut expected = telemetry.rejections;
        let counter = if matches!(source, DrawError::InstanceCapacity { .. }) {
            &mut expected.instance_capacity
        } else {
            &mut expected.draw_validation
        };
        *counter = counter.saturating_add(1);
        assert_eq!(after.rejections, expected);
    }

    #[test]
    fn group_uses_existing_adjacent_batches_and_immediate_order_barriers() {
        let a = mesh();
        let b = mesh();
        assert_eq!(draw_resident_checked(&a, Mat4::IDENTITY), Ok(true));
        crate::draw_triangle(
            crate::Vec2::ZERO,
            crate::Vec2::X,
            crate::Vec2::Y,
            crate::WHITE,
        );
        let model = Mat4::from_scale(Vec3::new(-2., 4., 8.));
        let report = draw_residents_checked(&[
            (&a, model),
            (&a, Mat4::IDENTITY),
            (&b, Mat4::from_translation(Vec3::Y)),
            (&a, Mat4::IDENTITY),
        ]);
        assert_eq!(
            report,
            InstanceGroupReport {
                accepted: 4,
                visible: 4,
                ..Default::default()
            }
        );
        crate::read(|s| {
            assert_eq!(
                s.retained
                    .commands
                    .iter()
                    .map(|c| c.kind)
                    .collect::<Vec<_>>(),
                [1, 0, 1, 1, 1]
            );
            assert_eq!(
                s.retained
                    .batches
                    .iter()
                    .map(|b| (b.first, b.count))
                    .collect::<Vec<_>>(),
                [(0, 1), (1, 2), (3, 1), (4, 1)]
            );
            let packed = &s.retained.instances[1];
            assert_eq!(&packed[..16], &model.to_cols_array());
            assert_eq!(
                &packed[16..],
                &[-0.5, 0., 0., 0., 0., 0.25, 0., 0., 0., 0., 0.125, 0.]
            );
            assert_eq!(std::mem::size_of::<RetainedBatch>(), 224);
            assert_eq!(std::mem::size_of_val(packed), 112);
        });
        crate::edit(|s| s.retained.finalize());
        crate::read(|s| {
            assert_eq!(
                s.retained
                    .commands
                    .iter()
                    .map(|c| c.kind)
                    .collect::<Vec<_>>(),
                [1, 0, 1, 1, 1]
            )
        });
    }

    #[test]
    fn group_late_errors_preserve_all_buffers_bits_lifetimes_and_visibility() {
        let mesh = mesh();
        let source = Mesh3d::cuboid(Vec3::ONE, crate::WHITE).mesh;
        let skin =
            ResidentMesh::with_skin(&source, Some((&vec![[0.; 8]; source.vertices.len()], 1)))
                .unwrap();
        let palette = crate::skinning::SkinPalette::new(&[Mat4::IDENTITY.to_cols_array()]).unwrap();
        let material =
            crate::material::ShaderMaterial::new(Default::default(), Default::default()).unwrap();
        crate::edit(|s| {
            assert!(s.retained.record_style(
                &skin,
                Mat4::IDENTITY,
                s.matrix,
                false,
                DrawStyle {
                    material: Some(&material),
                    skin: Some(&palette),
                    ..Default::default()
                }
            ));
            s.retained.finalize(); // Leave genuine prior content in scratch buffers.
            assert!(s.retained.record(
                &mesh,
                Mat4::from_scale(Vec3::new(-2., 4., 8.)),
                s.matrix,
                false,
                None
            ));
            // Unrelated buffers and NaN/signed-zero payloads must also survive.
            s.retained.effects.push(crate::effects::EffectBatch {
                start: 0,
                count: 3,
                flags: 5,
            });
            s.retained.sprite_batches.push(crate::effects::SpriteBatch {
                first: 0,
                count: 1,
                flags: 5,
                triangles: 2,
            });
            let mut sprite = [-0.; 16];
            sprite[0] = f32::from_bits(0x7fc0_0284);
            s.retained.sprites.push(sprite);
            s.retained.batches[0].params[0] = f32::from_bits(0x7fc0_0184);
        });
        for (model, error) in [
            (Mat4::ZERO, DrawError::NonAffineModel { column: 3 }),
            (Mat4::from_scale(Vec3::ZERO), DrawError::NonInvertibleModel),
        ] {
            assert_rejected(
                &[
                    (&mesh, Mat4::IDENTITY),
                    (&mesh, Mat4::IDENTITY),
                    (&mesh, model),
                ],
                2,
                error,
            );
        }
        let mut nonfinite = Mat4::IDENTITY;
        nonfinite[2][1] = f32::NAN;
        assert_rejected(
            &[
                (&mesh, Mat4::IDENTITY),
                (&mesh, nonfinite),
                (&mesh, Mat4::ZERO),
            ],
            1,
            DrawError::NonFiniteModel { column: 2, row: 1 },
        );
        assert_rejected(
            &[(&mesh, Mat4::IDENTITY), (&skin, Mat4::IDENTITY)],
            1,
            DrawError::SkinPaletteMismatch {
                expected: 1,
                actual: 0,
            },
        );
        crate::edit(|s| s.matrix[3][2] = f32::INFINITY);
        assert_rejected(
            &[(&mesh, Mat4::IDENTITY)],
            0,
            DrawError::NonFiniteCamera { column: 3, row: 2 },
        );
    }

    #[test]
    fn group_capacity_is_aggregate_atomic_and_recovers_after_clear() {
        let mesh = mesh();
        let other = mesh.clone();
        let input = vec![(&mesh, Mat4::IDENTITY); MAX_INSTANCES - 1];
        let result = draw_residents_checked(&input);
        assert_eq!(result.accepted, MAX_INSTANCES - 1);
        assert!(result.error.is_none());
        assert_eq!(telemetry_snapshot().draws, 1);
        // The first pair could merge into the previous batch; it must not do so.
        assert_rejected(
            &[(&mesh, Mat4::IDENTITY), (&other, Mat4::IDENTITY)],
            1,
            DrawError::InstanceCapacity {
                limit: MAX_INSTANCES,
            },
        );
        // Validation wins even after the capacity preflight observed overflow.
        assert_rejected(
            &[
                (&mesh, Mat4::IDENTITY),
                (&other, Mat4::IDENTITY),
                (&mesh, Mat4::ZERO),
            ],
            2,
            DrawError::NonAffineModel { column: 3 },
        );
        assert_eq!(draw_residents_checked(&input[..1]).accepted, 1);
        assert_rejected(
            &input[..1],
            0,
            DrawError::InstanceCapacity {
                limit: MAX_INSTANCES,
            },
        );
        let full = telemetry_snapshot();
        assert_eq!(full.instances, MAX_INSTANCES);
        crate::clear_background(crate::BLACK);
        assert_eq!(telemetry_snapshot().buffers, full.buffers);
        assert_eq!(telemetry_snapshot().rejections, full.rejections);
        assert_eq!(draw_residents_checked(&input[..2]).accepted, 2);
        assert_eq!(reset_telemetry(), full.rejections);
        assert_eq!(
            telemetry_snapshot().rejections,
            RetainedRejections::default()
        );
        crate::clear_background(crate::BLACK);
        let oversized = vec![(&mesh, Mat4::IDENTITY); MAX_INSTANCES + 1];
        assert_rejected(
            &oversized,
            MAX_INSTANCES,
            DrawError::InstanceCapacity {
                limit: MAX_INSTANCES,
            },
        );
    }

    #[test]
    fn group_empty_and_saturated_activity_counters_keep_exact_entry_counts() {
        let mesh = mesh();
        REJECTIONS.with(|r| {
            r.set(RetainedRejections {
                draw_validation: u64::MAX,
                ..Default::default()
            })
        });
        assert_rejected(
            &[
                (&mesh, Mat4::IDENTITY),
                (&mesh, Mat4::ZERO),
                (&mesh, Mat4::ZERO),
            ],
            1,
            DrawError::NonAffineModel { column: 3 },
        );
        crate::edit(|s| s.matrix = Mat4::ZERO * f32::NAN);
        let before = snapshot();
        let telemetry = telemetry_snapshot();
        assert_eq!(draw_residents_checked(&[]), InstanceGroupReport::default());
        assert_eq!(snapshot(), before);
        assert_eq!(telemetry_snapshot(), telemetry);
        let error = InstanceGroupError {
            index: 7,
            source: DrawError::NonInvertibleModel,
        };
        assert!(error.to_string().contains("entry[7]"));
        assert_eq!(
            std::error::Error::source(&error).unwrap().to_string(),
            error.source.to_string()
        );
    }

    #[test]
    fn group_preserves_resident_snapshots_duplicates_and_recorded_lifetimes() {
        let mut source = Mesh3d::cuboid(Vec3::ONE, crate::WHITE).mesh;
        let resident = ResidentMesh::try_new(&source).unwrap();
        let clone = resident.clone();
        let id = resident.id();
        let expected = resident.0.vertices.clone();
        source.vertices[0].position.x = f32::NAN;
        source.indices.clear();
        drop(resident);
        let dead = mesh();
        let dead_id = dead.id();
        drop(dead);
        assert!(geometry(dead_id).is_none());
        let before = telemetry_snapshot();
        let result = draw_residents_checked(&[
            (&clone, Mat4::IDENTITY),
            (&clone, Mat4::from_translation(Vec3::X)),
        ]);
        assert_eq!(
            (
                result.accepted,
                result.rejected,
                result.culled,
                result.visible
            ),
            (2, 0, 0, 2)
        );
        assert_eq!(telemetry_snapshot().live_meshes, before.live_meshes);
        assert_eq!(telemetry_snapshot().mesh_bytes, before.mesh_bytes);
        crate::read(|s| {
            assert_eq!(s.retained.keep_alive.len(), 1);
            assert_eq!(s.retained.batches[0].count, 2);
            assert_eq!(s.retained.instances[1][12], 1.);
            assert_eq!(s.retained.keep_alive[0].0.vertices, expected);
        });
        let rejected = mesh();
        let rejected_id = rejected.id();
        assert_rejected(
            &[(&rejected, Mat4::IDENTITY), (&clone, Mat4::ZERO)],
            1,
            DrawError::NonAffineModel { column: 3 },
        );
        drop(rejected);
        assert!(geometry(rejected_id).is_none());
        drop(clone);
        assert_eq!(geometry(id).unwrap().vertices, expected);
        crate::clear_background(crate::BLACK);
        assert!(geometry(id).is_none());
        assert!(mesh().id() > rejected_id);
    }

    #[test]
    fn group_retains_legacy_full_inverse_validation_boundary() {
        let resident = mesh();
        let mut model = Mat4::from_scale(Vec3::new(1e-20, 1., 1.));
        model.w_axis.x = f32::MAX;
        assert!(ferravex_math::affine::PreparedAffine3D::try_new(model).is_ok());
        assert_eq!(
            validate_transform(model),
            Err(DrawError::NonInvertibleModel)
        );
        assert_rejected(
            &[(&resident, Mat4::IDENTITY), (&resident, model)],
            1,
            DrawError::NonInvertibleModel,
        );
    }

    #[test]
    fn group_matches_single_draw_recording_culling_shadows_materials_and_finalization() {
        use crate::material::{BlendMode, MaterialOptions, ShaderMaterial};
        let opaque = mesh();
        let transparent = ResidentMesh::try_new(
            &Mesh3d::cuboid(Vec3::ONE, crate::Color::new(1., 1., 1., 0.5)).mesh,
        )
        .unwrap();
        let mut reflected_shear = Mat4::from_scale(Vec3::new(-0.5, 0.25, 0.5));
        reflected_shear.y_axis.x = 0.125;
        for depth in [false, true] {
            for mesh in [&opaque, &transparent] {
                for blend in [BlendMode::Opaque, BlendMode::Alpha, BlendMode::Additive] {
                    for casts_shadows in [false, true] {
                        for bounds_padding in [0., 4.] {
                            for max_distance in [None, Some(1.)] {
                                let material = ShaderMaterial::new(
                                    Default::default(),
                                    MaterialOptions {
                                        blend,
                                        bounds_padding,
                                        casts_shadows,
                                        receives_shadows: false,
                                    },
                                )
                                .unwrap();
                                crate::edit(|s| {
                                    s.retained.clear();
                                    s.visibility_stats = Default::default();
                                    s.matrix = Mat4::IDENTITY;
                                    s.depth = depth;
                                    s.frustum = depth.then(|| {
                                        ferravex_graphics::Frustum::from_matrix(
                                            Mat4::IDENTITY,
                                            ferravex_graphics::ClipDepth::NegativeOneToOne,
                                        )
                                        .unwrap()
                                    });
                                    s.shadow_frustum = ferravex_graphics::Frustum::from_matrix(
                                        Mat4::from_scale(Vec3::splat(0.1)),
                                        ferravex_graphics::ClipDepth::NegativeOneToOne,
                                    );
                                    s.eye = Some(Vec3::ZERO);
                                    s.visibility = crate::visibility::VisibilitySettings {
                                        frustum_culling: true,
                                        max_distance,
                                    };
                                    s.shader = Some(material.clone());
                                    s.shader_params = [0.25; 16];
                                    s.lighting = Some(Default::default());
                                });
                                let input = [
                                    (mesh, reflected_shear),
                                    (mesh, Mat4::IDENTITY),
                                    (&opaque, Mat4::from_translation(Vec3::X * 4.)),
                                    (mesh, Mat4::from_translation(Vec3::X * 100.)),
                                ];
                                let mut visible = 0;
                                for &(m, model) in &input {
                                    visible +=
                                        usize::from(draw_resident_checked(m, model).unwrap());
                                }
                                let expected = snapshot();
                                let stats = crate::visibility_stats();
                                let accepted = expected.instances.len();
                                crate::edit(|s| {
                                    s.retained.finalize();
                                });
                                let finalized = snapshot();
                                crate::edit(|s| {
                                    s.retained.clear();
                                    s.visibility_stats = Default::default();
                                });
                                let group = draw_residents_checked(&input);
                                assert_eq!(
                                    group,
                                    InstanceGroupReport {
                                        accepted,
                                        visible,
                                        culled: input.len() - accepted,
                                        ..Default::default()
                                    }
                                );
                                assert_eq!(crate::visibility_stats(), stats);
                                let actual = snapshot();
                                assert_eq!(actual.commands, expected.commands);
                                assert_eq!(actual.batches, expected.batches);
                                assert_eq!(actual.instances, expected.instances);
                                assert_eq!(actual.meshes, expected.meshes);
                                assert_eq!(actual.materials, expected.materials);
                                crate::edit(|s| {
                                    s.retained.finalize();
                                });
                                let actual = snapshot();
                                assert_eq!(actual.commands, finalized.commands);
                                assert_eq!(actual.batches, finalized.batches);
                                assert_eq!(actual.instances, finalized.instances);
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn group_capacity_counts_visible_and_shadow_instances_but_not_complete_culls() {
        let mesh = mesh();
        let input = vec![(&mesh, Mat4::IDENTITY); MAX_INSTANCES - 1];
        assert_eq!(draw_residents_checked(&input).accepted, input.len());
        crate::edit(|s| {
            s.depth = true;
            s.frustum = ferravex_graphics::Frustum::from_matrix(
                Mat4::IDENTITY,
                ferravex_graphics::ClipDepth::NegativeOneToOne,
            );
            s.shadow_frustum = ferravex_graphics::Frustum::from_matrix(
                Mat4::from_scale(Vec3::splat(0.1)),
                ferravex_graphics::ClipDepth::NegativeOneToOne,
            );
        });
        let outside = Mat4::from_translation(Vec3::X * 100.);
        let shadow = Mat4::from_translation(Vec3::X * 4.);
        assert_rejected(
            &[(&mesh, outside), (&mesh, shadow), (&mesh, Mat4::IDENTITY)],
            2,
            DrawError::InstanceCapacity {
                limit: MAX_INSTANCES,
            },
        );
        let report = draw_residents_checked(&[(&mesh, outside), (&mesh, shadow)]);
        assert_eq!(
            report,
            InstanceGroupReport {
                accepted: 1,
                culled: 1,
                ..Default::default()
            }
        );
        crate::read(|s| assert_eq!(s.retained.batches.last().unwrap().depth & 32, 32));
        let full = telemetry_snapshot();
        assert_eq!(full.instances, MAX_INSTANCES);
        // Even a full frame may process a group needing no instance storage.
        assert_eq!(
            draw_residents_checked(&[(&mesh, outside)]),
            InstanceGroupReport {
                culled: 1,
                ..Default::default()
            }
        );
        assert_eq!(telemetry_snapshot(), full);
        assert_rejected(
            &[(&mesh, outside), (&mesh, Mat4::IDENTITY)],
            1,
            DrawError::InstanceCapacity {
                limit: MAX_INSTANCES,
            },
        );
    }
}
