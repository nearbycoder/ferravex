use ferravex_render::{
    Rect, Vec2,
    canvas::Canvas2D,
    input::{
        ActionKind, Binding, InputCode, PlayerInput, host_frame,
        touch::{Control, Event, MAX_TOUCH_CONTROLS, Output, Phase, Region, TouchLayoutSettings},
        touch_event,
        touch_layout::{
            MinimumTouchSize as Minimum, TouchGeometryError as Error, TouchLayout, TouchSlot,
        },
    },
    ui::{UiRect, Viewport},
    vec2,
};

fn button(slot: usize, channel: u8, min: [f32; 2], size: [f32; 2]) -> TouchSlot {
    TouchSlot::rect(slot, UiRect { min, size }, Output::Button(channel))
}
fn center(control: Control) -> Vec2 {
    match control.region {
        Region::Rect(r) => r.center(),
        Region::Circle { center, .. } => center,
    }
}
fn event(id: u64, position: Vec2, phase: Phase) {
    touch_event(Event {
        id,
        position,
        phase,
    });
}
fn player() -> PlayerInput {
    let mut input = PlayerInput::transient("journey400-278");
    for (name, channel) in [
        ("left", 12),
        ("right", 13),
        ("jump", 3),
        ("pause", 11),
        ("confirm", 10),
    ] {
        input
            .action(
                name,
                name,
                ActionKind::Button,
                vec![Binding::digital(InputCode::Touch(channel))],
            )
            .unwrap();
    }
    input
}
const NAMES: [&str; 5] = ["left", "right", "jump", "pause", "confirm"];
fn emberline_slots() -> [TouchSlot; 5] {
    [
        button(0, 12, [12., 288.], [56.; 2]),
        button(1, 13, [80., 288.], [56.; 2]),
        button(2, 3, [244., 288.], [64., 56.]),
        button(3, 11, [236., 10.], [72., 44.]),
        button(4, 10, [12., 100.], [296., 120.]),
    ]
}

#[test]
fn viewport_and_canvas_extract_existing_projection_without_changing_slots() {
    let screen = [800., 600.];
    let viewport = Viewport::fit(screen, [400., 200.], [10., 20., 30., 40.]).unwrap();
    let canvas = Canvas2D::try_fit_in(
        vec2(400., 200.),
        Vec2::from(screen),
        UiRect {
            min: [10., 20.],
            size: [760., 540.],
        },
    )
    .unwrap()
    .unwrap();
    let slots = [
        button(0, 12, [10., 20.], [56., 44.]),
        TouchSlot {
            slot: 1,
            control: Some(Control {
                region: Region::Circle {
                    center: vec2(202., 160.),
                    radius: 210.,
                },
                output: Output::Stick {
                    x: 0,
                    y: 1,
                    radius: 66.,
                    floating: false,
                },
            }),
        },
    ];
    let ui = TouchLayout::from_viewport(viewport, &slots, Minimum::Preserve).unwrap();
    let world = TouchLayout::from_canvas(canvas, &slots, Minimum::Preserve).unwrap();
    assert_eq!(ui, world);
    assert_eq!(world.controls()[0].output, Output::Button(12));
    assert_eq!(
        world.controls()[1],
        Control {
            region: Region::Circle {
                center: canvas.project(vec2(202., 160.)),
                radius: 210. * canvas.scale
            },
            output: Output::Stick {
                x: 0,
                y: 1,
                radius: 66. * canvas.scale,
                floating: false
            },
        }
    );
    // Broad Neon's default acquisition region stays circular and extends outside
    // the canvas; the adapter does not silently crop it into another shape.
    let Region::Circle { center, radius } = world.controls()[1].region else {
        panic!()
    };
    assert!(center.x - radius < canvas.offset.x);
}

