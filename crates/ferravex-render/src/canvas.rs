//! Aspect-preserving 2D canvas with one coordinate transform for drawing and input.
//!
//! World positions use the caller's authored units. Screens, content rectangles,
//! offsets and pointer/touch positions use host logical pixels (Y down), relative
//! to the full screen's top-left. The host applies DPR only to the framebuffer;
//! do not multiply these inputs by DPR or apply safe-area insets twice.
use crate::prelude::*;
use ferravex_play::ui::{CanvasFit, HudLayoutError, Rect as ScreenRect};

/// Fits a Y-down world into a logical-pixel screen, with aspect-preserving letterboxing.
/// Uses camera zoom rather than a raw GPU viewport, so high-DPI screens behave identically.
/// This transform does not scissor drawing outside the authored world bounds.
/// Public fields remain available for existing struct literals; manually changed
/// fields are the caller's responsibility. Prefer the checked constructors.
#[derive(Debug, Clone, Copy)]
pub struct Canvas2D {
    /// Positive world dimensions; fitting never changes the authored framing.
    pub size: Vec2,
    /// Full host screen dimensions in logical pixels, including reserved HUD space.
    pub screen: Vec2,
    /// Logical screen pixels per world unit.
    pub scale: f32,
    /// Displayed world's top-left in absolute logical screen pixels.
    pub offset: Vec2,
}

impl Canvas2D {
    /// Legacy full-screen fit, preserving its original arithmetic and signature.
    ///
    /// # Panics
    /// Panics for nonfinite or nonpositive world/screen dimensions. Use
    /// [`Self::try_fit`] for checked input and suspended/zero-sized screens.
    pub fn fit(size: Vec2, screen: Vec2) -> Self {
        assert!(
            size.is_finite()
                && screen.is_finite()
                && size.min_element() > 0.0
                && screen.min_element() > 0.0,
            "canvas and screen dimensions must be finite and positive"
        );
        let scale = (screen / size).min_element();
        Self {
            size,
            screen,
            scale,
            offset: (screen - size * scale) * 0.5,
        }
    }

    /// Checked full-screen fit. World dimensions must be finite and positive;
    /// screen dimensions must be finite and nonnegative. Returns `Ok(None)` when
    /// either screen dimension is below one logical pixel. Invalid dimensions or
    /// an unrepresentable transform return [`HudLayoutError::InvalidLayout`].
    /// Skip world rendering and input when there is no canvas; reconstruction
    /// after resize has no retained offsets or state.
    ///
    /// ```
    /// use ferravex_render::{canvas::Canvas2D, vec2};
    /// let world = vec2(1280., 800.);
    /// let canvas = Canvas2D::try_fit(world, vec2(390., 844.))?.unwrap();
    /// assert_eq!(canvas.size, world);
    /// assert!(Canvas2D::try_fit(world, vec2(0., 0.))?.is_none());
    /// assert!(Canvas2D::try_fit(vec2(0., 800.), vec2(390., 844.)).is_err());
    /// # Ok::<(), ferravex_play::ui::HudLayoutError>(())
    /// ```
    pub fn try_fit(size: Vec2, screen: Vec2) -> Result<Option<Self>, HudLayoutError> {
        Self::try_fit_in(
            size,
            screen,
            ScreenRect {
                min: [0.; 2],
                size: screen.to_array(),
            },
        )
    }

