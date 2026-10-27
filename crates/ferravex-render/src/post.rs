//! Opt-in full-screen post-processing: bloom, tonemapping, grading, vignette and
//! optional film grain, chromatic aberration and scanlines.
//!
//! Games choose a [`PostFx`] with [`set_post_fx`]. The setting is **sticky**: it
//! stays active across frames (and `clear_background`) until replaced, so a game
//! usually sets it once per scene. [`PostFx::off()`] (the default) restores the
//! original direct-to-screen path with zero extra GPU work.
//!
//! While post-processing is active, the world is drawn into an offscreen target.
//! At the **world/HUD boundary** the hosts resolve that target, run the bloom
//! chain and composite the graded image to the screen; every later draw (HUD,
//! menus, the Controls editor) is drawn directly on top, crisp, un-bloomed and
//! un-vignetted. The boundary is the first of:
//!
//! - an explicit [`post_fx_hud_begin`];
//! - `set_world_colors(false)` while world colors are enabled;
//! - `set_default_camera()` while world colors are enabled (that is, after a
//!   `set_camera` in the same frame).
//!
//! The last two follow the existing world-palette boundary and can be disabled
//! with [`PostFx::auto_hud`]. Without a boundary, the whole frame is
//! post-processed. Only the first boundary in a frame counts.
//!
//! Player preferences and accessibility always win over authored settings:
//!
//! - An explicit Low quality choice (Details, `?quality=low` or
//!   `FERRAVEX_QUALITY=low`) disables post-processing. Medium uses a four-level
//!   bloom chain; High uses five levels. Without a player choice, an authored
//!   `set_quality(High)` selects High and anything else selects Medium.
//! - Sustained adaptive-performance pressure (level 2 and above) caps the tier at
//!   Medium.
//! - Reduced flash caps bloom intensity at [`REDUCED_FLASH_BLOOM`] and disables
//!   grain and chromatic aberration.
//!
//! Developers can force a preset for a session with `FERRAVEX_POST=off|subtle|
//! neon|warm-film|crt` (native) or `?post=…` (browser) without editing a game.
//!
//! ```
//! use ferravex_render::{input, post::{self, PostFx, PostTier}};
//! post::set_post_fx(PostFx::neon().vignette(0.25, 0.6));
//! let status = input::host_frame(1. / 60., 320, 200, || {
//!     ferravex_render::clear_background(ferravex_render::BLACK);
//!     // World: bright additive accents feed the bloom.
//!     post::glow_circle(160., 100., 24., ferravex_render::ORANGE, 1.5);
//!     // HUD: drawn after the boundary, it stays crisp.
//!     post::post_fx_hud_begin();
//!     ferravex_render::draw_rectangle(8., 8., 40., 6., ferravex_render::WHITE);
//!     post::post_fx_status()
//! });
//! assert_ne!(status.tier, PostTier::Off);
//! post::set_post_fx(PostFx::off());
//! ```
use crate::{Color, Mat4, Vec3, lighting::Quality};

/// Number of `f32` values in the resolved per-frame host block.
pub const POST_FRAME_FLOATS: usize = 28;
/// Bloom intensity cap applied while the player requests reduced flash.
pub const REDUCED_FLASH_BLOOM: f32 = 0.6;
/// Maximum bloom chain length used by the High tier.
pub const MAX_BLOOM_LEVELS: usize = 5;

/// How bloom and over-bright scene values are mapped to the display.
#[repr(u32)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tonemap {
    /// Add bloom and clip at white. Highlights saturate toward pure hues.
    Clamp = 0,
    /// Identity for ordinary colors; bloom and over-bright values roll off
    /// softly toward white (`1 - (1 - c) * exp(-glow)`). Preserves the
    /// authored palette exactly where nothing glows.
    #[default]
    Soft = 1,
    /// ACES-style filmic curve (Narkowicz fit) applied in approximately
    /// linear light (display gamma 2.2). Mid-grey 0.5 maps to about 0.6 and
    /// white to about 0.9, rolling highlights and bloom off smoothly.
    Filmic = 2,
}

/// Effective post-processing tier after preferences, accessibility and adaptive
/// performance are applied.
#[repr(u32)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum PostTier {
    /// Direct rendering; no offscreen target or extra pass.
    #[default]
    Off = 0,
    /// Four bloom levels starting at half resolution.
    Medium = 1,
    /// Five bloom levels starting at half resolution.
    High = 2,
}

