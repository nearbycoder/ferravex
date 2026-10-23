use crate::{Vec2, Vec4, vec2};

/// Axis-aligned 2D rectangle. Width and height should be nonnegative.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}
impl Rect {
    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0, 0.0);
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }
    pub const fn point(self) -> Vec2 {
        vec2(self.x, self.y)
    }
    pub const fn size(self) -> Vec2 {
        vec2(self.w, self.h)
    }
    pub fn center(self) -> Vec2 {
        self.point() + self.size() * 0.5
    }
    pub const fn left(self) -> f32 {
        self.x
    }
    pub const fn right(self) -> f32 {
        self.x + self.w
    }
    pub const fn top(self) -> f32 {
        self.y
    }
    pub const fn bottom(self) -> f32 {
        self.y + self.h
    }
    pub fn is_finite(self) -> bool {
        [self.x, self.y, self.w, self.h]
            .into_iter()
            .all(f32::is_finite)
    }
    pub fn contains(self, p: Vec2) -> bool {
        self.is_finite()
            && self.w >= 0.0
            && self.h >= 0.0
            && p.x >= self.x
            && p.x <= self.right()
            && p.y >= self.y
            && p.y <= self.bottom()
    }
    /// Positive-area overlap. Touching edges are not solid penetration.
    pub fn overlaps(self, other: &Self) -> bool {
        self.w > 0.0
            && self.h > 0.0
            && other.w > 0.0
            && other.h > 0.0
            && self.x < other.right()
            && self.right() > other.x
            && self.y < other.bottom()
            && self.bottom() > other.y
    }
    pub fn intersect(self, other: Self) -> Option<Self> {
        if !self.overlaps(&other) {
            return None;
        }
        let point = self.point().max(other.point());
        let end = vec2(self.right(), self.bottom()).min(vec2(other.right(), other.bottom()));
        Some(Self::new(
            point.x,
            point.y,
            end.x - point.x,
            end.y - point.y,
        ))
    }
    pub fn combine_with(self, other: Self) -> Self {
        let p = self.point().min(other.point());
        let end = vec2(self.right(), self.bottom()).max(vec2(other.right(), other.bottom()));
        Self::new(p.x, p.y, end.x - p.x, end.y - p.y)
    }
    pub fn move_to(&mut self, p: Vec2) {
        self.x = p.x;
        self.y = p.y;
    }
    pub fn offset(self, delta: Vec2) -> Self {
        Self::new(self.x + delta.x, self.y + delta.y, self.w, self.h)
    }
    pub fn normalized(self) -> Self {
        Self::new(
            self.x.min(self.right()),
            self.y.min(self.bottom()),
            self.w.abs(),
            self.h.abs(),
        )
    }
}

/// Failure from checked color authoring or transfer conversion.
///
/// ```
/// use ferravex_math::{Color, ColorError};
/// assert_eq!(Color::try_new(0.0, 0.0, 0.0, 2.0), Err(ColorError::AlphaOutOfRange));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorError {
    /// At least one input RGBA channel is NaN or infinite.
    NonFinite,
    /// Finite alpha is outside the inclusive range `0..=1`.
    AlphaOutOfRange,
    /// A converted RGB channel would be non-finite when stored as `f32`.
    ConversionOverflow,
}

impl std::fmt::Display for ColorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::NonFinite => "color channels must be finite",
            Self::AlphaOutOfRange => "color alpha must be in 0..=1",
            Self::ConversionOverflow => "converted color channel overflows f32",
        })
    }
}

impl std::error::Error for ColorError {}