    /// Checked fit into an explicit content rectangle, in absolute host logical
    /// pixels. `screen` is the **full** logical screen, not `content.size`.
    /// Content must be finite, nonnegative and wholly inside that screen.
    /// Validate dimensions before returning `Ok(None)` for content below one
    /// logical pixel on either axis. Other failures, including f32 overflow,
    /// underflow or collapsed displayed edges, are `InvalidLayout`.
    ///
    /// Fractional fits round scale inward when necessary to stay in content.
    /// HUD text is unaffected. Use [`Self::pointer_half_open`] to keep shared
    /// right/bottom edges owned by adjacent HUD rectangles.
    ///
    /// ```
    /// use ferravex_play::ui::Rect;
    /// use ferravex_render::{canvas::Canvas2D, vec2};
    /// let content = Rect { min: [10., 80.], size: [760., 480.] };
    /// let canvas = Canvas2D::try_fit_in(vec2(400., 200.), vec2(800., 600.), content)?
    ///     .unwrap();
    /// assert_eq!(canvas.offset, vec2(10., 130.));
    /// assert_eq!(canvas.pointer_half_open(vec2(390., 320.)), Some(vec2(200., 100.)));
    /// assert!(canvas.pointer_half_open(vec2(40., 40.)).is_none());
    /// # Ok::<(), ferravex_play::ui::HudLayoutError>(())
    /// ```
    pub fn try_fit_in(
        size: Vec2,
        screen: Vec2,
        content: ScreenRect,
    ) -> Result<Option<Self>, HudLayoutError> {
        if !size.is_finite()
            || size.min_element() <= 0.
            || !screen.is_finite()
            || screen.min_element() < 0.
            || !content.valid()
            || (0..2).any(|i| {
                content.min[i] < 0. || content.min[i] + content.size[i] > screen.to_array()[i]
            })
        {
            return Err(HudLayoutError::InvalidLayout);
        }
        if content.size.into_iter().any(|v| v < 1.) {
            return Ok(None);
        }
        let available = Vec2::from(content.size);
        let mut scale = (available / size).min_element();
        if !scale.is_finite() || scale <= 0. {
            return Err(HudLayoutError::InvalidLayout);
        }
        // Like HudRegions::fit_canvas, keep rounded division from expanding the
        // world into an adjacent HUD strip when multiplied back into pixels.
        if (0..2).any(|i| (size * scale).to_array()[i] > content.size[i]) {
            scale = scale.next_down();
        }
        let displayed = size * scale;
        let canvas = Self {
            size,
            screen,
            scale,
            offset: Vec2::from(content.min) + (available - displayed) * 0.5,
        };
        let camera = canvas.camera();
        let end = canvas.project(size);
        if scale <= 0.
            || !displayed.is_finite()
            || displayed.min_element() <= 0.
            || !canvas.offset.is_finite()
            || !end.is_finite()
            || !camera.zoom.is_finite()
            || camera.zoom.min_element() <= 0.
            || !camera.offset.is_finite()
            || (0..2).any(|i| {
                end.to_array()[i] <= canvas.offset.to_array()[i]
                    || canvas.offset.to_array()[i] < content.min[i]
                    || end.to_array()[i] > content.min[i] + content.size[i]
            })
        {
            return Err(HudLayoutError::InvalidLayout);
        }
        Ok(Some(canvas))
    }

    /// Adapt task 271's already-checked world fit without recomputing geometry.
    /// [`Self::camera`] honors its asymmetric offset automatically. This is a
    /// pure, allocation-free adapter; it does not change host state or input.
    ///
    /// ```
    /// use ferravex_play::ui::{HudEdge, HudRegions};
    /// use ferravex_render::{Camera, canvas::Canvas2D, vec2};
    /// let mut hud = HudRegions::new([800., 600.], [10., 20., 30., 40.], [0.; 2])?;
    /// hud.reserve("header", HudEdge::Top, 60.)?;
    /// if let Some(fit) = hud.fit_canvas([400., 200.])? {
    ///     let canvas = Canvas2D::from_fit(fit);
    ///     let point = vec2(200., 100.);
    ///     let logical = canvas.project(point);
    ///     assert_eq!(canvas.pointer_half_open(logical), Some(point));
    ///     let ndc = canvas.camera().matrix(800., 600.).project_point3(point.extend(0.));
    ///     let dpr = 2.; // Synthetic host framebuffer conversion, not device evidence.
    ///     let framebuffer = vec2((ndc.x + 1.) * 400., (1. - ndc.y) * 300.) * dpr;
    ///     assert!((framebuffer / dpr).distance(logical) < 0.001);
    /// }
    /// # Ok::<(), ferravex_play::ui::HudLayoutError>(())
    /// ```
    pub fn from_fit(fit: CanvasFit) -> Self {
        let viewport = fit.viewport();
        Self {
            size: Vec2::from(viewport.bounds().size),
            screen: Vec2::from(fit.screen()),
            scale: viewport.scale(),
            offset: Vec2::from(viewport.pixels().min),
        }
    }

