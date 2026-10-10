// SPDX-FileCopyrightText: 2021 Jorrit Rouwe
// SPDX-FileCopyrightText: 2026 TomoEngine contributors
// SPDX-License-Identifier: MIT
//! Motion kernels derived from Jolt Body.inl / MotionProperties.inl.
//! The checked settings and non-wrapping slot arena are Tomo-specific, not an
//! implementation of Jolt's concurrent BodyManager.

use crate::error::PhysicsError;
use crate::rotation::{Quat, Transform};
use crate::shape::Shape;
use crate::{Aabb, BodyId, Vec3};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MotionType {
    Static,
    Kinematic,
    Dynamic,
}

/// Two-sided bit-mask filtering: both bodies must accept the other's membership.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CollisionFilter {
    pub membership: u32,
    pub mask: u32,
}

impl Default for CollisionFilter {
    fn default() -> Self {
        Self {
            membership: 1,
            mask: u32::MAX,
        }
    }
}

impl CollisionFilter {
    pub fn allows(self, other: Self) -> bool {
        self.membership & other.mask != 0 && other.membership & self.mask != 0
    }
}

#[derive(Clone, Debug)]
pub struct BodySettings {
    pub shape: Shape,
    pub position: Vec3,
    pub rotation: Quat,
    pub motion_type: MotionType,
    pub mass: f32,
    pub linear_velocity: Vec3,
    pub angular_velocity: Vec3,
    pub friction: f32,
    pub restitution: f32,
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub gravity_factor: f32,
    pub max_linear_velocity: f32,
    pub max_angular_velocity: f32,
    pub filter: CollisionFilter,
    pub is_sensor: bool,
    pub allow_sleep: bool,
    pub apply_gyroscopic_force: bool,
}

impl BodySettings {
    pub fn dynamic(shape: Shape, position: Vec3, mass: f32) -> Self {
        Self {
            shape,
            position,
            rotation: Quat::IDENTITY,
            motion_type: MotionType::Dynamic,
            mass,
            linear_velocity: Vec3::ZERO,
            angular_velocity: Vec3::ZERO,
            friction: 0.5,
            restitution: 0.0,
            linear_damping: 0.05,
            angular_damping: 0.05,
            gravity_factor: 1.0,
            max_linear_velocity: 500.0,
            max_angular_velocity: 100.0,
            filter: CollisionFilter::default(),
            is_sensor: false,
            allow_sleep: true,
            apply_gyroscopic_force: false,
        }
    }

    pub fn fixed(shape: Shape, position: Vec3) -> Self {
        Self {
            motion_type: MotionType::Static,
            ..Self::dynamic(shape, position, 1.0)
        }
    }

    pub fn kinematic(shape: Shape, position: Vec3) -> Self {
        Self {
            motion_type: MotionType::Kinematic,
            ..Self::dynamic(shape, position, 1.0)
        }
    }
}

/// Read-only world-owned body. Mutations go through PhysicsSystem to invalidate
/// contact caches and wake connected bodies. IDs are local to one world.
#[derive(Clone, Debug)]
pub struct Body {
    pub(crate) id: BodyId,
    pub(crate) pose: Transform,
    pub(crate) shape: Shape,
    pub(crate) motion_type: MotionType,
    pub(crate) inverse_mass: f32,
    pub(crate) inverse_inertia: Vec3,
    pub(crate) linear_velocity: Vec3,
    pub(crate) angular_velocity: Vec3,
    pub(crate) force: Vec3,
    pub(crate) torque: Vec3,
    pub(crate) friction: f32,
    pub(crate) restitution: f32,
    pub(crate) linear_damping: f32,
    pub(crate) angular_damping: f32,
    pub(crate) gravity_factor: f32,
    pub(crate) max_linear_velocity: f32,
    pub(crate) max_angular_velocity: f32,
    pub(crate) filter: CollisionFilter,
    pub(crate) is_sensor: bool,
    pub(crate) allow_sleep: bool,
    pub(crate) gyroscopic: bool,
    pub(crate) sleeping: bool,
    pub(crate) sleep_time: f32,
    pub(crate) changed: bool,
}

