//! Saved logical touch-layout edits. Slots refer to stable authored controls.
use super::*;
use crate::persistence::{Document, SaveData, SaveError, Value};

/// Bounded diagnostics for an already resolved layout, indexed by authored slot.
///
/// Unused slots and the overlap diagonal are false. Overlaps are symmetric and
/// include look regions: an intentional background look region can overlap a
/// button, but layout order still determines which control captures a finger.
/// Touching edges count as overlaps because touch hit tests include boundaries.
/// No controls are moved, removed or reordered by diagnostics.
///
/// ```
/// use ferravex_math::{Rect, Vec2};
/// use ferravex_play::input::touch::{Control, Output, Region, TouchLayoutSettings};
/// let controls = [Control {
///     region: Region::Rect(Rect::new(0., 0., 60., 60.)),
///     output: Output::Button(0),
/// }; 2];
/// let report = TouchLayoutSettings::diagnose_layout(
///     &controls, Vec2::splat(200.), Rect::new(10., 10., 180., 180.),
/// )?;
/// assert!(report.out_of_bounds[0]);
/// assert!(report.overlaps[0][1] && report.overlaps[1][0]);
/// assert!(!report.is_clear());
/// # Ok::<(), ferravex_play::input::InputError>(())
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TouchLayoutDiagnostics {
    /// True when any part of a control's hit region lies outside the safe area.
    pub out_of_bounds: [bool; MAX_TOUCH_CONTROLS],
    /// True when two distinct controls share any point in their hit regions.
    pub overlaps: [[bool; MAX_TOUCH_CONTROLS]; MAX_TOUCH_CONTROLS],
}
impl TouchLayoutDiagnostics {
    /// Whether every region fits and no distinct regions share a hit point.
    /// See the type's example for inspecting individual slots.
    pub fn is_clear(&self) -> bool {
        !self.out_of_bounds.iter().any(|&outside| outside)
            && !self.overlaps.iter().flatten().any(|&overlap| overlap)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TouchEdit {
    pub offset: Vec2,
    pub scale: f32,
}
impl Default for TouchEdit {
    fn default() -> Self {
        Self {
            offset: Vec2::ZERO,
            scale: 1.,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TouchLayoutSettings {
    pub mirrored: bool,
    pub scale: f32,
    pub opacity: f32,
    pub edits: [TouchEdit; MAX_TOUCH_CONTROLS],
}
impl Default for TouchLayoutSettings {
    fn default() -> Self {
        Self {
            mirrored: false,
            scale: 1.,
            opacity: 0.65,
            edits: [TouchEdit::default(); MAX_TOUCH_CONTROLS],
        }
    }
}
impl TouchLayoutSettings {
    pub fn valid(self) -> bool {
        self.scale.is_finite()
            && (0.6..=1.8).contains(&self.scale)
            && self.opacity.is_finite()
            && (0.2..=1.).contains(&self.opacity)
            && self.edits.iter().all(|e| {
                e.offset.is_finite()
                    && e.offset.abs().max_element() <= 0.4
                    && e.scale.is_finite()
                    && (0.5..=2.).contains(&e.scale)
            })
    }
    pub fn apply(self, mut c: Control, slot: usize, viewport: Vec2) -> Result<Control, InputError> {
        if !self.valid()
            || slot >= MAX_TOUCH_CONTROLS
            || !c.valid()
            || !viewport.is_finite()
            || viewport.min_element() < 64.
        {
            return Err(InputError::InvalidTouch);
        }
        if self == Self::default() {
            return Ok(c);
        }
        if let Output::Look { .. } = c.output {
            if self.mirrored {
                c.region = match c.region {
                    Region::Rect(r) => {
                        Region::Rect(Rect::new(viewport.x - r.x - r.w, r.y, r.w, r.h))
                    }
                    Region::Circle { center, radius } => Region::Circle {
                        center: Vec2::new(viewport.x - center.x, center.y),
                        radius,
                    },
                };
            }
            return Ok(c);
        }
        let edit = self.edits[slot];
        let scale = self.scale * edit.scale;
        let mut center = c.region.center();
        if self.mirrored {
            center.x = viewport.x - center.x;
        }
        center += edit.offset * viewport;
        c.region = match c.region {
            Region::Circle { radius, .. } => {
                let radius = (radius * scale)
                    .max(22.)
                    .min(viewport.min_element() * 0.5 - 8.);
                let center = center.clamp(
                    Vec2::splat(radius + 8.),
                    viewport - Vec2::splat(radius + 8.),
                );
                Region::Circle { center, radius }
            }
            Region::Rect(r) => {
                let size = (Vec2::new(r.w, r.h) * scale)
                    .max(Vec2::splat(44.))
                    .min(viewport - Vec2::splat(16.));
                let half = size * 0.5;
                let center =
                    center.clamp(half + Vec2::splat(8.), viewport - half - Vec2::splat(8.));
                Region::Rect(Rect::new(
                    center.x - half.x,
                    center.y - half.y,
                    size.x,
                    size.y,
                ))
            }
        };
        if let Output::Stick { radius, .. } = &mut c.output {
            *radius = (*radius * scale).clamp(8., viewport.min_element() * 0.5 - 8.);
        }
        Ok(c)
    }

    /// Apply saved edits inside a checked safe rectangle in logical pixels.
    ///
    /// `c` must be the authored control for `viewport`, before any saved edits
    /// or safe-area adaptation. Its center is mapped by `center / viewport`
    /// into `safe_area`; mirroring and normalized edit offsets are relative to
    /// that area. Buttons and sticks keep their logical-pixel size, scaled by
    /// the saved settings, with the same 44-pixel minimum and 8-pixel margin as
    /// customized [`Self::apply`] layouts. Oversized controls shrink to fit.
    /// Look regions scale with the safe area and fit without that margin;
    /// like `apply`, they ignore per-slot edits and size settings.
    ///
    /// Always recompute from authored controls when resizing. Neither saved
    /// offsets nor authored controls are mutated, so clamping in a small area
    /// cannot accumulate drift after resizing back. Geometry never depends on
    /// opacity or edits to other slots. Unlike legacy `apply`, default settings
    /// also fit controls. Installing a changed layout in [`TouchSurface`] still
    /// releases existing captures; this method does not remap active fingers.
    ///
    /// Returns [`InputError::InvalidTouch`] for invalid settings/slot/control,
    /// non-finite or out-of-range geometry, or a safe area outside the viewport.
    /// Each viewport dimension must be in `64..=1_000_000`; each safe-area
    /// dimension must be at least 64. Resolved controls must also satisfy the
    /// existing touch geometry limits. Errors leave caller-owned state intact.
    ///
    /// ```
    /// use ferravex_math::{Rect, Vec2};
    /// use ferravex_play::input::touch::{Control, Output, Region, TouchLayoutSettings};
    /// let mut settings = TouchLayoutSettings::default();
    /// settings.edits[0].offset = Vec2::new(0.1, 0.);
    /// let authored = Control {
    ///     region: Region::Circle { center: Vec2::new(100., 100.), radius: 24. },
    ///     output: Output::Button(0),
    /// };
    /// let viewport = Vec2::new(400., 200.);
    /// let safe = Rect::new(20., 10., 360., 180.);
    /// let resolved = settings.apply_in_safe_area(authored, 0, viewport, safe)?;
    /// assert_eq!(resolved.region, Region::Circle {
    ///     center: Vec2::new(146., 100.), radius: 24.,
    /// });
    /// assert!(TouchLayoutSettings::diagnose_layout(&[resolved], viewport, safe)?.is_clear());
    /// assert_eq!(settings.edits[0].offset, Vec2::new(0.1, 0.));
    /// # Ok::<(), ferravex_play::input::InputError>(())
    /// ```
    pub fn apply_in_safe_area(
        self,
        mut c: Control,
        slot: usize,
        viewport: Vec2,
        safe_area: Rect,
    ) -> Result<Control, InputError> {
        check_safe_area(viewport, safe_area)?;
        if !self.valid() || slot >= MAX_TOUCH_CONTROLS || !c.valid() {
            return Err(InputError::InvalidTouch);
        }
        let available = Vec2::new(safe_area.w, safe_area.h);
        let origin = Vec2::new(safe_area.x, safe_area.y);
        let look = matches!(c.output, Output::Look { .. });
        let edit = self.edits[slot];
        let mut normalized = c.region.center() / viewport;
        if self.mirrored {
            normalized.x = 1. - normalized.x;
        }
        if !look {
            normalized += edit.offset;
        }
        let center = origin + normalized * available;
        let margin = if look { 0. } else { 8. };
        let scale = self.scale * edit.scale;
        let ratio = available / viewport;
        c.region = match c.region {
            Region::Circle { radius, .. } => {
                let radius = if look {
                    radius * ratio.min_element()
                } else {
                    (radius * scale).max(22.)
                };
                fit_circle(center, radius, safe_area, margin)
            }
            Region::Rect(r) => {
                let size = Vec2::new(r.w, r.h);
                let size = if look {
                    size * ratio
                } else {
                    (size * scale).max(Vec2::splat(44.))
                };
                let (x, w) = fit_axis(center.x, size.x, safe_area.x, safe_area.w, margin);
                let (y, h) = fit_axis(center.y, size.y, safe_area.y, safe_area.h, margin);
                Region::Rect(Rect::new(x, y, w, h))
            }
        };
        if let Output::Stick { radius, .. } = &mut c.output {
            *radius = (*radius * scale).clamp(8., available.min_element() * 0.5 - 8.);
        }
        if !c.valid() || !region_inside(c.region, safe_area) {
            return Err(InputError::InvalidTouch);
        }
        Ok(c)
    }

    /// Check resolved controls against a safe area without applying settings.
    ///
    /// Reports all out-of-bounds regions and every intersecting pair, including
    /// pairs outside the safe area. Shape tests distinguish circles from their
    /// bounding boxes and include tangencies, matching inclusive touch regions.
    /// Work is bounded by 16 controls / 120 pairs and allocates no heap storage.
    /// Invalid controls or safe geometry return [`InputError::InvalidTouch`];
    /// more than [`MAX_TOUCH_CONTROLS`] returns [`InputError::TouchCapacity`].
    /// Safe-area limits are those of [`Self::apply_in_safe_area`].
    ///
    /// ```
    /// use ferravex_math::{Rect, Vec2};
    /// use ferravex_play::input::touch::{Control, Output, Region, TouchLayoutSettings};
    /// let controls = [Control {
    ///     region: Region::Circle { center: Vec2::splat(50.), radius: 24. },
    ///     output: Output::Button(0),
    /// }];
    /// let report = TouchLayoutSettings::diagnose_layout(
    ///     &controls, Vec2::splat(100.), Rect::new(0., 0., 100., 100.),
    /// )?;
    /// assert!(report.is_clear());
    /// # Ok::<(), ferravex_play::input::InputError>(())
    /// ```
    pub fn diagnose_layout(
        controls: &[Control],
        viewport: Vec2,
        safe_area: Rect,
    ) -> Result<TouchLayoutDiagnostics, InputError> {
        if controls.len() > MAX_TOUCH_CONTROLS {
            return Err(InputError::TouchCapacity);
        }
        check_safe_area(viewport, safe_area)?;
        if controls.iter().any(|c| !c.valid()) {
            return Err(InputError::InvalidTouch);
        }
        let mut report = TouchLayoutDiagnostics::default();
        for (slot, control) in controls.iter().enumerate() {
            report.out_of_bounds[slot] = !region_inside(control.region, safe_area);
            for (other, previous) in controls[..slot].iter().enumerate() {
                let overlap = regions_overlap(control.region, previous.region);
                report.overlaps[slot][other] = overlap;
                report.overlaps[other][slot] = overlap;
            }
        }
        Ok(report)
    }
}

fn check_safe_area(viewport: Vec2, safe: Rect) -> Result<(), InputError> {
    if !viewport.is_finite()
        || viewport.min_element() < 64.
        || viewport.max_element() > 1e6
        || ![safe.x, safe.y, safe.w, safe.h]
            .into_iter()
            .all(f32::is_finite)
        || safe.x < 0.
        || safe.y < 0.
        || safe.w < 64.
        || safe.h < 64.
        || f64::from(safe.x) + f64::from(safe.w) > f64::from(viewport.x)
        || f64::from(safe.y) + f64::from(safe.h) > f64::from(viewport.y)
    {
        return Err(InputError::InvalidTouch);
    }
    Ok(())
}

fn region_inside(region: Region, safe: Rect) -> bool {
    let (min, max) = match region {
        Region::Circle { center, radius } => (
            [
                f64::from(center.x) - f64::from(radius),
                f64::from(center.y) - f64::from(radius),
            ],
            [
                f64::from(center.x) + f64::from(radius),
                f64::from(center.y) + f64::from(radius),
            ],
        ),
        Region::Rect(r) => (
            [f64::from(r.x), f64::from(r.y)],
            [
                f64::from(r.x) + f64::from(r.w),
                f64::from(r.y) + f64::from(r.h),
            ],
        ),
    };
    min[0] >= f64::from(safe.x)
        && min[1] >= f64::from(safe.y)
        && max[0] <= f64::from(safe.x) + f64::from(safe.w)
        && max[1] <= f64::from(safe.y) + f64::from(safe.h)
}

// Fit in f64 so fractional origins cannot invert equal f32 clamp endpoints.
// Round sizes inward after returning to f32, including any center rounding.
fn round_size_down(value: f64) -> f32 {
    let rounded = value as f32;
    if f64::from(rounded) > value {
        rounded.next_down()
    } else {
        rounded
    }
}

fn fit_axis(center: f32, size: f32, origin: f32, extent: f32, margin: f32) -> (f32, f32) {
    let low = f64::from(origin) + f64::from(margin);
    let high = f64::from(origin) + f64::from(extent) - f64::from(margin);
    let size = f64::from(size).min(high - low);
    let mut start = (f64::from(center) - size * 0.5).clamp(low, high - size) as f32;
    if f64::from(start) < low {
        start = start.next_up();
    }
    (start, round_size_down(size.min(high - f64::from(start))))
}

fn fit_circle(center: Vec2, radius: f32, safe: Rect, margin: f32) -> Region {
    let low = [
        f64::from(safe.x) + f64::from(margin),
        f64::from(safe.y) + f64::from(margin),
    ];
    let high = [
        f64::from(safe.x) + f64::from(safe.w) - f64::from(margin),
        f64::from(safe.y) + f64::from(safe.h) - f64::from(margin),
    ];
    let radius = f64::from(radius).min((high[0] - low[0]).min(high[1] - low[1]) * 0.5);
    let center = Vec2::new(
        f64::from(center.x).clamp(low[0] + radius, high[0] - radius) as f32,
        f64::from(center.y).clamp(low[1] + radius, high[1] - radius) as f32,
    );
    let radius = radius
        .min(f64::from(center.x) - low[0])
        .min(high[0] - f64::from(center.x))
        .min(f64::from(center.y) - low[1])
        .min(high[1] - f64::from(center.y));
    Region::Circle {
        center,
        radius: round_size_down(radius),
    }
}

fn regions_overlap(a: Region, b: Region) -> bool {
    match (a, b) {
        (Region::Rect(a), Region::Rect(b)) => {
            f64::from(a.x) <= f64::from(b.x) + f64::from(b.w)
                && f64::from(b.x) <= f64::from(a.x) + f64::from(a.w)
                && f64::from(a.y) <= f64::from(b.y) + f64::from(b.h)
                && f64::from(b.y) <= f64::from(a.y) + f64::from(a.h)
        }
        (
            Region::Circle {
                center: a,
                radius: ar,
            },
            Region::Circle {
                center: b,
                radius: br,
            },
        ) => distance_within(a, b, f64::from(ar) + f64::from(br)),
        (Region::Circle { center, radius }, Region::Rect(r))
        | (Region::Rect(r), Region::Circle { center, radius }) => {
            let x = f64::from(center.x);
            let y = f64::from(center.y);
            let dx = x - x.clamp(f64::from(r.x), f64::from(r.x) + f64::from(r.w));
            let dy = y - y.clamp(f64::from(r.y), f64::from(r.y) + f64::from(r.h));
            dx * dx + dy * dy <= f64::from(radius) * f64::from(radius)
        }
    }
}

fn distance_within(a: Vec2, b: Vec2, radius: f64) -> bool {
    let x = f64::from(a.x) - f64::from(b.x);
    let y = f64::from(a.y) - f64::from(b.y);
    x * x + y * y <= radius * radius
}

impl SaveData for TouchLayoutSettings {
    const SCHEMA: u64 = 0x4658544f55434831;
    const VERSION: u32 = 1;
    const MAX_BYTES: usize = 512;
    fn write(&self, d: &mut Document) -> Result<(), SaveError> {
        if !self.valid() {
            return Err(SaveError::InvalidData);
        }
        let mut b = vec![u8::from(self.mirrored)];
        for v in [self.scale, self.opacity] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        for e in self.edits {
            for v in [e.offset.x, e.offset.y, e.scale] {
                b.extend_from_slice(&v.to_le_bytes());
            }
        }
        d.set("layout", Value::Bytes(b))
    }
    fn read(d: &Document) -> Result<Self, SaveError> {
        let b = d.bytes("layout")?;
        if b.len() != 201 || b[0] > 1 {
            return Err(SaveError::InvalidData);
        }
        let mut values = b[1..]
            .as_chunks::<4>()
            .0
            .iter()
            .map(|w| f32::from_le_bytes(*w));
        let mut result = Self {
            mirrored: b[0] != 0,
            scale: values.next().unwrap(),
            opacity: values.next().unwrap(),
            ..Default::default()
        };
        for e in &mut result.edits {
            *e = TouchEdit {
                offset: Vec2::new(values.next().unwrap(), values.next().unwrap()),
                scale: values.next().unwrap(),
            };
        }
        if !result.valid() {
            return Err(SaveError::InvalidData);
        }
        Ok(result)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn button(region: Region) -> Control {
        Control {
            region,
            output: Output::Button(0),
        }
    }

    fn circle(x: f32, y: f32, radius: f32) -> Region {
        Region::Circle {
            center: Vec2::new(x, y),
            radius,
        }
    }

    fn close(a: Vec2, b: Vec2) {
        assert!((a - b).abs().max_element() < 0.0001, "{a:?} != {b:?}");
    }

    #[test]
    fn safe_area_preserves_normalized_edits_across_resize_and_clamping() {
        let mut settings = TouchLayoutSettings::default();
        settings.edits[0].offset = Vec2::new(0.1, -0.1);
        let saved = settings.encode(9).unwrap();
        let resolve = |viewport: Vec2, safe: Rect| {
            let authored = button(Region::Circle {
                center: viewport * Vec2::new(0.25, 0.75),
                radius: 30.,
            });
            settings
                .apply_in_safe_area(authored, 0, viewport, safe)
                .unwrap()
        };
        let viewport = Vec2::new(400., 300.);
        let safe = Rect::new(20., 30., 360., 240.);
        let original = resolve(viewport, safe);
        close(original.region.center(), Vec2::new(146., 186.));
        let larger = resolve(viewport * 2., Rect::new(20., 30., 760., 540.));
        close(larger.region.center(), Vec2::new(286., 381.));
        assert_eq!(larger.region, circle(286., 381., 30.));
        let tiny = resolve(Vec2::splat(80.), Rect::new(8., 8., 64., 64.));
        assert_eq!(tiny.region, circle(40., 40., 24.));
        assert_eq!(resolve(viewport, safe), original);
        assert_eq!(settings.encode(9).unwrap(), saved);
        assert_eq!(TouchLayoutSettings::VERSION, 1);
    }

    #[test]
    fn mirroring_and_stick_radius_use_the_asymmetric_safe_area() {
        let settings = TouchLayoutSettings {
            mirrored: true,
            scale: 1.5,
            ..Default::default()
        };
        let c = Control {
            region: circle(100., 100., 24.),
            output: Output::Stick {
                x: 2,
                y: 3,
                radius: 20.,
                floating: true,
            },
        };
        let adapted = settings
            .apply_in_safe_area(c, 0, Vec2::new(400., 200.), Rect::new(40., 0., 320., 200.))
            .unwrap();
        assert_eq!(adapted.region, circle(280., 100., 36.));
        assert_eq!(
            adapted.output,
            Output::Stick {
                x: 2,
                y: 3,
                radius: 30.,
                floating: true,
            }
        );
    }

    #[test]
    fn look_regions_map_into_safe_area_without_button_minimums_or_edits() {
        let viewport = Vec2::new(400., 200.);
        let safe = Rect::new(40., 10., 320., 160.);
        let c = Control {
            region: Region::Rect(Rect::new(200., 0., 200., 200.)),
            output: Output::Look { x: 0, y: 1 },
        };
        let mut settings = TouchLayoutSettings {
            mirrored: true,
            scale: 1.8,
            ..Default::default()
        };
        settings.edits[0] = TouchEdit {
            offset: Vec2::splat(0.4),
            scale: 2.,
        };
        let adapted = settings.apply_in_safe_area(c, 0, viewport, safe).unwrap();
        assert_eq!(
            adapted.region,
            Region::Rect(Rect::new(40., 10., 160., 160.))
        );
        assert_eq!(adapted.output, c.output);
        let small = Control {
            region: circle(100., 100., 10.),
            ..c
        };
        assert_eq!(
            settings
                .apply_in_safe_area(small, 0, viewport, safe)
                .unwrap()
                .region,
            circle(280., 90., 8.)
        );
    }

    #[test]
    fn opacity_and_other_slots_never_move_controls_or_drop_captures() {
        let viewport = Vec2::new(400., 200.);
        let safe = Rect::new(20., 10., 360., 180.);
        let c = button(circle(100., 100., 10.));
        let mut settings = TouchLayoutSettings::default();
        let original = settings.apply_in_safe_area(c, 0, viewport, safe).unwrap();
        let mut surface = TouchSurface::default();
        let mut map = ActionMap::default();
        surface.set_layout(&[original], &mut map).unwrap();
        surface
            .handle(
                Event {
                    id: 1,
                    position: original.region.center(),
                    phase: Phase::Start,
                },
                &mut map,
            )
            .unwrap();
        settings.opacity = 0.2;
        settings.edits[1].scale = 2.;
        let restyled = settings.apply_in_safe_area(c, 0, viewport, safe).unwrap();
        assert_eq!(restyled, original);
        surface.set_layout(&[restyled], &mut map).unwrap();
        assert_eq!(surface.active(), 1);
        assert_eq!(map.physical_value(InputCode::Touch(0)), 1.);

        let resized = settings
            .apply_in_safe_area(
                button(circle(100.25, 100., 10.)),
                0,
                Vec2::new(401., 200.),
                Rect::new(20., 10., 361., 180.),
            )
            .unwrap();
        close(
            resized.region.center() - original.region.center(),
            Vec2::new(0.25, 0.),
        );
        surface.set_layout(&[resized], &mut map).unwrap();
        assert_eq!(surface.active(), 0);
        assert_eq!(map.physical_value(InputCode::Touch(0)), 0.);
    }

    #[test]
    fn checked_safe_area_rejects_invalid_geometry_settings_and_output() {
        let settings = TouchLayoutSettings::default();
        let c = button(circle(50., 50., 24.));
        let viewport = Vec2::splat(200.);
        let safe = Rect::new(10., 10., 180., 180.);
        for viewport in [
            Vec2::new(f32::NAN, 200.),
            Vec2::new(200., f32::INFINITY),
            Vec2::new(63., 200.),
            Vec2::new(-200., 200.),
            Vec2::splat(f32::MAX),
            Vec2::splat(1_000_001.),
        ] {
            assert_eq!(
                settings.apply_in_safe_area(c, 0, viewport, safe),
                Err(InputError::InvalidTouch)
            );
            assert_eq!(
                TouchLayoutSettings::diagnose_layout(&[c], viewport, safe),
                Err(InputError::InvalidTouch)
            );
        }
        for safe in [
            Rect::new(f32::NAN, 0., 100., 100.),
            Rect::new(0., f32::INFINITY, 100., 100.),
            Rect::new(0., 0., f32::INFINITY, 100.),
            Rect::new(0., 0., 100., f32::NAN),
            Rect::new(-1., 0., 100., 100.),
            Rect::new(0., -1., 100., 100.),
            Rect::new(0., 0., 0., 100.),
            Rect::new(0., 0., 100., 63.),
            Rect::new(101., 0., 100., 100.),
            Rect::new(0., 101., 100., 100.),
        ] {
            assert_eq!(
                settings.apply_in_safe_area(c, 0, viewport, safe),
                Err(InputError::InvalidTouch)
            );
            assert_eq!(
                TouchLayoutSettings::diagnose_layout(&[c], viewport, safe),
                Err(InputError::InvalidTouch)
            );
        }
        assert_eq!(
            settings.apply_in_safe_area(c, MAX_TOUCH_CONTROLS, viewport, safe),
            Err(InputError::InvalidTouch)
        );
        assert_eq!(
            TouchLayoutSettings {
                scale: f32::NAN,
                ..settings
            }
            .apply_in_safe_area(c, 0, viewport, safe),
            Err(InputError::InvalidTouch)
        );
        let invalid = Control {
            output: Output::Button(64),
            ..c
        };
        assert_eq!(
            settings.apply_in_safe_area(invalid, 0, viewport, safe),
            Err(InputError::InvalidTouch)
        );
        assert_eq!(
            TouchLayoutSettings::diagnose_layout(&[invalid], viewport, safe),
            Err(InputError::InvalidTouch)
        );
        // Finite inputs can still produce a radius beyond TouchSurface's limit.
        assert_eq!(
            TouchLayoutSettings {
                scale: 1.8,
                ..settings
            }
            .apply_in_safe_area(
                button(circle(500_000., 500_000., 100_000.)),
                0,
                Vec2::splat(1_000_000.),
                Rect::new(0., 0., 1_000_000., 1_000_000.),
            ),
            Err(InputError::InvalidTouch)
        );
    }

    #[test]
    fn fits_buttons_rectangles_and_sticks_at_the_minimum_safe_size() {
        let settings = TouchLayoutSettings::default();
        let viewport = Vec2::splat(200.);
        let safe = Rect::new(20., 30., 64., 64.);
        for region in [
            circle(-50., 250., 100.),
            Region::Rect(Rect::new(-50., -50., 400., 400.)),
        ] {
            let c = Control {
                region,
                output: Output::Stick {
                    x: 0,
                    y: 1,
                    radius: 200.,
                    floating: false,
                },
            };
            let adapted = settings.apply_in_safe_area(c, 0, viewport, safe).unwrap();
            assert!(region_inside(adapted.region, safe));
            close(adapted.region.center(), Vec2::new(52., 62.));
            assert!(matches!(adapted.output, Output::Stick { radius: 24., .. }));
            assert!(
                TouchLayoutSettings::diagnose_layout(&[adapted], viewport, safe)
                    .unwrap()
                    .is_clear()
            );
        }
    }

    #[test]
    fn fractional_safe_edges_fit_without_panics_or_spurious_bounds_errors() {
        let settings = TouchLayoutSettings::default();
        let viewport = Vec2::splat(1_000_000.);
        for origin in [0.1, 0.3, 1.1, 17.3, 100_000.1] {
            for size in [64.1, 100.3, 240.7] {
                let safe = Rect::new(origin, origin, size, size);
                for region in [
                    Region::Rect(Rect::new(0., 0., 1_000_000., 1_000_000.)),
                    circle(500_000., 500_000., 100_000.),
                ] {
                    for output in [Output::Look { x: 0, y: 1 }, Output::Button(0)] {
                        let authored = Control { region, output };
                        let adapted = settings
                            .apply_in_safe_area(authored, 0, viewport, safe)
                            .unwrap();
                        assert!(
                            TouchLayoutSettings::diagnose_layout(&[adapted], viewport, safe)
                                .unwrap()
                                .is_clear()
                        );
                    }
                }
            }
        }
        // A look circle filling the safe area has equal clamp endpoints.
        let adapted = settings
            .apply_in_safe_area(
                Control {
                    region: circle(100., 100., 100.),
                    output: Output::Look { x: 0, y: 1 },
                },
                0,
                Vec2::splat(200.),
                Rect::new(17.3, 17.3, 64.1, 64.1),
            )
            .unwrap();
        assert!(region_inside(
            adapted.region,
            Rect::new(17.3, 17.3, 64.1, 64.1)
        ));
    }

    #[test]
    fn diagnostics_keep_small_extents_at_large_coordinates() {
        let viewport = Vec2::splat(1_000_000.);
        let safe = Rect::new(500_000., 500_000., 64., 64.);
        let report = TouchLayoutSettings::diagnose_layout(
            &[button(circle(500_000., 500_000., 0.001))],
            viewport,
            safe,
        )
        .unwrap();
        assert!(report.out_of_bounds[0]);
        let report = TouchLayoutSettings::diagnose_layout(
            &[
                button(Region::Rect(Rect::new(500_000., 500_001., 0.005, 1.))),
                button(circle(500_000.03, 500_001., 0.03)),
            ],
            viewport,
            safe,
        )
        .unwrap();
        assert!(report.overlaps[0][1]);
    }

    #[test]
    fn diagnostics_use_real_shapes_and_inclusive_edges() {
        let viewport = Vec2::splat(400.);
        let safe = Rect::new(0., 0., 400., 400.);
        let overlap = |a, b| {
            let report =
                TouchLayoutSettings::diagnose_layout(&[button(a), button(b)], viewport, safe)
                    .unwrap();
            assert_eq!(report.overlaps[0][1], report.overlaps[1][0]);
            assert!(!report.overlaps[0][0] && !report.overlaps[1][1]);
            report.overlaps[0][1]
        };
        assert!(!overlap(circle(50., 50., 10.), circle(65., 65., 10.)));
        assert!(overlap(circle(50., 50., 10.), circle(70., 50., 10.)));
        assert!(overlap(circle(50., 50., 20.), circle(50., 50., 1.)));
        let rect = Region::Rect(Rect::new(100., 100., 40., 40.));
        assert!(!overlap(rect, circle(92., 92., 10.)));
        assert!(overlap(rect, circle(94., 92., 10.))); // 6-8-10 corner tangency.
        assert!(overlap(circle(90., 120., 10.), rect));
        assert!(overlap(rect, circle(120., 120., 1.)));
        assert!(overlap(rect, Region::Rect(Rect::new(140., 140., 20., 20.))));
        assert!(!overlap(
            rect,
            Region::Rect(Rect::new(140.5, 100., 20., 20.))
        ));
    }

    #[test]
    fn diagnostics_report_full_extents_without_changing_priority_or_controls() {
        let viewport = Vec2::splat(200.);
        let safe = Rect::new(10., 10., 180., 180.);
        let controls = [
            button(circle(20., 30., 11.)),
            button(Region::Rect(Rect::new(10., 10., 180., 180.))),
            Control {
                region: circle(100., 100., 10.),
                output: Output::Look { x: 0, y: 1 },
            },
            button(Region::Rect(Rect::new(189., 30., 2., 2.))),
        ];
        let before = controls;
        let report = TouchLayoutSettings::diagnose_layout(&controls, viewport, safe).unwrap();
        assert_eq!(&report.out_of_bounds[..4], &[true, false, false, true]);
        assert!(report.overlaps[0][1] && report.overlaps[1][2] && report.overlaps[1][3]);
        assert!(!report.overlaps[0][2]);
        assert!(report.out_of_bounds[4..].iter().all(|&outside| !outside));
        assert!(
            report.overlaps[4..]
                .iter()
                .flatten()
                .all(|&overlap| !overlap)
        );
        assert_eq!(controls, before);
    }

    #[test]
    fn diagnostic_capacity_is_bounded_and_empty_layout_is_clear() {
        let viewport = Vec2::splat(200.);
        let safe = Rect::new(0., 0., 200., 200.);
        assert!(
            TouchLayoutSettings::diagnose_layout(&[], viewport, safe)
                .unwrap()
                .is_clear()
        );
        let controls = [button(circle(50., 50., 24.)); MAX_TOUCH_CONTROLS];
        let report = TouchLayoutSettings::diagnose_layout(&controls, viewport, safe).unwrap();
        assert_eq!(
            report
                .overlaps
                .iter()
                .flatten()
                .filter(|&&value| value)
                .count(),
            240
        );
        assert_eq!(
            TouchLayoutSettings::diagnose_layout(
                &[controls[0]; MAX_TOUCH_CONTROLS + 1],
                viewport,
                safe
            ),
            Err(InputError::TouchCapacity)
        );
    }

    #[test]
    fn edits_roundtrip_mirror_resize_and_stay_visible() {
        let mut p = TouchLayoutSettings {
            mirrored: true,
            scale: 1.5,
            ..Default::default()
        };
        p.edits[0].offset = Vec2::new(0.4, 0.4);
        assert_eq!(
            TouchLayoutSettings::decode(&p.encode(4).unwrap()).unwrap(),
            (p, 4)
        );
        let c = Control {
            region: Region::Circle {
                center: Vec2::new(40., 140.),
                radius: 20.,
            },
            output: Output::Button(2),
        };
        let output = p.apply(c, 0, Vec2::new(320., 200.)).unwrap();
        if let Region::Circle { center, radius } = output.region {
            assert!(center.x + radius <= 312. && center.y + radius <= 192.);
            assert!(radius >= 22.);
        }
        assert_eq!(output.output, c.output);
        assert!(p.apply(c, 16, Vec2::new(320., 200.)).is_err());
    }
    #[test]
    fn defaults_preserve_authored_controls() {
        let c = Control {
            region: Region::Rect(Rect::new(0., 0., 100., 100.)),
            output: Output::Look { x: 0, y: 1 },
        };
        assert_eq!(
            TouchLayoutSettings::default()
                .apply(c, 0, Vec2::new(320., 200.))
                .unwrap(),
            c
        );
    }
}
