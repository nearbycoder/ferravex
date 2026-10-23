//! Executed opt-in recipe: live session preferences -> validated theme -> measured
//! text + flow -> retained Ui paint/inspection and the very same touch rectangles.
//! Native/headless synthetic input, not a browser/physical-device playtest.
use ferravex_play::{
    feedback::FeedbackPolicy,
    ui::{
        flow::{self, FlowItem, FlowOptions},
        theme::{ColorRole, ThemeMode, UiTheme},
    },
};
use ferravex_render::{
    input::{host_frame, touch, touch_event},
    preferences::{self, UiPreferenceError, UiPreferences},
    ui::{
        ButtonStyle, TextStyle, Ui, UiError, UiRect, Viewport,
        text_layout::{LineLimit, WrappedText},
    },
    vec2,
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};

#[test]
fn defaults_and_all_valid_scale_contrast_combinations_use_real_validated_tokens() {
    assert_eq!(preferences::ui_preferences(), UiPreferences::default());
    assert_eq!(
        UiPreferences::default().theme().unwrap(),
        UiTheme::preset(ThemeMode::Normal)
    );
    for scale in [1., 1.5, 2.] {
        for contrast in [0, 1] {
            let choice = UiPreferences::from_snapshot(1, scale, contrast).unwrap();
            assert_eq!(
                (choice.version(), choice.text_scale(), choice.contrast()),
                (1, scale, contrast)
            );
            let theme = choice.theme().unwrap();
            assert_eq!(theme.types().body, 18. * scale);
            assert_eq!(theme.types().label, 16. * scale);
            assert_eq!(theme.types().heading, 24. * scale);
            assert_eq!(theme.spacing().control_height, 44. * scale);
            assert_eq!(theme.spacing().compact_gap, 8. * scale);
            assert_eq!(
                UiTheme::try_new(
                    theme.mode(),
                    theme.types(),
                    theme.spacing(),
                    theme.palette()
                ),
                Ok(theme)
            );
            assert!(
                theme
                    .role_contrast(ColorRole::Text, ColorRole::Surface)
                    .unwrap()
                    >= if contrast == 1 { 7. } else { 4.5 }
            );
            preferences::apply_ui_preferences(choice);
            assert_eq!(preferences::ui_preferences(), choice);
            let unchanged = preferences::apply_ui_preferences(choice);
            assert!(!unchanged.changed && !unchanged.reflow_required);
        }
    }
    let reset = preferences::apply_ui_preferences(UiPreferences::default());
    assert!(reset.changed && reset.reflow_required);
    assert_eq!(preferences::ui_preferences(), UiPreferences::default());
}

#[test]
fn rejected_version_scale_and_last_field_leave_ui_and_legacy_channels_unchanged() {
    assert_eq!(preferences::apply_ui_snapshot_v1(1, 1.5, 1), 1);
    let previous = preferences::ui_preferences();
    let legacy = preferences::current();
    for motion in [false, true] {
        for flash in [false, true] {
            let policy = FeedbackPolicy::new(motion, flash, 0.375).unwrap();
            preferences::set_feedback_policy(policy);
            for version in [0, 2, u32::MAX] {
                assert_eq!(
                    UiPreferences::from_snapshot(version, 1., 0),
                    Err(UiPreferenceError::Version)
                );
                assert_eq!(preferences::apply_ui_snapshot_v1(version, 1., 0), 0);
            }
            for scale in [
                f32::NAN,
                f32::INFINITY,
                f32::NEG_INFINITY,
                -0.,
                1_f32.next_down(),
                2_f32.next_up(),
                f32::MAX,
            ] {
                assert_eq!(
                    UiPreferences::from_snapshot(1, scale, 0),
                    Err(UiPreferenceError::TextScale)
                );
                assert_eq!(preferences::apply_ui_snapshot_v1(1, scale, 0), 0);
            }
            for contrast in [2, u32::MAX] {
                // Earlier fields are valid AND changed, but the final field fails.
                assert_eq!(
                    UiPreferences::from_snapshot(1, 2., contrast),
                    Err(UiPreferenceError::Contrast)
                );
                assert_eq!(preferences::apply_ui_snapshot_v1(1, 2., contrast), 0);
            }
            assert_eq!(preferences::ui_preferences(), previous);
            assert_eq!(preferences::feedback_policy(), policy);
            assert_eq!(preferences::current(), legacy);
            assert_eq!(preferences::apply_ui_snapshot_v1(1, 1.5, 1), 1);
            assert_eq!(preferences::feedback_policy(), policy);
            preferences::try_apply(legacy).unwrap();
            assert_eq!(
                preferences::ui_preferences(),
                previous,
                "legacy reapplication cannot reset UI"
            );
        }
    }
}

