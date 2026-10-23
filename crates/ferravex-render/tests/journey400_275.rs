//! Public native regression and caller-buffer/allocation evidence for task275.
use ferravex_play::ui::{
    meters::*,
    theme::{ThemeMode, UiTheme},
};
use ferravex_render::{
    input::host_frame,
    ui::{Ui, UiError, UiLabel, UiRect, Viewport, meters::*},
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    hint::black_box,
};

const BOUNDS: UiRect = UiRect {
    min: [16., 12.],
    size: [320., 600.],
};
const COPY: MeterCopy<'static> = MeterCopy {
    label: "ENERGY",
    display: "2.0s REMAINING",
    status: "COOLING",
};
fn theme() -> UiTheme {
    UiTheme::preset(ThemeMode::Normal)
}
fn known(current: f64) -> MeterValue {
    MeterValue::Known {
        current,
        capacity: 6.,
    }
}
fn meter(value: MeterValue) -> MeterLayout<'static> {
    MeterLayout::meter(BOUNDS, COPY, value, &[], theme()).unwrap()
}
fn viewport() -> Viewport {
    Viewport::responsive([352., 700.], [0.; 4]).unwrap()
}

#[test]
fn continuous_boundaries_no_capacity_unknown_and_reset_are_distinct() {
    let r = UiRect {
        min: [-10.5, 2.25],
        size: [120., 8.],
    };
    for (value, width) in [
        (known(0.), 0.),
        (known(3.), 60.),
        (known(6.), 120.),
        (MeterValue::Empty, 0.),
    ] {
        let g = MeterGeometry::new(r, value).unwrap();
        assert_eq!(g.value(), value);
        assert_eq!(g.bounds(), r);
        assert_eq!(
            g.fill().unwrap(),
            UiRect {
                size: [width, 8.],
                ..r
            }
        );
    }
    assert_eq!(MeterValue::Unknown.fraction(), Ok(None));
    assert_eq!(MeterValue::Empty.fraction(), Ok(Some(0.)));
    assert_eq!(
        MeterGeometry::new(r, MeterValue::Unknown).unwrap().fill(),
        None
    );
    let mut snapshot = meter(known(6.));
    assert_eq!(snapshot.continuous().unwrap().fill().unwrap().size[0], 316.);
    snapshot = meter(MeterValue::Unknown);
    assert_eq!(snapshot.continuous().unwrap().fill(), None);
    snapshot = meter(known(0.));
    assert_eq!(snapshot.continuous().unwrap().fill().unwrap().size[0], 0.);
    assert_eq!(snapshot.copy(), COPY);
}

#[test]
fn invalid_values_never_clamp_or_divide_zero() {
    for (current, capacity) in [
        (0., 0.),
        (1., 0.),
        (-1., 1.),
        (2., 1.),
        (0., -1.),
        (f64::NAN, 1.),
        (0., f64::NAN),
        (f64::INFINITY, 1.),
        (0., f64::INFINITY),
        (f64::NEG_INFINITY, 1.),
        (f64::MIN_POSITIVE, f64::MAX),
    ] {
        let value = MeterValue::Known { current, capacity };
        assert_eq!(value.fraction(), Err(MeterError::InvalidValue));
        assert!(matches!(
            MeterLayout::meter(BOUNDS, COPY, value, &[], theme()),
            Err(UiError::InvalidLayout)
        ));
    }
    let extreme = MeterValue::Known {
        current: f64::MAX,
        capacity: f64::MAX,
    };
    assert_eq!(extreme.fraction(), Ok(Some(1.)));
    assert_eq!(
        MeterGeometry::new(BOUNDS, extreme).unwrap().fill(),
        Some(BOUNDS)
    );
}

#[test]
fn geometry_rejects_negative_overflow_unrepresentable_and_handles_zero() {
    for r in [
        UiRect {
            min: [f32::NAN, 0.],
            ..BOUNDS
        },
        UiRect {
            size: [-1., 1.],
            ..BOUNDS
        },
        UiRect {
            min: [f32::MAX, 0.],
            size: [f32::MAX, 1.],
        },
        UiRect {
            min: [1e30, 0.],
            size: [1., 1.],
        },
    ] {
        assert_eq!(
            MeterGeometry::new(r, known(1.)),
            Err(MeterError::InvalidGeometry)
        );
    }
    for size in [[0., 0.], [0., 1.], [1., 0.], [0.125, 0.25]] {
        let r = UiRect { min: [0.; 2], size };
        assert!(MeterGeometry::new(r, known(3.)).is_ok());
        assert!(matches!(
            MeterLayout::meter(r, COPY, known(3.), &[], theme()),
            Err(UiError::InvalidLayout)
        ));
    }
    assert!(
        MeterGeometry::new(
            BOUNDS,
            MeterValue::Known {
                current: 1e-200,
                capacity: 1.
            }
        )
        .is_err()
    );
}

