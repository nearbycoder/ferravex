//! Fixed-capacity screen-space reservations around an aspect-fitted world.
//!
//! All rectangles use absolute host logical pixels, including safe-area offsets;
//! device pixel ratio belongs to the host. Draw HUD regions with a full-screen
//! [`Viewport::responsive`] using **zero insets** to avoid applying the safe area
//! twice. HUD text stays at its authored size. Only the world is scaled.
//!
//! Reservations consume the current content rectangle in call order. An earlier
//! top strip spans the full width; a later left strip starts below it. Reversing
//! those calls assigns the corner to the left strip instead. Shared boundaries
//! follow [`Rect::contains`]'s half-open convention.
//!
//! ```
//! use ferravex_play::ui::{HudEdge, HudRegions, Viewport};
//! // Reserve readable rows and a 44-pixel action target; keep a useful world area.
//! let screen = [390., 844.];
//! let mut hud = HudRegions::new(screen, [8., 24., 8., 16.], [160., 100.])?;
//! let header = hud.reserve("header", HudEdge::Top, 76.)?;
//! let actions = hud.reserve("actions", HudEdge::Bottom, 44.)?;
//! let footer = hud.reserve("footer", HudEdge::Bottom, 40.)?;
//! assert_eq!(hud.region("header"), Some(header));
//! assert_eq!(hud.content().min, [8., 100.]);
//! assert!(header.intersect(actions).is_none());
//! assert!(footer.intersect(hud.content()).is_none());
//! let ui = Viewport::responsive(screen, [0.; 4]).unwrap();
//! assert_eq!(ui.scale(), 1.); // No font scaling, including in portrait.
//! assert_eq!(ui.unproject(header.min), Some(header.min));
//! # Ok::<(), ferravex_play::ui::HudLayoutError>(())
//! ```

use super::{Rect, Viewport};

/// Maximum number of named strips, including zero-thickness strips.
pub const HUD_REGION_CAPACITY: usize = 8;

/// The edge of the *remaining* content from which a strip is taken.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HudEdge {
    Top,
    Right,
    Bottom,
    Left,
}

/// One strip in reservation order. Names are borrowed static labels, never copied
/// into allocated storage. Use the same rectangle for painting and hit testing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HudRegion {
    pub name: &'static str,
    pub edge: HudEdge,
    pub rect: Rect,
}

/// Layout failure; no failed operation changes existing reservations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HudLayoutError {
    /// Negative/non-finite geometry, empty name, or an unrepresentable transform.
    InvalidLayout,
    /// A reservation already uses this name (compared by string contents).
    DuplicateName,
    /// All [`HUD_REGION_CAPACITY`] slots are occupied.
    Capacity,
    /// A strip or the caller's minimum content size does not fit. No automatic
    /// text shrinking, reservation clipping or minimum-size relaxation occurs.
    InsufficientSpace,
}

impl std::fmt::Display for HudLayoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidLayout => "invalid HUD layout or unrepresentable world fit",
            Self::DuplicateName => "duplicate HUD region name",
            Self::Capacity => "HUD reservation capacity exhausted",
            Self::InsufficientSpace => "insufficient space for HUD and minimum content",
        })
    }
}

impl std::error::Error for HudLayoutError {}

const EMPTY: HudRegion = HudRegion {
    name: "",
    edge: HudEdge::Top,
    rect: Rect {
        min: [0.; 2],
        size: [0.; 2],
    },
};

/// An allocation-free, bounded HUD composition. Rebuild on resize with [`Self::new`]
/// or reuse the current screen/safe area with [`Self::clear`]. There is no retained
/// input, simulation, font, renderer or clock state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HudRegions {
    screen: [f32; 2],
    bounds: Rect,
    content: Rect,
    minimum_content: [f32; 2],
    regions: [HudRegion; HUD_REGION_CAPACITY],
    len: usize,
}