/// Authored post-processing settings. Start from a preset and adjust with the
/// builder methods. Values are sanitized when applied: non-finite numbers use
/// the neutral value and every field is clamped to its documented range.
///
/// ```
/// use ferravex_render::post::{PostFx, Tonemap};
/// let fx = PostFx::subtle()
///     .bloom(0.5, 0.7, 1.2)
///     .tint(ferravex_render::Color::new(1.05, 1., 0.92, 1.))
///     .vignette(0.3, 0.5)
///     .tonemap(Tonemap::Soft);
/// assert!(!fx.is_off());
/// assert!(PostFx::off().is_off());
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PostFx {
    /// Bloom strength, 0–8. Zero skips the bloom chain. The chain is
    /// normalized by its level count, so a value keeps the same overall
    /// energy on Medium and High; small bright shapes need roughly 1–3 for a
    /// clearly visible halo.
    pub bloom_intensity: f32,
    /// Brightness (max RGB channel) where bloom starts, 0–4.
    pub bloom_threshold: f32,
    /// Soft-threshold width around the threshold, 0–1.
    pub bloom_knee: f32,
    /// Upsample filter spread, 0.25–3. One is the dual-filter default.
    pub bloom_radius: f32,
    pub tonemap: Tonemap,
    /// Scene multiplier before tonemapping, 0–8.
    pub exposure: f32,
    /// Power-law contrast around mid-grey (`0.5 * (2c)^contrast`), 0–2.
    /// Black stays black, so dark palettes keep their relative detail.
    pub contrast: f32,
    /// Saturation, 0 (greyscale) – 2.
    pub saturation: f32,
    /// Shadow lift per channel, -0.5–0.5.
    pub lift: [f32; 3],
    /// Midtone gamma per channel, 0.2–5. Above one brightens.
    pub gamma: [f32; 3],
    /// Highlight gain (tint) per channel, 0–4.
    pub gain: [f32; 3],
    /// Corner darkening, 0–1.
    pub vignette: f32,
    /// Vignette falloff width, 0.05 (tight corners) – 1 (from the center).
    pub vignette_softness: f32,
    /// Animated film grain amplitude, 0–0.25. Default off.
    pub grain: f32,
    /// Radial chromatic aberration, 0–1 (1 ≈ 1% of the screen). Default off.
    pub aberration: f32,
    /// Scanline darkening, 0–1, one line every three logical pixels. Default off.
    pub scanlines: f32,
    /// Treat `set_world_colors(false)` / `set_default_camera()` as the HUD
    /// boundary. Default true.
    pub auto_hud: bool,
}

impl Default for PostFx {
    fn default() -> Self {
        Self::off()
    }
}