const COPY: &str = "Collect the amber keys before opening the gate.";
const LABELS: [&str; 2] = ["RESUME", "CONTROLS"];

// A first-party consumer recipe, intentionally using existing Ui/flow APIs. A
// caller with less space must add scrolling/repositioning or report overflow;
// this recipe rejects before queuing rather than shrinking/omitting copy.
fn compose(ui: &mut Ui, viewport: Viewport) -> Result<([UiRect; 2], usize), UiError> {
    let theme = preferences::ui_preferences().theme().unwrap();
    let spacing = theme.spacing();
    let bounds = viewport.bounds().inset([spacing.inset; 4])?;
    let size = theme.types().body;
    let measured = WrappedText::new(COPY, bounds.size[0], size)?.measure();
    let body = UiRect {
        min: bounds.min,
        size: [bounds.size[0], measured.height],
    };
    let y = body.min[1] + body.size[1] + spacing.gap;
    let actions = UiRect {
        min: [bounds.min[0], y],
        size: [bounds.size[0], (bounds.min[1] + bounds.size[1] - y).max(0.)],
    };
    let metrics = ferravex_render::get_default_font()
        .try_metrics(8, size / 8.)
        .unwrap();
    let items = LABELS.map(|label| {
        FlowItem::fixed(label.len() as f32 * metrics.glyph_advance() + 2. * spacing.compact_gap)
    });
    let mut rects = [actions; 2];
    let report = flow::layout(
        actions,
        &items,
        FlowOptions {
            row_height: spacing.control_height,
            gap: [spacing.gap; 2],
            wrap: true,
        },
        &mut rects,
    )
    .map_err(|_| UiError::InvalidLayout)?;
    if report.overflow != 0 || measured.width > bounds.size[0] {
        return Err(UiError::InvalidLayout);
    }
    ui.begin(viewport);
    ui.panel(viewport.bounds(), theme.color(ColorRole::Canvas))?;
    assert_eq!(
        ui.wrapped_text(
            body,
            COPY,
            TextStyle {
                size,
                color: theme.color(ColorRole::Text),
                wrap: true,
                ..Default::default()
            },
            LineLimit::Unlimited
        )?,
        measured
    );
    for (index, rect) in rects.iter().enumerate() {
        // `button`, not a fitting/ellipsis adapter: every authored label survives.
        ui.button(
            index as u64 + 1,
            *rect,
            LABELS[index],
            true,
            ButtonStyle {
                fill: theme.color(ColorRole::Surface),
                ink: theme.color(ColorRole::Text),
                focus: theme.color(ColorRole::Focus),
                disabled: theme.color(ColorRole::Surface),
                size,
            },
        )?;
    }
    ui.finish(true);
    Ok((rects, report.rows))
}

