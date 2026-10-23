//! Bounded horizontal meter geometry in logical UI units, without rendering,
//! clocks, input, smoothing, allocation or gameplay availability decisions.
//!
//! Values are validated, never clamped. `Empty` explicitly means no capacity;
//! it differs from a known zero with a positive capacity and from `Unknown`.
//! A known zero denominator is rejected; callers must choose `Empty` or
//! `Unknown`. Geometry rounds to f32 logical units, not simulation precision.
//!
//! ```
//! use ferravex_play::ui::{Rect, meters::*};
//! let track = Rect { min: [-10., 2.5], size: [120., 8.] };
//! let meter = MeterGeometry::new(track,
//!     MeterValue::Known { current: 3., capacity: 6. })?;
//! assert_eq!(meter.fill().unwrap().size, [60., 8.]);
//! let mut markers = [track; MAX_METER_MARKERS];
//! assert_eq!(meter.markers_into(&[0., 0.25, 1.], 2., &mut markers)?, 3);
//! assert_eq!(markers[0].min[0], -10.);
//! assert_eq!(markers[2].min[0] + markers[2].size[0], 110.);
//! # Ok::<(), MeterError>(())
//! ```
use super::Rect;

/// Maximum explicit thresholds per continuous meter.
pub const MAX_METER_MARKERS: usize = 16;
/// Maximum segments per strip; callers must choose a smaller representation
/// explicitly when their telemetry exceeds this bound.
pub const MAX_METER_SEGMENTS: usize = 64;

/// Supplied telemetry. `Unknown` has no implied fraction; `Empty` fills zero.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MeterValue {
    Known { current: f64, capacity: f64 },
    Empty,
    Unknown,
}
impl MeterValue {
    /// Validate finite `0 <= current <= capacity`, `capacity > 0`. Returns
    /// `Some(0)` for Empty, `None` for Unknown. Underflow to a false zero rejects.
    pub fn fraction(self) -> Result<Option<f64>, MeterError> {
        match self {
            Self::Known { current, capacity } => {
                if !current.is_finite()
                    || !capacity.is_finite()
                    || capacity <= 0.
                    || current < 0.
                    || current > capacity
                {
                    return Err(MeterError::InvalidValue);
                }
                let fraction = current / capacity;
                if current > 0. && fraction == 0. {
                    return Err(MeterError::InvalidValue);
                }
                Ok(Some(fraction))
            }
            Self::Empty => Ok(Some(0.)),
            Self::Unknown => Ok(None),
        }
    }
}

/// Count telemetry, with no fractional/invented segments. Empty has zero cells;
/// Unknown may reserve zero through 64 cells, without marking any as accepted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SegmentValue {
    Known { filled: usize, capacity: usize },
    Empty,
    Unknown { capacity: usize },
}
impl SegmentValue {
    /// Validated cell count. Known requires positive capacity and filled <= it.
    pub fn count(self) -> Result<usize, MeterError> {
        let count = match self {
            Self::Known { filled, capacity } => {
                if capacity == 0 || filled > capacity {
                    return Err(MeterError::InvalidValue);
                }
                capacity
            }
            Self::Empty => 0,
            Self::Unknown { capacity } => capacity,
        };
        if count > MAX_METER_SEGMENTS {
            return Err(MeterError::Capacity);
        }
        Ok(count)
    }
}

/// Validation/capacity errors are reported before changing caller buffers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeterError {
    InvalidValue,
    InvalidGeometry,
    Capacity,
}
impl std::fmt::Display for MeterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidValue => "invalid meter telemetry",
            Self::InvalidGeometry => "invalid or unrepresentable meter geometry",
            Self::Capacity => "meter capacity exceeded",
        })
    }
}
impl std::error::Error for MeterError {}

/// Immutable continuous geometry. Zero extents are valid (and paint nothing);
/// positive extents must have representable edges. Negative origins are valid.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeterGeometry {
    bounds: Rect,
    value: MeterValue,
    fill: Option<Rect>,
}
impl MeterGeometry {
    pub fn new(bounds: Rect, value: MeterValue) -> Result<Self, MeterError> {
        valid(bounds)?;
        let fill = value
            .fraction()?
            .map(|fraction| {
                let width = (f64::from(bounds.size[0]) * fraction) as f32;
                if fraction > 0. && bounds.size[0] > 0. && width == 0. {
                    return Err(MeterError::InvalidGeometry);
                }
                let rect = Rect {
                    size: [width, bounds.size[1]],
                    ..bounds
                };
                inside(bounds, rect)?;
                Ok(rect)
            })
            .transpose()?;
        Ok(Self {
            bounds,
            value,
            fill,
        })
    }
    pub fn bounds(self) -> Rect {
        self.bounds
    }
    pub fn value(self) -> MeterValue {
        self.value
    }
    pub fn fill(self) -> Option<Rect> {
        self.fill
    }

