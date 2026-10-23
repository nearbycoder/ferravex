//! Public composition and allocation regressions; synthetic/headless input only.
use ferravex_play::ui::{HudEdge, HudRegions, scroll::ScrollState};
use ferravex_render::{
    input::{host_frame, touch, touch_event},
    ui::{
        ButtonStyle, Ui, UiError, UiRect, Viewport,
        scroll::{SCROLL_ITEM_CAPACITY, ScrollItem, ScrollRegion},
    },
    vec2,
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    hint::black_box,
};

#[test]
fn hud_region_help_recipe_with_public_touch_input_and_stable_ids() {
    let mut hud = HudRegions::new([320., 240.], [12., 20., 12., 12.], [0.; 2]).unwrap();
    let bounds = hud
        .reserve("course information", HudEdge::Top, 100.)
        .unwrap();
    let viewport = Viewport::responsive([320., 240.], [0.; 4]).unwrap();
    let items = [101, 205, 309].map(|id| ScrollItem {
        id,
        rect: UiRect {
            min: [0., ((id - 101) / 104) as f32 * 70.],
            size: [bounds.size[0], 44.],
        },
        enabled: true,
    });
    let mut region = ScrollRegion::new(3);
    let mut paint = Ui::new();
    host_frame(0., 320, 240, || {
        region.update(viewport, bounds, 184., &items, true).unwrap();
        for _ in 0..3 {
            region.focus_next(false);
        }
        assert_eq!(region.focused(), Some(309));
        assert_eq!(region.scroll().offset(), 84.);
        paint.begin(viewport);
        for (item, label) in items.iter().zip(["COURSE", "BEST TIME", "CONTROLS"]) {
            region
                .button(&mut paint, item.id, label, ButtonStyle::default())
                .unwrap();
        }
        paint.paint(); // Exercises the shared CPU painter without a GPU/window.
    });
    // The synthetic touch uses the same visible geometry, in logical pixels.
    let shown = region.rect(309).unwrap();
    let position = vec2(shown.min[0] + 20., shown.min[1] + 20.);
    for phase in [touch::Phase::Start, touch::Phase::End] {
        touch_event(touch::Event {
            id: 9,
            position,
            phase,
        });
    }
    host_frame(0., 320, 240, || {
        region.update(viewport, bounds, 184., &items, true).unwrap();
        assert!(region.activated(309));
        assert!(!region.activated(101));
        region.reset();
        assert_eq!(region.focused(), None);
        assert_eq!(region.scroll().offset(), 0.);
    });
}

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
// SAFETY: Arguments are forwarded unchanged to System; the thread-local counter
// neither allocates nor dereferences the pointers and tolerates thread teardown.
unsafe impl GlobalAlloc for CountingAllocator {
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
        unsafe { System.dealloc(pointer, layout) };
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

struct StopCounting;
impl Drop for StopCounting {
    fn drop(&mut self) {
        ALLOCATIONS.with(|counter| counter.set(None));
    }
}

#[test]
fn full_capacity_rebuild_navigation_queue_errors_and_reset_reuse_storage() {
    let items: [_; SCROLL_ITEM_CAPACITY] = std::array::from_fn(|index| ScrollItem {
        id: index as u64,
        rect: UiRect {
            min: [0., index as f32 * 44.],
            size: [200., 44.],
        },
        enabled: true,
    });
    let viewport = Viewport::responsive([320., 240.], [0.; 4]).unwrap();
    let bounds = UiRect {
        min: [10., 20.],
        size: [200., 100.],
    };
    let mut region = ScrollRegion::new(usize::MAX);
    let mut paint = Ui::new();
    let extent = items.len() as f32 * 44.;
    assert_eq!(region.capacity(), SCROLL_ITEM_CAPACITY);
    for _ in 0..16 {
        host_frame(0., 320, 240, || {
            ALLOCATIONS.with(|counter| counter.set(Some(0)));
            let guard = StopCounting;
            let mut pure = ScrollState::new(100., extent).unwrap();
            pure.ensure_visible(extent - 44., 44.).unwrap();
            assert_eq!(pure.set_offset(f32::NAN), Err(UiError::InvalidLayout));
            pure.reset();
            region
                .update(viewport, bounds, extent, &items, true)
                .unwrap();
            assert_eq!(region.focus_next(true), Some((items.len() - 1) as u64));
            black_box(region.activate_focused());
            region.page(true);
            region.scroll_by(f32::MAX).unwrap();
            region.reveal(0);
            paint.begin(viewport);
            for item in items {
                region
                    .button(&mut paint, item.id, "", ButtonStyle::default())
                    .unwrap();
            }
            assert_eq!(
                region.button(&mut paint, 0, "", ButtonStyle::default()),
                Err(UiError::Capacity)
            );
            region.reset();
            assert_eq!(region.focused(), None);
            assert_eq!(region.scroll().offset(), 0.);
            let count = ALLOCATIONS.with(|counter| counter.get().unwrap());
            drop(guard);
            assert_eq!(count, 0);
        });
    }
}