#[test]
fn live_reflow_rebuilds_full_labels_and_the_same_paint_hit_touch_rectangles() {
    let mut ui = Ui::new();
    ui.set_inspection_enabled(true);
    let mut normal = None;
    for width in [320, 390, 960] {
        let viewport = Viewport::responsive([width as f32, 640.], [8., 16., 8., 12.]).unwrap();
        for scale in [1., 1.5, 2.] {
            for contrast in [0, 1] {
                assert_eq!(preferences::apply_ui_snapshot_v1(1, scale, contrast), 1);
                if width == 320 && scale == 2. {
                    // The full CONTROLS label exceeds this safe/inset width at
                    // 2x. Reject explicitly; a real consumer must add scrolling
                    // or a wrapped-label composition, never shrink the text.
                    assert!(compose(&mut ui, viewport).is_err());
                    ui.cancel();
                    continue;
                }
                let mut result = None;
                host_frame(0., width, 640, || {
                    let (rects, rows) = compose(&mut ui, viewport).unwrap();
                    assert_eq!(rects[0].size[1], 44. * scale);
                    if width == 320 && scale == 1. {
                        normal = Some(rects);
                        assert_eq!(rows, 1);
                    }
                    if width == 320 && scale == 1.5 {
                        assert_eq!(rows, 2);
                        assert_ne!(rects, normal.unwrap());
                    }
                    for (index, rect) in rects.iter().enumerate() {
                        let element = ui
                            .inspect()
                            .unwrap()
                            .find(|e| e.id == Some(index as u64 + 1))
                            .unwrap();
                        assert_eq!(element.bounds, *rect);
                        assert_eq!(element.visible_bounds, Some(*rect));
                        assert_eq!(element.painted_label, LABELS[index]);
                        assert!(element.interaction_registered);
                    }
                    ui.paint(); // Normal renderer CPU submission, with unchanged world camera.
                    result = Some(rects);
                });
                let rects = result.unwrap();
                let target = rects[1];
                for phase in [touch::Phase::Start, touch::Phase::End] {
                    touch_event(touch::Event {
                        id: 19,
                        position: vec2(
                            target.min[0] + target.size[0] / 2.,
                            target.min[1] + target.size[1] / 2.,
                        ),
                        phase,
                    });
                }
                host_frame(0., width, 640, || {
                    assert_eq!(compose(&mut ui, viewport).unwrap().0, rects);
                    assert!(ui.activated(2));
                    assert!(!ui.activated(1));
                    assert!(ui.interaction().pointer_consumed);
                });
            }
        }
    }
    preferences::apply_ui_preferences(UiPreferences::default());
    assert!(compose(&mut ui, Viewport::responsive([320., 80.], [0.; 4]).unwrap()).is_err());
    assert_eq!(preferences::ui_preferences(), UiPreferences::default());
    assert!(compose(&mut ui, Viewport::responsive([1., 640.], [0.; 4]).unwrap()).is_err());
}

thread_local! { static ALLOCATIONS: Cell<Option<usize>> = const { Cell::new(None) }; }
struct CountingAllocator;
fn allocated() {
    let _ = ALLOCATIONS.try_with(|c| {
        if let Some(n) = c.get() {
            c.set(Some(n + 1));
        }
    });
}
// SAFETY: Forward unchanged allocator arguments/results; TLS accounting does not allocate.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        allocated();
        unsafe { System.alloc(l) }
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        allocated();
        unsafe { System.alloc_zeroed(l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        allocated();
        unsafe { System.realloc(p, l, n) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;
struct StopCounting;
impl Drop for StopCounting {
    fn drop(&mut self) {
        ALLOCATIONS.with(|c| c.set(None));
    }
}
#[test]
fn warmed_live_ui_apply_validate_reflow_and_paint_do_not_allocate() {
    let mut ui = Ui::new();
    ui.set_inspection_enabled(true);
    let viewport = Viewport::responsive([320., 640.], [0.; 4]).unwrap();
    let mut run = |i: usize| {
        preferences::apply_ui_snapshot_v1(1, [1., 1.5, 2.][i % 3], (i % 2) as u32);
        host_frame(0., 320, 640, || {
            compose(&mut ui, viewport).unwrap();
            ui.paint();
        });
        assert_eq!(preferences::apply_ui_snapshot_v1(1, 1., 2), 0);
    };
    for i in 0..12 {
        run(i);
    }
    ALLOCATIONS.with(|c| c.set(Some(0)));
    let guard = StopCounting;
    for i in 0..120 {
        run(i);
    }
    let count = ALLOCATIONS.with(|c| c.get().unwrap());
    drop(guard);
    assert_eq!(count, 0);
}
