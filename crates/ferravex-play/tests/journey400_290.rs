//! Public task290 behavior and scoped native allocation evidence.
use ferravex_math::Color;
use ferravex_play::ui::theme::*;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    hint::black_box,
};

const BLACK: Color = Color::from_hex(0);
const WHITE: Color = Color::from_hex(0xffffff);
fn base() -> UiTheme {
    UiTheme::preset(ThemeMode::Normal)
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-6, "{a} vs {b}");
}

#[test]
fn presets_validate_every_required_role_pair_and_preserve_metrics() {
    let normal = base();
    for mode in [ThemeMode::Normal, ThemeMode::HighContrast] {
        let theme = UiTheme::preset(mode);
        assert_eq!(
            UiTheme::try_new(mode, theme.types(), theme.spacing(), theme.palette()),
            Ok(theme)
        );
        assert_eq!(theme.types(), normal.types());
        assert_eq!(theme.spacing(), normal.spacing());
        for role in ColorRole::ALL.into_iter().skip(2) {
            let mark = matches!(role, ColorRole::Focus | ColorRole::Border);
            let required = match (mode, mark) {
                (ThemeMode::Normal, true) => 3.,
                (ThemeMode::Normal, false) | (ThemeMode::HighContrast, true) => 4.5,
                (ThemeMode::HighContrast, false) => 7.,
            };
            for bg in [ColorRole::Canvas, ColorRole::Surface] {
                assert!(
                    theme.role_contrast(role, bg).unwrap() >= required,
                    "{mode:?} {role:?} {bg:?}"
                );
            }
        }
        assert_eq!(theme.text(TextRole::Label).size, 16.);
        assert_eq!(theme.text(TextRole::Body).size, 18.);
        assert_eq!(theme.text(TextRole::Heading).size, 24.);
        assert_eq!(
            theme.text(TextRole::Label).color,
            theme.color(ColorRole::MutedText)
        );
    }
}

#[test]
fn srgb_linear_units_and_known_luminances_are_distinct() {
    use RgbEncoding::{LinearSrgb, Srgb};
    assert_eq!(relative_luminance(BLACK, Srgb), Ok(0.));
    assert_eq!(relative_luminance(WHITE, Srgb), Ok(1.));
    assert_eq!(contrast_ratio(BLACK, WHITE, Srgb), Ok(21.));
    assert_eq!(contrast_ratio(WHITE, BLACK, LinearSrgb), Ok(21.));
    assert_eq!(contrast_ratio(WHITE, WHITE, Srgb), Ok(1.));
    for (color, lum) in [
        (Color::new(1., 0., 0., 1.), 0.2126),
        (Color::new(0., 1., 0., 1.), 0.7152),
        (Color::new(0., 0., 1., 1.), 0.0722),
    ] {
        close(relative_luminance(color, Srgb).unwrap(), lum);
    }
    let gray = Color::from_hex(0x808080);
    let linear_gray = gray.try_srgb_to_linear().unwrap();
    close(relative_luminance(gray, Srgb).unwrap(), 0.2158605);
    assert_eq!(
        relative_luminance(gray, Srgb),
        relative_luminance(linear_gray, LinearSrgb)
    );
    assert_eq!(
        contrast_ratio(gray, WHITE, Srgb),
        contrast_ratio(linear_gray, WHITE, LinearSrgb)
    );
    close(
        contrast_ratio(Color::new(0.5, 0.5, 0.5, 1.), BLACK, LinearSrgb).unwrap(),
        11.,
    );
    close(
        contrast_ratio(Color::new(0.5, 0.5, 0.5, 1.), BLACK, Srgb).unwrap(),
        5.2808228,
    );
}