#[test]
fn safe_origins_are_added_once_and_dpr_is_only_a_synthetic_host_conversion() {
    for screen in [[390., 844.], [640., 320.], [2560., 720.]] {
        let viewport = Viewport::responsive(screen, [10., 24., 12., 16.]).unwrap();
        let slots = [button(0, 3, [8., 12.], [56., 44.])];
        let layout = TouchLayout::from_viewport(viewport, &slots, Minimum::Require(44.)).unwrap();
        assert_eq!(
            layout.controls()[0].region,
            Region::Rect(Rect::new(18., 36., 56., 44.))
        );
        let point = center(layout.controls()[0]);
        assert_eq!(viewport.unproject(point.to_array()), Some([36., 34.]));
        for dpr in [1., 1.5, 2., 3.] {
            assert_eq!((point * dpr) / dpr, point);
        }
    }
}

#[test]
fn minimum_is_explicit_no_crop_or_invisible_enlargement_and_defaults_stay_compatible() {
    let viewport = Viewport::responsive([320., 360.], [0.; 4]).unwrap();
    let slots = [button(0, 3, [-10., 350.], [20., 30.])];
    let authored = TouchLayout::from_viewport(viewport, &slots, Minimum::Preserve).unwrap();
    assert_eq!(authored.controls()[0], slots[0].control.unwrap());
    assert_eq!(
        TouchLayout::from_viewport(viewport, &slots, Minimum::Require(44.)),
        Err(Error::BelowMinimum)
    );
    for n in [0., -1., f32::NAN, f32::INFINITY] {
        assert_eq!(
            TouchLayout::from_viewport(viewport, &slots, Minimum::Require(n)),
            Err(Error::InvalidGeometry)
        );
    }
    let scaled = Viewport::fit([160., 180.], [320., 360.], [0.; 4]).unwrap();
    assert_eq!(
        TouchLayout::from_viewport(scaled, &emberline_slots(), Minimum::Require(44.)),
        Err(Error::BelowMinimum)
    );
    let circle = [TouchSlot {
        slot: 0,
        control: Some(Control {
            region: Region::Circle {
                center: vec2(50., 50.),
                radius: 21.,
            },
            output: Output::Button(3),
        }),
    }];
    assert_eq!(
        TouchLayout::from_viewport(viewport, &circle, Minimum::Require(44.)),
        Err(Error::BelowMinimum)
    );
    // Custom legacy defaults still bypass size changes; nondefault settings use
    // the existing 44 minimum/8 margin. The source layout remains unchanged.
    let settings = TouchLayoutSettings {
        opacity: 0.7,
        ..Default::default()
    };
    let custom = authored.resolve(settings, [320., 360.]).unwrap();
    assert_eq!(
        custom.controls()[0].region,
        Region::Rect(Rect::new(8., 308., 44., 44.))
    );
    assert_eq!(authored.controls()[0], slots[0].control.unwrap());
}

