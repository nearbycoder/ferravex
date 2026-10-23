use ferravex_core::session::{GameSession, SessionState};
use ferravex_play::{objective::*, ui::notices::*};
use ferravex_render::{
    input::{ActionKind, Binding, InputCode, PlayerInput},
    status::*,
    ui::{ButtonStyle, TextStyle, Ui, UiRect, Viewport},
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};
thread_local! {
    static TRACK: Cell<bool> = const { Cell::new(false) };
    static ALLOCS: Cell<usize> = const { Cell::new(0) };
}
struct Allocator;
fn record(p: *mut u8) {
    if !p.is_null() {
        let _ = TRACK.try_with(|t| {
            if t.get() {
                let _ = ALLOCS.try_with(|n| n.set(n.get() + 1));
            }
        });
    }
}
// SAFETY: forwards each allocation contract unchanged to System. TLS counters
// do not allocate and ignore destruction-time access errors.
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(l) };
        record(p);
        p
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        let p = unsafe { System.alloc_zeroed(l) };
        record(p);
        p
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        let p = unsafe { System.realloc(p, l, n) };
        record(p);
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) };
    }
}
#[global_allocator]
static ALLOCATOR: Allocator = Allocator;
const RECT: UiRect = UiRect {
    min: [10., 10.],
    size: [250., 40.],
};
fn facts<'a>(items: &'a [StatusItem<'a>]) -> StatusFacts<'a> {
    StatusFacts {
        items,
        ui: None,
        ui_actions: &[],
    }
}
fn item(id: u64, label: &str) -> StatusItem<'_> {
    StatusItem {
        id,
        label,
        kind: StatusKind::Objective,
    }
}
fn json(bridge: &StatusBridge) -> &str {
    std::str::from_utf8(bridge.bytes()).unwrap()
}
fn begin(ui: &mut Ui) {
    ui.begin(Viewport::responsive([640., 480.], [0.; 4]).unwrap());
}