#[test]
fn markers_at_endpoints_and_atomic_caller_buffers() {
    let r = UiRect {
        min: [-8., 1.5],
        size: [120., 8.],
    };
    let g = MeterGeometry::new(r, known(3.)).unwrap();
    let sentinel = UiRect {
        min: [999.; 2],
        size: [1.; 2],
    };
    let mut output = [sentinel; MAX_METER_MARKERS + 1];
    assert_eq!(g.markers_into(&[0., 0.25, 1.], 2., &mut output), Ok(3));
    assert_eq!(output[0].min[0], -8.);
    assert_eq!(output[1].min[0], 21.);
    assert_eq!(output[2].min[0] + output[2].size[0], 112.);
    assert!(output[3..].iter().all(|r| *r == sentinel));
    let before = output;
    for bad in [f64::NAN, f64::INFINITY, -0.01, 1.01] {
        assert_eq!(
            g.markers_into(&[0.25, bad], 2., &mut output),
            Err(MeterError::InvalidValue)
        );
        assert_eq!(output, before);
    }
    assert_eq!(
        g.markers_into(&[0.; MAX_METER_MARKERS + 1], 2., &mut output),
        Err(MeterError::Capacity)
    );
    assert_eq!(
        g.markers_into(&[0.25; 2], 2., &mut output[..1]),
        Err(MeterError::Capacity)
    );
    for width in [0., -1., f32::NAN, f32::INFINITY, 121.] {
        assert_eq!(
            g.markers_into(&[0.25], width, &mut output),
            Err(MeterError::InvalidGeometry)
        );
        assert_eq!(output, before);
    }
    assert_eq!(
        g.markers_into(&[0.5; MAX_METER_MARKERS], 2., &mut output),
        Ok(MAX_METER_MARKERS)
    );
    assert_eq!(output[MAX_METER_MARKERS], sentinel);
}

#[test]
fn six_steps_max_segments_empty_unknown_and_atomic_output() {
    let r = UiRect {
        min: [-4., 0.],
        size: [118., 10.],
    };
    let sentinel = SegmentRect {
        bounds: BOUNDS,
        filled: true,
    };
    let mut output = [sentinel; MAX_METER_SEGMENTS + 1];
    for filled in [0, 2, 6] {
        let g = SegmentGeometry::new(
            r,
            SegmentValue::Known {
                filled,
                capacity: 6,
            },
            2.,
        )
        .unwrap();
        assert_eq!(g.count(), 6);
        assert_eq!(g.bounds(), r);
        assert_eq!(g.write_into(&mut output), Ok(6));
        assert_eq!(output[..6].iter().filter(|c| c.filled).count(), filled);
        assert_eq!(output[5].bounds.min[0] + output[5].bounds.size[0], 114.);
        for pair in output[..6].windows(2) {
            assert_eq!(
                pair[1].bounds.min[0] - pair[0].bounds.min[0] - pair[0].bounds.size[0],
                2.
            );
        }
        let before = output;
        assert_eq!(g.write_into(&mut output[..5]), Err(MeterError::Capacity));
        assert_eq!(output, before);
    }
    for value in [SegmentValue::Empty, SegmentValue::Unknown { capacity: 0 }] {
        let g = SegmentGeometry::new(r, value, 0.).unwrap();
        let before = output;
        assert_eq!(g.write_into(&mut output), Ok(0));
        assert_eq!(output, before);
    }
    let max = SegmentGeometry::new(
        UiRect {
            size: [4096., 10.],
            ..r
        },
        SegmentValue::Known {
            filled: 64,
            capacity: 64,
        },
        0.,
    )
    .unwrap();
    assert_eq!(max.write_into(&mut output), Ok(64));
    assert!(output[..64].iter().all(|c| c.filled));
    assert_eq!(output[64], sentinel);
    for value in [
        SegmentValue::Known {
            filled: 0,
            capacity: 0,
        },
        SegmentValue::Known {
            filled: 7,
            capacity: 6,
        },
    ] {
        assert_eq!(value.count(), Err(MeterError::InvalidValue));
    }
    assert_eq!(
        SegmentValue::Unknown { capacity: 65 }.count(),
        Err(MeterError::Capacity)
    );
    for gap in [-1., f32::NAN, f32::INFINITY, 24., f32::MAX] {
        assert!(
            SegmentGeometry::new(
                r,
                SegmentValue::Known {
                    filled: 0,
                    capacity: 6
                },
                gap
            )
            .is_err()
        );
    }
    let unknown = SegmentGeometry::new(r, SegmentValue::Unknown { capacity: 6 }, 2.).unwrap();
    unknown.write_into(&mut output).unwrap();
    assert!(output[..6].iter().all(|c| !c.filled));
}