impl Body {
    pub(crate) fn from_settings(settings: BodySettings) -> Result<Self, PhysicsError> {
        let pose = checked_transform(Transform::new(settings.position, settings.rotation))?;
        check_vector(settings.linear_velocity, 1.0e6, "linear velocity")?;
        check_vector(settings.angular_velocity, 1.0e6, "angular velocity")?;
        check_range(settings.friction, 0.0, 100.0, "friction")?;
        check_range(settings.restitution, 0.0, 1.0, "restitution")?;
        check_range(settings.linear_damping, 0.0, 100.0, "linear damping")?;
        check_range(settings.angular_damping, 0.0, 100.0, "angular damping")?;
        check_range(settings.gravity_factor, -100.0, 100.0, "gravity factor")?;
        check_range(
            settings.max_linear_velocity,
            1.0e-4,
            1.0e5,
            "max linear velocity",
        )?;
        check_range(
            settings.max_angular_velocity,
            1.0e-4,
            1.0e5,
            "max angular velocity",
        )?;
        if settings.motion_type == MotionType::Static
            && (settings.linear_velocity != Vec3::ZERO || settings.angular_velocity != Vec3::ZERO)
        {
            return Err(PhysicsError::InvalidParameter("static body velocity"));
        }
        let (inverse_mass, inverse_inertia) = if settings.motion_type == MotionType::Dynamic {
            let properties = settings.shape.mass_properties(settings.mass)?;
            let inertia = properties.inertia_diagonal;
            (
                1.0 / properties.mass,
                Vec3::new(1.0 / inertia.x, 1.0 / inertia.y, 1.0 / inertia.z),
            )
        } else {
            (0.0, Vec3::ZERO)
        };
        Ok(Self {
            id: BodyId::INVALID,
            pose,
            shape: settings.shape,
            motion_type: settings.motion_type,
            inverse_mass,
            inverse_inertia,
            linear_velocity: clamp_length(settings.linear_velocity, settings.max_linear_velocity),
            angular_velocity: clamp_length(
                settings.angular_velocity,
                settings.max_angular_velocity,
            ),
            force: Vec3::ZERO,
            torque: Vec3::ZERO,
            friction: settings.friction,
            restitution: settings.restitution,
            linear_damping: settings.linear_damping,
            angular_damping: settings.angular_damping,
            gravity_factor: settings.gravity_factor,
            max_linear_velocity: settings.max_linear_velocity,
            max_angular_velocity: settings.max_angular_velocity,
            filter: settings.filter,
            is_sensor: settings.is_sensor,
            allow_sleep: settings.allow_sleep,
            gyroscopic: settings.apply_gyroscopic_force,
            sleeping: false,
            sleep_time: 0.0,
            changed: true,
        })
    }

    pub fn id(&self) -> BodyId {
        self.id
    }
    pub fn transform(&self) -> Transform {
        self.pose
    }
    pub fn position(&self) -> Vec3 {
        self.pose.position
    }
    pub fn rotation(&self) -> Quat {
        self.pose.rotation
    }
    pub fn shape(&self) -> Shape {
        self.shape
    }
    pub fn motion_type(&self) -> MotionType {
        self.motion_type
    }
    pub fn linear_velocity(&self) -> Vec3 {
        self.linear_velocity
    }
    pub fn angular_velocity(&self) -> Vec3 {
        self.angular_velocity
    }
    pub fn inverse_mass(&self) -> f32 {
        self.inverse_mass
    }
    pub fn is_sleeping(&self) -> bool {
        self.sleeping
    }
    pub fn is_sensor(&self) -> bool {
        self.is_sensor
    }
    pub fn world_bounds(&self) -> Aabb {
        self.shape.world_bounds(self.pose)
    }
    pub fn is_dynamic(&self) -> bool {
        self.motion_type == MotionType::Dynamic
    }