#[test]
fn capacity_order_hidden_gaps_and_invalid_shapes_are_rejected_without_partial_mutation() {
    let viewport = Viewport::responsive([800., 600.], [0.; 4]).unwrap();
    let slots: [_; MAX_TOUCH_CONTROLS] =
        std::array::from_fn(|i| button(i, i as u8, [i as f32 * 48., 20.], [44.; 2]));
    let full = TouchLayout::from_viewport(viewport, &slots, Minimum::Require(44.)).unwrap();
    assert_eq!(full.controls().len(), 16);
    let oversized: [_; 17] = std::array::from_fn(|i| button(i, 0, [0.; 2], [44.; 2]));
    assert_eq!(
        TouchLayout::from_viewport(viewport, &oversized, Minimum::Preserve),
        Err(Error::Capacity)
    );
    for wrong in [1, 16, usize::MAX] {
        assert_eq!(
            TouchLayout::from_viewport(viewport, &[TouchSlot::hidden(wrong)], Minimum::Preserve),
            Err(Error::SlotOrder)
        );
    }
    assert_eq!(
        TouchLayout::from_viewport(
            viewport,
            &[TouchSlot::hidden(0), slots[1]],
            Minimum::Preserve
        ),
        Err(Error::HiddenGap)
    );
    for bad in [
        button(0, 64, [0.; 2], [44.; 2]),
        button(0, 0, [f32::NAN, 0.], [44.; 2]),
        button(0, 0, [0.; 2], [0., 44.]),
        TouchSlot {
            slot: 0,
            control: Some(Control {
                region: Region::Circle {
                    center: Vec2::ZERO,
                    radius: -1.,
                },
                output: Output::Button(0),
            }),
        },
        TouchSlot {
            slot: 0,
            control: Some(Control {
                region: Region::Rect(Rect::new(0., 0., 44., 44.)),
                output: Output::Stick {
                    x: 1,
                    y: 1,
                    radius: 10.,
                    floating: true,
                },
            }),
        },
    ] {
        assert_eq!(
            TouchLayout::from_viewport(viewport, &[bad], Minimum::Preserve),
            Err(Error::InvalidGeometry)
        );
    }
    for screen in [[-1., 600.], [f32::NAN, 600.]] {
        assert_eq!(
            full.resolve(TouchLayoutSettings::default(), screen),
            Err(Error::InvalidGeometry)
        );
    }
    let mut invalid = TouchLayoutSettings::default();
    invalid.edits[15].scale = f32::NAN;
    assert_eq!(
        full.resolve(invalid, [800., 600.]),
        Err(Error::InvalidGeometry)
    );
    assert_eq!(full.controls().len(), 16);
}

#[test]
fn suspension_and_resize_reconstruct_from_authored_shapes_without_stale_offsets() {
    let slots = emberline_slots();
    let good = Canvas2D::try_fit(vec2(320., 360.), vec2(640., 720.))
        .unwrap()
        .unwrap();
    let before = TouchLayout::from_canvas(good, &slots, Minimum::Preserve).unwrap();
    for canvas in [
        Canvas2D {
            screen: Vec2::ZERO,
            ..good
        },
        Canvas2D {
            scale: 0.001,
            ..good
        },
    ] {
        assert!(
            TouchLayout::from_canvas(canvas, &slots, Minimum::Preserve)
                .unwrap()
                .controls()
                .is_empty()
        );
    }
    for canvas in [
        Canvas2D { scale: 0., ..good },
        Canvas2D {
            offset: vec2(f32::NAN, 0.),
            ..good
        },
        Canvas2D {
            screen: vec2(-1., 0.),
            ..good
        },
    ] {
        assert_eq!(
            TouchLayout::from_canvas(canvas, &slots, Minimum::Preserve),
            Err(Error::InvalidGeometry)
        );
    }
    let hidden = Viewport::responsive([0.; 2], [10.; 4]).unwrap();
    assert!(
        TouchLayout::from_viewport(hidden, &slots, Minimum::Require(44.))
            .unwrap()
            .controls()
            .is_empty()
    );
    assert_eq!(
        before,
        TouchLayout::from_canvas(good, &slots, Minimum::Preserve).unwrap()
    );
}

#[test]
fn real_journal_each_visible_button_emits_only_its_edge_and_taps_wait_for_ticks() {
    let screen = [390, 844];
    let layout = TouchLayout::from_viewport(
        Viewport::responsive(screen.map(|n| n as f32), [10., 24., 12., 16.]).unwrap(),
        &emberline_slots(),
        Minimum::Require(44.),
    )
    .unwrap();
    let mut input = player();
    host_frame(0., screen[0], screen[1], || {
        input.set_touch_layout(layout.controls()).unwrap();
        input.sample(true);
    });
    for (index, control) in layout.controls().iter().enumerate() {
        let position = center(*control);
        event(278, position, Phase::Start);
        event(278, position, Phase::End);
        host_frame(0., screen[0], screen[1], || {
            input.sample(true);
            input.sample(true);
        });
        // A second render frame with zero simulated ticks cannot lose the tap.
        host_frame(0., screen[0], screen[1], || {
            input.sample(true);
            let tick = input.take_tick(NAMES);
            for (i, action) in tick.iter().enumerate() {
                assert_eq!(action.pressed, i == index, "slot {index}, action {i}");
                assert_eq!(action.released, i == index);
                assert_eq!(action.value, 0.);
            }
            assert!(
                input
                    .take_tick(NAMES)
                    .iter()
                    .all(|a| !a.pressed && !a.released)
            );
        });
    }
}

