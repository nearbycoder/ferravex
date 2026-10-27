//! Test-only CPU rasterizer and PNG writer for the modern UI specimen sheet.
//!
//! `FERRAVEX_SPECIMEN=/tmp/specimen.png cargo test -p ferravex-render --lib specimen`
//! writes the sheet; without the variable the test only checks the geometry.
use crate::{Color, Rect, vec2};

/// Rasterizes the current frame's vertices: immediate triangles alpha-blend,
/// effect batches flagged additive add. Samples at pixel centers.
pub(crate) fn rasterize(w: usize, h: usize, clear: [f32; 3]) -> Vec<[f32; 3]> {
    let (vertices, additive) = crate::read(|s| {
        let ranges: Vec<(u32, u32)> = s
            .retained
            .effects
            .iter()
            .filter(|b| b.flags & 4 != 0)
            .map(|b| (b.start, b.start + b.count))
            .collect();
        (s.vertices.clone(), ranges)
    });
    let mut image = vec![clear; w * h];
    for (t, tri) in vertices.chunks_exact(3).enumerate() {
        let index = (t * 3) as u32;
        let add = additive.iter().any(|&(a, b)| index >= a && index < b);
        let p: Vec<(f32, f32)> = tri
            .iter()
            .map(|v| {
                (
                    (v[0] / v[3] + 1.) * w as f32 * 0.5,
                    (1. - v[1] / v[3]) * h as f32 * 0.5,
                )
            })
            .collect();
        let area = (p[1].0 - p[0].0) * (p[2].1 - p[0].1) - (p[2].0 - p[0].0) * (p[1].1 - p[0].1);
        if area.abs() < 1e-9 {
            continue;
        }
        let x0 = p
            .iter()
            .map(|q| q.0)
            .fold(f32::MAX, f32::min)
            .floor()
            .max(0.) as usize;
        let x1 = p
            .iter()
            .map(|q| q.0)
            .fold(f32::MIN, f32::max)
            .ceil()
            .min(w as f32) as usize;
        let y0 = p
            .iter()
            .map(|q| q.1)
            .fold(f32::MAX, f32::min)
            .floor()
            .max(0.) as usize;
        let y1 = p
            .iter()
            .map(|q| q.1)
            .fold(f32::MIN, f32::max)
            .ceil()
            .min(h as f32) as usize;
        for y in y0..y1 {
            for x in x0..x1 {
                // A tiny offset breaks ties on shared edges consistently.
                let (cx, cy) = (x as f32 + 0.5 + 1e-4, y as f32 + 0.5 + 2e-4);
                let e = |a: (f32, f32), b: (f32, f32)| {
                    (b.0 - a.0) * (cy - a.1) - (b.1 - a.1) * (cx - a.0)
                };
                let w0 = e(p[1], p[2]) / area;
                let w1 = e(p[2], p[0]) / area;
                let w2 = e(p[0], p[1]) / area;
                if w0 < 0. || w1 < 0. || w2 < 0. {
                    continue;
                }
                let c = |k: usize| w0 * tri[0][k] + w1 * tri[1][k] + w2 * tri[2][k];
                let a = c(7).clamp(0., 1.);
                let dst = &mut image[y * w + x];
                for (k, d) in dst.iter_mut().enumerate() {
                    let src = c(4 + k).clamp(0., 1.);
                    *d = if add {
                        (*d + src * a).min(1.)
                    } else {
                        *d * (1. - a) + src * a
                    };
                }
            }
        }
    }
    image
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0_u32;
    for &b in bytes {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// Minimal RGB PNG with stored (uncompressed) deflate blocks.
pub(crate) fn png(w: usize, h: usize, pixels: &[[f32; 3]], scale: usize) -> Vec<u8> {
    let (sw, sh) = (w * scale, h * scale);
    let mut raw = Vec::with_capacity((sw * 3 + 1) * sh);
    for y in 0..sh {
        raw.push(0);
        for x in 0..sw {
            let p = pixels[(y / scale) * w + x / scale];
            raw.extend(p.map(|v| (v.clamp(0., 1.) * 255. + 0.5) as u8));
        }
    }
    let mut z = vec![0x78, 0x01];
    let chunks: Vec<&[u8]> = raw.chunks(65535).collect();
    for (i, chunk) in chunks.iter().enumerate() {
        z.push((i + 1 == chunks.len()) as u8);
        let len = chunk.len() as u16;
        z.extend(len.to_le_bytes());
        z.extend((!len).to_le_bytes());
        z.extend_from_slice(chunk);
    }
    let (mut a, mut b) = (1_u32, 0_u32);
    for &byte in &raw {
        a = (a + byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    z.extend(((b << 16) | a).to_be_bytes());
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut chunk = |kind: &[u8], data: &[u8]| {
        out.extend((data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend_from_slice(data);
        out.extend_from_slice(&body);
        out.extend(crc32(&body).to_be_bytes());
    };
    let mut header = Vec::new();
    header.extend((sw as u32).to_be_bytes());
    header.extend((sh as u32).to_be_bytes());
    header.extend([8, 2, 0, 0, 0]);
    chunk(b"IHDR", &header);
    chunk(b"IDAT", &z);
    chunk(b"IEND", &[]);
    out
}

pub(crate) fn hex(rgb: u32) -> Color {
    Color::from_hex(rgb)
}

/// Draws the specimen sheet into the current frame.
pub(crate) fn sheet(w: f32) {
    use crate::shapes::{self, Cap, Fill, Join, Stroke};
    use crate::text::{FontFace, TextStyle, draw_text_styled, draw_text_wrapped};
    let ink = hex(0xf2ede3);
    let muted = hex(0x9aa7b8);
    let amber = hex(0xf7b553);
    let cyan = hex(0x5bd5d8);
    let x = 32.;
    let title = TextStyle::modern(32., ink).with_shadow(Color::new(0.02, 0.03, 0.08, 1.));
    draw_text_styled("Ferravex Modern", x, 56., title);
    draw_text_styled(
        "A hand-authored pixel face for readable, modern game UI",
        x,
        86.,
        TextStyle::modern(16., muted),
    );
    let mut y = 124.;
    let body = TextStyle::modern(16., ink);
    for line in [
        "ABCDEFGHIJKLMNOPQRSTUVWXYZ",
        "abcdefghijklmnopqrstuvwxyz",
        "0123456789 !?.,:;'\"()[]{}<>/\\|@#$%&*+-=_~^`",
        "• · × ÷ ± → ← ↑ ↓ ★ ☆ ♥ ♦ ♠ ♣ ° … – — ‘’ “” © ✓ € £ ¡ ¿ « »",
        "▲ ▼ ◀ ▶ ■ □ ● ○ ∞ ≤ ≥ ß æ ø Ø",
        "ÀÁÂÃÄÅ Ç ÈÉÊË ÌÍÎÏ Ñ ÒÓÔÕÖ ÙÚÛÜ Ý  àáâãäå ç èéêë ìíîï ñ òóôõö ùúûü ýÿ",
    ] {
        draw_text_styled(line, x, y, body);
        y += 30.;
    }
    y += 6.;
    for (size, label) in [
        (8., "1×"),
        (16., "2×"),
        (24., "3×"),
        (12., "1.5×"),
        (20., "2.5×"),
    ] {
        let s = TextStyle::modern(size, ink);
        draw_text_styled(label, x, y, TextStyle::modern(16., muted));
        let copy = if size > 20. {
            "The quick brown fox jumps over the lazy dog."
        } else {
            "The quick brown fox jumps over the lazy dog. Score 12,480"
        };
        draw_text_styled(copy, x + 64., y, s);
        y += s.line_height().max(28.) + 4.;
    }
    y += 4.;
    let classic = TextStyle::classic(16., ink);
    draw_text_styled("Classic 5×7", x, y, TextStyle::modern(16., muted));
    draw_text_styled("Press Start · Level 3 → Boss", x + 160., y, classic);
    y += 30.;
    draw_text_styled("Modern", x, y, TextStyle::modern(16., muted));
    draw_text_styled("Press Start · Level 3 → Boss", x + 160., y, body);
    y += 34.;
    let outlined = TextStyle::modern(24., hex(0xffe9a8))
        .with_outline(hex(0x1a0f08))
        .with_shadow(hex(0x1a0f08));
    draw_text_styled("+250 Combo!", x, y, outlined);
    let spaced = TextStyle::modern(16., cyan).with_letter_spacing(2.);
    draw_text_styled("PAUSED", x + 260., y, spaced);
    let classic_fx = TextStyle::classic(24., amber).with_shadow(Color::new(0.3, 0.05, 0.0, 1.));
    draw_text_styled("GAME OVER", x + 420., y, classic_fx);
    y += 40.;
    draw_text_wrapped(
        "Wrapped body copy keeps words together, honours explicit\nline breaks and splits only words wider than the column.",
        x,
        y,
        420.,
        TextStyle::modern(16., muted),
    );

    // Kit column.
    use crate::ui::kit::{self, ButtonKind, ButtonMotion, ButtonState, KitTheme, Tone};
    use ferravex_play::tween::Appearance;
    let theme = KitTheme::default();
    let sx = w - 380.;
    let inner = kit::card(Rect::new(sx, 24., 340., 196.), Some("Paused"), &theme);
    let states = [
        ButtonState::default(),
        ButtonState {
            hovered: true,
            focused: true,
            ..ButtonState::default()
        },
        ButtonState {
            hovered: true,
            pressed: true,
            ..ButtonState::default()
        },
    ];
    for (i, (label, kind)) in [
        ("Resume", ButtonKind::Primary),
        ("Options", ButtonKind::Secondary),
        ("Quit", ButtonKind::Danger),
    ]
    .iter()
    .enumerate()
    {
        let mut motion = ButtonMotion::default();
        motion.snap(states[i]);
        kit::button(
            Rect::new(inner.x + i as f32 * 106., inner.y + 4., 96., 36.),
            label,
            *kind,
            &motion,
            &theme,
        );
    }
    draw_text_styled(
        "Health",
        inner.x,
        inner.y + 70.,
        TextStyle::modern(16., muted),
    );
    kit::meter(
        Rect::new(inner.x + 72., inner.y + 59., inner.w - 72., 12.),
        0.58,
        Some(0.8),
        hex(0xff6b6b),
        &theme,
    );
    draw_text_styled(
        "Energy",
        inner.x,
        inner.y + 96.,
        TextStyle::modern(16., muted),
    );
    kit::meter(
        Rect::new(inner.x + 72., inner.y + 85., inner.w - 72., 12.),
        0.82,
        None,
        hex(0x5bd5d8),
        &theme,
    );
    let px = kit::key_prompt(inner.x, inner.y + 110., &["E"], "Open", &theme);
    kit::key_prompt(
        inner.x + px + 20.,
        inner.y + 110.,
        &["Shift", "Space"],
        "Dash",
        &theme,
    );
    kit::toast(
        sx + 170.,
        236.,
        Some('★'),
        "Achievement unlocked",
        Tone::Success,
        Appearance::SHOWN,
        &theme,
    );
    let cy = 318.;
    shapes::fill_circle(
        vec2(sx + 40., cy),
        22.,
        Fill::radial(vec2(sx + 34., cy - 6.), 26., hex(0xfff1c2), amber),
    );
    shapes::stroke_circle(vec2(sx + 110., cy), 20., 4., cyan);
    shapes::stroke_circle(vec2(sx + 180., cy), 20., 5., Color::new(1., 1., 1., 0.12));
    shapes::stroke_arc(
        vec2(sx + 180., cy),
        20.,
        -std::f32::consts::FRAC_PI_2,
        4.2,
        Stroke::new(5., amber).cap(Cap::Round),
    );
    shapes::fill_capsule(
        vec2(sx + 230., cy - 12.),
        vec2(sx + 330., cy - 12.),
        8.,
        hex(0x7c5cff),
    );
    shapes::fill_capsule(
        vec2(sx + 230., cy + 14.),
        vec2(sx + 300., cy + 4.),
        6.,
        cyan,
    );
    let path = [
        vec2(sx + 10., 400.),
        vec2(sx + 70., 360.),
        vec2(sx + 130., 392.),
        vec2(sx + 190., 356.),
        vec2(sx + 250., 388.),
        vec2(sx + 330., 360.),
    ];
    shapes::polyline(
        &path,
        Stroke::new(6., hex(0xff6b8b))
            .join(Join::Round)
            .cap(Cap::Round),
    );
    let path2 = path.map(|p| p + vec2(0., 34.));
    shapes::polyline(&path2, Stroke::new(3., hex(0x8ce99a)).join(Join::Miter));
    let path3 = path.map(|p| p + vec2(0., 62.));
    shapes::polyline(&path3, Stroke::new(1., ink).join(Join::Bevel));
    // Glows on a dark well.
    let well = Rect::new(sx, 480., 340., 120.);
    shapes::fill_rounded_rect(well, 12., hex(0x07090f));
    shapes::glow_disc(vec2(sx + 70., 540.), 56., Color::new(1., 0.55, 0.2, 0.9));
    shapes::fill_circle(vec2(sx + 70., 540.), 6., hex(0xfff3d6));
    shapes::glow_ring(
        vec2(sx + 190., 540.),
        36.,
        16.,
        Color::new(0.35, 0.8, 1., 0.9),
    );
    shapes::glow_disc(vec2(sx + 290., 540.), 30., Color::new(0.6, 0.4, 1., 0.8));
    shapes::glow_disc(vec2(sx + 290., 540.), 10., Color::new(1., 1., 1., 0.8));

    let _ = FontFace::Modern;
}

#[test]
fn specimen_sheet_renders_finite_geometry_and_optionally_writes_png() {
    let (w, h) = (1200, 760);
    crate::input::host_frame(1. / 60., w as u32, h as u32, || sheet(w as f32));
    let image = rasterize(w, h, [0.075, 0.09, 0.13]);
    assert!(image.iter().flatten().all(|v| v.is_finite()));
    if let Ok(path) = std::env::var("FERRAVEX_SPECIMEN") {
        let scale = std::env::var("FERRAVEX_SPECIMEN_SCALE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1);
        std::fs::write(path, png(w, h, &image, scale)).unwrap();
    }
}
