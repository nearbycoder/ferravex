//! Executed first-party card recipes; synthetic native input, no window/device.
use ferravex_play::ui::scroll::ScrollState;
use ferravex_render::{
    Vec2,
    input::{host_frame, touch, touch_event},
    ui::{
        ButtonStyle, PANEL, TextStyle, Ui, UiError, UiLabel, UiRect, Viewport,
        cards::{
            CardAction, CardContent, CardLayout, CardSection, MAX_CARD_ACTIONS, MAX_CARD_BYTES,
            MAX_CARD_SECTIONS,
        },
        stats::{MAX_STAT_ROWS, StatRow},
        text_layout::LineLimit,
    },
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};

const BOUNDS: UiRect = UiRect {
    min: [8., 12.],
    size: [304., 456.],
};
const ACTIONS: [CardAction<'static>; 3] = [
    CardAction {
        id: 901,
        label: "RESUME",
        enabled: true,
    },
    CardAction {
        id: 902,
        label: "CONTROLS",
        enabled: false,
    },
    CardAction {
        id: 903,
        label: "RETRY",
        enabled: true,
    },
];
fn source<'a>(sections: &'a [CardSection<'a>], actions: &'a [CardAction<'a>]) -> CardContent<'a> {
    CardContent {
        title: "PAUSED",
        sections,
        actions,
        primary: 901,
        blocker: 900,
    }
}
fn viewport() -> Viewport {
    Viewport::responsive([320., 480.], [0.; 4]).unwrap()
}
fn inject(id: u64, phase: touch::Phase, rect: UiRect) {
    touch_event(touch::Event {
        id,
        phase,
        position: Vec2::new(rect.min[0] + 2., rect.min[1] + 2.),
    });
}
type Snapshot = Vec<(Option<u64>, String, UiRect, Option<UiRect>, bool)>;
fn snapshot(ui: &Ui) -> Snapshot {
    ui.inspect()
        .unwrap()
        .map(|e| {
            (
                e.id,
                e.painted_label.to_owned(),
                e.bounds,
                e.visible_bounds,
                e.interaction_registered,
            )
        })
        .collect()
}

#[test]
fn native_start_pause_results_recipe_with_real_touch_journal_and_fixed_action_geometry() {
    let rows = [StatRow {
        label: "TIME",
        value: "1:00.001",
        description: Some("é日 result"),
    }];
    let sections = [
        CardSection::Body("Collect crystals. Return to the entrance."),
        CardSection::Stats(&rows),
    ];
    let mut ui = Ui::new();
    ui.set_inspection_enabled(true);
    for width in [320., 960.] {
        for (title, label, count) in [
            ("CRYSTAL CAVERN", "START", 1),
            ("PAUSED", "RESUME", 3),
            ("EXPEDITION COMPLETE", "RETRY", 3),
        ] {
            let actions = [
                CardAction {
                    label,
                    ..ACTIONS[0]
                },
                ACTIONS[1],
                ACTIONS[2],
            ];
            let viewport = Viewport::responsive([width, 480.], [8., 12., 8., 12.]).unwrap();
            let card = CardLayout::new(
                viewport.bounds(),
                CardContent {
                    title,
                    ..source(&sections, &actions[..count])
                },
            )
            .unwrap();
            for action in &actions[..count] {
                let rect = card.action_rect(action.id).unwrap();
                assert_eq!(rect.size[1], 44.);
                assert!(rect.size[0] >= 44.);
                let physical = UiRect {
                    min: std::array::from_fn(|a| {
                        viewport.pixels().min[a] + rect.min[a] * viewport.scale()
                    }),
                    size: rect.size,
                };
                inject(action.id, touch::Phase::Start, physical);
                inject(action.id, touch::Phase::End, physical);
                host_frame(0., width as u32, 480, || {
                    ui.begin(viewport);
                    assert_eq!(ui.card(&card, None).unwrap().omitted_sections, 0);
                    ui.finish(true);
                    assert_eq!(ui.activated(action.id), action.enabled);
                    assert!(!ui.activated(999));
                    let element = ui
                        .inspect()
                        .unwrap()
                        .find(|e| e.id == Some(action.id))
                        .unwrap();
                    assert_eq!(element.bounds, rect);
                    assert_eq!(element.visible_bounds, Some(rect));
                    assert_eq!(element.painted_label, action.label);
                    assert_eq!(element.label, UiLabel::Full(action.label));
                    assert_eq!(element.enabled, action.enabled);
                    assert_eq!(
                        ui.interaction().pointer_activated,
                        action.enabled.then_some(action.id)
                    );
                    ui.paint();
                });
            }
            assert_eq!(card.action_rect(999), None);
        }
    }
}

#[test]
fn bounded_source_unknown_primary_labels_and_impossible_bounds_fail_before_insertion() {
    let body = [CardSection::Body("BODY")];
    let base = source(&body, &ACTIONS);
    for bounds in [
        UiRect {
            size: [320., 80.],
            ..BOUNDS
        },
        UiRect {
            size: [0., 480.],
            ..BOUNDS
        },
        UiRect {
            min: [f32::NAN, 0.],
            ..BOUNDS
        },
        UiRect {
            min: [1e30; 2],
            size: [320., 480.],
        },
    ] {
        assert_eq!(
            CardLayout::new(bounds, base).unwrap_err(),
            UiError::InvalidLayout
        );
    }
    assert_eq!(
        CardLayout::new(
            BOUNDS,
            CardContent {
                primary: 999,
                ..base
            }
        )
        .unwrap_err(),
        UiError::InvalidLayout
    );
    assert_eq!(
        CardLayout::new(
            BOUNDS,
            CardContent {
                blocker: 901,
                ..base
            }
        )
        .unwrap_err(),
        UiError::DuplicateId
    );
    assert_eq!(
        CardLayout::new(
            BOUNDS,
            CardContent {
                title: " \n",
                ..base
            }
        )
        .unwrap_err(),
        UiError::InvalidLayout
    );
    for label in [
        "",
        " ",
        "RE\nTRY",
        "RE\tTRY",
        "RE\u{2028}TRY",
        "THIS ESSENTIAL LABEL CANNOT FIT IN ONE ACTION",
    ] {
        let actions = [CardAction {
            label,
            ..ACTIONS[0]
        }];
        assert_eq!(
            CardLayout::new(BOUNDS, source(&body, &actions)).unwrap_err(),
            UiError::InvalidLayout
        );
    }
    let duplicate = [ACTIONS[0]; 2];
    assert_eq!(
        CardLayout::new(BOUNDS, source(&body, &duplicate)).unwrap_err(),
        UiError::DuplicateId
    );
    let sections = [CardSection::Body(""); MAX_CARD_SECTIONS + 1];
    assert_eq!(
        CardLayout::new(BOUNDS, source(&sections, &ACTIONS)).unwrap_err(),
        UiError::Capacity
    );
    let actions = [ACTIONS[0]; MAX_CARD_ACTIONS + 1];
    assert_eq!(
        CardLayout::new(BOUNDS, source(&body, &actions)).unwrap_err(),
        UiError::Capacity
    );
    let huge = "é".repeat(MAX_CARD_BYTES / 2);
    let sections = [CardSection::Body(&huge)];
    assert_eq!(
        CardLayout::new(BOUNDS, source(&sections, &ACTIONS)).unwrap_err(),
        UiError::Capacity
    );
    let rows = [StatRow {
        label: "L",
        value: "V",
        description: None,
    }; MAX_STAT_ROWS + 1];
    let sections = [CardSection::Stats(&rows)];
    assert_eq!(
        CardLayout::new(BOUNDS, source(&sections, &ACTIONS)).unwrap_err(),
        UiError::Capacity
    );
    let sections = [
        CardSection::Stats(&rows[..40]),
        CardSection::Stats(&rows[..25]),
    ];
    assert_eq!(
        CardLayout::new(BOUNDS, source(&sections, &ACTIONS)).unwrap_err(),
        UiError::Capacity
    );
    let mut ui = Ui::new();
    ui.set_inspection_enabled(true);
    ui.begin(viewport());
    assert!(
        ui.compose(|g| CardLayout::new(
            UiRect {
                size: [320., 40.],
                ..BOUNDS
            },
            base
        )?
        .submit(g, None))
            .is_err()
    );
    assert_eq!(ui.inspect().unwrap().len(), 0);
    // Full eight-action maximum is registered production code, not just layout.
    let actions: [_; MAX_CARD_ACTIONS] = std::array::from_fn(|i| CardAction {
        id: 901 + i as u64,
        label: "GO",
        enabled: true,
    });
    let card = CardLayout::new(BOUNDS, source(&[], &actions)).unwrap();
    ui.card(&card, None).unwrap();
    assert_eq!(
        ui.inspect()
            .unwrap()
            .filter(|e| e.role == ferravex_render::ui::UiRole::Button)
            .count(),
        MAX_CARD_ACTIONS
    );
}

#[test]
fn long_unicode_copy_and_max_rows_scroll_with_full_logical_lines_and_explicit_omission() {
    let copy = "é日 ALPHA BETA\n\n".repeat(90);
    let rows: [_; MAX_STAT_ROWS] = std::array::from_fn(|i| StatRow {
        label: "TOTAL",
        value: if i == MAX_STAT_ROWS - 1 {
            "LAST VALUE"
        } else {
            "18446744073709551615"
        },
        description: Some("Accepted only."),
    });
    let sections = [CardSection::Body(&copy), CardSection::Stats(&rows)];
    let card = CardLayout::new(BOUNDS, source(&sections, &ACTIONS)).unwrap();
    assert_eq!(card.section_rect(2), None);
    let mut ui = Ui::new();
    ui.set_inspection_enabled(true);
    let mut scroll =
        ScrollState::new(card.content_bounds().size[1], card.content_extent()).unwrap();
    ui.begin(viewport());
    let omitted = ui.card(&card, None).unwrap();
    assert_eq!(
        (
            omitted.omitted_sections,
            omitted.omitted_stat_rows,
            omitted.scrollable
        ),
        (2, MAX_STAT_ROWS, false)
    );
    assert!(
        ui.inspect()
            .unwrap()
            .any(|e| e.painted_label == "CONTENT OMITTED")
    );
    assert!(
        ui.inspect()
            .unwrap()
            .all(|e| !e.painted_label.contains("184467"))
    );
    // A fractional offset deliberately clips the first line; logical bounds are
    // preserved and shared with the task274 transformation, not cropped/reflowed.
    scroll.set_offset(9.5).unwrap();
    ui.begin(viewport());
    let ancestor = UiRect {
        min: [12., 20.],
        size: [296., 448.],
    };
    ui.push_clip(ancestor).unwrap();
    let report = ui.card(&card, Some(&scroll)).unwrap();
    assert_eq!(
        (
            report.omitted_sections,
            report.omitted_stat_rows,
            report.scrollable
        ),
        (0, 0, true)
    );
    let first = ui
        .inspect()
        .unwrap()
        .find(|e| e.painted_label == "é日 ALPHA BETA")
        .unwrap();
    let expected = scroll
        .translate_rect(
            card.content_bounds(),
            UiRect {
                min: [0.; 2],
                size: [card.content_bounds().size[0], 18.],
            },
        )
        .unwrap();
    assert_eq!(first.bounds, expected);
    assert_eq!(
        first.visible_bounds,
        expected
            .intersect(card.content_bounds())
            .and_then(|r| r.intersect(ancestor))
    );
    assert_eq!(first.label, UiLabel::Full("é日 ALPHA BETA"));
    ui.pop_clip();
    ui.paint();
    scroll.set_offset(f32::MAX).unwrap();
    ui.begin(viewport());
    ui.card(&card, Some(&scroll)).unwrap();
    assert!(
        ui.inspect()
            .unwrap()
            .any(|e| e.painted_label == "LAST VALUE")
    );
    // A mismatched/reset state cannot be used with a stale layout.
    let before = snapshot(&ui);
    let bad = ScrollState::default();
    assert_eq!(ui.card(&card, Some(&bad)), Err(UiError::InvalidLayout));
    assert_eq!(snapshot(&ui), before);
    scroll.reset();
    assert_eq!(scroll.offset(), 0.);
    let short = [CardSection::Body("SHORT")];
    let next = CardLayout::new(BOUNDS, source(&short, &ACTIONS[..1])).unwrap();
    scroll
        .set_extents(next.content_bounds().size[1], next.content_extent())
        .unwrap();
    ui.begin(viewport());
    assert!(!ui.card(&next, Some(&scroll)).unwrap().scrollable);
    assert!(ui.inspect().unwrap().any(|e| e.painted_label == "SHORT"));
    assert!(
        ui.inspect()
            .unwrap()
            .all(|e| !e.painted_label.contains("OMITTED") && e.id != Some(903))
    );
}

#[test]
fn very_short_cards_keep_primary_and_notice_readable_or_reject_without_partial_card() {
    let long = "LONG COPY ".repeat(120);
    let sections = [CardSection::Body(&long)];
    let bounds = UiRect {
        min: [0.; 2],
        size: [320., 170.],
    };
    let card = CardLayout::new(bounds, source(&sections, &ACTIONS[..1])).unwrap();
    let mut ui = Ui::new();
    ui.set_inspection_enabled(true);
    ui.begin(viewport());
    assert_eq!(ui.card(&card, None).unwrap().omitted_sections, 1);
    let primary = ui.inspect().unwrap().find(|e| e.id == Some(901)).unwrap();
    assert_eq!(primary.bounds.size[1], 44.);
    assert_eq!(primary.visible_bounds, Some(primary.bounds));
    assert!(
        ui.inspect()
            .unwrap()
            .any(|e| e.painted_label == "CONTENT OMITTED" && e.bounds.size[1] == 18.)
    );
    assert_eq!(
        CardLayout::new(
            UiRect {
                size: [320., 140.],
                ..bounds
            },
            source(&sections, &ACTIONS[..1])
        )
        .unwrap_err(),
        UiError::InvalidLayout
    );
    // Narrow but fully fitting cards need no impossible overflow marker.
    assert!(
        CardLayout::new(
            UiRect {
                size: [130., 180.],
                ..bounds
            },
            CardContent {
                title: "GO",
                ..source(
                    &[],
                    &[CardAction {
                        label: "GO",
                        ..ACTIONS[0]
                    }]
                )
            }
        )
        .is_ok()
    );
}

#[test]
fn modal_command_text_and_vertex_exhaustion_roll_back_inspection_and_known_ids() {
    let sections = [CardSection::Body("REQUIRED BODY COPY")];
    let card = CardLayout::new(BOUNDS, source(&sections, &ACTIONS)).unwrap();
    let mut ui = Ui::new();
    ui.set_inspection_enabled(true);
    let prior = UiRect {
        min: [20.; 2],
        size: [80., 44.],
    };
    for budget in 0..3 {
        host_frame(0., 320, 480, || {
            ui.begin(viewport());
            ui.button(10, prior, "PRIOR", true, ButtonStyle::default())
                .unwrap();
            match budget {
                0 => {
                    for _ in 0..1021 {
                        ui.panel(prior, PANEL).unwrap();
                    }
                }
                1 => {
                    ui.wrapped_text(
                        prior,
                        &"x".repeat(32760),
                        TextStyle::default(),
                        LineLimit::Clip(0),
                    )
                    .unwrap();
                }
                _ => {
                    ui.wrapped_text(
                        UiRect {
                            size: [1e6, 18.],
                            ..prior
                        },
                        &"x".repeat(1540),
                        TextStyle::default(),
                        LineLimit::Unlimited,
                    )
                    .unwrap();
                }
            }
            let before = snapshot(&ui);
            assert_eq!(ui.card(&card, None), Err(UiError::Capacity));
            assert_eq!(snapshot(&ui), before);
            ui.finish(true);
            assert!(!ui.activated(901));
            assert!(!ui.activated(900));
            assert!(
                ui.inspect()
                    .unwrap()
                    .find(|e| e.id == Some(10))
                    .unwrap()
                    .interaction_registered
            );
            ui.paint();
        });
    }
    // Failure restores a previous modal's registry rather than reviving its HUD.
    ui.begin(viewport());
    ui.button(5, prior, "HIDDEN HUD", true, ButtonStyle::default())
        .unwrap();
    ui.modal(50).unwrap();
    ui.button(10, prior, "PRIOR", true, ButtonStyle::default())
        .unwrap();
    let before = snapshot(&ui);
    assert_eq!(
        ui.compose(|g| {
            card.submit(g, None)?;
            Err::<(), _>(UiError::DuplicateId)
        }),
        Err(UiError::DuplicateId)
    );
    assert_eq!(snapshot(&ui), before);
    assert!(
        !ui.inspect()
            .unwrap()
            .find(|e| e.id == Some(5))
            .unwrap()
            .interaction_registered
    );
    // The rejected group did not steal IDs or clip depth.
    ui.card(&card, None).unwrap();
    assert!(
        ui.inspect()
            .unwrap()
            .any(|e| e.id == Some(901) && e.interaction_registered)
    );
}

thread_local! { static ALLOCATIONS: Cell<Option<usize>> = const { Cell::new(None) }; }
struct Allocator;
fn counted(pointer: *mut u8) {
    if !pointer.is_null() {
        let _ = ALLOCATIONS.try_with(|count| {
            if let Some(n) = count.get() {
                count.set(Some(n + 1));
            }
        });
    }
}
// SAFETY: Forward System's unchanged contract; TLS counting does not allocate.
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        counted(p);
        p
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc_zeroed(layout) };
        counted(p);
        p
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        let p = unsafe { System.realloc(p, l, n) };
        counted(p);
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) };
    }
}
#[global_allocator]
static ALLOCATOR: Allocator = Allocator;
struct StopCounting;
impl Drop for StopCounting {
    fn drop(&mut self) {
        ALLOCATIONS.with(|count| count.set(None));
    }
}