/// Unpremultiplied RGBA floating-point channels. No implicit gamma conversion.
///
/// The color space is not stored: callers must track whether RGB is sRGB encoded
/// or linear light. Existing constructors and [`Self::lerp`] operate on raw
/// channels. Use [`Self::try_srgb_to_linear`] and [`Self::try_linear_to_srgb`] to
/// convert explicitly, before premultiplying. Alpha is linear coverage, never
/// gamma encoded; conversions leave it unchanged, even for transparent colors.
///
/// Checked construction and conversions require finite channels and alpha in
/// `0..=1`. Signed RGB values outside `0..=1` are allowed for HDR and out-of-gamut
/// intermediates. Only [`Self::try_clamped`] and byte output explicitly clip;
/// no operation performs tone mapping. Conversions round to `f32`, so very small
/// values may underflow to zero and round trips are approximate.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}
impl Color {
    /// Stores raw channels without validation, clamping or gamma conversion.
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }
    /// Constructs finite raw RGB with alpha in `0..=1`, without conversion.
    ///
    /// RGB may be negative or greater than one; no channels are clamped.
    /// Returns [`ColorError::NonFinite`] for any NaN/infinity, otherwise
    /// [`ColorError::AlphaOutOfRange`] for invalid alpha.
    ///
    /// ```
    /// use ferravex_math::{Color, ColorError};
    /// let hdr = Color::try_new(4.0, -0.25, 0.5, 0.75)?;
    /// assert_eq!(hdr.to_array(), [4.0, -0.25, 0.5, 0.75]);
    /// assert_eq!(Color::try_new(0.0, 0.0, 0.0, -0.1), Err(ColorError::AlphaOutOfRange));
    /// # Ok::<(), ColorError>(())
    /// ```
    pub fn try_new(r: f32, g: f32, b: f32, a: f32) -> Result<Self, ColorError> {
        let color = Self::new(r, g, b, a);
        if !color.is_finite() {
            return Err(ColorError::NonFinite);
        }
        if !(0.0..=1.0).contains(&a) {
            return Err(ColorError::AlphaOutOfRange);
        }
        Ok(color)
    }
    /// Normalizes bytes to raw channels; does not decode sRGB.
    pub const fn from_rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self::new(
            r as f32 / 255.0,
            g as f32 / 255.0,
            b as f32 / 255.0,
            a as f32 / 255.0,
        )
    }
    /// Opaque 0xRRGGBB color. Does not decode sRGB.
    pub const fn from_hex(rgb: u32) -> Self {
        Self::from_rgba((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255)
    }
    pub const fn to_vec(self) -> Vec4 {
        Vec4::new(self.r, self.g, self.b, self.a)
    }
    pub const fn to_array(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }
    /// Whether all RGBA channels are finite, regardless of their ranges.
    ///
    /// ```
    /// use ferravex_math::Color;
    /// assert!(Color::new(4.0, -0.5, 0.0, 2.0).is_finite());
    /// assert!(!Color::new(0.0, 0.0, 0.0, f32::NAN).is_finite());
    /// ```
    pub fn is_finite(self) -> bool {
        self.to_array().into_iter().all(f32::is_finite)
    }
    /// Clips finite RGBA channels to `0..=1` without gamma conversion.
    ///
    /// Returns [`ColorError::NonFinite`] instead of repairing NaN/infinity.
    /// Finite out-of-range alpha is intentionally accepted and clipped here.
    /// This discards HDR/out-of-gamut information; it is not tone mapping.
    ///
    /// ```
    /// use ferravex_math::{Color, ColorError};
    /// let color = Color::new(2.0, -0.25, 0.5, 1.5).try_clamped()?;
    /// assert_eq!(color.to_array(), [1.0, 0.0, 0.5, 1.0]);
    /// assert_eq!(color.to_rgba(), [255, 0, 128, 255]);
    /// assert_eq!(Color::new(f32::NAN, 0.0, 0.0, 1.0).try_clamped(), Err(ColorError::NonFinite));
    /// # Ok::<(), ColorError>(())
    /// ```
    pub fn try_clamped(self) -> Result<Self, ColorError> {
        if !self.is_finite() {
            return Err(ColorError::NonFinite);
        }
        Ok(Self::from(self.to_array().map(|c| c.clamp(0.0, 1.0))))
    }
    /// Decodes unpremultiplied sRGB channels to linear-light sRGB.
    ///
    /// Uses the piecewise sRGB transfer function, with a linear segment for
    /// magnitudes up to `0.04045`. Its sign-preserving extension supports negative
    /// RGB and values above one without clipping, as in
    /// [CSS Color 4](https://www.w3.org/TR/css-color-4/#color-conversion-code).
    /// Alpha is unchanged and RGB is not multiplied by alpha.
    ///
    /// Returns the input errors from [`Self::try_new`], or
    /// [`ColorError::ConversionOverflow`] if decoded HDR RGB overflows `f32`.
    /// Intermediate calculations use `f64`; results are not bitwise portable.
    ///
    /// ```
    /// use ferravex_math::{Color, ColorError};
    /// let authored = Color::from_rgba(128, 0, 255, 64);
    /// let linear = authored.try_srgb_to_linear()?;
    /// assert!((linear.r - 0.2158605).abs() < 1.0e-6);
    /// assert_eq!(linear.a, authored.a);
    /// assert_eq!(linear.try_linear_to_srgb()?.to_rgba(), [128, 0, 255, 64]);
    /// # Ok::<(), ColorError>(())
    /// ```
    pub fn try_srgb_to_linear(self) -> Result<Self, ColorError> {
        self.try_map_rgb(|c| {
            if c.abs() <= 0.04045 {
                c / 12.92
            } else {
                ((c.abs() + 0.055) / 1.055).powf(2.4).copysign(c)
            }
        })
    }
    /// Encodes unpremultiplied linear-light sRGB channels to sRGB.
    ///
    /// The linear segment covers magnitudes up to `0.0031308`. Negative RGB
    /// uses the sign-preserving extension described in [`Self::try_srgb_to_linear`].
    /// HDR RGB is retained above one; there is no clamping or tone mapping.
    /// Alpha is unchanged and RGB is not multiplied by alpha. Returns the input
    /// errors from [`Self::try_new`]. Finite linear RGB cannot overflow on encode.
    /// Intermediate calculations use `f64`; results are not bitwise portable.
    ///
    /// ```
    /// use ferravex_math::{Color, ColorError};
    /// // Interpolate in linear light, then encode for an sRGB destination.
    /// let black = Color::from_hex(0x000000).try_srgb_to_linear()?;
    /// let white = Color::from_hex(0xffffff).try_srgb_to_linear()?;
    /// let middle = black.lerp(white, 0.5).try_linear_to_srgb()?;
    /// assert_eq!(middle.to_rgba(), [188, 188, 188, 255]);
    /// # Ok::<(), ColorError>(())
    /// ```
    pub fn try_linear_to_srgb(self) -> Result<Self, ColorError> {
        self.try_map_rgb(|c| {
            if c.abs() <= 0.0031308 {
                12.92 * c
            } else {
                (1.055 * c.abs().powf(1.0 / 2.4) - 0.055).copysign(c)
            }
        })
    }
    fn try_map_rgb(self, convert: impl Fn(f64) -> f64) -> Result<Self, ColorError> {
        Self::try_new(self.r, self.g, self.b, self.a)?;
        let [r, g, b] = [self.r, self.g, self.b].map(|c| convert(f64::from(c)) as f32);
        let color = Self::new(r, g, b, self.a);
        if !color.is_finite() {
            return Err(ColorError::ConversionOverflow);
        }
        Ok(color)
    }
    /// Clips and rounds raw channels to bytes without gamma conversion.
    ///
    /// Legacy non-finite behavior is preserved: NaN and negative infinity become
    /// zero; positive infinity becomes 255. Use [`Self::try_clamped`] first to
    /// reject non-finite channels. Encode linear RGB explicitly before sRGB output.
    pub fn to_rgba(self) -> [u8; 4] {
        self.to_array()
            .map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8)
    }
    pub const fn with_alpha(self, alpha: f32) -> Self {
        Self { a: alpha, ..self }
    }
    /// Interpolates raw RGBA channels, with no gamma conversion or clamping of `t`.
    /// Decode sRGB endpoints first when linear-light interpolation is desired.
    pub fn lerp(self, other: Self, t: f32) -> Self {
        Self::from(self.to_vec().lerp(other.to_vec(), t))
    }
    pub fn premultiplied(self) -> Self {
        Self::new(self.r * self.a, self.g * self.a, self.b * self.a, self.a)
    }
}
impl From<Vec4> for Color {
    fn from(v: Vec4) -> Self {
        Self::new(v.x, v.y, v.z, v.w)
    }
}
impl From<[f32; 4]> for Color {
    fn from(v: [f32; 4]) -> Self {
        Self::new(v[0], v[1], v[2], v[3])
    }
}
impl From<Color> for [f32; 4] {
    fn from(v: Color) -> Self {
        v.to_array()
    }
}
impl From<Color> for Vec4 {
    fn from(v: Color) -> Self {
        v.to_vec()
    }
}