#[test]
fn headless_game_objective_accepted_notice_ui_and_current_remap_consumer() {
    let mut session = GameSession::new(64., 4, true);
    let mut objectives = ObjectiveGraph::new(
        1,
        [ObjectiveSpec {
            id: 17,
            all: 0,
            any: 0,
            exclusive_group: 0,
            kind: ObjectiveKind::Event { id: 7, count: 1 },
        }],
    )
    .unwrap();
    let mut notices = NoticeQueue::<2, &str>::new();
    let mut player = PlayerInput::transient("journey400-296");
    player
        .action(
            "retry",
            "Retry",
            ActionKind::Button,
            vec![Binding::digital(InputCode::Key(32))],
        )
        .unwrap();
    let mut hint_buffer = [0; 128];
    assert_eq!(
        player.format_hint("retry", &mut hint_buffer).unwrap(),
        "Space"
    );
    player
        .rebind("retry", vec![Binding::digital(InputCode::Key(82))])
        .unwrap();
    let hint = player.format_hint("retry", &mut hint_buffer).unwrap();
    assert_eq!(hint, "R");
    let label = format!("Retry ({hint})");
    let mut bridge = StatusBridge::new(296).unwrap();
    assert_eq!(objectives.inspect(0).unwrap().state, ObjectiveState::Ready);
    bridge
        .publish(Some(facts(&[
            StatusItem {
                id: 1,
                kind: StatusKind::Lifecycle,
                label: "Playing",
            },
            item(17, "Find the authored key"),
        ])))
        .unwrap();
    // Execute only actual scheduled ticks. Result comes from accepted objective
    // events, not from elapsed time, stationary geometry or requested input.
    let mut executed = 0;
    for _context in session.advance_batch(3. / 64.).ticks() {
        if !session.is_playing() {
            break;
        }
        executed += 1;
        let accepted = objectives.event(7, 1, 1).unwrap();
        if accepted.completed != 0 {
            notices
                .insert(
                    Notice {
                        key: Some(17),
                        priority: NoticePriority::High,
                        message: "Key accepted",
                        lifetime_seconds: 2.,
                    },
                    NoticeDuplicate::Keep,
                )
                .unwrap();
            session.finish(true);
        }
    }
    assert_eq!((executed, session.tick()), (1, 3));
    assert_eq!(session.state(), SessionState::Won);
    assert_eq!(
        objectives.inspect(0).unwrap().state,
        ObjectiveState::Complete
    );
    assert_eq!(objectives.event(7, 1, 1).unwrap().completed, 0);
    let mut ui = Ui::new();
    ui.set_inspection_enabled(true);
    begin(&mut ui);
    ui.text(RECT, "Paint-only decoration", TextStyle::default())
        .unwrap();
    ui.button(55, RECT, &label, true, ButtonStyle::default())
        .unwrap();
    ui.button(
        56,
        UiRect {
            min: [10., 70.],
            ..RECT
        },
        "Not approved",
        true,
        ButtonStyle::default(),
    )
    .unwrap();
    ui.finish(false);
    let items = [
        StatusItem {
            id: 1,
            kind: StatusKind::Lifecycle,
            label: "Won",
        },
        item(17, "Find the authored key: complete"),
        StatusItem {
            id: 2,
            kind: StatusKind::Result,
            label: "Objective accepted — victory",
        },
        StatusItem {
            id: 3,
            kind: StatusKind::Notice,
            label: notices.front().unwrap().notice.message,
        },
    ];
    let maps = [UiAction {
        widget_id: 55,
        status_id: u64::MAX,
    }];
    let before = ui.interaction();
    let update = bridge
        .publish(Some(StatusFacts {
            items: &items,
            ui: ui.inspect(),
            ui_actions: &maps,
        }))
        .unwrap();
    assert_eq!(update.serial, 2);
    assert_eq!(update.omitted, 0);
    assert_eq!(ui.interaction(), before);
    assert_eq!(notices.len(), 1);
    assert!(json(&bridge).contains("Retry (R)"));
    assert!(json(&bridge).contains("18446744073709551615"));
    assert!(!json(&bridge).contains("Not approved"));
    assert!(!json(&bridge).contains("decoration"));
    if let Ok(path) = std::env::var("FERRAVEX_STATUS_FIXTURE") {
        std::fs::write(path, bridge.bytes()).unwrap();
    }
    bridge.publish_browser().unwrap();
    assert_eq!(ferravex_status_v1_version(), 1);
    assert_eq!(ferravex_status_v1_len() as usize, bridge.bytes().len());
    assert!(!ferravex_status_v1_ptr().is_null());
    clear_browser(296).unwrap();
}
#[test]
fn available_empty_unavailable_and_reset_are_distinct_with_no_stale_tail() {
    assert!(matches!(
        StatusBridge::new(0),
        Err(StatusError::InvalidOwner)
    ));
    let mut b = StatusBridge::new(1).unwrap();
    assert!(b.bytes().is_empty());
    assert_eq!(b.publish(None).unwrap().serial, 1);
    assert!(json(&b).contains("\"available\":false"));
    assert!(!b.publish(None).unwrap().changed);
    b.publish(Some(facts(&[]))).unwrap();
    assert!(json(&b).contains("\"available\":true"));
    b.publish(Some(facts(&[item(1, &"x".repeat(256))])))
        .unwrap();
    b.publish(Some(facts(&[]))).unwrap();
    assert!(!json(&b).contains("xxxx"));
    let mut small = [0xa5; 2];
    assert_eq!(
        b.write(&mut small),
        Err(StatusError::OutputCapacity {
            required_bytes: b.bytes().len()
        })
    );
    assert_eq!(small, [0xa5; 2]);
    let mut out = [0xa5; MAX_WIRE_BYTES];
    let len = b.write(&mut out).unwrap();
    assert_eq!(&out[..len], b.bytes());
    assert!(out[len..].iter().all(|n| *n == 0xa5));
    b.reset().unwrap();
    assert!(b.bytes().is_empty());
    let u = b.publish(None).unwrap();
    assert_eq!((u.epoch, u.serial), (2, 1));
}
#[test]
fn complete_validation_retains_last_good_and_rejects_duplicates_in_omitted_suffix() {
    let mut b = StatusBridge::new(1).unwrap();
    b.publish(Some(facts(&[item(1, "good")]))).unwrap();
    let good = b.bytes().to_vec();
    let mut inputs: Vec<_> = (0..MAX_SOURCE_ITEMS)
        .map(|i| item(i as u64, "ok"))
        .collect();
    inputs[127].id = 0;
    assert_eq!(
        b.publish(Some(facts(&inputs))),
        Err(StatusError::DuplicateId(0))
    );
    inputs.push(item(129, "extra"));
    assert_eq!(
        b.publish(Some(facts(&inputs))),
        Err(StatusError::SourceItems)
    );
    assert_eq!(
        b.publish(Some(facts(&[item(1, &"x".repeat(MAX_SOURCE_BYTES + 1))]))),
        Err(StatusError::SourceBytes)
    );
    assert_eq!(
        b.publish(Some(facts(&[
            StatusItem {
                id: 1,
                kind: StatusKind::Result,
                label: "A"
            },
            StatusItem {
                id: 2,
                kind: StatusKind::Result,
                label: "B"
            }
        ]))),
        Err(StatusError::MultipleSingletons)
    );
    assert_eq!(b.bytes(), good);
    assert!(!b.publish(Some(facts(&[item(1, "good")]))).unwrap().changed);
}
#[test]
fn bounded_full_source_and_multibyte_truncation_and_worst_escaped_output() {
    let mut b = StatusBridge::new(u64::MAX).unwrap();
    let text = "é".repeat(127) + "🦀";
    let items: Vec<_> = (0..128).map(|i| item(i as u64, &text)).collect();
    assert_eq!(
        b.publish(Some(facts(&items))),
        Err(StatusError::SourceBytes)
    );
    let update = b.publish(Some(facts(&items[..126]))).unwrap();
    assert_eq!((update.omitted, update.shortened), (94, 32));
    assert!(json(&b).contains(&"é".repeat(127)));
    assert!(!json(&b).contains('🦀'));
    assert!(json(&b).contains("\"truncated\":true"));
    let different_suffix = "é".repeat(127) + "界界";
    let same_retained: Vec<_> = (0..126).map(|i| item(i, &different_suffix)).collect();
    assert!(!b.publish(Some(facts(&same_retained))).unwrap().changed);
    let exact = "\0".repeat(256);
    let items: Vec<_> = (0..128).map(|i| item(i, &exact)).collect();
    let update = b.publish(Some(facts(&items))).unwrap();
    assert_eq!((update.omitted, update.shortened), (96, 0));
    assert!(b.bytes().len() > 49000 && b.bytes().len() < MAX_WIRE_BYTES);
    assert!(!json(&b).contains('\0'));
    assert!(json(&b).contains("\\u0000"));
    b.publish(Some(facts(&[item(1, "\"\\\n<script>é🦀\u{1f}")])))
        .unwrap();
    assert!(json(&b).contains("\\\"\\\\\\u000a<script>é🦀\\u001f"));
}
#[test]
fn actual_ui_only_supported_explicit_current_available_labels() {
    let mut b = StatusBridge::new(1).unwrap();
    let mut ui = Ui::new();
    begin(&mut ui);
    let maps = [UiAction {
        widget_id: 1,
        status_id: 4,
    }];
    assert_eq!(
        b.publish(Some(StatusFacts {
            items: &[],
            ui: ui.inspect(),
            ui_actions: &maps
        })),
        Err(StatusError::InspectionUnavailable)
    );
    ui.set_inspection_enabled(true);
    begin(&mut ui);
    ui.button(1, RECT, "Enabled", true, ButtonStyle::default())
        .unwrap();
    ui.button(
        2,
        UiRect {
            min: [10., 70.],
            ..RECT
        },
        "Disabled",
        false,
        ButtonStyle::default(),
    )
    .unwrap();
    ui.button(
        3,
        UiRect {
            min: [-1000.; 2],
            ..RECT
        },
        "Clipped",
        true,
        ButtonStyle::default(),
    )
    .unwrap();
    let maps = [
        UiAction {
            widget_id: 1,
            status_id: 4,
        },
        UiAction {
            widget_id: 2,
            status_id: 5,
        },
        UiAction {
            widget_id: 3,
            status_id: 6,
        },
        UiAction {
            widget_id: 99,
            status_id: 7,
        },
    ];
    b.publish(Some(StatusFacts {
        items: &[],
        ui: ui.inspect(),
        ui_actions: &maps,
    }))
    .unwrap();
    assert!(json(&b).contains("Enabled"));
    assert!(!json(&b).contains("Disabled"));
    assert!(!json(&b).contains("Clipped"));
    let duplicate = [
        UiAction {
            widget_id: 1,
            status_id: 8,
        },
        UiAction {
            widget_id: 1,
            status_id: 9,
        },
    ];
    assert_eq!(
        b.publish(Some(StatusFacts {
            items: &[],
            ui: ui.inspect(),
            ui_actions: &duplicate
        })),
        Err(StatusError::DuplicateWidget(1))
    );
    assert_eq!(
        b.publish(Some(StatusFacts {
            items: &[item(4, "collision")],
            ui: ui.inspect(),
            ui_actions: &maps
        })),
        Err(StatusError::DuplicateId(4))
    );
    ui.modal(90).unwrap(); // prior buttons no longer registered
    b.publish(Some(StatusFacts {
        items: &[],
        ui: ui.inspect(),
        ui_actions: &maps,
    }))
    .unwrap();
    assert!(!json(&b).contains("Enabled"));
    begin(&mut ui);
    ui.button(1, RECT, "Current label", true, ButtonStyle::default())
        .unwrap();
    b.publish(Some(StatusFacts {
        items: &[],
        ui: ui.inspect(),
        ui_actions: &maps,
    }))
    .unwrap();
    assert!(json(&b).contains("Current label"));
}
#[test]
fn missing_ui_full_copy_is_explicit_omission_not_painted_prefix() {
    let mut ui = Ui::new();
    ui.set_inspection_enabled(true);
    begin(&mut ui);
    ui.label(RECT, &"x".repeat(32768), TextStyle::default())
        .unwrap();
    ui.hud_button(
        1,
        UiRect {
            size: [1., 40.],
            ..RECT
        },
        "Hidden full label",
        true,
        ButtonStyle::default(),
    )
    .unwrap();
    let mut b = StatusBridge::new(1).unwrap();
    let update = b
        .publish(Some(StatusFacts {
            items: &[],
            ui: ui.inspect(),
            ui_actions: &[UiAction {
                widget_id: 1,
                status_id: 5,
            }],
        }))
        .unwrap();
    assert_eq!(update.omitted, 1);
    assert!(json(&b).contains("\"items\":[]"));
}
#[test]
fn browser_ownership_noop_clear_and_reset_lifetime() {
    let mut a = StatusBridge::new(1).unwrap();
    let mut b = StatusBridge::new(2).unwrap();
    a.publish(None).unwrap();
    a.publish_browser().unwrap();
    let revision = ferravex_status_v1_revision();
    a.publish_browser().unwrap();
    assert_eq!(ferravex_status_v1_revision(), revision);
    b.publish(None).unwrap();
    assert_eq!(b.publish_browser(), Err(StatusError::OwnerConflict));
    assert_eq!(clear_browser(2), Err(StatusError::OwnerConflict));
    a.reset().unwrap();
    a.publish_browser().unwrap();
    assert_eq!(ferravex_status_v1_len(), 0);
    assert!(ferravex_status_v1_ptr().is_null());
    clear_browser(1).unwrap();
    assert!(!clear_browser(1).unwrap());
    b.publish_browser().unwrap();
    clear_browser(2).unwrap();
}
#[test]
fn warmed_publication_serial_changes_and_browser_copy_allocate_nothing() {
    let mut b = StatusBridge::new(123).unwrap();
    b.publish(None).unwrap();
    b.publish_browser().unwrap();
    let mut output = [0; MAX_WIRE_BYTES];
    ALLOCS.with(|n| n.set(0));
    TRACK.with(|t| t.set(true));
    for i in 0..200 {
        let label = if i % 2 == 0 { "first" } else { "second" };
        let update = b.publish(Some(facts(&[item(1, label)]))).unwrap();
        assert!(update.changed);
        let noop = b.publish(Some(facts(&[item(1, label)]))).unwrap();
        assert!(!noop.changed);
        b.publish_browser().unwrap();
        b.write(&mut output).unwrap();
    }
    TRACK.with(|t| t.set(false));
    assert_eq!(ALLOCS.with(Cell::get), 0);
    clear_browser(123).unwrap();
}