#[test]
fn text_wraps_without_shrink_or_ellipsis_and_inspection_is_passive() {
    let mut ui = Ui::new();
    ui.set_inspection_enabled(true);
    let authored = MeterCopy {
        label: "ROUTE ACCEPTED",
        display: "2 OF 6 ACCEPTED PACKAGES",
        status: "PAUSED: SAME TELEMETRY",
    };
    for mode in [ThemeMode::Normal, ThemeMode::HighContrast] {
        for factor in [1., 1.5] {
            let mut t = UiTheme::preset(mode);
            t.set_metrics(t.types().try_scaled(factor).unwrap(), t.spacing())
                .unwrap();
            let layout = MeterLayout::segments(
                UiRect {
                    size: [180., 600.],
                    ..BOUNDS
                },
                authored,
                SegmentValue::Known {
                    filled: 2,
                    capacity: 6,
                },
                t,
            )
            .unwrap();
            assert!(layout.regions().display.size[1] > t.types().body);
            assert_eq!(layout.cells().iter().filter(|c| c.filled).count(), 2);
            host_frame(0., 352, 700, || {
                ui.begin(viewport());
                layout.draw(&mut ui).unwrap();
                ui.finish(true);
                for (text, rect) in [
                    (authored.label, layout.regions().label),
                    (authored.display, layout.regions().display),
                    (authored.status, layout.regions().status),
                ] {
                    let e = ui
                        .inspect()
                        .unwrap()
                        .find(|e| e.label == UiLabel::Full(text))
                        .unwrap();
                    assert_eq!(e.painted_label, text);
                    assert_eq!(e.bounds, rect);
                    assert_eq!(e.visible_bounds, Some(rect));
                    assert!(!e.interaction_registered);
                }
                assert_eq!(ui.focused(), None);
                assert!(!ui.interaction().pointer_consumed);
                ui.paint();
            });
        }
    }
}

#[test]
fn layout_rejects_blank_copy_source_caps_marker_caps_and_small_bounds() {
    for copy in [
        MeterCopy { label: " ", ..COPY },
        MeterCopy {
            display: "",
            ..COPY
        },
        MeterCopy {
            status: "\n\t",
            ..COPY
        },
    ] {
        assert!(matches!(
            MeterLayout::meter(BOUNDS, copy, known(3.), &[], theme()),
            Err(UiError::InvalidLayout)
        ));
    }
    let large = "A".repeat(MAX_METER_BYTES);
    assert!(matches!(
        MeterLayout::meter(
            BOUNDS,
            MeterCopy {
                display: &large,
                ..COPY
            },
            known(3.),
            &[],
            theme()
        ),
        Err(UiError::Capacity)
    ));
    let exact = "A".repeat(MAX_METER_BYTES - 2);
    let huge = UiRect {
        min: [0.; 2],
        size: [10000., 10000.],
    };
    assert!(
        MeterLayout::meter(
            huge,
            MeterCopy {
                label: "L",
                display: &exact,
                status: "S"
            },
            known(0.),
            &[],
            theme()
        )
        .is_ok()
    );
    let threshold = Threshold {
        fraction: 0.25,
        label: "REARM >25%",
    };
    assert!(
        MeterLayout::meter(
            BOUNDS,
            COPY,
            known(3.),
            &[threshold; MAX_METER_MARKERS],
            theme()
        )
        .is_ok()
    );
    assert!(matches!(
        MeterLayout::meter(
            BOUNDS,
            COPY,
            known(3.),
            &[threshold; MAX_METER_MARKERS + 1],
            theme()
        ),
        Err(UiError::Capacity)
    ));
    assert!(matches!(
        MeterLayout::meter(
            BOUNDS,
            COPY,
            known(3.),
            &[Threshold {
                label: "",
                ..threshold
            }],
            theme()
        ),
        Err(UiError::InvalidLayout)
    ));
    for size in [
        [0., 1000.],
        [320., 0.],
        [1., 1000.],
        [320., 50.],
        [44., 120.],
    ] {
        assert!(matches!(
            MeterLayout::meter(UiRect { size, ..BOUNDS }, COPY, known(3.), &[], theme()),
            Err(UiError::InvalidLayout)
        ));
    }
    assert!(matches!(
        MeterLayout::segments(
            BOUNDS,
            COPY,
            SegmentValue::Unknown { capacity: 65 },
            theme()
        ),
        Err(UiError::Capacity)
    ));
    assert!(matches!(
        MeterLayout::segments(
            BOUNDS,
            COPY,
            SegmentValue::Known {
                filled: 1,
                capacity: 64
            },
            theme()
        ),
        Err(UiError::InvalidLayout)
    ));
    assert!(
        MeterLayout::segments(
            UiRect {
                size: [4096., 600.],
                ..BOUNDS
            },
            COPY,
            SegmentValue::Known {
                filled: 64,
                capacity: 64
            },
            theme()
        )
        .is_ok()
    );
}

