//! Orbit's pictograms for the CPU raster.
//!
//! The engine's `ferravex_render::icons` draw through the immediate GPU
//! batch, which never reaches this software framebuffer, so Orbit keeps a
//! small matching vocabulary of its own (star, trophy, bolt, shield, …) plus
//! game glyphs (panel, satellite, capsules). Each icon is a few layers of
//! signed-distance shapes in a unit box (`[-1, 1]²`, Y down). Every pixel of
//! the box evaluates the union of a layer's shapes once, so overlapping parts
//! never double-blend at their seams, translucent icons fade evenly and the
//! same art stays crisp from 10 to 48 pixels. Drawing never allocates.
use super::{Canvas, INK, PAPER, alpha, mix, scale_rgb};

/// A pictogram. Shared ideas use the engine's names (`Star`, `Trophy`, …).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Icon {
    /// Points.
    Star,
    /// Personal best.
    Trophy,
    /// The top title.
    Crown,
    /// The next title to chase.
    Flag,
    /// A fully charged, armor-piercing serve.
    Bolt,
    /// The opening ball save.
    Shield,
    /// Precise center returns.
    Target,
    /// Volleys: panel hits since the last paddle touch.
    Flame,
    /// Run time and countdowns.
    Clock,
    /// Flawless sectors.
    Gem,
    /// Resume.
    Play,
    /// Pause.
    Pause,
    /// Restart and retry.
    Restart,
    /// Launch.
    ArrowUp,
    ArrowLeft,
    ArrowRight,
    /// Steering nudges beside the paddle.
    ChevronLeft,
    ChevronRight,
    /// Launch nudges above a waiting ball.
    ChevronUp,
    /// Touch steering.
    Hand,
    /// An undiscovered secret.
    Question,
    /// A destructible panel.
    Panel,
    /// The bonus satellite.
    Satellite,
    /// A ball (and the backup ball).
    Ball,
    /// A ball that drained.
    BallLost,
    /// The three capsules, matching the falling drops.
    Wide,
    Multi,
    Sticky,
}

/// How a layer is colored relative to the requested color.
#[derive(Clone, Copy)]
enum Tone {
    Main,
    /// Inner marks: the color pushed toward black.
    Dark,
    /// A capsule's tinted shell.
    Deep,
    /// Highlights: the color pushed toward white.
    Light,
    /// Bright glyphs on a capsule.
    Paper,
}

