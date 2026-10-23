//! Synthetic native journal/CPU paint recipes; no browser or physical device.
use ferravex_play::ui::theme::{ThemeMode, UiTheme};
use ferravex_render::{
    Vec2,
    input::{PlayerInput, host_frame, touch, touch_event},
    ui::{
        ButtonStyle, TextStyle, Ui, UiError, UiLabel, UiRect, Viewport, inspector::*,
        text_layout::LineLimit,
    },
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};

fn theme() -> UiTheme {
    UiTheme::preset(ThemeMode::Normal)
}
fn viewport() -> Viewport {
    Viewport::responsive([320., 600.], [0.; 4]).unwrap()
}
fn field() -> InspectorField<'static> {
    InspectorField {
        id: 1,
        label: "MASS",
        display: "1.00",
        unit: "kg",
        state: FieldState::Enabled,
        control: InspectorControl::Stepper {
            value: 1.,
            min: 0.,
            max: 10.,
            step: 0.25,
            policy: LimitPolicy::Reject,
            decrease: 2,
            increase: 3,
        },
    }
}
fn inject(id: u64, phase: touch::Phase, r: UiRect) {
    touch_event(touch::Event {
        id,
        phase,
        position: Vec2::new(r.min[0] + 2., r.min[1] + 2.),
    });
}
fn tap(r: UiRect) {
    inject(77, touch::Phase::Start, r);
    inject(77, touch::Phase::End, r);
}
fn run<E>(
    layout: &InspectorLayout<'_>,
    ui: &mut Ui,
    enabled: bool,
    validate: impl FnOnce(ValueChange) -> Result<(), E>,
) -> InspectorOutcome<E> {
    host_frame(0., 320, 600, || {
        ui.begin(viewport());
        let result = layout.finish(ui, enabled, validate).unwrap();
        ui.paint();
        result
    })
}
fn accept(_: ValueChange) -> Result<(), ()> {
    Ok(())
}
fn expected(field: u64, old: FieldValue, value: FieldValue) -> InspectorOutcome<()> {
    InspectorOutcome::Accepted(ValueChange {
        field,
        previous: old,
        value,
    })
}

#[test]
fn snapshot_validation_application_is_external_and_rejection_is_a_true_noop() {
    let fields = [field()];
    let layout = InspectorLayout::new(viewport().bounds(), &fields, theme()).unwrap();
    let mut ui = Ui::new();
    ui.set_inspection_enabled(true);
    let mut model = 1.;
    tap(layout.control_rect(3).unwrap());
    let result = run(&layout, &mut ui, true, |change| {
        assert_eq!(model, 1.);
        assert_eq!(change.value, FieldValue::Number(1.25));
        Err("domain mass rejected")
    });
    assert_eq!(
        result,
        InspectorOutcome::Invalid {
            field: 1,
            reason: InvalidEdit::Validation("domain mass rejected")
        }
    );
    assert_eq!(model, 1.);
    assert!(ui.interaction().pointer_consumed);
    tap(layout.control_rect(3).unwrap());
    let result = run(&layout, &mut ui, true, accept);
    assert_eq!(
        result,
        expected(1, FieldValue::Number(1.), FieldValue::Number(1.25))
    );
    if let InspectorOutcome::Accepted(ValueChange {
        value: FieldValue::Number(n),
        ..
    }) = result
    {
        model = n;
    }
    assert_eq!(model, 1.25);
    // No held-state/repeated-render change. Model and formatting rebuild together.
    let fields = [InspectorField {
        display: "1.25",
        control: InspectorControl::Stepper {
            value: model,
            min: 0.,
            max: 10.,
            step: 0.25,
            policy: LimitPolicy::Reject,
            decrease: 2,
            increase: 3,
        },
        ..field()
    }];
    let rebuilt = InspectorLayout::new(viewport().bounds(), &fields, theme()).unwrap();
    assert_eq!(run(&rebuilt, &mut ui, true, accept), InspectorOutcome::Idle);
    for id in [2, 3] {
        let e = ui.inspect().unwrap().find(|e| e.id == Some(id)).unwrap();
        assert_eq!(e.bounds, rebuilt.control_rect(id).unwrap());
        assert_eq!(e.visible_bounds, Some(e.bounds));
        assert!(e.enabled && e.interaction_registered);
        assert!(matches!(e.label, UiLabel::Full(_)));
        assert_eq!(e.bounds.size[1], 44.);
    }
    assert!(ui.inspect().unwrap().any(|e| e.painted_label == "1.25 kg"));
}

