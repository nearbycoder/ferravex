use ferravex_render::ui::{
    TextStyle, Ui, UiError, UiRect, Viewport,
    text_layout::{LineLimit, TextBlockMetrics, WrappedText},
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};

#[test]
fn borrowed_spans_preserve_existing_whitespace_newline_and_utf8_rules() {
    for (copy, columns, expected) in [
        ("", 5, vec![]),
        ("\n", 5, vec![""]),
        ("\n\n", 5, vec!["", ""]),
        ("A\n", 5, vec!["A"]),
        ("A\n\n", 5, vec!["A", ""]),
        ("A\r\nB", 5, vec!["A\r", "B"]),
        ("  A  ", 8, vec!["  A  "]),
        ("alpha  beta\n\né日A", 5, vec!["alpha", "beta", "", "é日A"]),
        ("A\t\r B", 2, vec!["A", "B"]),
        ("A\u{2003}BC", 2, vec!["A", "\u{2003}B", "C"]),
        ("é日ABC", 2, vec!["é日", "AB", "C"]),
        ("AB", 0, vec!["A", "B"]),
        ("   ", 1, vec![" ", " ", " "]),
    ] {
        let layout = WrappedText::new(copy, columns as f32 * 6., 8.).unwrap();
        let actual: Vec<_> = layout.lines().collect();
        assert_eq!(
            actual.iter().map(|l| l.text).collect::<Vec<_>>(),
            expected,
            "{copy:?}"
        );
        let mut end = 0;
        for line in actual {
            assert!(line.byte_range.start >= end);
            assert_eq!(&copy[line.byte_range.clone()], line.text);
            end = line.byte_range.end;
            assert_eq!(line.suffix, "");
            assert_eq!(line.width, line.text.chars().count() as f32 * 6.);
        }
        let report = layout.measure();
        assert_eq!(report.line_count, expected.len());
        assert_eq!(report.visible_lines, report.line_count);
        assert_eq!(report.hidden_lines, 0);
        assert_eq!(report.required_height, expected.len() as f32 * 8.);
        assert_eq!(report.height, report.required_height);
        assert!(!report.is_truncated());
    }
}

#[test]
fn clip_and_ellipsis_count_hidden_lines_and_only_mark_actual_omissions() {
    let copy = "é日ABC DEF\n\nZ";
    let base = WrappedText::new(copy, 30., 8.).unwrap();
    assert_eq!(base.measure().line_count, 4);
    for count in [0, 1, 2, 3, 4, usize::MAX] {
        for limit in [LineLimit::Clip(count), LineLimit::Ellipsis(count)] {
            let layout = base.with_limit(limit);
            let report = layout.measure();
            assert_eq!(report.visible_lines, count.min(4));
            assert_eq!(report.hidden_lines, 4 - count.min(4));
            assert_eq!(report.is_truncated(), count < 4);
            assert_eq!(report.required_height, 32.);
            assert_eq!(report.height, count.min(4) as f32 * 8.);
            let lines: Vec<_> = layout.lines().collect();
            assert_eq!(lines.len(), report.visible_lines);
            for (i, line) in lines.iter().enumerate() {
                assert_eq!(&copy[line.byte_range.clone()], line.text);
                let marked = matches!(limit, LineLimit::Ellipsis(_)) && count < 4 && i + 1 == count;
                assert_eq!(line.suffix, if marked { "..." } else { "" });
                assert!(line.width <= 30.);
            }
        }
    }
    let line = base
        .with_limit(LineLimit::Ellipsis(1))
        .lines()
        .next()
        .unwrap();
    assert_eq!(
        (line.text, line.suffix, line.byte_range),
        ("é日", "...", 0..5)
    );
    for copy in ["ABCDE", "ABCDE\n", ""] {
        let layout = WrappedText::new(copy, 30., 8.)
            .unwrap()
            .with_limit(LineLimit::Ellipsis(1));
        assert!(!layout.measure().is_truncated());
        assert!(layout.lines().all(|line| line.suffix.is_empty()));
    }
    assert_eq!(
        WrappedText::new("", 0., 8.).unwrap().measure(),
        TextBlockMetrics::default()
    );
}

#[test]
fn narrow_limits_never_shrink_or_split_utf8_and_invalid_geometry_is_rejected() {
    for (width, expected) in [(0., ""), (5., ""), (6., "."), (12., ".."), (18., "...")] {
        let layout = WrappedText::new("é日ABCD", width, 8.)
            .unwrap()
            .with_limit(LineLimit::Ellipsis(1));
        let line = layout.lines().next().unwrap();
        assert_eq!((line.text, line.suffix), ("", expected));
        assert!(line.width <= width);
        assert_eq!(layout.line_height(), 8.);
        assert!(layout.measure().is_truncated());
    }
    for (width, count) in [(12_f32.next_down(), 2), (12., 1), (12_f32.next_up(), 1)] {
        assert_eq!(
            WrappedText::new("AB", width, 8.)
                .unwrap()
                .measure()
                .line_count,
            count
        );
    }
    for width in [-1., f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(
            WrappedText::new("", width, 8.).unwrap_err(),
            UiError::InvalidLayout
        );
    }
    for size in [
        0.,
        1_f32.next_down(),
        256_f32.next_up(),
        f32::NAN,
        f32::INFINITY,
    ] {
        assert_eq!(
            WrappedText::new("", 30., size).unwrap_err(),
            UiError::InvalidLayout
        );
    }
    for size in [1., 8., 17.3, 18.5, 256.] {
        let metrics = ferravex_render::get_default_font()
            .try_metrics(8, size / 8.)
            .unwrap();
        let layout = WrappedText::new("AB\nCD", f32::MAX, size).unwrap();
        assert_eq!(layout.line_height(), metrics.line_height());
        assert_eq!(layout.measure().width, 2. * metrics.glyph_advance());
        assert_eq!(layout.measure().height, 2. * size);
    }
}