#[test]
fn late_command_and_clip_errors_roll_back_inspection_and_paint_description() {
    let mut ui = Ui::new();
    ui.set_inspection_enabled(true);
    let layout = meter(known(3.));
    host_frame(0., 352, 700, || {
        ui.begin(viewport());
        for _ in 0..1020 {
            ui.panel(BOUNDS, ferravex_render::ui::PANEL).unwrap();
        }
        let mut before = [0u8; 300000];
        let count = ui.write_inspection(&mut before).unwrap();
        assert_eq!(layout.draw(&mut ui), Err(UiError::Capacity));
        let mut after = [0u8; 300000];
        assert_eq!(ui.write_inspection(&mut after).unwrap(), count);
        assert_eq!(before[..count], after[..count]);
        assert_eq!(ui.inspect().unwrap().len(), 1020);
        // The failed meter did not consume the four remaining command slots.
        for _ in 0..4 {
            ui.panel(BOUNDS, ferravex_render::ui::PANEL).unwrap();
        }
        assert_eq!(
            ui.panel(BOUNDS, ferravex_render::ui::PANEL),
            Err(UiError::Capacity)
        );
        ui.begin(viewport());
        for _ in 0..15 {
            ui.push_clip(BOUNDS).unwrap();
        }
        assert_eq!(layout.draw(&mut ui), Err(UiError::Capacity));
        assert_eq!(ui.inspect().unwrap().len(), 0);
        for _ in 0..15 {
            ui.pop_clip();
        }
        layout.draw(&mut ui).unwrap();
        let count = ui.inspect().unwrap().len();
        assert!(
            ui.compose(|g| {
                layout.submit(g)?;
                Err::<(), _>(UiError::InvalidLayout)
            })
            .is_err()
        );
        assert_eq!(ui.inspect().unwrap().len(), count);
    });
}

#[test]
fn late_text_arena_error_restores_prior_copy_and_remaining_bytes() {
    let mut ui = Ui::new();
    ui.set_inspection_enabled(true);
    let prior = "A".repeat(32760);
    host_frame(0., 352, 700, || {
        ui.begin(viewport());
        ui.wrapped_text(
            BOUNDS,
            &prior,
            ferravex_render::ui::TextStyle::default(),
            ferravex_render::ui::text_layout::LineLimit::Clip(0),
        )
        .unwrap();
        assert_eq!(meter(known(3.)).draw(&mut ui), Err(UiError::Capacity));
        let e = ui.inspect().unwrap().next().unwrap();
        assert_eq!(e.label, UiLabel::Full(&prior));
        assert_eq!(e.painted_label, prior);
        assert_eq!(ui.inspect().unwrap().len(), 1);
        ui.text(
            BOUNDS,
            "12345678",
            ferravex_render::ui::TextStyle::default(),
        )
        .unwrap();
        assert_eq!(
            ui.text(BOUNDS, "X", ferravex_render::ui::TextStyle::default()),
            Err(UiError::Capacity)
        );
    });
}