#[test]
fn real_press_hold_release_cancel_removed_capture_and_global_disable() {
    let fields = [field()];
    let layout = InspectorLayout::new(viewport().bounds(), &fields, theme()).unwrap();
    let r = layout.control_rect(3).unwrap();
    let mut ui = Ui::new();
    inject(8, touch::Phase::Start, r);
    assert_eq!(run(&layout, &mut ui, true, accept), InspectorOutcome::Idle);
    assert_eq!(ui.interaction().pointer_capture, Some(3));
    assert_eq!(run(&layout, &mut ui, true, accept), InspectorOutcome::Idle);
    inject(8, touch::Phase::End, r);
    assert!(matches!(
        run(&layout, &mut ui, true, accept),
        InspectorOutcome::Accepted(_)
    ));
    assert_eq!(ui.interaction().pointer_capture, None);
    inject(9, touch::Phase::Start, r);
    run(&layout, &mut ui, true, accept);
    inject(9, touch::Phase::Cancel, r);
    assert_eq!(run(&layout, &mut ui, true, accept), InspectorOutcome::Idle);
    inject(10, touch::Phase::Start, r);
    run(&layout, &mut ui, true, accept);
    let empty = InspectorLayout::new(viewport().bounds(), &[], theme()).unwrap();
    inject(10, touch::Phase::End, r);
    assert_eq!(run(&empty, &mut ui, true, accept), InspectorOutcome::Idle);
    assert!(ui.interaction().pointer_consumed);
    tap(r);
    assert_eq!(
        run(&layout, &mut ui, false, |_| -> Result<(), ()> {
            panic!("disabled validator")
        }),
        InspectorOutcome::Idle
    );
    inject(11, touch::Phase::Start, r);
    run(&layout, &mut ui, true, accept);
    ui.cancel();
    inject(11, touch::Phase::End, r);
    assert_eq!(run(&layout, &mut ui, true, accept), InspectorOutcome::Idle);
}

#[test]
fn explicit_field_states_choice_disabled_selected_and_toggle_edges() {
    let mut ui = Ui::new();
    ui.set_inspection_enabled(true);
    for (state, prefix) in [
        (FieldState::Disabled("locked"), "X DISABLED:"),
        (FieldState::Unavailable("body removed"), "X UNAVAILABLE:"),
        (FieldState::Invalid("friction relation"), "! INVALID:"),
    ] {
        let fields = [InspectorField { state, ..field() }];
        let layout = InspectorLayout::new(viewport().bounds(), &fields, theme()).unwrap();
        tap(layout.control_rect(3).unwrap());
        assert_eq!(
            run(&layout, &mut ui, true, |_| -> Result<(), ()> {
                panic!("unavailable validator")
            }),
            InspectorOutcome::Idle
        );
        assert!(ui.interaction().pointer_consumed);
        assert!(
            ui.inspect()
                .unwrap()
                .any(|e| e.painted_label.starts_with(prefix))
        );
    }
    let choices = [
        InspectorChoice {
            id: 12,
            label: "BALL",
            enabled: true,
        },
        InspectorChoice {
            id: 13,
            label: "CRATE",
            enabled: false,
        },
        InspectorChoice {
            id: 14,
            label: "COMPOUND",
            enabled: true,
        },
    ];
    let fields = [InspectorField {
        id: 10,
        label: "SHAPE",
        display: "BALL",
        unit: "",
        state: FieldState::Enabled,
        control: InspectorControl::Choice {
            selected: 0,
            choices: &choices,
        },
    }];
    let layout = InspectorLayout::new(viewport().bounds(), &fields, theme()).unwrap();
    tap(layout.control_rect(13).unwrap());
    assert_eq!(run(&layout, &mut ui, true, accept), InspectorOutcome::Idle);
    tap(layout.control_rect(12).unwrap());
    assert_eq!(run(&layout, &mut ui, true, accept), InspectorOutcome::Idle);
    tap(layout.control_rect(14).unwrap());
    assert_eq!(
        run(&layout, &mut ui, true, accept),
        expected(10, FieldValue::Choice(0), FieldValue::Choice(2))
    );
    assert_eq!(run(&layout, &mut ui, true, accept), InspectorOutcome::Idle);
    assert!(ui.inspect().unwrap().any(|e| e.painted_label == "[X] BALL"));
    let fields = [InspectorField {
        id: 20,
        label: "SLEEP",
        display: "ON",
        unit: "",
        state: FieldState::Enabled,
        control: InspectorControl::Toggle {
            value: true,
            id: 21,
            on: "ON",
            off: "OFF",
        },
    }];
    let layout = InspectorLayout::new(viewport().bounds(), &fields, theme()).unwrap();
    tap(layout.control_rect(21).unwrap());
    assert_eq!(
        run(&layout, &mut ui, true, accept),
        expected(20, FieldValue::Toggle(true), FieldValue::Toggle(false))
    );
    assert_eq!(run(&layout, &mut ui, true, accept), InspectorOutcome::Idle);
}