#[test]
fn multiparagraph_briefing_reflows_at_320_units_and_cursors_restart() {
    let copy = "Collect the amber keys before opening the gate.\n\n\
                Return to the checkpoint if the route is blocked.";
    let layout = WrappedText::new(copy, 288., 18.).unwrap();
    assert_eq!(
        layout.lines().map(|l| l.text).collect::<Vec<_>>(),
        [
            "Collect the amber",
            "keys before opening",
            "the gate.",
            "",
            "Return to the",
            "checkpoint if the",
            "route is blocked.",
        ]
    );
    assert_eq!(layout.measure().height, 126.);
    let mut cursor = layout.with_limit(LineLimit::Clip(2)).lines();
    assert_eq!(cursor.next().unwrap().text, "Collect the amber");
    let saved = cursor.clone();
    assert_eq!(cursor.by_ref().count(), 1);
    assert_eq!(cursor.next(), None);
    assert_eq!(cursor.next(), None);
    assert_eq!(saved.count(), 1);
    assert_eq!(layout.lines().count(), 7);
    let wide = WrappedText::new(copy, 640., 18.).unwrap();
    assert!(wide.measure().height < layout.measure().height);
    let mut ui = Ui::new();
    ui.begin(Viewport::responsive([320., 400.], [0.; 4]).unwrap());
    let report = ui
        .wrapped_text(
            UiRect {
                min: [16., 24.],
                size: [288., layout.measure().height],
            },
            copy,
            TextStyle::default(),
            LineLimit::Unlimited,
        )
        .unwrap();
    assert_eq!(report, layout.measure());
}

thread_local! {
    static ALLOCATIONS: Cell<Option<usize>> = const { Cell::new(None) };
}
struct CountingAllocator;
fn count_allocation() {
    let _ = ALLOCATIONS.try_with(|count| {
        if let Some(value) = count.get() {
            count.set(Some(value + 1));
        }
    });
}
// SAFETY: All allocation operations forward their original arguments to System;
// the thread-local counter stores only a number and performs no allocation.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count_allocation();
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count_allocation();
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        count_allocation();
        unsafe { System.realloc(ptr, layout, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

#[test]
fn layout_and_reused_ui_queuing_allocate_nothing_and_large_sources_remain_borrowed() {
    let large = "é日ABC DEF\n\n".repeat(10_000);
    let copy = "Collect the keys.\n\nReturn to the amber gate.";
    let mut ui = Ui::new();
    let viewport = Viewport::responsive([320., 240.], [0.; 4]).unwrap();
    let reserved = ui.reserved_bytes();
    ALLOCATIONS.with(|count| count.set(Some(0)));
    let report = WrappedText::new(&large, 30., 8.)
        .unwrap()
        .with_limit(LineLimit::Ellipsis(2))
        .measure();
    for width in [0., 1., 6., 30., 288., 640., 288.] {
        for limit in [
            LineLimit::Unlimited,
            LineLimit::Clip(0),
            LineLimit::Ellipsis(2),
        ] {
            let layout = WrappedText::new(copy, width, 18.)
                .unwrap()
                .with_limit(limit);
            std::hint::black_box(layout.measure());
            for line in layout.lines() {
                std::hint::black_box(line);
            }
            ui.begin(viewport);
            ui.wrapped_text(
                UiRect {
                    min: [0.; 2],
                    size: [width, 200.],
                },
                copy,
                TextStyle::default(),
                limit,
            )
            .unwrap();
        }
    }
    let allocations = ALLOCATIONS.with(|count| count.replace(None).unwrap());
    assert_eq!(allocations, 0);
    assert_eq!(ui.reserved_bytes(), reserved);
    assert_eq!(report.line_count, 30_000);
    assert_eq!(report.hidden_lines, 29_998);
    assert_eq!(
        ui.wrapped_text(
            UiRect {
                min: [0.; 2],
                size: [30., 20.]
            },
            &large,
            TextStyle::default(),
            LineLimit::Clip(1),
        ),
        Err(UiError::Capacity)
    );
}

#[test]
fn wrapped_paint_with_clips_and_ellipsis_allocates_nothing_after_warmup() {
    let mut ui = Ui::new();
    let viewport = Viewport::responsive([320., 240.], [0.; 4]).unwrap();
    let reserved = ui.reserved_bytes();
    let mut draw = |index: usize| {
        ferravex_render::input::host_frame(1. / 60., 320, 240, || {
            ui.begin(viewport);
            ui.push_clip(UiRect {
                min: [16., 24.],
                size: [288., 180.],
            })
            .unwrap();
            let width = [0., 1., 6., 30., 288.][index % 5];
            ui.wrapped_text(
                UiRect {
                    min: [16., 24.],
                    size: [width, 180.],
                },
                "Collect the keys.\n\né日ABC DEF\nReturn to the amber gate.",
                TextStyle {
                    shadow: true,
                    ..Default::default()
                },
                [
                    LineLimit::Unlimited,
                    LineLimit::Clip(0),
                    LineLimit::Ellipsis(2),
                ][index % 3],
            )
            .unwrap();
            ui.paint();
        });
    };
    for index in 0..30 {
        draw(index);
    }
    ALLOCATIONS.with(|count| count.set(Some(0)));
    for index in 0..300 {
        draw(index);
    }
    let allocations = ALLOCATIONS.with(|count| count.replace(None).unwrap());
    assert_eq!(allocations, 0);
    assert_eq!(ui.reserved_bytes(), reserved);
}
