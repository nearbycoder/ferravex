//! Specimen sheet for the procedural icon set, on dark and light surfaces and
//! at three sizes. Run `cargo run -p ferravex-render --example icon_sheet
//! --offline --locked`; `-- --screenshot sheet.png` saves frame 10 and exits.
//! Review modes: `-- --large` draws every icon at 72 px (`--size 16` picks
//! another size) and `-- --zoom heart,star,gear` draws the named icons large
//! on dark and light; add `--alpha 0.4` to either to check translucency.
//! `-- --poster` draws the website's 16:10 icon poster.
use ferravex_render::{
    self as render, Color, Rect,
    icons::{self, Icon, IconStyle},
    shapes,
    text::{TextAlign, TextStyle, draw_text_styled},
    vec2,
};

const COLUMNS: usize = 12;
const CELL: f32 = 104.;

/// Fits a 1260 x 940 authoring canvas into the window.
fn canvas() {
    let (w, h) = (render::screen_width(), render::screen_height());
    render::clear_background(Color::new(0.05, 0.065, 0.09, 1.));
    let scale = (w / 1260.).min(h / 940.);
    render::set_camera(&render::Camera2D::from_display_rect(Rect::new(
        0.,
        0.,
        w / scale,
        h / scale,
    )));
}

fn sheet() {
    canvas();
    let light = Rect::new(COLUMNS as f32 * CELL * 0.5 + 6., 0., 1260., 940.);
    shapes::fill_rounded_rect(light, 0., Color::new(0.93, 0.91, 0.86, 1.));
    let label =
        TextStyle::modern(10., Color::new(0.6, 0.67, 0.74, 1.)).with_align(TextAlign::Center);
    for (i, &icon) in Icon::ALL.iter().enumerate() {
        let (col, row) = (i % COLUMNS, i / COLUMNS);
        let x = 12. + col as f32 * CELL + CELL * 0.5;
        let y = 16. + row as f32 * 82. + 30.;
        let on_light = col >= COLUMNS / 2;
        let (main, small) = if on_light {
            (
                Color::new(0.16, 0.2, 0.27, 1.),
                Color::new(0.85, 0.35, 0.2, 1.),
            )
        } else {
            (
                Color::new(0.94, 0.95, 0.96, 1.),
                Color::new(0.97, 0.71, 0.33, 1.),
            )
        };
        icons::draw_styled(
            icon,
            vec2(x - 18., y),
            40.,
            IconStyle::new(main).shadow(Color::new(0., 0., 0., 0.3)),
        );
        icons::draw(icon, vec2(x + 20., y - 8.), 20., small);
        icons::draw(icon, vec2(x + 20., y + 12.), 12., main);
        draw_text_styled(
            icon.name(),
            x,
            y + 36.,
            if on_light {
                label.with_color(Color::new(0.35, 0.38, 0.42, 1.))
            } else {
                label
            },
        );
    }
    // Badges along the bottom row.
    for (i, &icon) in Icon::ALL.iter().take(20).enumerate() {
        icons::badge(
            icon,
            vec2(40. + i as f32 * 60., 900.),
            44.,
            Color::new(0.2, 0.55, 0.45, 1.),
            Color::new(0.97, 0.98, 0.96, 1.),
        );
    }
}

/// Every icon at `size` px, white with a shadow, for detail review. Below 1,
/// `alpha` drops the shadow and shows where translucent pieces overlap.
fn large(size: f32, alpha: f32) {
    canvas();
    let label =
        TextStyle::modern(10., Color::new(0.6, 0.67, 0.74, 1.)).with_align(TextAlign::Center);
    for (i, &icon) in Icon::ALL.iter().enumerate() {
        let (col, row) = (i % COLUMNS, i / COLUMNS);
        let (x, y) = (52. + col as f32 * 104., 50. + row as f32 * 108.);
        let style = IconStyle::new(Color::new(0.94, 0.95, 0.96, alpha));
        let style = if alpha < 1. {
            style
        } else {
            style.shadow(Color::new(0., 0., 0., 0.35))
        };
        icons::draw_styled(icon, vec2(x, y), size, style);
        draw_text_styled(icon.name(), x, y + 46., label);
    }
}

