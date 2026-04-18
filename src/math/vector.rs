use std::ops::{Add, AddAssign, Div, DivAssign, Index, IndexMut, Mul, MulAssign, Sub, SubAssign};

#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[repr(transparent)]
pub struct Vec2(pub(crate) glam::Vec2);

impl Vec2 {
    pub const ZERO: Self = Self(glam::Vec2::ZERO);
    pub const ONE: Self = Self(glam::Vec2::ONE);

    #[inline]
    pub fn new(x: f32, y: f32) -> Self {
        Self(glam::Vec2::new(x, y))
    }

    #[inline]
    pub fn splat(value: f32) -> Self {
        Self(glam::Vec2::splat(value))
    }

    #[inline]
    pub fn from_array(value: [f32; 2]) -> Self {
        Self(glam::Vec2::from_array(value))
    }

    #[inline]
    pub fn to_array(self) -> [f32; 2] {
        self.0.to_array()
    }

    #[inline]
    pub fn x(self) -> f32 {
        self.0.x
    }

    #[inline]
    pub fn y(self) -> f32 {
        self.0.y
    }

    #[inline]
    pub fn length_squared(self) -> f32 {
        self.0.length_squared()
    }

    #[inline]
    pub fn length(self) -> f32 {
        self.0.length()
    }

    #[inline]
    pub fn dot(self, rhs: Self) -> f32 {
        self.0.dot(rhs.0)
    }

    #[inline]
    pub fn abs(self) -> Self {
        Self(self.0.abs())
    }

    #[inline]
    pub fn min(self, rhs: Self) -> Self {
        Self(self.0.min(rhs.0))
    }

    #[inline]
    pub fn max(self, rhs: Self) -> Self {
        Self(self.0.max(rhs.0))
    }

    #[inline]
    pub fn clamp(self, min: Self, max: Self) -> Self {
        Self(self.0.clamp(min.0, max.0))
    }

    #[inline]
    pub fn lerp(self, rhs: Self, t: f32) -> Self {
        Self(self.0.lerp(rhs.0, t))
    }

    #[inline]
    pub fn try_normalized(self) -> Option<Self> {
        let len_sq = self.length_squared();
        if len_sq <= f32::EPSILON {
            None
        } else {
            Some(Self(self.0 / len_sq.sqrt()))
        }
    }

