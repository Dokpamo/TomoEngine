// SPDX-FileCopyrightText: 2021 Jorrit Rouwe
// SPDX-FileCopyrightText: 2026 TomoEngine contributors
// SPDX-License-Identifier: MIT
//! Sphere/box bounds, mass properties and ray queries derived from pinned Jolt.
//! Shape ownership and checked constructors are Rust-specific. No convex radius yet.

use crate::error::PhysicsError;
use crate::ray::{NO_HIT, RayInvDirection, ray_aabb, ray_sphere};
use crate::rotation::Transform;
use crate::{Aabb, Vec3};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ShapeKind {
    Sphere(f32),
    Box(Vec3),
}

/// Immutable primitive geometry. Dimensions are checked at construction.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shape {
    pub(crate) kind: ShapeKind,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MassProperties {
    pub mass: f32,
    pub inertia_diagonal: Vec3,
}

impl Shape {
    /// Radius in the supported [1e-4, 1e4] length range.
    pub fn sphere(radius: f32) -> Result<Self, PhysicsError> {
        if !valid_dimension(radius) {
            return Err(PhysicsError::InvalidParameter("sphere radius"));
        }
        Ok(Self {
            kind: ShapeKind::Sphere(radius),
        })
    }

    /// Sharp-edged box. Half extents must each be in [1e-4, 1e4].
    pub fn cuboid(half_extent: Vec3) -> Result<Self, PhysicsError> {
        if !half_extent.to_array().into_iter().all(valid_dimension) {
            return Err(PhysicsError::InvalidParameter("box half extent"));
        }
        Ok(Self {
            kind: ShapeKind::Box(half_extent),
        })
    }

    pub fn half_extent(self) -> Vec3 {
        match self.kind {
            ShapeKind::Sphere(radius) => Vec3::splat(radius),
            ShapeKind::Box(half) => half,
        }
    }

    pub fn bounding_radius(self) -> f32 {
        match self.kind {
            ShapeKind::Sphere(radius) => radius,
            ShapeKind::Box(half) => half.length(),
        }
    }

    pub fn volume(self) -> f32 {
        match self.kind {
            ShapeKind::Sphere(radius) => {
                (4.0 / 3.0 * std::f32::consts::PI) * radius * radius * radius
            }
            ShapeKind::Box(half) => 8.0 * half.x * half.y * half.z,
        }
    }

    /// Solid sphere/box inertia around its center of mass, with supplied total mass.
    pub fn mass_properties(self, mass: f32) -> Result<MassProperties, PhysicsError> {
        if !mass.is_finite() || !(1.0e-6..=1.0e9).contains(&mass) {
            return Err(PhysicsError::InvalidParameter("mass"));
        }
        let inertia_diagonal = match self.kind {
            ShapeKind::Sphere(radius) => Vec3::splat((2.0 / 5.0) * mass * radius * radius),
            ShapeKind::Box(half) => {
                let size = 2.0 * half;
                let squared = size * size;
                (mass / 12.0)
                    * Vec3::new(
                        squared.y + squared.z,
                        squared.x + squared.z,
                        squared.x + squared.y,
                    )
            }
        };
        if !inertia_diagonal.is_finite()
            || inertia_diagonal
                .to_array()
                .into_iter()
                .any(|v| v <= f32::MIN_POSITIVE)
        {
            return Err(PhysicsError::InvalidParameter("inertia"));
        }
        Ok(MassProperties {
            mass,
            inertia_diagonal,
        })
    }

    /// Requires a finite transform and unit rotation (validated by PhysicsSystem).
    pub fn world_bounds(self, transform: Transform) -> Aabb {
        let extent = match self.kind {
            ShapeKind::Sphere(radius) => Vec3::splat(radius),
            ShapeKind::Box(half) => {
                let axes = transform.rotation.axes();
                axes[0].abs() * half.x + axes[1].abs() * half.y + axes[2].abs() * half.z
            }
        };
        Aabb::new(transform.position - extent, transform.position + extent)
    }

    pub(crate) fn ray_fraction(self, transform: Transform, origin: Vec3, direction: Vec3) -> f32 {
        let local_origin = transform.inverse_transform_point(origin);
        let local_direction = transform.rotation.inverse_rotate(direction);
        match self.kind {
            ShapeKind::Sphere(radius) => {
                ray_sphere(local_origin, local_direction, Vec3::ZERO, radius)
            }
            ShapeKind::Box(half) => {
                let fraction = ray_aabb(
                    local_origin,
                    &RayInvDirection::new(local_direction),
                    Aabb::new(-half, half),
                );
                if fraction == NO_HIT {
                    NO_HIT
                } else {
                    fraction.max(0.0)
                }
            }
        }
    }

    pub(crate) fn surface_normal(self, local: Vec3) -> Vec3 {
        match self.kind {
            ShapeKind::Sphere(_) => local.normalized_or(Vec3::Y),
            ShapeKind::Box(half) => {
                let distances = (local.abs() - half).abs();
                let index = lowest_axis(distances);
                axis(index) * if local[index] > 0.0 { 1.0 } else { -1.0 }
            }
        }
    }
}

fn valid_dimension(value: f32) -> bool {
    value.is_finite() && (1.0e-4..=1.0e4).contains(&value)
}

pub(crate) fn axis(index: usize) -> Vec3 {
    [Vec3::X, Vec3::Y, Vec3::Z][index]
}

pub(crate) fn lowest_axis(value: Vec3) -> usize {
    if value.x <= value.y && value.x <= value.z {
        0
    } else if value.y <= value.z {
        1
    } else {
        2
    }
}