#[test]
fn warmed_real_frames_layout_scroll_rollback_fallback_inspection_and_paint_do_not_allocate() {
    let copy = "é日 long briefing\n".repeat(180);
    let rows = [StatRow {
        label: "COUNT",
        value: "18446744073709551615",
        description: None,
    }; MAX_STAT_ROWS];
    let sections = [CardSection::Body(&copy), CardSection::Stats(&rows)];
    let fallback = [CardSection::Body("RESULTS UNAVAILABLE")];
    for inspection in [false, true] {
        let mut ui = Ui::new();
        ui.set_inspection_enabled(inspection);
        let reserved = ui.reserved_bytes();
        let mut scroll = ScrollState::default();
        let mut buffer = [0; 32768];
        let mut run = |frame: usize| {
            host_frame(0., 960, 480, || {
                let bounds = if frame.is_multiple_of(2) {
                    BOUNDS
                } else {
                    UiRect {
                        size: [920., 456.],
                        ..BOUNDS
                    }
                };
                let card = CardLayout::new(bounds, source(&sections, &ACTIONS)).unwrap();
                scroll
                    .set_extents(card.content_bounds().size[1], card.content_extent())
                    .unwrap();
                scroll
                    .set_offset(if frame.is_multiple_of(3) {
                        0.
                    } else {
                        f32::MAX
                    })
                    .unwrap();
                ui.begin(Viewport::responsive([960., 480.], [0.; 4]).unwrap());
                assert_eq!(
                    ui.compose(|g| {
                        card.submit(g, Some(&scroll))?;
                        Err::<(), _>(UiError::InvalidLayout)
                    }),
                    Err(UiError::InvalidLayout)
                );
                let fallback_card =
                    CardLayout::new(bounds, source(&fallback, &ACTIONS[..1])).unwrap();
                ui.card(&fallback_card, None).unwrap();
                ui.finish(true);
                if inspection {
                    ui.write_inspection(&mut buffer).unwrap();
                }
                ui.paint();
            });
        };
        for frame in 0..6 {
            run(frame);
        }
        ALLOCATIONS.with(|count| count.set(Some(0)));
        let guard = StopCounting;
        for frame in 6..306 {
            run(frame);
        }
        let calls = ALLOCATIONS.with(|count| count.get().unwrap());
        drop(guard);
        assert_eq!(calls, 0);
        assert_eq!(ui.reserved_bytes(), reserved);
    }
}

