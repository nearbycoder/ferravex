use ferravex_play::ui::flow::{
    FLOW_ITEM_CAPACITY, FlowError, FlowItem, FlowOptions, FlowReport, layout,
};
use ferravex_play::ui::{FocusDirection, FocusWrap, HudEdge, HudRegions, Rect, UiState, Widget};

const EMPTY: Rect = Rect {
    min: [0.; 2],
    size: [0.; 2],
};

fn area(width: f32, height: f32) -> Rect {
    Rect {
        size: [width, height],
        ..EMPTY
    }
}

fn item(minimum: f32, preferred: f32, maximum: f32, grow: f32) -> FlowItem {
    FlowItem {
        minimum,
        preferred,
        maximum,
        grow,
    }
}

fn inside(parent: Rect, rects: &[Rect], items: &[FlowItem]) {
    for (i, (&rect, item)) in rects.iter().zip(items).enumerate() {
        assert!(rect.valid(), "{rect:?}");
        assert!(rect.size[0] >= item.minimum && rect.size[0] <= item.maximum);
        for axis in 0..2 {
            assert!(rect.min[axis] >= parent.min[axis]);
            assert!(rect.min[axis] + rect.size[axis] <= parent.min[axis] + parent.size[axis]);
        }
        for other in &rects[..i] {
            assert!(other.intersect(rect).is_none(), "{rect:?} {other:?}");
        }
    }
}

#[test]
fn exact_width_and_height_boundaries_and_one_ulp_short() {
    let items = [FlowItem::fixed(44.); 3];
    let options = FlowOptions::default();
    let mut rects = [EMPTY; 3];
    let report = layout(area(96., 96.), &items, options, &mut rects).unwrap();
    assert_eq!(
        report,
        FlowReport {
            written: 3,
            overflow: 0,
            rows: 2
        }
    );
    assert_eq!(rects[0], area(44., 44.));
    assert_eq!(rects[1].min, [52., 0.]);
    assert_eq!(rects[2].min, [0., 52.]);
    let report = layout(area(96_f32.next_down(), 96.), &items, options, &mut rects).unwrap();
    assert_eq!(
        report,
        FlowReport {
            written: 2,
            overflow: 1,
            rows: 2
        }
    );
    assert_eq!(rects[1].min, [0., 52.]);
    let report = layout(area(96., 96_f32.next_down()), &items, options, &mut rects).unwrap();
    assert_eq!(
        report,
        FlowReport {
            written: 2,
            overflow: 1,
            rows: 1
        }
    );
    let report = layout(area(96., 44_f32.next_down()), &items, options, &mut rects).unwrap();
    assert_eq!(
        report,
        FlowReport {
            written: 0,
            overflow: 3,
            rows: 0
        }
    );
}

#[test]
fn fixed_icon_and_flexible_label_preserve_distinct_minima() {
    let items = [
        FlowItem::fixed(44.),
        item(100., 180., 300., 1.),
        FlowItem::fixed(64.),
    ];
    let mut rects = [EMPTY; 3];
    for (width, expected) in [
        (224., 100.),
        (260., 136.),
        (304., 180.),
        (320., 196.),
        (500., 300.),
    ] {
        let parent = area(width, 44.);
        let report = layout(parent, &items, FlowOptions::default(), &mut rects).unwrap();
        assert_eq!(report.written, 3);
        assert_eq!(rects.map(|r| r.size[0]), [44., expected, 64.]);
        inside(parent, &rects, &items);
    }
}

#[test]
fn preferred_deficits_share_compression_independently_of_grow_weights() {
    let items = [item(20., 60., 100., 0.), item(40., 120., 200., 100.)];
    let mut rects = [EMPTY; 2];
    let options = FlowOptions {
        gap: [0.; 2],
        ..FlowOptions::default()
    };
    layout(area(120., 44.), &items, options, &mut rects).unwrap();
    assert_eq!(rects.map(|r| r.size[0]), [40., 80.]);
    layout(area(240., 44.), &items, options, &mut rects).unwrap();
    assert_eq!(rects.map(|r| r.size[0]), [60., 180.]);
}