impl HudRegions {
    /// Create a layout in absolute logical screen coordinates. Insets are left,
    /// top, right, bottom, with the same proportional collapse as
    /// [`Viewport::responsive`]. Dimensions, insets and `minimum_content` must be
    /// finite and nonnegative. Empty screens are valid when the minimum permits.
    ///
    /// `minimum_content` is an authored size in screen units, independent of the
    /// world's design size. Insufficient space is explicit so callers can choose
    /// a compact arrangement or omit secondary rows while retaining readable text.
    pub fn new(
        screen: [f32; 2],
        insets: [f32; 4],
        minimum_content: [f32; 2],
    ) -> Result<Self, HudLayoutError> {
        if !minimum_content.into_iter().all(nonnegative) {
            return Err(HudLayoutError::InvalidLayout);
        }
        let bounds = Viewport::responsive(screen, insets)
            .map_err(|_| HudLayoutError::InvalidLayout)?
            .pixels();
        if !bounds.valid() {
            return Err(HudLayoutError::InvalidLayout);
        }
        if (0..2).any(|axis| bounds.size[axis] < minimum_content[axis]) {
            return Err(HudLayoutError::InsufficientSpace);
        }
        Ok(Self {
            screen,
            bounds,
            content: bounds,
            minimum_content,
            regions: [EMPTY; HUD_REGION_CAPACITY],
            len: 0,
        })
    }

    /// Original safe-area rectangle, before any reservations.
    pub fn bounds(&self) -> Rect {
        self.bounds
    }

    /// Remaining world/content rectangle in absolute logical screen units.
    pub fn content(&self) -> Rect {
        self.content
    }

    /// Borrow the occupied slots in reservation order; no temporary allocation.
    pub fn regions(&self) -> &[HudRegion] {
        &self.regions[..self.len]
    }

    /// Look up a named rectangle. This accepts non-static queries as well.
    pub fn region(&self, name: &str) -> Option<Rect> {
        self.regions()
            .iter()
            .find(|r| r.name == name)
            .map(|r| r.rect)
    }

    /// Reserve a strip with the given thickness in logical screen units.
    /// Top/bottom consume height; left/right consume width. Repeated edges are
    /// allowed. A zero thickness creates an inert named strip and uses one slot.
    ///
    /// Invalid requests, duplicate names, capacity and then available space are
    /// checked in that order. Every failure is atomic. Floating-point extents are
    /// rounded inward when necessary to keep rectangles disjoint; a positive
    /// thickness whose edge cannot be represented is rejected as invalid.
    ///
    /// ```
    /// use ferravex_play::ui::{HudEdge, HudLayoutError, HudRegions};
    /// let mut hud = HudRegions::new([320., 200.], [0.; 4], [160., 100.])?;
    /// hud.reserve("header", HudEdge::Top, 60.)?;
    /// let before = hud;
    /// assert_eq!(hud.reserve("footer", HudEdge::Bottom, 44.),
    ///            Err(HudLayoutError::InsufficientSpace));
    /// assert_eq!(hud, before);
    /// # Ok::<(), HudLayoutError>(())
    /// ```
    pub fn reserve(
        &mut self,
        name: &'static str,
        edge: HudEdge,
        thickness: f32,
    ) -> Result<Rect, HudLayoutError> {
        if name.is_empty() || !nonnegative(thickness) {
            return Err(HudLayoutError::InvalidLayout);
        }
        if self.region(name).is_some() {
            return Err(HudLayoutError::DuplicateName);
        }
        if self.len == HUD_REGION_CAPACITY {
            return Err(HudLayoutError::Capacity);
        }
        let axis = usize::from(matches!(edge, HudEdge::Top | HudEdge::Bottom));
        if thickness as f64 + self.minimum_content[axis] as f64 > self.content.size[axis] as f64 {
            return Err(HudLayoutError::InsufficientSpace);
        }
        let leading = matches!(edge, HudEdge::Top | HudEdge::Left);
        let mut region = self.content;
        let mut content = self.content;
        let start = content.min[axis];
        let end = start + content.size[axis];
        if thickness == 0. {
            region.min[axis] = if leading { start } else { end };
            region.size[axis] = 0.;
        } else {
            let split = if leading {
                start + thickness
            } else {
                end - thickness
            }
            .clamp(start, end);
            if (leading && split <= start) || (!leading && split >= end) {
                return Err(HudLayoutError::InvalidLayout);
            }
            let (strip_start, strip_end, rest_start, rest_end) = if leading {
                (start, split, split, end)
            } else {
                (split, end, start, split)
            };
            region.min[axis] = strip_start;
            region.size[axis] = inward_extent(strip_start, strip_end);
            content.min[axis] = rest_start;
            content.size[axis] = inward_extent(rest_start, rest_end);
            if content.size[axis] < self.minimum_content[axis] {
                return Err(HudLayoutError::InsufficientSpace);
            }
        }
        self.regions[self.len] = HudRegion {
            name,
            edge,
            rect: region,
        };
        self.len += 1;
        self.content = content;
        Ok(region)
    }

