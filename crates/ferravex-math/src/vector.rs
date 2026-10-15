use std::ops::{
    Add, AddAssign, Div, DivAssign, Index, IndexMut, Mul, MulAssign, Neg, Sub, SubAssign,
};

macro_rules! vector {
    ($name:ident, $mask:ident, $n:expr, $($field:ident : $index:tt),+) => {
        #[repr(C)]
        #[derive(Clone, Copy, Debug, Default, PartialEq)]
        pub struct $name { $(pub $field: f32),+ }

        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
        pub struct $mask { $(pub $field: bool),+ }
        impl $mask {
            pub const fn all(self) -> bool { true $(&& self.$field)+ }
            pub const fn any(self) -> bool { false $(|| self.$field)+ }
            pub const fn to_array(self) -> [bool; $n] { [$(self.$field),+] }
        }
        impl $name {
            pub const ZERO: Self = Self::splat(0.0);
            pub const ONE: Self = Self::splat(1.0);
            pub const NEG_ONE: Self = Self::splat(-1.0);
            pub const NAN: Self = Self::splat(f32::NAN);
            pub const INFINITY: Self = Self::splat(f32::INFINITY);
            pub const NEG_INFINITY: Self = Self::splat(f32::NEG_INFINITY);
            pub const fn new($($field: f32),+) -> Self { Self {$($field),+} }
            pub const fn splat(value: f32) -> Self { Self {$($field: value),+} }
            pub const fn from_array(values: [f32; $n]) -> Self { Self {$($field: values[$index]),+} }
            pub const fn to_array(self) -> [f32; $n] { [$(self.$field),+] }
            pub fn from_slice(values: &[f32]) -> Self { Self {$($field: values[$index]),+} }
            pub fn write_to_slice(self, values: &mut [f32]) { $(values[$index] = self.$field;)+ }
            pub fn is_finite(self) -> bool { true $(&& self.$field.is_finite())+ }
            pub fn is_nan(self) -> bool { false $(|| self.$field.is_nan())+ }
            pub fn dot(self, rhs: Self) -> f32 { 0.0 $(+ self.$field * rhs.$field)+ }
            pub fn length_squared(self) -> f32 { self.dot(self) }
            /// Uses wider intermediates so finite small vectors do not underflow.
            pub fn length(self) -> f32 { (0.0_f64 $(+ f64::from(self.$field).powi(2))+).sqrt() as f32 }
            pub fn length_recip(self) -> f32 { self.length().recip() }
            pub fn distance(self, other: Self) -> f32 { (self-other).length() }
            pub fn distance_squared(self, other: Self) -> f32 { (self-other).length_squared() }
            /// Normalizes every finite nonzero vector, including subnormal inputs.
            pub fn try_normalize(self) -> Option<Self> {
                let length = (0.0_f64 $(+ f64::from(self.$field).powi(2))+).sqrt();
                if length > 0.0 && length.is_finite() {
                    Some(Self {$($field: (f64::from(self.$field) / length) as f32),+})
                } else { None }
            }
            pub fn normalize(self) -> Self { self.try_normalize().unwrap_or(Self::NAN) }
            pub fn normalize_or_zero(self) -> Self { self.try_normalize().unwrap_or(Self::ZERO) }
            pub fn normalize_or(self, fallback: Self) -> Self { self.try_normalize().unwrap_or(fallback) }
            pub fn is_normalized(self) -> bool { (self.length_squared()-1.0).abs() <= 1.0e-5 }
            pub fn min(self, rhs: Self) -> Self { Self {$($field: self.$field.min(rhs.$field)),+} }
            pub fn max(self, rhs: Self) -> Self { Self {$($field: self.$field.max(rhs.$field)),+} }
            pub fn min_element(self) -> f32 { self.to_array().into_iter().fold(f32::INFINITY, f32::min) }
            pub fn max_element(self) -> f32 { self.to_array().into_iter().fold(f32::NEG_INFINITY, f32::max) }
            pub fn element_sum(self) -> f32 { 0.0 $(+ self.$field)+ }
            pub fn element_product(self) -> f32 { 1.0 $(* self.$field)+ }
            pub fn abs(self) -> Self { Self {$($field: self.$field.abs()),+} }
            pub fn signum(self) -> Self { Self {$($field: self.$field.signum()),+} }
            pub fn floor(self) -> Self { Self {$($field: self.$field.floor()),+} }
            pub fn ceil(self) -> Self { Self {$($field: self.$field.ceil()),+} }
            pub fn round(self) -> Self { Self {$($field: self.$field.round()),+} }
            pub fn trunc(self) -> Self { Self {$($field: self.$field.trunc()),+} }
            pub fn fract(self) -> Self { self-self.floor() }
            pub fn recip(self) -> Self { Self {$($field: self.$field.recip()),+} }
            pub fn sqrt(self) -> Self { Self {$($field: self.$field.sqrt()),+} }
            pub fn powf(self, exponent: f32) -> Self { Self {$($field: self.$field.powf(exponent)),+} }
            pub fn exp(self) -> Self { Self {$($field: self.$field.exp()),+} }
            pub fn clamp(self, min: Self, max: Self) -> Self { Self {$($field: self.$field.clamp(min.$field, max.$field)),+} }
            pub fn clamp_length(self, min: f32, max: f32) -> Self {
                assert!(min.is_finite() && max.is_finite() && min >= 0.0 && min <= max);
                let length = self.length();
                if length > max { self.normalize_or_zero()*max }
                else if length < min { self.normalize_or_zero()*min }
                else { self }
            }
            pub fn clamp_length_max(self, max: f32) -> Self {
                assert!(max.is_finite() && max >= 0.0);
                if self.length() > max { self.normalize_or_zero()*max } else { self }
            }
            pub fn clamp_length_min(self, min: f32) -> Self {
                assert!(min.is_finite() && min >= 0.0);
                if self.length() < min { self.normalize_or_zero()*min } else { self }
            }
            pub fn lerp(self, rhs: Self, t: f32) -> Self { self * (1.0-t) + rhs*t }
            pub fn mul_add(self, multiplier: Self, add: Self) -> Self { Self {$($field: self.$field.mul_add(multiplier.$field, add.$field)),+} }
            pub fn abs_diff_eq(self, rhs: Self, tolerance: f32) -> bool { (self-rhs).abs().cmple(Self::splat(tolerance)).all() }
            pub fn cmpeq(self, rhs: Self) -> $mask { $mask {$($field: self.$field == rhs.$field),+} }
            pub fn cmpne(self, rhs: Self) -> $mask { $mask {$($field: self.$field != rhs.$field),+} }
            pub fn cmplt(self, rhs: Self) -> $mask { $mask {$($field: self.$field < rhs.$field),+} }
            pub fn cmple(self, rhs: Self) -> $mask { $mask {$($field: self.$field <= rhs.$field),+} }
            pub fn cmpgt(self, rhs: Self) -> $mask { $mask {$($field: self.$field > rhs.$field),+} }
            pub fn cmpge(self, rhs: Self) -> $mask { $mask {$($field: self.$field >= rhs.$field),+} }
            pub fn select(mask: $mask, if_true: Self, if_false: Self) -> Self { Self {$($field: if mask.$field {if_true.$field} else {if_false.$field}),+} }
            pub fn project_onto(self, other: Self) -> Self { other * (self.dot(other)/other.length_squared()) }
            pub fn reject_from(self, other: Self) -> Self { self-self.project_onto(other) }
            pub fn project_onto_normalized(self, normal: Self) -> Self { normal*self.dot(normal) }
            pub fn reject_from_normalized(self, normal: Self) -> Self { self-self.project_onto_normalized(normal) }
            /// Reflects an incident vector about a unit surface normal.
            pub fn reflect(self, normal: Self) -> Self { self-2.0*self.dot(normal)*normal }
            pub fn move_towards(self, rhs: Self, max_distance: f32) -> Self {
                assert!(max_distance.is_finite() && max_distance >= 0.0);
                let difference=rhs-self;
                if difference.length() <= max_distance {rhs} else {self+difference.normalize_or_zero()*max_distance}
            }
        }
        impl From<[f32; $n]> for $name { fn from(v: [f32; $n]) -> Self { Self::from_array(v) } }
        impl From<$name> for [f32; $n] { fn from(v: $name) -> Self { v.to_array() } }
        impl Index<usize> for $name { type Output=f32; fn index(&self,index:usize)->&f32 { match index {$($index=>&self.$field,)+ _=>panic!("vector index {index} is out of range")} } }
        impl IndexMut<usize> for $name { fn index_mut(&mut self,index:usize)->&mut f32 { match index {$($index=>&mut self.$field,)+ _=>panic!("vector index {index} is out of range")} } }
        impl Neg for $name { type Output=Self; fn neg(self)->Self { Self {$($field:-self.$field),+} } }
        impl Add for $name { type Output=Self; fn add(self,rhs:Self)->Self { Self {$($field:self.$field+rhs.$field),+} } }
        impl Sub for $name { type Output=Self; fn sub(self,rhs:Self)->Self { Self {$($field:self.$field-rhs.$field),+} } }
        impl Mul for $name { type Output=Self; fn mul(self,rhs:Self)->Self { Self {$($field:self.$field*rhs.$field),+} } }
        impl Div for $name { type Output=Self; fn div(self,rhs:Self)->Self { Self {$($field:self.$field/rhs.$field),+} } }
        impl Add<f32> for $name { type Output=Self; fn add(self,rhs:f32)->Self { self+Self::splat(rhs) } }
        impl Sub<f32> for $name { type Output=Self; fn sub(self,rhs:f32)->Self { self-Self::splat(rhs) } }
        impl Mul<f32> for $name { type Output=Self; fn mul(self,rhs:f32)->Self { Self {$($field:self.$field*rhs),+} } }
        impl Div<f32> for $name { type Output=Self; fn div(self,rhs:f32)->Self { Self {$($field:self.$field/rhs),+} } }
        impl Add<$name> for f32 { type Output=$name; fn add(self,rhs:$name)->$name { $name::splat(self)+rhs } }
        impl Sub<$name> for f32 { type Output=$name; fn sub(self,rhs:$name)->$name { $name::splat(self)-rhs } }
        impl Mul<$name> for f32 { type Output=$name; fn mul(self,rhs:$name)->$name { rhs*self } }
        impl Div<$name> for f32 { type Output=$name; fn div(self,rhs:$name)->$name { $name::splat(self)/rhs } }
        impl AddAssign for $name { fn add_assign(&mut self,rhs:Self) { *self=*self+rhs; } }
        impl SubAssign for $name { fn sub_assign(&mut self,rhs:Self) { *self=*self-rhs; } }
        impl MulAssign for $name { fn mul_assign(&mut self,rhs:Self) { *self=*self*rhs; } }
        impl DivAssign for $name { fn div_assign(&mut self,rhs:Self) { *self=*self/rhs; } }
        impl AddAssign<f32> for $name { fn add_assign(&mut self,rhs:f32) { *self=*self+rhs; } }
        impl SubAssign<f32> for $name { fn sub_assign(&mut self,rhs:f32) { *self=*self-rhs; } }
        impl MulAssign<f32> for $name { fn mul_assign(&mut self,rhs:f32) { *self=*self*rhs; } }
        impl DivAssign<f32> for $name { fn div_assign(&mut self,rhs:f32) { *self=*self/rhs; } }
        impl std::iter::Sum for $name { fn sum<I:Iterator<Item=Self>>(iter:I)->Self { iter.fold(Self::ZERO, |a,b|a+b) } }
        impl std::iter::Product for $name { fn product<I:Iterator<Item=Self>>(iter:I)->Self { iter.fold(Self::ONE, |a,b|a*b) } }
    }
}
vector!(Vec2, BVec2, 2, x:0,y:1);
vector!(Vec3, BVec3, 3, x:0,y:1,z:2);
vector!(Vec4, BVec4, 4, x:0,y:1,z:2,w:3);

