use ferravex_render::{
    Color, Vec2,
    input::{host_frame, touch, touch_event},
    ui::{
        ButtonStyle, PANEL, TextStyle, Ui, UiComposition, UiError, UiLabel, UiRect, Viewport,
        text_layout::LineLimit,
    },
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    fmt,
};

thread_local! {
    static RECORD: Cell<bool> = const { Cell::new(false) };
    static CALLS: Cell<usize> = const { Cell::new(0) };
}
struct Allocator;
fn allocated(pointer: *mut u8) {
    if !pointer.is_null() {
        let _ = RECORD.try_with(|on| {
            if on.get() {
                let _ = CALLS.try_with(|calls| calls.set(calls.get() + 1));
            }
        });
    }
}
// SAFETY: Forward exactly the caller's allocation contract to System. Counters
// are thread-local and do not allocate; ignore destruction-time TLS failures.
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        allocated(pointer);
        pointer
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        allocated(pointer);
        pointer
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let pointer = unsafe { System.realloc(pointer, layout, size) };
        allocated(pointer);
        pointer
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: Allocator = Allocator;

const RECT: UiRect = UiRect {
    min: [20.; 2],
    size: [120., 44.],
};
const OTHER: UiRect = UiRect {
    min: [20., 100.],
    size: [120., 44.],
};

fn details(group: &mut UiComposition<'_>, frame: usize) -> Result<(), UiError> {
    group.gradient(RECT, PANEL, Color::new(0.2, 0.4, 0.6, 1.))?;
    group.text_fmt(RECT, TextStyle::default(), format_args!("RESULT {frame}"))?;
    group.wrapped_text(
        OTHER,
        "Delivery complete.\nContinue to the next route.",
        TextStyle::default(),
        LineLimit::Ellipsis(2),
    )?;
    group.label(
        RECT,
        "FULL INSPECTABLE RESULT TITLE é",
        TextStyle::default(),
    )?;
    group.hud_button(
        10,
        OTHER,
        "CONTINUE EXPEDITION",
        true,
        ButtonStyle::default(),
    )
}

#[test]
fn rejected_result_group_has_no_public_paint_description_or_pointer_owner() {
    let mut ui = Ui::new();
    ui.set_inspection_enabled(true);
    let viewport = Viewport::responsive([200.; 2], [0.; 4]).unwrap();
    let huge = "x".repeat(32769);
    // Synthetic native touch injection: the rejected button must not claim input.
    for phase in [touch::Phase::Start, touch::Phase::End] {
        touch_event(touch::Event {
            id: 1,
            phase,
            position: Vec2::new(30., 110.),
        });
    }
    host_frame(0., 200, 200, || {
        ui.begin(viewport);
        ui.button(1, RECT, "PRIOR", true, ButtonStyle::default())
            .unwrap();
        assert_eq!(
            ui.compose(|group| {
                group.modal(99)?;
                details(group, 1)?;
                group.wrapped_text(OTHER, &huge, TextStyle::default(), LineLimit::Clip(0))?;
                Ok(())
            }),
            Err(UiError::Capacity)
        );
        assert_eq!(ui.inspect().unwrap().len(), 1);
        let prior = ui.inspect().unwrap().next().unwrap();
        assert_eq!(prior.id, Some(1));
        assert!(prior.interaction_registered);
        ui.finish(true);
        assert!(!ui.activated(10) && !ui.activated(99));
        assert!(!ui.interaction().pointer_consumed);
        assert_eq!(ui.interaction().pointer_activated, None);
        ui.paint();
    });
    // Replace the rejected card with small, readable copy and the same primary
    // action. This actual synthetic press/release activates that fallback once.
    for phase in [touch::Phase::Start, touch::Phase::End] {
        touch_event(touch::Event {
            id: 2,
            phase,
            position: Vec2::new(30., 110.),
        });
    }
    host_frame(0., 200, 200, || {
        ui.begin(viewport);
        assert_eq!(
            ui.compose(|group| {
                details(group, 2)?;
                group.text(RECT, &huge, TextStyle::default())
            }),
            Err(UiError::Capacity)
        );
        ui.compose(|fallback| {
            fallback.panel(RECT, PANEL)?;
            fallback.label(RECT, "RESULTS UNAVAILABLE", TextStyle::default())?;
            fallback.hud_button(10, OTHER, "CONTINUE", true, ButtonStyle::default())
        })
        .unwrap();
        ui.finish(true);
        assert!(ui.activated(10));
        assert_eq!(ui.interaction().pointer_activated, Some(10));
        assert!(ui.interaction().pointer_consumed);
        let primary = ui.inspect().unwrap().find(|e| e.id == Some(10)).unwrap();
        assert_eq!(primary.label, UiLabel::Full("CONTINUE"));
        assert_eq!(primary.visible_bounds, Some(OTHER));
        ui.paint();
    });
}