    /// Remove all reservations and restore the original safe area/minimum policy.
    /// Reuses the fixed storage, including after a failed reservation.
    ///
    /// ```
    /// use ferravex_play::ui::{HudEdge, HudRegions};
    /// let mut hud = HudRegions::new([640., 360.], [8.; 4], [160., 90.])?;
    /// hud.reserve("header", HudEdge::Top, 44.)?;
    /// hud.clear();
    /// assert!(hud.regions().is_empty());
    /// assert_eq!(hud.region("header"), None);
    /// assert_eq!(hud.content(), hud.bounds());
    /// hud.reserve("header", HudEdge::Top, 60.)?;
    /// # Ok::<(), ferravex_play::ui::HudLayoutError>(())
    /// ```
    pub fn clear(&mut self) {
        self.regions = [EMPTY; HUD_REGION_CAPACITY];
        self.len = 0;
        self.content = self.bounds;
    }

    /// Fit a positive, finite world design size into the remaining content.
    /// Returns `None` when either content dimension is below one logical pixel;
    /// callers must skip world rendering and pointer handling in that case.
    /// An unrepresentable scale, camera zoom or displayed extent returns an error.
    /// HUD reservations and fonts are never scaled by this operation.
    ///
    /// ```
    /// use ferravex_play::ui::{HudEdge, HudRegions};
    /// let mut hud = HudRegions::new([800., 600.], [10., 20., 30., 40.], [0.; 2])?;
    /// hud.reserve("header", HudEdge::Top, 60.)?;
    /// let fit = hud.fit_canvas([400., 200.])?.unwrap();
    /// let world = fit.viewport();
    /// assert_eq!(world.scale(), 1.9);
    /// assert_eq!(world.pixels().min, [10., 130.]);
    /// assert_eq!(world.unproject([390., 320.]), Some([200., 100.]));
    /// assert_eq!(world.unproject([40., 40.]), None); // Header is not world input.
    /// assert_eq!(fit.screen(), [800., 600.]);
    /// assert!((fit.camera_offset()[0] + 0.025).abs() < 0.00001);
    /// assert!((fit.camera_offset()[1] + 1. / 15.).abs() < 0.00001);
    /// # Ok::<(), ferravex_play::ui::HudLayoutError>(())
    /// ```
    pub fn fit_canvas(&self, design: [f32; 2]) -> Result<Option<CanvasFit>, HudLayoutError> {
        if !design.into_iter().all(|v| v.is_finite() && v > 0.) {
            return Err(HudLayoutError::InvalidLayout);
        }
        if self.content.size.into_iter().any(|v| v < 1.) {
            return Ok(None);
        }
        let mut scale = (self.content.size[0] / design[0]).min(self.content.size[1] / design[1]);
        if !scale.is_finite() || scale <= 0. {
            return Err(HudLayoutError::InvalidLayout);
        }
        // Round down a division that would multiply back outside the content.
        if (0..2).any(|i| design[i] * scale > self.content.size[i]) {
            scale = scale.next_down();
        }
        let size = design.map(|v| v * scale);
        let pixels = Rect {
            min: std::array::from_fn(|i| {
                self.content.min[i] + (self.content.size[i] - size[i]) * 0.5
            }),
            size,
        };
        let zoom = self.screen.map(|v| 2. * scale / v);
        if scale <= 0.
            || !pixels.valid()
            || size.into_iter().any(|v| v <= 0.)
            || zoom.into_iter().any(|v| !v.is_finite() || v <= 0.)
            || (0..2).any(|i| pixels.min[i] + pixels.size[i] <= pixels.min[i])
            || (0..2).any(|i| {
                pixels.min[i] + pixels.size[i] > self.content.min[i] + self.content.size[i]
            })
        {
            return Err(HudLayoutError::InvalidLayout);
        }
        let center: [f32; 2] = std::array::from_fn(|i| pixels.min[i] + pixels.size[i] * 0.5);
        Ok(Some(CanvasFit {
            viewport: Viewport {
                bounds: Rect {
                    min: [0.; 2],
                    size: design,
                },
                pixels,
                scale,
            },
            screen: self.screen,
            camera_offset: [
                2. * (center[0] / self.screen[0] - 0.5),
                -2. * (center[1] / self.screen[1] - 0.5),
            ],
        }))
    }
}