    pub fn point_velocity(&self, world_point: Vec3) -> Vec3 {
        self.linear_velocity
            + self
                .angular_velocity
                .cross(world_point - self.pose.position)
    }

    pub fn multiply_inverse_inertia(&self, world_vector: Vec3) -> Vec3 {
        self.pose
            .rotation
            .rotate(self.inverse_inertia * self.pose.rotation.inverse_rotate(world_vector))
    }

    pub(crate) fn wake(&mut self) {
        if self.is_dynamic() {
            self.sleeping = false;
            self.sleep_time = 0.0;
        }
    }

    pub(crate) fn integrate_forces(&mut self, gravity: Vec3, dt: f32) {
        if !self.is_dynamic() || self.sleeping {
            return;
        }
        if self.gyroscopic {
            // MotionProperties::ApplyGyroscopicForceInternal, primitive principal axes.
            let local_velocity = self.pose.rotation.inverse_rotate(self.angular_velocity);
            let local_inertia = Vec3::new(
                1.0 / self.inverse_inertia.x,
                1.0 / self.inverse_inertia.y,
                1.0 / self.inverse_inertia.z,
            );
            let momentum = local_inertia * local_velocity;
            let next = momentum - dt * local_velocity.cross(momentum);
            let next_squared = next.length_squared();
            let corrected = if next_squared > 0.0 {
                next * (momentum.length_squared() / next_squared).sqrt()
            } else {
                Vec3::ZERO
            };
            self.angular_velocity = self.pose.rotation.rotate(self.inverse_inertia * corrected);
        }
        self.linear_velocity +=
            dt * (gravity * self.gravity_factor + self.force * self.inverse_mass);
        self.angular_velocity += dt * self.multiply_inverse_inertia(self.torque);
        self.linear_velocity *= (1.0 - self.linear_damping * dt).max(0.0);
        self.angular_velocity *= (1.0 - self.angular_damping * dt).max(0.0);
        self.clamp_velocities();
    }

    pub(crate) fn clamp_velocities(&mut self) {
        self.linear_velocity = clamp_length(self.linear_velocity, self.max_linear_velocity);
        self.angular_velocity = clamp_length(self.angular_velocity, self.max_angular_velocity);
    }

    pub(crate) fn integrate_position(&mut self, dt: f32) {
        if self.motion_type != MotionType::Static && !self.sleeping {
            self.clamp_velocities();
            self.pose.position += dt * self.linear_velocity;
            self.pose.rotation = self.pose.rotation.rotation_step(dt * self.angular_velocity);
        }
    }

    pub(crate) fn has_valid_state(&self) -> bool {
        self.pose.position.is_finite()
            && self
                .pose
                .position
                .abs()
                .to_array()
                .into_iter()
                .all(|v| v <= 1.0e6)
            && self.pose.rotation.is_finite()
            && (self.pose.rotation.length_squared() - 1.0).abs() < 1.0e-3
            && self.linear_velocity.is_finite()
            && self.angular_velocity.is_finite()
    }
}

/// Scaling first avoids overflowing length_squared for large, but finite impulses.
pub(crate) fn clamp_length(vector: Vec3, max_length: f32) -> Vec3 {
    let abs = vector.abs();
    let maximum = abs.x.max(abs.y).max(abs.z);
    if maximum == 0.0 {
        return vector;
    }
    if maximum > max_length {
        let scaled = vector / maximum;
        return scaled * (max_length / scaled.length());
    }
    let squared = vector.length_squared();
    if squared > max_length * max_length {
        vector * (max_length / squared.sqrt())
    } else {
        vector
    }
}