#[test]
fn numeric_limits_explicit_clamp_overflow_and_rounded_step() {
    for (value, min, max, step, policy, increase, result) in [
        (
            1.,
            0.,
            1.,
            0.25,
            LimitPolicy::Reject,
            true,
            Some(InvalidEdit::NumericLimit),
        ),
        (
            0.,
            0.,
            1.,
            0.25,
            LimitPolicy::Reject,
            false,
            Some(InvalidEdit::NumericLimit),
        ),
        (
            f32::MAX,
            0.,
            f32::MAX,
            f32::MAX,
            LimitPolicy::Clamp,
            true,
            Some(InvalidEdit::NumericLimit),
        ),
        (
            1e30,
            0.,
            f32::MAX,
            0.25,
            LimitPolicy::Reject,
            true,
            Some(InvalidEdit::NoNumericProgress),
        ),
        (1., 0., 1., 0.25, LimitPolicy::Clamp, true, None),
    ] {
        let fields = [InspectorField {
            control: InspectorControl::Stepper {
                value,
                min,
                max,
                step,
                policy,
                decrease: 2,
                increase: 3,
            },
            ..field()
        }];
        let layout = InspectorLayout::new(viewport().bounds(), &fields, theme()).unwrap();
        tap(layout.control_rect(if increase { 3 } else { 2 }).unwrap());
        let mut ui = Ui::new();
        let outcome = run(&layout, &mut ui, true, |_| -> Result<(), ()> {
            panic!("invalid numeric validator")
        });
        assert_eq!(
            outcome,
            result.map_or(InspectorOutcome::Idle, |reason| InspectorOutcome::Invalid {
                field: 1,
                reason
            })
        );
    }
    let fields = [InspectorField {
        control: InspectorControl::Stepper {
            value: 0.9,
            min: 0.,
            max: 1.,
            step: 0.25,
            policy: LimitPolicy::Clamp,
            decrease: 2,
            increase: 3,
        },
        ..field()
    }];
    let layout = InspectorLayout::new(viewport().bounds(), &fields, theme()).unwrap();
    tap(layout.control_rect(3).unwrap());
    assert_eq!(
        run(&layout, &mut Ui::new(), true, accept),
        expected(1, FieldValue::Number(0.9), FieldValue::Number(1.))
    );
}