/// Signed-distance primitives in unit-box coordinates. Angles are degrees,
/// clockwise on screen (Y down) from +X.
#[derive(Clone, Copy)]
enum Shape {
    Disc([f32; 2], f32),
    /// Center, radius, stroke width.
    Ring([f32; 2], f32, f32),
    /// Round-capped segment: ends and radius.
    Line([f32; 2], [f32; 2], f32),
    /// Rounded box: min, max and corner radius.
    Box([f32; 2], [f32; 2], f32),
    /// Simple polygon (convex or concave), even-odd filled.
    Poly(&'static [[f32; 2]]),
    /// Round-capped arc: center, radius, start, sweep, stroke width.
    Arc([f32; 2], f32, f32, f32, f32),
}

struct Layer {
    tone: Tone,
    shapes: &'static [Shape],
}

const fn main(shapes: &'static [Shape]) -> Layer {
    Layer {
        tone: Tone::Main,
        shapes,
    }
}
const fn dark(shapes: &'static [Shape]) -> Layer {
    Layer {
        tone: Tone::Dark,
        shapes,
    }
}
const fn light(shapes: &'static [Shape]) -> Layer {
    Layer {
        tone: Tone::Light,
        shapes,
    }
}

use Shape::{Arc, Box, Disc, Line, Poly, Ring};

const STAR: &[Layer] = &[main(&[Poly(&[
    [0., -0.9],
    [0.25, -0.25],
    [0.94, -0.21],
    [0.4, 0.22],
    [0.58, 0.89],
    [0., 0.51],
    [-0.58, 0.89],
    [-0.4, 0.22],
    [-0.94, -0.21],
    [-0.25, -0.25],
])])];

const TROPHY: &[Layer] = &[
    main(&[
        Poly(&[
            [-0.6, -0.84],
            [0.6, -0.84],
            [0.57, -0.4],
            [0.44, -0.1],
            [0.22, 0.1],
            [0., 0.16],
            [-0.22, 0.1],
            [-0.44, -0.1],
            [-0.57, -0.4],
        ]),
        Arc([-0.58, -0.48], 0.25, 90., 180., 0.15),
        Arc([0.58, -0.48], 0.25, 270., 180., 0.15),
        Box([-0.11, 0.08], [0.11, 0.58], 0.),
        Box([-0.48, 0.52], [0.48, 0.86], 0.1),
    ]),
    dark(&[Box([-0.48, 0.7], [0.48, 0.86], 0.08)]),
    light(&[Line([-0.36, -0.64], [-0.3, -0.3], 0.07)]),
];

const CROWN: &[Layer] = &[
    main(&[
        Poly(&[
            [-0.84, 0.6],
            [-0.88, -0.42],
            [-0.44, 0.02],
            [0., -0.6],
            [0.44, 0.02],
            [0.88, -0.42],
            [0.84, 0.6],
        ]),
        Disc([-0.88, -0.52], 0.15),
        Disc([0., -0.72], 0.15),
        Disc([0.88, -0.52], 0.15),
    ]),
    dark(&[Box([-0.84, 0.36], [0.84, 0.6], 0.04)]),
];

const FLAG: &[Layer] = &[main(&[
    Line([-0.6, -0.86], [-0.6, 0.9], 0.1),
    Poly(&[[-0.56, -0.88], [0.82, -0.5], [-0.56, -0.1]]),
])];

const BOLT: &[Layer] = &[main(&[Poly(&[
    [0.24, -0.96],
    [-0.6, 0.14],
    [-0.06, 0.14],
    [-0.26, 0.96],
    [0.62, -0.2],
    [0.06, -0.2],
])])];

const SHIELD: &[Layer] = &[
    main(&[Poly(&[
        [-0.74, -0.72],
        [0., -0.96],
        [0.74, -0.72],
        [0.72, -0.06],
        [0.5, 0.48],
        [0., 0.95],
        [-0.5, 0.48],
        [-0.72, -0.06],
    ])]),
    dark(&[Poly(&[
        [0., -0.96],
        [0.74, -0.72],
        [0.72, -0.06],
        [0.5, 0.48],
        [0., 0.95],
    ])]),
];

const TARGET: &[Layer] = &[main(&[
    Ring([0., 0.], 0.78, 0.22),
    Ring([0., 0.], 0.4, 0.22),
    Disc([0., 0.], 0.15),
])];

const FLAME: &[Layer] = &[
    main(&[
        Disc([0., 0.36], 0.56),
        Poly(&[
            [0.06, -0.98],
            [0.5, -0.3],
            [0.56, 0.3],
            [-0.56, 0.3],
            [-0.44, -0.1],
            [-0.22, -0.02],
            [-0.16, -0.48],
        ]),
    ]),
    light(&[
        Disc([0., 0.5], 0.3),
        Poly(&[[0.02, -0.22], [0.3, 0.42], [-0.3, 0.42]]),
    ]),
];

const CLOCK: &[Layer] = &[
    main(&[Disc([0., 0.], 0.92)]),
    dark(&[
        Line([0., 0.], [0., -0.58], 0.11),
        Line([0., 0.], [0.42, 0.16], 0.11),
    ]),
];

const GEM: &[Layer] = &[
    main(&[Poly(&[
        [-0.5, -0.72],
        [0.5, -0.72],
        [0.94, -0.2],
        [0., 0.92],
        [-0.94, -0.2],
    ])]),
    light(&[Poly(&[
        [-0.5, -0.72],
        [0., -0.72],
        [-0.3, -0.2],
        [-0.94, -0.2],
    ])]),
    dark(&[Poly(&[[0.25, -0.2], [0.94, -0.2], [0., 0.92]])]),
];

const PLAY: &[Layer] = &[main(&[Poly(&[[-0.52, -0.84], [0.86, 0.], [-0.52, 0.84]])])];

const PAUSE: &[Layer] = &[main(&[
    Box([-0.64, -0.78], [-0.16, 0.78], 0.14),
    Box([0.16, -0.78], [0.64, 0.78], 0.14),
])];

const RESTART: &[Layer] = &[main(&[
    Arc([0., 0.08], 0.6, -30., 270., 0.26),
    Poly(&[[0.1, -0.7], [-0.14, -0.18], [-0.46, -0.74]]),
])];

const ARROW_UP: &[Layer] = &[main(&[Poly(&[
    [0., -0.92],
    [0.8, -0.06],
    [0.3, -0.06],
    [0.3, 0.88],
    [-0.3, 0.88],
    [-0.3, -0.06],
    [-0.8, -0.06],
])])];

const ARROW_LEFT: &[Layer] = &[main(&[Poly(&[
    [-0.92, 0.],
    [-0.06, -0.8],
    [-0.06, -0.3],
    [0.88, -0.3],
    [0.88, 0.3],
    [-0.06, 0.3],
    [-0.06, 0.8],
])])];

const ARROW_RIGHT: &[Layer] = &[main(&[Poly(&[
    [0.92, 0.],
    [0.06, 0.8],
    [0.06, 0.3],
    [-0.88, 0.3],
    [-0.88, -0.3],
    [0.06, -0.3],
    [0.06, -0.8],
])])];

const CHEVRON_LEFT: &[Layer] = &[main(&[
    Line([0.32, -0.72], [-0.36, 0.], 0.2),
    Line([-0.36, 0.], [0.32, 0.72], 0.2),
])];

const CHEVRON_RIGHT: &[Layer] = &[main(&[
    Line([-0.32, -0.72], [0.36, 0.], 0.2),
    Line([0.36, 0.], [-0.32, 0.72], 0.2),
])];

const CHEVRON_UP: &[Layer] = &[main(&[
    Line([-0.72, 0.32], [0., -0.36], 0.2),
    Line([0., -0.36], [0.72, 0.32], 0.2),
])];

const HAND: &[Layer] = &[main(&[
    Line([-0.18, -0.86], [-0.18, 0.1], 0.17),
    Disc([0.1, -0.12], 0.17),
    Disc([0.37, -0.04], 0.16),
    Box([-0.36, -0.12], [0.55, 0.84], 0.3),
    Line([-0.32, 0.4], [-0.64, 0.04], 0.15),
])];

const QUESTION: &[Layer] = &[main(&[
    Arc([0., -0.4], 0.38, 180., 230., 0.26),
    Line([0.24, -0.11], [0.02, 0.22], 0.13),
    Disc([0., 0.72], 0.16),
])];

const PANEL: &[Layer] = &[
    main(&[Box([-0.96, -0.5], [0.96, 0.5], 0.22)]),
    light(&[Line([-0.7, -0.34], [0.7, -0.34], 0.06)]),
    dark(&[Disc([-0.68, -0.08], 0.1), Disc([0.68, -0.08], 0.1)]),
];

const SATELLITE: &[Layer] = &[
    main(&[
        Box([-0.26, -0.28], [0.26, 0.28], 0.1),
        Box([-0.96, -0.2], [-0.42, 0.2], 0.05),
        Box([0.42, -0.2], [0.96, 0.2], 0.05),
        Line([-0.45, 0.], [0.45, 0.], 0.05),
        Line([0.08, -0.28], [0.3, -0.62], 0.06),
        Disc([0.32, -0.7], 0.12),
    ]),
    dark(&[
        Line([-0.69, -0.16], [-0.69, 0.16], 0.04),
        Line([0.69, -0.16], [0.69, 0.16], 0.04),
    ]),
];

const BALL: &[Layer] = &[
    main(&[Disc([0., 0.], 0.8)]),
    light(&[Disc([-0.26, -0.26], 0.27)]),
];

const BALL_LOST: &[Layer] = &[main(&[
    Ring([0., 0.], 0.66, 0.17),
    Line([-0.66, 0.66], [0.66, -0.66], 0.1),
])];

/// Capsule shell and rim shared by the three capsules (20 x 16 at size 20).
const SHELL: Layer = Layer {
    tone: Tone::Deep,
    shapes: &[Box([-1., -0.8], [1., 0.8], 0.6)],
};
const RIM: Layer = main(&[Line([-0.7, -0.6], [0.7, -0.6], 0.1)]);

const WIDE: &[Layer] = &[
    SHELL,
    RIM,
    Layer {
        tone: Tone::Paper,
        shapes: &[Box([-0.6, -0.1], [0.6, 0.3], 0.2)],
    },
];

const MULTI: &[Layer] = &[
    SHELL,
    RIM,
    Layer {
        tone: Tone::Paper,
        shapes: &[
            Disc([-0.5, 0.2], 0.21),
            Disc([0., -0.2], 0.21),
            Disc([0.5, 0.2], 0.21),
        ],
    },
];

const STICKY: &[Layer] = &[
    SHELL,
    RIM,
    Layer {
        tone: Tone::Paper,
        shapes: &[
            Box([-0.6, -0.3], [0.6, 0.], 0.1),
            Box([-0.4, -0.1], [-0.2, 0.2], 0.1),
            Box([0., -0.1], [0.2, 0.2], 0.1),
            Box([0.3, -0.1], [0.5, 0.5], 0.1),
        ],
    },
];

fn layers(icon: Icon) -> &'static [Layer] {
    match icon {
        Icon::Star => STAR,
        Icon::Trophy => TROPHY,
        Icon::Crown => CROWN,
        Icon::Flag => FLAG,
        Icon::Bolt => BOLT,
        Icon::Shield => SHIELD,
        Icon::Target => TARGET,
        Icon::Flame => FLAME,
        Icon::Clock => CLOCK,
        Icon::Gem => GEM,
        Icon::Play => PLAY,
        Icon::Pause => PAUSE,
        Icon::Restart => RESTART,
        Icon::ArrowUp => ARROW_UP,
        Icon::ArrowLeft => ARROW_LEFT,
        Icon::ArrowRight => ARROW_RIGHT,
        Icon::ChevronLeft => CHEVRON_LEFT,
        Icon::ChevronRight => CHEVRON_RIGHT,
        Icon::ChevronUp => CHEVRON_UP,
        Icon::Hand => HAND,
        Icon::Question => QUESTION,
        Icon::Panel => PANEL,
        Icon::Satellite => SATELLITE,
        Icon::Ball => BALL,
        Icon::BallLost => BALL_LOST,
        Icon::Wide => WIDE,
        Icon::Multi => MULTI,
        Icon::Sticky => STICKY,
    }
}

