// SPDX-FileCopyrightText: 2021 Jorrit Rouwe
// SPDX-FileCopyrightText: 2026 TomoEngine contributors
// SPDX-License-Identifier: MIT
//! Scalar RaySphere, RayTriangle and RayAABox ports; see upstream/jolt.toml.
//! Ray directions are unnormalized and rays are unbounded, not [0, 1] segments.
//! Finite, non-overflowing inputs are required; sphere radius must be nonnegative.

use crate::math::find_root;
use crate::{Aabb, Vec3};

/// Upstream FLT_MAX no-hit sentinel.
pub const NO_HIT: f32 = f32::MAX;

/// Closest nonnegative sphere intersection fraction; an inside start returns 0.
pub fn ray_sphere(origin: Vec3, direction: Vec3, center: Vec3, radius: f32) -> f32 {
    let center_origin = origin - center;
    let a = direction.length_squared();
    let b = 2.0 * direction.dot(center_origin);
    let c = center_origin.length_squared() - radius * radius;
    let Some((mut first, mut second)) = find_root(a, b, c).pair() else {
        return if c <= 0.0 { 0.0 } else { NO_HIT };
    };
    if first > second {
        std::mem::swap(&mut first, &mut second);
    }
    if first >= 0.0 {
        first
    } else if second >= 0.0 {
        0.0
    } else {
        NO_HIT
    }
}

/// Double-sided triangle intersection with Jolt's 1e-12 determinant threshold.
/// Degenerate, parallel, out-of-triangle and behind-origin hits return NO_HIT.
pub fn ray_triangle(origin: Vec3, direction: Vec3, v0: Vec3, v1: Vec3, v2: Vec3) -> f32 {
    let e1 = v1 - v0;
    let e2 = v2 - v0;
    let p = direction.cross_precise(e2);
    let determinant = e1.dot(p);
    if determinant.abs() < 1.0e-12 {
        return NO_HIT;
    }
    let s = origin - v0;
    let u = s.dot(p) / determinant;
    let q = s.cross_precise(e1);
    let v = direction.dot(q) / determinant;
    let t = e2.dot(q) / determinant;
    if u < 0.0 || v < 0.0 || u + v > 1.0 || t < 0.0 {
        NO_HIT
    } else {
        t
    }
}

/// Cached inverse direction and near-parallel flags, as in Jolt RayInvDirection.
#[derive(Clone, Copy, Debug)]
pub struct RayInvDirection {
    inverse: [f32; 3],
    parallel: [bool; 3],
}

impl RayInvDirection {
    pub fn new(direction: Vec3) -> Self {
        let mut inverse = [1.0; 3];
        let mut parallel = [false; 3];
        for axis in 0..3 {
            parallel[axis] = direction[axis].abs() <= 1.0e-20;
            if !parallel[axis] {
                inverse[axis] = 1.0 / direction[axis];
            }
        }
        Self { inverse, parallel }
    }
}

/// Ray/bounds entry and exit fractions. Entry may be negative for inside starts.
/// A stationary point inside bounds returns (-f32::MAX, f32::MAX).
/// Invalid bounds are deliberately rejected even on the scalar path.
pub fn ray_aabb_interval(
    origin: Vec3,
    inverse: &RayInvDirection,
    bounds: Aabb,
) -> Option<(f32, f32)> {
    if !bounds.is_valid() {
        return None;
    }
    let mut entry = -f32::MAX;
    let mut exit = f32::MAX;
    for axis in 0..3 {
        if inverse.parallel[axis] {
            if origin[axis] < bounds.min[axis] || origin[axis] > bounds.max[axis] {
                return None;
            }
        } else {
            let first = (bounds.min[axis] - origin[axis]) * inverse.inverse[axis];
            let second = (bounds.max[axis] - origin[axis]) * inverse.inverse[axis];
            entry = entry.max(first.min(second));
            exit = exit.min(first.max(second));
        }
    }
    if entry > exit || exit < 0.0 {
        None
    } else {
        Some((entry, exit))
    }
}

/// Jolt's raw entry fraction, including negative inside entries, or NO_HIT.
pub fn ray_aabb(origin: Vec3, inverse: &RayInvDirection, bounds: Aabb) -> f32 {
    ray_aabb_interval(origin, inverse, bounds).map_or(NO_HIT, |(entry, _)| entry)
}