#[test]
fn source_geometry_capacity_and_invalid_specs_are_checked_before_submission() {
    let bounds = viewport().bounds();
    for control in [
        InspectorControl::Stepper {
            value: f32::NAN,
            min: 0.,
            max: 1.,
            step: 1.,
            policy: LimitPolicy::Reject,
            decrease: 2,
            increase: 3,
        },
        InspectorControl::Stepper {
            value: 1.,
            min: 2.,
            max: 1.,
            step: 1.,
            policy: LimitPolicy::Reject,
            decrease: 2,
            increase: 3,
        },
        InspectorControl::Stepper {
            value: 1.,
            min: 0.,
            max: 1.,
            step: 0.,
            policy: LimitPolicy::Reject,
            decrease: 2,
            increase: 3,
        },
        InspectorControl::Stepper {
            value: 1.,
            min: 0.,
            max: f32::INFINITY,
            step: 1.,
            policy: LimitPolicy::Reject,
            decrease: 2,
            increase: 3,
        },
        InspectorControl::Toggle {
            value: false,
            id: 2,
            on: "",
            off: "OFF",
        },
        InspectorControl::Toggle {
            value: false,
            id: 2,
            on: "LINE\nBREAK",
            off: "OFF",
        },
        InspectorControl::Choice {
            selected: 0,
            choices: &[],
        },
    ] {
        assert_eq!(
            InspectorLayout::new(bounds, &[InspectorField { control, ..field() }], theme())
                .unwrap_err(),
            UiError::InvalidLayout
        );
    }
    let duplicate = [InspectorField {
        control: InspectorControl::Stepper {
            value: 1.,
            min: 0.,
            max: 1.,
            step: 1.,
            policy: LimitPolicy::Reject,
            decrease: 1,
            increase: 3,
        },
        ..field()
    }];
    assert_eq!(
        InspectorLayout::new(bounds, &duplicate, theme()).unwrap_err(),
        UiError::DuplicateId
    );
    assert_eq!(
        InspectorLayout::new(bounds, &[field(); MAX_INSPECTOR_FIELDS + 1], theme()).unwrap_err(),
        UiError::Capacity
    );
    let choices = [InspectorChoice {
        id: 3,
        label: "BALL",
        enabled: true,
    }; MAX_INSPECTOR_CHOICES + 1];
    assert_eq!(
        InspectorLayout::new(
            bounds,
            &[InspectorField {
                control: InspectorControl::Choice {
                    selected: 0,
                    choices: &choices
                },
                ..field()
            }],
            theme()
        )
        .unwrap_err(),
        UiError::Capacity
    );
    let huge = "x".repeat(MAX_INSPECTOR_BYTES);
    assert_eq!(
        InspectorLayout::new(
            bounds,
            &[InspectorField {
                label: &huge,
                ..field()
            }],
            theme()
        )
        .unwrap_err(),
        UiError::Capacity
    );
    for size in [
        [320., 50.],
        [50., 600.],
        [f32::NAN, 600.],
        [f32::INFINITY, 600.],
    ] {
        assert_eq!(
            InspectorLayout::new(UiRect { size, ..bounds }, &[field()], theme()).unwrap_err(),
            UiError::InvalidLayout
        );
    }
    assert_eq!(
        InspectorLayout::new(
            UiRect {
                min: [1e30; 2],
                ..bounds
            },
            &[field()],
            theme()
        )
        .unwrap_err(),
        UiError::InvalidLayout
    );
    let mut ui = Ui::new();
    ui.set_inspection_enabled(true);
    ui.begin(viewport());
    assert!(ui.inspect().unwrap().next().is_none());
    let fields: [_; MAX_INSPECTOR_FIELDS] = std::array::from_fn(|i| InspectorField {
        id: 100 + i as u64 * 3,
        control: InspectorControl::Stepper {
            value: 1.,
            min: 0.,
            max: 2.,
            step: 0.1,
            policy: LimitPolicy::Reject,
            decrease: 101 + i as u64 * 3,
            increase: 102 + i as u64 * 3,
        },
        ..field()
    });
    let tall = UiRect {
        min: [0.; 2],
        size: [320., 6000.],
    };
    assert!(InspectorLayout::new(tall, &fields, theme()).is_ok());
}