impl PostFx {
    /// Neutral settings: no post-processing and no extra GPU work.
    pub const fn off() -> Self {
        Self {
            bloom_intensity: 0.,
            bloom_threshold: 0.8,
            bloom_knee: 0.25,
            bloom_radius: 1.,
            tonemap: Tonemap::Soft,
            exposure: 1.,
            contrast: 1.,
            saturation: 1.,
            lift: [0.; 3],
            gamma: [1.; 3],
            gain: [1.; 3],
            vignette: 0.,
            vignette_softness: 0.6,
            grain: 0.,
            aberration: 0.,
            scanlines: 0.,
            auto_hud: true,
        }
    }
    /// Gentle glow on the brightest pixels, slight contrast/saturation and a
    /// light vignette. Unlit areas keep their authored colors.
    pub const fn subtle() -> Self {
        let mut fx = Self::off();
        fx.bloom_intensity = 0.9;
        fx.bloom_threshold = 0.78;
        fx.bloom_knee = 0.25;
        fx.bloom_radius = 1.;
        fx.contrast = 1.04;
        fx.saturation = 1.06;
        fx.vignette = 0.2;
        fx.vignette_softness = 0.55;
        fx
    }
    /// Strong, wide glow for dark scenes with saturated emissive shapes.
    pub const fn neon() -> Self {
        let mut fx = Self::off();
        fx.bloom_intensity = 2.4;
        fx.bloom_threshold = 0.5;
        fx.bloom_knee = 0.3;
        fx.bloom_radius = 1.35;
        fx.contrast = 1.08;
        fx.saturation = 1.18;
        fx.vignette = 0.32;
        fx.vignette_softness = 0.6;
        fx
    }
    /// Filmic curve, warm highlights, lifted shadows, soft vignette and a hint
    /// of grain.
    pub const fn warm_film() -> Self {
        let mut fx = Self::off();
        fx.bloom_intensity = 0.8;
        fx.bloom_threshold = 0.75;
        fx.bloom_knee = 0.25;
        fx.bloom_radius = 1.15;
        fx.tonemap = Tonemap::Filmic;
        fx.exposure = 1.;
        fx.contrast = 1.06;
        fx.saturation = 0.94;
        fx.lift = [0.025, 0.014, 0.];
        fx.gain = [1.05, 0.99, 0.9];
        fx.vignette = 0.38;
        fx.vignette_softness = 0.65;
        fx.grain = 0.03;
        fx
    }
    /// Arcade monitor: scanlines, light aberration, bloom and a deep vignette.
    pub const fn crt() -> Self {
        let mut fx = Self::off();
        fx.bloom_intensity = 1.5;
        fx.bloom_threshold = 0.6;
        fx.bloom_knee = 0.3;
        fx.bloom_radius = 1.2;
        fx.contrast = 1.06;
        fx.saturation = 1.1;
        fx.vignette = 0.45;
        fx.vignette_softness = 0.5;
        fx.aberration = 0.25;
        fx.scanlines = 0.3;
        fx
    }
    /// Preset by name: `off`, `subtle`, `neon`, `warm-film` (or `warm_film`), `crt`.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::from_index(match name {
            "off" => 0,
            "subtle" => 1,
            "neon" => 2,
            "warm-film" | "warm_film" => 3,
            "crt" => 4,
            _ => return None,
        })
    }
    /// Preset by index in [`from_name`](Self::from_name) order.
    pub fn from_index(index: u32) -> Option<Self> {
        Some(match index {
            0 => Self::off(),
            1 => Self::subtle(),
            2 => Self::neon(),
            3 => Self::warm_film(),
            4 => Self::crt(),
            _ => return None,
        })
    }
    pub const fn bloom(mut self, intensity: f32, threshold: f32, radius: f32) -> Self {
        self.bloom_intensity = intensity;
        self.bloom_threshold = threshold;
        self.bloom_radius = radius;
        self
    }
    pub const fn bloom_knee(mut self, knee: f32) -> Self {
        self.bloom_knee = knee;
        self
    }
    pub const fn tonemap(mut self, tonemap: Tonemap) -> Self {
        self.tonemap = tonemap;
        self
    }
    pub const fn exposure(mut self, exposure: f32) -> Self {
        self.exposure = exposure;
        self
    }
    pub const fn contrast(mut self, contrast: f32) -> Self {
        self.contrast = contrast;
        self
    }
    pub const fn saturation(mut self, saturation: f32) -> Self {
        self.saturation = saturation;
        self
    }
    pub const fn lift_gamma_gain(
        mut self,
        lift: [f32; 3],
        gamma: [f32; 3],
        gain: [f32; 3],
    ) -> Self {
        self.lift = lift;
        self.gamma = gamma;
        self.gain = gain;
        self
    }
    /// Multiply highlights by a color (sets `gain`).
    pub const fn tint(mut self, tint: Color) -> Self {
        self.gain = [tint.r, tint.g, tint.b];
        self
    }
    pub const fn vignette(mut self, strength: f32, softness: f32) -> Self {
        self.vignette = strength;
        self.vignette_softness = softness;
        self
    }
    pub const fn grain(mut self, amount: f32) -> Self {
        self.grain = amount;
        self
    }
    pub const fn aberration(mut self, amount: f32) -> Self {
        self.aberration = amount;
        self
    }
    pub const fn scanlines(mut self, amount: f32) -> Self {
        self.scanlines = amount;
        self
    }
    pub const fn auto_hud(mut self, enabled: bool) -> Self {
        self.auto_hud = enabled;
        self
    }
    /// Clamp every field to its documented range; non-finite values become neutral.
    pub fn sanitized(self) -> Self {
        let n = Self::off();
        let f = |v: f32, neutral: f32, lo: f32, hi: f32| {
            if v.is_finite() {
                v.clamp(lo, hi)
            } else {
                neutral
            }
        };
        let rgb = |v: [f32; 3], neutral: [f32; 3], lo: f32, hi: f32| {
            [
                f(v[0], neutral[0], lo, hi),
                f(v[1], neutral[1], lo, hi),
                f(v[2], neutral[2], lo, hi),
            ]
        };
        Self {
            bloom_intensity: f(self.bloom_intensity, 0., 0., 8.),
            bloom_threshold: f(self.bloom_threshold, n.bloom_threshold, 0., 4.),
            bloom_knee: f(self.bloom_knee, n.bloom_knee, 0., 1.),
            bloom_radius: f(self.bloom_radius, 1., 0.25, 3.),
            tonemap: self.tonemap,
            exposure: f(self.exposure, 1., 0., 8.),
            contrast: f(self.contrast, 1., 0., 2.),
            saturation: f(self.saturation, 1., 0., 2.),
            lift: rgb(self.lift, n.lift, -0.5, 0.5),
            gamma: rgb(self.gamma, n.gamma, 0.2, 5.),
            gain: rgb(self.gain, n.gain, 0., 4.),
            vignette: f(self.vignette, 0., 0., 1.),
            vignette_softness: f(self.vignette_softness, n.vignette_softness, 0.05, 1.),
            grain: f(self.grain, 0., 0., 0.25),
            aberration: f(self.aberration, 0., 0., 1.),
            scanlines: f(self.scanlines, 0., 0., 1.),
            auto_hud: self.auto_hud,
        }
    }
    /// True when the sanitized settings are the identity image transform, so
    /// hosts keep their direct path. `Filmic` is never the identity.
    pub fn is_off(&self) -> bool {
        let s = self.sanitized();
        s.bloom_intensity == 0.
            && s.tonemap != Tonemap::Filmic
            && s.exposure == 1.
            && s.contrast == 1.
            && s.saturation == 1.
            && s.lift == [0.; 3]
            && s.gamma == [1.; 3]
            && s.gain == [1.; 3]
            && s.vignette == 0.
            && s.grain == 0.
            && s.aberration == 0.
            && s.scanlines == 0.
    }
}