pub const fn vec2(x: f32, y: f32) -> Vec2 {
    Vec2::new(x, y)
}
pub const fn vec3(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(x, y, z)
}
pub const fn vec4(x: f32, y: f32, z: f32, w: f32) -> Vec4 {
    Vec4::new(x, y, z, w)
}
impl From<(f32, f32)> for Vec2 {
    fn from(v: (f32, f32)) -> Self {
        Self::new(v.0, v.1)
    }
}
impl From<Vec2> for (f32, f32) {
    fn from(v: Vec2) -> Self {
        (v.x, v.y)
    }
}
impl From<(f32, f32, f32)> for Vec3 {
    fn from(v: (f32, f32, f32)) -> Self {
        Self::new(v.0, v.1, v.2)
    }
}
impl From<(f32, f32, f32, f32)> for Vec4 {
    fn from(v: (f32, f32, f32, f32)) -> Self {
        Self::new(v.0, v.1, v.2, v.3)
    }
}
impl Vec2 {
    pub const X: Self = Self::new(1.0, 0.0);
    pub const Y: Self = Self::new(0.0, 1.0);
    pub const NEG_X: Self = Self::new(-1.0, 0.0);
    pub const NEG_Y: Self = Self::new(0.0, -1.0);
    pub const AXES: [Self; 2] = [Self::X, Self::Y];
    pub const fn extend(self, z: f32) -> Vec3 {
        vec3(self.x, self.y, z)
    }
    pub const fn perp(self) -> Self {
        vec2(-self.y, self.x)
    }
    pub fn perp_dot(self, rhs: Self) -> f32 {
        self.x * rhs.y - self.y * rhs.x
    }
    pub fn from_angle(angle: f32) -> Self {
        let (s, c) = angle.sin_cos();
        vec2(c, s)
    }
    pub fn to_angle(self) -> f32 {
        self.y.atan2(self.x)
    }
    /// Signed shortest angle from this direction to `rhs` in [-pi, pi].
    pub fn angle_between(self, rhs: Self) -> f32 {
        let a = self.normalize();
        let b = rhs.normalize();
        a.perp_dot(b).atan2(a.dot(b))
    }
    /// Complex multiplication, useful for applying a unit 2D rotation.
    pub fn rotate(self, rhs: Self) -> Self {
        vec2(
            self.x * rhs.x - self.y * rhs.y,
            self.y * rhs.x + self.x * rhs.y,
        )
    }
    pub const fn yx(self) -> Self {
        vec2(self.y, self.x)
    }
    pub const fn xx(self) -> Self {
        Self::splat(self.x)
    }
    pub const fn yy(self) -> Self {
        Self::splat(self.y)
    }
}
impl Vec3 {
    pub const X: Self = Self::new(1.0, 0.0, 0.0);
    pub const Y: Self = Self::new(0.0, 1.0, 0.0);
    pub const Z: Self = Self::new(0.0, 0.0, 1.0);
    pub const NEG_X: Self = Self::new(-1.0, 0.0, 0.0);
    pub const NEG_Y: Self = Self::new(0.0, -1.0, 0.0);
    pub const NEG_Z: Self = Self::new(0.0, 0.0, -1.0);
    pub const AXES: [Self; 3] = [Self::X, Self::Y, Self::Z];
    pub const fn extend(self, w: f32) -> Vec4 {
        vec4(self.x, self.y, self.z, w)
    }
    pub const fn truncate(self) -> Vec2 {
        vec2(self.x, self.y)
    }
    pub fn cross(self, rhs: Self) -> Self {
        vec3(
            self.y * rhs.z - self.z * rhs.y,
            self.z * rhs.x - self.x * rhs.z,
            self.x * rhs.y - self.y * rhs.x,
        )
    }
    pub fn angle_between(self, rhs: Self) -> f32 {
        self.normalize()
            .dot(rhs.normalize())
            .clamp(-1.0, 1.0)
            .acos()
    }
    pub fn any_orthogonal_vector(self) -> Self {
        if self.x.abs() < self.y.abs() {
            self.cross(Self::X)
        } else {
            self.cross(Self::Y)
        }
    }
    pub fn any_orthonormal_vector(self) -> Self {
        self.any_orthogonal_vector().normalize()
    }
    pub const fn xy(self) -> Vec2 {
        vec2(self.x, self.y)
    }
    pub const fn xz(self) -> Vec2 {
        vec2(self.x, self.z)
    }
    pub const fn yz(self) -> Vec2 {
        vec2(self.y, self.z)
    }
    pub const fn yx(self) -> Vec2 {
        vec2(self.y, self.x)
    }
    pub const fn zx(self) -> Vec2 {
        vec2(self.z, self.x)
    }
    pub const fn zy(self) -> Vec2 {
        vec2(self.z, self.y)
    }
    pub const fn xyz(self) -> Self {
        self
    }
    pub const fn xzy(self) -> Self {
        vec3(self.x, self.z, self.y)
    }
    pub const fn yxz(self) -> Self {
        vec3(self.y, self.x, self.z)
    }
    pub const fn yzx(self) -> Self {
        vec3(self.y, self.z, self.x)
    }
    pub const fn zxy(self) -> Self {
        vec3(self.z, self.x, self.y)
    }
    pub const fn zyx(self) -> Self {
        vec3(self.z, self.y, self.x)
    }
}
impl Vec4 {
    pub const X: Self = Self::new(1.0, 0.0, 0.0, 0.0);
    pub const Y: Self = Self::new(0.0, 1.0, 0.0, 0.0);
    pub const Z: Self = Self::new(0.0, 0.0, 1.0, 0.0);
    pub const W: Self = Self::new(0.0, 0.0, 0.0, 1.0);
    pub const AXES: [Self; 4] = [Self::X, Self::Y, Self::Z, Self::W];
    pub const fn truncate(self) -> Vec3 {
        vec3(self.x, self.y, self.z)
    }
    pub const fn xyz(self) -> Vec3 {
        self.truncate()
    }
    pub const fn xy(self) -> Vec2 {
        vec2(self.x, self.y)
    }
    pub const fn xz(self) -> Vec2 {
        vec2(self.x, self.z)
    }
    pub const fn zw(self) -> Vec2 {
        vec2(self.z, self.w)
    }
    pub const fn xyzw(self) -> Self {
        self
    }
    pub const fn wzyx(self) -> Self {
        vec4(self.w, self.z, self.y, self.x)
    }
}

/// Compatibility imports: common swizzles are inherent methods.
pub trait Vec2Swizzles {}
pub trait Vec3Swizzles {}
pub trait Vec4Swizzles {}
impl Vec2Swizzles for Vec2 {}
impl Vec3Swizzles for Vec3 {}
impl Vec4Swizzles for Vec4 {}
