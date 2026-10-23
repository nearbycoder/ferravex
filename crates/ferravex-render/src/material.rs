//! Reusable shader handles. GPU compilation is host-owned and replacements are transactional.
//!
//! [`ShaderMaterial::new_with_layout`] opts into checked, material-bound parameters.
//! The layout describes how authored hooks read the existing `u_params[4]` ABI;
//! it is not GLSL reflection. Source reloads must preserve that contract.
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};
pub const MAX_SOURCE_BYTES: usize = 65536;
pub const MAX_ERROR_BYTES: usize = 4096;
const MAX_MATERIALS: usize = 128;

pub const DEFAULT_VERTEX_HOOK: &str = r#"
vec3 ferravex_vertex(vec3 position){return position;}
vec3 ferravex_normal(vec3 normal){return normal;}
"#;
pub const DEFAULT_FRAGMENT_HOOK: &str = r#"
vec4 ferravex_fragment(vec4 color,vec3 world,vec3 normal){return color;}
"#;
pub const FRAGMENT_BODY: &str = r#"
varying vec4 v_color;
varying vec3 v_world;
varying vec3 v_normal;
uniform vec4 u_params[4];
uniform vec4 u_light[4];
uniform vec4 u_points[16];
uniform int u_point_count;
uniform mat4 u_shadow_matrix;
uniform sampler2D u_shadow_texture;
uniform vec4 u_shadow_options;
uniform vec4 u_shadow_extra;
vec4 ferravex_fragment(vec4 color,vec3 world,vec3 normal);
float ferravex_visibility(vec3 world,vec3 normal){
 if(u_shadow_options.w<0.5)return 1.0;
 vec4 clip=u_shadow_matrix*vec4(world+normal*u_shadow_extra.x,1.0);
 vec3 p=clip.xyz/clip.w*0.5+0.5;
 if(any(lessThan(p,vec3(0.0)))||any(greaterThan(p,vec3(1.0))))return 1.0;
 float lit=0.0;float samples=0.0;
 for(int y=-1;y<=1;y++)for(int x=-1;x<=1;x++){
  if(abs(float(x))>u_shadow_options.z||abs(float(y))>u_shadow_options.z)continue;
  float depth=texture(u_shadow_texture,p.xy+vec2(float(x),float(y))*u_shadow_extra.y).r;
  lit+=(p.z-u_shadow_options.x<=depth)?1.0:0.0;samples+=1.0;
 }
 return mix(1.0,lit/max(samples,1.0),u_shadow_options.y);
}
void main(){
 vec3 normal=v_normal;float n=length(normal);normal=n>0.0?normal/n:vec3(0.0);
 vec4 color=v_color;
 if(u_light[0].w>0.5){
  float ambient=u_light[1].w;
  float diffuse=clamp(dot(normal,u_light[0].xyz),0.0,1.0);
  vec3 light=vec3(ambient+(1.0-ambient)*diffuse*ferravex_visibility(v_world,normal));
  for(int i=0;i<8;i++){
   if(i>=u_point_count)break;
   vec4 point=u_points[i*2];vec4 tint=u_points[i*2+1];vec3 delta=point.xyz-v_world;float distance=length(delta);
   float falloff=max(0.0,1.0-distance/point.w);vec3 direction=distance>0.0001?delta/distance:normal;
   light+=tint.rgb*tint.w*falloff*falloff*max(dot(normal,direction),0.0)*(1.0-ambient);
  }
  color.rgb*=light;
  float fog=clamp((distance(v_world,u_light[1].xyz)-u_light[3].x)/max(u_light[3].y-u_light[3].x,0.001),0.0,1.0);
  color.rgb=mix(color.rgb,u_light[2].rgb,fog);
 }
 color=ferravex_fragment(color,v_world,normal);
 float look=u_light[3].z;
 if(look>2.5){float y=dot(color.rgb,vec3(0.2126,0.7152,0.0722));color.rgb=y*vec3(0.82,0.94,1.0);}
 else if(look>1.5)color.rgb=min(vec3(1.0),vec3(color.r*0.87,color.g*1.02,color.b*1.08+color.r*0.015));
 else if(look>0.5)color.rgb=min(vec3(1.0),vec3(color.r*1.06+color.g*0.02,color.g*0.98,color.b*0.88));
 gl_FragColor=color;
}
"#;