fn length(v: [f32; 2]) -> f32 {
    v[0].hypot(v[1])
}

fn segment(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let ab = [b[0] - a[0], b[1] - a[1]];
    let ap = [p[0] - a[0], p[1] - a[1]];
    let t =
        ((ap[0] * ab[0] + ap[1] * ab[1]) / (ab[0] * ab[0] + ab[1] * ab[1]).max(1e-6)).clamp(0., 1.);
    length([ap[0] - ab[0] * t, ap[1] - ab[1] * t])
}

/// Signed distance in unit-box units; negative inside.
fn distance(shape: &Shape, p: [f32; 2]) -> f32 {
    match *shape {
        Disc(c, r) => length([p[0] - c[0], p[1] - c[1]]) - r,
        Ring(c, r, w) => (length([p[0] - c[0], p[1] - c[1]]) - r).abs() - w * 0.5,
        Line(a, b, r) => segment(p, a, b) - r,
        Box(min, max, r) => {
            let center = [(min[0] + max[0]) * 0.5, (min[1] + max[1]) * 0.5];
            let q = [
                (p[0] - center[0]).abs() - ((max[0] - min[0]) * 0.5 - r),
                (p[1] - center[1]).abs() - ((max[1] - min[1]) * 0.5 - r),
            ];
            length([q[0].max(0.), q[1].max(0.)]) + q[0].max(q[1]).min(0.) - r
        }
        Poly(points) => {
            let mut nearest = f32::INFINITY;
            let mut inside = false;
            for (i, &a) in points.iter().enumerate() {
                let b = points[(i + 1) % points.len()];
                nearest = nearest.min(segment(p, a, b));
                if (a[1] > p[1]) != (b[1] > p[1])
                    && p[0] < a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0])
                {
                    inside = !inside;
                }
            }
            if inside { -nearest } else { nearest }
        }
        Arc(c, r, start, sweep, w) => {
            let v = [p[0] - c[0], p[1] - c[1]];
            let start = start.to_radians();
            let sweep = sweep.to_radians();
            let relative = (v[1].atan2(v[0]) - start).rem_euclid(std::f32::consts::TAU);
            if relative <= sweep {
                (length(v) - r).abs() - w * 0.5
            } else {
                let end = start + sweep;
                let a = [c[0] + r * start.cos(), c[1] + r * start.sin()];
                let b = [c[0] + r * end.cos(), c[1] + r * end.sin()];
                length([p[0] - a[0], p[1] - a[1]]).min(length([p[0] - b[0], p[1] - b[1]])) - w * 0.5
            }
        }
    }
}