fn nonnegative(v: f32) -> bool {
    v.is_finite() && v >= 0.
}

// Rect stores an extent, so subtraction followed by addition can round past the
// shared endpoint. One inward ULP prevents overlap without moving that endpoint.
fn inward_extent(start: f32, end: f32) -> f32 {
    let extent = end - start;
    if start + extent > end {
        extent.next_down()
    } else {
        extent
    }
}

/// Pure adapter data for an existing `ferravex_render::canvas::Canvas2D`.
/// This crate cannot depend on the renderer, which already depends on play.
///
/// The renderer's `Canvas2D::from_fit` consumes this value without recomputing
/// geometry; its `camera()` honors the asymmetric offset automatically. A custom
/// adapter can use `size = viewport().bounds().size`, `screen = screen()`,
/// `scale = viewport().scale()` and `offset = viewport().pixels().min`.
/// [`Self::camera_offset`] supplies normalized translation for hosts constructing
/// their camera directly.
///
/// Use `viewport().unproject(pointer)` or `Canvas2D::pointer_half_open` for
/// half-open world hit testing. Legacy `Canvas2D::pointer` includes its far
/// edges, so it can also accept a
/// shared HUD boundary. This adapter does not clip out-of-world draw commands;
/// callers keep their authored world bounds or apply their existing clipping.
/// See [`HudRegions::fit_canvas`] for an executable fitting/input example.
#[derive(Clone, Copy, Debug)]
pub struct CanvasFit {
    viewport: Viewport,
    screen: [f32; 2],
    camera_offset: [f32; 2],
}

impl CanvasFit {
    /// World coordinates, displayed rectangle and matching half-open hit mapping.
    pub fn viewport(self) -> Viewport {
        self.viewport
    }

    /// Full host screen size, not the reserved content size; used by the camera.
    pub fn screen(self) -> [f32; 2] {
        self.screen
    }