#[test]
fn command_text_vertex_clip_and_duplicate_failures_rollback_before_validation() {
    let fields = [field()];
    let layout = InspectorLayout::new(viewport().bounds(), &fields, theme()).unwrap();
    let mut ui = Ui::new();
    ui.set_inspection_enabled(true);
    for failure in 0..5 {
        host_frame(0., 320, 600, || {
            ui.begin(viewport());
            match failure {
                0 => {
                    for _ in 0..1020 {
                        ui.label(viewport().bounds(), "", TextStyle::default())
                            .unwrap();
                    }
                }
                1 => {
                    ui.wrapped_text(
                        viewport().bounds(),
                        &"x".repeat(32760),
                        TextStyle::default(),
                        LineLimit::Clip(0),
                    )
                    .unwrap();
                }
                2 => {
                    ui.text(viewport().bounds(), &"x".repeat(1560), TextStyle::default())
                        .unwrap();
                }
                3 => {
                    for _ in 0..15 {
                        ui.push_clip(viewport().bounds()).unwrap();
                    }
                }
                _ => ui
                    .button(
                        3,
                        viewport().bounds(),
                        "PRIOR",
                        true,
                        ButtonStyle::default(),
                    )
                    .unwrap(),
            }
            let before: Vec<_> = ui
                .inspect()
                .unwrap()
                .map(|e| (e.id, e.painted_label.to_owned(), e.bounds))
                .collect();
            let result = layout.finish(&mut ui, true, |_| -> Result<(), ()> {
                panic!("validation before atomic submit")
            });
            assert_eq!(
                result,
                Err(if failure == 4 {
                    UiError::DuplicateId
                } else {
                    UiError::Capacity
                })
            );
            let after: Vec<_> = ui
                .inspect()
                .unwrap()
                .map(|e| (e.id, e.painted_label.to_owned(), e.bounds))
                .collect();
            assert_eq!(before, after);
            ui.finish(true);
        });
    }
    // Exact restoration allows an explicit readable fallback.
    host_frame(0., 320, 600, || {
        ui.begin(viewport());
        ui.button(
            3,
            viewport().bounds(),
            "PRIOR",
            true,
            ButtonStyle::default(),
        )
        .unwrap();
        assert!(layout.finish(&mut ui, true, accept).is_err());
        ui.text(
            UiRect {
                min: [16., 16.],
                size: [288., 44.],
            },
            "USE LARGER INSPECTOR",
            TextStyle::default(),
        )
        .unwrap();
        ui.finish(true);
        ui.paint();
        assert_eq!(ui.inspect().unwrap().count(), 2);
    });
}

#[test]
fn theme_safe_area_wrapping_and_remapped_finish_share_measured_geometry() {
    let mut ui = Ui::new();
    ui.set_inspection_enabled(true);
    let mut player = PlayerInput::transient("journey400-285");
    for mode in [ThemeMode::Normal, ThemeMode::HighContrast] {
        for width in [320., 960.] {
            let viewport = Viewport::responsive([width, 900.], [8., 12., 8., 12.]).unwrap();
            let mut theme = UiTheme::preset(mode);
            let types = theme.types().try_scaled(1.5).unwrap();
            let mut spacing = theme.spacing();
            spacing.control_height = 64.;
            theme.set_metrics(types, spacing).unwrap();
            let fields = [InspectorField {
                label: "A complete wrapped mass label é日\nwith units",
                state: FieldState::Invalid(
                    "Static friction must exceed sliding friction. Full message.",
                ),
                ..field()
            }];
            let layout = InspectorLayout::new(viewport.bounds(), &fields, theme).unwrap();
            host_frame(0., width as u32, 900, || {
                player.sample(true);
                ui.begin(viewport);
                assert_eq!(
                    layout
                        .finish_player(&mut ui, &mut player, true, accept)
                        .unwrap(),
                    InspectorOutcome::Idle
                );
                for id in [2, 3] {
                    let e = ui.inspect().unwrap().find(|e| e.id == Some(id)).unwrap();
                    assert_eq!(e.bounds, layout.control_rect(id).unwrap());
                    assert_eq!(e.visible_bounds, Some(e.bounds));
                    assert_eq!(e.bounds.size[1], 64.);
                }
                assert!(layout.field_rect(1).unwrap().size[1] > 100.);
                assert_eq!(layout.field_rect(999), None);
                assert_eq!(layout.control_rect(999), None);
                ui.paint();
            });
        }
    }
}

struct Counting;
thread_local! {static TRACK:Cell<bool>=const{Cell::new(false)};static ALLOCS:Cell<usize>=const{Cell::new(0)};}
fn count() {
    TRACK.with(|t| {
        if t.get() {
            ALLOCS.with(|c| c.set(c.get() + 1));
        }
    });
}
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        count();
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        count();
        unsafe { System.realloc(p, l, n) }
    }
}
#[global_allocator]
static ALLOCATOR: Counting = Counting;