impl Canvas {
    /// Draw `icon` centered on `(cx, cy)` in a `size`-pixel box. The color's
    /// alpha fades the whole icon evenly. Honors the playfield camera kick.
    pub(crate) fn icon(&mut self, icon: Icon, cx: f32, cy: f32, size: f32, color: [u8; 4]) {
        if color[3] == 0 || !(size >= 4. && cx.is_finite() && cy.is_finite()) {
            return;
        }
        let tones = [
            self.tint(color),
            self.tint(mix(color, [0, 0, 0, color[3]], 0.42)),
            self.tint(scale_rgb(color, 0.36)),
            self.tint(mix(color, [255, 255, 255, color[3]], 0.55)),
            self.tint(alpha(PAPER, f32::from(color[3]) / 255.)),
        ];
        self.paint_icon(icon, cx, cy, size, |tone| match tone {
            Tone::Main => tones[0],
            Tone::Dark => tones[1],
            Tone::Deep => tones[2],
            Tone::Light => tones[3],
            Tone::Paper => tones[4],
        });
    }

    /// [`Canvas::icon`] over a one-pixel ink shadow, for HUD legibility.
    pub(crate) fn icon_shadowed(
        &mut self,
        icon: Icon,
        cx: f32,
        cy: f32,
        size: f32,
        color: [u8; 4],
    ) {
        let ink = self.tint(alpha(INK, f32::from(color[3]) / 255. * 0.85));
        // One union of every layer, so the shadow has no inner seams.
        self.paint_silhouette(icon, cx + 1., cy + 1., size, ink);
        self.icon(icon, cx, cy, size, color);
    }

