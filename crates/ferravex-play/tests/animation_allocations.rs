use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};
thread_local! {static RECORD:Cell<bool>=const {Cell::new(false)};static CALLS:Cell<usize>=const {Cell::new(0)};}
struct Allocator;
fn allocated(p: *mut u8) {
    if !p.is_null() {
        let _ = RECORD.try_with(|on| {
            if on.get() {
                let _ = CALLS.try_with(|n| n.set(n.get() + 1));
            }
        });
    }
}
// SAFETY: Every operation forwards the identical allocation contract to System;
// thread-local counters do not allocate and ignore destruction-time TLS failures.
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(l) };
        allocated(p);
        p
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        let p = unsafe { System.alloc_zeroed(l) };
        allocated(p);
        p
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        let p = unsafe { System.realloc(p, l, n) };
        allocated(p);
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
}
#[global_allocator]
static ALLOCATOR: Allocator = Allocator;
#[test]
fn playback_graph_layers_events_and_skeleton_reuse_storage() {
    use ferravex_play::{
        animation::*,
        scene::{IDENTITY, Joint, Skeleton, Transform},
    };
    let track = Vec3Track::new(
        vec![
            Keyframe {
                time: 0.,
                value: [0.; 3],
            },
            Keyframe {
                time: 1.,
                value: [1.; 3],
            },
        ],
        Interpolation::Linear,
    )
    .unwrap();
    let bind = vec![Transform::default(); 16];
    let clip = AnimationClip::new(
        16,
        1.,
        LoopMode::Repeat,
        vec![JointTrack {
            joint: 0,
            translation: Some(track),
            rotation: None,
            scale: None,
        }],
        vec![AnimationEvent { time: 0.5, id: 1 }],
    )
    .unwrap();
    let bank = AnimationBank::new(bind, vec![clip.clone(), clip]).unwrap();
    let graph = AnimationGraph::new(
        &bank,
        vec![
            AnimationState { clip: 0, speed: 1. },
            AnimationState { clip: 1, speed: 1. },
        ],
        vec![
            AnimationTransition {
                from: 0,
                to: 1,
                seconds: 0.25,
                exit_time: 0.,
                conditions: vec![AnimationCondition::Trigger(0)],
            },
            AnimationTransition {
                from: 1,
                to: 0,
                seconds: 0.25,
                exit_time: 0.,
                conditions: vec![AnimationCondition::Trigger(0)],
            },
        ],
        1,
    )
    .unwrap();
    let mut player = GraphPlayer::new(&graph, 0).unwrap();
    let mask = JointMask::new(vec![0.5; 16]).unwrap();
    let skeleton = Skeleton::new(
        (0..16)
            .map(|i| Joint {
                parent: if i == 0 { None } else { Some(i - 1) },
                inverse_bind: IDENTITY,
            })
            .collect(),
    )
    .unwrap();
    let mut world = [IDENTITY; 16];
    let mut palette = [IDENTITY; 16];
    let mut events = EventBuffer::<8>::default();
    let mut sprites = SpritePlayer::new(
        SpriteClip::new(vec![
            SpriteFrame {
                index: 0,
                duration: 0.25,
            },
            SpriteFrame {
                index: 1,
                duration: 0.75,
            },
        ])
        .unwrap(),
        LoopMode::Repeat,
        vec![],
    )
    .unwrap();
    CALLS.with(|n| n.set(0));
    RECORD.with(|on| on.set(true));
    for i in 0..1000 {
        if i % 193 == 0 {
            player.reset(0).unwrap();
        }
        if i % 7 == 0 {
            player.trigger(0).unwrap();
        }
        player.advance(1. / 60., &mut events).unwrap();
        player
            .animator_mut()
            .apply_layer(0, i as f32 * 0.01, 0.5, Some(&mask), LayerMode::Override)
            .unwrap();
        player
            .animator_mut()
            .apply_layer(1, i as f32 * 0.01, 0.25, Some(&mask), LayerMode::Additive)
            .unwrap();
        if i % 97 == 0 {
            player.animator_mut().seek(0.25).unwrap();
        }
        skeleton
            .skin_palette_into(player.animator().pose(), &mut world, &mut palette)
            .unwrap();
        sprites.advance(1. / 60., &mut events).unwrap();
    }
    RECORD.with(|on| on.set(false));
    assert_eq!(CALLS.with(Cell::get), 0);
    assert!(palette.iter().flatten().all(|v| v.is_finite()));
}