    /// Write threshold rectangles inside the track. Fractions must be finite
    /// in 0..=1, width positive and no wider than the track. Marker centers follow
    /// fraction * width; endpoint marks move inward just enough to fit. No value
    /// is clamped. All validation precedes writes; unused output stays unchanged.
    /// Empty input succeeds even on a zero-width track (width still validates).
    pub fn markers_into(
        self,
        fractions: &[f64],
        width: f32,
        output: &mut [Rect],
    ) -> Result<usize, MeterError> {
        if fractions.len() > MAX_METER_MARKERS || output.len() < fractions.len() {
            return Err(MeterError::Capacity);
        }
        if !width.is_finite()
            || width <= 0.
            || (!fractions.is_empty() && width > self.bounds.size[0])
        {
            return Err(MeterError::InvalidGeometry);
        }
        let mut checked = [self.bounds; MAX_METER_MARKERS];
        for (i, &fraction) in fractions.iter().enumerate() {
            if !fraction.is_finite() || !(0. ..=1.).contains(&fraction) {
                return Err(MeterError::InvalidValue);
            }
            let offset = (f64::from(self.bounds.size[0]) * fraction - f64::from(width) / 2.)
                .max(0.)
                .min(f64::from(self.bounds.size[0] - width));
            let r = Rect {
                min: [
                    (f64::from(self.bounds.min[0]) + offset) as f32,
                    self.bounds.min[1],
                ],
                size: [width, self.bounds.size[1]],
            };
            inside(self.bounds, r)?;
            checked[i] = r;
        }
        output[..fractions.len()].copy_from_slice(&checked[..fractions.len()]);
        Ok(fractions.len())
    }
}

/// Geometry and accepted-count state for one cell; Unknown has filled=false,
/// and must be distinguished by the enclosing SegmentGeometry::value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SegmentRect {
    pub bounds: Rect,
    pub filled: bool,
}

/// Immutable strip descriptor; results use caller-owned storage.
///
/// ```
/// use ferravex_play::ui::{Rect, meters::*};
/// let r = Rect { min: [0.; 2], size: [118., 10.] };
/// let strip = SegmentGeometry::new(r,
///     SegmentValue::Known { filled: 2, capacity: 6 }, 2.)?;
/// let mut cells = [SegmentRect { bounds: r, filled: false }; MAX_METER_SEGMENTS];
/// assert_eq!(strip.write_into(&mut cells)?, 6);
/// assert_eq!(cells[..6].iter().filter(|c| c.filled).count(), 2);
/// assert_eq!(cells[5].bounds.min[0] + cells[5].bounds.size[0], 118.);
/// # Ok::<(), MeterError>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SegmentGeometry {
    bounds: Rect,
    value: SegmentValue,
    gap: f32,
    count: usize,
}
impl SegmentGeometry {
    pub fn new(bounds: Rect, value: SegmentValue, gap: f32) -> Result<Self, MeterError> {
        valid(bounds)?;
        let count = value.count()?;
        if !gap.is_finite() || gap < 0. {
            return Err(MeterError::InvalidGeometry);
        }
        let result = Self {
            bounds,
            value,
            gap,
            count,
        };
        // Validate every derived rectangle before returning a usable descriptor.
        for i in 0..count {
            result.cell(i)?;
        }
        Ok(result)
    }
    pub fn bounds(self) -> Rect {
        self.bounds
    }
    pub fn value(self) -> SegmentValue {
        self.value
    }
    pub fn count(self) -> usize {
        self.count
    }
    /// Full prevalidation and output-length check precede writes. The output
    /// tail is unchanged on success, and the entire slice is unchanged on error.
    pub fn write_into(self, output: &mut [SegmentRect]) -> Result<usize, MeterError> {
        if output.len() < self.count {
            return Err(MeterError::Capacity);
        }
        for (i, dest) in output[..self.count].iter_mut().enumerate() {
            // Constructor checked these immutable inputs already.
            *dest = self.cell(i).expect("validated immutable segment geometry");
        }
        Ok(self.count)
    }
    fn cell(self, index: usize) -> Result<SegmentRect, MeterError> {
        let width = (f64::from(self.bounds.size[0])
            - f64::from(self.gap) * (self.count - 1) as f64)
            / self.count as f64;
        if width <= 0. || !width.is_finite() {
            return Err(MeterError::InvalidGeometry);
        }
        let left =
            (f64::from(self.bounds.min[0]) + (width + f64::from(self.gap)) * index as f64) as f32;
        let right = if index + 1 == self.count {
            self.bounds.min[0] + self.bounds.size[0]
        } else {
            (f64::from(self.bounds.min[0]) + (width + f64::from(self.gap)) * index as f64 + width)
                as f32
        };
        let bounds = Rect {
            min: [left, self.bounds.min[1]],
            size: [right - left, self.bounds.size[1]],
        };
        if bounds.size[0] <= 0. {
            return Err(MeterError::InvalidGeometry);
        }
        inside(self.bounds, bounds)?;
        Ok(SegmentRect {
            bounds,
            filled: matches!(self.value, SegmentValue::Known { filled, .. } if index < filled),
        })
    }
}

fn valid(rect: Rect) -> Result<(), MeterError> {
    if !rect.valid()
        || (0..2).any(|i| rect.size[i] > 0. && rect.min[i] + rect.size[i] <= rect.min[i])
    {
        Err(MeterError::InvalidGeometry)
    } else {
        Ok(())
    }
}
fn inside(parent: Rect, rect: Rect) -> Result<(), MeterError> {
    valid(rect)?;
    if (0..2).any(|i| {
        rect.min[i] < parent.min[i] || rect.min[i] + rect.size[i] > parent.min[i] + parent.size[i]
    }) {
        Err(MeterError::InvalidGeometry)
    } else {
        Ok(())
    }
}