/// The named icons at up to 200 px: white on dark above, two-tone on light below.
fn zoom(names: &[Icon], alpha: f32) {
    canvas();
    shapes::fill_rounded_rect(
        Rect::new(0., 470., 1260., 470.),
        0.,
        Color::new(0.93, 0.91, 0.86, 1.),
    );
    let n = names.len().max(1) as f32;
    let size = (1180. / n - 30.).min(200.);
    for (i, &icon) in names.iter().enumerate() {
        let x = 40. + size * 0.5 + i as f32 * (size + 30.);
        icons::draw(
            icon,
            vec2(x, 235.),
            size,
            Color::new(0.94, 0.95, 0.96, alpha),
        );
        icons::draw(
            icon,
            vec2(x, 705.),
            size,
            Color::new(0.85, 0.35, 0.2, alpha),
        );
    }
}

/// A 16:10 marketing poster: every icon on a soft tile, colored in diagonal
/// bands. Authored at 1280 x 800 (`-- --poster`; the website uses a 1600 x
/// 1000 capture of it).
fn poster() {
    let (w, h) = (render::screen_width(), render::screen_height());
    let scale = (w / 1280.).min(h / 800.);
    render::set_camera(&render::Camera2D::from_display_rect(Rect::new(
        0.,
        0.,
        w / scale,
        h / scale,
    )));
    let full = Rect::new(0., 0., 1280., 800.);
    shapes::fill_rounded_rect(
        full,
        0.,
        shapes::Fill::vertical(
            full,
            Color::new(0.075, 0.095, 0.14, 1.),
            Color::new(0.03, 0.04, 0.065, 1.),
        ),
    );
    const PALETTE: [u32; 6] = [0xf7b553, 0xe5484d, 0xb78cf2, 0x5bd5d8, 0x8fd16a, 0x6ab0f3];
    let (columns, cell) = (12, 96.);
    let rows = Icon::ALL.len().div_ceil(columns);
    let left = 640. - columns as f32 * cell * 0.5;
    let top = 400. - rows as f32 * cell * 0.5;
    for (i, &icon) in Icon::ALL.iter().enumerate() {
        let (col, row) = (i % columns, i / columns);
        let center = vec2(
            left + (col as f32 + 0.5) * cell,
            top + (row as f32 + 0.5) * cell,
        );
        let tile = Rect::new(center.x - 38., center.y - 38., 76., 76.);
        shapes::drop_shadow(
            tile,
            18.,
            shapes::Shadow::soft(Color::new(0., 0., 0., 0.3))
                .offset(vec2(0., 3.))
                .blur(8.),
        );
        shapes::fill_rounded_rect(
            tile,
            18.,
            shapes::Fill::vertical(
                tile,
                Color::new(0.13, 0.16, 0.215, 1.),
                Color::new(0.095, 0.12, 0.165, 1.),
            ),
        );
        shapes::stroke_rounded_rect(tile, 18., 1., Color::new(1., 1., 1., 0.07));
        let color = Color::from_hex(PALETTE[((col + row) / 2) % PALETTE.len()]);
        icons::draw_styled(
            icon,
            center,
            42.,
            IconStyle::new(color).shadow(Color::new(0., 0., 0., 0.4)),
        );
    }
}

async fn game() {
    let args: Vec<String> = std::env::args().collect();
    let value = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|at| args.get(at + 1).cloned())
    };
    let shot = value("--screenshot");
    let zoomed: Vec<Icon> = value("--zoom")
        .map(|list| list.split(',').filter_map(Icon::from_name).collect())
        .unwrap_or_default();
    let large_mode = args.iter().any(|a| a == "--large");
    let poster_mode = args.iter().any(|a| a == "--poster");
    let size = value("--size")
        .and_then(|a| a.parse::<f32>().ok())
        .map_or(72., |a| a.clamp(4., 96.));
    let alpha = value("--alpha")
        .and_then(|a| a.parse::<f32>().ok())
        .map_or(1., |a| a.clamp(0., 1.));
    for frame in 0.. {
        if poster_mode {
            poster();
        } else if !zoomed.is_empty() {
            zoom(&zoomed, alpha);
        } else if large_mode {
            large(size, alpha);
        } else {
            sheet();
        }
        if let Some(path) = &shot
            && frame == 10
        {
            render::get_screen_data().export_png(path);
            render::next_frame().await;
            return;
        }
        render::next_frame().await;
    }
}

fn main() {
    let mut conf = render::window_conf("Ferravex icons");
    let poster = std::env::args().any(|a| a == "--poster");
    conf.window_width = if poster { 1280 } else { 1260 };
    conf.window_height = if poster { 800 } else { 940 };
    render::run(conf, game());
}