/// Effective post-processing state for the current settings and preferences.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PostStatus {
    pub tier: PostTier,
    /// Bloom chain length (0 when bloom is disabled or the tier is Off).
    pub bloom_levels: u8,
    /// Settings after accessibility caps (equal to the authored settings,
    /// sanitized, unless reduced flash applies).
    pub effective: PostFx,
    /// A HUD boundary has been recorded in the current frame.
    pub hud_started: bool,
}

/// Replace the sticky post-processing settings. Takes effect for the current
/// frame's composite; use [`PostFx::off()`] to return to direct rendering.
pub fn set_post_fx(fx: PostFx) {
    crate::edit(|s| s.post = fx);
}
/// Current authored settings (before session overrides and preferences).
pub fn post_fx() -> PostFx {
    crate::read(|s| s.post)
}
/// Effective tier, bloom levels and capped settings for the current state.
pub fn post_fx_status() -> PostStatus {
    crate::read(|s| {
        let (tier, effective) = s.post_effective();
        PostStatus {
            tier,
            bloom_levels: levels(tier, &effective),
            effective,
            hud_started: s.post_hud,
        }
    })
}
/// Mark the world/HUD boundary explicitly. Draws after this call go straight to
/// the screen on top of the post-processed world. Returns true when this call
/// recorded the frame's boundary; false if post-processing is off or a
/// boundary already exists in this frame.
pub fn post_fx_hud_begin() -> bool {
    crate::edit(|s| s.post_boundary())
}
/// Draw immediate primitives (rectangles, circles, lines, text, meshes) with
/// additive blending inside `draw`, so bright shapes accumulate light and feed
/// the bloom. Equivalent to `EffectBlend::Additive` for immediate geometry;
/// works with post-processing off as plain additive blending.
///
/// ```
/// use ferravex_render::{input, post, draw_circle, Color};
/// input::host_frame(1. / 60., 64, 64, || {
///     post::with_glow(|| draw_circle(32., 32., 6., Color::new(1., 0.6, 0.2, 0.8)));
/// });
/// ```
pub fn with_glow<T>(draw: impl FnOnce() -> T) -> T {
    let previous = crate::edit(|s| std::mem::replace(&mut s.glow, true));
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            let previous = self.0;
            crate::edit(|s| s.glow = previous);
        }
    }
    let _restore = Restore(previous);
    draw()
}
const GLOW_SEGMENTS: usize = 16;
/// Radius fraction and alpha: a Gaussian-like falloff (`exp(-4.5 r²)`), zero at the rim.
const GLOW_RINGS: [(f32, f32); 5] = [(0., 1.), (0.25, 0.76), (0.5, 0.32), (0.75, 0.08), (1., 0.)];
/// Soft additive radial glow in the current camera's 2D coordinates (z = 0).
/// `intensity` multiplies RGB and may exceed one: over-bright values bloom
/// more strongly on half-float targets. Uses 112 unlit triangles; the world
/// palette still applies inside a world camera.
pub fn glow_circle(x: f32, y: f32, radius: f32, color: Color, intensity: f32) {
    if ![x, y, radius, intensity].iter().all(|v| v.is_finite()) || radius <= 0. || intensity <= 0. {
        return;
    }
    let rgb = [
        color.r * intensity,
        color.g * intensity,
        color.b * intensity,
    ];
    let ring = |ring: usize, segment: usize| {
        let (fraction, alpha) = GLOW_RINGS[ring];
        let angle = segment as f32 * std::f32::consts::TAU / GLOW_SEGMENTS as f32;
        (
            Vec3::new(
                x + angle.cos() * radius * fraction,
                y + angle.sin() * radius * fraction,
                0.,
            ),
            Vec3::Z,
            Color::new(rgb[0], rgb[1], rgb[2], color.a * alpha),
        )
    };
    crate::edit(|s| {
        let glow = std::mem::replace(&mut s.glow, true);
        let lighting = s.lighting.take();
        let triangles = (0..GLOW_SEGMENTS).flat_map(|i| {
            let j = (i + 1) % GLOW_SEGMENTS;
            let fan = [ring(0, i), ring(1, i), ring(1, j)];
            let bands = (1..GLOW_RINGS.len() - 1).flat_map(move |r| {
                [
                    ring(r, i),
                    ring(r + 1, i),
                    ring(r + 1, j),
                    ring(r, i),
                    ring(r + 1, j),
                    ring(r, j),
                ]
            });
            fan.into_iter().chain(bands)
        });
        s.push(triangles, Mat4::IDENTITY);
        s.lighting = lighting;
        s.glow = glow;
    });
}