#[derive(Clone, Debug)]
pub struct ShaderSource {
    pub vertex: String,
    pub fragment: String,
}
impl Default for ShaderSource {
    fn default() -> Self {
        Self {
            vertex: DEFAULT_VERTEX_HOOK.into(),
            fragment: DEFAULT_FRAGMENT_HOOK.into(),
        }
    }
}
impl ShaderSource {
    pub fn fragment(fragment: impl Into<String>) -> Self {
        Self {
            fragment: fragment.into(),
            ..Default::default()
        }
    }
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.vertex.len() + self.fragment.len() > MAX_SOURCE_BYTES
            || self.vertex.contains('\0')
            || self.fragment.contains('\0')
        {
            return Err("shader sources exceed 64 KiB or contain NUL");
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BlendMode {
    /// Promise that every output pixel is opaque; enables depth writes and opaque regrouping.
    Opaque,
    #[default]
    Alpha,
    Additive,
}
#[derive(Clone, Copy, Debug)]
pub struct MaterialOptions {
    pub blend: BlendMode,
    /// Authored local-space bound expansion for vertex displacement.
    pub bounds_padding: f32,
    pub casts_shadows: bool,
    pub receives_shadows: bool,
}
impl Default for MaterialOptions {
    fn default() -> Self {
        Self {
            blend: BlendMode::Alpha,
            bounds_padding: 0.,
            casts_shadows: true,
            receives_shadows: true,
        }
    }
}

/// The components a shader hook reads from one `u_params` slot.
///
/// Every slot is still uploaded as a `vec4`. Smaller values occupy `.x`, `.xy`
/// or `.xyz`, with the remaining components zeroed. See
/// [`ShaderMaterial::new_with_layout`] for a checked usage example.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParameterType {
    Float,
    Vec2,
    Vec3,
    Vec4,
}

impl std::fmt::Display for ParameterType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Float => "float",
            Self::Vec2 => "vec2",
            Self::Vec3 => "vec3",
            Self::Vec4 => "vec4",
        })
    }
}

/// A typed update to one slot; [`MaterialParameters::set`] checks its type and finiteness.
///
/// See [`MaterialParameters::update`] for a transactional multi-slot example.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ParameterValue {
    Float(f32),
    Vec2([f32; 2]),
    Vec3([f32; 3]),
    Vec4([f32; 4]),
}

impl ParameterValue {
    fn packed(self) -> (ParameterType, [f32; 4]) {
        match self {
            Self::Float(x) => (ParameterType::Float, [x, 0., 0., 0.]),
            Self::Vec2([x, y]) => (ParameterType::Vec2, [x, y, 0., 0.]),
            Self::Vec3([x, y, z]) => (ParameterType::Vec3, [x, y, z, 0.]),
            Self::Vec4(value) => (ParameterType::Vec4, value),
        }
    }
}

/// A rejected update or binding. Failures leave the parameter block and active binding intact.
///
/// Slots and components use zero-based indices. Errors implement [`std::error::Error`]
/// and their display text identifies the affected `u_params` slot or material IDs.
/// See [`MaterialParameters::set`] and [`ShaderMaterial::begin_checked`] for examples.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParameterError {
    SlotOutOfRange {
        slot: usize,
    },
    UndeclaredSlot {
        slot: usize,
    },
    TypeMismatch {
        slot: usize,
        expected: ParameterType,
        provided: ParameterType,
    },
    NonFinite {
        slot: usize,
        component: usize,
    },
    MaterialMismatch {
        expected: u32,
        provided: u32,
    },
}

impl std::fmt::Display for ParameterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SlotOutOfRange { slot } => {
                write!(f, "u_params[{slot}] is out of range; valid slots are 0..4")
            }
            Self::UndeclaredSlot { slot } => {
                write!(
                    f,
                    "u_params[{slot}] is not declared in this material's parameter layout"
                )
            }
            Self::TypeMismatch {
                slot,
                expected,
                provided,
            } => {
                write!(
                    f,
                    "u_params[{slot}] expects {expected} in this material's parameter layout; received {provided}"
                )
            }
            Self::NonFinite { slot, component } => {
                write!(f, "u_params[{slot}][{component}] must be finite")
            }
            Self::MaterialMismatch { expected, provided } => {
                write!(
                    f,
                    "parameter block belongs to material {provided}, not material {expected}; create it with the receiving material's parameters()"
                )
            }
        }
    }
}

impl std::error::Error for ParameterError {}

/// A fixed-size, finite parameter block tied to one material's declared layout.
///
/// Create one with [`ShaderMaterial::parameters`]. Cloning copies the values and
/// retains the material handle; updates never allocate or change renderer state.
/// Bind explicitly with [`ShaderMaterial::begin_checked`].
#[derive(Clone)]
pub struct MaterialParameters {
    material: ShaderMaterial,
    rows: [[f32; 4]; 4],
}

impl std::fmt::Debug for MaterialParameters {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MaterialParameters")
            .field("material_id", &self.material.id())
            .field("rows", &self.rows)
            .finish()
    }
}

