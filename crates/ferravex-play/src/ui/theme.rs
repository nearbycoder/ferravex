//! Opt-in, caller-owned UI tokens in logical screen pixels, before viewport/DPR
//! projection. No game palette, renderer default, global state, input, motion or
//! flash preference is changed. Rebuild measured layout and share its rectangles
//! with paint/hits after changing tokens; wrap, scroll or report overflow rather
//! than reducing essential text. A fitted viewport can still scale these units;
//! use a responsive viewport when readable pixel-sized UI is intended.
//!
//! Palette RGB is **unpremultiplied, normalized sRGB encoded** and opaque. These
//! are foreground roles on Canvas/Surface, not arbitrary fills behind text.
//! Measurements decode via `ferravex_math::Color::try_srgb_to_linear`, then use
//! linear-light relative luminance. They describe an ideal sRGB color pair,
//! not physical-display accessibility, glyph legibility or renderer blending.
//! Transparency is rejected by tokens/opaque measurements. [`contrast_on`]
//! explicitly composites in linear light over a supplied opaque backdrop; a
//! renderer blending encoded channels can produce a different result.
//!
//! ```
//! use ferravex_math::Color;
//! use ferravex_play::ui::theme::*;
//! let mut theme = UiTheme::preset(ThemeMode::Normal); // Explicit adoption.
//! theme.set_colors(&[(ColorRole::Accent, Color::from_hex(0xffdb80))])?;
//! assert_eq!(theme.text(TextRole::Body).size, 18.);
//! assert!(theme.role_contrast(ColorRole::Accent, ColorRole::Surface)? >= 4.5);
//! // Enlarge by author/user choice, never by available layout width.
//! let types = theme.types().try_scaled(1.5)?;
//! let spacing = Spacing { control_height: 56., ..theme.spacing() };
//! theme.set_metrics(types, spacing)?;
//! assert_eq!(theme.text(TextRole::Body).size, 27.);
//! assert_eq!(theme.state(ThemeState::Warning).cue.label, "WARNING");
//! theme = UiTheme::preset(ThemeMode::HighContrast); // Explicit reset/replacement.
//! assert_eq!(theme.types().body, 18.);
//! # Ok::<(), ThemeError>(())
//! ```
//!
//! Renderer mapping (the executed task290 receipt recipe supplies full code):
//! map [`TextTokens`] into `TextStyle` size/color; `StatRowsStyle` accepts those
//! styles and spacing gaps. For ordinary retained `ButtonStyle`, use Surface for
//! both fill/disabled fill, Text for ink, Focus for the outline, and body size.
//! Supply an explicit unavailable label/shape; the renderer does not draw token
//! cues automatically. A meter can compose Surface track, semantic foreground
//! fill, and a separate Text value plus labeled tick/shape cues, sharing pure
//! rectangles with custom software hosts. Text ON a semantic fill needs a
//! separate pair check, since roles are validated on Canvas/Surface only.
//!
//! Current task277 `CardLayout::new(bounds, content)` has no style argument:
//! title 24/body/stat/action 18, padding 16/gap 12/action gap 8/height 44, PANEL,
//! default TextStyle and ButtonStyle remain fixed. Passing tokens to that API
//! does not theme it. Compose panels/wrapped text/stats/buttons via `Ui::compose`
//! for explicit themed cards/inspectors; parent may later add an additive card
//! style seam. Button padding/focus stroke are also renderer-fixed.

use ferravex_math::Color;

/// RGB units for measurement, never inferred from [`Color`]. Alpha is linear
/// coverage in either encoding. All RGBA channels must be in `0..=1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RgbEncoding {
    Srgb,
    LinearSrgb,
}

/// Opt-in presets/contrast policies. Neither changes existing UI defaults.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeMode {
    Normal,
    HighContrast,
}

/// Order for [`Palette::try_new_srgb`]. All roles except Canvas/Surface are
/// foregrounds on BOTH backgrounds. Focus/Border are non-text marks; all other
/// foreground roles must satisfy the small-text threshold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum ColorRole {
    Canvas,
    Surface,
    Text,
    MutedText,
    Accent,
    Success,
    Warning,
    Danger,
    Focus,
    Border,
    DisabledText,
}