#[test]
fn weighted_growth_redistributes_caps_and_leaves_unclaimed_slack() {
    let items = [
        item(10., 20., 30., 1.),
        item(10., 20., 200., 3.),
        item(10., 20., 80., 0.),
    ];
    let mut rects = [EMPTY; 3];
    let options = FlowOptions {
        gap: [0.; 2],
        ..FlowOptions::default()
    };
    for (width, expected) in [
        (80., [25., 35., 20.]),
        (160., [30., 110., 20.]),
        (400., [30., 200., 20.]),
    ] {
        layout(area(width, 44.), &items, options, &mut rects).unwrap();
        assert_eq!(rects.map(|r| r.size[0]), expected);
    }
    let many_caps = [
        item(0., 0., 10., 1.),
        item(0., 0., 30., 1.),
        item(0., 0., 200., 1.),
    ];
    layout(area(150., 44.), &many_caps, options, &mut rects).unwrap();
    assert_eq!(rects.map(|r| r.size[0]), [10., 30., 110.]);
}

#[test]
fn last_row_grows_independently_and_no_wrap_keeps_only_first_row() {
    let items = [item(60., 80., 300., 1.); 3];
    let mut rects = [EMPTY; 3];
    let options = FlowOptions {
        gap: [10., 8.],
        ..FlowOptions::default()
    };
    let report = layout(area(150., 96.), &items, options, &mut rects).unwrap();
    assert_eq!(report.rows, 2);
    assert_eq!(rects.map(|r| r.size[0]), [70., 70., 150.]);
    let report = layout(
        area(150., 96.),
        &items,
        FlowOptions {
            wrap: false,
            ..options
        },
        &mut rects,
    )
    .unwrap();
    assert_eq!(
        report,
        FlowReport {
            written: 2,
            overflow: 1,
            rows: 1
        }
    );
    assert_eq!(rects[2].size[0], 150.); // Unwritten tail is intentionally untouched.
}

#[test]
fn oversized_item_stops_prefix_without_skipping_or_squeezing() {
    let items = [
        FlowItem::fixed(40.),
        FlowItem::fixed(101.),
        FlowItem::fixed(10.),
    ];
    let mut rects = [EMPTY; 3];
    let report = layout(area(100., 400.), &items, FlowOptions::default(), &mut rects).unwrap();
    assert_eq!(
        report,
        FlowReport {
            written: 1,
            overflow: 2,
            rows: 1
        }
    );
    assert_eq!(rects[0].size, [40., 44.]);
    let report = layout(
        area(100., 400.),
        &items[1..],
        FlowOptions::default(),
        &mut rects,
    )
    .unwrap();
    assert_eq!(
        report,
        FlowReport {
            written: 0,
            overflow: 2,
            rows: 0
        }
    );
}

#[test]
fn huge_gaps_only_apply_between_items_and_rows() {
    let items = [FlowItem::fixed(40.); 3];
    let mut rects = [EMPTY; 3];
    let mut options = FlowOptions {
        gap: [f32::MAX, 8.],
        ..FlowOptions::default()
    };
    let report = layout(area(100., 148.), &items, options, &mut rects).unwrap();
    assert_eq!(
        report,
        FlowReport {
            written: 3,
            overflow: 0,
            rows: 3
        }
    );
    assert_eq!(rects.map(|r| r.min[1]), [0., 52., 104.]);
    options.gap[1] = f32::MAX;
    let report = layout(area(100., 148.), &items, options, &mut rects).unwrap();
    assert_eq!(
        report,
        FlowReport {
            written: 1,
            overflow: 2,
            rows: 1
        }
    );
    let report = layout(area(40., 44.), &items[..1], options, &mut rects).unwrap();
    assert_eq!(report.overflow, 0);
}

#[test]
fn output_capacity_preserves_full_layout_and_only_changes_written_prefix() {
    let items = [item(30., 50., 200., 1.); 5];
    let parent = area(150., 148.);
    let options = FlowOptions::default();
    let mut full = [EMPTY; 5];
    layout(parent, &items, options, &mut full).unwrap();
    for capacity in 0..=5 {
        let mut output = [parent; 6];
        let report = layout(parent, &items, options, &mut output[..capacity]).unwrap();
        assert_eq!(report.written, capacity);
        assert_eq!(report.overflow, 5 - capacity);
        assert_eq!(report.rows, capacity.div_ceil(4));
        assert_eq!(output[..capacity], full[..capacity]);
        assert!(output[capacity..].iter().all(|&r| r == parent));
    }
}