#[cfg(test)]
mod color_tests {
    use super::{Color, ColorError};

    fn near(actual: f32, expected: f32) {
        let tolerance = 2.0e-7 * expected.abs().max(1.0);
        assert!(
            (actual - expected).abs() <= tolerance,
            "{actual} != {expected} (tolerance {tolerance})"
        );
    }

    #[test]
    fn legacy_constructors_and_lerp_keep_raw_channel_semantics() {
        const RAW: Color = Color::new(2.0, -1.0, 0.5, 3.0);
        const BYTES: Color = Color::from_rgba(128, 64, 255, 32);
        const HEX: Color = Color::from_hex(0x8040ff);
        assert_eq!(RAW.to_array(), [2.0, -1.0, 0.5, 3.0]);
        assert_eq!(RAW.to_rgba(), [255, 0, 128, 255]);
        assert_eq!(
            BYTES.to_array(),
            [128.0 / 255.0, 64.0 / 255.0, 1.0, 32.0 / 255.0]
        );
        assert_eq!(HEX, BYTES.with_alpha(1.0));
        assert_eq!(Color::from(RAW.to_vec()), RAW);
        assert_eq!(Color::from(RAW.to_array()), RAW);

        let black = Color::new(0.0, 0.0, 0.0, 0.0);
        let white = Color::new(1.0, 1.0, 1.0, 1.0);
        assert_eq!(black.lerp(white, 0.5).to_rgba(), [128; 4]);
        assert_eq!(black.lerp(white, 1.5).to_array(), [1.5; 4]);
        assert_eq!(black.lerp(white, -0.5).to_array(), [-0.5; 4]);
        assert_eq!(white.with_alpha(0.5).premultiplied().to_array(), [0.5; 4]);
        assert_eq!(
            Color::new(f32::NAN, f32::NEG_INFINITY, f32::INFINITY, 0.5).to_rgba(),
            [0, 0, 255, 128]
        );
    }

