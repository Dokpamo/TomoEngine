// SPDX-FileCopyrightText: 2021 Jorrit Rouwe
// SPDX-FileCopyrightText: 2026 TomoEngine contributors
// SPDX-License-Identifier: MIT
//! Scalar subset of Jolt/Geometry/AABox.h. See upstream/jolt.toml.

use crate::Vec3;

/// Axis-aligned bounds. Inverted bounds represent empty boxes.
/// Ordinary input coordinates must be finite; raw constructors permit emptiness.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Default for Aabb {
    fn default() -> Self {
        Self::EMPTY
    }
}

impl Aabb {
    pub const EMPTY: Self = Self::new(Vec3::splat(f32::MAX), Vec3::splat(-f32::MAX));

    pub const fn new(min: Vec3, max: Vec3) -> Self {
        Self { min, max }
    }

    pub fn from_two_points(a: Vec3, b: Vec3) -> Self {
        Self::new(a.min(b), a.max(b))
    }

    /// Upstream biggest box; its size remains representable.
    pub fn biggest() -> Self {
        Self::new(Vec3::splat(-0.5 * f32::MAX), Vec3::splat(0.5 * f32::MAX))
    }

    pub fn is_valid(self) -> bool {
        self.min.x <= self.max.x && self.min.y <= self.max.y && self.min.z <= self.max.z
    }

    pub fn encapsulate_point(&mut self, point: Vec3) {
        self.min = self.min.min(point);
        self.max = self.max.max(point);
    }

    pub fn encapsulate_box(&mut self, other: Self) {
        self.min = self.min.min(other.min);
        self.max = self.max.max(other.max);
    }

    pub fn intersection(self, other: Self) -> Self {
        Self::new(self.min.max(other.min), self.max.min(other.max))
    }

    pub fn center(self) -> Vec3 {
        (self.min + self.max) * 0.5
    }

    pub fn extent(self) -> Vec3 {
        self.size() * 0.5
    }

    pub fn size(self) -> Vec3 {
        self.max - self.min
    }

    /// Requires valid bounds and non-overflowing dimensions.
    pub fn surface_area(self) -> f32 {
        let e = self.size();
        2.0 * (e.x * e.y + e.x * e.z + e.y * e.z)
    }

    /// Requires valid bounds and non-overflowing dimensions.
    pub fn volume(self) -> f32 {
        let e = self.size();
        e.x * e.y * e.z
    }

    pub fn contains_point(self, point: Vec3) -> bool {
        self.min.x <= point.x
            && point.x <= self.max.x
            && self.min.y <= point.y
            && point.y <= self.max.y
            && self.min.z <= point.z
            && point.z <= self.max.z
    }

    /// Component comparisons match AABox::Contains; empty subset is permitted.
    pub fn contains_box(self, other: Self) -> bool {
        self.min.x <= other.min.x
            && self.min.y <= other.min.y
            && self.min.z <= other.min.z
            && self.max.x >= other.max.x
            && self.max.y >= other.max.y
            && self.max.z >= other.max.z
    }

    /// Inclusive overlap; touching faces count. Invalid boxes never overlap.
    /// The explicit validity guard is a deliberate strengthening of upstream.
    pub fn overlaps(self, other: Self) -> bool {
        self.is_valid()
            && other.is_valid()
            && self.min.x <= other.max.x
            && self.max.x >= other.min.x
            && self.min.y <= other.max.y
            && self.max.y >= other.min.y
            && self.min.z <= other.max.z
            && self.max.z >= other.min.z
    }

    pub fn translated(self, translation: Vec3) -> Self {
        Self::new(self.min + translation, self.max + translation)
    }

    /// Reorders endpoints for negative/nonuniform scales; preserves emptiness.
    pub fn scaled(self, scale: Vec3) -> Self {
        if !self.is_valid() {
            return Self::EMPTY;
        }
        Self::from_two_points(self.min * scale, self.max * scale)
    }

    pub fn expanded(self, amount: Vec3) -> Self {
        Self::new(self.min - amount, self.max + amount)
    }

    pub fn ensure_minimal_edge_length(&mut self, length: f32) {
        for axis in 0..3 {
            if self.max[axis] - self.min[axis] < length {
                match axis {
                    0 => self.max.x = self.min.x + length,
                    1 => self.max.y = self.min.y + length,
                    _ => self.max.z = self.min.z + length,
                }
            }
        }
    }

    /// Extreme point along direction; zero components choose max like Jolt.
    /// Requires a valid box.
    pub fn support(self, direction: Vec3) -> Vec3 {
        Vec3::new(
            if direction.x < 0.0 {
                self.min.x
            } else {
                self.max.x
            },
            if direction.y < 0.0 {
                self.min.y
            } else {
                self.max.y
            },
            if direction.z < 0.0 {
                self.min.z
            } else {
                self.max.z
            },
        )
    }

    /// Requires valid bounds.
    pub fn closest_point(self, point: Vec3) -> Vec3 {
        point.max(self.min).min(self.max)
    }

    pub fn squared_distance_to(self, point: Vec3) -> f32 {
        (self.closest_point(point) - point).length_squared()
    }
}