#[test]
fn capacity_limit_and_stateless_reuse_leave_no_visible_stale_tail() {
    let items = [FlowItem::fixed(1.); FLOW_ITEM_CAPACITY + 1];
    let parent = area(256., 1.);
    let mut output = [EMPTY; FLOW_ITEM_CAPACITY];
    let address = output.as_ptr();
    let options = FlowOptions {
        row_height: 1.,
        gap: [0.; 2],
        wrap: true,
    };
    for _ in 0..32 {
        let report = layout(parent, &items[..FLOW_ITEM_CAPACITY], options, &mut output).unwrap();
        assert_eq!(report.written, FLOW_ITEM_CAPACITY);
        assert_eq!(output[255].min, [255., 0.]);
        let before = output;
        assert_eq!(
            layout(parent, &items, options, &mut output),
            Err(FlowError::Capacity)
        );
        assert_eq!(output, before);
        let report = layout(parent, &[], options, &mut output).unwrap();
        assert_eq!(report, FlowReport::default());
        assert_eq!(output, before);
        assert_eq!(output.as_ptr(), address);
    }
}

#[test]
fn invalid_input_including_hidden_suffix_is_atomic_and_indexed() {
    let parent = area(100., 44.);
    let options = FlowOptions::default();
    let mut output = [parent; 2];
    for bad in [-1., f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        for invalid in [
            item(bad, 40., 80., 1.),
            item(20., bad, 80., 1.),
            item(20., 40., bad, 1.),
            item(20., 40., 80., bad),
            item(40., 20., 80., 1.),
            item(20., 80., 40., 1.),
        ] {
            let items = [FlowItem::fixed(101.), invalid];
            assert_eq!(
                layout(parent, &items, options, &mut output),
                Err(FlowError::InvalidItem { index: 1 })
            );
            assert_eq!(output, [parent; 2]);
        }
        for invalid in [
            FlowOptions {
                row_height: bad,
                ..options
            },
            FlowOptions {
                gap: [bad, 0.],
                ..options
            },
            FlowOptions {
                gap: [0., bad],
                ..options
            },
            FlowOptions {
                row_height: 0.,
                ..options
            },
        ] {
            assert_eq!(
                layout(parent, &[], invalid, &mut output),
                Err(FlowError::InvalidLayout)
            );
            assert_eq!(output, [parent; 2]);
        }
    }
    for invalid in [
        Rect {
            min: [f32::NAN, 0.],
            ..parent
        },
        area(-1., 10.),
        Rect {
            min: [f32::MAX; 2],
            size: [f32::MAX; 2],
        },
    ] {
        assert_eq!(
            layout(invalid, &[], options, &mut output),
            Err(FlowError::InvalidLayout)
        );
    }
}

#[test]
fn empty_parents_and_zero_width_items_remain_inert() {
    let items = [FlowItem::fixed(0.); 3];
    let mut rects = [EMPTY; 3];
    for parent in [area(0., 100.), area(100., 0.), EMPTY] {
        let report = layout(parent, &items, FlowOptions::default(), &mut rects).unwrap();
        assert_eq!(
            report,
            FlowReport {
                written: 0,
                overflow: 3,
                rows: 0
            }
        );
    }
    let report = layout(area(16., 44.), &items, FlowOptions::default(), &mut rects).unwrap();
    assert_eq!(report.written, 3);
    assert_eq!(rects.map(|r| r.min[0]), [0., 8., 16.]);
    assert!(rects.iter().all(|r| !r.contains(r.min)));
}

#[test]
fn huge_finite_values_do_not_overflow_weight_or_extent_arithmetic() {
    let items = [item(1., f32::MAX, f32::MAX, f32::MAX); 3];
    let parent = area(f32::MAX, 44.);
    let mut output = [EMPTY; 3];
    layout(parent, &items, FlowOptions::default(), &mut output).unwrap();
    inside(parent, &output, &items);
    let items = [item(0., 0., f32::MAX, f32::MAX); 3];
    layout(parent, &items, FlowOptions::default(), &mut output).unwrap();
    inside(parent, &output, &items);
    let items = [
        item(0., 0., 100., f32::from_bits(1)),
        item(0., 0., 100., f32::MAX),
    ];
    layout(area(80., 44.), &items, FlowOptions::default(), &mut output).unwrap();
    inside(area(80., 44.), &output[..2], &items);
    assert!(output[1].size[0] > 71.9);
}

#[test]
fn lost_positive_edges_reject_the_entire_call() {
    let mut output = [area(1., 1.); 3];
    let before = output;
    let parent = Rect {
        min: [1e20, 0.],
        size: [1e20, 100.],
    };
    assert_eq!(
        layout(
            parent,
            &[FlowItem::fixed(1.)],
            FlowOptions::default(),
            &mut output
        ),
        Err(FlowError::Unrepresentable)
    );
    let parent = Rect {
        min: [0., 1e20],
        size: [100., 1e20],
    };
    assert_eq!(
        layout(
            parent,
            &[FlowItem::fixed(1.)],
            FlowOptions::default(),
            &mut output
        ),
        Err(FlowError::Unrepresentable)
    );
    assert_eq!(output, before);

    // First row is representable; the second crosses f32's integer precision
    // boundary. Even that late failure must not publish the earlier row.
    let parent = Rect {
        min: [0., 16_777_215.],
        size: [1., 16.],
    };
    let options = FlowOptions {
        row_height: 1.,
        gap: [0.; 2],
        wrap: true,
    };
    assert_eq!(
        layout(parent, &[FlowItem::fixed(1.); 2], options, &mut output),
        Err(FlowError::Unrepresentable)
    );
    assert_eq!(output, before);
}