    #[test]
    fn checked_construction_accepts_hdr_but_rejects_invalid_alpha() {
        let hdr = Color::try_new(-f32::MAX, f32::MAX, -0.25, -0.0).unwrap();
        assert_eq!(hdr.to_array(), [-f32::MAX, f32::MAX, -0.25, -0.0]);
        assert_eq!(hdr.a.to_bits(), (-0.0_f32).to_bits());
        assert!(hdr.is_finite());
        assert!(Color::try_new(1.0, 0.0, 0.0, 1.0).is_ok());
        for alpha in [-f32::MAX, -f32::from_bits(1), 1.000_000_1, f32::MAX] {
            let color = Color::new(0.25, 0.5, 0.75, alpha);
            assert!(color.is_finite());
            assert_eq!(
                Color::try_new(0.25, 0.5, 0.75, alpha),
                Err(ColorError::AlphaOutOfRange)
            );
            assert_eq!(color.try_srgb_to_linear(), Err(ColorError::AlphaOutOfRange));
            assert_eq!(color.try_linear_to_srgb(), Err(ColorError::AlphaOutOfRange));
        }
    }

    #[test]
    fn all_checked_operations_reject_every_nonfinite_channel() {
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            for index in 0..4 {
                let mut channels = [0.5; 4];
                channels[index] = invalid;
                let color = Color::from(channels);
                assert!(!color.is_finite());
                assert_eq!(
                    Color::try_new(color.r, color.g, color.b, color.a),
                    Err(ColorError::NonFinite)
                );
                assert_eq!(color.try_srgb_to_linear(), Err(ColorError::NonFinite));
                assert_eq!(color.try_linear_to_srgb(), Err(ColorError::NonFinite));
                assert_eq!(color.try_clamped(), Err(ColorError::NonFinite));
            }
        }
        // Reject malformed hidden RGB too, and report non-finite input before range errors.
        assert_eq!(
            Color::new(f32::NAN, 0.0, 0.0, 0.0).try_srgb_to_linear(),
            Err(ColorError::NonFinite)
        );
        assert_eq!(
            Color::try_new(f32::INFINITY, 0.0, 0.0, 2.0),
            Err(ColorError::NonFinite)
        );
    }

    #[test]
    fn clamping_is_explicit_includes_alpha_and_does_not_convert() {
        let color = Color::new(-1.0, 4.0, 0.5, 2.0);
        let clipped = color.try_clamped().unwrap();
        assert_eq!(clipped.to_array(), [0.0, 1.0, 0.5, 1.0]);
        assert_eq!(clipped.try_clamped(), Ok(clipped));
        assert_eq!(color.with_alpha(-2.0).try_clamped().unwrap().a, 0.0);
        assert_eq!(color.to_array(), [-1.0, 4.0, 0.5, 2.0]);
        assert_eq!(clipped.to_rgba(), [0, 255, 128, 255]);
    }

    #[test]
    fn transfer_reference_values_and_endpoints() {
        let color = Color::new(0.0, 0.5, 1.0, 0.25);
        let decoded = color.try_srgb_to_linear().unwrap();
        assert_eq!([decoded.r, decoded.b, decoded.a], [0.0, 1.0, 0.25]);
        near(decoded.g, 0.214_041_14);
        let encoded = color.try_linear_to_srgb().unwrap();
        assert_eq!([encoded.r, encoded.b, encoded.a], [0.0, 1.0, 0.25]);
        near(encoded.g, 0.735_357);
        near(
            Color::from_hex(0x808080).try_srgb_to_linear().unwrap().r,
            0.215_860_5,
        );
        near(
            Color::new(0.18, 0.18, 0.18, 1.0)
                .try_linear_to_srgb()
                .unwrap()
                .r,
            0.461_356_13,
        );
    }

    #[test]
    fn dark_segments_and_signed_extension_match_reference_values() {
        let decoded = Color::new(0.02, -0.02, -0.5, 1.0)
            .try_srgb_to_linear()
            .unwrap();
        near(decoded.r, 0.001_547_987_6);
        assert_eq!(decoded.g, -decoded.r);
        near(decoded.b, -0.214_041_14);
        let encoded = Color::new(0.001, -0.001, -0.5, 1.0)
            .try_linear_to_srgb()
            .unwrap();
        near(encoded.r, 0.012_92);
        assert_eq!(encoded.g, -encoded.r);
        near(encoded.b, -0.735_357);
    }

    #[test]
    fn transfer_breakpoints_match_reference_samples_on_both_sides() {
        // Adjacent f32 inputs straddling the specified decimal breakpoints.
        // The published rounded cutoffs have a tiny discontinuity.
        for (bits, expected) in [
            (0x3d25_aee5, 0.003_130_804_6),
            (0x3d25_aee6, 0.003_130_804_8),
            (0x3d25_aee7, 0.003_130_807_5),
        ] {
            let value = f32::from_bits(bits);
            let decoded = Color::new(value, -value, 0.0, 1.0)
                .try_srgb_to_linear()
                .unwrap();
            assert!((decoded.r - expected).abs() < 5.0e-10);
            assert_eq!(decoded.g, -decoded.r);
        }
        for (bits, expected) in [
            (0x3b4d_2e1b, 0.040_449_936),
            (0x3b4d_2e1c, 0.040_449_91),
            (0x3b4d_2e1d, 0.040_449_913),
        ] {
            let value = f32::from_bits(bits);
            let encoded = Color::new(value, -value, 0.0, 1.0)
                .try_linear_to_srgb()
                .unwrap();
            assert!((encoded.r - expected).abs() < 5.0e-9);
            assert_eq!(encoded.g, -encoded.r);
        }
    }

    #[test]
    fn all_byte_channels_round_trip_with_alpha_unchanged() {
        for value in 0..=u8::MAX {
            let bytes = [value, 255 - value, value / 2, 255 - value];
            let color = Color::from_rgba(bytes[0], bytes[1], bytes[2], bytes[3]);
            let linear = color.try_srgb_to_linear().unwrap();
            let encoded = linear.try_linear_to_srgb().unwrap();
            assert_eq!(linear.a.to_bits(), color.a.to_bits());
            assert_eq!(encoded.a.to_bits(), color.a.to_bits());
            assert_eq!(encoded.to_rgba(), bytes);
        }
    }

    #[test]
    fn conversions_preserve_transparent_rgb_and_alpha_bits() {
        for alpha in [-0.0, 0.0, f32::from_bits(1), 0.5, 1.0] {
            let color = Color::new(0.5, 0.25, 0.75, alpha);
            let decoded = color.try_srgb_to_linear().unwrap();
            near(decoded.r, 0.214_041_14);
            assert_eq!(decoded.a.to_bits(), alpha.to_bits());
            let encoded = color.try_linear_to_srgb().unwrap();
            near(encoded.r, 0.735_357);
            assert_eq!(encoded.a.to_bits(), alpha.to_bits());
        }
    }

    #[test]
    fn hdr_and_out_of_gamut_channels_round_trip_without_clipping() {
        for value in [
            -1.0e10, -100.0, -4.0, -1.0, -0.25, 0.25, 1.0, 4.0, 100.0, 1.0e10,
        ] {
            let color = Color::try_new(value, -value, value / 2.0, 0.5).unwrap();
            let decoded = color.try_srgb_to_linear().unwrap();
            let encoded = color.try_linear_to_srgb().unwrap();
            assert_eq!(decoded.r.is_sign_negative(), value.is_sign_negative());
            assert_eq!(encoded.r.is_sign_negative(), value.is_sign_negative());
            if value.abs() > 1.0 {
                assert!(decoded.r.abs() > 1.0);
                assert!(encoded.r.abs() > 1.0);
            }
            for (actual, expected) in decoded
                .try_linear_to_srgb()
                .unwrap()
                .to_array()
                .into_iter()
                .zip(color.to_array())
            {
                near(actual, expected);
            }
            for (actual, expected) in encoded
                .try_srgb_to_linear()
                .unwrap()
                .to_array()
                .into_iter()
                .zip(color.to_array())
            {
                near(actual, expected);
            }
        }
    }

    #[test]
    fn decode_reports_overflow_and_encode_accepts_extreme_finite_rgb() {
        for value in [f32::MAX, -f32::MAX] {
            for index in 0..3 {
                let mut channels = [0.5; 4];
                channels[index] = value;
                let color = Color::from(channels);
                assert_eq!(
                    color.try_srgb_to_linear(),
                    Err(ColorError::ConversionOverflow)
                );
                let encoded = color.try_linear_to_srgb().unwrap();
                assert!(encoded.is_finite());
                assert!(encoded.to_array()[index].abs() > 1.0);
                assert_eq!(
                    encoded.to_array()[index].is_sign_negative(),
                    value.is_sign_negative()
                );
                assert_eq!(color.to_array(), channels);
            }
        }
    }

    #[test]
    fn signed_zero_and_subnormal_rounding_are_explicit() {
        let color = Color::new(-0.0, -f32::from_bits(1), f32::from_bits(1), -0.0);
        let decoded = color.try_srgb_to_linear().unwrap();
        assert_eq!(decoded.r.to_bits(), (-0.0_f32).to_bits());
        assert_eq!(decoded.g.to_bits(), (-0.0_f32).to_bits());
        assert_eq!(decoded.b.to_bits(), 0.0_f32.to_bits());
        let encoded = color.try_linear_to_srgb().unwrap();
        assert_eq!(encoded.r.to_bits(), (-0.0_f32).to_bits());
        assert!(encoded.g < 0.0 && encoded.b > 0.0);
        assert_eq!(encoded.g, -encoded.b);
        assert_eq!(encoded.a.to_bits(), (-0.0_f32).to_bits());
    }

    #[test]
    fn linear_light_interpolation_is_opt_in() {
        let black = Color::from_hex(0x000000);
        let white = Color::from_hex(0xffffff);
        let middle = black
            .try_srgb_to_linear()
            .unwrap()
            .lerp(white.try_srgb_to_linear().unwrap(), 0.5)
            .try_linear_to_srgb()
            .unwrap();
        assert_eq!(middle.to_rgba(), [188, 188, 188, 255]);
        assert_eq!(black.lerp(white, 0.5).to_rgba(), [128, 128, 128, 255]);
    }
}
