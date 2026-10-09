// SPDX-FileCopyrightText: 2021 Jorrit Rouwe
// SPDX-FileCopyrightText: 2026 TomoEngine contributors
// SPDX-License-Identifier: MIT
//! Scalar ports from Jolt/Math/{Vec3.inl, Math.h, FindRoot.h}.
//! See upstream/jolt.toml. Inputs must be finite and intermediates representable.

use std::ops::{Add, AddAssign, Div, DivAssign, Index, Mul, MulAssign, Neg, Sub, SubAssign};

/// Three scalar f32 components. Not layout-compatible with Jolt's SIMD Vec3.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const ZERO: Self = Self::splat(0.0);
    pub const ONE: Self = Self::splat(1.0);
    pub const X: Self = Self::new(1.0, 0.0, 0.0);
    pub const Y: Self = Self::new(0.0, 1.0, 0.0);
    pub const Z: Self = Self::new(0.0, 0.0, 1.0);

    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub const fn splat(value: f32) -> Self {
        Self::new(value, value, value)
    }

    pub const fn to_array(self) -> [f32; 3] {
        [self.x, self.y, self.z]
    }

    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }

    /// Non-FMA, scalar reduction order matching the upstream default scalar path.
    pub fn dot(self, rhs: Self) -> f32 {
        (self.x * rhs.x + self.y * rhs.y) + self.z * rhs.z
    }

    pub fn cross(self, rhs: Self) -> Self {
        Self::new(
            self.y * rhs.z - self.z * rhs.y,
            self.z * rhs.x - self.x * rhs.z,
            self.x * rhs.y - self.y * rhs.x,
        )
    }

    /// Upstream CrossPrecise, with compensated products under the fma feature.
    pub fn cross_precise(self, rhs: Self) -> Self {
        Self::new(
            difference_of_products(self.y, rhs.z, self.z, rhs.y),
            difference_of_products(self.z, rhs.x, self.x, rhs.z),
            difference_of_products(self.x, rhs.y, self.y, rhs.x),
        )
    }

    pub fn length_squared(self) -> f32 {
        self.dot(self)
    }

    pub fn length(self) -> f32 {
        self.length_squared().sqrt()
    }

    /// Upstream NormalizedOr: subnormal/zero squared lengths use the fallback.
    /// Does not sanitize non-finite inputs or overflow.
    pub fn normalized_or(self, fallback: Self) -> Self {
        let length_squared = self.length_squared();
        if length_squared <= f32::MIN_POSITIVE {
            fallback
        } else {
            self / length_squared.sqrt()
        }
    }

    pub fn abs(self) -> Self {
        Self::new(self.x.abs(), self.y.abs(), self.z.abs())
    }

    /// Finite-input component minimum; NaN SIMD parity is not provided.
    pub fn min(self, rhs: Self) -> Self {
        Self::new(self.x.min(rhs.x), self.y.min(rhs.y), self.z.min(rhs.z))
    }

    pub fn max(self, rhs: Self) -> Self {
        Self::new(self.x.max(rhs.x), self.y.max(rhs.y), self.z.max(rhs.z))
    }
}

impl Index<usize> for Vec3 {
    type Output = f32;

    fn index(&self, index: usize) -> &Self::Output {
        match index {
            0 => &self.x,
            1 => &self.y,
            2 => &self.z,
            _ => panic!("Vec3 index {index} is outside 0..3"),
        }
    }
}

impl Add for Vec3 {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }
}

impl Sub for Vec3 {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }
}

impl Mul<f32> for Vec3 {
    type Output = Self;

    fn mul(self, rhs: f32) -> Self {
        Self::new(self.x * rhs, self.y * rhs, self.z * rhs)
    }
}

impl Mul for Vec3 {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self {
        Self::new(self.x * rhs.x, self.y * rhs.y, self.z * rhs.z)
    }
}

impl Mul<Vec3> for f32 {
    type Output = Vec3;

    fn mul(self, rhs: Vec3) -> Vec3 {
        rhs * self
    }
}

impl Div<f32> for Vec3 {
    type Output = Self;

    fn div(self, rhs: f32) -> Self {
        Self::new(self.x / rhs, self.y / rhs, self.z / rhs)
    }
}

impl Neg for Vec3 {
    type Output = Self;

    fn neg(self) -> Self {
        Self::new(-self.x, -self.y, -self.z)
    }
}

impl AddAssign for Vec3 {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl SubAssign for Vec3 {
    fn sub_assign(&mut self, rhs: Self) {
        *self = *self - rhs;
    }
}

impl MulAssign<f32> for Vec3 {
    fn mul_assign(&mut self, rhs: f32) {
        *self = *self * rhs;
    }
}

impl DivAssign<f32> for Vec3 {
    fn div_assign(&mut self, rhs: f32) {
        *self = *self / rhs;
    }
}

/// Port of Jolt DifferenceOfProducts, including its optional compensated FMA path.
pub fn difference_of_products(a: f32, b: f32, c: f32, d: f32) -> f32 {
    #[cfg(feature = "fma")]
    {
        let cd = c * d;
        let error = (-c).mul_add(d, cd);
        let difference = a.mul_add(b, -cd);
        difference + error
    }
    #[cfg(not(feature = "fma"))]
    {
        a * b - c * d
    }
}

/// Root count and values as reported by upstream FindRoot, without sorting.
/// Two equal roots can be reported as Two; the all-zero equation reports One(0).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Roots {
    None,
    One(f32),
    Two(f32, f32),
}

impl Roots {
    pub fn pair(self) -> Option<(f32, f32)> {
        match self {
            Self::None => None,
            Self::One(x) => Some((x, x)),
            Self::Two(x, y) => Some((x, y)),
        }
    }
}

/// Solve a*x*x + b*x + c = 0 using Jolt's cancellation-resistant algorithm.
/// The input and intermediate arithmetic must remain finite.
pub fn find_root(a: f32, b: f32, c: f32) -> Roots {
    if a == 0.0 {
        if b == 0.0 {
            return if c == 0.0 {
                Roots::One(0.0)
            } else {
                Roots::None
            };
        }
        return Roots::One(-c / b);
    }
    let determinant = difference_of_products(b, b, 4.0 * a, c);
    if determinant < 0.0 {
        return Roots::None;
    }
    // Upstream Sign compares with zero: Sign(-0.0) is +1, unlike copysign.
    let sign = if b < 0.0 { -1.0 } else { 1.0 };
    let q = (b + sign * determinant.sqrt()) / -2.0;
    let x = q / a;
    if q == 0.0 {
        Roots::One(x)
    } else {
        Roots::Two(x, c / q)
    }
}
