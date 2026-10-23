use ferravex_render::{
    Color, Vec2,
    input::{host_frame, touch, touch_event},
    ui::{
        ButtonStyle, TextStyle, Ui, UiInspectionExportError, UiLabel, UiRect, UiRole, Viewport,
        text_layout::LineLimit,
    },
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
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
    min: [10., 20.],
    size: [80., 40.],
};

#[test]
fn export_is_lossless_versioned_and_atomic_for_caller_capacity() {
    let mut ui = Ui::new();
    let mut bytes = [0xa5; 8192];
    assert_eq!(
        ui.write_inspection(&mut bytes),
        Err(UiInspectionExportError::Disabled)
    );
    assert!(bytes.iter().all(|b| *b == 0xa5));
    ui.set_inspection_enabled(true);
    assert!(ui.inspect().is_none());
    ui.begin(Viewport::responsive([200.; 2], [0.; 4]).unwrap());
    let empty = ui.write_inspection(&mut bytes).unwrap();
    assert_eq!(&bytes[..empty], b"{\"schema\":1,\"elements\":[]}");
    ui.panel(RECT, Color::new(1., 0., 0., 1.)).unwrap();
    let panel = ui.write_inspection(&mut bytes).unwrap();
    assert_eq!(
        std::str::from_utf8(&bytes[..panel]).unwrap(),
        "{\"schema\":1,\"elements\":[{\"id\":null,\"role\":\"panel\",\"label\":\"\",\"painted_label\":\"\",\"bounds\":[10,20,80,40],\"visible_bounds\":[10,20,80,40],\"enabled\":false,\"focused\":false,\"interaction_registered\":false}]}"
    );
    let label = "Full é🦀\n\t\"\\\0\u{1f} original label";
    ui.hud_button(u64::MAX, RECT, label, false, ButtonStyle::default())
        .unwrap();
    let huge = "x".repeat(32769);
    ui.label(RECT, &huge, TextStyle::default()).unwrap();
    let mut tiny = [0x7b; 3];
    let Err(UiInspectionExportError::Capacity { required_bytes }) = ui.write_inspection(&mut tiny)
    else {
        panic!("capacity must report exact required bytes")
    };
    assert_eq!(tiny, [0x7b; 3]);
    bytes.fill(0xa5);
    let written = ui.write_inspection(&mut bytes).unwrap();
    assert_eq!(written, required_bytes);
    assert!(bytes[written..].iter().all(|b| *b == 0xa5));
    let json = std::str::from_utf8(&bytes[..written]).unwrap();
    assert!(json.contains("\"id\":\"18446744073709551615\""));
    assert!(json.contains("Full é🦀\\u000a\\u0009\\\"\\\\\\u0000\\u001f original label"));
    assert!(json.contains("\"label\":null,\"label_required_bytes\":32769"));
    let mut exact = vec![0; written];
    assert_eq!(ui.write_inspection(&mut exact), Ok(written));
    assert_eq!(exact, bytes[..written]);
    let mut short = vec![0x77; written - 1];
    assert_eq!(
        ui.write_inspection(&mut short),
        Err(UiInspectionExportError::Capacity { required_bytes })
    );
    assert!(short.iter().all(|b| *b == 0x77));
    ui.set_inspection_enabled(false);
    assert_eq!(
        ui.write_inspection(&mut short),
        Err(UiInspectionExportError::Disabled)
    );
}

#[test]
fn all_1024_shortened_labels_fit_metadata_and_reset_without_stale_ids() {
    let mut ui = Ui::new();
    ui.set_inspection_enabled(true);
    let viewport = Viewport::responsive([200.; 2], [0.; 4]).unwrap();
    ui.begin(viewport);
    let invisible = UiRect {
        size: [0.; 2],
        ..RECT
    };
    for id in 0..1024 {
        ui.hud_button(id, invisible, "é🦀", true, ButtonStyle::default())
            .unwrap();
    }
    for (id, element) in ui.inspect().unwrap().enumerate() {
        assert_eq!(element.id, Some(id as u64));
        assert_eq!(element.role, UiRole::Button);
        assert_eq!(element.label, UiLabel::Full("é🦀"));
        assert_eq!(element.painted_label, "");
        assert_eq!(element.visible_bounds, None);
    }
    assert_eq!(ui.inspect().unwrap().len(), 1024);
    assert_eq!(
        ui.hud_button(1024, invisible, "overflow", true, ButtonStyle::default()),
        Err(ferravex_render::ui::UiError::Capacity)
    );
    ui.modal(4096).unwrap();
    assert_eq!(ui.inspect().unwrap().len(), 1025);
    let last = ui.inspect().unwrap().last().unwrap();
    assert_eq!(last.role, UiRole::ModalBlocker);
    assert_eq!(last.id, Some(4096));
    ui.begin(viewport);
    assert_eq!(ui.inspect().unwrap().count(), 0);
    ui.button(2000, RECT, "fresh", true, ButtonStyle::default())
        .unwrap();
    let element = ui.inspect().unwrap().next().unwrap();
    assert_eq!(element.label, UiLabel::Full("fresh"));
    assert_eq!(element.id, Some(2000));
}

#[test]
fn inspect_export_full_utf8_and_paint_allocate_nothing_after_warmup() {
    let mut ui = Ui::new();
    ui.set_inspection_enabled(true);
    let viewport = Viewport::responsive([320., 240.], [0.; 4]).unwrap();
    let reserved = ui.reserved_bytes();
    let full = "é🦀".repeat(5462); // 32,772 bytes: explicit semantic capacity miss.
    let mut bytes = [0; 8192];
    let mut run = |frame: usize| {
        touch_event(touch::Event {
            id: frame as u64,
            phase: touch::Phase::Start,
            position: Vec2::new(40., 40.),
        });
        touch_event(touch::Event {
            id: frame as u64,
            phase: touch::Phase::End,
            position: Vec2::new(40., 40.),
        });
        host_frame(0., 320, 240, || {
            ui.begin(viewport);
            ui.hud_button(
                1,
                RECT,
                "Inspect full label é🦀\nsecond line",
                true,
                ButtonStyle::default(),
            )
            .unwrap();
            ui.label(RECT, &full, TextStyle::default()).unwrap();
            ui.text_fmt(
                UiRect {
                    min: [10., 70.],
                    ..RECT
                },
                TextStyle::default(),
                format_args!("FRAME {frame}"),
            )
            .unwrap();
            ui.wrapped_text(
                RECT,
                "Full wrapped\nsource\ncopy",
                TextStyle::default(),
                LineLimit::Ellipsis(1),
            )
            .unwrap();
            ui.finish(true);
            assert!(ui.activated(1));
            let ownership = ui.interaction();
            let mut elements = ui.inspect().unwrap();
            assert_eq!(elements.len(), 4);
            let button = elements.next().unwrap();
            assert_eq!(
                button.label,
                UiLabel::Full("Inspect full label é🦀\nsecond line")
            );
            assert!(button.focused);
            assert_eq!(
                elements.next().unwrap().label,
                UiLabel::Unavailable {
                    required_bytes: full.len()
                }
            );
            assert_eq!(
                elements.last().unwrap().label,
                UiLabel::Full("Full wrapped\nsource\ncopy")
            );
            let len = ui.write_inspection(&mut bytes).unwrap();
            assert!(len > 500 && len < bytes.len());
            assert_eq!(ui.interaction(), ownership);
            ui.paint();
        });
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