thread_local! {
    static RECORD: Cell<bool> = const { Cell::new(false) };
    static CALLS: Cell<usize> = const { Cell::new(0) };
}
struct Allocator;
fn allocated(p: *mut u8) {
    if !p.is_null() {
        let _ = RECORD.try_with(|on| {
            if on.get() {
                let _ = CALLS.try_with(|calls| calls.set(calls.get() + 1));
            }
        });
    }
}
// SAFETY: Each operation forwards its exact allocation contract to System;
// counters use nonallocating thread-local Cells and tolerate TLS destruction.
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(l) };
        allocated(p);
        p
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        let p = unsafe { System.alloc_zeroed(l) };
        allocated(p);
        p
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        let p = unsafe { System.realloc(p, l, n) };
        allocated(p);
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
}
#[global_allocator]
static ALLOCATOR: Allocator = Allocator;

#[test]
fn warmed_layout_paint_inspection_and_caller_buffers_allocate_zero() {
    let mut ui = Ui::new();
    ui.set_inspection_enabled(true);
    let thresholds = [Threshold {
        fraction: 0.25,
        label: "REARM >25%",
    }];
    let mut output = [0u8; 8192];
    let mut markers = [BOUNDS; MAX_METER_MARKERS];
    let mut cells = [SegmentRect {
        bounds: BOUNDS,
        filled: false,
    }; MAX_METER_SEGMENTS];
    let mut cycle = || {
        host_frame(0., 352, 700, || {
            ui.begin(viewport());
            let m = MeterLayout::meter(BOUNDS, COPY, known(3.), &thresholds, theme()).unwrap();
            m.draw(&mut ui).unwrap();
            ui.finish(true);
            black_box(ui.write_inspection(&mut output).unwrap());
            m.continuous()
                .unwrap()
                .markers_into(&[0., 0.25, 1.], 2., &mut markers)
                .unwrap();
            let s = SegmentGeometry::new(
                BOUNDS,
                SegmentValue::Known {
                    filled: 2,
                    capacity: 6,
                },
                8.,
            )
            .unwrap();
            s.write_into(&mut cells).unwrap();
            ui.paint();
            ui.begin(viewport());
            let s = MeterLayout::segments(
                BOUNDS,
                COPY,
                SegmentValue::Known {
                    filled: 2,
                    capacity: 6,
                },
                theme(),
            )
            .unwrap();
            s.draw(&mut ui).unwrap();
            ui.finish(true);
            black_box(ui.write_inspection(&mut output).unwrap());
            ui.paint();
        })
    };
    cycle();
    CALLS.with(|c| c.set(0));
    RECORD.with(|c| c.set(true));
    for _ in 0..1000 {
        cycle();
    }
    RECORD.with(|c| c.set(false));
    assert_eq!(CALLS.with(Cell::get), 0);
}