#[test]
fn real_journal_identical_layout_preserves_capture_changed_resize_and_hide_cancel_it() {
    let mut input = player();
    let slots = [button(0, 3, [20., 20.], [56.; 2])];
    let a = TouchLayout::from_viewport(
        Viewport::responsive([320., 360.], [0.; 4]).unwrap(),
        &slots,
        Minimum::Preserve,
    )
    .unwrap();
    let b = TouchLayout::from_viewport(
        Viewport::responsive([640., 360.], [30., 0., 0., 0.]).unwrap(),
        &slots,
        Minimum::Preserve,
    )
    .unwrap();
    host_frame(0., 320, 360, || {
        input.set_touch_layout(a.controls()).unwrap();
        input.sample(true);
    });
    event(1, center(a.controls()[0]), Phase::Start);
    host_frame(0., 320, 360, || {
        input.sample(true);
        assert!(input.take("jump").pressed);
        input.set_touch_layout(a.controls()).unwrap();
        assert!(input.touch_active());
        assert!(input.held("jump"));
    });
    host_frame(0., 640, 360, || {
        input.set_touch_layout(b.controls()).unwrap();
        assert!(!input.touch_active());
        assert!(input.take("jump").released);
        input.sample(true);
    });
    event(1, center(b.controls()[0]), Phase::Move); // Old finger cannot reacquire.
    host_frame(0., 640, 360, || {
        input.sample(true);
        assert!(!input.held("jump"));
    });
    event(1, center(b.controls()[0]), Phase::Start);
    host_frame(0., 640, 360, || {
        input.sample(true);
        assert!(input.take("jump").pressed);
        let hidden = TouchLayout::from_viewport(
            Viewport::responsive([640., 360.], [0.; 4]).unwrap(),
            &[TouchSlot::hidden(0)],
            Minimum::Preserve,
        )
        .unwrap();
        input.set_touch_layout(hidden.controls()).unwrap();
        assert!(!input.touch_active());
        assert!(input.take("jump").released);
    });
    event(1, center(b.controls()[0]), Phase::Start);
    host_frame(0., 640, 360, || {
        input.sample(true);
        assert!(!input.take("jump").pressed);
    });
}