#[test]
fn warmed_borrowed_layout_finish_paint_and_rejection_allocate_no_storage() {
    let mut ui = Ui::new();
    let fields = [field()];
    for inspection in [false, true] {
        ui.set_inspection_enabled(inspection);
        let reserved = ui.reserved_bytes();
        for frame in 0..306 {
            ALLOCS.with(|c| c.set(0));
            TRACK.with(|t| t.set(frame >= 6));
            host_frame(0., 320, 600, || {
                ui.begin(viewport());
                let layout = InspectorLayout::new(viewport().bounds(), &fields, theme()).unwrap();
                if frame % 2 == 0 {
                    tap(layout.control_rect(3).unwrap());
                }
                let _ = layout
                    .finish(&mut ui, true, |_| Err("caller rejects"))
                    .unwrap();
                if inspection {
                    let mut output = [0; 8192];
                    assert!(ui.write_inspection(&mut output).is_ok());
                }
                ui.paint();
            });
            TRACK.with(|t| t.set(false));
            assert_eq!(
                ALLOCS.with(Cell::get),
                0,
                "frame {frame}, inspection {inspection}"
            );
            assert_eq!(ui.reserved_bytes(), reserved);
        }
    }
}

#[test]
fn exact_byte_choice_bounds_and_fractional_safe_projection_keep_complete_copy() {
    // Source bytes are counted before prefixes or any text measurement.
    let mut label = "x".repeat(MAX_INSPECTOR_BYTES - 6); // display=4, unit=2
    let large = UiRect {
        min: [0.; 2],
        size: [100000.; 2],
    };
    {
        let fields = [InspectorField {
            label: &label,
            ..field()
        }];
        assert!(InspectorLayout::new(large, &fields, theme()).is_ok());
    }
    label.push('x');
    assert_eq!(
        InspectorLayout::new(
            large,
            &[InspectorField {
                label: &label,
                ..field()
            }],
            theme()
        )
        .unwrap_err(),
        UiError::Capacity
    );
    // Missing snapshots have an explicit unavailable outcome, never a NaN proposal.
    let fields = [InspectorField {
        display: "N/A",
        state: FieldState::Unavailable("body removed"),
        control: InspectorControl::Stepper {
            value: f32::NAN,
            min: 0.,
            max: 1.,
            step: 0.25,
            policy: LimitPolicy::Reject,
            decrease: 2,
            increase: 3,
        },
        ..field()
    }];
    assert!(InspectorLayout::new(viewport().bounds(), &fields, theme()).is_ok());
    let choices: [_; MAX_INSPECTOR_CHOICES] = std::array::from_fn(|i| InspectorChoice {
        id: 10 + i as u64,
        label: "OPTION é日",
        enabled: true,
    });
    let fields = [InspectorField {
        control: InspectorControl::Choice {
            selected: 0,
            choices: &choices,
        },
        ..field()
    }];
    let viewport = Viewport::responsive([352., 700.], [8., 12., 16., 20.]).unwrap();
    let bounds = UiRect {
        min: [0.25, 0.125],
        size: [
            viewport.bounds().size[0] - 0.5,
            viewport.bounds().size[1] - 0.25,
        ],
    };
    let layout = InspectorLayout::new(bounds, &fields, theme()).unwrap();
    let rect = layout.control_rect(17).unwrap();
    let projected = UiRect {
        min: std::array::from_fn(|a| viewport.pixels().min[a] + rect.min[a] * viewport.scale()),
        size: rect.size,
    };
    tap(projected);
    let mut ui = Ui::new();
    ui.set_inspection_enabled(true);
    host_frame(0., 352, 700, || {
        ui.begin(viewport);
        assert_eq!(
            layout.finish(&mut ui, true, accept).unwrap(),
            expected(1, FieldValue::Choice(0), FieldValue::Choice(7))
        );
        for i in 0..MAX_INSPECTOR_CHOICES {
            let e = ui
                .inspect()
                .unwrap()
                .find(|e| e.id == Some(10 + i as u64))
                .unwrap();
            assert_eq!(e.bounds, layout.control_rect(10 + i as u64).unwrap());
            assert_eq!(e.visible_bounds, Some(e.bounds));
            assert_eq!(
                e.label,
                UiLabel::Full(if i == 0 {
                    "[X] OPTION é日"
                } else {
                    "[ ] OPTION é日"
                })
            );
        }
        ui.paint();
    });
}
