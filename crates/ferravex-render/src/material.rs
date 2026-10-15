//! Reusable shader handles. GPU compilation is host-owned and replacements are transactional.
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
vec4 ferravex_fragment(vec4 color,vec3 world,vec3 normal);
void main(){gl_FragColor=ferravex_fragment(v_color,v_world,v_normal);}
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
#[derive(Clone, Copy, Debug, Default)]
pub struct MaterialOptions {
    pub blend: BlendMode,
    /// Authored local-space bound expansion for vertex displacement.
    pub bounds_padding: f32,
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
    status: RefCell<MaterialStatus>,
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
    pub fn new(source: ShaderSource, options: MaterialOptions) -> Result<Self, &'static str> {
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
                status: RefCell::new(MaterialStatus {
                    requested_revision: 1,
                    last_valid_revision: None,
                    status: CompileStatus::Pending,
                }),
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
    /// Queue a source replacement. Bad metadata leaves the request unchanged; GPU errors retain
    /// each host's last valid program. The latest request is applied at presentation time.
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
    pub fn begin(&self, params: [[f32; 4]; 4]) -> Result<(), &'static str> {
        if params.iter().flatten().any(|v| !v.is_finite()) {
            return Err("material parameters must be finite");
        }
        let mut flat = [0.; 16];
        for (i, p) in params.iter().enumerate() {
            flat[i * 4..i * 4 + 4].copy_from_slice(p);
        }
        crate::set_shader(Some(self.clone()), flat);
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
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_material_source(id: u32, stage: u32) -> *const u8 {
        lookup(id).map_or(std::ptr::null(), |m| {
            let s = m.source.borrow();
            if stage == 0 {
                s.vertex.as_ptr()
            } else {
                s.fragment.as_ptr()
            }
        })
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_material_source_len(id: u32, stage: u32) -> u32 {
        lookup(id).map_or(0, |m| {
            let s = m.source.borrow();
            if stage == 0 {
                s.vertex.len() as u32
            } else {
                s.fragment.len() as u32
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
