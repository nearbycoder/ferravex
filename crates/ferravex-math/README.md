# ferravex-math

Original scalar Rust mathematics for Ferravex. No third-party packages, copied library implementations, platform FFI, unsafe code, or generated source. The library uses only Rust's standard library. Types have public fields and predictable `#[repr(C)]` layouts; no SIMD alignment or serialized memory-layout compatibility is promised.

```rust
use ferravex_math::{Mat4, Quat, Vec3, vec3};
let model = Mat4::from_scale_rotation_translation(
    Vec3::splat(2.0),
    Quat::from_rotation_y(0.5),
    vec3(4.0, 0.0, -2.0),
);
let world = model.transform_point3(Vec3::X);
let local = model.try_inverse().unwrap().transform_point3(world);
assert!(local.abs_diff_eq(Vec3::X, 0.00001));
```

## Conventions

- `Vec2`, `Vec3`, and `Vec4` use `f32` components. Indexing follows x, y, z, w. Vector/vector multiplication and division are componentwise; `dot` and `cross` are explicit. Common swizzles are inherent methods. Compatibility swizzle trait names are available for import, but methods do not require those traits.
- Angles are radians; rotations follow the right-hand rule. A quaternion stores x, y, z, w. `a * b` applies b first, then a. Quaternion/vector multiplication requires a unit quaternion. Axis-angle constructors normalize a finite nonzero axis. Matrix construction normalizes its quaternion.
- Euler sequences are intrinsic: `from_euler(EulerRot::XYZ, a, b, c)` means `Rx(a) * Ry(b) * Rz(c)`. All six non-repeating axis orders are supported. The arguments correspond to the order named by the sequence.
- `Mat4` is column-major with named columns `x_axis` through `w_axis`, and acts on column vectors. An affine model matrix is translation × rotation × scale. `transform_point3` uses w=1 without perspective division; `transform_vector3` uses w=0; `project_point3` divides by the resulting w.
- Right-handed view matrices look down camera -Z. Left-handed views look down +Z. `perspective_rh_gl` and `orthographic_rh_gl` map near/far depth to [-1, 1]; their `_rh` and `_lh` counterparts map to [0, 1]. Infinite and reverse-depth right-handed perspectives are also provided. Projection functions panic for invalid frustums. Orthographic functions allow intentionally reversed ranges.
- `Rect::contains` includes boundary points. `Rect::overlaps` requires positive area, so touching edges are not penetration. Negative extents can be converted explicitly with `normalized`.
- `Color` stores unpremultiplied floating-point RGBA with no implicit gamma conversion. `from_hex` interprets opaque 0xRRGGBB. Converting to 8-bit channels clamps and rounds.

## Degenerate inputs and precision

`try_normalize` returns `None` for zero or non-finite vectors. It accepts finite subnormal and extreme-magnitude vectors using wider intermediate arithmetic. `normalize_or_zero` / `normalize_or` provide explicit fallbacks. Unchecked `normalize` returns NaNs for invalid input. A zero vector remains zero when clamping to a minimum length because it has no direction.

`Quat::try_inverse` accepts any finite nonzero quaternion whose inverse fits in f32. Quaternion `inverse` is an algebraic inverse, not just conjugation. Slerp normalizes its endpoints and chooses the shortest rotation, including equivalent q/-q endpoints. A zero-length rotation direction or degenerate look-at basis propagates NaNs; it does not invent a camera orientation.

`Mat4::try_inverse` performs general Gauss–Jordan inversion with partial pivoting and f64 intermediates. Singular matrices, non-finite inputs, and results outside f32 range return `None`; `inverse` returns a NaN matrix in those cases. Ill-conditioned but invertible matrices are not rejected by an arbitrary epsilon, so applications must still account for floating-point error. `try_to_scale_rotation_translation` rejects zero scale, non-affine matrices, and shear; reflected TRS transforms are supported.

Ordinary vector arithmetic, dot/cross, matrix multiplication, and output storage use f32. No cross-platform bitwise determinism, SIMD speedup, complete glam API compatibility, or GPU ABI compatibility is claimed. Geometric tests use numerical tolerances.

## Validation

```sh
cargo test -p ferravex-math
cargo clippy -p ferravex-math --all-targets -- -D warnings
cargo check -p ferravex-math --target wasm32-unknown-unknown
```

Tests cover vector arithmetic/swizzles, extreme normalization, cross-product orthogonality, reflection, quaternion composition/inverse and antipodal interpolation, column layout, affine point/vector semantics, general matrix inversion/pivoting, perspective and orthographic near/far planes, view transforms, degenerate inputs, strict rectangle contact, and 256 deterministic randomized TRS round trips including mirrored scale.