/// Fixed palette capacity; no growing registry or heap storage.
pub const COLOR_ROLE_COUNT: usize = 11;

impl ColorRole {
    pub const ALL: [Self; COLOR_ROLE_COUNT] = [
        Self::Canvas,
        Self::Surface,
        Self::Text,
        Self::MutedText,
        Self::Accent,
        Self::Success,
        Self::Warning,
        Self::Danger,
        Self::Focus,
        Self::Border,
        Self::DisabledText,
    ];
}

/// Rejection before theme mutation. Contrast reports the first failed pair in
/// role order. Ratios are unitless; colors are never silently clipped/repaired.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ThemeError {
    InvalidTypeScale,
    InvalidSpacing,
    NonFiniteColor,
    ColorOutOfRange,
    AlphaRequiresBackdrop,
    Capacity,
    DuplicateRole,
    Contrast {
        foreground: ColorRole,
        background: ColorRole,
        measured: f64,
        required: f64,
    },
}
impl std::fmt::Display for ThemeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "UI theme: {self:?}")
    }
}
impl std::error::Error for ThemeError {}

fn linear(color: Color, encoding: RgbEncoding) -> Result<Color, ThemeError> {
    if !color.is_finite() {
        return Err(ThemeError::NonFiniteColor);
    }
    if !color
        .to_array()
        .into_iter()
        .all(|v| (0. ..=1.).contains(&v))
    {
        return Err(ThemeError::ColorOutOfRange);
    }
    match encoding {
        RgbEncoding::Srgb => color
            .try_srgb_to_linear()
            .map_err(|_| ThemeError::NonFiniteColor),
        RgbEncoding::LinearSrgb => Ok(color),
    }
}
fn opaque(color: Color) -> Result<(), ThemeError> {
    if color.a != 1. {
        return Err(ThemeError::AlphaRequiresBackdrop);
    }
    Ok(())
}
fn luminance(color: Color) -> f64 {
    0.2126 * f64::from(color.r) + 0.7152 * f64::from(color.g) + 0.0722 * f64::from(color.b)
}
fn ratio(a: Color, b: Color) -> f64 {
    let a = luminance(a);
    let b = luminance(b);
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// Relative luminance `0..=1` of opaque normalized RGB in the stated units.
/// Linear input is not decoded a second time.
///
/// ```
/// use ferravex_math::Color;
/// use ferravex_play::ui::theme::{relative_luminance, RgbEncoding};
/// let gray = Color::from_hex(0x808080);
/// let a = relative_luminance(gray, RgbEncoding::Srgb)?;
/// let b = relative_luminance(gray.try_srgb_to_linear()?, RgbEncoding::LinearSrgb)?;
/// assert!((a - 0.21586).abs() < 0.00001);
/// assert_eq!(a, b);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn relative_luminance(color: Color, encoding: RgbEncoding) -> Result<f64, ThemeError> {
    let color = linear(color, encoding)?;
    opaque(color)?;
    Ok(luminance(color))
}

/// Symmetric ratio `1..=21` for two opaque colors in the same stated units.
/// Transparent inputs require [`contrast_on`], not an assumed black backdrop.
///
/// ```
/// use ferravex_math::Color;
/// use ferravex_play::ui::theme::{contrast_ratio, RgbEncoding};
/// assert_eq!(contrast_ratio(Color::from_hex(0), Color::from_hex(0xffffff),
///                          RgbEncoding::Srgb)?, 21.);
/// # Ok::<(), ferravex_play::ui::theme::ThemeError>(())
/// ```
pub fn contrast_ratio(a: Color, b: Color, encoding: RgbEncoding) -> Result<f64, ThemeError> {
    let a = linear(a, encoding)?;
    let b = linear(b, encoding)?;
    opaque(a)?;
    opaque(b)?;
    Ok(ratio(a, b))
}

