//! Allocation-free vertical scrolling in logical UI units, positive Y down.
//!
//! Content rectangles start at the content's top-left, before scrolling. Keep
//! widget IDs independent of offsets and list positions. A host chooses focus,
//! calls [`ScrollState::ensure_visible`] before painting, and uses the same
//! translated rectangles and viewport clip for paint and pointer input. This
//! module has no clock, input ownership, inertia, horizontal or nested routing.
//!
//! ```
//! use ferravex_play::ui::{Rect, scroll::ScrollState};
//! let viewport = Rect { min: [12., 24.], size: [200., 100.] };
//! let focused = Rect { min: [0., 180.], size: [200., 40.] };
//! let mut scroll = ScrollState::new(100., 400.)?;
//! assert!(scroll.ensure_visible(focused.min[1], focused.size[1])?);
//! assert_eq!(scroll.offset(), 120.);
//! let displayed = scroll.translate_rect(viewport, focused)?;
//! assert_eq!(displayed.min, [12., 84.]);
//! assert_eq!(scroll.content_point(viewport, [20., 90.]), Some([8., 186.]));
//! assert_eq!(scroll.content_point(viewport, [20., 124.]), None);
//! scroll.set_extents(300., 400.)?; // Resize clamps the old offset.
//! assert_eq!(scroll.offset(), 100.);
//! scroll.reset();
//! assert_eq!(scroll.offset(), 0.);
//! # Ok::<(), ferravex_play::ui::UiError>(())
//! ```

use super::{Rect, UiError};

/// Two finite, nonnegative extents and a clamped offset, with no dynamic storage.
/// Zero viewport height suspends scrolling at offset zero. Invalid operations
/// return [`UiError::InvalidLayout`] without changing the state.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScrollState {
    viewport_extent: f32,
    content_extent: f32,
    offset: f32,
}

impl ScrollState {
    /// Construct a top-aligned scroll state; see the module's executable recipe.
    pub fn new(viewport_extent: f32, content_extent: f32) -> Result<Self, UiError> {
        let mut state = Self::default();
        state.set_extents(viewport_extent, content_extent)?;
        Ok(state)
    }

    pub fn viewport_extent(&self) -> f32 {
        self.viewport_extent
    }

    pub fn content_extent(&self) -> f32 {
        self.content_extent
    }

    pub fn offset(&self) -> f32 {
        self.offset
    }

    /// Largest legal offset; zero for an empty viewport or fitting content.
    pub fn max_offset(&self) -> f32 {
        if self.viewport_extent == 0. {
            0.
        } else {
            (self.content_extent - self.viewport_extent).max(0.)
        }
    }

    /// Resize/reflow, preserving the old offset where possible. Returns whether
    /// the offset changed, not whether the extents changed.
    pub fn set_extents(
        &mut self,
        viewport_extent: f32,
        content_extent: f32,
    ) -> Result<bool, UiError> {
        if ![viewport_extent, content_extent]
            .into_iter()
            .all(|n| n.is_finite() && n >= 0.)
        {
            return Err(UiError::InvalidLayout);
        }
        self.viewport_extent = viewport_extent;
        self.content_extent = content_extent;
        Ok(self.assign(self.offset as f64))
    }

    /// Clamp a finite requested offset, including negative values, to the range.
    pub fn set_offset(&mut self, offset: f32) -> Result<bool, UiError> {
        if !offset.is_finite() {
            return Err(UiError::InvalidLayout);
        }
        Ok(self.assign(offset as f64))
    }

    /// Move by logical units. Positive deltas scroll down. Finite sums that
    /// exceed f32 range saturate at the scroll boundary, never at infinity.
    pub fn scroll_by(&mut self, delta: f32) -> Result<bool, UiError> {
        if !delta.is_finite() {
            return Err(UiError::InvalidLayout);
        }
        Ok(self.assign(self.offset as f64 + delta as f64))
    }

