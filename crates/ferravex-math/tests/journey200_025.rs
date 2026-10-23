use ferravex_math::{Vec2, Vec3, Vec4};

fn wide_length<const N: usize>(components: [f32; N]) -> f64 {
    components.into_iter().fold(0.0_f64, |length, component| {
        length.hypot(f64::from(component))
    })
}

fn assert_length_near<const N: usize>(components: [f32; N], expected: f64) {
    let actual = wide_length(components);
    let tolerance =
        expected * f64::from(2.0 * f32::EPSILON) + N as f64 * f64::from(f32::from_bits(1));
    assert!(
        (actual - expected).abs() <= tolerance,
        "length {actual} != {expected}, components {components:?}"
    );
}

macro_rules! vector_checks {
    ($module:ident, $vector:ident, $n:expr) => {
        mod $module {
            use super::*;
            type Vector = $vector;

            #[test]
            fn normalization_threshold_is_strict_and_uses_a_wide_length() {
                let direction = Vector::X * 3.0 + Vector::Y * 4.0;
                assert_eq!(direction.try_normalize_with_min_length(5.0), None);
                assert_eq!(direction.try_normalize_with_min_length(6.0), None);
                assert_eq!(
                    direction.try_normalize_with_min_length(4.0),
                    direction.try_normalize()
                );

                let tiny = -Vector::X * f32::from_bits(1);
                assert_eq!(tiny.try_normalize(), Some(-Vector::X));
                assert_eq!(tiny.try_normalize_with_min_length(0.0), Some(-Vector::X));
                assert_eq!(tiny.try_normalize_with_min_length(f32::from_bits(1)), None);

                let huge = Vector::splat(f32::MAX);
                assert!(huge.length().is_infinite());
                let unit = huge.try_normalize_with_min_length(f32::MAX).unwrap();
                assert!(unit.is_finite());
                assert_length_near(unit.to_array(), 1.0);
            }

            #[test]
            fn normalization_and_clamping_cover_the_f32_exponent_range() {
                let magnitudes = (0..=254)
                    .map(|exponent| f32::from_bits((exponent << 23) | 0x0040_0000))
                    .chain([f32::from_bits(1), f32::MAX]);
                for magnitude in magnitudes {
                    let input = Vector::from_array(std::array::from_fn(|i| {
                        magnitude / (i + 1) as f32 * if i % 2 == 0 { 1.0 } else { -1.0 }
                    }));
                    let unit = input.try_normalize().unwrap();
                    assert!(unit.is_finite());
                    assert_length_near(unit.to_array(), 1.0);
                    assert_eq!(
                        input
                            .try_normalize_with_min_length(0.0)
                            .unwrap()
                            .to_array()
                            .map(f32::to_bits),
                        unit.to_array().map(f32::to_bits)
                    );
                    assert_eq!(input.normalize(), unit);
                    assert_eq!(input.normalize_or_zero(), unit);
                    assert_eq!(input.normalize_or(Vector::Y), unit);

                    for bound in [f32::from_bits(1), f32::MIN_POSITIVE, 1.0, f32::MAX] {
                        let capped = input.try_clamp_length_max(bound).unwrap();
                        let raised = input.try_clamp_length_min(bound).unwrap();
                        let resized = input.try_clamp_length(bound, bound).unwrap();
                        assert!(capped.is_finite() && raised.is_finite() && resized.is_finite());
                        let length = wide_length(input.to_array());
                        assert_length_near(capped.to_array(), length.min(f64::from(bound)));
                        assert_length_near(raised.to_array(), length.max(f64::from(bound)));
                        assert_length_near(resized.to_array(), f64::from(bound));
                        for i in 0..$n {
                            for output in [unit, capped, raised, resized] {
                                assert_eq!(
                                    output[i].is_sign_negative(),
                                    input[i].is_sign_negative()
                                );
                            }
                        }
                    }
                }
            }

            #[test]
            fn non_finite_components_and_invalid_bounds_are_rejected() {
                for index in 0..$n {
                    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
                        let mut input = Vector::ONE;
                        input[index] = bad;
                        assert_eq!(input.try_normalize(), None);
                        assert_eq!(input.try_normalize_with_min_length(0.0), None);
                        assert_eq!(input.try_clamp_length(0.0, 1.0), None);
                        assert_eq!(input.try_clamp_length_max(0.0), None);
                        assert_eq!(input.try_clamp_length_max(1.0), None);
                        assert_eq!(input.try_clamp_length_min(0.0), None);
                        assert_eq!(input.try_clamp_length_min(1.0), None);
                        assert!(input.normalize().is_nan());
                        assert_eq!(input.normalize_or_zero(), Vector::ZERO);
                        assert_eq!(input.normalize_or(Vector::Y), Vector::Y);
                    }
                }
                for bad in [
                    -f32::from_bits(1),
                    -1.0,
                    f32::NAN,
                    f32::INFINITY,
                    f32::NEG_INFINITY,
                ] {
                    for input in [Vector::ZERO, Vector::X, Vector::splat(f32::MAX)] {
                        assert_eq!(input.try_normalize_with_min_length(bad), None);
                        assert_eq!(input.try_clamp_length(bad, 1.0), None);
                        assert_eq!(input.try_clamp_length(0.0, bad), None);
                        assert_eq!(input.try_clamp_length_max(bad), None);
                        assert_eq!(input.try_clamp_length_min(bad), None);
                    }
                }
                assert_eq!(Vector::ONE.try_clamp_length(2.0, 1.0), None);
            }

            #[test]
            fn zero_has_no_direction_to_grow_and_signed_zero_is_preserved() {
                let zero =
                    Vector::from_array(std::array::from_fn(
                        |i| {
                            if i % 2 == 0 { -0.0 } else { 0.0 }
                        },
                    ));
                assert_eq!(zero.try_normalize(), None);
                assert_eq!(zero.try_normalize_with_min_length(-0.0), None);
                assert_eq!(zero.try_clamp_length(1.0, 2.0), None);
                assert_eq!(zero.try_clamp_length_min(1.0), None);
                for unchanged in [
                    zero.try_clamp_length(-0.0, 0.0),
                    zero.try_clamp_length(0.0, 2.0),
                    zero.try_clamp_length_max(-0.0),
                    zero.try_clamp_length_max(2.0),
                    zero.try_clamp_length_min(-0.0),
                ] {
                    assert_eq!(
                        unchanged.unwrap().to_array().map(f32::to_bits),
                        zero.to_array().map(f32::to_bits)
                    );
                }
                assert_eq!(Vector::ONE.try_clamp_length_max(0.0), Some(Vector::ZERO));
                assert_eq!(
                    Vector::splat(f32::MAX).try_clamp_length(0.0, 0.0),
                    Some(Vector::ZERO)
                );
                // Legacy minimum clamping deliberately keeps its zero fallback.
                assert_eq!(zero.clamp_length_min(1.0), Vector::ZERO);
                assert_eq!(zero.clamp_length(1.0, 2.0), Vector::ZERO);
                assert_eq!(zero.normalize_or_zero(), Vector::ZERO);
                assert!(zero.normalize().is_nan());
            }

            #[test]
            fn range_clamping_handles_both_ends_and_preserves_in_range_bits() {
                let mut direction = Vector::splat(-0.0);
                direction[0] = 3.0;
                direction[1] = -4.0;
                for output in [
                    direction.try_clamp_length(5.0, 5.0),
                    direction.try_clamp_length(1.0, 10.0),
                    direction.try_clamp_length_max(5.0),
                    direction.try_clamp_length_min(5.0),
                ] {
                    assert_eq!(
                        output.unwrap().to_array().map(f32::to_bits),
                        direction.to_array().map(f32::to_bits)
                    );
                }
                assert_eq!(direction.try_clamp_length(1.0, 2.5), Some(direction * 0.5));
                assert_eq!(
                    direction.try_clamp_length(10.0, 20.0),
                    Some(direction * 2.0)
                );
                let huge = Vector::splat(f32::MAX);
                assert_eq!(huge.try_clamp_length_min(f32::MAX), Some(huge));
            }

            #[test]
            fn scaling_avoids_loss_from_a_normalized_f32_intermediate() {
                let mut huge = Vector::ZERO;
                huge[0] = f32::MAX;
                huge[1] = 1.0e-10;
                assert_eq!(huge.try_normalize().unwrap()[1], 0.0);
                let capped = huge.try_clamp_length_max(f32::MAX * 0.5).unwrap();
                assert_eq!(capped, huge * 0.5);
                assert!(capped[1] > 0.0);

                let tiny = -Vector::X * f32::from_bits(1);
                assert_eq!(
                    tiny.try_clamp_length_min(f32::MAX),
                    Some(-Vector::X * f32::MAX)
                );
                assert_eq!(
                    (-Vector::X * f32::MAX).try_clamp_length_max(f32::from_bits(1)),
                    Some(tiny)
                );
            }

            #[test]
            fn subnormal_limits_compare_lengths_before_rounding_to_f32() {
                let tiny = f32::from_bits(1);
                let diagonal = Vector::X * tiny + Vector::Y * tiny;
                // Its public f32 length rounds down to `tiny`, but its wide
                // length is greater, so the threshold must accept it.
                assert_eq!(diagonal.length(), tiny);
                assert!(diagonal.try_normalize_with_min_length(tiny).is_some());
                assert_eq!(diagonal.try_clamp_length_min(tiny), Some(diagonal));
                // The nearest f32 components at this magnitude are still `tiny`:
                // clamping promises rounded lengths, not exact real-number bounds.
                assert_eq!(diagonal.try_clamp_length_max(tiny), Some(diagonal));
            }

            #[test]
            #[should_panic]
            fn legacy_length_clamping_still_panics_on_invalid_bounds() {
                let _ = Vector::ONE.clamp_length(2.0, 1.0);
            }
        }
    };
}

vector_checks!(vec2, Vec2, 2);
vector_checks!(vec3, Vec3, 3);
vector_checks!(vec4, Vec4, 4);