/// Decode all colors in the stated units; source-over background onto opaque
/// backdrop, then foreground onto that result, **in linear light**. Return
/// foreground-result versus background-result contrast. No premultiplied/HDR,
/// encoded-channel blending, unknown-backdrop or physical-display claim.
///
/// ```
/// use ferravex_math::Color;
/// use ferravex_play::ui::theme::{contrast_on, RgbEncoding};
/// let black = Color::from_hex(0);
/// assert!((contrast_on(Color::new(1., 1., 1., 0.5), black, black,
///                      RgbEncoding::Srgb)? - 11.).abs() < 1e-6);
/// # Ok::<(), ferravex_play::ui::theme::ThemeError>(())
/// ```
pub fn contrast_on(
    foreground: Color,
    background: Color,
    backdrop: Color,
    encoding: RgbEncoding,
) -> Result<f64, ThemeError> {
    let fg = linear(foreground, encoding)?;
    let bg = linear(background, encoding)?;
    let backdrop = linear(backdrop, encoding)?;
    opaque(backdrop)?;
    fn over(a: Color, b: Color) -> Color {
        let blend = |a_rgb: f32, b_rgb: f32| {
            (f64::from(a_rgb) * f64::from(a.a) + f64::from(b_rgb) * (1. - f64::from(a.a))) as f32
        };
        Color::new(blend(a.r, b.r), blend(a.g, b.g), blend(a.b, b.b), 1.)
    }
    let bg = over(bg, backdrop);
    Ok(ratio(over(fg, bg), bg))
}

/// Essential label/body/heading sizes in logical pixels, each `16..=256`, with
/// `label <= body <= heading`. Raw authoring value; checked on theme adoption.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TypeScale {
    pub label: f32,
    pub body: f32,
    pub heading: f32,
}
impl TypeScale {
    fn validate(self) -> Result<(), ThemeError> {
        if ![self.label, self.body, self.heading]
            .into_iter()
            .all(|v| v.is_finite() && (16. ..=256.).contains(&v))
            || self.label > self.body
            || self.body > self.heading
        {
            return Err(ThemeError::InvalidTypeScale);
        }
        Ok(())
    }
    /// Explicit enlargement by finite factor >= 1. Reject overflow/size cap;
    /// never clamp or shrink to fit. See the module's executable theme recipe.
    pub fn try_scaled(self, factor: f32) -> Result<Self, ThemeError> {
        self.validate()?;
        if !factor.is_finite() || factor < 1. {
            return Err(ThemeError::InvalidTypeScale);
        }
        let scaled = Self {
            label: self.label * factor,
            body: self.body * factor,
            heading: self.heading * factor,
        };
        scaled.validate()?;
        Ok(scaled)
    }
}

/// Raw logical metrics. Gaps/inset `0..=4096`, stroke `1..=64`, control height
/// `44..=4096` and at least body + 2 * compact_gap. A minimum layout request,
/// not a text measurement/fit guarantee. Reflow/overflow remains consumer-owned.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spacing {
    pub inset: f32,
    pub gap: f32,
    pub compact_gap: f32,
    pub control_height: f32,
    pub stroke: f32,
}
impl Spacing {
    fn validate(self, types: TypeScale) -> Result<(), ThemeError> {
        if ![self.inset, self.gap, self.compact_gap]
            .into_iter()
            .all(|v| v.is_finite() && (0. ..=4096.).contains(&v))
            || !self.stroke.is_finite()
            || !(1. ..=64.).contains(&self.stroke)
            || !self.control_height.is_finite()
            || !(44. ..=4096.).contains(&self.control_height)
            || self.control_height < types.body + 2. * self.compact_gap
        {
            return Err(ThemeError::InvalidSpacing);
        }
        Ok(())
    }
}