#[test]
fn fractional_geometry_is_stable_contained_and_has_no_overlapping_hits() {
    let items = [
        item(20.1, 44.3, 160.7, 1.),
        item(44.2, 70.9, 200.1, 3.),
        FlowItem::fixed(36.5),
        item(50.7, 108.9, 180.3, 2.),
    ];
    let options = FlowOptions {
        row_height: 44.25,
        gap: [7.3, 8.1],
        wrap: true,
    };
    for origin in [[0.1, 3.7], [-71.3, -48.9], [120.8, 33.1]] {
        for width in [145.3, 210.1, 319.7, 390.25, 2559.9] {
            let parent = Rect {
                min: origin,
                size: [width, 180.],
            };
            let mut a = [EMPTY; 4];
            let mut b = [EMPTY; 4];
            let first = layout(parent, &items, options, &mut a).unwrap();
            let second = layout(parent, &items, options, &mut b).unwrap();
            assert_eq!(first, second);
            assert_eq!(a, b);
            inside(parent, &a[..first.written], &items);
            for pair in a[..first.written].windows(2) {
                if pair[0].min[1] == pair[1].min[1] {
                    assert!(
                        pair[1].min[0] as f64
                            >= (pair[0].min[0] + pair[0].size[0]) as f64 + options.gap[0] as f64
                    );
                } else {
                    assert!(
                        pair[1].min[1] as f64
                            >= (pair[0].min[1] + pair[0].size[1]) as f64 + options.gap[1] as f64
                    );
                }
            }
        }
    }
}

#[test]
fn absolute_hud_rectangles_drive_pointer_focus_and_resize_removal() {
    let items = [
        FlowItem::fixed(44.),
        item(100., 180., 300., 1.),
        FlowItem::fixed(64.),
    ];
    let mut ui = UiState::with_capacity(3);
    let mut rects = [EMPTY; 3];
    let mut rebuild = |ui: &mut UiState, screen| {
        let mut hud = HudRegions::new(screen, [8., 24., 8., 16.], [0.; 2]).unwrap();
        let actions = hud.reserve("actions", HudEdge::Top, 96.).unwrap();
        let report = layout(actions, &items, FlowOptions::default(), &mut rects).unwrap();
        ui.begin_frame();
        for (index, &rect) in rects[..report.written].iter().enumerate() {
            ui.add(Widget {
                id: index as u64 + 1,
                rect,
                clip: Some(actions),
                enabled: true,
                focusable: true,
            })
            .unwrap();
        }
        ui.end_frame();
        (report, rects)
    };
    let (report, visible) = rebuild(&mut ui, [336., 240.]);
    assert_eq!(report.rows, 1);
    assert_eq!(visible[0].min, [8., 24.]);
    assert_eq!(ui.hit_test([8., 24.]), Some(1));
    assert_eq!(ui.hit_test([52., 30.]), None); // Half-open icon end and gap.
    assert_eq!(ui.focus_next(false), Some(1));
    assert_eq!(
        ui.focus_direction(FocusDirection::Right, FocusWrap::Clamp),
        Some(2)
    );
    assert_eq!(ui.pointer_down([270., 30.]), Some(3));
    let (report, visible) = rebuild(&mut ui, [176., 240.]);
    assert_eq!(report.rows, 2);
    assert_eq!(visible[2].min, [8., 76.]);
    assert_eq!(ui.focused(), Some(3));
    assert_eq!(ui.pointer_up([270., 30.]), None);
    assert_eq!(ui.hit_test([20., 80.]), Some(3));
    assert_eq!(
        ui.focus_direction(FocusDirection::Up, FocusWrap::Clamp),
        Some(1)
    );
    ui.pointer_down([20., 80.]);
    let (report, _) = rebuild(&mut ui, [70., 240.]);
    assert_eq!((report.written, report.overflow), (1, 2));
    assert_eq!(ui.focused(), None);
    assert_eq!(ui.captured(), None);
}
