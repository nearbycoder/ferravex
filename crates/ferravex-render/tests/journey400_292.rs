use ferravex_render::{
    Rect,
    input::{
        ActionKind, Binding, HintDevice, HintError, InputCode, InputError, PlayerInput,
        VisibleHintError as Error, VisibleTouchLabel as Label,
        touch::{Control, Output, Region, TouchLayoutSettings},
        touch_layout::{MinimumTouchSize, TouchLayout, TouchSlot},
    },
    ui::{UiRect, Viewport},
};

fn binding(code: InputCode, scale: f32) -> Binding {
    Binding {
        code,
        scale,
        deadzone: 0.,
    }
}
fn control(output: Output) -> Control {
    Control {
        region: Region::Rect(Rect::new(10., 10., 80., 80.)),
        output,
    }
}
fn player(kind: ActionKind, bindings: Vec<Binding>) -> PlayerInput {
    let mut input = PlayerInput::transient("journey400-292");
    input.action("action", "Action", kind, bindings).unwrap();
    input
}

#[test]
fn labels_follow_real_slots_hidden_suffix_remaps_custom_layouts_import_and_reset() {
    let layout = TouchLayout::from_viewport(
        Viewport::responsive([320., 480.], [0.; 4]).unwrap(),
        &[
            TouchSlot::rect(
                0,
                UiRect {
                    min: [10., 380.],
                    size: [64.; 2],
                },
                Output::Button(6),
            ),
            TouchSlot::hidden(1),
        ],
        MinimumTouchSize::Require(44.),
    )
    .unwrap();
    let mut input = player(
        ActionKind::Button,
        vec![Binding::digital(InputCode::Touch(6))],
    );
    input.set_touch_layout(layout.controls()).unwrap();
    let labels = [Label {
        slot: 0,
        code: InputCode::Touch(6),
        label: "BRAKE",
    }];
    let mut text = [0; 256];
    let original = input.export_config().unwrap();
    assert_eq!(
        input
            .format_visible_hint_for("action", None, &labels, &mut text)
            .unwrap(),
        "BRAKE"
    );
    input
        .rebind("action", vec![binding(InputCode::Touch(14), -0.5)])
        .unwrap();
    assert_eq!(
        input
            .format_visible_hint_for("action", None, &labels, &mut text)
            .unwrap(),
        "Touch 14 x-0.5 (not visible)"
    );
    // A hidden slot cannot turn an arbitrary output channel into a visible button.
    let hidden = [Label {
        slot: 1,
        code: InputCode::Touch(14),
        label: "GAS",
    }];
    assert_eq!(
        input.format_visible_hint_for("action", None, &hidden, &mut text),
        Err(Error::InvalidLabel { index: 0 })
    );
    input.import_config(&original).unwrap();
    input
        .set_touch_settings(TouchLayoutSettings {
            mirrored: true,
            scale: 1.4,
            ..Default::default()
        })
        .unwrap();
    assert!(input.touch_customized());
    let drawing = layout
        .resolve(input.touch_settings(), [320., 480.])
        .unwrap();
    assert_ne!(drawing.controls()[0].region, layout.controls()[0].region);
    // Caller draws BRAKE on resolved custom geometry, so its name stays valid.
    assert_eq!(
        input
            .format_visible_hint_for("action", None, &labels, &mut text)
            .unwrap(),
        "BRAKE"
    );
    // If the custom overlay hides those labels, caller omits them.
    assert_eq!(
        input
            .format_visible_hint_for("action", None, &[], &mut text)
            .unwrap(),
        "Touch 6 (not visible)"
    );
    input.rebind("action", vec![]).unwrap();
    assert_eq!(
        input
            .format_visible_hint_for("action", None, &labels, &mut text)
            .unwrap(),
        "Unbound"
    );
    input.reset_defaults();
    assert_eq!(input.export_config().unwrap(), original);
    assert_eq!(
        input
            .format_visible_hint_for("action", None, &labels, &mut text)
            .unwrap(),
        "BRAKE"
    );
    input.set_touch_layout(&[]).unwrap();
    assert_eq!(
        input.format_visible_hint_for("action", None, &labels, &mut text),
        Err(Error::InvalidLabel { index: 0 })
    );
    assert_eq!(
        input.format_hint_for("action", None, &mut text).unwrap(),
        "Touch 6"
    );
}