    #[inline]
    pub fn normalized(self) -> Self {
        self.try_normalized().unwrap_or(Self::ZERO)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[repr(transparent)]
pub struct Vec3(pub(crate) glam::Vec3);

impl Vec3 {
    pub const ZERO: Self = Self(glam::Vec3::ZERO);
    pub const ONE: Self = Self(glam::Vec3::ONE);

    #[inline]
    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Self(glam::Vec3::new(x, y, z))
    }

    #[inline]
    pub fn splat(value: f32) -> Self {
        Self(glam::Vec3::splat(value))
    }

    #[inline]
    pub fn from_array(value: [f32; 3]) -> Self {
        Self(glam::Vec3::from_array(value))
    }

    #[inline]
    pub fn to_array(self) -> [f32; 3] {
        self.0.to_array()
    }

    #[inline]
    pub fn x(self) -> f32 {
        self.0.x
    }

    #[inline]
    pub fn y(self) -> f32 {
        self.0.y
    }

    #[inline]
    pub fn z(self) -> f32 {
        self.0.z
    }

    #[inline]
    pub fn length_squared(self) -> f32 {
        self.0.length_squared()
    }

    #[inline]
    pub fn length(self) -> f32 {
        self.0.length()
    }

    #[inline]
    pub fn dot(self, rhs: Self) -> f32 {
        self.0.dot(rhs.0)
    }

    #[inline]
    pub fn cross(self, rhs: Self) -> Self {
        Self(self.0.cross(rhs.0))
    }

    #[inline]
    pub fn abs(self) -> Self {
        Self(self.0.abs())
    }

    #[inline]
    pub fn min(self, rhs: Self) -> Self {
        Self(self.0.min(rhs.0))
    }

    #[inline]
    pub fn max(self, rhs: Self) -> Self {
        Self(self.0.max(rhs.0))
    }

    #[inline]
    pub fn clamp(self, min: Self, max: Self) -> Self {
        Self(self.0.clamp(min.0, max.0))
    }

    #[inline]
    pub fn lerp(self, rhs: Self, t: f32) -> Self {
        Self(self.0.lerp(rhs.0, t))
    }

    #[inline]
    pub fn try_normalized(self) -> Option<Self> {
        let len_sq = self.length_squared();
        if len_sq <= f32::EPSILON {
            None
        } else {
            Some(Self(self.0 / len_sq.sqrt()))
        }
    }

    #[inline]
    pub fn normalized(self) -> Self {
        self.try_normalized().unwrap_or(Self::ZERO)
    }

    #[inline]
    pub(crate) fn from_glam(value: glam::Vec3) -> Self {
        Self(value)
    }

    #[inline]
    pub(crate) fn as_glam(self) -> glam::Vec3 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[repr(transparent)]
pub struct Vec4(pub(crate) glam::Vec4);

impl Vec4 {
    pub const ZERO: Self = Self(glam::Vec4::ZERO);
    pub const ONE: Self = Self(glam::Vec4::ONE);

    #[inline]
    pub fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self(glam::Vec4::new(x, y, z, w))
    }

    #[inline]
    pub fn splat(value: f32) -> Self {
        Self(glam::Vec4::splat(value))
    }

    #[inline]
    pub fn from_array(value: [f32; 4]) -> Self {
        Self(glam::Vec4::from_array(value))
    }

    #[inline]
    pub fn to_array(self) -> [f32; 4] {
        self.0.to_array()
    }

    #[inline]
    pub fn x(self) -> f32 {
        self.0.x
    }

    #[inline]
    pub fn y(self) -> f32 {
        self.0.y
    }

    #[inline]
    pub fn z(self) -> f32 {
        self.0.z
    }

    #[inline]
    pub fn w(self) -> f32 {
        self.0.w
    }

    #[inline]
    pub(crate) fn from_glam(value: glam::Vec4) -> Self {
        Self(value)
    }

    #[inline]
    pub(crate) fn as_glam(self) -> glam::Vec4 {
        self.0
    }
}

macro_rules! impl_index {
    ($ty:ident, $len:expr, $($field:ident => $index:expr),+ $(,)?) => {
        impl Index<usize> for $ty {
            type Output = f32;

            fn index(&self, index: usize) -> &Self::Output {
                match index {
                    $($index => &self.0.$field,)+
                    _ => panic!("index {index} out of bounds for {}", stringify!($ty)),
                }
            }
        }

        impl IndexMut<usize> for $ty {
            fn index_mut(&mut self, index: usize) -> &mut Self::Output {
                match index {
                    $($index => &mut self.0.$field,)+
                    _ => panic!("index {index} out of bounds for {}", stringify!($ty)),
                }
            }
        }
    };
}

macro_rules! impl_vec_ops {
    ($ty:ident) => {
        impl Add for $ty {
            type Output = Self;

            fn add(self, rhs: Self) -> Self::Output {
                Self(self.0 + rhs.0)
            }
        }

        impl AddAssign for $ty {
            fn add_assign(&mut self, rhs: Self) {
                self.0 += rhs.0;
            }
        }

        impl Sub for $ty {
            type Output = Self;

            fn sub(self, rhs: Self) -> Self::Output {
                Self(self.0 - rhs.0)
            }
        }

        impl SubAssign for $ty {
            fn sub_assign(&mut self, rhs: Self) {
                self.0 -= rhs.0;
            }
        }

        impl Mul<f32> for $ty {
            type Output = Self;

            fn mul(self, rhs: f32) -> Self::Output {
                Self(self.0 * rhs)
            }
        }

        impl MulAssign<f32> for $ty {
            fn mul_assign(&mut self, rhs: f32) {
                self.0 *= rhs;
            }
        }

        impl Div<f32> for $ty {
            type Output = Self;

            fn div(self, rhs: f32) -> Self::Output {
                Self(self.0 / rhs)
            }
        }

        impl DivAssign<f32> for $ty {
            fn div_assign(&mut self, rhs: f32) {
                self.0 /= rhs;
            }
        }
    };
}

impl_index!(Vec2, 2, x => 0, y => 1);
impl_index!(Vec3, 3, x => 0, y => 1, z => 2);
impl_index!(Vec4, 4, x => 0, y => 1, z => 2, w => 3);
impl_vec_ops!(Vec2);
impl_vec_ops!(Vec3);
impl_vec_ops!(Vec4);

impl Mul for Vec2 {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self::Output {
        Self(self.0 * rhs.0)
    }
}

impl Mul for Vec3 {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self::Output {
        Self(self.0 * rhs.0)
    }
}

impl Mul for Vec4 {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self::Output {
        Self(self.0 * rhs.0)
    }
}

impl From<[f32; 2]> for Vec2 {
    fn from(value: [f32; 2]) -> Self {
        Self::from_array(value)
    }
}

impl From<[f32; 3]> for Vec3 {
    fn from(value: [f32; 3]) -> Self {
        Self::from_array(value)
    }
}

impl From<[f32; 4]> for Vec4 {
    fn from(value: [f32; 4]) -> Self {
        Self::from_array(value)
    }
}

#[cfg(test)]
mod tests {
    use super::{Vec2, Vec3, Vec4};

