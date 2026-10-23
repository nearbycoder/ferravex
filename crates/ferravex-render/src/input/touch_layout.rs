//! Project authored HUD shapes once for both drawing and existing touch routing.
//!
//! Geometry is in **host logical screen pixels**, never framebuffer pixels. A
//! viewport adds its safe origin once; a canvas adds its world offset once. DPR
//! belongs to the host. Rectangles/circles and stick travel radii scale together;
//! output channels, floating-stick policy and authored slot order never change.
//! There is no implicit cropping, enlargement, overlap avoidance or font scaling.
//! Draw the returned `Control::region` with a screen camera (or a zero-inset,
//! responsive UI viewport), rather than projecting it a second time.
//!
//! Install [`TouchLayout::controls`] as the **authored** PlayerInput layout, then
//! draw [`TouchLayout::resolve`] using the player's loaded settings. Installing
//! an already resolved layout would apply customization twice. This preserves
//! the existing editor, save indexes, hit priority and resize cancellation.
//!
//! Hidden suffixes are supported, including an entirely hidden layout. Hidden
//! gaps return an error: PlayerInput indexes saved edits by slice position and
//! has no per-slot enable mask. Never compact a hidden middle slot or park its
//! geometry offscreen (saved edits could move that geometry back onscreen).
//!
//! ```
//! use ferravex_render::{input::{PlayerInput, touch::{Output, TouchLayoutSettings},
//!     touch_layout::{TouchLayout, TouchSlot, MinimumTouchSize}}, ui::{UiRect, Viewport}};
//! let viewport = Viewport::responsive([390., 844.], [10., 24., 12., 16.]).unwrap();
//! let slots = [TouchSlot::rect(0, UiRect { min: [8., 700.], size: [56.; 2] },
//!     Output::Button(3)), TouchSlot::hidden(1)];
//! let authored = TouchLayout::from_viewport(viewport, &slots, MinimumTouchSize::Require(44.))?;
//! let mut input = PlayerInput::transient("touch-geometry-example");
//! input.set_touch_layout(authored.controls())?; // Loads any saved settings first.
//! let drawing = authored.resolve(input.touch_settings(), [390., 844.])?;
//! assert_eq!(drawing.controls().len(), 1);
//! assert_eq!(input.touch_settings(), TouchLayoutSettings::default());
//! // Paint drawing.controls()[0].region in absolute logical screen coordinates.
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use super::touch::{Control, MAX_TOUCH_CONTROLS, Output, Region, TouchLayoutSettings};
use crate::{
    Rect, Vec2,
    canvas::Canvas2D,
    ui::{UiRect, Viewport},
};

/// Explicit saved-layout identity. Visible slots must be a prefix in slot order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TouchSlot {
    pub slot: usize,
    /// `None` omits this slot from drawing and input; only a suffix may be hidden.
    pub control: Option<Control>,
}
impl TouchSlot {
    /// Adapt a UI rectangle in the supplied viewport's local units.
    /// Geometry is checked by the layout constructor, including zero extents.
    pub fn rect(slot: usize, rect: UiRect, output: Output) -> Self {
        Self {
            slot,
            control: Some(Control {
                region: Region::Rect(Rect::new(
                    rect.min[0],
                    rect.min[1],
                    rect.size[0],
                    rect.size[1],
                )),
                output,
            }),
        }
    }

    /// Hide a trailing authored slot without changing earlier saved indexes.
    pub fn hidden(slot: usize) -> Self {
        Self {
            slot,
            control: None,
        }
    }
}

/// Minimum **screen logical** extent after projection, never a silent resize.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum MinimumTouchSize {
    /// Preserve existing authored defaults, including small controls.
    #[default]
    Preserve,
    /// Reject rectangles with either side, or circles with diameter, below this
    /// positive finite size. Look regions are also checked. Consumers can reflow
    /// or hide the whole layout on error; this adapter does not choose a policy.
    Require(f32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TouchGeometryError {
    Capacity,
    /// Slot IDs must equal their slice indexes, including hidden suffix slots.
    SlotOrder,
    HiddenGap,
    InvalidGeometry,
    BelowMinimum,
}
impl std::fmt::Display for TouchGeometryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Capacity => "touch geometry exceeds 16 slots",
            Self::SlotOrder => "touch geometry must retain authored slot order",
            Self::HiddenGap => "PlayerInput cannot hide a slot before a visible slot",
            Self::InvalidGeometry => "invalid touch geometry, output, transform or settings",
            Self::BelowMinimum => "touch geometry is below the requested logical minimum",
        })
    }
}
impl std::error::Error for TouchGeometryError {}

