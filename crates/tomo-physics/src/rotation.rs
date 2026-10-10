// SPDX-FileCopyrightText: 2021 Jorrit Rouwe
// SPDX-FileCopyrightText: 2026 TomoEngine contributors
// SPDX-License-Identifier: MIT
//! Scalar quaternion kernels derived from Jolt/Math/Quat.inl and Body.inl.
//! Rust's scalar trigonometric functions are used, not Jolt's SIMD SinCos.

use std::ops::Mul;

use crate::Vec3;

/// Quaternion in (x, y, z, w) order. Rotation methods require a unit quaternion.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quat {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Default for Quat {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Quat {
    pub const IDENTITY: Self = Self::new(0.0, 0.0, 0.0, 1.0);

    pub const fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self { x, y, z, w }
    }

    pub fn length_squared(self) -> f32 {
        (self.x * self.x + self.y * self.y) + (self.z * self.z + self.w * self.w)
    }

    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite() && self.w.is_finite()
    }

    /// Reject zero, non-finite, underflowed or overflowing quaternion lengths.
    pub fn normalized(self) -> Option<Self> {
        let squared = self.length_squared();
        if !self.is_finite() || !squared.is_finite() || squared <= f32::MIN_POSITIVE {
            return None;
        }
        let length = squared.sqrt();
        Some(Self::new(
            self.x / length,
            self.y / length,
            self.z / length,
            self.w / length,
        ))
    }

    /// Checked boundary around Jolt's sRotation; the supplied axis is normalized.
    pub fn from_axis_angle(axis: Vec3, angle: f32) -> Option<Self> {
        let squared = axis.length_squared();
        if !axis.is_finite()
            || !angle.is_finite()
            || !squared.is_finite()
            || squared <= f32::MIN_POSITIVE
        {
            return None;
        }
        Some(Self::rotation(axis / squared.sqrt(), angle))
    }

    fn rotation(unit_axis: Vec3, angle: f32) -> Self {
        let (sine, cosine) = (0.5 * angle).sin_cos();
        let xyz = unit_axis * sine;
        Self::new(xyz.x, xyz.y, xyz.z, cosine)
    }

    pub const fn conjugated(self) -> Self {
        Self::new(-self.x, -self.y, -self.z, self.w)
    }

    pub fn rotate(self, vector: Vec3) -> Vec3 {
        let imaginary = Vec3::new(self.x, self.y, self.z);
        let cross = imaginary.cross(vector);
        vector + 2.0 * (self.w * cross + imaginary.cross(cross))
    }

    pub fn inverse_rotate(self, vector: Vec3) -> Vec3 {
        self.conjugated().rotate(vector)
    }

    pub fn axes(self) -> [Vec3; 3] {
        [
            self.rotate(Vec3::X),
            self.rotate(Vec3::Y),
            self.rotate(Vec3::Z),
        ]
    }

    /// Jolt Body::AddRotationStep: apply a world-space angular displacement.
    /// Requires finite, non-overflowing input and a unit initial quaternion.
    pub fn rotation_step(self, angular_displacement: Vec3) -> Self {
        let angle = angular_displacement.length();
        if angle <= 1.0e-6 {
            self
        } else {
            let q = Self::rotation(angular_displacement / angle, angle) * self;
            let length = q.length_squared().sqrt();
            Self::new(q.x / length, q.y / length, q.z / length, q.w / length)
        }
    }
}

impl Mul for Quat {
    type Output = Self;

    /// Parenthesization follows the upstream scalar multiplication kernel.
    fn mul(self, rhs: Self) -> Self {
        let Self {
            x: a,
            y: b,
            z: c,
            w: d,
        } = self;
        let Self { x, y, z, w } = rhs;
        Self::new(
            (a * w + b * z) + (d * x - c * y),
            (b * w + c * x) + (d * y - a * z),
            (c * w + a * y) + (d * z - b * x),
            -(a * x + b * y) + (d * w - c * z),
        )
    }
}

/// Center-of-mass transform. No scale or shear is represented.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Transform {
    pub position: Vec3,
    pub rotation: Quat,
}

impl Transform {
    pub const IDENTITY: Self = Self::new(Vec3::ZERO, Quat::IDENTITY);

    pub const fn new(position: Vec3, rotation: Quat) -> Self {
        Self { position, rotation }
    }

    pub fn transform_point(self, local: Vec3) -> Vec3 {
        self.position + self.rotation.rotate(local)
    }

    pub fn inverse_transform_point(self, world: Vec3) -> Vec3 {
        self.rotation.inverse_rotate(world - self.position)
    }
}