#[test]
fn transparency_requires_explicit_opaque_backdrop_and_composites_in_linear_light() {
    for encoding in [RgbEncoding::Srgb, RgbEncoding::LinearSrgb] {
        let transparent = Color::new(1., 0., 1., 0.);
        let half_white = Color::new(1., 1., 1., 0.5);
        assert_eq!(
            relative_luminance(transparent, encoding),
            Err(ThemeError::AlphaRequiresBackdrop)
        );
        assert_eq!(
            contrast_ratio(WHITE, half_white, encoding),
            Err(ThemeError::AlphaRequiresBackdrop)
        );
        assert_eq!(
            contrast_ratio(half_white, WHITE, encoding),
            Err(ThemeError::AlphaRequiresBackdrop)
        );
        close(
            contrast_on(half_white, BLACK, BLACK, encoding).unwrap(),
            11.,
        );
        close(
            contrast_on(BLACK, half_white, BLACK, encoding).unwrap(),
            11.,
        );
        assert_eq!(
            contrast_on(transparent, half_white, BLACK, encoding),
            Ok(1.)
        );
        assert_eq!(contrast_on(BLACK, transparent, WHITE, encoding), Ok(21.));
        assert_eq!(
            contrast_on(WHITE, BLACK, transparent, encoding),
            Err(ThemeError::AlphaRequiresBackdrop)
        );
        assert_eq!(
            contrast_on(WHITE, BLACK, WHITE, encoding),
            contrast_ratio(WHITE, BLACK, encoding)
        );
    }
    let fg = Color::new(0.2, 0.8, 0.6, 0.3);
    let bg = Color::new(0.8, 0.1, 0.4, 0.7);
    let back = Color::from_hex(0x204080);
    assert_eq!(
        contrast_on(fg, bg, back, RgbEncoding::Srgb),
        contrast_on(
            fg.try_srgb_to_linear().unwrap(),
            bg.try_srgb_to_linear().unwrap(),
            back.try_srgb_to_linear().unwrap(),
            RgbEncoding::LinearSrgb
        )
    );
}

#[test]
fn every_channel_rejects_nonfinite_and_out_of_gamut_without_repair() {
    for channel in 0..4 {
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.1, 1.1] {
            let mut rgba = [1.; 4];
            rgba[channel] = value;
            let color = Color::new(rgba[0], rgba[1], rgba[2], rgba[3]);
            let error = if value.is_finite() {
                ThemeError::ColorOutOfRange
            } else {
                ThemeError::NonFiniteColor
            };
            for encoding in [RgbEncoding::Srgb, RgbEncoding::LinearSrgb] {
                assert_eq!(relative_luminance(color, encoding), Err(error));
                assert_eq!(contrast_ratio(color, BLACK, encoding), Err(error));
                assert_eq!(contrast_ratio(BLACK, color, encoding), Err(error));
                assert_eq!(contrast_on(color, BLACK, BLACK, encoding), Err(error));
                assert_eq!(contrast_on(BLACK, color, BLACK, encoding), Err(error));
                assert_eq!(contrast_on(BLACK, BLACK, color, encoding), Err(error));
            }
            let mut theme = base();
            assert_eq!(theme.set_colors(&[(ColorRole::Accent, color)]), Err(error));
            assert_eq!(theme, base());
            assert_eq!(Palette::try_new_srgb([color; COLOR_ROLE_COUNT]), Err(error));
        }
    }
    let translucent = Color::new(1., 1., 1., 0.99);
    let mut theme = base();
    assert_eq!(
        theme.set_colors(&[(ColorRole::Surface, translucent)]),
        Err(ThemeError::AlphaRequiresBackdrop)
    );
    assert_eq!(theme, base());
    assert_eq!(
        Palette::try_new_srgb([translucent; COLOR_ROLE_COUNT]),
        Err(ThemeError::AlphaRequiresBackdrop)
    );
}

#[test]
fn contrast_errors_report_first_pair_and_roll_back_earlier_valid_edits() {
    let mut theme = base();
    let error = theme.set_colors(&[
        (ColorRole::Accent, WHITE),
        (ColorRole::Text, theme.color(ColorRole::Canvas)),
    ]);
    match error {
        Err(ThemeError::Contrast {
            foreground,
            background,
            measured,
            required,
        }) => {
            assert_eq!(
                (foreground, background),
                (ColorRole::Text, ColorRole::Canvas)
            );
            assert_eq!((measured, required), (1., 4.5));
        }
        other => panic!("unexpected {other:?}"),
    }
    assert_eq!(theme, base());
    // Only one of two backgrounds fails: neither check may be skipped.
    let error = theme.set_colors(&[(ColorRole::Surface, WHITE)]);
    assert!(matches!(
        error,
        Err(ThemeError::Contrast {
            foreground: ColorRole::Text,
            background: ColorRole::Surface,
            ..
        })
    ));
    assert_eq!(theme, base());
    let colors = ColorRole::ALL.map(|role| {
        if matches!(role, ColorRole::Canvas | ColorRole::Surface) {
            BLACK
        } else {
            Color::from_hex(0x777777)
        }
    });
    let palette = Palette::try_new_srgb(colors).unwrap();
    assert!(UiTheme::try_new(ThemeMode::Normal, theme.types(), theme.spacing(), palette).is_ok());
    assert!(matches!(
        UiTheme::try_new(
            ThemeMode::HighContrast,
            theme.types(),
            theme.spacing(),
            palette
        ),
        Err(ThemeError::Contrast { required: 7., .. })
    ));
}