#[test]
fn real_journal_saved_slot_edits_match_drawing_and_survive_hidden_suffix_and_resize() {
    let slots = emberline_slots();
    let mut settings = TouchLayoutSettings {
        mirrored: true,
        scale: 0.8,
        opacity: 0.4,
        ..Default::default()
    };
    for (i, edit) in settings.edits.iter_mut().take(5).enumerate() {
        edit.offset = vec2(0.01 * i as f32, 0.01);
    }
    let saved = settings;
    let mut input = player();
    for screen in [[640, 720], [390, 844], [640, 720]] {
        let canvas = Canvas2D::try_fit(vec2(320., 360.), Vec2::from(screen.map(|n| n as f32)))
            .unwrap()
            .unwrap();
        let authored = TouchLayout::from_canvas(canvas, &slots, Minimum::Preserve).unwrap();
        let resolved = authored
            .resolve(settings, screen.map(|n| n as f32))
            .unwrap();
        host_frame(0., screen[0], screen[1], || {
            input.set_touch_layout(authored.controls()).unwrap();
            input.set_touch_settings(settings).unwrap();
            input.sample(true);
            assert!(input.touch_customized());
        });
        for (i, control) in resolved.controls().iter().enumerate() {
            assert_eq!(
                *control,
                settings
                    .apply(
                        authored.controls()[i],
                        i,
                        Vec2::from(screen.map(|n| n as f32))
                    )
                    .unwrap()
            );
            event(2, center(*control), Phase::Start);
            host_frame(0., screen[0], screen[1], || {
                input.sample(true);
                for (j, action) in input.take_tick(NAMES).iter().enumerate() {
                    assert_eq!(action.pressed, i == j);
                }
            });
            event(2, center(*control), Phase::Cancel);
            host_frame(0., screen[0], screen[1], || {
                input.sample(true);
                input.take_tick(NAMES);
            });
        }
        let mut suffix_hidden = slots;
        suffix_hidden[4] = TouchSlot::hidden(4);
        let short = TouchLayout::from_canvas(canvas, &suffix_hidden, Minimum::Preserve).unwrap();
        host_frame(0., screen[0], screen[1], || {
            input.set_touch_layout(short.controls()).unwrap();
            assert_eq!(input.touch_settings(), saved);
            assert_eq!(
                short
                    .resolve(input.touch_settings(), screen.map(|n| n as f32))
                    .unwrap()
                    .controls(),
                &resolved.controls()[..4]
            );
        });
    }
    assert_eq!(settings, saved);
    host_frame(0., 640, 720, || {
        input
            .set_touch_settings(TouchLayoutSettings::default())
            .unwrap();
        assert!(!input.touch_customized());
    });
}

#[test]
fn real_journal_shared_edges_keep_first_slot_priority_and_look_stick_outputs_work() {
    let mut input = player();
    input
        .action(
            "move",
            "Move",
            ActionKind::Axis,
            vec![Binding::digital(InputCode::Touch(0))],
        )
        .unwrap();
    input
        .action(
            "look",
            "Look",
            ActionKind::Delta,
            vec![Binding::digital(InputCode::TouchDelta(0))],
        )
        .unwrap();
    let slots = [
        button(0, 12, [10., 10.], [44.; 2]),
        button(1, 13, [54., 10.], [44.; 2]),
        TouchSlot {
            slot: 2,
            control: Some(Control {
                region: Region::Circle {
                    center: vec2(150., 150.),
                    radius: 44.,
                },
                output: Output::Stick {
                    x: 0,
                    y: 1,
                    radius: 36.,
                    floating: false,
                },
            }),
        },
        TouchSlot {
            slot: 3,
            control: Some(Control {
                region: Region::Rect(Rect::new(220., 100., 80., 100.)),
                output: Output::Look { x: 0, y: 1 },
            }),
        },
    ];
    let layout = TouchLayout::from_viewport(
        Viewport::responsive([320., 360.], [0.; 4]).unwrap(),
        &slots,
        Minimum::Preserve,
    )
    .unwrap();
    host_frame(0., 320, 360, || {
        input.set_touch_layout(layout.controls()).unwrap();
        input.sample(true);
    });
    event(1, vec2(54., 20.), Phase::Start);
    event(2, vec2(168., 150.), Phase::Start);
    event(3, vec2(240., 150.), Phase::Start);
    event(3, vec2(252., 150.), Phase::Move);
    host_frame(0., 320, 360, || {
        input.sample(true);
        assert!(input.take("left").pressed);
        assert!(!input.take("right").pressed);
        assert_eq!(input.value("move"), 0.5);
        assert!((input.take("look").value - 12. * input.settings().touch_sensitivity).abs() < 1e-6);
    });
    for id in [1, 2, 3] {
        event(id, Vec2::ZERO, Phase::Cancel);
    }
    host_frame(0., 320, 360, || {
        input.sample(true);
        assert!(!input.touch_active());
        assert_eq!(input.value("move"), 0.);
    });
}

