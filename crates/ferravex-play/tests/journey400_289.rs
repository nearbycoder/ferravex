use ferravex_math::{Mat4, Quat, Vec3};
use ferravex_play::{
    animation::{retarget::*, *},
    scene::{IDENTITY, Joint, Skeleton, Transform},
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};

thread_local! {
    static RECORD: Cell<bool> = const { Cell::new(false) };
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}
struct Allocator;
fn allocated(p: *mut u8) {
    if !p.is_null() {
        let _ = RECORD.try_with(|on| {
            if on.get() {
                let _ = ALLOCATIONS.try_with(|n| n.set(n.get() + 1));
            }
        });
    }
}
// SAFETY: Allocation contracts are forwarded unchanged to System. Counters are
// thread-local, nonallocating, and tolerate thread-local destruction.
unsafe impl GlobalAlloc for Allocator {
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
        unsafe { System.dealloc(p, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: Allocator = Allocator;

fn local(t: [f32; 3], q: Quat, s: f32) -> Transform {
    Transform {
        translation: t,
        rotation: q.to_array(),
        scale: [s; 3],
    }
}
fn rig(parents: &[Option<usize>]) -> Skeleton {
    Skeleton::new(
        parents
            .iter()
            .map(|&parent| Joint {
                parent,
                inverse_bind: IDENTITY,
            })
            .collect(),
    )
    .unwrap()
}
fn bound_rig(parents: &[Option<usize>], rest: &[Transform]) -> Skeleton {
    let world = rig(parents).world_pose(rest).unwrap();
    Skeleton::new(
        parents
            .iter()
            .zip(world)
            .map(|(&parent, matrix)| Joint {
                parent,
                inverse_bind: Mat4::from_cols_array(&matrix)
                    .checked_inverse()
                    .unwrap()
                    .to_cols_array(),
            })
            .collect(),
    )
    .unwrap()
}
fn near(a: f32, b: f32) {
    assert!((a - b).abs() < 2e-5, "{a} != {b}");
}
fn same_local(a: Transform, b: Transform) {
    assert!(
        Mat4::from_cols_array(&a.matrix().unwrap())
            .abs_diff_eq(Mat4::from_cols_array(&b.matrix().unwrap()), 2e-5)
    );
}
fn bits(transforms: &[Transform]) -> Vec<u32> {
    transforms
        .iter()
        .flat_map(|t| t.translation.into_iter().chain(t.rotation).chain(t.scale))
        .map(f32::to_bits)
        .collect()
}
fn sentinel() -> Transform {
    Transform {
        translation: [-0., f32::from_bits(0x7fc01234), 99.],
        ..Transform::default()
    }
}

#[test]
fn real_animation_two_skeletons_different_order_orientation_length_world_and_skin() {
    let sr = [
        Transform::default(),
        local([1., 0., 0.], Quat::IDENTITY, 1.),
        local([0., 0., 1.], Quat::IDENTITY, 1.),
    ];
    let tr = [
        sr[0],
        sr[2],
        local(
            [0., 2., 0.],
            Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
            2.,
        ),
    ];
    let source = bound_rig(&[None, Some(0), Some(0)], &sr);
    let target = bound_rig(&[None, Some(0), Some(0)], &tr);
    let map = PreparedRetarget::try_new(
        &source,
        &["root", "arm", "marker"],
        &sr,
        &target,
        &["root", "marker", "arm"],
        &tr,
        MissingJointPolicy::Strict,
    )
    .unwrap();
    assert!(std::ptr::eq(map.source(), &source));
    assert!(std::ptr::eq(map.target(), &target));
    let clip = AnimationClip::new(
        3,
        1.,
        LoopMode::Clamp,
        vec![JointTrack {
            joint: 1,
            translation: Some(
                Track::new(
                    vec![
                        Keyframe {
                            time: 0.,
                            value: [1., 0., 0.],
                        },
                        Keyframe {
                            time: 1.,
                            value: [2., 0., 0.],
                        },
                    ],
                    Interpolation::Linear,
                )
                .unwrap(),
            ),
            rotation: Some(
                Track::new(
                    vec![
                        Keyframe {
                            time: 0.,
                            value: Quat::IDENTITY.to_array(),
                        },
                        Keyframe {
                            time: 1.,
                            value: Quat::from_rotation_x(0.6).to_array(),
                        },
                    ],
                    Interpolation::Linear,
                )
                .unwrap(),
            ),
            scale: None,
        }],
        vec![],
    )
    .unwrap();
    let bank = AnimationBank::new(sr.to_vec(), vec![clip]).unwrap();
    let mut player = Animator::new(&bank, 0).unwrap();
    player
        .advance(0.5, &mut EventBuffer::<0>::default())
        .unwrap();
    let mut out = [Transform::default(); 3];
    map.map_pose_into(player.pose(), &mut out).unwrap();
    near(out[2].translation[0], 0.);
    near(out[2].translation[1], 3.);
    same_local(
        out[2],
        local(
            [0., 3., 0.],
            Quat::from_array(tr[2].rotation) * Quat::from_rotation_x(0.3),
            2.,
        ),
    );
    assert_eq!(out[1], sr[2]);
    let mut world = [IDENTITY; 3];
    let mut palette = [IDENTITY; 3];
    target.world_pose_into(&out, &mut world).unwrap();
    near(world[2][13], 3.);
    target
        .skin_palette_into(&out, &mut world, &mut palette)
        .unwrap();
    assert!(palette.iter().flatten().all(|v| v.is_finite()));
    let endpoint = Mat4::from_cols_array(&palette[2]).transform_point3(Vec3::new(0., 2., 0.));
    near(endpoint.y, 3.);
    map.map_pose_into(&sr, &mut out).unwrap();
    assert_eq!(out, tr);
    target
        .skin_palette_into(&out, &mut world, &mut palette)
        .unwrap();
    assert!(
        palette
            .into_iter()
            .all(|m| Mat4::from_cols_array(&m).abs_diff_eq(Mat4::IDENTITY, 2e-5))
    );
}

#[test]
fn nonidentity_source_rest_right_delta_and_animated_uniform_scale() {
    let source = rig(&[None]);
    let target = rig(&[None]);
    let sq = Quat::from_rotation_y(0.7);
    let tq = Quat::from_rotation_z(-0.4);
    let sr = [local([3., 2., -1.], sq, 2.)];
    let tr = [local([-2., 5., 1.], tq, 3.)];
    let map = PreparedRetarget::try_new(
        &source,
        &["root"],
        &sr,
        &target,
        &["root"],
        &tr,
        MissingJointPolicy::Strict,
    )
    .unwrap();
    // Compose an explicit rest-local translation, twist and positive scale delta.
    let d = Vec3::new(0.4, -0.2, 0.6);
    let source_t = Vec3::from_array(sr[0].translation) + sq * (d * 2.);
    let pose = [local(
        source_t.to_array(),
        sq * Quat::from_rotation_x(0.8),
        2.5,
    )];
    let target_t = Vec3::from_array(tr[0].translation) + tq * (d * 3.);
    let mut out = [Transform::default()];
    map.map_pose_into(&pose, &mut out).unwrap();
    same_local(
        out[0],
        local(target_t.to_array(), tq * Quat::from_rotation_x(0.8), 3.75),
    );
}

#[test]
fn rest_identity_nonunit_quaternions_and_repeated_reset_are_stable() {
    let source = rig(&[None, Some(0)]);
    let target = rig(&[None, Some(0)]);
    let sr = [local([1., 2., 3.], Quat::from_xyzw(0., 2., 0., 2.), 0.5); 2];
    let tr = [local([-2., 1., 4.], Quat::from_xyzw(0., 0., 3., 3.), 2.); 2];
    let map = PreparedRetarget::try_new(
        &source,
        &["a", "b"],
        &sr,
        &target,
        &["a", "b"],
        &tr,
        MissingJointPolicy::Strict,
    )
    .unwrap();
    let mut pose = sr;
    pose[1].translation[0] += 1.;
    let mut out = [Transform::default(); 2];
    map.map_pose_into(&pose, &mut out).unwrap();
    let expected = out;
    for _ in 0..20 {
        map.map_pose_into(&sr, &mut out).unwrap();
        same_local(out[1], tr[1]);
        near(Quat::from_array(out[1].rotation).length(), 1.);
        map.map_pose_into(&pose, &mut out).unwrap();
        assert_eq!(out, expected);
    }
}

#[test]
fn sibling_subtrees_reorder_but_names_cannot_hide_parent_mismatch() {
    let sr = [Transform::default(); 5];
    let source = rig(&[None, Some(0), Some(1), Some(0), Some(3)]);
    let target = rig(&[None, Some(0), Some(1), Some(0), Some(3)]);
    let sn = ["root", "left", "left_tip", "right", "right_tip"];
    let tn = ["root", "right", "right_tip", "left", "left_tip"];
    let map = PreparedRetarget::try_new(
        &source,
        &sn,
        &sr,
        &target,
        &tn,
        &sr,
        MissingJointPolicy::Strict,
    )
    .unwrap();
    let mut pose = sr;
    for (i, local) in pose.iter_mut().enumerate() {
        local.translation[0] = i as f32;
    }
    let mut out = sr;
    map.map_pose_into(&pose, &mut out).unwrap();
    assert_eq!(out.map(|t| t.translation[0]), [0., 3., 4., 1., 2.]);
    let wrong = rig(&[None, Some(0), Some(0), Some(0), Some(3)]);
    assert!(matches!(
        PreparedRetarget::try_new(
            &source,
            &sn,
            &sr,
            &wrong,
            &tn,
            &sr,
            MissingJointPolicy::Strict
        ),
        Err(RetargetError::HierarchyMismatch {
            source_joint: 4,
            target_joint: 2
        })
    ));
    let roots = rig(&[None, None, Some(1), Some(0), Some(3)]);
    assert!(matches!(
        PreparedRetarget::try_new(
            &source,
            &sn,
            &sr,
            &roots,
            &tn,
            &sr,
            MissingJointPolicy::Strict
        ),
        Err(RetargetError::HierarchyMismatch {
            target_joint: 1,
            ..
        })
    ));
}

#[test]
fn strict_missing_diagnostics_cover_both_sides_and_partial_retains_target_rest() {
    let source = rig(&[None, Some(0)]);
    let target = rig(&[None, Some(0), Some(1)]);
    let sr = [Transform::default(); 2];
    let tr = [local([0., 2., 0.], Quat::IDENTITY, 1.); 3];
    assert!(matches!(
        PreparedRetarget::try_new(
            &source,
            &["root", "unused"],
            &sr,
            &target,
            &["root", "new", "tip"],
            &tr,
            MissingJointPolicy::Strict
        ),
        Err(RetargetError::MissingJoint {
            side: SkeletonSide::Target,
            joint: 1
        })
    ));
    let single = rig(&[None]);
    assert!(matches!(
        PreparedRetarget::try_new(
            &source,
            &["root", "unused"],
            &sr,
            &single,
            &["root"],
            &tr[..1],
            MissingJointPolicy::Strict
        ),
        Err(RetargetError::MissingJoint {
            side: SkeletonSide::Source,
            joint: 1
        })
    ));
    let map = PreparedRetarget::try_new(
        &source,
        &["root", "unused"],
        &sr,
        &target,
        &["root", "new", "tip"],
        &tr,
        MissingJointPolicy::TargetRestFallback,
    )
    .unwrap();
    let mut out = [Transform::default(); 3];
    let mut pose = sr;
    pose[0].translation[0] = 3.;
    map.map_pose_into(&pose, &mut out).unwrap();
    assert_eq!(out[1..], tr[1..]);
    assert_eq!(out[0].translation, [3., 2., 0.]);
    // Even an unused source joint cannot smuggle invalid numeric input through.
    let before = out;
    pose[1].rotation = [0.; 4];
    assert!(matches!(
        map.map_pose_into(&pose, &mut out),
        Err(RetargetError::Transform {
            side: SkeletonSide::Source,
            joint: 1,
            ..
        })
    ));
    assert_eq!(out, before);
}

#[test]
fn partial_policy_does_not_skip_unmatched_parents_or_bind_unrelated_roots() {
    let source = rig(&[None, Some(0)]);
    let target = rig(&[None, Some(0), Some(1)]);
    let sr = [Transform::default(); 2];
    let tr = [Transform::default(); 3];
    assert!(matches!(
        PreparedRetarget::try_new(
            &source,
            &["root", "tip"],
            &sr,
            &target,
            &["root", "new", "tip"],
            &tr,
            MissingJointPolicy::TargetRestFallback
        ),
        Err(RetargetError::HierarchyMismatch {
            source_joint: 1,
            target_joint: 2
        })
    ));
    // Completely disjoint names have an explicit, deterministic rest fallback.
    let map = PreparedRetarget::try_new(
        &source,
        &["a", "b"],
        &sr,
        &target,
        &["x", "y", "z"],
        &tr,
        MissingJointPolicy::TargetRestFallback,
    )
    .unwrap();
    let mut out = [sentinel(); 3];
    map.map_pose_into(&sr, &mut out).unwrap();
    assert_eq!(out, tr);
}

#[test]
fn metadata_lengths_empty_long_duplicate_and_literal_names_are_checked() {
    let rig = rig(&[None, Some(0)]);
    let rest = [Transform::default(); 2];
    let long = "x".repeat(4097);
    for side in [SkeletonSide::Source, SkeletonSide::Target] {
        let attempt = |names: &[&str], locals: &[Transform]| {
            if side == SkeletonSide::Source {
                PreparedRetarget::try_new(
                    &rig,
                    names,
                    locals,
                    &rig,
                    &["root", "child"],
                    &rest,
                    MissingJointPolicy::Strict,
                )
            } else {
                PreparedRetarget::try_new(
                    &rig,
                    &["root", "child"],
                    &rest,
                    &rig,
                    names,
                    locals,
                    MissingJointPolicy::Strict,
                )
            }
        };
        assert!(
            matches!(attempt(&["root"], &rest), Err(RetargetError::MetadataLength { side: s, .. }) if s == side)
        );
        assert!(matches!(
            attempt(&["root", "child"], &rest[..1]),
            Err(RetargetError::MetadataLength { .. })
        ));
        assert!(
            matches!(attempt(&["root", ""], &rest), Err(RetargetError::InvalidName { side: s, joint: 1 }) if s == side)
        );
        assert!(matches!(
            attempt(&["root", &long], &rest),
            Err(RetargetError::InvalidName { .. })
        ));
        assert!(
            matches!(attempt(&["same", "same"], &rest), Err(RetargetError::DuplicateName { side: s, first: 0, duplicate: 1 }) if s == side)
        );
    }
    // No case folding, path parsing, trimming or Unicode normalization.
    PreparedRetarget::try_new(
        &rig,
        &["Root/α", "root/α"],
        &rest,
        &rig,
        &["Root/α", "root/α"],
        &rest,
        MissingJointPolicy::Strict,
    )
    .unwrap();
    let max = "x".repeat(4096);
    PreparedRetarget::try_new(
        &rig,
        &["root", &max],
        &rest,
        &rig,
        &["root", &max],
        &rest,
        MissingJointPolicy::Strict,
    )
    .unwrap();
}

#[test]
fn bound_256_works_and_larger_real_skeletons_are_rejected() {
    for count in [256, 257] {
        let skeleton = rig(&vec![None; count]);
        let names: Vec<_> = (0..count).map(|i| format!("j{i}")).collect();
        let names: Vec<_> = names.iter().map(String::as_str).collect();
        let rest = vec![Transform::default(); count];
        let result = PreparedRetarget::try_new(
            &skeleton,
            &names,
            &rest,
            &skeleton,
            &names,
            &rest,
            MissingJointPolicy::Strict,
        );
        if count == 256 {
            let map = result.unwrap();
            let mut out = rest.clone();
            map.map_pose_into(&rest, &mut out).unwrap();
            assert_eq!(out, rest);
        } else {
            assert!(matches!(
                result,
                Err(RetargetError::JointLimit {
                    side: SkeletonSide::Source,
                    count: 257
                })
            ));
            let small = rig(&[None]);
            assert!(matches!(
                PreparedRetarget::try_new(
                    &small,
                    &["j0"],
                    &rest[..1],
                    &skeleton,
                    &names,
                    &rest,
                    MissingJointPolicy::Strict
                ),
                Err(RetargetError::JointLimit {
                    side: SkeletonSide::Target,
                    count: 257
                })
            ));
        }
    }
}

#[test]
fn unsupported_numeric_rest_poses_fail_on_each_side_including_unmatched_joints() {
    let skeleton = rig(&[None, Some(0)]);
    let good = [Transform::default(); 2];
    let invalid = [
        Transform {
            translation: [f32::NAN, 0., 0.],
            ..good[0]
        },
        Transform {
            translation: [f32::INFINITY, 0., 0.],
            ..good[0]
        },
        Transform {
            rotation: [0.; 4],
            ..good[0]
        },
        Transform {
            rotation: [f32::NEG_INFINITY; 4],
            ..good[0]
        },
        Transform {
            rotation: [1e-30; 4],
            ..good[0]
        },
        Transform {
            scale: [0.; 3],
            ..good[0]
        },
        Transform {
            scale: [-1.; 3],
            ..good[0]
        },
        Transform {
            scale: [-1., 1., 1.],
            ..good[0]
        },
        Transform {
            scale: [1., 2., 1.],
            ..good[0]
        },
        Transform {
            scale: [f32::NAN; 3],
            ..good[0]
        },
        Transform {
            scale: [f32::from_bits(1); 3],
            ..good[0]
        },
    ];
    for bad in invalid {
        for side in [SkeletonSide::Source, SkeletonSide::Target] {
            let rest = [good[0], bad];
            let result = if side == SkeletonSide::Source {
                PreparedRetarget::try_new(
                    &skeleton,
                    &["a", "unused"],
                    &rest,
                    &skeleton,
                    &["a", "other"],
                    &good,
                    MissingJointPolicy::TargetRestFallback,
                )
            } else {
                PreparedRetarget::try_new(
                    &skeleton,
                    &["a", "unused"],
                    &good,
                    &skeleton,
                    &["a", "other"],
                    &rest,
                    MissingJointPolicy::TargetRestFallback,
                )
            };
            assert!(
                matches!(result, Err(RetargetError::Transform { side: s, joint: 1, .. }) if s == side),
                "{bad:?}: {result:?}"
            );
        }
    }
    assert!(matches!(
        PreparedRetarget::try_new(
            &skeleton,
            &["a", "b"],
            &[
                good[0],
                Transform {
                    scale: [1., 2., 1.],
                    ..good[0]
                }
            ],
            &skeleton,
            &["a", "b"],
            &good,
            MissingJointPolicy::Strict
        ),
        Err(RetargetError::Transform {
            error: RetargetTransformError::NonUniformScale,
            ..
        })
    ));
}

#[test]
fn checked_inverse_and_offset_range_failures_are_explicit() {
    let source = rig(&[None]);
    let target = rig(&[None]);
    // Linear inverse fits, but source full inverse translation cannot fit f32.
    let sr = [local([f32::MAX, 0., 0.], Quat::IDENTITY, 1e-20)];
    let tr = [Transform::default()];
    assert!(matches!(
        PreparedRetarget::try_new(
            &source,
            &["a"],
            &sr,
            &target,
            &["a"],
            &tr,
            MissingJointPolicy::Strict
        ),
        Err(RetargetError::Transform {
            error: RetargetTransformError::Matrix(_),
            ..
        })
    ));
    // Both individual scales are representable; their offset is not.
    let sr = [local([0.; 3], Quat::IDENTITY, 1e-20)];
    let tr = [local([0.; 3], Quat::IDENTITY, 1e20)];
    assert!(matches!(
        PreparedRetarget::try_new(
            &source,
            &["a"],
            &sr,
            &target,
            &["a"],
            &tr,
            MissingJointPolicy::Strict
        ),
        Err(RetargetError::Transform {
            side: SkeletonSide::Target,
            error: RetargetTransformError::Affine(_),
            ..
        })
    ));
}

#[test]
fn capacity_input_length_and_numeric_errors_leave_all_output_bits_untouched() {
    let source = rig(&[None, Some(0)]);
    let target = rig(&[None, Some(0)]);
    let rest = [Transform::default(); 2];
    let map = PreparedRetarget::try_new(
        &source,
        &["a", "b"],
        &rest,
        &target,
        &["a", "b"],
        &rest,
        MissingJointPolicy::Strict,
    )
    .unwrap();
    let mut output = [sentinel(); 4];
    let before = bits(&output);
    assert_eq!(
        map.map_pose_into(&rest[..1], &mut output),
        Err(RetargetError::PoseLength {
            required: 2,
            actual: 1
        })
    );
    assert_eq!(
        map.map_pose_into(&rest, &mut output[..1]),
        Err(RetargetError::Capacity {
            required: 2,
            available: 1
        })
    );
    assert_eq!(
        map.map_pose_into(&rest, &mut []),
        Err(RetargetError::Capacity {
            required: 2,
            available: 0
        })
    );
    assert_eq!(bits(&output), before);
    for invalid in [
        local([f32::NAN; 3], Quat::IDENTITY, 1.),
        local([0.; 3], Quat::from_array([0.; 4]), 1.),
        local([0.; 3], Quat::IDENTITY, -1.),
        Transform {
            scale: [1., 1., 1.000001],
            ..rest[0]
        },
    ] {
        let pose = [local([10.; 3], Quat::IDENTITY, 1.), invalid];
        assert!(matches!(
            map.map_pose_into(&pose, &mut output),
            Err(RetargetError::Transform {
                side: SkeletonSide::Source,
                joint: 1,
                ..
            })
        ));
        assert_eq!(bits(&output), before);
    }
    map.map_pose_into(&rest, &mut output).unwrap();
    assert_eq!(output[..2], rest);
    assert_eq!(bits(&output[2..]), before[20..]);
}

#[test]
fn output_translation_or_scale_overflow_on_late_joint_is_atomic_and_reusable() {
    let source = rig(&[None, Some(0)]);
    let target = rig(&[None, Some(0)]);
    let sr = [Transform::default(); 2];
    let tr = [sr[0], local([0.; 3], Quat::IDENTITY, 2.)];
    let map = PreparedRetarget::try_new(
        &source,
        &["a", "b"],
        &sr,
        &target,
        &["a", "b"],
        &tr,
        MissingJointPolicy::Strict,
    )
    .unwrap();
    for bad in [
        local([f32::MAX, 0., 0.], Quat::IDENTITY, 1.),
        local([0.; 3], Quat::IDENTITY, f32::MAX),
    ] {
        let mut out = [sentinel(); 3];
        let previous = bits(&out);
        assert!(matches!(
            map.map_pose_into(&[sr[0], bad], &mut out),
            Err(RetargetError::Transform {
                side: SkeletonSide::Target,
                joint: 1,
                ..
            })
        ));
        assert_eq!(bits(&out), previous);
        map.map_pose_into(&sr, &mut out).unwrap();
        assert_eq!(out[..2], tr);
        assert_eq!(bits(&out[2..]), previous[20..]);
    }
}

#[test]
fn output_scale_underflow_is_rejected_atomically_with_error_chain() {
    use std::error::Error;
    let source = rig(&[None]);
    let target = rig(&[None]);
    let sr = [Transform::default()];
    let tr = [local([0.; 3], Quat::IDENTITY, 1e-20)];
    let map = PreparedRetarget::try_new(
        &source,
        &["r"],
        &sr,
        &target,
        &["r"],
        &tr,
        MissingJointPolicy::Strict,
    )
    .unwrap();
    let mut out = [sentinel(); 2];
    let before = bits(&out);
    let error = map
        .map_pose_into(&[local([0.; 3], Quat::IDENTITY, 1e-30)], &mut out)
        .unwrap_err();
    assert_eq!(
        error,
        RetargetError::Transform {
            side: SkeletonSide::Target,
            joint: 0,
            error: RetargetTransformError::NonPositiveScale
        }
    );
    assert!(error.source().is_some());
    assert!(error.to_string().contains("Target joint 0"));
    assert_eq!(bits(&out), before);
    map.map_pose_into(&sr, &mut out).unwrap();
    assert_eq!(out[0], tr[0]);
}

#[test]
fn huge_quaternions_extreme_supported_scales_and_antipodes_stay_finite() {
    let skeleton = rig(&[None]);
    for scale in [1e-30, 1e30] {
        let rest = [local([0.; 3], Quat::IDENTITY, scale)];
        let map = PreparedRetarget::try_new(
            &skeleton,
            &["r"],
            &rest,
            &skeleton,
            &["r"],
            &rest,
            MissingJointPolicy::Strict,
        )
        .unwrap();
        let mut out = [Transform::default()];
        let pose = [local([1., -2., 3.], Quat::from_array([f32::MAX; 4]), scale)];
        map.map_pose_into(&pose, &mut out).unwrap();
        same_local(
            out[0],
            local([1., -2., 3.], Quat::from_array([0.5; 4]), scale),
        );
        let first = out[0];
        let antipode = [Transform {
            rotation: [-f32::MAX; 4],
            ..pose[0]
        }];
        map.map_pose_into(&antipode, &mut out).unwrap();
        assert!(
            Quat::from_array(first.rotation)
                .dot(Quat::from_array(out[0].rotation))
                .abs()
                > 0.99999
        );
        assert!(out[0].valid());
    }
}

#[test]
fn failed_replacement_retains_last_good_map_and_names_are_not_retained() {
    let skeleton = rig(&[None]);
    let rest = [Transform::default()];
    let mut map = {
        let names = [String::from("r")];
        let temporary_rest = [Transform::default()];
        PreparedRetarget::try_new(
            &skeleton,
            &[&names[0]],
            &temporary_rest,
            &skeleton,
            &[&names[0]],
            &temporary_rest,
            MissingJointPolicy::Strict,
        )
        .unwrap()
    };
    let replacement = PreparedRetarget::try_new(
        &skeleton,
        &["r"],
        &rest,
        &skeleton,
        &["different"],
        &rest,
        MissingJointPolicy::Strict,
    );
    assert!(matches!(
        replacement,
        Err(RetargetError::MissingJoint { .. })
    ));
    if let Ok(replacement) = replacement {
        map = replacement;
    }
    let pose = [local([2., 3., 4.], Quat::from_rotation_x(0.2), 1.)];
    let mut out = rest;
    map.map_pose_into(&pose, &mut out).unwrap();
    same_local(out[0], pose[0]);
    let changed_rest = [local([10., 0., 0.], Quat::IDENTITY, 1.)];
    map = PreparedRetarget::try_new(
        &skeleton,
        &["r"],
        &rest,
        &skeleton,
        &["r"],
        &changed_rest,
        MissingJointPolicy::Strict,
    )
    .unwrap();
    let mut out = rest;
    map.map_pose_into(&rest, &mut out).unwrap();
    assert_eq!(out, changed_rest);
}

#[test]
fn warmed_mapping_errors_rest_reset_world_and_skin_have_zero_allocations() {
    let parents: Vec<_> = (0..256)
        .map(|i| if i == 0 { None } else { Some(0) })
        .collect();
    let skeleton = rig(&parents);
    let names: Vec<_> = (0..256).map(|i| format!("j{i}")).collect();
    let names: Vec<_> = names.iter().map(String::as_str).collect();
    let rest = [Transform::default(); 256];
    let map = PreparedRetarget::try_new(
        &skeleton,
        &names,
        &rest,
        &skeleton,
        &names,
        &rest,
        MissingJointPolicy::Strict,
    )
    .unwrap();
    let pose = [local([0.5, 0.2, -0.1], Quat::from_rotation_y(0.2), 1.); 256];
    let mut bad = pose;
    bad[255].rotation = [0.; 4];
    let mut output = rest;
    let mut world = [IDENTITY; 256];
    let mut palette = world;
    map.map_pose_into(&pose, &mut output).unwrap();
    skeleton
        .skin_palette_into(&output, &mut world, &mut palette)
        .unwrap();
    // Positive control: verify this thread's instrumentation observes an
    // actual allocation before measuring the warmed production operations.
    ALLOCATIONS.with(|n| n.set(0));
    RECORD.with(|on| on.set(true));
    let probe = std::hint::black_box(Box::new(7_u8));
    RECORD.with(|on| on.set(false));
    assert_eq!(*probe, 7);
    assert!(ALLOCATIONS.with(Cell::get) > 0);
    ALLOCATIONS.with(|n| n.set(0));
    RECORD.with(|on| on.set(true));
    for _ in 0..128 {
        map.map_pose_into(&pose, &mut output).unwrap();
        skeleton
            .skin_palette_into(&output, &mut world, &mut palette)
            .unwrap();
        assert!(map.map_pose_into(&bad, &mut output).is_err());
        assert!(map.map_pose_into(&pose, &mut output[..255]).is_err());
        map.map_pose_into(&rest, &mut output).unwrap();
    }
    RECORD.with(|on| on.set(false));
    assert_eq!(ALLOCATIONS.with(Cell::get), 0);
    assert_eq!(output, rest);
}