#[test]
fn related_palette_overrides_are_atomic_and_capacity_is_exact() {
    let mut theme = base();
    let original = theme;
    assert!(theme.set_colors(&[(ColorRole::Canvas, WHITE)]).is_err());
    assert_eq!(theme, original);
    let edits = ColorRole::ALL.map(|role| {
        (
            role,
            if matches!(role, ColorRole::Canvas | ColorRole::Surface) {
                WHITE
            } else {
                BLACK
            },
        )
    });
    theme.set_colors(&edits).unwrap(); // Exact fixed capacity; a complete light palette.
    for (role, color) in edits {
        assert_eq!(theme.color(role), color);
    }
    assert_eq!(theme.mode(), original.mode());
    assert_eq!(theme.types(), original.types());
    assert_eq!(theme.spacing(), original.spacing());
    let light = theme;
    assert_eq!(
        theme.set_colors(&[(ColorRole::Text, BLACK); 2]),
        Err(ThemeError::DuplicateRole)
    );
    assert_eq!(theme, light);
    assert_eq!(
        theme.set_colors(
            &[(ColorRole::Text, Color::new(f32::NAN, 0., 0., 1.)); COLOR_ROLE_COUNT + 1]
        ),
        Err(ThemeError::Capacity)
    );
    assert_eq!(theme, light);
    theme.set_colors(&[]).unwrap();
    assert_eq!(theme, light);
    theme = UiTheme::preset(ThemeMode::HighContrast);
    assert_eq!(theme.color(ColorRole::Canvas), BLACK);
    assert_eq!(theme.color(ColorRole::Text), WHITE);
    assert_eq!(original, base()); // No singleton/shared mutation.
}

#[test]
fn invalid_type_metrics_and_scaling_never_change_spacing_or_shrink_text() {
    for field in 0..3 {
        for value in [-1., 0., 15.99, 257., f32::MAX, f32::NAN, f32::INFINITY] {
            let mut theme = base();
            let mut sizes = [16., 18., 24.];
            sizes[field] = value;
            let types = TypeScale {
                label: sizes[0],
                body: sizes[1],
                heading: sizes[2],
            };
            assert_eq!(
                theme.set_metrics(
                    types,
                    Spacing {
                        gap: 42.,
                        ..theme.spacing()
                    }
                ),
                Err(ThemeError::InvalidTypeScale)
            );
            assert_eq!(theme, base());
        }
    }
    for types in [
        TypeScale {
            label: 20.,
            body: 18.,
            heading: 24.,
        },
        TypeScale {
            label: 16.,
            body: 24.,
            heading: 18.,
        },
    ] {
        assert_eq!(types.try_scaled(1.), Err(ThemeError::InvalidTypeScale));
    }
    for factor in [0., -1., 0.99, 100., f32::MAX, f32::NAN, f32::INFINITY] {
        assert_eq!(
            base().types().try_scaled(factor),
            Err(ThemeError::InvalidTypeScale)
        );
    }
    let types = base().types();
    assert_eq!(types.try_scaled(1.), Ok(types));
    assert_eq!(
        types.try_scaled(2.).unwrap(),
        TypeScale {
            label: 32.,
            body: 36.,
            heading: 48.
        }
    );
    let mut theme = base();
    assert_eq!(
        theme.set_metrics(types.try_scaled(2.).unwrap(), theme.spacing()),
        Err(ThemeError::InvalidSpacing)
    );
    assert_eq!(theme, base());
}