/// Opaque normalized sRGB in [`ColorRole::ALL`] order. Private storage preserves
/// channel/alpha validation. A palette alone has no contrast policy;
/// [`UiTheme::try_new`] checks required pairs before adoption.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    colors: [Color; COLOR_ROLE_COUNT],
}
impl Palette {
    /// Author a full palette without silently converting/clipping channels.
    ///
    /// ```
    /// use ferravex_play::ui::theme::*;
    /// let base = UiTheme::preset(ThemeMode::Normal);
    /// let colors = ColorRole::ALL.map(|r| base.color(r));
    /// let palette = Palette::try_new_srgb(colors)?;
    /// let custom = UiTheme::try_new(base.mode(), base.types(), base.spacing(), palette)?;
    /// assert_eq!(custom, base);
    /// # Ok::<(), ThemeError>(())
    /// ```
    pub fn try_new_srgb(colors: [Color; COLOR_ROLE_COUNT]) -> Result<Self, ThemeError> {
        for color in colors {
            linear(color, RgbEncoding::Srgb)?;
            opaque(color)?;
        }
        Ok(Self { colors })
    }
    pub fn color(self, role: ColorRole) -> Color {
        self.colors[role as usize]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextRole {
    Label,
    Body,
    Heading,
}

/// Renderer-independent text inputs, on Canvas/Surface.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextTokens {
    pub size: f32,
    pub color: Color,
}

/// Supplemental shape guidance; consumers draw it, not this pure module.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CueShape {
    None,
    Check,
    Outline,
    Cross,
    Triangle,
    Octagon,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CuePattern {
    Solid,
    DoubleOutline,
    Hatch,
    Dots,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeState {
    Neutral,
    Selected,
    Focused,
    Disabled,
    Success,
    Warning,
    Error,
}

/// Render a descriptive localizable label (replace this English fallback when
/// needed) AND a visible shape/pattern; never convey a state only by hue. Static
/// cues require no flash/motion and have no input/remapping semantics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StateCue {
    pub label: &'static str,
    pub shape: CueShape,
    pub pattern: CuePattern,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StateTokens {
    pub foreground: Color,
    pub cue: StateCue,
}

/// Validated metrics/palette with checked atomic edits. Fixed value storage;
/// create/copy/reset per consumer, with no retained history or heap allocation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiTheme {
    mode: ThemeMode,
    types: TypeScale,
    spacing: Spacing,
    palette: Palette,
}
impl UiTheme {
    /// Explicit authored presets: label 16/body 18/heading 24, inset 16/gap 12/
    /// compact gap 8/control height 44/stroke 2 in BOTH modes. Normal text roles
    /// require 4.5:1, high-contrast text roles 7:1; Focus/Border require 3:1 and
    /// 4.5:1 respectively on Canvas and Surface independently. These thresholds
    /// are token policy, not whole-renderer conformance/physical readability.
    pub fn preset(mode: ThemeMode) -> Self {
        let hex = match mode {
            ThemeMode::Normal => [
                0x101820, 0x182430, 0xf4f7fa, 0xbfc9d2, 0xffdb80, 0x94e0ac, 0xffdb80, 0xffa6b2,
                0x80ddff, 0x92a4b5, 0xbfc9d2,
            ],
            ThemeMode::HighContrast => [
                0x000000, 0x000000, 0xffffff, 0xe0e0e0, 0xffff00, 0x80ff80, 0xffff00, 0xffa6b2,
                0x00ffff, 0xffffff, 0xe0e0e0,
            ],
        };
        Self {
            mode,
            types: TypeScale {
                label: 16.,
                body: 18.,
                heading: 24.,
            },
            spacing: Spacing {
                inset: 16.,
                gap: 12.,
                compact_gap: 8.,
                control_height: 44.,
                stroke: 2.,
            },
            palette: Palette {
                colors: hex.map(Color::from_hex),
            },
        }
    }
    /// Validate a complete authored choice; see [`Palette`]'s executable example.
    pub fn try_new(
        mode: ThemeMode,
        types: TypeScale,
        spacing: Spacing,
        palette: Palette,
    ) -> Result<Self, ThemeError> {
        types.validate()?;
        spacing.validate(types)?;
        let theme = Self {
            mode,
            types,
            spacing,
            palette,
        };
        theme.validate_contrast()?;
        Ok(theme)
    }
    pub fn mode(self) -> ThemeMode {
        self.mode
    }
    pub fn types(self) -> TypeScale {
        self.types
    }
    pub fn spacing(self) -> Spacing {
        self.spacing
    }
    pub fn palette(self) -> Palette {
        self.palette
    }
    pub fn color(self, role: ColorRole) -> Color {
        self.palette.color(role)
    }