impl MaterialParameters {
    /// Replace one slot, rejecting invalid indices, undeclared slots, wrong types
    /// and nonfinite components before changing any values.
    ///
    /// ```
    /// use ferravex_render::material::{ParameterError, ParameterValue, ShaderMaterial};
    /// let material = ShaderMaterial::new(Default::default(), Default::default())?;
    /// let mut params = material.parameters(); // Legacy materials declare four vec4 slots.
    /// params.set(3, ParameterValue::Vec4([1., 2., 3., 4.]))?;
    /// assert_eq!(params.set(4, ParameterValue::Float(1.)),
    ///            Err(ParameterError::SlotOutOfRange { slot: 4 }));
    /// assert_eq!(params.as_rows()[3], [1., 2., 3., 4.]);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn set(&mut self, slot: usize, value: ParameterValue) -> Result<(), ParameterError> {
        self.update(&[(slot, value)])
    }

    /// Apply all updates or leave the entire block unchanged on the first error.
    /// Repeated slots are validated in order; the last valid value wins on success.
    /// An empty update is a no-op. Storage remains bounded to four slots.
    ///
    /// ```
    /// use ferravex_render::material::{ParameterError, ParameterValue, ShaderMaterial};
    /// let material = ShaderMaterial::new(Default::default(), Default::default())?;
    /// let mut params = material.parameters();
    /// let before = *params.as_rows();
    /// assert_eq!(params.update(&[
    ///     (0, ParameterValue::Vec4([1.; 4])),
    ///     (1, ParameterValue::Vec4([0., f32::NAN, 0., 0.])),
    /// ]), Err(ParameterError::NonFinite { slot: 1, component: 1 }));
    /// assert_eq!(*params.as_rows(), before);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn update(&mut self, updates: &[(usize, ParameterValue)]) -> Result<(), ParameterError> {
        let mut next = self.rows;
        for &(slot, value) in updates {
            let expected = self
                .material
                .0
                .parameter_layout
                .get(slot)
                .ok_or(ParameterError::SlotOutOfRange { slot })?
                .ok_or(ParameterError::UndeclaredSlot { slot })?;
            let (provided, row) = value.packed();
            if expected != provided {
                return Err(ParameterError::TypeMismatch {
                    slot,
                    expected,
                    provided,
                });
            }
            if let Some(component) = row.iter().position(|v| !v.is_finite()) {
                return Err(ParameterError::NonFinite { slot, component });
            }
            next[slot] = row;
        }
        self.rows = next;
        Ok(())
    }

    /// Read the unchanged four-`vec4` ABI, including zero padding and unused slots.
    /// See [`Self::set`] for an example. No mutable access is exposed.
    pub fn as_rows(&self) -> &[[f32; 4]; 4] {
        &self.rows
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompileStatus {
    Pending,
    Ready,
    Failed(String),
}
#[derive(Clone, Debug)]
pub struct MaterialStatus {
    pub requested_revision: u32,
    pub last_valid_revision: Option<u32>,
    pub status: CompileStatus,
}
pub(crate) struct Data {
    pub id: u32,
    pub options: MaterialOptions,
    pub revision: Cell<u32>,
    pub source: RefCell<ShaderSource>,
    pub last_valid_source: RefCell<Option<ShaderSource>>,
    status: RefCell<MaterialStatus>,
    parameter_layout: [Option<ParameterType>; 4],
}
#[derive(Clone)]
pub struct ShaderMaterial(pub(crate) Rc<Data>);
#[derive(Default)]
struct Registry {
    next: u32,
    materials: Vec<Weak<Data>>,
}
thread_local! {static REGISTRY:RefCell<Registry>=RefCell::new(Registry::default());}
impl ShaderMaterial {
    /// Validates storage/metadata. Actual shader compilation occurs when presented by a GPU host.
    /// Checked parameter blocks from this constructor use four [`ParameterType::Vec4`] slots.
    pub fn new(source: ShaderSource, options: MaterialOptions) -> Result<Self, &'static str> {
        Self::new_with_layout(source, options, [Some(ParameterType::Vec4); 4])
    }

    /// Declare how shader hooks consume each existing `u_params` slot.
    /// `None` reserves an unused, zero-filled slot in checked parameter blocks.
    ///
    /// This is an author-declared contract, not shader reflection or compilation.
    /// It remains fixed across [`Self::reload`]; create a new material when changing
    /// the contract. The legacy [`Self::begin`] accepts raw rows without checking types.
    ///
    /// ```
    /// use ferravex_render::material::{ParameterType, ParameterValue, ShaderMaterial, ShaderSource};
    /// let material = ShaderMaterial::new_with_layout(
    ///     ShaderSource::fragment(r#"
    ///         vec4 ferravex_fragment(vec4 color, vec3 world, vec3 normal) {
    ///             return vec4(color.rgb * u_params[1].xyz, color.a * u_params[0].x);
    ///         }
    ///     "#),
    ///     Default::default(),
    ///     [Some(ParameterType::Float), Some(ParameterType::Vec3), None, None],
    /// )?;
    /// assert_eq!(material.parameter_layout()[0], Some(ParameterType::Float));
    /// let mut params = material.parameters();
    /// params.set(0, ParameterValue::Float(0.5))?;
    /// params.set(1, ParameterValue::Vec3([1., 0.8, 0.6]))?;
    /// material.begin_checked(&params)?;
    /// // Retained mesh draws here capture these parameter values.
    /// material.end();
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn new_with_layout(
        source: ShaderSource,
        options: MaterialOptions,
        parameter_layout: [Option<ParameterType>; 4],
    ) -> Result<Self, &'static str> {
        source.validate()?;
        if !options.bounds_padding.is_finite() || options.bounds_padding < 0. {
            return Err("invalid material bound padding");
        }
        REGISTRY.with(|r| {
            let mut r = r.borrow_mut();
            r.materials.retain(|w| w.strong_count() > 0);
            if r.materials.len() >= MAX_MATERIALS {
                return Err("material capacity exceeded");
            }
            r.next = r.next.checked_add(1).ok_or("material identity exhausted")?;
            let data = Rc::new(Data {
                id: r.next,
                options,
                revision: Cell::new(1),
                source: RefCell::new(source),
                last_valid_source: RefCell::new(None),
                status: RefCell::new(MaterialStatus {
                    requested_revision: 1,
                    last_valid_revision: None,
                    status: CompileStatus::Pending,
                }),
                parameter_layout,
            });
            r.materials.push(Rc::downgrade(&data));
            Ok(Self(data))
        })
    }
    pub fn id(&self) -> u32 {
        self.0.id
    }
    pub fn status(&self) -> MaterialStatus {
        self.0.status.borrow().clone()
    }

    /// Return the immutable authored slot contract; see [`Self::new_with_layout`].
    pub fn parameter_layout(&self) -> [Option<ParameterType>; 4] {
        self.0.parameter_layout
    }

    /// Create a zero-filled, independent parameter block bound to this material.
    /// Clones of this shader handle accept the same block; another material cannot.
    /// The block retains this handle and its registry slot until dropped.
    /// See [`Self::new_with_layout`] for an example.
    pub fn parameters(&self) -> MaterialParameters {
        MaterialParameters {
            material: self.clone(),
            rows: [[0.; 4]; 4],
        }
    }

    /// Bind checked values, preserving the active shader and values on a material mismatch.
    /// Does not require or imply GPU compilation; existing pending/fallback behavior applies.
    /// Updating the block after binding does not change the active values until bound again.
    ///
    /// ```
    /// use ferravex_render::material::{ParameterError, ShaderMaterial};
    /// let a = ShaderMaterial::new(Default::default(), Default::default())?;
    /// let b = ShaderMaterial::new(Default::default(), Default::default())?;
    /// let params = a.parameters();
    /// a.clone().begin_checked(&params)?;
    /// assert_eq!(b.begin_checked(&params), Err(ParameterError::MaterialMismatch {
    ///     expected: b.id(), provided: a.id(),
    /// }));
    /// a.end();
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn begin_checked(&self, params: &MaterialParameters) -> Result<(), ParameterError> {
        if !Rc::ptr_eq(&self.0, &params.material.0) {
            return Err(ParameterError::MaterialMismatch {
                expected: self.id(),
                provided: params.material.id(),
            });
        }
        crate::set_shader(Some(self.clone()), flatten_params(&params.rows));
        Ok(())
    }

    /// Queue a source replacement. Bad metadata leaves the request unchanged; GPU errors retain
    /// each host's last valid program. The latest request is applied at presentation time.
    /// Checked parameter layouts are immutable: authored replacements must keep the same contract.
    pub fn reload(&self, source: ShaderSource) -> Result<u32, &'static str> {
        source.validate()?;
        let revision = self
            .0
            .revision
            .get()
            .checked_add(1)
            .ok_or("material revision exhausted")?;
        *self.0.source.borrow_mut() = source;
        self.0.revision.set(revision);
        let mut status = self.0.status.borrow_mut();
        status.requested_revision = revision;
        status.status = CompileStatus::Pending;
        Ok(revision)
    }
    /// Bind raw ABI rows after a finite-value check, preserving the active binding on failure.
    /// This compatibility API does not enforce the layout; use [`Self::begin_checked`] for that.
    pub fn begin(&self, params: [[f32; 4]; 4]) -> Result<(), &'static str> {
        if params.iter().flatten().any(|v| !v.is_finite()) {
            return Err("material parameters must be finite");
        }
        crate::set_shader(Some(self.clone()), flatten_params(&params));
        Ok(())
    }
    pub fn end(&self) {
        crate::set_shader(None, [0.; 16]);
    }
    pub(crate) fn padded(
        &self,
        mut bounds: ferravex_graphics::Bounds,
    ) -> ferravex_graphics::Bounds {
        let padding = crate::Vec3::splat(self.0.options.bounds_padding);
        bounds.min -= padding;
        bounds.max += padding;
        bounds
    }
}