/// Fixed-capacity projected controls; construction/resolution allocate nothing.
/// Rebuild from authored slots on resize; do not feed resolved geometry back in.
/// Empty layouts release existing captures when installed through PlayerInput.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TouchLayout {
    controls: [Control; MAX_TOUCH_CONTROLS],
    len: usize,
    minimum: MinimumTouchSize,
}
impl TouchLayout {
    /// Project viewport-local shapes, including [`TouchSlot::rect`] UI rectangles.
    /// Bounds are not a clip: shapes extending outside the viewport stay intact.
    /// A viewport with either displayed extent below one logical pixel suspends
    /// the entire layout, after validating slot order and authored geometry.
    ///
    /// ```
    /// use ferravex_render::{input::{touch::Output, touch_layout::*}, ui::{UiRect, Viewport}};
    /// let v = Viewport::fit([800., 600.], [400., 200.], [0.; 4]).unwrap();
    /// let slots = [TouchSlot::rect(0, UiRect { min: [10., 20.], size: [30., 40.] }, Output::Button(7))];
    /// let layout = TouchLayout::from_viewport(v, &slots, MinimumTouchSize::Require(44.))?;
    /// assert_eq!(layout.controls()[0].region, ferravex_render::input::touch::Region::Rect(
    ///     ferravex_render::Rect::new(20., 140., 60., 80.)));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn from_viewport(
        viewport: Viewport,
        slots: &[TouchSlot],
        minimum: MinimumTouchSize,
    ) -> Result<Self, TouchGeometryError> {
        let pixels = viewport.pixels();
        Self::project(
            slots,
            minimum,
            Vec2::from(pixels.min),
            viewport.scale(),
            pixels.valid() && viewport.bounds().valid(),
            pixels.size.into_iter().any(|n| n < 1.),
        )
    }

    /// Project canvas-world shapes using the same affine arithmetic as drawing.
    /// No crop is applied, preserving existing broad/background touch regions.
    /// Manually authored canvas fields are checked; zero/subpixel screen or
    /// displayed-world extents suspend input instead of leaving stale targets.
    ///
    /// ```
    /// use ferravex_render::{vec2, canvas::Canvas2D,
    ///     input::{touch::{Control, Output, Region}, touch_layout::*}};
    /// let canvas = Canvas2D::try_fit(vec2(400., 200.), vec2(800., 600.))?.unwrap();
    /// let slots = [TouchSlot { slot: 0, control: Some(Control {
    ///     region: Region::Circle { center: vec2(20., 30.), radius: 22. },
    ///     output: Output::Stick { x: 0, y: 1, radius: 16., floating: false },
    /// }) }];
    /// let layout = TouchLayout::from_canvas(canvas, &slots, MinimumTouchSize::Preserve)?;
    /// assert_eq!(layout.controls()[0].region,
    ///     Region::Circle { center: canvas.project(vec2(20., 30.)), radius: 44. });
    /// assert_eq!(layout.controls()[0].output,
    ///     Output::Stick { x: 0, y: 1, radius: 32., floating: false });
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn from_canvas(
        canvas: Canvas2D,
        slots: &[TouchSlot],
        minimum: MinimumTouchSize,
    ) -> Result<Self, TouchGeometryError> {
        Self::project(
            slots,
            minimum,
            canvas.offset,
            canvas.scale,
            canvas.size.is_finite()
                && canvas.size.min_element() > 0.
                && canvas.screen.is_finite()
                && canvas.screen.min_element() >= 0.
                && (canvas.size * canvas.scale).is_finite()
                && canvas.project(canvas.size).is_finite(),
            canvas.screen.min_element() < 1. || (canvas.size * canvas.scale).min_element() < 1.,
        )
    }

    /// Visible prefix, in unchanged authored slot order. Used for both drawing
    /// and installation; boundaries retain existing inclusive touch hit tests.
    /// Overlapping/shared edges resolve to the earliest slot, as in TouchSurface.
    pub fn controls(&self) -> &[Control] {
        &self.controls[..self.len]
    }

    /// Resolve the exact legacy PlayerInput customization for drawing. Supply
    /// current full-screen logical dimensions, not the safe-area size. Like
    /// PlayerInput, each dimension is floored at 64 before `settings.apply`.
    /// Safe offsets are already projected; `apply_in_safe_area` here would apply
    /// them twice. This method never mutates settings, channels or saved slots.
    /// The minimum is checked again after edits; errors preserve the authored
    /// value. Never install this result into PlayerInput a second time.
    pub fn resolve(
        &self,
        settings: TouchLayoutSettings,
        screen: [f32; 2],
    ) -> Result<Self, TouchGeometryError> {
        if !settings.valid() || screen.into_iter().any(|n| !n.is_finite() || n < 0.) {
            return Err(TouchGeometryError::InvalidGeometry);
        }
        let mut resolved = *self;
        for (slot, control) in resolved.controls[..self.len].iter_mut().enumerate() {
            *control = settings
                .apply(*control, slot, Vec2::from(screen).max(Vec2::splat(64.)))
                .map_err(|_| TouchGeometryError::InvalidGeometry)?;
            check_control(*control, self.minimum)?;
        }
        Ok(resolved)
    }

    fn project(
        slots: &[TouchSlot],
        minimum: MinimumTouchSize,
        offset: Vec2,
        scale: f32,
        valid: bool,
        suspended: bool,
    ) -> Result<Self, TouchGeometryError> {
        if slots.len() > MAX_TOUCH_CONTROLS {
            return Err(TouchGeometryError::Capacity);
        }
        if !valid
            || !offset.is_finite()
            || !scale.is_finite()
            || scale <= 0.
            || matches!(minimum, MinimumTouchSize::Require(n) if !n.is_finite() || n <= 0.)
        {
            return Err(TouchGeometryError::InvalidGeometry);
        }
        let mut result = Self {
            controls: [Control {
                region: Region::Rect(Rect::new(0., 0., 1., 1.)),
                output: Output::Button(0),
            }; MAX_TOUCH_CONTROLS],
            len: 0,
            minimum,
        };
        let mut hidden = false;
        for (i, slot) in slots.iter().enumerate() {
            if slot.slot != i {
                return Err(TouchGeometryError::SlotOrder);
            }
            let Some(mut control) = slot.control else {
                hidden = true;
                continue;
            };
            if hidden {
                return Err(TouchGeometryError::HiddenGap);
            }
            check_control(control, MinimumTouchSize::Preserve)?;
            if suspended {
                continue;
            }
            control.region = match control.region {
                Region::Rect(r) => {
                    let p = offset + Vec2::new(r.x, r.y) * scale;
                    Region::Rect(Rect::new(p.x, p.y, r.w * scale, r.h * scale))
                }
                Region::Circle { center, radius } => Region::Circle {
                    center: offset + center * scale,
                    radius: radius * scale,
                },
            };
            if let Output::Stick { radius, .. } = &mut control.output {
                *radius *= scale;
            }
            check_control(control, minimum)?;
            result.controls[result.len] = control;
            result.len += 1;
        }
        Ok(result)
    }
}

fn check_control(control: Control, minimum: MinimumTouchSize) -> Result<(), TouchGeometryError> {
    // Reuse the existing touch contract rather than duplicate channel/shape limits.
    TouchLayoutSettings::default()
        .apply(control, 0, Vec2::splat(64.))
        .map_err(|_| TouchGeometryError::InvalidGeometry)?;
    if let MinimumTouchSize::Require(n) = minimum {
        let size = match control.region {
            Region::Rect(r) => r.w.min(r.h),
            Region::Circle { radius, .. } => 2. * radius,
        };
        if size < n {
            return Err(TouchGeometryError::BelowMinimum);
        }
    }
    Ok(())
}