    #[test]
    fn component_mul_is_supported_for_all_public_vectors() {
        assert_eq!(
            (Vec2::new(2.0, 3.0) * Vec2::new(4.0, 5.0)).to_array(),
            [8.0, 15.0]
        );
        assert_eq!(
            (Vec3::new(2.0, 3.0, 4.0) * Vec3::new(5.0, 6.0, 7.0)).to_array(),
            [10.0, 18.0, 28.0]
        );
        assert_eq!(
            (Vec4::new(1.0, 2.0, 3.0, 4.0) * Vec4::new(5.0, 6.0, 7.0, 8.0)).to_array(),
            [5.0, 12.0, 21.0, 32.0]
        );
    }

    #[test]
    fn vec2_and_vec3_expose_common_utility_methods() {
        let a2 = Vec2::new(-2.0, 4.0);
        let b2 = Vec2::new(6.0, -1.0);
        assert_eq!(a2.abs().to_array(), [2.0, 4.0]);
        assert_eq!(a2.min(b2).to_array(), [-2.0, -1.0]);
        assert_eq!(a2.max(b2).to_array(), [6.0, 4.0]);
        assert_eq!(
            a2.clamp(Vec2::new(-1.0, 0.0), Vec2::new(3.0, 3.0))
                .to_array(),
            [-1.0, 3.0]
        );
        assert_eq!(a2.lerp(b2, 0.5).to_array(), [2.0, 1.5]);

        let a3 = Vec3::new(-2.0, 4.0, -6.0);
        let b3 = Vec3::new(6.0, -1.0, 8.0);
        assert_eq!(a3.abs().to_array(), [2.0, 4.0, 6.0]);
        assert_eq!(a3.min(b3).to_array(), [-2.0, -1.0, -6.0]);
        assert_eq!(a3.max(b3).to_array(), [6.0, 4.0, 8.0]);
        assert_eq!(
            a3.clamp(Vec3::new(-1.0, 0.0, -4.0), Vec3::new(3.0, 3.0, 4.0))
                .to_array(),
            [-1.0, 3.0, -4.0]
        );
        assert_eq!(a3.lerp(b3, 0.5).to_array(), [2.0, 1.5, 1.0]);
    }
}