    /// Translation in normalized camera coordinates (positive Y points up).
    /// Use for a custom camera adapter. `Canvas2D::from_fit(self).camera()` already
    /// supplies the corresponding translation; no extra assignment is required.
    pub fn camera_offset(self) -> [f32; 2] {
        self.camera_offset
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(min: [f32; 2], size: [f32; 2]) -> Rect {
        Rect { min, size }
    }

    fn inside(outer: Rect, inner: Rect) {
        assert!(inner.valid(), "{inner:?}");
        for axis in 0..2 {
            assert!(inner.min[axis] >= outer.min[axis], "{outer:?}, {inner:?}");
            assert!(
                inner.min[axis] + inner.size[axis] <= outer.min[axis] + outer.size[axis],
                "{outer:?}, {inner:?}"
            );
        }
    }

    fn disjoint(layout: &HudRegions) {
        inside(layout.bounds(), layout.content());
        for (i, region) in layout.regions().iter().enumerate() {
            inside(layout.bounds(), region.rect);
            assert!(region.rect.intersect(layout.content()).is_none());
            for earlier in &layout.regions()[..i] {
                assert!(region.rect.intersect(earlier.rect).is_none());
            }
        }
    }

    #[test]
    fn four_edges_have_exact_ordered_geometry_and_named_lookup() {
        let mut hud = HudRegions::new([800., 600.], [10., 20., 30., 40.], [200., 100.]).unwrap();
        assert_eq!(hud.bounds(), rect([10., 20.], [760., 540.]));
        assert_eq!(
            hud.reserve("header", HudEdge::Top, 60.).unwrap(),
            rect([10., 20.], [760., 60.])
        );
        assert_eq!(
            hud.reserve("footer", HudEdge::Bottom, 40.).unwrap(),
            rect([10., 520.], [760., 40.])
        );
        assert_eq!(
            hud.reserve("actions", HudEdge::Left, 80.).unwrap(),
            rect([10., 80.], [80., 440.])
        );
        assert_eq!(
            hud.reserve("details", HudEdge::Right, 100.).unwrap(),
            rect([670., 80.], [100., 440.])
        );
        assert_eq!(hud.content(), rect([90., 80.], [580., 440.]));
        assert_eq!(
            hud.regions().iter().map(|r| r.name).collect::<Vec<_>>(),
            ["header", "footer", "actions", "details"]
        );
        assert_eq!(hud.region("actions"), Some(hud.regions()[2].rect));
        assert_eq!(hud.region("missing"), None);
        disjoint(&hud);
    }

    #[test]
    fn earlier_edges_own_corners_and_repeated_edges_stack_inward() {
        for first in [HudEdge::Top, HudEdge::Left] {
            let second = if first == HudEdge::Top {
                HudEdge::Left
            } else {
                HudEdge::Top
            };
            let mut hud = HudRegions::new([200.; 2], [0.; 4], [0.; 2]).unwrap();
            let a = hud.reserve("first", first, 40.).unwrap();
            let b = hud.reserve("second", second, 20.).unwrap();
            assert!(a.contains([10., 10.]));
            assert!(!b.contains([10., 10.]));
            let c = hud.reserve("third", second, 10.).unwrap();
            let axis = usize::from(second == HudEdge::Top);
            assert_eq!(b.min[axis] + b.size[axis], c.min[axis]);
            disjoint(&hud);
        }
    }

    #[test]
    fn failures_are_atomic_and_minimum_content_is_never_relaxed() {
        let mut hud = HudRegions::new([320., 200.], [0.; 4], [160., 100.]).unwrap();
        hud.reserve("header", HudEdge::Top, 60.).unwrap();
        let original = hud;
        for (name, edge, thickness, error) in [
            (
                "header",
                HudEdge::Bottom,
                10.,
                HudLayoutError::DuplicateName,
            ),
            ("", HudEdge::Left, 10., HudLayoutError::InvalidLayout),
            ("bad", HudEdge::Left, -1., HudLayoutError::InvalidLayout),
            (
                "bad",
                HudEdge::Left,
                f32::NAN,
                HudLayoutError::InvalidLayout,
            ),
            (
                "bad",
                HudEdge::Left,
                f32::INFINITY,
                HudLayoutError::InvalidLayout,
            ),
            (
                "footer",
                HudEdge::Bottom,
                44.,
                HudLayoutError::InsufficientSpace,
            ),
            (
                "sidebar",
                HudEdge::Right,
                161.,
                HudLayoutError::InsufficientSpace,
            ),
        ] {
            assert_eq!(hud.reserve(name, edge, thickness), Err(error));
            assert_eq!(hud, original);
        }
        hud.reserve("footer", HudEdge::Bottom, 40.).unwrap();
        hud.reserve("sidebar", HudEdge::Right, 160.).unwrap();
        assert_eq!(hud.content().size, [160., 100.]);
        disjoint(&hud);
        assert_eq!(
            HudRegions::new([10.; 2], [0.; 4], [11., 0.]),
            Err(HudLayoutError::InsufficientSpace)
        );
    }

    #[test]
    fn capacity_and_reset_reuse_fixed_storage_and_release_names() {
        let mut hud = HudRegions::new([640., 360.], [8.; 4], [100.; 2]).unwrap();
        let address = hud.regions.as_ptr();
        let bytes = std::mem::size_of_val(&hud);
        for _ in 0..64 {
            for name in ["a", "b", "c", "d", "e", "f", "g", "h"] {
                hud.reserve(name, HudEdge::Top, 1.).unwrap();
            }
            assert_eq!(hud.regions().len(), HUD_REGION_CAPACITY);
            let before = hud;
            assert_eq!(
                hud.reserve("overflow", HudEdge::Left, 0.),
                Err(HudLayoutError::Capacity)
            );
            assert_eq!(
                hud.reserve("a", HudEdge::Left, 0.),
                Err(HudLayoutError::DuplicateName)
            );
            assert_eq!(hud, before);
            hud.clear();
            assert_eq!(hud.content(), hud.bounds());
            assert!(hud.regions().is_empty());
            assert_eq!(hud.region("a"), None);
            assert_eq!(hud.regions.as_ptr(), address);
            assert_eq!(std::mem::size_of_val(&hud), bytes);
            assert_eq!(hud.regions, [EMPTY; HUD_REGION_CAPACITY]);
        }
        assert_eq!(
            hud.reserve("too large", HudEdge::Top, 300.),
            Err(HudLayoutError::InsufficientSpace)
        );
    }

    #[test]
    fn zero_strips_and_exhausted_content_are_inert() {
        let mut hud = HudRegions::new([100., 80.], [0.; 4], [0.; 2]).unwrap();
        for (name, edge) in [
            ("t", HudEdge::Top),
            ("r", HudEdge::Right),
            ("b", HudEdge::Bottom),
            ("l", HudEdge::Left),
        ] {
            let strip = hud.reserve(name, edge, 0.).unwrap();
            assert!(!strip.contains(strip.min));
        }
        assert_eq!(hud.content(), hud.bounds());
        hud.reserve("all", HudEdge::Top, 80.).unwrap();
        assert!(!hud.content().contains(hud.content().min));
        assert!(hud.fit_canvas([100.; 2]).unwrap().is_none());
        assert_eq!(
            hud.reserve("extra", HudEdge::Bottom, 1.),
            Err(HudLayoutError::InsufficientSpace)
        );
        disjoint(&hud);
    }

    #[test]
    fn tiny_screens_and_excessive_safe_insets_never_make_an_active_canvas() {
        for screen in [[0.; 2], [0., 100.], [100., 0.], [0.5, 100.], [100., 0.25]] {
            let hud = HudRegions::new(screen, [0.; 4], [0.; 2]).unwrap();
            assert!(hud.fit_canvas([1280., 720.]).unwrap().is_none());
            assert_eq!(
                HudRegions::new(screen, [0.; 4], [100.; 2]),
                Err(HudLayoutError::InsufficientSpace)
            );
        }
        let hud = HudRegions::new([100., 60.], [80., 20., 80., 60.], [0.; 2]).unwrap();
        assert_eq!(hud.bounds(), rect([50., 15.], [0.; 2]));
        assert!(hud.fit_canvas([100.; 2]).unwrap().is_none());
        let one = HudRegions::new([1.; 2], [0.; 4], [0.; 2]).unwrap();
        assert!(one.fit_canvas([1.; 2]).unwrap().is_some());
    }

    #[test]
    fn invalid_geometry_and_unrepresentable_transforms_are_rejected() {
        for bad in [-1., f32::NAN, f32::INFINITY] {
            assert_eq!(
                HudRegions::new([bad, 100.], [0.; 4], [0.; 2]),
                Err(HudLayoutError::InvalidLayout)
            );
            assert_eq!(
                HudRegions::new([100.; 2], [0., bad, 0., 0.], [0.; 2]),
                Err(HudLayoutError::InvalidLayout)
            );
            assert_eq!(
                HudRegions::new([100.; 2], [0.; 4], [0., bad]),
                Err(HudLayoutError::InvalidLayout)
            );
        }
        let hud = HudRegions::new([100.; 2], [0.; 4], [0.; 2]).unwrap();
        for design in [
            [0.; 2],
            [-1., 1.],
            [f32::NAN, 1.],
            [f32::INFINITY, 1.],
            [f32::from_bits(1); 2],
            [f32::MAX, f32::from_bits(1)],
        ] {
            assert!(matches!(
                hud.fit_canvas(design),
                Err(HudLayoutError::InvalidLayout)
            ));
        }
        let mut huge = HudRegions::new([f32::MAX; 2], [1e30, 1e30, 0., 0.], [0.; 2]).unwrap();
        let before = huge;
        assert_eq!(
            huge.reserve("lost precision", HudEdge::Top, 1.),
            Err(HudLayoutError::InvalidLayout)
        );
        assert_eq!(huge, before);
    }

    #[test]
    fn portrait_short_landscape_and_ultrawide_keep_authored_hud_sizes() {
        for screen in [[640., 320.], [390., 844.], [2560., 720.]] {
            let mut hud = HudRegions::new(screen, [8., 24., 12., 16.], [160., 100.]).unwrap();
            let header = hud.reserve("header", HudEdge::Top, 76.).unwrap();
            let actions = hud.reserve("actions", HudEdge::Bottom, 44.).unwrap();
            let footer = hud.reserve("footer", HudEdge::Bottom, 40.).unwrap();
            assert_eq!(header.size[1], 76.);
            assert_eq!(actions.size[1], 44.);
            assert_eq!(footer.size[1], 40.);
            let fit = hud.fit_canvas([1280., 720.]).unwrap().unwrap();
            let world = fit.viewport();
            inside(hud.content(), world.pixels());
            assert_eq!(world.bounds().size, [1280., 720.]);
            for point in [[0., 0.], [640., 360.], [1200., 700.]] {
                let projected =
                    std::array::from_fn(|i| world.pixels().min[i] + point[i] * world.scale());
                let actual = world.unproject(projected).unwrap();
                for i in 0..2 {
                    assert!((actual[i] - point[i]).abs() < 0.001);
                }
            }
            for strip in [header, actions, footer] {
                let center = std::array::from_fn(|i| strip.min[i] + strip.size[i] * 0.5);
                assert_eq!(world.unproject(center), None);
            }
            disjoint(&hud);
        }
    }

    #[test]
    fn safe_offset_letterboxing_and_half_open_pointer_boundaries_match() {
        let mut hud = HudRegions::new([800., 600.], [10., 20., 30., 40.], [0.; 2]).unwrap();
        hud.reserve("header", HudEdge::Top, 60.).unwrap();
        let fit = hud.fit_canvas([400., 200.]).unwrap().unwrap();
        let v = fit.viewport();
        assert_eq!(v.pixels(), rect([10., 130.], [760., 380.]));
        assert_eq!(v.unproject([10., 130.]), Some([0.; 2]));
        assert_eq!(v.unproject([390., 320.]), Some([200., 100.]));
        for outside in [
            [9., 130.],
            [10., 129.],
            [770., 130.],
            [10., 510.],
            [f32::NAN, 320.],
        ] {
            assert_eq!(v.unproject(outside), None);
        }
        // A full-height fit meets the footer exactly: that edge belongs to HUD.
        hud.clear();
        let footer = hud.reserve("footer", HudEdge::Bottom, 40.).unwrap();
        let v = hud.fit_canvas([100.; 2]).unwrap().unwrap().viewport();
        let boundary = [400., footer.min[1]];
        assert!(footer.contains(boundary));
        assert_eq!(v.unproject(boundary), None);
    }

    #[test]
    fn resize_reconstruction_has_no_stale_names_or_old_safe_origin() {
        let mut hud = HudRegions::new([800., 600.], [10.; 4], [0.; 2]).unwrap();
        hud.reserve("header", HudEdge::Top, 80.).unwrap();
        hud = HudRegions::new([390., 844.], [5., 24., 8., 16.], [0.; 2]).unwrap();
        assert_eq!(hud.region("header"), None);
        assert_eq!(hud.content(), rect([5., 24.], [377., 804.]));
        let fit = hud.fit_canvas([377., 804.]).unwrap().unwrap();
        assert_eq!(fit.viewport().unproject([5., 24.]), Some([0.; 2]));
        assert_eq!(fit.viewport().unproject([4., 24.]), None);
    }

    #[test]
    fn fractional_reservations_remain_disjoint_in_every_edge_order() {
        let edges = [HudEdge::Top, HudEdge::Right, HudEdge::Bottom, HudEdge::Left];
        for a in edges {
            for b in edges {
                for c in edges {
                    for d in edges {
                        for screen in [[319.7, 211.3], [390.25, 843.75], [2559.9, 719.7]] {
                            let mut hud =
                                HudRegions::new(screen, [0.1, 1.3, 5.7, 0.9], [50.; 2]).unwrap();
                            for ((name, edge), thickness) in ["a", "b", "c", "d"]
                                .into_iter()
                                .zip([a, b, c, d])
                                .zip([7.1, 8.3, 9.7, 10.1])
                            {
                                hud.reserve(name, edge, thickness).unwrap();
                                disjoint(&hud);
                            }
                            let fit = hud.fit_canvas([1280., 720.]).unwrap().unwrap();
                            inside(hud.content(), fit.viewport().pixels());
                        }
                    }
                }
            }
        }
    }
}
