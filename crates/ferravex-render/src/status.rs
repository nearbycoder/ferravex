//! Opt-in, read-only status. Games own facts, IDs, availability and accepted events.
//!
//! IDs are unique across *all* supplied facts and mapped widgets, including omitted
//! items. Keep each ID attached to one meaning for one owner lifetime. `owner` is a
//! nonzero game-assigned provenance token, not an authentication credential. Use a
//! fresh token after replacing a game; do not reuse it while readers retain state.
//! A bridge belongs to its Rust owner; browser publication belongs to the calling
//! thread (single-threaded Wasm only). No scene discovery or input execution occurs.
//!
//! Epoch starts at 1; serial starts at 0 and increments only on changed, valid
//! publication. It compares retained semantics, including order and truncation
//! counts, not omitted source text or simulation ticks. Reset advances epoch,
//! clears publication and resets serial to 0.
//! Neither counter wraps: exhaustion is an error retaining last-good data. Browser
//! reconnect reads the current complete snapshot; it does not replay announcements.
//! Strings containing u64 values are decimal strings, never JS numbers.
//!
//! Runnable headless consumer (also see `tests/journey400_296.rs`):
//! ```
//! use ferravex_core::session::{GameSession, SessionState};
//! use ferravex_render::status::*;
//! let session = GameSession::new(64., 4, false);
//! let lifecycle = match session.state() {
//!     SessionState::Ready => "Ready", _ => "Started",
//! };
//! let items = [StatusItem { id: 1, kind: StatusKind::Lifecycle, label: lifecycle }];
//! let mut bridge = StatusBridge::new(42)?;
//! let facts = || StatusFacts { items: &items, ui: None, ui_actions: &[] };
//! assert!(bridge.publish(Some(facts()))?.changed);
//! assert!(!bridge.publish(Some(facts()))?.changed);
//! assert!(std::str::from_utf8(bridge.bytes()).unwrap().contains("Ready"));
//! bridge.publish_browser()?; // Explicit opt-in; host calls after its normal update.
//! assert!(clear_browser(42)?); // Host teardown, on the same thread.
//! # Ok::<(), StatusError>(())
//! ```
use crate::ui::{UiInspection, UiLabel, UiRole};
use std::{
    cell::RefCell,
    fmt::{self, Write},
};

/// Hard source-entry/work ceiling; exceeding it rejects the entire snapshot.
pub const MAX_SOURCE_ITEMS: usize = 128;
/// Total full source-label bytes, including labels omitted by item capacity.
pub const MAX_SOURCE_BYTES: usize = 32768;
/// Retained/published item ceiling (declaration order, then explicit UI map order).
pub const MAX_ITEMS: usize = 32;
/// Retained bytes per label. A whole UTF-8 prefix is retained with an explicit flag.
pub const MAX_LABEL_BYTES: usize = 256;
/// Maximum JSON bytes, including worst-case control-character escaping.
pub const MAX_WIRE_BYTES: usize = 57344;