    /// Inspect any actual token pair, including pairs outside required roles.
    /// This measures, rather than makes the pair pass policy.
    pub fn role_contrast(self, a: ColorRole, b: ColorRole) -> Result<f64, ThemeError> {
        contrast_ratio(self.color(a), self.color(b), RgbEncoding::Srgb)
    }
    fn validate_contrast(self) -> Result<(), ThemeError> {
        for foreground in ColorRole::ALL.into_iter().skip(2) {
            let required = match (self.mode, foreground) {
                (ThemeMode::Normal, ColorRole::Focus | ColorRole::Border) => 3.,
                (ThemeMode::Normal, _) => 4.5,
                (ThemeMode::HighContrast, ColorRole::Focus | ColorRole::Border) => 4.5,
                (ThemeMode::HighContrast, _) => 7.,
            };
            for background in [ColorRole::Canvas, ColorRole::Surface] {
                let measured = self.role_contrast(foreground, background)?;
                if measured < required {
                    return Err(ThemeError::Contrast {
                        foreground,
                        background,
                        measured,
                        required,
                    });
                }
            }
        }
        Ok(())
    }
    /// Override at most [`COLOR_ROLE_COUNT`] distinct roles. Related background/
    /// foreground edits may be adopted together. Duplicate/capacity/color/pair
    /// failure leaves self intact; empty edits succeed unchanged. See module recipe.
    pub fn set_colors(&mut self, edits: &[(ColorRole, Color)]) -> Result<(), ThemeError> {
        if edits.len() > COLOR_ROLE_COUNT {
            return Err(ThemeError::Capacity);
        }
        let mut candidate = *self;
        let mut seen = [false; COLOR_ROLE_COUNT];
        for &(role, color) in edits {
            if seen[role as usize] {
                return Err(ThemeError::DuplicateRole);
            }
            seen[role as usize] = true;
            linear(color, RgbEncoding::Srgb)?;
            opaque(color)?;
            candidate.palette.colors[role as usize] = color;
        }
        candidate.validate_contrast()?;
        *self = candidate;
        Ok(())
    }
    /// Adopt type/spacing together before layout/paint/hit rebuild. Invalid
    /// metrics leave both unchanged. No fitting/scaling occurs here.
    pub fn set_metrics(&mut self, types: TypeScale, spacing: Spacing) -> Result<(), ThemeError> {
        types.validate()?;
        spacing.validate(types)?;
        self.types = types;
        self.spacing = spacing;
        Ok(())
    }
    pub fn text(self, role: TextRole) -> TextTokens {
        TextTokens {
            size: match role {
                TextRole::Label => self.types.label,
                TextRole::Body => self.types.body,
                TextRole::Heading => self.types.heading,
            },
            color: self.color(if role == TextRole::Label {
                ColorRole::MutedText
            } else {
                ColorRole::Text
            }),
        }
    }
    /// State foregrounds plus non-color guidance for cards/meters/inspectors.
    /// Focus color is for non-text outlines; accompanying labels stay in Text.
    /// Nothing automatically draws cues or changes authoritative game state.
    pub fn state(self, state: ThemeState) -> StateTokens {
        use CuePattern::{Dots, DoubleOutline, Hatch, Solid};
        use CueShape::{Check, Cross, None, Octagon, Outline, Triangle};
        let (role, label, shape, pattern) = match state {
            ThemeState::Neutral => (ColorRole::Text, "", None, Solid),
            ThemeState::Selected => (ColorRole::Accent, "SELECTED", Check, DoubleOutline),
            ThemeState::Focused => (ColorRole::Focus, "FOCUSED", Outline, DoubleOutline),
            ThemeState::Disabled => (ColorRole::DisabledText, "UNAVAILABLE", Cross, Hatch),
            ThemeState::Success => (ColorRole::Success, "SUCCESS", Check, Solid),
            ThemeState::Warning => (ColorRole::Warning, "WARNING", Triangle, Dots),
            ThemeState::Error => (ColorRole::Danger, "ERROR", Octagon, Hatch),
        };
        StateTokens {
            foreground: self.color(role),
            cue: StateCue {
                label,
                shape,
                pattern,
            },
        }
    }
}