fn flatten_params(params: &[[f32; 4]; 4]) -> [f32; 16] {
    let mut flat = [0.; 16];
    for (i, row) in params.iter().enumerate() {
        flat[i * 4..i * 4 + 4].copy_from_slice(row);
    }
    flat
}

pub(crate) fn lookup(id: u32) -> Option<Rc<Data>> {
    REGISTRY.with(|r| {
        r.borrow()
            .materials
            .iter()
            .filter_map(Weak::upgrade)
            .find(|m| m.id == id)
    })
}
pub(crate) fn count() -> usize {
    REGISTRY.with(|r| {
        r.borrow()
            .materials
            .iter()
            .filter(|m| m.strong_count() > 0)
            .count()
    })
}
#[cfg(target_arch = "wasm32")]
fn id_at(index: usize) -> u32 {
    REGISTRY.with(|r| {
        r.borrow()
            .materials
            .iter()
            .filter_map(Weak::upgrade)
            .nth(index)
            .map_or(0, |m| m.id)
    })
}
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn for_each(mut f: impl FnMut(Rc<Data>)) {
    REGISTRY.with(|r| {
        for data in r.borrow().materials.iter().filter_map(Weak::upgrade) {
            f(data);
        }
    });
}
pub(crate) fn feedback(id: u32, revision: u32, result: Result<(), &str>) {
    let Some(data) = lookup(id) else {
        return;
    };
    if data.revision.get() != revision {
        return;
    }
    let mut status = data.status.borrow_mut();
    status.status = match result {
        Ok(()) => {
            status.last_valid_revision = Some(revision);
            *data.last_valid_source.borrow_mut() = Some(data.source.borrow().clone());
            CompileStatus::Ready
        }
        Err(message) => {
            let mut end = message.len().min(MAX_ERROR_BYTES);
            while !message.is_char_boundary(end) {
                end -= 1;
            }
            CompileStatus::Failed(message[..end].into())
        }
    };
}