#[test]
fn exact_byte_section_capacity_fractional_reflow_and_reset_use_only_current_geometry() {
    let mut ui = Ui::new();
    ui.set_inspection_enabled(true);
    let overhead =
        source(&[], &ACTIONS).title.len() + ACTIONS.iter().map(|a| a.label.len()).sum::<usize>();
    let copy = "X".repeat(MAX_CARD_BYTES - overhead);
    let sections = [CardSection::Body(&copy)];
    let card = CardLayout::new(BOUNDS, source(&sections, &ACTIONS)).unwrap();
    let state = ScrollState::new(card.content_bounds().size[1], card.content_extent()).unwrap();
    ui.begin(viewport());
    assert!(ui.card(&card, Some(&state)).unwrap().scrollable);
    let excessive = format!("{copy}X");
    assert_eq!(
        CardLayout::new(BOUNDS, source(&[CardSection::Body(&excessive)], &ACTIONS)).unwrap_err(),
        UiError::Capacity
    );
    let sections = [CardSection::Body("OK"); MAX_CARD_SECTIONS];
    for width in [287.5, 304., 311.25, 920.5] {
        for origin in [[-3.25, -7.75], [0., 0.], [12.25, 16.5]] {
            let bounds = UiRect {
                min: origin,
                size: [width, 456.25],
            };
            let card = CardLayout::new(bounds, source(&sections, &ACTIONS)).unwrap();
            let clip = UiRect {
                min: [-100.; 2],
                size: [2000.; 2],
            };
            let viewport = Viewport::fit([2000.; 2], [2000.; 2], [0.; 4]).unwrap();
            ui.begin(viewport);
            ui.push_clip(clip).unwrap();
            assert_eq!(ui.card(&card, None).unwrap().omitted_sections, 0);
            for (i, action) in ACTIONS.iter().enumerate() {
                let rect = card.action_rect(action.id).unwrap();
                for axis in 0..2 {
                    assert!(rect.min[axis] >= bounds.min[axis] + 16.);
                    assert!(
                        rect.min[axis] + rect.size[axis]
                            <= bounds.min[axis] + bounds.size[axis] - 16.
                    );
                }
                for other in &ACTIONS[..i] {
                    assert_eq!(rect.intersect(card.action_rect(other.id).unwrap()), None);
                }
                let element = ui
                    .inspect()
                    .unwrap()
                    .find(|e| e.id == Some(action.id))
                    .unwrap();
                assert_eq!(element.bounds, rect);
                assert_eq!(
                    element.visible_bounds,
                    rect.intersect(viewport.bounds())
                        .and_then(|r| r.intersect(clip))
                );
            }
            ui.pop_clip();
        }
    }
    ui.begin(viewport());
    let reset = CardLayout::new(BOUNDS, source(&[], &ACTIONS[..1])).unwrap();
    ui.card(&reset, None).unwrap();
    assert_eq!(
        ui.inspect()
            .unwrap()
            .filter(|e| e.painted_label == "OK")
            .count(),
        0
    );
    assert!(ui.inspect().unwrap().all(|e| e.id != Some(903)));
}

#[test]
fn exhausted_inherited_clip_stack_rejects_card_without_partial_modal_or_commands() {
    let card = CardLayout::new(BOUNDS, source(&[CardSection::Body("BODY")], &ACTIONS)).unwrap();
    let mut ui = Ui::new();
    ui.set_inspection_enabled(true);
    ui.begin(viewport());
    ui.button(1, BOUNDS, "PRIOR", true, ButtonStyle::default())
        .unwrap();
    for _ in 0..15 {
        ui.push_clip(BOUNDS).unwrap();
    }
    let before = snapshot(&ui);
    assert_eq!(ui.card(&card, None), Err(UiError::Capacity));
    assert_eq!(snapshot(&ui), before);
    for _ in 0..15 {
        ui.pop_clip();
    }
    ui.card(&card, None).unwrap();
    let primary = ui.inspect().unwrap().find(|e| e.id == Some(901)).unwrap();
    assert_eq!(primary.visible_bounds, Some(card.action_rect(901).unwrap()));
}