#[test]
fn preserves_all_bindings_order_duplicates_scales_and_distinguishes_sticks_and_gestures() {
    let mut input = player(
        ActionKind::Axis,
        vec![
            binding(InputCode::Key(32), 1.),
            binding(InputCode::Touch(6), -1.),
            binding(InputCode::Touch(0), 0.5),
            binding(InputCode::Touch(6), 1.),
            binding(InputCode::Touch(1), 0.),
            binding(InputCode::PadAxis(0), -2.),
        ],
    );
    input
        .set_touch_layout(&[
            control(Output::Button(6)),
            control(Output::Stick {
                x: 0,
                y: 1,
                radius: 24.,
                floating: true,
            }),
            control(Output::Button(6)),
            control(Output::Look { x: 0, y: 1 }),
        ])
        .unwrap();
    let labels = [
        Label {
            slot: 2,
            code: InputCode::Touch(6),
            label: "RIGHT",
        },
        Label {
            slot: 0,
            code: InputCode::Touch(6),
            label: "LEFT",
        },
        Label {
            slot: 1,
            code: InputCode::Touch(0),
            label: "STEER",
        },
        Label {
            slot: 3,
            code: InputCode::TouchDelta(0),
            label: "SWIPE",
        },
    ];
    let mut text = [0; 256];
    assert_eq!(
        input
            .format_visible_hint_for("action", None, &labels, &mut text)
            .unwrap(),
        "Space / RIGHT x-1 / LEFT x-1 / STEER (stick X) x0.5 / RIGHT / LEFT / Pad axis 0 x-2"
    );
    assert_eq!(
        input
            .format_visible_hint_for("action", Some(HintDevice::Touch), &labels, &mut text)
            .unwrap(),
        "RIGHT x-1 / LEFT x-1 / STEER (stick X) x0.5 / RIGHT / LEFT"
    );
    assert_eq!(
        input
            .format_visible_hint("action", &labels, &mut text)
            .unwrap(),
        "Space"
    );
    input
        .action(
            "look",
            "Look",
            ActionKind::Delta,
            vec![
                binding(InputCode::TouchDelta(0), -0.25),
                binding(InputCode::TouchDelta(1), 1.),
            ],
        )
        .unwrap();
    assert_eq!(
        input
            .format_visible_hint_for("look", None, &labels, &mut text)
            .unwrap(),
        "SWIPE (gesture X) x-0.25 / Touch look 1 (gesture; not visible)"
    );
    assert_eq!(
        input
            .format_visible_hint_for("look", Some(HintDevice::Gamepad), &labels, &mut text)
            .unwrap(),
        "Unbound"
    );
}

#[test]
fn visible_errors_and_utf8_capacity_are_exact_and_atomic() {
    let mut input = player(ActionKind::Button, vec![binding(InputCode::Touch(6), -1.)]);
    input
        .set_touch_layout(&[control(Output::Button(6))])
        .unwrap();
    let valid = Label {
        slot: 0,
        code: InputCode::Touch(6),
        label: "Frein é",
    };
    let invalid = [
        Label { slot: 16, ..valid },
        Label {
            code: InputCode::Touch(14),
            ..valid
        },
        Label {
            code: InputCode::Key(32),
            ..valid
        },
        Label {
            code: InputCode::TouchDelta(6),
            ..valid
        },
        Label { label: "", ..valid },
        Label {
            label: "bad\nlabel",
            ..valid
        },
    ];
    let mut text = [b'?'; 128];
    for label in invalid {
        assert_eq!(
            input.format_visible_hint_for("action", None, &[label], &mut text),
            Err(Error::InvalidLabel { index: 0 })
        );
        assert_eq!(text, [b'?'; 128]);
    }
    let long = "A".repeat(97);
    assert_eq!(
        input.format_visible_hint_for(
            "action",
            None,
            &[Label {
                label: &long,
                ..valid
            }],
            &mut text
        ),
        Err(Error::InvalidLabel { index: 0 })
    );
    assert_eq!(
        input.format_visible_hint_for("action", None, &[valid; 2], &mut text),
        Err(Error::InvalidLabel { index: 1 })
    );
    assert_eq!(
        input.format_visible_hint_for("action", None, &[valid; 33], &mut text),
        Err(Error::Capacity)
    );
    assert_eq!(
        input.format_visible_hint_for("missing", None, &[valid; 33], &mut text),
        Err(Error::Hint(HintError::Input(InputError::InvalidHandle)))
    );
    let expected = "Frein é x-1";
    for size in 0..expected.len() {
        assert_eq!(
            input.format_visible_hint_for("action", None, &[valid], &mut text[..size]),
            Err(Error::Hint(HintError::BufferTooSmall {
                required: expected.len()
            }))
        );
        assert_eq!(text, [b'?'; 128]);
    }
    assert_eq!(
        input
            .format_visible_hint_for("action", None, &[valid], &mut text[..expected.len()])
            .unwrap(),
        expected
    );
    assert!(text[expected.len()..].iter().all(|b| *b == b'?'));
}