fn levels(tier: PostTier, fx: &PostFx) -> u8 {
    match tier {
        PostTier::Off => 0,
        _ if fx.bloom_intensity <= 0. => 0,
        PostTier::Medium => 4,
        PostTier::High => MAX_BLOOM_LEVELS as u8,
    }
}

/// Tier from authored settings plus the player's explicit quality choice
/// (`None` follows the authored quality) and adaptive-performance level.
pub(crate) fn tier(
    fx: &PostFx,
    choice: Option<Quality>,
    authored: Quality,
    adaptive: Option<u8>,
) -> PostTier {
    if fx.is_off() {
        return PostTier::Off;
    }
    let tier = match choice {
        Some(Quality::Low) => PostTier::Off,
        Some(Quality::Medium) => PostTier::Medium,
        Some(Quality::High) => PostTier::High,
        None if authored == Quality::High => PostTier::High,
        None => PostTier::Medium,
    };
    if adaptive.is_some_and(|level| level >= 2) {
        tier.min(PostTier::Medium)
    } else {
        tier
    }
}

/// Apply accessibility caps to sanitized settings.
pub(crate) fn accessible(fx: PostFx, reduced_flash: bool) -> PostFx {
    let mut fx = fx.sanitized();
    if reduced_flash {
        fx.bloom_intensity = fx.bloom_intensity.min(REDUCED_FLASH_BLOOM);
        fx.grain = 0.;
        fx.aberration = 0.;
    }
    fx
}

/// Pack the host uniform block (seven vec4 values). Index 0 is the tier; zero
/// means hosts render directly. See `docs/VISUALS.md` for the layout.
pub(crate) fn pack(
    tier: PostTier,
    fx: &PostFx,
    frame: u64,
    reduced_motion: bool,
    dpi: f32,
) -> [f32; POST_FRAME_FLOATS] {
    if tier == PostTier::Off {
        return [0.; POST_FRAME_FLOATS];
    }
    let seed = if reduced_motion {
        0.
    } else {
        (frame % 1024) as f32
    };
    let dpi = if dpi.is_finite() {
        dpi.clamp(0.25, 8.)
    } else {
        1.
    };
    [
        tier as u32 as f32,
        levels(tier, fx) as f32,
        seed,
        fx.tonemap as u32 as f32,
        fx.bloom_intensity,
        fx.bloom_threshold,
        fx.bloom_knee,
        fx.bloom_radius,
        fx.exposure,
        fx.contrast,
        fx.saturation,
        fx.vignette,
        fx.lift[0],
        fx.lift[1],
        fx.lift[2],
        fx.vignette_softness,
        fx.gamma[0],
        fx.gamma[1],
        fx.gamma[2],
        fx.grain,
        fx.gain[0],
        fx.gain[1],
        fx.gain[2],
        fx.aberration,
        fx.scanlines,
        (3. * dpi).max(2.),
        0.,
        0.,
    ]
}