    fn paint_icon(
        &mut self,
        icon: Icon,
        cx: f32,
        cy: f32,
        size: f32,
        color: impl Fn(Tone) -> [u8; 4],
    ) {
        let (cx, cy) = (cx + self.shift[0] as f32, cy + self.shift[1] as f32);
        let half = size * 0.5;
        let reach = half * 1.08 + 1.;
        for py in (cy - reach).floor() as i32..=(cy + reach).ceil() as i32 {
            for px in (cx - reach).floor() as i32..=(cx + reach).ceil() as i32 {
                let p = [(px as f32 + 0.5 - cx) / half, (py as f32 + 0.5 - cy) / half];
                for layer in layers(icon) {
                    let d = layer
                        .shapes
                        .iter()
                        .fold(f32::INFINITY, |d, shape| d.min(distance(shape, p)));
                    let coverage = (0.5 - d * half).clamp(0., 1.);
                    if coverage > 0. {
                        self.pixel(px, py, alpha(color(layer.tone), coverage));
                    }
                }
            }
        }
    }

    fn paint_silhouette(&mut self, icon: Icon, cx: f32, cy: f32, size: f32, color: [u8; 4]) {
        let (cx, cy) = (cx + self.shift[0] as f32, cy + self.shift[1] as f32);
        let half = size * 0.5;
        let reach = half * 1.08 + 1.;
        for py in (cy - reach).floor() as i32..=(cy + reach).ceil() as i32 {
            for px in (cx - reach).floor() as i32..=(cx + reach).ceil() as i32 {
                let p = [(px as f32 + 0.5 - cx) / half, (py as f32 + 0.5 - cy) / half];
                let d = layers(icon)
                    .iter()
                    .flat_map(|layer| layer.shapes)
                    .fold(f32::INFINITY, |d, shape| d.min(distance(shape, p)));
                let coverage = (0.5 - d * half).clamp(0., 1.);
                if coverage > 0. {
                    self.pixel(px, py, alpha(color, coverage));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::WIDTH;

    const ALL: [Icon; 28] = [
        Icon::Star,
        Icon::Trophy,
        Icon::Crown,
        Icon::Flag,
        Icon::Bolt,
        Icon::Shield,
        Icon::Target,
        Icon::Flame,
        Icon::Clock,
        Icon::Gem,
        Icon::Play,
        Icon::Pause,
        Icon::Restart,
        Icon::ArrowUp,
        Icon::ArrowLeft,
        Icon::ArrowRight,
        Icon::ChevronLeft,
        Icon::ChevronRight,
        Icon::ChevronUp,
        Icon::Hand,
        Icon::Question,
        Icon::Panel,
        Icon::Satellite,
        Icon::Ball,
        Icon::BallLost,
        Icon::Wide,
        Icon::Multi,
        Icon::Sticky,
    ];

    fn blank() -> Canvas {
        let mut canvas = Canvas::new();
        canvas.frame.pixels_mut().fill([0, 0, 0, 255]);
        canvas
    }

    #[test]
    fn every_icon_paints_inside_its_box_and_fades_evenly() {
        for icon in ALL {
            let mut canvas = blank();
            canvas.icon(icon, 100., 100., 40., [255, 255, 255, 255]);
            let mut lit = 0;
            for (i, p) in canvas.frame.pixels().iter().enumerate() {
                if p[..3] != [0, 0, 0] {
                    lit += 1;
                    let (x, y) = ((i as u32 % WIDTH) as f32, (i as u32 / WIDTH) as f32);
                    assert!(
                        (78. ..=122.).contains(&x) && (78. ..=122.).contains(&y),
                        "{icon:?} leaves its box at ({x}, {y})"
                    );
                }
            }
            assert!(lit > 60, "{icon:?} drew only {lit} pixels");
            // A translucent union never exceeds its own alpha: shapes of one
            // layer do not double-blend where they overlap.
            let mut faded = blank();
            faded.paint_silhouette(icon, 100., 100., 40., [255, 255, 255, 128]);
            if layers(icon).len() == 1 {
                faded.icon(icon, 300., 100., 40., [255, 255, 255, 128]);
            }
            let brightest = faded.frame.pixels().iter().map(|p| p[0]).max().unwrap();
            assert!(brightest <= 130, "{icon:?} overlaps itself: {brightest}");
        }
    }

    #[test]
    fn invisible_or_degenerate_icons_draw_nothing() {
        let mut canvas = blank();
        canvas.icon(Icon::Star, f32::NAN, 10., 20., [255; 4]);
        canvas.icon(Icon::Star, 10., 10., 0., [255; 4]);
        canvas.icon(Icon::Star, 10., 10., 20., [255, 255, 255, 0]);
        canvas.icon(Icon::Star, -400., 9000., 20., [255; 4]);
        assert!(canvas.frame.pixels().iter().all(|p| p[..3] == [0, 0, 0]));
    }

    /// `ORBIT_ICON_SHEET=path.png cargo test -p owned-orbit icon_sheet` writes
    /// a specimen of every icon at HUD, card and callout sizes for review.
    #[test]
    fn icon_sheet() {
        let Ok(path) = std::env::var("ORBIT_ICON_SHEET") else {
            return;
        };
        let mut canvas = blank();
        canvas.frame.pixels_mut().fill([12, 24, 32, 255]);
        let colors = [super::super::CYAN, super::super::GOLD, PAPER];
        for (i, icon) in ALL.into_iter().enumerate() {
            let x = 30. + (i % 7) as f32 * 88.;
            let y = 40. + (i / 7) as f32 * 170.;
            let color = colors[i % 3];
            canvas.icon(icon, x, y, 48., color);
            canvas.icon_shadowed(icon, x, y + 52., 24., color);
            canvas.icon_shadowed(icon, x - 10., y + 84., 16., color);
            canvas.icon_shadowed(icon, x + 12., y + 84., 12., color);
            canvas.icon(icon, x, y + 112., 20., PAPER);
        }
        canvas.frame.save_png(path).expect("write icon sheet");
    }
}