#[test]
fn real_page_navigation_keys_have_named_hints_with_exact_buffers_and_unknown_fallbacks() {
    let mut input = player(ActionKind::Button, vec![]);
    for (code, expected) in [
        (0xff55, "PageUp"),
        (0xff56, "PageDown"),
        (0xff50, "Home"),
        (0xff57, "End"),
        (0xff63, "Insert"),
        (0xff58, "Key 65368"),
    ] {
        input
            .rebind("action", vec![Binding::digital(InputCode::Key(code))])
            .unwrap();
        let mut text = [b'?'; 32];
        for size in 0..expected.len() {
            assert_eq!(
                input.format_hint_for("action", None, &mut text[..size]),
                Err(HintError::BufferTooSmall {
                    required: expected.len()
                })
            );
            assert_eq!(text, [b'?'; 32]);
        }
        assert_eq!(
            input
                .format_hint_for("action", None, &mut text[..expected.len()])
                .unwrap(),
            expected
        );
        assert_eq!(
            input.format_visible_hint("action", &[], &mut text).unwrap(),
            expected
        );
    }
}

use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    hint::black_box,
};
thread_local! { static ALLOCATIONS: Cell<Option<usize>> = const { Cell::new(None) }; }
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
// SAFETY: Original allocator arguments are forwarded to System. Thread-local
// accounting allocates nothing and tolerates thread teardown.
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
fn maximum_bindings_labels_and_capacity_failures_allocate_nothing() {
    let mut input = player(
        ActionKind::Axis,
        (0..64)
            .map(|i| {
                binding(
                    InputCode::Touch((i % 2) as u8),
                    if i % 2 == 0 { f32::MAX } else { -f32::MAX },
                )
            })
            .collect(),
    );
    input
        .set_touch_layout(
            &[control(Output::Stick {
                x: 0,
                y: 1,
                radius: 24.,
                floating: false,
            }); 16],
        )
        .unwrap();
    let long = "X".repeat(96);
    let labels: [Label<'_>; 32] = std::array::from_fn(|i| Label {
        slot: i / 2,
        code: InputCode::Touch((i % 2) as u8),
        label: &long,
    });
    let expected = (0..64)
        .flat_map(|i| {
            let label = &long;
            (0..16).map(move |_| {
                format!(
                    "{label} (stick {}) x{}",
                    if i % 2 == 0 { "X" } else { "Y" },
                    if i % 2 == 0 { f32::MAX } else { -f32::MAX }
                )
            })
        })
        .collect::<Vec<_>>()
        .join(" / ");
    let mut buffer = vec![b'?'; expected.len() + 1];
    struct StopCounting;
    impl Drop for StopCounting {
        fn drop(&mut self) {
            ALLOCATIONS.with(|counter| counter.set(None));
        }
    }
    ALLOCATIONS.with(|counter| counter.set(Some(0)));
    let guard = StopCounting;
    for _ in 0..4 {
        assert_eq!(
            black_box(input.format_visible_hint_for("action", None, &labels, &mut buffer)).unwrap(),
            expected
        );
        assert_eq!(buffer[expected.len()], b'?');
        assert_eq!(
            input.format_visible_hint_for("action", None, &labels, &mut []),
            Err(Error::Hint(HintError::BufferTooSmall {
                required: expected.len()
            }))
        );
        assert_eq!(
            input
                .format_visible_hint("action", &labels, &mut buffer[..7])
                .unwrap(),
            "Unbound"
        );
        assert_eq!(
            input.format_visible_hint_for("action", None, &[labels[0]; 33], &mut buffer),
            Err(Error::Capacity)
        );
        assert_eq!(
            input.format_visible_hint_for("action", None, &[labels[0]; 2], &mut buffer),
            Err(Error::InvalidLabel { index: 1 })
        );
    }
    let count = ALLOCATIONS.with(|counter| counter.get().unwrap());
    drop(guard);
    assert_eq!(count, 0);
}