#[test]
fn actual_magazine_telemetry_survives_pause_catch_up_local_overlay_and_reset() {
    use ferravex_core::session::GameSession;
    use ferravex_play::gameplay::magazine::Magazine;
    let mut session = GameSession::try_new(64., 4, true).unwrap();
    let mut magazine = Magazine::new(6, 6, 2.).unwrap();
    for _ in 0..3 {
        assert!(magazine.fire());
    }
    assert!(magazine.reload());
    assert!(!magazine.fire()); // Rejected action never changes accepted counters.
    let mut executed = 0;
    let mut completions = 0;
    let mut ui = Ui::new();
    ui.set_inspection_enabled(true);
    let mut show = |magazine: &Magazine, status: &str| {
        let telemetry = magazine.reload_status().unwrap();
        let display = format!("{:.6}s REMAINING", telemetry.remaining_seconds);
        let copy = MeterCopy {
            label: "RELOAD",
            display: &display,
            status,
        };
        let layout = MeterLayout::meter(
            BOUNDS,
            copy,
            MeterValue::Known {
                current: f64::from(telemetry.progress),
                capacity: 1.,
            },
            &[],
            theme(),
        )
        .unwrap();
        assert_eq!(layout.copy().display, display);
        let geometry = layout.continuous().unwrap();
        assert_eq!(
            geometry.value().fraction().unwrap(),
            Some(f64::from(telemetry.progress))
        );
        assert_eq!(
            geometry.fill().unwrap().size[0],
            geometry.bounds().size[0] * telemetry.progress
        );
        let loaded = format!("{} / {}", magazine.loaded(), magazine.capacity());
        let strip = MeterLayout::segments(
            BOUNDS,
            MeterCopy {
                display: &loaded,
                ..copy
            },
            SegmentValue::Known {
                filled: usize::from(magazine.loaded()),
                capacity: usize::from(magazine.capacity()),
            },
            theme(),
        )
        .unwrap();
        assert_eq!(
            strip.cells().iter().filter(|c| c.filled).count(),
            usize::from(magazine.loaded())
        );
        host_frame(0., 352, 700, || {
            ui.begin(viewport());
            layout.draw(&mut ui).unwrap();
            ui.finish(true);
            assert!(
                ui.inspect()
                    .unwrap()
                    .any(|e| e.label == UiLabel::Full(&display) && e.painted_label == display)
            );
            ui.paint();
        });
        telemetry
    };
    let initial = show(&magazine, "RELOADING");
    assert_eq!(session.advance(0.5 / 64.).steps, 0);
    assert_eq!(show(&magazine, "RELOADING"), initial);
    session.set_paused(true);
    assert_eq!(session.advance(30.).steps, 0);
    assert_eq!(show(&magazine, "PAUSED"), initial);
    let manual = session.step_paused().unwrap();
    for dt in manual.ticks() {
        executed += 1;
        completions += usize::from(magazine.advance(dt).unwrap());
    }
    assert_eq!(show(&magazine, "PAUSED").remaining_seconds, 1.984375);
    session.set_paused(false);
    // The local overlay is presentation-only: no session.set_paused here.
    let local_overlay = true;
    let catch_up = session.advance(3. / 64.);
    assert_eq!(catch_up.steps, 3);
    for dt in catch_up.ticks() {
        executed += 1;
        completions += usize::from(magazine.advance(dt).unwrap());
    }
    assert!(local_overlay);
    assert_eq!(executed, 4);
    assert_eq!(session.tick(), 4);
    assert_eq!(
        show(&magazine, "LOCAL MENU; RELOAD CONTINUES").remaining_seconds,
        1.9375
    );
    while magazine.reloading() {
        let batch = session.advance(4. / 64.);
        for dt in batch.ticks() {
            executed += 1;
            completions += usize::from(magazine.advance(dt).unwrap());
        }
    }
    assert_eq!(completions, 1);
    assert_eq!(executed, 128);
    assert_eq!(magazine.loaded(), 6);
    assert_eq!(magazine.reserve(), 3);
    assert!(!magazine.advance(1.).unwrap());
    // Reset caller-owned model/session and rebuild; no old fill or copy is cached.
    session.restart();
    magazine = Magazine::new(6, 6, 2.).unwrap();
    assert_eq!(session.tick(), 0);
    assert!(magazine.fire());
    assert!(magazine.reload());
    assert_eq!(
        show(&magazine, "RESTARTED"),
        magazine.reload_status().unwrap()
    );
}

#[test]
fn fractional_and_negative_origins_keep_every_cell_and_marker_inside() {
    for x in [-100.25, -0.1, 0.1, 16.25] {
        for width in [118.2, 119.75, 300.125] {
            let r = UiRect {
                min: [x, -1.25],
                size: [width, 8.5],
            };
            let geometry = MeterGeometry::new(r, known(3.)).unwrap();
            let mut markers = [r; 3];
            geometry
                .markers_into(&[0., 0.3, 1.], 1.5, &mut markers)
                .unwrap();
            for marker in markers {
                assert!(marker.min[0] >= x);
                assert!(marker.min[0] + marker.size[0] <= x + width);
            }
            let strip = SegmentGeometry::new(
                r,
                SegmentValue::Known {
                    filled: 6,
                    capacity: 6,
                },
                1.25,
            )
            .unwrap();
            let mut cells = [SegmentRect {
                bounds: r,
                filled: false,
            }; 6];
            strip.write_into(&mut cells).unwrap();
            assert_eq!(cells[5].bounds.min[0] + cells[5].bounds.size[0], x + width);
            for c in cells {
                assert!(c.bounds.min[0] >= x && c.bounds.min[0] + c.bounds.size[0] <= x + width);
            }
        }
    }
}