    /// Move one viewport height; `reverse` selects Page Up. Exhaustion is false.
    pub fn page(&mut self, reverse: bool) -> bool {
        let sign = if reverse { -1. } else { 1. };
        self.assign(self.offset as f64 + sign * self.viewport_extent as f64)
    }

    /// Reveal a content-space vertical interval using the smallest movement.
    /// Oversized intervals align their top edge; empty intervals/viewports do
    /// nothing. The whole interval must be inside the declared content extent.
    /// No padding is inserted and no text is scaled.
    pub fn ensure_visible(&mut self, top: f32, height: f32) -> Result<bool, UiError> {
        let end = top as f64 + height as f64;
        if ![top, height].into_iter().all(|v| v.is_finite() && v >= 0.)
            || end > self.content_extent as f64
        {
            return Err(UiError::InvalidLayout);
        }
        if height == 0. || self.viewport_extent == 0. {
            return Ok(false);
        }
        let offset = self.offset as f64;
        let requested = if height > self.viewport_extent || (top as f64) < offset {
            top as f64
        } else if end > offset + self.viewport_extent as f64 {
            end - self.viewport_extent as f64
        } else {
            offset
        };
        Ok(self.assign(requested))
    }

    /// Translate a content rectangle into UI coordinates without clipping it.
    /// The viewport height must match this state's extent. Content must lie
    /// inside `[0, viewport.width] × [0, content_extent]`. Apply the viewport
    /// clip separately so a partially visible label keeps its original layout.
    pub fn translate_rect(&self, viewport: Rect, content: Rect) -> Result<Rect, UiError> {
        if !self.valid_viewport(viewport)
            || !content.valid()
            || (0..2).any(|axis| content.min[axis] < 0.)
            || content.min[0] as f64 + content.size[0] as f64 > viewport.size[0] as f64
            || content.min[1] as f64 + content.size[1] as f64 > self.content_extent as f64
        {
            return Err(UiError::InvalidLayout);
        }
        let translated = Rect {
            min: [
                (viewport.min[0] as f64 + content.min[0] as f64) as f32,
                (viewport.min[1] as f64 + content.min[1] as f64 - self.offset as f64) as f32,
            ],
            size: content.size,
        };
        if translated.valid() {
            Ok(translated)
        } else {
            Err(UiError::InvalidLayout)
        }
    }

    /// Map a half-open viewport hit back into content space. Empty trailing
    /// viewport space beyond the content is inert; nonfinite input is rejected.
    pub fn content_point(&self, viewport: Rect, point: [f32; 2]) -> Option<[f32; 2]> {
        if !self.valid_viewport(viewport) || !viewport.contains(point) {
            return None;
        }
        let local = [
            (point[0] as f64 - viewport.min[0] as f64) as f32,
            (point[1] as f64 - viewport.min[1] as f64 + self.offset as f64) as f32,
        ];
        (local[0] >= 0.
            && local[0] < viewport.size[0]
            && local[1] >= 0.
            && local[1] < self.content_extent)
            .then_some(local)
    }

    /// Return to the top without forgetting the current extents.
    pub fn reset(&mut self) {
        self.offset = 0.;
    }

    fn assign(&mut self, requested: f64) -> bool {
        let next = requested.clamp(0., self.max_offset() as f64) as f32;
        let changed = next != self.offset;
        self.offset = next;
        changed
    }