/// Full-screen triangle; no vertex attributes. Hosts prepend their GLSL
/// version/precision header.
pub const POST_VERTEX_BODY: &str = r#"
out vec2 v_uv;
void main(){vec2 p=vec2(float((gl_VertexID<<1)&2),float(gl_VertexID&2));v_uv=p;gl_Position=vec4(p*2.-1.,0.,1.);}
"#;
/// Dual-filter downsample. `u_texel.xy` is the source texel size; `u_texel.z`
/// selects the bright-pass prefilter (threshold `u_bloom.x`, knee `u_bloom.y`).
pub const POST_DOWN_FRAGMENT_BODY: &str = r#"
in vec2 v_uv;
out vec4 ferravex_pixel;
uniform sampler2D u_source;
uniform vec4 u_texel;
uniform vec4 u_bloom;
void main(){
 vec2 o=u_texel.xy;
 vec3 c=texture(u_source,v_uv).rgb*4.;
 c+=texture(u_source,v_uv-o).rgb;
 c+=texture(u_source,v_uv+o).rgb;
 c+=texture(u_source,v_uv+vec2(o.x,-o.y)).rgb;
 c+=texture(u_source,v_uv-vec2(o.x,-o.y)).rgb;
 c*=0.125;
 if(u_texel.z>0.5){
  float b=max(c.r,max(c.g,c.b));
  float k=max(u_bloom.y,0.0001);
  float s=clamp(b-u_bloom.x+k,0.,2.*k);
  s=s*s/(4.*k);
  c*=max(s,b-u_bloom.x)/max(b,0.0001);
 }
 ferravex_pixel=vec4(max(c,vec3(0.)),1.);
}
"#;
/// Dual-filter tent upsample. `u_texel.xy` is the source texel size and
/// `u_texel.z` the radius; hosts additively blend into the larger level.
pub const POST_UP_FRAGMENT_BODY: &str = r#"
in vec2 v_uv;
out vec4 ferravex_pixel;
uniform sampler2D u_source;
uniform vec4 u_texel;
void main(){
 vec2 o=u_texel.xy*u_texel.z;
 vec3 c=texture(u_source,v_uv+vec2(-2.*o.x,0.)).rgb;
 c+=texture(u_source,v_uv+vec2(-o.x,o.y)).rgb*2.;
 c+=texture(u_source,v_uv+vec2(0.,2.*o.y)).rgb;
 c+=texture(u_source,v_uv+vec2(o.x,o.y)).rgb*2.;
 c+=texture(u_source,v_uv+vec2(2.*o.x,0.)).rgb;
 c+=texture(u_source,v_uv+vec2(o.x,-o.y)).rgb*2.;
 c+=texture(u_source,v_uv+vec2(0.,-2.*o.y)).rgb;
 c+=texture(u_source,v_uv+vec2(-o.x,-o.y)).rgb*2.;
 ferravex_pixel=vec4(c/12.,1.);
}
"#;
/// Final composite: bloom upsample, exposure, tonemap, lift/gamma/gain,
/// power-law contrast, saturation, vignette, scanlines, grain and a static
/// sub-LSB dither (interleaved gradient noise) against 8-bit banding. `u_texel.xy` is the
/// bloom texel size and `u_texel.zw` the scene texel size.
pub const POST_COMPOSITE_FRAGMENT_BODY: &str = r#"
in vec2 v_uv;
out vec4 ferravex_pixel;
uniform sampler2D u_scene;
uniform sampler2D u_bloom;
uniform vec4 u_texel;
uniform vec4 u_post[7];
vec3 ferravex_bloom(vec2 uv){
 vec2 o=u_texel.xy*u_post[1].w;
 vec3 c=texture(u_bloom,uv+vec2(-2.*o.x,0.)).rgb;
 c+=texture(u_bloom,uv+vec2(-o.x,o.y)).rgb*2.;
 c+=texture(u_bloom,uv+vec2(0.,2.*o.y)).rgb;
 c+=texture(u_bloom,uv+vec2(o.x,o.y)).rgb*2.;
 c+=texture(u_bloom,uv+vec2(2.*o.x,0.)).rgb;
 c+=texture(u_bloom,uv+vec2(o.x,-o.y)).rgb*2.;
 c+=texture(u_bloom,uv+vec2(0.,-2.*o.y)).rgb;
 c+=texture(u_bloom,uv+vec2(-o.x,-o.y)).rgb*2.;
 return c/12.;
}
float ferravex_noise(vec2 p){return fract(52.9829189*fract(dot(p,vec2(0.06711056,0.00583715))));}
vec3 ferravex_filmic(vec3 x){return clamp(x*(2.51*x+0.03)/(x*(2.43*x+0.59)+0.14),0.,1.);}
void main(){
 vec2 uv=v_uv;
 vec3 scene;
 if(u_post[5].w>0.){
  vec2 d=(uv-0.5)*u_post[5].w*0.02;
  scene=vec3(texture(u_scene,uv+d).r,texture(u_scene,uv).g,texture(u_scene,uv-d).b);
 }else scene=texture(u_scene,uv).rgb;
 vec3 glow=vec3(0.);
 if(u_post[0].y>0.5)glow=ferravex_bloom(uv)*(u_post[1].x/u_post[0].y);
 vec3 c=max(scene*u_post[2].x,vec3(0.));
 if(u_post[0].w>1.5)c=pow(ferravex_filmic(pow(c+glow,vec3(2.2))),vec3(1./2.2));
 else if(u_post[0].w>0.5)c=1.-(1.-min(c,vec3(1.)))*exp(-(glow+max(c-1.,vec3(0.))));
 else c=min(c+glow,vec3(1.));
 c=clamp(u_post[5].rgb*(c+u_post[3].rgb*(1.-c)),0.,1.);
 c=pow(c,1./max(u_post[4].rgb,vec3(0.01)));
 c=0.5*pow(c*2.,vec3(u_post[2].y));
 float luma=dot(c,vec3(0.2126,0.7152,0.0722));
 c=mix(vec3(luma),c,u_post[2].z);
 if(u_post[2].w>0.){
  float aspect=u_texel.w/u_texel.z;
  vec2 q=(uv-0.5)*vec2(aspect,1.);
  float d=length(q)/length(vec2(aspect,1.)*0.5);
  c*=1.-u_post[2].w*smoothstep(1.-u_post[3].w,1.,d);
 }
 if(u_post[6].x>0.)c*=1.-u_post[6].x*(0.5-0.5*cos(gl_FragCoord.y*6.2831853/u_post[6].y));
 if(u_post[4].w>0.){
  vec2 p=gl_FragCoord.xy+5.588238*u_post[0].z;
  c+=(ferravex_noise(p)-0.5)*u_post[4].w;
 }
 // Static sub-LSB dither hides 8-bit banding in dark vignette/bloom gradients.
 c+=(ferravex_noise(gl_FragCoord.xy+vec2(37.,17.))-0.5)/255.;
 ferravex_pixel=vec4(clamp(c,0.,1.),1.);
}
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{input::host_frame, retained::Command};

    fn reset() {
        crate::edit(|s| *s = crate::State::default());
        crate::preferences::set_feedback_policy(Default::default());
    }
    fn kinds() -> Vec<u32> {
        crate::read(|s| {
            s.retained
                .commands
                .iter()
                .map(|c: &Command| c.kind)
                .collect()
        })
    }

    #[test]
    fn presets_sanitize_and_off_is_identity() {
        for name in ["off", "subtle", "neon", "warm-film", "crt"] {
            let fx = PostFx::from_name(name).unwrap();
            assert_eq!(fx.sanitized(), fx, "{name} preset is in range");
            assert_eq!(fx.is_off(), name == "off");
        }
        assert_eq!(PostFx::from_name("warm_film"), Some(PostFx::warm_film()));
        assert_eq!(PostFx::from_name("bright"), None);
        assert_eq!(PostFx::default(), PostFx::off());
        let wild = PostFx::off()
            .bloom(f32::NAN, 9., -1.)
            .exposure(f32::INFINITY)
            .vignette(3., 0.)
            .grain(1.)
            .lift_gamma_gain([-2.; 3], [0.; 3], [9.; 3]);
        let s = wild.sanitized();
        assert_eq!(
            (s.bloom_intensity, s.bloom_threshold, s.bloom_radius),
            (0., 4., 0.25)
        );
        assert_eq!(
            (s.exposure, s.vignette, s.vignette_softness),
            (1., 1., 0.05)
        );
        assert_eq!(
            (s.grain, s.lift, s.gamma, s.gain),
            (0.25, [-0.5; 3], [0.2; 3], [4.; 3])
        );
        // A filmic curve alone is not the identity transform.
        assert!(!PostFx::off().tonemap(Tonemap::Filmic).is_off());
        assert!(PostFx::off().tonemap(Tonemap::Clamp).is_off());
    }

    #[test]
    fn quality_preferences_and_adaptive_pressure_select_tiers() {
        let fx = PostFx::neon();
        assert_eq!(
            tier(&PostFx::off(), Some(Quality::High), Quality::High, None),
            PostTier::Off
        );
        assert_eq!(
            tier(&fx, Some(Quality::Low), Quality::High, None),
            PostTier::Off
        );
        assert_eq!(
            tier(&fx, Some(Quality::Medium), Quality::High, None),
            PostTier::Medium
        );
        assert_eq!(
            tier(&fx, Some(Quality::High), Quality::Low, None),
            PostTier::High
        );
        // Authored Low (the engine default) does not disable an opted-in game.
        assert_eq!(tier(&fx, None, Quality::Low, None), PostTier::Medium);
        assert_eq!(tier(&fx, None, Quality::High, None), PostTier::High);
        assert_eq!(tier(&fx, None, Quality::High, Some(1)), PostTier::High);
        assert_eq!(tier(&fx, None, Quality::High, Some(2)), PostTier::Medium);
        assert_eq!(levels(PostTier::Medium, &fx), 4);
        assert_eq!(levels(PostTier::High, &fx), 5);
        assert_eq!(levels(PostTier::High, &PostFx::off().vignette(0.3, 0.5)), 0);
    }

    #[test]
    fn reduced_flash_caps_bloom_and_disables_grain_and_aberration() {
        let fx = PostFx::crt().bloom(3., 0.5, 1.).grain(0.1);
        let capped = accessible(fx, true);
        assert_eq!(capped.bloom_intensity, REDUCED_FLASH_BLOOM);
        assert_eq!((capped.grain, capped.aberration), (0., 0.));
        assert_eq!(capped.scanlines, fx.scanlines);
        assert_eq!(accessible(fx, false), fx.sanitized());
        let packed = pack(PostTier::High, &capped, 7, false, 2.);
        assert_eq!(&packed[..4], &[2., 5., 7., 1.]);
        assert_eq!(packed[4], REDUCED_FLASH_BLOOM);
        assert_eq!(packed[25], 6.);
        assert_eq!(
            pack(PostTier::High, &capped, 7, true, 1.)[2],
            0.,
            "static grain"
        );
        assert_eq!(
            pack(PostTier::Off, &capped, 7, false, 1.),
            [0.; POST_FRAME_FLOATS]
        );
    }

    #[test]
    fn hud_boundary_follows_world_palette_rules_once_per_frame() {
        reset();
        // Off: no boundary command and an all-zero host block.
        host_frame(1. / 60., 64, 64, || {
            crate::set_camera(&crate::Camera2D::default());
            crate::draw_rectangle(0., 0., 1., 1., crate::WHITE);
            crate::set_default_camera();
            assert!(!post_fx_hud_begin());
            crate::draw_rectangle(0., 0., 1., 1., crate::WHITE);
        });
        assert!(!kinds().contains(&4));
        assert_eq!(crate::read(|s| s.retained.post), [0.; POST_FRAME_FLOATS]);

        set_post_fx(PostFx::subtle());
        host_frame(1. / 60., 64, 64, || {
            // Default-camera drawing before any world camera is world content.
            crate::draw_rectangle(0., 0., 1., 1., crate::WHITE);
            crate::set_default_camera();
            crate::set_camera(&crate::Camera2D::default());
            crate::draw_rectangle(0., 0., 1., 1., crate::WHITE);
            crate::set_default_camera();
            crate::draw_rectangle(0., 0., 1., 1., crate::WHITE);
            crate::set_camera(&crate::Camera2D::default());
            crate::set_default_camera();
            assert!(!post_fx_hud_begin(), "only the first boundary counts");
            assert!(post_fx_status().hud_started);
        });
        assert_eq!(kinds(), [0, 4, 0]);
        let block = crate::read(|s| s.retained.post);
        assert_eq!((block[0], block[1]), (1., 4.), "medium by default");

        host_frame(1. / 60., 64, 64, || {
            crate::set_world_colors(true);
            crate::draw_rectangle(0., 0., 1., 1., crate::WHITE);
            crate::set_world_colors(false);
            crate::draw_rectangle(0., 0., 1., 1., crate::WHITE);
        });
        assert_eq!(kinds(), [0, 4, 0], "set_world_colors(false) is a boundary");

        set_post_fx(PostFx::subtle().auto_hud(false));
        host_frame(1. / 60., 64, 64, || {
            crate::set_camera(&crate::Camera2D::default());
            crate::draw_rectangle(0., 0., 1., 1., crate::WHITE);
            crate::set_default_camera();
            crate::draw_rectangle(0., 0., 1., 1., crate::WHITE);
            assert!(!post_fx_status().hud_started);
            assert!(post_fx_hud_begin());
            crate::draw_rectangle(0., 0., 1., 1., crate::WHITE);
            // clear_background restarts the frame's command stream.
            crate::clear_background(crate::BLACK);
            assert!(!post_fx_status().hud_started);
        });
        assert!(kinds().is_empty());
        reset();
    }

    #[test]
    fn preferences_resolve_the_host_block_each_frame() {
        reset();
        set_post_fx(PostFx::warm_film());
        crate::set_quality(Quality::High);
        host_frame(1. / 60., 64, 64, || {});
        assert_eq!(crate::read(|s| s.retained.post[0]), 2.);
        crate::set_quality_preference(Some(Quality::Low));
        host_frame(1. / 60., 64, 64, || {
            crate::set_camera(&crate::Camera2D::default());
            crate::set_default_camera();
        });
        assert_eq!(crate::read(|s| s.retained.post), [0.; POST_FRAME_FLOATS]);
        assert!(!kinds().contains(&4), "no boundary while effectively off");
        crate::set_quality_preference(None);
        let policy = ferravex_play::feedback::FeedbackPolicy::new(false, true, 1.).unwrap();
        crate::preferences::set_feedback_policy(policy);
        host_frame(1. / 60., 64, 64, || {});
        let block = crate::read(|s| s.retained.post);
        assert_eq!(
            (block[0], block[19], block[23]),
            (2., 0., 0.),
            "grain/aberration off"
        );
        assert!(block[4] <= REDUCED_FLASH_BLOOM);
        // Session override wins over authored settings, including `off`.
        crate::edit(|s| s.post_override = Some(PostFx::off()));
        host_frame(1. / 60., 64, 64, || {});
        assert_eq!(crate::read(|s| s.retained.post[0]), 0.);
        reset();
    }

    #[test]
    fn glow_draws_are_additive_effect_batches_and_restore_state() {
        reset();
        host_frame(1. / 60., 64, 64, || {
            crate::draw_rectangle(0., 0., 4., 4., crate::WHITE);
            with_glow(|| {
                crate::draw_rectangle(0., 0., 4., 4., crate::WHITE);
                crate::draw_circle(8., 8., 2., crate::RED);
            });
            glow_circle(32., 32., 10., crate::ORANGE, 2.);
            glow_circle(32., 32., f32::NAN, crate::ORANGE, 2.);
            crate::draw_rectangle(0., 0., 4., 4., crate::WHITE);
        });
        assert_eq!(kinds(), [0, 2, 0]);
        crate::read(|s| {
            let batch = s.retained.effects[0];
            assert_eq!(batch.flags, 2 | 4, "additive, no depth write");
            assert!(batch.count >= 6 + 112 * 3);
            // Glow color keeps its over-bright intensity.
            let end = (batch.start + batch.count) as usize;
            assert!(s.vertices[end - 1][4..7].iter().any(|&c| c > 1.));
            assert!(!s.glow);
        });
        reset();
    }
}
