use ferravex_render::{
    Color, Vec2,
    input::{host_frame, touch, touch_event},
    ui::{
        ButtonStyle, PANEL, TextStyle, Ui, UiError, UiLabel, UiRect, Viewport,
        stats::{MAX_STAT_ROWS, StatRow, StatRowsLayout, StatRowsMode, StatRowsStyle},
        text_layout::{LineLimit, WrappedText},
    },
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    fmt::Write,
};

const BOUNDS: UiRect = UiRect {
    min: [16.; 2],
    size: [288., 900.],
};

fn row<'a>(label: &'a str, value: &'a str) -> StatRow<'a> {
    StatRow {
        label,
        value,
        description: None,
    }
}
fn viewport() -> Viewport {
    Viewport::responsive([1200., 1200.], [0.; 4]).unwrap()
}
fn inspected(ui: &Ui) -> Vec<(String, UiRect)> {
    ui.inspect()
        .unwrap()
        .map(|e| {
            assert_eq!(e.label, UiLabel::Full(e.painted_label));
            (e.painted_label.to_owned(), e.bounds)
        })
        .collect()
}
fn assert_geometry(
    layout: &StatRowsLayout<'_>,
    rows: &[StatRow<'_>],
    style: StatRowsStyle,
    bounds: UiRect,
) {
    for (i, (geometry, source)) in layout.rows().iter().zip(rows).enumerate() {
        if i > 0 {
            let previous = layout.rows()[i - 1].row;
            assert!(previous.min[1] + previous.size[1] <= geometry.row.min[1]);
        }
        for (copy, rect, style) in [
            Some((source.label, geometry.label, style.label)),
            Some((source.value, geometry.value, style.value)),
            source
                .description
                .zip(geometry.description)
                .map(|(copy, rect)| (copy, rect, style.description)),
        ]
        .into_iter()
        .flatten()
        {
            let metrics = WrappedText::new(copy, rect.size[0], style.size)
                .unwrap()
                .measure();
            assert_eq!(rect.size[1], metrics.height.max(style.size));
            assert!(metrics.width <= rect.size[0], "{copy:?} {rect:?}");
            for axis in 0..2 {
                assert!(rect.min[axis] >= geometry.row.min[axis]);
                assert!(
                    rect.min[axis] + rect.size[axis]
                        <= geometry.row.min[axis] + geometry.row.size[axis]
                );
                assert!(rect.min[axis] >= bounds.min[axis]);
                assert!(rect.min[axis] + rect.size[axis] <= bounds.min[axis] + bounds.size[axis]);
            }
        }
    }
}

#[test]
fn actual_ui_preserves_negative_rollover_large_and_unavailable_copy_at_320_and_wide() {
    let millis = 60_001_u64;
    let time = format!(
        "{}:{:02}.{:03}",
        millis / 60_000,
        millis / 1_000 % 60,
        millis % 1_000
    );
    let prior_millis = 59_999_u64;
    let previous = format!(
        "{}:{:02}.{:03}",
        prior_millis / 60_000,
        prior_millis / 1_000 % 60,
        prior_millis % 1_000
    );
    let delta = format!("{:+.3} s", -1.25_f64);
    let counter = u64::MAX.to_string();
    let rows = [
        row("PREVIOUS", &previous),
        row("TIME", &time),
        row("DELTA", &delta),
        row("DELIVERIES", &counter),
        row("BEST", "UNAVAILABLE"),
        row("EMPTY", ""),
    ];
    assert_eq!(
        (&*previous, &*time, &*delta, &*counter),
        ("0:59.999", "1:00.001", "-1.250 s", "18446744073709551615")
    );
    let style = StatRowsStyle::default();
    let mut ui = Ui::new();
    ui.set_inspection_enabled(true);
    for (width, mode) in [
        (608., StatRowsMode::Columns),
        (288., StatRowsMode::Stacked),
        (54., StatRowsMode::Stacked),
    ] {
        let bounds = UiRect {
            size: [width, 1100.],
            ..BOUNDS
        };
        ui.begin(viewport());
        let layout = StatRowsLayout::new(bounds, &rows, style).unwrap();
        assert_eq!(layout.report().mode, mode);
        assert_eq!(layout.report().hidden_rows, 0);
        assert_geometry(&layout, &rows, style, bounds);
        assert_eq!(ui.stat_rows(bounds, &rows, style).unwrap(), layout.report());
        let actual = inspected(&ui);
        for (index, (source, geometry)) in rows.iter().zip(layout.rows()).enumerate() {
            assert_eq!(actual[index * 2], (source.label.to_owned(), geometry.label));
            assert_eq!(
                actual[index * 2 + 1],
                (source.value.to_owned(), geometry.value)
            );
            if mode == StatRowsMode::Columns {
                assert_eq!(geometry.label.size[0], layout.report().label_width);
                assert_eq!(geometry.label.min[1], geometry.value.min[1]);
                assert!(geometry.label.min[0] + geometry.label.size[0] < geometry.value.min[0]);
            } else {
                assert!(geometry.label.min[1] + geometry.label.size[1] < geometry.value.min[1]);
                assert_eq!(geometry.label.size[0], width);
                assert_eq!(geometry.value.size[0], width);
            }
        }
        ui.paint();
    }
}

#[test]
fn multiline_unicode_long_labels_descriptions_and_distinct_sizes_reuse_wrapping() {
    let unicode = "é日ABC DEF\n\nRoute identification".repeat(8);
    let rows = [
        StatRow {
            label: &unicode,
            value: "-123456789\nUNAVAILABLE",
            description: Some("First line\n\né日 and a long description for wrapping."),
        },
        row("", ""),
        StatRow {
            label: "MODE",
            value: "READY",
            description: Some(""),
        },
    ];
    let style = StatRowsStyle {
        label: TextStyle {
            size: 18.5,
            ..Default::default()
        },
        value: TextStyle {
            size: 20.,
            ..Default::default()
        },
        description: TextStyle {
            size: 16.,
            ..Default::default()
        },
        ..Default::default()
    };
    let bounds = UiRect {
        size: [288., 2000.],
        ..BOUNDS
    };
    let layout = StatRowsLayout::new(bounds, &rows, style).unwrap();
    assert_eq!(layout.report().mode, StatRowsMode::Stacked);
    assert_eq!(layout.report().hidden_rows, 0);
    assert_geometry(&layout, &rows, style, bounds);
    let mut ui = Ui::new();
    ui.set_inspection_enabled(true);
    ui.begin(Viewport::responsive([320., 2100.], [0.; 4]).unwrap());
    assert_eq!(
        ui.compose(|group| layout.submit(group)).unwrap(),
        layout.report()
    );
    let accepted = inspected(&ui);
    assert_eq!(accepted[0].0, unicode);
    assert_eq!(accepted[1].0, "-123456789\nUNAVAILABLE");
    assert_eq!(accepted[2].0, rows[0].description.unwrap());
    assert_eq!(accepted.len(), 8);
    ui.paint();
}

#[test]
fn whole_prefix_overflow_exact_fit_tiny_zero_empty_and_row_cap() {
    let rows = [row("A", "1"), row("B", "2"), row("C", "3")];
    let style = StatRowsStyle::default();
    let full = StatRowsLayout::new(BOUNDS, &rows, style).unwrap();
    assert_eq!(full.report().required_height, 78.);
    for (height, visible) in [
        (0., 0),
        (17.999, 0),
        (18., 1),
        (47.999, 1),
        (48., 2),
        (78., 3),
    ] {
        let bounds = UiRect {
            size: [288., height],
            ..BOUNDS
        };
        let layout = StatRowsLayout::new(bounds, &rows, style).unwrap();
        let report = layout.report();
        assert_eq!(
            (report.visible_rows, report.hidden_rows),
            (visible, 3 - visible)
        );
        assert_eq!(report.required_height, 78.);
        assert_eq!(layout.rows().len(), visible);
        assert_geometry(&layout, &rows, style, bounds);
    }
    for width in [0., 1., 13.5_f32.next_down()] {
        let layout = StatRowsLayout::new(
            UiRect {
                size: [width, 900.],
                ..BOUNDS
            },
            &rows,
            style,
        )
        .unwrap();
        assert!(layout.report().width_overflow);
        assert_eq!(
            (layout.report().visible_rows, layout.report().hidden_rows),
            (0, 3)
        );
    }
    let cell_fit = StatRowsLayout::new(
        UiRect {
            size: [13.5, 900.],
            ..BOUNDS
        },
        &rows,
        style,
    )
    .unwrap();
    assert_eq!(cell_fit.report().hidden_rows, 0);
    let empty = StatRowsLayout::new(
        UiRect {
            size: [0.; 2],
            ..BOUNDS
        },
        &[],
        style,
    )
    .unwrap();
    assert_eq!(empty.report().required_height, 0.);
    assert!(!empty.report().width_overflow);
    let maximum = [row("", ""); MAX_STAT_ROWS];
    let layout = StatRowsLayout::new(
        UiRect {
            size: [288., 2000.],
            ..BOUNDS
        },
        &maximum,
        style,
    )
    .unwrap();
    assert_eq!(layout.rows().len(), MAX_STAT_ROWS);
    let mut ui = Ui::new();
    ui.set_inspection_enabled(true);
    ui.begin(Viewport::responsive([320., 2100.], [0.; 4]).unwrap());
    assert_eq!(
        ui.stat_rows(
            UiRect {
                size: [288., 2000.],
                ..BOUNDS
            },
            &maximum,
            style
        )
        .unwrap()
        .visible_rows,
        MAX_STAT_ROWS
    );
    assert_eq!(ui.inspect().unwrap().count(), MAX_STAT_ROWS * 2);
    let before = inspected(&ui);
    assert_eq!(
        ui.stat_rows(BOUNDS, &[row("", ""); MAX_STAT_ROWS + 1], style),
        Err(UiError::Capacity)
    );
    assert_eq!(inspected(&ui), before);

    assert_eq!(
        StatRowsLayout::new(BOUNDS, &[row("", ""); MAX_STAT_ROWS + 1], style).unwrap_err(),
        UiError::Capacity
    );
    // First row cannot fit; a short later row is not silently reordered forward.
    let mixed = [row("A\nB\nC", "1"), row("SHORT", "2")];
    let layout = StatRowsLayout::new(
        UiRect {
            size: [288., 18.],
            ..BOUNDS
        },
        &mixed,
        style,
    )
    .unwrap();
    assert_eq!(
        (layout.report().visible_rows, layout.report().hidden_rows),
        (0, 2)
    );
}

#[test]
fn invalid_bounds_styles_gaps_and_unrepresentable_geometry_reject_atomically() {
    let rows = [row("TIME", "1:00.001")];
    let mut ui = Ui::new();
    ui.set_inspection_enabled(true);
    ui.begin(viewport());
    ui.text(BOUNDS, "PRIOR", TextStyle::default()).unwrap();
    let prior = inspected(&ui);
    for bounds in [
        UiRect {
            min: [f32::NAN, 0.],
            ..BOUNDS
        },
        UiRect {
            min: [0., f32::INFINITY],
            ..BOUNDS
        },
        UiRect {
            size: [-1., 20.],
            ..BOUNDS
        },
        UiRect {
            size: [100., f32::NEG_INFINITY],
            ..BOUNDS
        },
        UiRect {
            min: [f32::MAX, 0.],
            size: [f32::MAX, 20.],
        },
        UiRect {
            min: [0., 1e30],
            size: [288., 900.],
        },
    ] {
        assert_eq!(
            ui.stat_rows(bounds, &rows, StatRowsStyle::default()),
            Err(UiError::InvalidLayout)
        );
        assert_eq!(inspected(&ui), prior);
    }
    for size in [
        0.,
        1_f32.next_down(),
        256_f32.next_up(),
        f32::NAN,
        f32::INFINITY,
    ] {
        for kind in 0..3 {
            let mut style = StatRowsStyle::default();
            [&mut style.label, &mut style.value, &mut style.description][kind].size = size;
            assert_eq!(
                ui.stat_rows(BOUNDS, &rows, style),
                Err(UiError::InvalidLayout)
            );
        }
    }
    for kind in 0..3 {
        let mut style = StatRowsStyle::default();
        [&mut style.label, &mut style.value, &mut style.description][kind].color =
            Color::new(1., f32::NAN, 1., 1.);
        assert_eq!(
            ui.stat_rows(BOUNDS, &rows, style),
            Err(UiError::InvalidLayout)
        );
    }
    for invalid in [-1., f32::NAN, f32::INFINITY] {
        for kind in 0..4 {
            let mut style = StatRowsStyle::default();
            *[
                &mut style.column_gap,
                &mut style.row_gap,
                &mut style.stack_gap,
                &mut style.description_gap,
            ][kind] = invalid;
            assert_eq!(
                ui.stat_rows(BOUNDS, &rows, style),
                Err(UiError::InvalidLayout)
            );
        }
    }
    assert_eq!(inspected(&ui), prior);
}

#[test]
fn arena_command_and_vertex_rejection_roll_back_list_and_outer_card_and_begin_resets() {
    let mut ui = Ui::new();
    ui.set_inspection_enabled(true);
    let huge = " ".repeat(32761); // Final value exceeds the remaining text bytes.
    let rows = [row("A", "1"), row("B", &huge)];
    let bounds = UiRect {
        min: [16.; 2],
        size: [500_000., 900.],
    };
    let huge_viewport = Viewport::responsive([600_000., 1200.], [0.; 4]).unwrap();
    ui.begin(huge_viewport);
    ui.text(BOUNDS, "PRIOR", TextStyle::default()).unwrap();
    let prior = inspected(&ui);
    // The last value runs out of text bytes after earlier rows were submitted.
    assert_eq!(
        ui.stat_rows(bounds, &rows, StatRowsStyle::default()),
        Err(UiError::Capacity)
    );
    assert_eq!(inspected(&ui), prior);
    ui.compose(|group| {
        assert_eq!(
            group.stat_rows(bounds, &rows, StatRowsStyle::default()),
            Err(UiError::Capacity)
        );
        group.text(BOUNDS, "LOCAL FALLBACK", TextStyle::default())
    })
    .unwrap();
    assert_eq!(ui.inspect().unwrap().count(), 2);
    let before_outer = inspected(&ui);
    assert_eq!(
        ui.compose(|group| {
            group.modal(999)?;
            group.button(70, BOUNDS, "REJECTED", true, ButtonStyle::default())?;
            group.stat_rows(bounds, &rows, StatRowsStyle::default())?;
            Ok(())
        }),
        Err(UiError::Capacity)
    );
    assert_eq!(inspected(&ui), before_outer);
    assert!(
        !ui.inspect()
            .unwrap()
            .any(|e| e.id == Some(70) || e.id == Some(999))
    );

    // Fill text storage without charging vertices, then fail the final value
    // after the first complete row and second label have actually been queued.
    ui.begin(viewport());
    let filler = " ".repeat(32761);
    ui.wrapped_text(BOUNDS, &filler, TextStyle::default(), LineLimit::Clip(0))
        .unwrap();
    let prior = inspected(&ui);
    assert_eq!(
        ui.stat_rows(
            BOUNDS,
            &[row("A", "1"), row("B", "12345")],
            StatRowsStyle::default()
        ),
        Err(UiError::Capacity)
    );
    assert_eq!(inspected(&ui), prior);
    ui.text(BOUNDS, "1234567", TextStyle::default()).unwrap(); // Exact remaining bytes.

    ui.begin(viewport());
    for _ in 0..1023 {
        ui.panel(BOUNDS, PANEL).unwrap();
    }
    let prior = inspected(&ui);
    assert_eq!(
        ui.stat_rows(BOUNDS, &[row("A", "1")], StatRowsStyle::default()),
        Err(UiError::Capacity)
    );
    assert_eq!(inspected(&ui), prior); // Label used last slot; its value failed.
    ui.panel(BOUNDS, PANEL).unwrap(); // Rollback restored the slot.

    ui.begin(viewport());
    let glyphs = "A".repeat(1559); // 196434 of 196608 estimated vertices.
    ui.text(
        UiRect {
            size: [30_000., 18.],
            ..BOUNDS
        },
        &glyphs,
        TextStyle::default(),
    )
    .unwrap();
    let prior = inspected(&ui);
    assert_eq!(
        ui.stat_rows(BOUNDS, &[row("A", "1")], StatRowsStyle::default()),
        Err(UiError::Capacity)
    );
    assert_eq!(inspected(&ui), prior); // First block fits; second fails.
    ui.begin(viewport());
    assert_eq!(ui.inspect().unwrap().count(), 0);
    let report = ui
        .stat_rows(BOUNDS, &[row("RESET", "READY")], StatRowsStyle::default())
        .unwrap();
    assert_eq!((report.visible_rows, report.hidden_rows), (1, 0));
    assert_eq!(ui.inspect().unwrap().next().unwrap().painted_label, "RESET");
    ui.begin(viewport());
    assert_eq!(
        ui.stat_rows(BOUNDS, &[], StatRowsStyle::default())
            .unwrap()
            .visible_rows,
        0
    );
    assert_eq!(ui.inspect().unwrap().count(), 0);
}

thread_local! {
    static RECORD: Cell<bool> = const { Cell::new(false) };
    static CALLS: Cell<usize> = const { Cell::new(0) };
}
struct Allocator;
fn allocated(pointer: *mut u8) {
    if !pointer.is_null() {
        let _ = RECORD.try_with(|on| {
            if on.get() {
                let _ = CALLS.try_with(|n| n.set(n.get() + 1));
            }
        });
    }
}
// SAFETY: Forward the exact original allocation contract to System. Thread-local
// counters allocate nothing and ignore destruction-time TLS failures.
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        allocated(p);
        p
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc_zeroed(layout) };
        allocated(p);
        p
    }
    unsafe fn realloc(&self, p: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let p = unsafe { System.realloc(p, layout, size) };
        allocated(p);
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        unsafe { System.dealloc(p, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: Allocator = Allocator;

#[test]
fn measured_layout_formatted_caller_buffer_atomic_fallback_inspection_and_paint_reuse_storage() {
    for inspection in [false, true] {
        let mut ui = Ui::new();
        ui.set_inspection_enabled(inspection);
        let reserved = ui.reserved_bytes();
        let mut formatted = String::with_capacity(64);
        let huge = " ".repeat(32769);
        let mut frame = |index: usize| {
            host_frame(0., 640, 1000, || {
                formatted.clear();
                write!(formatted, "{:+.3} s / {}", -1.25_f64, index).unwrap();
                let rows = [
                    row("DELTA", &formatted),
                    StatRow {
                        label: "TIME",
                        value: "1:00.001",
                        description: Some("Accepted results only.\nUNAVAILABLE stays explicit."),
                    },
                ];
                ui.begin(Viewport::responsive([640., 1000.], [0.; 4]).unwrap());
                let bounds = UiRect {
                    size: [if index.is_multiple_of(2) { 608. } else { 288. }, 900.],
                    ..BOUNDS
                };
                let layout = StatRowsLayout::new(bounds, &rows, StatRowsStyle::default()).unwrap();
                ui.compose(|group| {
                    group.text_fmt(BOUNDS, TextStyle::default(), format_args!("RESULT {index}"))?;
                    layout.submit(group)?;
                    let rejected = [row("A", "1"), row("B", &huge)];
                    assert_eq!(
                        group.stat_rows(
                            UiRect {
                                size: [600_000., 900.],
                                ..BOUNDS
                            },
                            &rejected,
                            StatRowsStyle::default()
                        ),
                        Err(UiError::Capacity)
                    );
                    group.text(BOUNDS, "RESULTS UNAVAILABLE", TextStyle::default())
                })
                .unwrap();
                ui.finish(false);
                if inspection {
                    assert!(ui.inspect().unwrap().any(|e| e.painted_label == formatted));
                    let mut output = [0; 8192];
                    ui.write_inspection(&mut output).unwrap();
                }
                ui.paint();
            })
        };
        for index in 0..6 {
            frame(index);
        }
        CALLS.with(|n| n.set(0));
        RECORD.with(|on| on.set(true));
        for index in 0..300 {
            frame(index);
        }
        RECORD.with(|on| on.set(false));
        assert_eq!(CALLS.with(Cell::get), 0);
        assert_eq!(ui.reserved_bytes(), reserved);
        assert_eq!(formatted.capacity(), 64);
    }
}

#[test]
fn adjacent_fractional_rows_and_blocks_remain_complete_and_nonoverlapping() {
    let rows = [StatRow {
        label: "LABEL\nSECOND",
        value: "-1.250 s",
        description: Some("Description wraps across lines."),
    }; 8];
    for size in [1., 8., 17.3, 18.5, 31.7, 256.] {
        for origin in [-16.1, 0., 0.1, 16., 100.3] {
            for width in [288., 288.1, 1200., 1200.1] {
                let text = TextStyle {
                    size,
                    ..Default::default()
                };
                let style = StatRowsStyle {
                    label: text,
                    value: text,
                    description: text,
                    column_gap: 0.,
                    row_gap: 0.,
                    stack_gap: 0.,
                    description_gap: 0.,
                };
                let bounds = UiRect {
                    min: [origin; 2],
                    size: [width, 40000.],
                };
                let layout = StatRowsLayout::new(bounds, &rows, style).unwrap();
                assert_geometry(&layout, &rows, style, bounds);
            }
        }
    }
}

#[test]
fn rejected_result_card_never_owns_synthetic_pointer_and_explicit_fallback_activates() {
    let mut ui = Ui::new();
    ui.set_inspection_enabled(true);
    let rejected_copy = "A".repeat(32769);
    let action = UiRect {
        min: [16., 180.],
        size: [288., 44.],
    };
    for phase in [touch::Phase::Start, touch::Phase::End] {
        touch_event(touch::Event {
            id: 1,
            phase,
            position: Vec2::new(30., 195.),
        });
    }
    host_frame(0., 320, 240, || {
        ui.begin(Viewport::responsive([320., 240.], [0.; 4]).unwrap());
        assert_eq!(
            ui.compose(|card| {
                card.modal(900)?;
                card.button(901, action, "REJECTED", true, ButtonStyle::default())?;
                card.stat_rows(
                    UiRect {
                        size: [600_000., 100.],
                        ..BOUNDS
                    },
                    &[row("RESULT", &rejected_copy)],
                    StatRowsStyle::default(),
                )?;
                Ok(())
            }),
            Err(UiError::Capacity)
        );
        assert_eq!(ui.inspect().unwrap().count(), 0);
        ui.finish(true);
        assert!(!ui.interaction().pointer_consumed);
        assert_eq!(ui.interaction().pointer_activated, None);
        assert!(!ui.activated(901));
        ui.paint();
    });
    for phase in [touch::Phase::Start, touch::Phase::End] {
        touch_event(touch::Event {
            id: 2,
            phase,
            position: Vec2::new(30., 195.),
        });
    }
    host_frame(0., 320, 240, || {
        ui.begin(Viewport::responsive([320., 240.], [0.; 4]).unwrap());
        ui.compose(|card| {
            card.text(BOUNDS, "RESULTS UNAVAILABLE", TextStyle::default())?;
            card.button(901, action, "CONTINUE", true, ButtonStyle::default())
        })
        .unwrap();
        ui.finish(true);
        assert_eq!(ui.interaction().pointer_activated, Some(901));
        assert!(ui.activated(901));
        ui.paint();
    });
}