    fn valid_viewport(&self, viewport: Rect) -> bool {
        viewport.valid() && viewport.size[1] == self.viewport_extent
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_pages_resize_and_reset() {
        let mut s = ScrollState::new(100., 350.).unwrap();
        assert!(!s.page(true));
        assert!(s.page(false));
        assert_eq!(s.offset(), 100.);
        assert!(s.scroll_by(f32::MAX).unwrap());
        assert_eq!(s.offset(), 250.);
        assert!(!s.page(false));
        assert!(!s.scroll_by(f32::MAX).unwrap());
        s.set_extents(300., 350.).unwrap();
        assert_eq!(s.offset(), 50.);
        s.reset();
        assert_eq!(s.offset(), 0.);
        assert_eq!(s.content_extent(), 350.);
        s.set_extents(0., 350.).unwrap();
        assert!(!s.set_offset(40.).unwrap());
        assert!(!s.ensure_visible(300., 50.).unwrap());
        s.set_extents(100., 50.).unwrap();
        assert!(!s.page(false));
        assert_eq!(s, ScrollState::new(100., 50.).unwrap());
    }

    #[test]
    fn reveal_offscreen_intervals_and_oversized_rows() {
        let mut s = ScrollState::new(100., 500.).unwrap();
        assert!(!s.ensure_visible(0., 40.).unwrap());
        assert!(s.ensure_visible(200., 40.).unwrap());
        assert_eq!(s.offset(), 140.);
        assert!(!s.ensure_visible(180., 20.).unwrap());
        assert!(s.ensure_visible(100., 20.).unwrap());
        assert_eq!(s.offset(), 100.);
        assert!(s.ensure_visible(250., 150.).unwrap());
        assert_eq!(s.offset(), 250.);
        assert!(!s.ensure_visible(500., 0.).unwrap());
    }

    #[test]
    fn errors_are_atomic_and_extreme_finite_deltas_saturate() {
        let mut s = ScrollState::new(10., 100.).unwrap();
        s.set_offset(50.).unwrap();
        let original = s;
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -1.] {
            assert_eq!(s.set_extents(bad, 100.), Err(UiError::InvalidLayout));
            assert_eq!(s.set_extents(10., bad), Err(UiError::InvalidLayout));
            assert_eq!(s.ensure_visible(bad, 1.), Err(UiError::InvalidLayout));
            assert_eq!(s.ensure_visible(0., bad), Err(UiError::InvalidLayout));
            if !bad.is_finite() {
                assert_eq!(s.set_offset(bad), Err(UiError::InvalidLayout));
                assert_eq!(s.scroll_by(bad), Err(UiError::InvalidLayout));
            }
            assert_eq!(s, original);
        }
        assert_eq!(s.ensure_visible(99., 2.), Err(UiError::InvalidLayout));
        assert_eq!(s, original);
        s.set_extents(1., f32::MAX).unwrap();
        s.set_offset(f32::MAX).unwrap();
        assert!(!s.scroll_by(f32::MAX).unwrap());
        s.scroll_by(-f32::MAX).unwrap();
        assert_eq!(s.offset(), 0.);
    }

    #[test]
    fn translation_and_hit_mapping_share_half_open_geometry() {
        let mut s = ScrollState::new(80., 300.).unwrap();
        s.set_offset(100.).unwrap();
        let viewport = Rect {
            min: [20., 40.],
            size: [120., 80.],
        };
        let content = Rect {
            min: [10., 120.],
            size: [40., 40.],
        };
        let displayed = s.translate_rect(viewport, content).unwrap();
        assert_eq!(displayed.min, [30., 60.]);
        for p in [[30., 60.], [50., 80.], [69.5, 99.5]] {
            assert_eq!(
                displayed.contains(p),
                content.contains(s.content_point(viewport, p).unwrap())
            );
        }
        for p in [[20., 120.], [140., 40.], [19., 40.], [f32::NAN, 50.]] {
            assert_eq!(s.content_point(viewport, p), None);
        }
        assert!(
            s.translate_rect(
                Rect {
                    size: [120., 81.],
                    ..viewport
                },
                content
            )
            .is_err()
        );
        assert!(
            s.translate_rect(
                viewport,
                Rect {
                    min: [100., 0.],
                    ..content
                }
            )
            .is_err()
        );
        assert!(
            s.translate_rect(
                viewport,
                Rect {
                    min: [0., 280.],
                    ..content
                }
            )
            .is_err()
        );
        s.set_extents(80., 10.).unwrap();
        assert_eq!(s.content_point(viewport, [21., 51.]), None);
    }
}
