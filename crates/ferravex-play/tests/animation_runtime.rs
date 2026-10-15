use ferravex_math::Quat;
use ferravex_play::{
    animation::*,
    scene::{IDENTITY, Joint, Skeleton, Transform, transform_point},
};
fn event(time: f32, id: u32) -> AnimationEvent {
    AnimationEvent { time, id }
}
fn tr(x: f32) -> Transform {
    Transform {
        translation: [x, 0., 0.],
        ..Default::default()
    }
}
fn track(a: f32, b: f32) -> Vec3Track {
    Vec3Track::new(
        vec![
            Keyframe {
                time: 0.,
                value: [a, 0., 0.],
            },
            Keyframe {
                time: 1.,
                value: [b, 0., 0.],
            },
        ],
        Interpolation::Linear,
    )
    .unwrap()
}
fn clip(a: f32, b: f32, mode: LoopMode) -> AnimationClip {
    AnimationClip::new(
        2,
        1.,
        mode,
        vec![JointTrack {
            joint: 0,
            translation: Some(track(a, b)),
            rotation: None,
            scale: None,
        }],
        vec![event(0., 10), event(0.5, 11), event(1., 12)],
    )
    .unwrap()
}
fn bank() -> AnimationBank {
    AnimationBank::new(
        vec![tr(0.), tr(2.)],
        vec![
            clip(0., 1., LoopMode::Repeat),
            clip(10., 20., LoopMode::Clamp),
            clip(30., 40., LoopMode::Repeat),
        ],
    )
    .unwrap()
}
fn near(a: f32, b: f32) {
    assert!((a - b).abs() < 1e-4, "{a} != {b}");
}
#[test]
fn repeat_seams_are_chronological_and_zero_dt_pause_seek_are_silent() {
    let events = EventTrack::new(1., vec![event(0., 0), event(0.5, 1), event(1., 2)]).unwrap();
    let mut clock = Timeline::new(1., LoopMode::Repeat).unwrap();
    let mut out = EventBuffer::<16>::default();
    clock.advance_events(0., &events, &mut out).unwrap();
    assert!(out.hits().is_empty());
    clock.set_paused(true);
    clock.advance_events(1., &events, &mut out).unwrap();
    assert!(out.hits().is_empty());
    clock.set_paused(false);
    clock.advance_events(2.5, &events, &mut out).unwrap();
    assert_eq!(
        out.hits()
            .iter()
            .map(|e| (e.id, e.cycle))
            .collect::<Vec<_>>(),
        [
            (0, 0),
            (1, 0),
            (2, 0),
            (0, 1),
            (1, 1),
            (2, 1),
            (0, 2),
            (1, 2)
        ]
    );
    assert_eq!(out.dropped(), 0);
    assert_eq!(clock.cycles(), 2);
    near(clock.time(), 0.5);
    clock.seek(0.).unwrap();
    clock.advance_events(0.25, &events, &mut out).unwrap();
    assert!(out.hits().is_empty());
    clock.restart();
    clock.advance_events(0.25, &events, &mut out).unwrap();
    assert_eq!(out.hits()[0].id, 0);
    clock.set_speed(0.).unwrap();
    clock.advance_events(5., &events, &mut out).unwrap();
    assert!(out.hits().is_empty());
    near(clock.time(), 0.25);
}
#[test]
fn events_are_partition_independent_and_one_shot_end_is_delivered_once() {
    let events = EventTrack::new(
        1.,
        vec![event(0., 0), event(0.25, 1), event(0.5, 2), event(1., 3)],
    )
    .unwrap();
    let mut full = Timeline::new(1., LoopMode::Repeat).unwrap();
    let mut steps = full.clone();
    let mut out = EventBuffer::<64>::default();
    full.advance_events(4., &events, &mut out).unwrap();
    let expected = out.hits().to_vec();
    let mut got = Vec::new();
    for _ in 0..32 {
        steps.advance_events(0.125, &events, &mut out).unwrap();
        got.extend_from_slice(out.hits());
    }
    assert_eq!(got, expected);
    near(steps.time(), full.time());
    assert_eq!(steps.cycles(), full.cycles());
    let mut one = Timeline::new(1., LoopMode::Clamp).unwrap();
    one.advance_events(1., &events, &mut out).unwrap();
    assert_eq!(out.hits().len(), 4);
    assert!(one.finished());
    one.advance_events(1., &events, &mut out).unwrap();
    assert!(out.hits().is_empty());
}
#[test]
fn event_storm_is_bounded_counted_and_does_not_stall_clock() {
    let events = EventTrack::new(0.0001, vec![event(0., 1), event(0.0001, 2)]).unwrap();
    let mut clock = Timeline::new(0.0001, LoopMode::Repeat).unwrap();
    clock.set_speed(8.).unwrap();
    let mut out = EventBuffer::<4>::default();
    clock.advance_events(60., &events, &mut out).unwrap();
    assert_eq!(out.hits().len(), 4);
    assert!(out.dropped() > 9_000_000);
    assert!(clock.cycles() > 4_000_000);
    assert_eq!(
        out.hits().iter().map(|e| e.id).collect::<Vec<_>>(),
        [1, 2, 1, 2]
    );
    let mut none = EventBuffer::<0>::default();
    clock.advance_events(60., &events, &mut none).unwrap();
    assert!(none.hits().is_empty());
    assert!(none.dropped() > 9_000_000);
}
#[test]
fn malformed_time_and_event_tracks_are_transactional() {
    let events = EventTrack::new(1., vec![event(0., 1)]).unwrap();
    let mut clock = Timeline::new(1., LoopMode::Clamp).unwrap();
    let mut out = EventBuffer::<4>::default();
    clock.advance_events(0.25, &events, &mut out).unwrap();
    let previous = out.hits().to_vec();
    for dt in [f32::NAN, f32::INFINITY, -1., 60.01] {
        assert!(clock.advance_events(dt, &events, &mut out).is_err());
        near(clock.time(), 0.25);
        assert_eq!(out.hits(), previous);
    }
    assert!(clock.set_speed(-1.).is_err());
    assert!(clock.seek(f32::INFINITY).is_err());
    assert!(EventTrack::new(1., vec![event(0.7, 1), event(0.3, 2)]).is_err());
    assert!(EventTrack::new(1., vec![event(1.1, 0)]).is_err());
    assert!(Timeline::new(0., LoopMode::Repeat).is_err());
    assert!(Timeline::new(3601., LoopMode::Repeat).is_err());
}
#[test]
fn outgoing_clip_keeps_moving_and_interrupted_crossfade_has_no_pop() {
    let mut player = Animator::new(&bank(), 0).unwrap();
    let mut events = EventBuffer::<16>::default();
    player.advance(0.25, &mut events).unwrap();
    near(player.pose()[0].translation[0], 0.25);
    player.transition(1, 1.).unwrap();
    player.advance(0.5, &mut events).unwrap();
    near(player.pose()[0].translation[0], (0.75 + 15.) * 0.5); // source advanced too
    assert!(events.hits().iter().all(|e| e.clip == 1));
    let previous = player.pose().to_vec();
    player.transition(2, 1.).unwrap();
    player.advance(0., &mut events).unwrap();
    assert_eq!(player.pose(), previous);
    player.advance(0.5, &mut events).unwrap();
    near(
        player.pose()[0].translation[0],
        (previous[0].translation[0] + 35.) * 0.5,
    );
    player.set_paused(true);
    player.advance(1., &mut events).unwrap();
    near(
        player.pose()[0].translation[0],
        (previous[0].translation[0] + 35.) * 0.5,
    );
    assert!(player.transitioning());
    player.set_paused(false);
    player.advance(0.5, &mut events).unwrap();
    assert!(!player.transitioning());
    near(player.pose()[0].translation[0], 30.);
    player.transition(2, 1.).unwrap();
    assert!(!player.transitioning());
    player.seek(0.5).unwrap();
    near(player.pose()[0].translation[0], 35.);
    assert_eq!(player.pose()[1], tr(2.));
}
#[test]
fn missing_channels_hold_bind_pose_and_short_tracks_hold_last_key() {
    let t = Vec3Track::new(
        vec![
            Keyframe {
                time: 0.,
                value: [0.; 3],
            },
            Keyframe {
                time: 0.25,
                value: [1.; 3],
            },
        ],
        Interpolation::Linear,
    )
    .unwrap();
    let clip = AnimationClip::new(
        1,
        1.,
        LoopMode::Repeat,
        vec![JointTrack {
            joint: 0,
            translation: Some(t),
            rotation: None,
            scale: None,
        }],
        vec![],
    )
    .unwrap();
    let bind = Transform {
        rotation: [0., 0., 0., 2.],
        scale: [2., 3., 4.],
        ..Default::default()
    };
    let mut out = [Transform::default()];
    clip.sample_into(0.8, &[bind], &mut out).unwrap();
    assert_eq!(out[0].translation, [1.; 3]);
    assert_eq!(out[0].rotation, [0., 0., 0., 1.]);
    assert_eq!(out[0].scale, bind.scale);
    let previous = out;
    assert!(clip.sample_into(f32::NAN, &[bind], &mut out).is_err());
    assert_eq!(out, previous);
    assert!(
        AnimationClip::new(
            1,
            1.,
            LoopMode::Clamp,
            vec![JointTrack {
                joint: 2,
                translation: Some(track(0., 1.)),
                rotation: None,
                scale: None
            }],
            vec![]
        )
        .is_err()
    );
    assert!(
        AnimationBank::new(
            vec![Transform::default()],
            vec![self::clip(0., 1., LoopMode::Clamp)]
        )
        .is_err()
    );
}
#[test]
fn masked_override_and_additive_rotation_use_authored_reference() {
    let base = [tr(0.), tr(1.)];
    let mut layer = [tr(8.), tr(9.)];
    layer[1].rotation = Quat::from_rotation_z(std::f32::consts::PI).to_array();
    let mask = JointMask::new(vec![0., 1.]).unwrap();
    let mut out = base;
    blend_pose_into(&base, &layer, 0.5, Some(&mask), &mut out).unwrap();
    assert_eq!(out[0], base[0]);
    near(out[1].translation[0], 5.);
    near(Quat::from_array(out[1].rotation).length(), 1.);
    let reference = [tr(4.), tr(5.)];
    additive_pose_into(&base, &layer, &reference, 0.5, Some(&mask), &mut out).unwrap();
    near(out[1].translation[0], 3.);
    let rotated = Quat::from_array(out[1].rotation) * ferravex_math::Vec3::X;
    near(rotated.x, 0.);
    near(rotated.y.abs(), 1.);
    let previous = out;
    assert!(blend_pose_into(&base, &layer, f32::NAN, Some(&mask), &mut out).is_err());
    assert_eq!(out, previous);
    let huge = [tr(f32::MAX), tr(f32::MAX)];
    let negative = [tr(-f32::MAX), tr(-f32::MAX)];
    assert!(additive_pose_into(&huge, &huge, &negative, 1., None, &mut out).is_err());
    assert_eq!(out, previous);
}
#[test]
fn layer_stack_resets_on_base_update_and_does_not_damage_frozen_transition_source() {
    let mut player = Animator::new(&bank(), 0).unwrap();
    let mut events = EventBuffer::<8>::default();
    player.advance(0.5, &mut events).unwrap();
    player
        .apply_layer(1, 0., 0.5, None, LayerMode::Additive)
        .unwrap();
    near(player.pose()[0].translation[0], 5.5);
    player.advance(0., &mut events).unwrap();
    near(player.pose()[0].translation[0], 0.5);
    player.transition(1, 1.).unwrap();
    player.advance(0.25, &mut events).unwrap();
    player.transition(2, 1.).unwrap();
    let frozen = player.pose().to_vec();
    player
        .apply_layer(1, 0., 1., None, LayerMode::Override)
        .unwrap();
    near(player.pose()[0].translation[0], 10.);
    player.advance(0., &mut events).unwrap();
    assert_eq!(player.pose(), frozen);
}
#[test]
fn sprite_playback_shares_speed_pause_and_event_semantics() {
    let clip = SpriteClip::new(vec![
        SpriteFrame {
            index: 3,
            duration: 0.25,
        },
        SpriteFrame {
            index: 8,
            duration: 0.75,
        },
    ])
    .unwrap();
    let mut player = SpritePlayer::new(clip, LoopMode::Repeat, vec![event(0.25, 10)]).unwrap();
    let mut out = EventBuffer::<8>::default();
    player.timeline_mut().set_speed(2.).unwrap();
    player.advance(0.125, &mut out).unwrap();
    assert_eq!(player.frame(), 8);
    assert_eq!(out.hits()[0].id, 10);
    player.timeline_mut().set_paused(true);
    player.advance(1., &mut out).unwrap();
    assert_eq!(player.frame(), 8);
    assert!(out.hits().is_empty());
    player.timeline_mut().seek(1.1).unwrap();
    assert_eq!(player.frame(), 3);
}
#[test]
fn preallocated_skeleton_matches_allocating_api_and_rejects_bad_scratch() {
    let skeleton = Skeleton::new(vec![
        Joint {
            parent: None,
            inverse_bind: IDENTITY,
        },
        Joint {
            parent: Some(0),
            inverse_bind: tr(-2.).matrix().unwrap(),
        },
    ])
    .unwrap();
    let locals = [tr(1.), tr(3.)];
    let mut world = [IDENTITY; 2];
    let mut palette = [IDENTITY; 2];
    skeleton
        .skin_palette_into(&locals, &mut world, &mut palette)
        .unwrap();
    assert_eq!(palette.as_slice(), skeleton.skin_palette(&locals).unwrap());
    assert_eq!(world.as_slice(), skeleton.world_pose(&locals).unwrap());
    assert_eq!(transform_point(palette[1], [0.; 3]), [2., 0., 0.]);
    assert!(
        skeleton
            .skin_palette_into(&locals, &mut world, &mut palette[..1])
            .is_err()
    );
    assert!(
        skeleton
            .world_pose_into(&[tr(f32::NAN), tr(0.)], &mut world)
            .is_err()
    );
}
#[test]
fn graph_conditions_triggers_priority_pause_and_completion_are_deterministic() {
    let transition = |from, to, conditions| AnimationTransition {
        from,
        to,
        seconds: 0.125,
        exit_time: 0.,
        conditions,
    };
    let graph = AnimationGraph::new(
        &bank(),
        vec![
            AnimationState { clip: 0, speed: 1. },
            AnimationState { clip: 1, speed: 1. },
            AnimationState { clip: 2, speed: 1. },
        ],
        vec![
            transition(
                0,
                1,
                vec![
                    AnimationCondition::Greater {
                        parameter: 0,
                        value: 0.5,
                    },
                    AnimationCondition::Trigger(1),
                ],
            ),
            transition(0, 2, vec![AnimationCondition::Trigger(1)]),
            transition(1, 0, vec![AnimationCondition::Finished]),
        ],
        2,
    )
    .unwrap();
    let mut player = GraphPlayer::new(&graph, 0).unwrap();
    let mut events = EventBuffer::<16>::default();
    player.set_parameter(0, 1.).unwrap();
    player.trigger(1).unwrap();
    player.animator_mut().set_paused(true);
    player.advance(0.5, &mut events).unwrap();
    assert_eq!(player.state(), 0);
    player.animator_mut().set_paused(false);
    player.advance(0., &mut events).unwrap();
    assert_eq!(player.state(), 0);
    player.advance(0.5, &mut events).unwrap();
    assert_eq!(player.state(), 1);
    assert!(events.hits().iter().all(|e| e.clip == 1));
    player.advance(0.5, &mut events).unwrap();
    assert!(player.animator().timeline().finished());
    player.advance(0.125, &mut events).unwrap();
    assert_eq!(player.state(), 0);
    player.advance(0.125, &mut events).unwrap();
    assert_eq!(player.state(), 0); // trigger consumed
    player.set_parameter(0, 0.).unwrap();
    player.trigger(1).unwrap();
    player.advance(0.125, &mut events).unwrap();
    assert_eq!(player.state(), 2);
    assert!(player.set_parameter(2, 0.).is_err());
    assert!(player.set_parameter(0, f32::NAN).is_err());
    assert!(player.trigger(2).is_err());
    assert!(
        AnimationGraph::new(
            &bank(),
            vec![AnimationState { clip: 0, speed: 1. }],
            vec![transition(0, 0, vec![])],
            0
        )
        .is_err()
    );
}
#[test]
fn interrupting_a_fade_does_not_bake_then_reapply_the_same_additive_layer() {
    let mut player = Animator::new(&bank(), 0).unwrap();
    let mut events = EventBuffer::<8>::default();
    player.advance(0.25, &mut events).unwrap();
    player.transition(1, 1.).unwrap();
    player.advance(0.25, &mut events).unwrap();
    let base = player.base_pose().to_vec();
    player
        .apply_layer(2, 0., 0.2, None, LayerMode::Additive)
        .unwrap();
    let visible = player.pose().to_vec();
    player.transition(2, 1.).unwrap();
    player.advance(0., &mut events).unwrap();
    assert_eq!(player.base_pose(), base);
    player
        .apply_layer(2, 0., 0.2, None, LayerMode::Additive)
        .unwrap();
    assert_eq!(player.pose(), visible);
}