    /// World units to absolute logical screen pixels, without clipping.
    pub fn project(self, point: Vec2) -> Vec2 {
        self.offset + point * self.scale
    }
    /// Absolute logical screen pixels to world units, without clipping.
    pub fn unproject(self, point: Vec2) -> Vec2 {
        (point - self.offset) / self.scale
    }

    /// Returns None for input in the letterbox margin, outside the playable canvas.
    /// Preserves inclusive right/bottom edges; use [`Self::pointer_half_open`]
    /// when the world shares those edges with reserved HUD controls.
    pub fn pointer(self, point: Vec2) -> Option<Vec2> {
        let p = self.unproject(point);
        (p.is_finite() && p.x >= 0.0 && p.y >= 0.0 && p.x <= self.size.x && p.y <= self.size.y)
            .then_some(p)
    }

    /// Maps mouse/touch positions in logical screen pixels to the displayed world.
    /// Left/top edges belong to the world; right/bottom edges do not. Membership
    /// uses displayed screen geometry before division, matching `Viewport` hits.
    /// Nonfinite input and nonpositive/nonfinite transform dimensions are inert.
    ///
    /// ```
    /// use ferravex_render::{canvas::Canvas2D, vec2};
    /// let canvas = Canvas2D::try_fit(vec2(400., 200.), vec2(800., 600.))?.unwrap();
    /// assert_eq!(canvas.pointer_half_open(canvas.project(vec2(0., 0.))), Some(vec2(0., 0.)));
    /// assert_eq!(canvas.pointer_half_open(canvas.project(canvas.size)), None);
    /// assert_eq!(canvas.pointer(canvas.project(canvas.size)), Some(canvas.size));
    /// # Ok::<(), ferravex_play::ui::HudLayoutError>(())
    /// ```
    pub fn pointer_half_open(self, point: Vec2) -> Option<Vec2> {
        if !self.size.is_finite()
            || self.size.min_element() <= 0.
            || !self.screen.is_finite()
            || self.screen.min_element() <= 0.
            || !self.scale.is_finite()
            || self.scale <= 0.
            || !(ScreenRect {
                min: self.offset.to_array(),
                size: (self.size * self.scale).to_array(),
            })
            .contains(point.to_array())
        {
            return None;
        }
        let world = self.unproject(point);
        world.is_finite().then_some(world)
    }

    /// Camera for the full screen, including translation into reserved content.
    /// Its offset is normalized (positive Y up); canvas offsets remain logical
    /// pixels (positive Y down). Legacy centered fits retain zero camera offset.
    pub fn camera(self) -> Camera2D {
        let centered = (self.screen - self.size * self.scale) * 0.5;
        Camera2D {
            target: self.size * 0.5,
            // The original 2D camera maps positive world Y toward the screen bottom.
            zoom: Vec2::splat(2.0 * self.scale) / self.screen,
            offset: (self.offset - centered) / self.screen * vec2(2., -2.),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn input_tracks_letterboxed_drawing_after_resize() {
        for screen in [
            vec2(1280.0, 800.0),
            vec2(390.0, 844.0),
            vec2(2560.0, 1440.0),
        ] {
            let canvas = Canvas2D::fit(vec2(1280.0, 800.0), screen);
            for point in [Vec2::ZERO, vec2(640.0, 400.0), vec2(1280.0, 800.0)] {
                assert!(canvas.unproject(canvas.project(point)).distance(point) < 0.001);
            }
            assert!(canvas.project(canvas.size * 0.5).distance(screen * 0.5) < 0.001);
            assert!(canvas.pointer(vec2(-1.0, -1.0)).is_none());
            assert_eq!(canvas.pointer(screen * 0.5), Some(canvas.size * 0.5));
            assert!(canvas.camera().zoom.y > 0.0);
        }
    }
}