#[cfg(not(target_arch = "wasm32"))]
type FileStamp = (std::time::SystemTime, u64);
#[cfg(not(target_arch = "wasm32"))]
type WatchStamp = (Option<FileStamp>, FileStamp);
#[cfg(not(target_arch = "wasm32"))]
pub struct MaterialFileWatcher {
    material: ShaderMaterial,
    vertex: Option<std::path::PathBuf>,
    fragment: std::path::PathBuf,
    last: Option<WatchStamp>,
    next: std::time::Instant,
}
#[cfg(not(target_arch = "wasm32"))]
impl MaterialFileWatcher {
    pub fn new(
        material: ShaderMaterial,
        vertex: Option<std::path::PathBuf>,
        fragment: std::path::PathBuf,
    ) -> Self {
        Self {
            material,
            vertex,
            fragment,
            last: None,
            next: std::time::Instant::now(),
        }
    }
    /// Poll at most twice per second. Reads are bounded; a read/validation error preserves source.
    pub fn poll(&mut self) -> Result<bool, String> {
        let now = std::time::Instant::now();
        if now < self.next {
            return Ok(false);
        }
        self.next = now + std::time::Duration::from_millis(500);
        let stamp = |p: &std::path::Path| -> Result<_, String> {
            let m = std::fs::metadata(p).map_err(|e| e.to_string())?;
            Ok((m.modified().map_err(|e| e.to_string())?, m.len()))
        };
        let current = (
            self.vertex.as_ref().map(|p| stamp(p)).transpose()?,
            stamp(&self.fragment)?,
        );
        if self.last.as_ref() == Some(&current) {
            return Ok(false);
        }
        let read = |p: &std::path::Path| -> Result<String, String> {
            use std::io::Read;
            let mut text = String::new();
            std::fs::File::open(p)
                .map_err(|e| e.to_string())?
                .take(MAX_SOURCE_BYTES as u64 + 1)
                .read_to_string(&mut text)
                .map_err(|e| e.to_string())?;
            if text.len() > MAX_SOURCE_BYTES {
                return Err("shader file exceeds 64 KiB".into());
            }
            Ok(text)
        };
        let vertex = if let Some(path) = &self.vertex {
            read(path)?
        } else {
            self.material.0.source.borrow().vertex.clone()
        };
        let fragment = read(&self.fragment)?;
        self.material
            .reload(ShaderSource { vertex, fragment })
            .map_err(str::to_string)?;
        self.last = Some(current);
        Ok(true)
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::*;
    thread_local! {static EDIT:RefCell<Vec<u8>>=RefCell::new(vec![0;MAX_SOURCE_BYTES]);static FEEDBACK:RefCell<Vec<u8>>=RefCell::new(vec![0;MAX_ERROR_BYTES]);}
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_material_count() -> u32 {
        count() as u32
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_material_id(index: u32) -> u32 {
        id_at(index as usize)
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_material_revision(id: u32) -> u32 {
        lookup(id).map_or(0, |m| m.revision.get())
    }
    /// Stages 0/1 are the requested vertex/fragment hooks; 2/3 preserve the last
    /// successfully compiled hooks for device restoration after a failed edit.
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_material_source(id: u32, stage: u32) -> *const u8 {
        lookup(id).map_or(std::ptr::null(), |m| {
            if stage < 2 {
                let s = m.source.borrow();
                if stage == 0 {
                    s.vertex.as_ptr()
                } else {
                    s.fragment.as_ptr()
                }
            } else if stage < 4 {
                m.last_valid_source
                    .borrow()
                    .as_ref()
                    .map_or(std::ptr::null(), |s| {
                        if stage == 2 {
                            s.vertex.as_ptr()
                        } else {
                            s.fragment.as_ptr()
                        }
                    })
            } else {
                std::ptr::null()
            }
        })
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_material_source_len(id: u32, stage: u32) -> u32 {
        lookup(id).map_or(0, |m| {
            if stage < 2 {
                let s = m.source.borrow();
                if stage == 0 {
                    s.vertex.len() as u32
                } else {
                    s.fragment.len() as u32
                }
            } else if stage < 4 {
                m.last_valid_source.borrow().as_ref().map_or(0, |s| {
                    if stage == 2 {
                        s.vertex.len() as u32
                    } else {
                        s.fragment.len() as u32
                    }
                })
            } else {
                0
            }
        })
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_material_feedback_buffer() -> *mut u8 {
        FEEDBACK.with(|b| b.borrow_mut().as_mut_ptr())
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_material_feedback(id: u32, revision: u32, ok: u32, len: u32) {
        if ok != 0 {
            feedback(id, revision, Ok(()));
        } else {
            FEEDBACK.with(|b| {
                feedback(
                    id,
                    revision,
                    Err(&String::from_utf8_lossy(
                        &b.borrow()[..(len as usize).min(MAX_ERROR_BYTES)],
                    )),
                )
            });
        }
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_material_edit_buffer() -> *mut u8 {
        EDIT.with(|b| b.borrow_mut().as_mut_ptr())
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_material_reload(id: u32, vertex: u32, fragment: u32) -> u32 {
        let Some(data) = lookup(id) else {
            return 0;
        };
        let Some(total) = vertex
            .checked_add(fragment)
            .filter(|n| *n as usize <= MAX_SOURCE_BYTES)
        else {
            return 0;
        };
        EDIT.with(|b| {
            let b = b.borrow();
            let Ok(v) = std::str::from_utf8(&b[..vertex as usize]) else {
                return 0;
            };
            let Ok(f) = std::str::from_utf8(&b[vertex as usize..total as usize]) else {
                return 0;
            };
            ShaderMaterial(data)
                .reload(ShaderSource {
                    vertex: v.into(),
                    fragment: f.into(),
                })
                .unwrap_or(0)
        })
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_material_status(id: u32) -> u32 {
        lookup(id).map_or(0, |d| match d.status.borrow().status {
            CompileStatus::Pending => 1,
            CompileStatus::Ready => 2,
            CompileStatus::Failed(_) => 3,
        })
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_material_error(id: u32) -> *const u8 {
        lookup(id).map_or(std::ptr::null(), |d| match &d.status.borrow().status {
            CompileStatus::Failed(s) => s.as_ptr(),
            _ => std::ptr::null(),
        })
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_material_error_len(id: u32) -> u32 {
        lookup(id).map_or(0, |d| match &d.status.borrow().status {
            CompileStatus::Failed(s) => s.len() as u32,
            _ => 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed_material() -> ShaderMaterial {
        ShaderMaterial::new_with_layout(
            Default::default(),
            Default::default(),
            [
                Some(ParameterType::Float),
                Some(ParameterType::Vec2),
                Some(ParameterType::Vec3),
                Some(ParameterType::Vec4),
            ],
        )
        .unwrap()
    }

    fn active_binding() -> (Option<u32>, [f32; 16]) {
        crate::read(|s| (s.shader.as_ref().map(ShaderMaterial::id), s.shader_params))
    }

    #[test]
    fn typed_parameters_pack_into_existing_abi_and_bind_through_clones() {
        let material = typed_material();
        let mut params = material.parameters();
        assert_eq!(*params.as_rows(), [[0.; 4]; 4]);
        params
            .update(&[
                (0, ParameterValue::Float(1.)),
                (1, ParameterValue::Vec2([2., 3.])),
                (2, ParameterValue::Vec3([4., 5., 6.])),
                (3, ParameterValue::Vec4([7., 8., 9., 10.])),
            ])
            .unwrap();
        assert_eq!(
            *params.as_rows(),
            [
                [1., 0., 0., 0.],
                [2., 3., 0., 0.],
                [4., 5., 6., 0.],
                [7., 8., 9., 10.]
            ]
        );
        material.clone().begin_checked(&params).unwrap();
        assert_eq!(
            active_binding(),
            (
                Some(material.id()),
                [
                    1., 0., 0., 0., 2., 3., 0., 0., 4., 5., 6., 0., 7., 8., 9., 10.
                ]
            )
        );
        material.end();
    }

    #[test]
    fn invalid_and_undeclared_slots_preserve_all_values() {
        let material = ShaderMaterial::new_with_layout(
            Default::default(),
            Default::default(),
            [Some(ParameterType::Float), None, None, None],
        )
        .unwrap();
        let mut params = material.parameters();
        params.set(0, ParameterValue::Float(5.)).unwrap();
        let before = *params.as_rows();
        for slot in [4, 5, usize::MAX] {
            assert_eq!(
                params.set(slot, ParameterValue::Float(7.)),
                Err(ParameterError::SlotOutOfRange { slot })
            );
            assert_eq!(*params.as_rows(), before);
        }
        for slot in 1..4 {
            assert_eq!(
                params.set(slot, ParameterValue::Float(7.)),
                Err(ParameterError::UndeclaredSlot { slot })
            );
            assert_eq!(*params.as_rows(), before);
        }
    }

    #[test]
    fn every_wrong_type_reports_the_declared_type_without_changing_values() {
        let material = typed_material();
        let values = [
            ParameterValue::Float(1.),
            ParameterValue::Vec2([2.; 2]),
            ParameterValue::Vec3([3.; 3]),
            ParameterValue::Vec4([4.; 4]),
        ];
        let mut params = material.parameters();
        for (slot, value) in values.into_iter().enumerate() {
            params.set(slot, value).unwrap();
        }
        let before = *params.as_rows();
        for (slot, expected) in material.parameter_layout().into_iter().enumerate() {
            let expected = expected.unwrap();
            for value in values {
                let provided = value.packed().0;
                if provided == expected {
                    continue;
                }
                let error = ParameterError::TypeMismatch {
                    slot,
                    expected,
                    provided,
                };
                assert_eq!(params.set(slot, value), Err(error));
                assert_eq!(*params.as_rows(), before);
                assert!(error.to_string().contains(&format!("u_params[{slot}]")));
                assert!(error.to_string().contains(&format!("expects {expected}")));
                assert!(error.to_string().contains(&format!("received {provided}")));
            }
        }
    }

    #[test]
    fn all_nonfinite_components_are_rejected_without_touching_the_active_binding() {
        let material = ShaderMaterial::new(Default::default(), Default::default()).unwrap();
        let mut params = material.parameters();
        for slot in 0..4 {
            params
                .set(slot, ParameterValue::Vec4([slot as f32 + 1.; 4]))
                .unwrap();
        }
        material.begin_checked(&params).unwrap();
        let before = *params.as_rows();
        let active = active_binding();
        for slot in 0..4 {
            for component in 0..4 {
                for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
                    let mut row = [1.; 4];
                    row[component] = invalid;
                    let error = ParameterError::NonFinite { slot, component };
                    assert_eq!(params.set(slot, ParameterValue::Vec4(row)), Err(error));
                    assert_eq!(*params.as_rows(), before);
                    assert_eq!(active_binding(), active);
                    assert_eq!(
                        error.to_string(),
                        format!("u_params[{slot}][{component}] must be finite")
                    );
                }
            }
        }
        material.end();
    }

    #[test]
    fn smaller_types_check_nonfinite_values_and_accept_finite_extremes() {
        let material = typed_material();
        let mut params = material.parameters();
        for (slot, value, component) in [
            (0, ParameterValue::Float(f32::NAN), 0),
            (1, ParameterValue::Vec2([0., f32::INFINITY]), 1),
            (2, ParameterValue::Vec3([0., 0., f32::NEG_INFINITY]), 2),
        ] {
            assert_eq!(
                params.set(slot, value),
                Err(ParameterError::NonFinite { slot, component })
            );
            assert_eq!(*params.as_rows(), [[0.; 4]; 4]);
        }
        params
            .set(3, ParameterValue::Vec4([f32::MIN, f32::MAX, -0., 0.]))
            .unwrap();
        assert_eq!(params.as_rows()[3][0], f32::MIN);
        assert_eq!(params.as_rows()[3][1], f32::MAX);
        assert_eq!(params.as_rows()[3][2].to_bits(), (-0_f32).to_bits());
    }

    #[test]
    fn multi_slot_updates_roll_back_every_failure_and_define_duplicate_order() {
        let material = ShaderMaterial::new_with_layout(
            Default::default(),
            Default::default(),
            [
                Some(ParameterType::Float),
                Some(ParameterType::Vec2),
                None,
                None,
            ],
        )
        .unwrap();
        let mut params = material.parameters();
        params.set(0, ParameterValue::Float(4.)).unwrap();
        let before = *params.as_rows();
        for bad_update in [
            (usize::MAX, ParameterValue::Float(5.)),
            (2, ParameterValue::Float(5.)),
            (1, ParameterValue::Float(5.)),
            (1, ParameterValue::Vec2([5., f32::NAN])),
            (0, ParameterValue::Float(f32::INFINITY)),
        ] {
            assert!(
                params
                    .update(&[(0, ParameterValue::Float(9.)), bad_update])
                    .is_err()
            );
            assert_eq!(*params.as_rows(), before);
        }
        // Even an invalid earlier duplicate cannot be hidden by a later valid update.
        assert!(
            params
                .update(&[
                    (0, ParameterValue::Float(f32::NAN)),
                    (0, ParameterValue::Float(9.)),
                ])
                .is_err()
        );
        assert_eq!(*params.as_rows(), before);
        params.update(&[]).unwrap();
        assert_eq!(*params.as_rows(), before);
        params
            .update(&[
                (0, ParameterValue::Float(6.)),
                (1, ParameterValue::Vec2([7., 8.])),
                (0, ParameterValue::Float(9.)),
            ])
            .unwrap();
        assert_eq!(
            *params.as_rows(),
            [[9., 0., 0., 0.], [7., 8., 0., 0.], [0.; 4], [0.; 4]]
        );
    }

    #[test]
    fn wrong_material_with_identical_layout_preserves_the_previous_binding() {
        let a = typed_material();
        let b = typed_material();
        let mut params = a.parameters();
        params.set(0, ParameterValue::Float(3.)).unwrap();
        a.begin_checked(&params).unwrap();
        let before = active_binding();
        let error = ParameterError::MaterialMismatch {
            expected: b.id(),
            provided: a.id(),
        };
        assert_eq!(b.begin_checked(&params), Err(error));
        assert_eq!(active_binding(), before);
        let message = error.to_string();
        assert!(message.contains(&format!("belongs to material {}", a.id())));
        assert!(message.contains(&format!("not material {}", b.id())));
        assert!(message.contains("parameters()"));
        a.end();
    }

    #[test]
    fn parameter_copies_are_independent_and_binding_captures_values() {
        let material = typed_material();
        let mut params = material.parameters();
        params.set(0, ParameterValue::Float(3.)).unwrap();
        let mut copy = params.clone();
        copy.set(0, ParameterValue::Float(4.)).unwrap();
        assert_eq!(params.as_rows()[0][0], 3.);
        assert_eq!(material.parameters().as_rows()[0][0], 0.);
        material.begin_checked(&params).unwrap();
        params.set(0, ParameterValue::Float(5.)).unwrap();
        assert_eq!(active_binding().1[0], 3.);
        material.begin_checked(&params).unwrap();
        assert_eq!(active_binding().1[0], 5.);
        material.begin_checked(&copy).unwrap();
        assert_eq!(active_binding().1[0], 4.);
        material.end();
    }

    #[test]
    fn parameter_blocks_retain_the_material_identity_until_dropped() {
        let material = typed_material();
        let id = material.id();
        let mut params = material.parameters();
        drop(material);
        assert!(lookup(id).is_some());
        params.set(0, ParameterValue::Float(1.)).unwrap();
        drop(params);
        assert!(lookup(id).is_none());
    }

    #[test]
    fn rejected_reload_preserves_checked_layout_values_and_last_valid_source() {
        let material = typed_material();
        let layout = material.parameter_layout();
        let mut params = material.parameters();
        params.set(0, ParameterValue::Float(2.)).unwrap();
        material.begin_checked(&params).unwrap();
        let before = active_binding();
        feedback(material.id(), 1, Ok(()));
        assert!(material.reload(ShaderSource::fragment("\0")).is_err());
        assert_eq!(material.status().requested_revision, 1);
        assert_eq!(material.status().status, CompileStatus::Ready);
        let revision = material.reload(ShaderSource::fragment("bad GLSL")).unwrap();
        feedback(material.id(), revision, Err("fragment shader failed"));
        assert_eq!(material.status().last_valid_revision, Some(1));
        assert_eq!(
            material
                .0
                .last_valid_source
                .borrow()
                .as_ref()
                .unwrap()
                .fragment,
            DEFAULT_FRAGMENT_HOOK
        );
        assert_eq!(material.parameter_layout(), layout);
        material.begin_checked(&params).unwrap();
        assert_eq!(active_binding(), before);
        params.set(0, ParameterValue::Float(3.)).unwrap();
        material.begin_checked(&params).unwrap();
        assert_eq!(active_binding().1[0], 3.);
        material.end();
    }

    #[test]
    fn legacy_raw_binding_stays_compatible_and_rejects_invalid_values_transactionally() {
        let material =
            ShaderMaterial::new_with_layout(Default::default(), Default::default(), [None; 4])
                .unwrap();
        let checked = material.parameters();
        material.begin_checked(&checked).unwrap();
        assert_eq!(active_binding().1, [0.; 16]);
        let rows = [
            [1., 2., 3., 4.],
            [5., 6., 7., 8.],
            [9., 10., 11., 12.],
            [13., 14., 15., 16.],
        ];
        material.begin(rows).unwrap();
        assert_eq!(active_binding().1, std::array::from_fn(|i| i as f32 + 1.));
        let before = active_binding();
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut bad_rows = rows;
            bad_rows[3][3] = invalid;
            assert_eq!(
                material.begin(bad_rows),
                Err("material parameters must be finite")
            );
            assert_eq!(active_binding(), before);
        }
        material.end();
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn file_reload_is_bounded_and_failed_reads_preserve_source() {
        let path = std::env::temp_dir().join(format!(
            "ferravex-material-{}-{}.glsl",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&path, DEFAULT_FRAGMENT_HOOK).unwrap();
        let material = ShaderMaterial::new(Default::default(), Default::default()).unwrap();
        let mut watcher = MaterialFileWatcher::new(material.clone(), None, path.clone());
        assert!(watcher.poll().unwrap());
        assert_eq!(material.status().requested_revision, 2);
        std::fs::write(&path, "x".repeat(MAX_SOURCE_BYTES + 1)).unwrap();
        watcher.next = std::time::Instant::now();
        assert!(watcher.poll().is_err());
        assert_eq!(material.status().requested_revision, 2);
        std::fs::write(&path, DEFAULT_FRAGMENT_HOOK).unwrap();
        watcher.next = std::time::Instant::now();
        watcher.last = None;
        assert!(watcher.poll().unwrap());
        assert_eq!(material.status().requested_revision, 3);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn revisions_and_feedback_are_transactional() {
        let m = ShaderMaterial::new(Default::default(), Default::default()).unwrap();
        feedback(m.id(), 1, Ok(()));
        assert_eq!(m.status().last_valid_revision, Some(1));
        m.reload(ShaderSource::fragment("bad GLSL")).unwrap();
        feedback(m.id(), 1, Err("stale"));
        assert_eq!(m.status().status, CompileStatus::Pending);
        feedback(m.id(), 2, Err("new shader rejected"));
        assert_eq!(m.status().last_valid_revision, Some(1));
        assert!(matches!(m.status().status, CompileStatus::Failed(_)));
        assert!(m.reload(ShaderSource::fragment("\0")).is_err());
        assert_eq!(m.status().requested_revision, 2);
    }
    #[test]
    fn source_uniform_and_metadata_limits_are_checked() {
        assert!(
            ShaderMaterial::new(
                ShaderSource::fragment("x".repeat(MAX_SOURCE_BYTES)),
                Default::default()
            )
            .is_err()
        );
        assert!(
            ShaderMaterial::new(
                Default::default(),
                MaterialOptions {
                    bounds_padding: f32::NAN,
                    ..Default::default()
                }
            )
            .is_err()
        );
        let m = ShaderMaterial::new(Default::default(), Default::default()).unwrap();
        assert!(m.begin([[f32::NAN; 4]; 4]).is_err());
        feedback(m.id(), 1, Err(&"é".repeat(5000)));
        if let CompileStatus::Failed(s) = m.status().status {
            assert!(s.len() <= MAX_ERROR_BYTES);
        }
    }
}