#[test]
fn spacing_boundaries_and_joint_metric_adoption_preserve_other_choices() {
    for field in 0..5 {
        for value in [-1., f32::MAX, f32::NAN, f32::INFINITY] {
            let mut theme = base();
            let mut metrics = [16., 12., 8., 44., 2.];
            metrics[field] = value;
            let spacing = Spacing {
                inset: metrics[0],
                gap: metrics[1],
                compact_gap: metrics[2],
                control_height: metrics[3],
                stroke: metrics[4],
            };
            assert_eq!(
                theme.set_metrics(theme.types(), spacing),
                Err(ThemeError::InvalidSpacing)
            );
            assert_eq!(theme, base());
        }
    }
    for spacing in [
        Spacing {
            control_height: 43.99,
            ..base().spacing()
        },
        Spacing {
            stroke: 0.,
            ..base().spacing()
        },
        Spacing {
            stroke: 64.01,
            ..base().spacing()
        },
        Spacing {
            compact_gap: 14.,
            ..base().spacing()
        },
    ] {
        let mut theme = base();
        assert_eq!(
            theme.set_metrics(theme.types(), spacing),
            Err(ThemeError::InvalidSpacing)
        );
        assert_eq!(theme, base());
    }
    let mut theme = base();
    for size in [16., 256.] {
        let types = TypeScale {
            label: size,
            body: size,
            heading: size,
        };
        let spacing = Spacing {
            inset: 0.,
            gap: 4096.,
            compact_gap: 0.,
            control_height: size.max(44.),
            stroke: 64.,
        };
        theme.set_metrics(types, spacing).unwrap();
        assert_eq!(theme.text(TextRole::Body).size, size);
        assert_eq!(theme.spacing(), spacing);
        assert_eq!(theme.palette(), base().palette());
    }
    // Invalid complete authoring also fails before producing a validated theme.
    assert_eq!(
        UiTheme::try_new(
            theme.mode(),
            theme.types(),
            Spacing {
                inset: -1.,
                ..theme.spacing()
            },
            theme.palette()
        ),
        Err(ThemeError::InvalidSpacing)
    );
}

#[test]
fn every_semantic_state_has_static_noncolor_guidance_in_both_modes() {
    let states = [
        ThemeState::Selected,
        ThemeState::Focused,
        ThemeState::Disabled,
        ThemeState::Success,
        ThemeState::Warning,
        ThemeState::Error,
    ];
    for (i, state) in states.into_iter().enumerate() {
        let normal = base().state(state);
        let high = UiTheme::preset(ThemeMode::HighContrast).state(state);
        assert_eq!(normal.cue, high.cue);
        assert!(!normal.cue.label.is_empty());
        assert_ne!(normal.cue.shape, CueShape::None);
        for previous in &states[..i] {
            assert_ne!(normal.cue.label, base().state(*previous).cue.label);
        }
    }
    assert_eq!(
        base().state(ThemeState::Warning).cue.shape,
        CueShape::Triangle
    );
    assert_eq!(
        base().state(ThemeState::Disabled).cue.pattern,
        CuePattern::Hatch
    );
    assert_eq!(base().state(ThemeState::Neutral).cue.shape, CueShape::None);
}

struct CountingAllocator;
thread_local! { static ALLOCS: Cell<Option<usize>> = const { Cell::new(None) }; }
fn count_alloc() {
    let _ = ALLOCS.try_with(|cell| {
        if let Some(n) = cell.get() {
            cell.set(Some(n + 1));
        }
    });
}
// SAFETY: All allocation/deallocation contracts are forwarded unchanged to System.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count_alloc();
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count_alloc();
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        count_alloc();
        unsafe { System.realloc(ptr, layout, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

#[test]
fn construction_accepted_rejected_edits_measurements_and_reset_allocate_nothing() {
    ALLOCS.with(|cell| cell.set(Some(0)));
    for _ in 0..1000 {
        let mut theme = UiTheme::preset(black_box(ThemeMode::Normal));
        let palette = Palette::try_new_srgb(ColorRole::ALL.map(|r| theme.color(r))).unwrap();
        theme = UiTheme::try_new(theme.mode(), theme.types(), theme.spacing(), palette).unwrap();
        theme
            .set_colors(black_box(&[(ColorRole::Accent, WHITE)]))
            .unwrap();
        assert!(
            theme
                .set_colors(black_box(&[(ColorRole::Text, BLACK)]))
                .is_err()
        );
        assert_eq!(
            theme.set_colors(black_box(
                &[(ColorRole::Accent, WHITE); COLOR_ROLE_COUNT + 1]
            )),
            Err(ThemeError::Capacity)
        );
        theme
            .set_metrics(
                theme.types().try_scaled(1.5).unwrap(),
                Spacing {
                    control_height: 56.,
                    ..theme.spacing()
                },
            )
            .unwrap();
        black_box(theme.text(TextRole::Heading));
        black_box(theme.state(ThemeState::Disabled));
        black_box(
            contrast_on(
                WHITE,
                Color::new(0.2, 0.4, 0.6, 0.5),
                BLACK,
                RgbEncoding::Srgb,
            )
            .unwrap(),
        );
        black_box(relative_luminance(WHITE, RgbEncoding::LinearSrgb).unwrap());
        theme = UiTheme::preset(black_box(ThemeMode::HighContrast));
        black_box(theme);
    }
    let count = ALLOCS.with(|cell| cell.replace(None).unwrap());
    assert_eq!(count, 0);
}