/// Explicit semantic category. Only lifecycle and result text is live-announced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusKind {
    Lifecycle,
    Objective,
    Result,
    Action,
    Notice,
    Telemetry,
}
impl StatusKind {
    fn name(self) -> &'static str {
        match self {
            Self::Lifecycle => "lifecycle",
            Self::Objective => "objective",
            Self::Result => "result",
            Self::Action => "action",
            Self::Notice => "notice",
            Self::Telemetry => "telemetry",
        }
    }
}
/// Borrowed game-authored fact. Action labels may include current remapping hints.
#[derive(Clone, Copy, Debug)]
pub struct StatusItem<'a> {
    pub id: u64,
    pub kind: StatusKind,
    pub label: &'a str,
}
/// Explicit supported UI meaning; a button is not automatically a game action.
#[derive(Clone, Copy, Debug)]
pub struct UiAction {
    pub widget_id: u64,
    pub status_id: u64,
}
/// Complete current facts, not a patch. `None` passed to publish means unavailable;
/// `Some` with empty slices means available and empty. Inspection is immutable and
/// must be from the current finished composition. Host input blocking and semantic
/// availability remain the game's responsibility: omit blocked mappings yourself.
pub struct StatusFacts<'a> {
    pub items: &'a [StatusItem<'a>],
    pub ui: Option<UiInspection<'a>>,
    pub ui_actions: &'a [UiAction],
}
/// Reported errors leave the last-good bridge/browser publication unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusError {
    InvalidOwner,
    OwnerConflict,
    SourceItems,
    SourceBytes,
    DuplicateId(u64),
    DuplicateWidget(u64),
    MultipleSingletons,
    InspectionUnavailable,
    OutputCapacity { required_bytes: usize },
    CounterExhausted,
}
/// Bounded publication outcome; truncation counts are for retained labels and
/// omitted available items (including unavailable full UI labels), respectively.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StatusUpdate {
    pub changed: bool,
    pub epoch: u64,
    pub serial: u64,
    pub omitted: usize,
    pub shortened: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Item {
    id: u64,
    kind: StatusKind,
    text: [u8; MAX_LABEL_BYTES],
    len: usize,
    truncated: bool,
}
const EMPTY: Item = Item {
    id: 0,
    kind: StatusKind::Objective,
    text: [0; MAX_LABEL_BYTES],
    len: 0,
    truncated: false,
};
#[derive(Clone, Debug, PartialEq, Eq)]
struct Snapshot {
    available: bool,
    items: [Item; MAX_ITEMS],
    len: usize,
    omitted: usize,
    shortened: usize,
}
impl Snapshot {
    fn new(available: bool) -> Self {
        Self {
            available,
            items: [EMPTY; MAX_ITEMS],
            len: 0,
            omitted: 0,
            shortened: 0,
        }
    }
    fn push(&mut self, id: u64, kind: StatusKind, text: &str) {
        if self.len == MAX_ITEMS {
            self.omitted += 1;
            return;
        }
        let mut len = text.len().min(MAX_LABEL_BYTES);
        while !text.is_char_boundary(len) {
            len -= 1;
        }
        let mut item = Item {
            id,
            kind,
            len,
            truncated: len < text.len(),
            ..EMPTY
        };
        item.text[..len].copy_from_slice(&text.as_bytes()[..len]);
        self.shortened += usize::from(item.truncated);
        self.items[self.len] = item;
        self.len += 1;
    }
}
/// Fixed retained storage, no allocations during ordinary publication. See module
/// example for complete usage. O(128² + 128×1025 + MAX_WIRE_BYTES) bounded work;
/// labels are measured by byte length without scanning oversized source text.
pub struct StatusBridge {
    owner: u64,
    epoch: u64,
    serial: u64,
    snapshot: Option<Snapshot>,
    wire: [u8; MAX_WIRE_BYTES],
    len: usize,
}
impl StatusBridge {
    /// Start an unpublished bridge. No default snapshot or browser adoption.
    ///
    /// ```
    /// use ferravex_render::{status::*, ui::*};
    /// let mut ui = Ui::new();
    /// ui.set_inspection_enabled(true);
    /// ui.begin(Viewport::responsive([640., 480.], [0.; 4]).unwrap());
    /// ui.button(7, UiRect { min: [20.; 2], size: [160., 48.] },
    ///     "Resume", true, ButtonStyle::default()).unwrap();
    /// ui.finish(false); // No input requested by this headless recipe.
    /// let mappings = [UiAction { widget_id: 7, status_id: 20 }];
    /// let mut status = StatusBridge::new(100)?;
    /// status.publish(Some(StatusFacts { items: &[], ui: ui.inspect(),
    ///     ui_actions: &mappings }))?;
    /// let mut output = [0; 1024];
    /// let length = status.write(&mut output)?;
    /// assert!(std::str::from_utf8(&output[..length]).unwrap().contains("Resume"));
    /// status.reset()?;
    /// assert!(status.bytes().is_empty());
    /// # Ok::<(), StatusError>(())
    /// ```
    pub fn new(owner: u64) -> Result<Self, StatusError> {
        if owner == 0 {
            return Err(StatusError::InvalidOwner);
        }
        Ok(Self {
            owner,
            epoch: 1,
            serial: 0,
            snapshot: None,
            wire: [0; MAX_WIRE_BYTES],
            len: 0,
        })
    }
    /// Validate the entire bounded source before replacing publication. Rejects
    /// duplicates even in the omitted suffix. At most one lifecycle and result.
    /// UI maps select only registered, enabled, clipped-in buttons; absent/disabled
    /// buttons are unavailable actions, not truncation. Missing full labels count
    /// as omissions; their painter abbreviation is never substituted.
    pub fn publish(&mut self, facts: Option<StatusFacts<'_>>) -> Result<StatusUpdate, StatusError> {
        let mut next = Snapshot::new(facts.is_some());
        if let Some(facts) = facts {
            let count = facts
                .items
                .len()
                .checked_add(facts.ui_actions.len())
                .ok_or(StatusError::SourceItems)?;
            if count > MAX_SOURCE_ITEMS {
                return Err(StatusError::SourceItems);
            }
            let mut ids = [0; MAX_SOURCE_ITEMS];
            let mut bytes = 0usize;
            let mut lifecycle = false;
            let mut result = false;
            for (i, item) in facts.items.iter().enumerate() {
                check_id(&ids[..i], item.id)?;
                ids[i] = item.id;
                let singleton = match item.kind {
                    StatusKind::Lifecycle => Some(&mut lifecycle),
                    StatusKind::Result => Some(&mut result),
                    _ => None,
                };
                if let Some(seen) = singleton {
                    if *seen {
                        return Err(StatusError::MultipleSingletons);
                    }
                    *seen = true;
                }
                add_bytes(&mut bytes, item.label.len())?;
                next.push(item.id, item.kind, item.label);
            }
            if !facts.ui_actions.is_empty() && facts.ui.is_none() {
                return Err(StatusError::InspectionUnavailable);
            }
            for (i, action) in facts.ui_actions.iter().enumerate() {
                let n = facts.items.len() + i;
                check_id(&ids[..n], action.status_id)?;
                ids[n] = action.status_id;
                if facts.ui_actions[..i]
                    .iter()
                    .any(|a| a.widget_id == action.widget_id)
                {
                    return Err(StatusError::DuplicateWidget(action.widget_id));
                }
                // UiInspection has a hard 1025-entry limit. No generic iterator or
                // callback can inject an unbounded scan or execute gameplay here.
                let element = facts.ui.clone().and_then(|mut entries| {
                    entries.find(|e| {
                        e.id == Some(action.widget_id)
                            && e.role == UiRole::Button
                            && e.interaction_registered
                    })
                });
                if let Some(e) = element.filter(|e| e.enabled && e.visible_bounds.is_some()) {
                    match e.label {
                        UiLabel::Full(label) => {
                            add_bytes(&mut bytes, label.len())?;
                            next.push(action.status_id, StatusKind::Action, label);
                        }
                        UiLabel::Unavailable { required_bytes } => {
                            add_bytes(&mut bytes, required_bytes)?;
                            next.omitted += 1;
                        }
                    }
                }
            }
        }
        let changed = self.snapshot.as_ref() != Some(&next);
        if changed {
            let serial = self
                .serial
                .checked_add(1)
                .ok_or(StatusError::CounterExhausted)?;
            // Serialize to private scratch first, never expose partially written data.
            let mut scratch = [0; MAX_WIRE_BYTES];
            let mut writer = Writer {
                out: &mut scratch,
                len: 0,
            };
            encode(&mut writer, self.owner, self.epoch, serial, &next)
                .expect("fixed wire maximum covers bounded snapshot");
            let len = writer.len;
            self.wire.fill(0);
            self.wire[..len].copy_from_slice(&scratch[..len]);
            self.len = len;
            self.serial = serial;
        }
        let update = StatusUpdate {
            changed,
            epoch: self.epoch,
            serial: self.serial,
            omitted: next.omitted,
            shortened: next.shortened,
        };
        self.snapshot = Some(next);
        Ok(update)
    }
    /// Current initialized JSON prefix only; empty before publish/after reset.
    pub fn bytes(&self) -> &[u8] {
        &self.wire[..self.len]
    }
    /// Atomic caller-buffer copy; tail is untouched and never part of the wire.
    pub fn write(&self, output: &mut [u8]) -> Result<usize, StatusError> {
        if output.len() < self.len {
            return Err(StatusError::OutputCapacity {
                required_bytes: self.len,
            });
        }
        output[..self.len].copy_from_slice(self.bytes());
        Ok(self.len)
    }
    /// New epoch; same owner and storage. Call publish_browser again if adopted
    /// so readers see unavailable until the next complete publication.
    pub fn reset(&mut self) -> Result<(), StatusError> {
        self.epoch = self
            .epoch
            .checked_add(1)
            .ok_or(StatusError::CounterExhausted)?;
        self.serial = 0;
        self.snapshot = None;
        self.len = 0;
        self.wire.fill(0);
        Ok(())
    }
    /// Explicit copy to this thread's readonly export slot. Another owner must
    /// clear its slot before replacement. Repeated identical copies are no-ops.
    /// Host must call clear_browser on teardown; this does not retain `self`.
    pub fn publish_browser(&self) -> Result<(), StatusError> {
        BROWSER.with_borrow_mut(|slot| {
            if let Some(current) = &slot.data {
                if current.owner != self.owner {
                    return Err(StatusError::OwnerConflict);
                }
                if current.len == self.len && current.wire[..current.len] == *self.bytes() {
                    return Ok(());
                }
            }
            let revision = slot
                .revision
                .checked_add(1)
                .ok_or(StatusError::CounterExhausted)?;
            let current = slot.data.get_or_insert_with(|| {
                Box::new(BrowserData {
                    owner: self.owner,
                    wire: [0; MAX_WIRE_BYTES],
                    len: 0,
                })
            });
            current.wire.fill(0);
            current.wire[..self.len].copy_from_slice(self.bytes());
            current.len = self.len;
            slot.revision = revision;
            Ok(())
        })
    }
}
fn check_id(ids: &[u64], id: u64) -> Result<(), StatusError> {
    if ids.contains(&id) {
        Err(StatusError::DuplicateId(id))
    } else {
        Ok(())
    }
}
fn add_bytes(total: &mut usize, len: usize) -> Result<(), StatusError> {
    *total = total
        .checked_add(len)
        .filter(|n| *n <= MAX_SOURCE_BYTES)
        .ok_or(StatusError::SourceBytes)?;
    Ok(())
}
struct Writer<'a> {
    out: &'a mut [u8],
    len: usize,
}
impl Write for Writer<'_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let end = self
            .len
            .checked_add(s.len())
            .filter(|n| *n <= self.out.len())
            .ok_or(fmt::Error)?;
        self.out[self.len..end].copy_from_slice(s.as_bytes());
        self.len = end;
        Ok(())
    }
}
fn quoted(w: &mut impl Write, text: &str) -> fmt::Result {
    w.write_char('"')?;
    for c in text.chars() {
        match c {
            '"' => w.write_str("\\\"")?,
            '\\' => w.write_str("\\\\")?,
            c if c <= '\u{1f}' => write!(w, "\\u{:04x}", c as u32)?,
            _ => w.write_char(c)?,
        }
    }
    w.write_char('"')
}
fn encode(w: &mut impl Write, owner: u64, epoch: u64, serial: u64, s: &Snapshot) -> fmt::Result {
    write!(
        w,
        "{{\"version\":1,\"provenance\":\"ferravex-status\",\"owner\":\"{owner}\",\"epoch\":\"{epoch}\",\"serial\":\"{serial}\",\"available\":{},\"omitted\":{},\"shortened\":{},\"items\":[",
        s.available, s.omitted, s.shortened
    )?;
    for (i, item) in s.items[..s.len].iter().enumerate() {
        if i != 0 {
            w.write_char(',')?;
        }
        write!(
            w,
            "{{\"id\":\"{}\",\"kind\":\"{}\",\"truncated\":{},\"label\":",
            item.id,
            item.kind.name(),
            item.truncated
        )?;
        quoted(
            w,
            std::str::from_utf8(&item.text[..item.len]).expect("whole UTF-8 prefix"),
        )?;
        w.write_char('}')?;
    }
    w.write_str("]}")
}
struct BrowserData {
    owner: u64,
    wire: [u8; MAX_WIRE_BYTES],
    len: usize,
}
struct BrowserSlot {
    revision: u64,
    data: Option<Box<BrowserData>>,
}
thread_local! { static BROWSER: RefCell<BrowserSlot> = const { RefCell::new(BrowserSlot { revision: 0, data: None }) }; }
/// Remove only this owner's publication, freeing storage. Checked revision never
/// wraps. No listeners, loops or action exports exist. See module cleanup recipe.
pub fn clear_browser(owner: u64) -> Result<bool, StatusError> {
    BROWSER.with_borrow_mut(|slot| {
        let Some(data) = &slot.data else {
            return Ok(false);
        };
        if owner != data.owner {
            return Err(StatusError::OwnerConflict);
        }
        slot.revision = slot
            .revision
            .checked_add(1)
            .ok_or(StatusError::CounterExhausted)?;
        slot.data = None;
        Ok(true)
    })
}
// One Wasm contract. Pointer/length describe only the initialized prefix until
// next same-thread publish/clear. Read revision before/after copying. No threads,
// shared-memory mutation, async borrow, C string, capacity scan or writer export.
/// Readonly ABI version, always 1; this does not imply a publisher exists.
#[cfg_attr(target_arch = "wasm32", unsafe(no_mangle))]
pub extern "C" fn ferravex_status_v1_version() -> u32 {
    1
}
/// Thread-local nonwrapping u64 publication stamp (Wasm i64 / JavaScript BigInt).
/// Wasm returns the signed two's-complement BigInt view; compare that losslessly,
/// or use BigInt.asUintN(64, value). Semantic JSON counters are decimal strings.
/// Read before and after copying; stamp changes on copy/clear, not no-op copies.
#[cfg_attr(target_arch = "wasm32", unsafe(no_mangle))]
pub extern "C" fn ferravex_status_v1_revision() -> u64 {
    BROWSER.with_borrow(|s| s.revision)
}
/// Borrowed initialized-prefix address, null when absent/reset. Valid only until
/// the next same-thread publish/clear; reacquire current memory after growth.
#[cfg_attr(target_arch = "wasm32", unsafe(no_mangle))]
pub extern "C" fn ferravex_status_v1_ptr() -> *const u8 {
    BROWSER.with_borrow(|s| {
        s.data
            .as_ref()
            .filter(|d| d.len != 0)
            .map_or(std::ptr::null(), |d| d.wire.as_ptr())
    })
}
/// Initialized JSON byte length (no terminator), zero when absent/reset.
#[cfg_attr(target_arch = "wasm32", unsafe(no_mangle))]
pub extern "C" fn ferravex_status_v1_len() -> u32 {
    BROWSER.with_borrow(|s| s.data.as_ref().map_or(0, |d| d.len as u32))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn counters_never_wrap_and_errors_preserve_last_good() {
        let mut bridge = StatusBridge::new(u64::MAX).unwrap();
        bridge.serial = u64::MAX - 1;
        assert_eq!(bridge.publish(None).unwrap().serial, u64::MAX);
        let good = bridge.bytes().to_vec();
        assert!(!bridge.publish(None).unwrap().changed);
        assert_eq!(
            bridge.publish(Some(StatusFacts {
                items: &[],
                ui: None,
                ui_actions: &[]
            })),
            Err(StatusError::CounterExhausted)
        );
        assert_eq!(bridge.bytes(), good);
        bridge.epoch = u64::MAX;
        assert_eq!(bridge.reset(), Err(StatusError::CounterExhausted));
        assert_eq!(bridge.bytes(), good);
        BROWSER.with_borrow_mut(|s| s.revision = u64::MAX);
        assert_eq!(bridge.publish_browser(), Err(StatusError::CounterExhausted));
        assert_eq!(ferravex_status_v1_len(), 0);
        BROWSER.with_borrow_mut(|s| s.revision = 0);
        bridge.publish_browser().unwrap();
        BROWSER.with_borrow_mut(|s| s.revision = u64::MAX);
        assert_eq!(clear_browser(u64::MAX), Err(StatusError::CounterExhausted));
        assert_eq!(ferravex_status_v1_len() as usize, good.len());
        BROWSER.with_borrow_mut(|s| {
            s.data = None;
            s.revision = 0;
        });
    }
    #[test]
    fn wire_and_private_tail_bounds_at_maximal_counters() {
        let mut bridge = StatusBridge::new(u64::MAX).unwrap();
        bridge.epoch = u64::MAX;
        bridge.serial = u64::MAX - 1;
        let label = "\0".repeat(MAX_LABEL_BYTES);
        let items: Vec<_> = (0..MAX_ITEMS)
            .map(|i| StatusItem {
                id: u64::MAX - i as u64,
                kind: StatusKind::Notice,
                label: &label,
            })
            .collect();
        bridge
            .publish(Some(StatusFacts {
                items: &items,
                ui: None,
                ui_actions: &[],
            }))
            .unwrap();
        assert!(bridge.len < MAX_WIRE_BYTES);
        bridge.serial = 0;
        bridge.publish(None).unwrap();
        assert!(bridge.wire[bridge.len..].iter().all(|b| *b == 0));
    }
}