#[test]
fn pocket_four_button_order_and_regions_retain_saved_channel_identity() {
    let mut input = PlayerInput::transient("journey400-278-pocket");
    let channels = [12, 13, 14, 6];
    let names = ["left", "right", "gas", "brake"];
    for (name, channel) in names.into_iter().zip(channels) {
        input
            .action(
                name,
                name,
                ActionKind::Button,
                vec![Binding::digital(InputCode::Touch(channel))],
            )
            .unwrap();
    }
    let screen = [390., 844.];
    let lane = 97.5;
    let positions = [0., lane, screen[0] - lane, screen[0] - lane * 2.];
    let slots: [_; 4] =
        std::array::from_fn(|i| button(i, channels[i], [positions[i], 714.], [lane, 130.]));
    let layout = TouchLayout::from_viewport(
        Viewport::responsive(screen, [0.; 4]).unwrap(),
        &slots,
        Minimum::Require(44.),
    )
    .unwrap();
    host_frame(0., 390, 844, || {
        input.set_touch_layout(layout.controls()).unwrap();
        input.sample(true);
    });
    for (i, control) in layout.controls().iter().enumerate() {
        assert_eq!(*control, slots[i].control.unwrap());
        event(278, center(*control), Phase::Start);
        event(278, center(*control), Phase::End);
        host_frame(0., 390, 844, || {
            input.sample(true);
            for (j, action) in input.take_tick(names).iter().enumerate() {
                assert_eq!(action.pressed, i == j);
                assert_eq!(action.released, i == j);
            }
        });
    }
}

use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    hint::black_box,
};
thread_local! {
    static ALLOCATIONS: Cell<Option<usize>> = const { Cell::new(None) };
}
struct CountingAllocator;
fn allocated(pointer: *mut u8) {
    if !pointer.is_null() {
        let _ = ALLOCATIONS.try_with(|counter| {
            if let Some(count) = counter.get() {
                counter.set(Some(count + 1));
            }
        });
    }
}
// SAFETY: All allocator operations forward original arguments to System.
// Thread-local accounting allocates nothing and tolerates thread teardown.
unsafe impl GlobalAlloc for CountingAllocator {
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
        unsafe { System.dealloc(p, layout) };
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

#[test]
fn projection_resolution_capacity_errors_and_resize_cycles_allocate_nothing() {
    struct StopCounting;
    impl Drop for StopCounting {
        fn drop(&mut self) {
            ALLOCATIONS.with(|counter| counter.set(None));
        }
    }
    ALLOCATIONS.with(|counter| counter.set(Some(0)));
    let guard = StopCounting;
    for _ in 0..64 {
        let slots = black_box(emberline_slots());
        let canvas = Canvas2D::try_fit(vec2(320., 360.), black_box(vec2(640., 720.)))
            .unwrap()
            .unwrap();
        let layout = TouchLayout::from_canvas(canvas, &slots, Minimum::Require(44.)).unwrap();
        let settings = TouchLayoutSettings {
            mirrored: true,
            ..Default::default()
        };
        black_box(layout.resolve(settings, [640., 720.]).unwrap());
        black_box(
            TouchLayout::from_viewport(
                Viewport::responsive([640., 720.], [10., 20., 30., 40.]).unwrap(),
                &slots,
                Minimum::Preserve,
            )
            .unwrap(),
        );
        assert!(
            TouchLayout::from_canvas(
                Canvas2D {
                    screen: Vec2::ZERO,
                    ..canvas
                },
                &slots,
                Minimum::Require(44.)
            )
            .unwrap()
            .controls()
            .is_empty()
        );
        assert_eq!(
            TouchLayout::from_canvas(
                Canvas2D {
                    scale: f32::NAN,
                    ..canvas
                },
                &slots,
                Minimum::Preserve
            ),
            Err(Error::InvalidGeometry)
        );
        let oversized = [TouchSlot::hidden(0); 17];
        assert_eq!(
            TouchLayout::from_canvas(canvas, &oversized, Minimum::Preserve),
            Err(Error::Capacity)
        );
    }
    let count = ALLOCATIONS.with(|counter| counter.get().unwrap());
    drop(guard);
    assert_eq!(count, 0);
}