#[test]
fn partial_formatter_error_restores_arena_and_ignored_error_stops_later_formatting() {
    struct Broken<'a>(&'a Cell<usize>);
    impl fmt::Display for Broken<'_> {
        fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
            self.0.set(self.0.get() + 1);
            out.write_str("PARTIAL é")?;
            Err(fmt::Error)
        }
    }
    let mut ui = Ui::new();
    ui.set_inspection_enabled(true);
    ui.begin(Viewport::responsive([200.; 2], [0.; 4]).unwrap());
    ui.label(RECT, "PRIOR FULL LABEL", TextStyle::default())
        .unwrap();
    let calls = Cell::new(0);
    assert_eq!(
        ui.compose(|group| {
            group.hud_button(
                10,
                OTHER,
                "PARTIAL FULL LABEL",
                true,
                ButtonStyle::default(),
            )?;
            let _ = group.text_fmt(
                RECT,
                TextStyle::default(),
                format_args!("{}", Broken(&calls)),
            );
            assert_eq!(
                group.text_fmt(
                    RECT,
                    TextStyle::default(),
                    format_args!("{}", Broken(&calls))
                ),
                Err(UiError::Capacity)
            );
            Err::<(), _>(UiError::DuplicateId) // Earlier submission error wins.
        }),
        Err(UiError::Capacity)
    );
    assert_eq!(calls.get(), 1); // Caller side effect is deliberately not undone.
    assert_eq!(ui.inspect().unwrap().len(), 1);
    assert_eq!(
        ui.inspect().unwrap().next().unwrap().label,
        UiLabel::Full("PRIOR FULL LABEL")
    );
    ui.compose(|group| group.hud_button(10, OTHER, "CONTINUE", true, ButtonStyle::default()))
        .unwrap();
    assert_eq!(
        ui.inspect().unwrap().last().unwrap().label,
        UiLabel::Full("CONTINUE")
    );
}

#[test]
fn warmed_nested_modal_rollback_and_fallback_reuse_storage_with_inspection_on_and_off() {
    for inspection in [false, true] {
        let mut ui = Ui::new();
        ui.set_inspection_enabled(inspection);
        let viewport = Viewport::responsive([200.; 2], [0.; 4]).unwrap();
        let huge = "é".repeat(20000);
        let mut bytes = [0; 8192];
        let reserved = ui.reserved_bytes();
        let mut run = |frame: usize| {
            host_frame(0., 200, 200, || {
                ui.begin(viewport);
                ui.hud_button(1, RECT, "PRIOR FULL LABEL é", true, ButtonStyle::default())
                    .unwrap();
                ui.compose(|group| {
                    group.push_clip(viewport.bounds())?;
                    let result = group.compose(|child| {
                        for id in 100..132 {
                            child.modal(id)?;
                        }
                        details(child, frame)?;
                        child.label(RECT, &huge, TextStyle::default())?; // Inspection miss.
                        child.button(10, OTHER, "DUPLICATE", true, ButtonStyle::default())
                    });
                    assert_eq!(result, Err(UiError::DuplicateId));
                    group.label(OTHER, "RESULTS UNAVAILABLE", TextStyle::default())?;
                    group.pop_clip()
                })
                .unwrap();
                assert_eq!(
                    ui.compose(|group| {
                        group.modal(500)?;
                        details(group, frame)?;
                        Err::<(), _>(UiError::InvalidLayout)
                    }),
                    Err(UiError::InvalidLayout)
                );
                ui.finish(true);
                if inspection {
                    let mut elements = ui.inspect().unwrap();
                    assert_eq!(elements.len(), 2);
                    assert_eq!(elements.next().unwrap().id, Some(1));
                    assert_eq!(
                        elements.next().unwrap().label,
                        UiLabel::Full("RESULTS UNAVAILABLE")
                    );
                    ui.write_inspection(&mut bytes).unwrap();
                } else {
                    assert!(ui.inspect().is_none());
                }
                ui.paint();
            })
        };
        for frame in 0..6 {
            run(frame);
        }
        CALLS.with(|calls| calls.set(0));
        RECORD.with(|on| on.set(true));
        for frame in 6..506 {
            run(frame);
        }
        RECORD.with(|on| on.set(false));
        assert_eq!(CALLS.with(Cell::get), 0);
        assert_eq!(ui.reserved_bytes(), reserved);
    }
}