pub(crate) fn check_vector(
    vector: Vec3,
    bound: f32,
    name: &'static str,
) -> Result<(), PhysicsError> {
    if vector.is_finite() && vector.abs().to_array().into_iter().all(|v| v <= bound) {
        Ok(())
    } else {
        Err(PhysicsError::InvalidParameter(name))
    }
}

pub(crate) fn check_range(
    value: f32,
    min: f32,
    max: f32,
    name: &'static str,
) -> Result<(), PhysicsError> {
    if value.is_finite() && (min..=max).contains(&value) {
        Ok(())
    } else {
        Err(PhysicsError::InvalidParameter(name))
    }
}

pub(crate) fn checked_transform(transform: Transform) -> Result<Transform, PhysicsError> {
    check_vector(transform.position, 1.0e6, "position")?;
    let rotation = transform
        .rotation
        .normalized()
        .ok_or(PhysicsError::InvalidParameter("rotation"))?;
    Ok(Transform::new(transform.position, rotation))
}

#[derive(Clone)]
struct Slot {
    sequence: u8,
    body: Option<Body>,
}

/// A slot is retired instead of wrapping its 8-bit sequence. No ABA alias can be
/// created by reusing a slot 256 times. This differs from Jolt's reuse policy.
#[derive(Clone, Default)]
pub(crate) struct BodyArena {
    slots: Vec<Slot>,
    free: Vec<usize>,
    count: usize,
}

impl BodyArena {
    pub fn insert(&mut self, mut body: Body) -> Result<BodyId, PhysicsError> {
        let index = if let Some(index) = self.free.pop() {
            index
        } else {
            let index = self.slots.len();
            if index > BodyId::MAX_INDEX as usize {
                return Err(PhysicsError::BodyCapacityExceeded);
            }
            self.slots.push(Slot {
                sequence: 0,
                body: None,
            });
            index
        };
        let slot = &mut self.slots[index];
        let id =
            BodyId::new(index as u32, slot.sequence).ok_or(PhysicsError::BodyCapacityExceeded)?;
        body.id = id;
        slot.body = Some(body);
        self.count += 1;
        Ok(id)
    }

    pub fn get(&self, id: BodyId) -> Option<&Body> {
        self.slots
            .get(id.index() as usize)?
            .body
            .as_ref()
            .filter(|body| body.id == id)
    }

    pub fn get_mut(&mut self, id: BodyId) -> Option<&mut Body> {
        self.slots
            .get_mut(id.index() as usize)?
            .body
            .as_mut()
            .filter(|body| body.id == id)
    }

    pub fn remove(&mut self, id: BodyId) -> Option<Body> {
        if self.get(id).is_none() {
            return None;
        }
        let index = id.index() as usize;
        let slot = &mut self.slots[index];
        let removed = slot.body.take();
        if slot.sequence != u8::MAX {
            slot.sequence += 1;
            self.free.push(index);
        }
        self.count -= 1;
        removed
    }

    pub fn pair_mut(&mut self, a: BodyId, b: BodyId) -> Option<(&mut Body, &mut Body)> {
        let ai = a.index() as usize;
        let bi = b.index() as usize;
        if ai == bi || ai >= self.slots.len() || bi >= self.slots.len() {
            return None;
        }
        let (first, second) = if ai < bi {
            let (left, right) = self.slots.split_at_mut(bi);
            (left[ai].body.as_mut()?, right[0].body.as_mut()?)
        } else {
            let (left, right) = self.slots.split_at_mut(ai);
            (right[0].body.as_mut()?, left[bi].body.as_mut()?)
        };
        if first.id == a && second.id == b {
            Some((first, second))
        } else {
            None
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &Body> {
        self.slots.iter().filter_map(|slot| slot.body.as_ref())
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Body> {
        self.slots.iter_mut().filter_map(|slot| slot.body.as_mut())
    }

    pub fn len(&self) -> usize {
        self.count
    }
    pub fn slot_count(&self) -> usize {
        self.slots.len()
    }
}
