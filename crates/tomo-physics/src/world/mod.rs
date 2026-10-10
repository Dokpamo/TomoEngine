// SPDX-License-Identifier: MIT
//! Single-threaded runtime connecting source-derived motion and constraint kernels.
//! Supported primitive subset, not Jolt PhysicsSystem feature parity.

use crate::body::{
    Body, BodyArena, BodySettings, CollisionFilter, MotionType, check_range, check_vector,
    checked_transform, clamp_length,
};
use crate::broad_phase;
use crate::collision::{ContactManifold, collide};
use crate::error::PhysicsError;
use crate::joint::{DistanceJoint, DistanceJointSettings, JointId};
use crate::ray::NO_HIT;
use crate::rotation::Transform;
use crate::solver::{CachedContact, ContactConstraint, PositionConfig, SolverConfig, distance_row};
use crate::{BodyId, Vec3};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

type Pair = (BodyId, BodyId);

#[derive(Clone, Debug)]
pub struct PhysicsSettings {
    pub gravity: Vec3,
    pub substeps: usize,
    pub velocity_iterations: usize,
    pub position_iterations: usize,
    pub penetration_slop: f32,
    pub max_position_correction: f32,
    pub baumgarte: f32,
    pub speculative_distance: f32,
    pub restitution_velocity_threshold: f32,
    pub sleep_velocity_threshold: f32,
    pub time_before_sleep: f32,
    pub enable_sleeping: bool,
    pub warm_start: bool,
    pub max_bodies: usize,
}

impl Default for PhysicsSettings {
    fn default() -> Self {
        Self {
            gravity: Vec3::new(0.0, -9.81, 0.0),
            substeps: 2,
            velocity_iterations: 20,
            position_iterations: 8,
            penetration_slop: 0.001,
            max_position_correction: 0.2,
            baumgarte: 0.2,
            speculative_distance: 0.02,
            restitution_velocity_threshold: 1.0,
            sleep_velocity_threshold: 0.05,
            time_before_sleep: 0.5,
            enable_sleeping: true,
            warm_start: true,
            max_bodies: 16384,
        }
    }
}

impl PhysicsSettings {
    fn validate(&self) -> Result<(), PhysicsError> {
        check_vector(self.gravity, 1.0e4, "gravity")?;
        if !(1..=64).contains(&self.substeps) {
            return Err(PhysicsError::InvalidParameter("substeps"));
        }
        if !(1..=128).contains(&self.velocity_iterations) {
            return Err(PhysicsError::InvalidParameter("velocity iterations"));
        }
        if !(1..=64).contains(&self.position_iterations) {
            return Err(PhysicsError::InvalidParameter("position iterations"));
        }
        if self.max_bodies == 0 || self.max_bodies > BodyId::MAX_INDEX as usize + 1 {
            return Err(PhysicsError::InvalidParameter("max bodies"));
        }
        check_range(self.penetration_slop, 0.0, 1.0, "penetration slop")?;
        check_range(
            self.max_position_correction,
            1.0e-6,
            10.0,
            "position correction",
        )?;
        check_range(self.baumgarte, 0.0, 1.0, "baumgarte")?;
        check_range(
            self.speculative_distance,
            self.penetration_slop,
            1.0,
            "speculative distance",
        )?;
        check_range(
            self.restitution_velocity_threshold,
            0.0,
            1.0e4,
            "restitution threshold",
        )?;
        check_range(self.sleep_velocity_threshold, 0.0, 10.0, "sleep velocity")?;
        check_range(self.time_before_sleep, 0.0, 60.0, "sleep time")?;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct StepStats {
    pub substeps: usize,
    /// Sum of candidate pairs over all substeps.
    pub broad_phase_pairs: usize,
    /// Points at the last substep's detection phase, including sensors.
    pub contact_points: usize,
    pub active_dynamic_bodies: usize,
    pub sleeping_bodies: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContactEventKind {
    Started,
    Persisted,
    Stopped,
}

#[derive(Clone, Copy, Debug)]
pub struct ContactEvent {
    pub body_a: BodyId,
    pub body_b: BodyId,
    pub kind: ContactEventKind,
    pub is_sensor: bool,
    pub substep_index: u64,
}

#[derive(Clone, Debug)]
pub struct BodyContact {
    pub body_a: BodyId,
    pub body_b: BodyId,
    pub is_sensor: bool,
    pub manifold: ContactManifold,
}

#[derive(Clone, Copy, Debug)]
pub struct RayCastOptions {
    pub max_fraction: f32,
    pub filter: CollisionFilter,
    pub exclude: Option<BodyId>,
    pub include_sensors: bool,
}

impl Default for RayCastOptions {
    fn default() -> Self {
        Self {
            max_fraction: 1.0,
            filter: CollisionFilter {
                membership: u32::MAX,
                mask: u32::MAX,
            },
            exclude: None,
            include_sensors: true,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RayHit {
    pub body_id: BodyId,
    pub fraction: f32,
    pub position: Vec3,
    pub normal: Vec3,
}

pub struct PhysicsSystem {
    settings: PhysicsSettings,
    bodies: BodyArena,
    joints: BTreeMap<JointId, DistanceJoint>,
    next_joint: u64,
    cache: BTreeMap<Pair, Vec<CachedContact>>,
    previous_contacts: BTreeMap<Pair, bool>,
    events: Vec<ContactEvent>,
    pending_events: Vec<ContactEvent>,
    contacts: Vec<BodyContact>,
    previous_dt: f32,
    substep_index: u64,
    simulation_time: f64,
    numerical_failure: bool,
}

impl Default for PhysicsSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl PhysicsSystem {
    pub fn new() -> Self {
        Self::with_settings(PhysicsSettings::default()).expect("default physics settings are valid")
    }

    pub fn with_settings(settings: PhysicsSettings) -> Result<Self, PhysicsError> {
        settings.validate()?;
        Ok(Self {
            settings,
            bodies: BodyArena::default(),
            joints: BTreeMap::new(),
            next_joint: 0,
            cache: BTreeMap::new(),
            previous_contacts: BTreeMap::new(),
            events: Vec::new(),
            pending_events: Vec::new(),
            contacts: Vec::new(),
            previous_dt: 0.0,
            substep_index: 0,
            simulation_time: 0.0,
            numerical_failure: false,
        })
    }

    pub fn settings(&self) -> &PhysicsSettings {
        &self.settings
    }
    pub fn body_count(&self) -> usize {
        self.bodies.len()
    }
    pub fn joint_count(&self) -> usize {
        self.joints.len()
    }
    pub fn simulation_time(&self) -> f64 {
        self.simulation_time
    }
    pub fn body(&self, id: BodyId) -> Option<&Body> {
        self.bodies.get(id)
    }
    pub fn bodies(&self) -> impl Iterator<Item = &Body> {
        self.bodies.iter()
    }
    /// Geometry from the latest detection phase, before integration.
    pub fn contacts(&self) -> &[BodyContact] {
        &self.contacts
    }
    /// Per-substep events; a frame can have multiple Persisted events.
    pub fn contact_events(&self) -> &[ContactEvent] {
        &self.events
    }

    pub fn add_body(&mut self, settings: BodySettings) -> Result<BodyId, PhysicsError> {
        let body = Body::from_settings(settings)?;
        if self.body_count() >= self.settings.max_bodies {
            return Err(PhysicsError::BodyCapacityExceeded);
        }
        self.bodies.insert(body)
    }

    pub fn remove_body(&mut self, id: BodyId) -> Result<Body, PhysicsError> {
        if self.body(id).is_none() {
            return Err(PhysicsError::InvalidBody);
        }
        self.wake_connected(id);
        let removed = self.bodies.remove(id).ok_or(PhysicsError::InvalidBody)?;
        self.joints
            .retain(|_, joint| joint.settings.body_a != id && joint.settings.body_b != id);
        self.invalidate_cache(id);
        self.contacts.retain(|c| c.body_a != id && c.body_b != id);
        let ended: Vec<_> = self
            .previous_contacts
            .iter()
            .filter(|(pair, _)| pair.0 == id || pair.1 == id)
            .map(|(&pair, &sensor)| (pair, sensor))
            .collect();
        for (pair, sensor) in ended {
            self.previous_contacts.remove(&pair);
            self.pending_events
                .push(self.event(pair, sensor, ContactEventKind::Stopped));
        }
        Ok(removed)
    }

    pub fn set_transform(&mut self, id: BodyId, transform: Transform) -> Result<(), PhysicsError> {
        let transform = checked_transform(transform)?;
        if self.body(id).is_none() {
            return Err(PhysicsError::InvalidBody);
        }
        self.wake_connected(id);
        let body = self.bodies.get_mut(id).ok_or(PhysicsError::InvalidBody)?;
        body.pose = transform;
        body.changed = true;
        self.invalidate_cache(id);
        Ok(())
    }

    pub fn set_velocity(
        &mut self,
        id: BodyId,
        linear: Vec3,
        angular: Vec3,
    ) -> Result<(), PhysicsError> {
        check_vector(linear, 1.0e6, "linear velocity")?;
        check_vector(angular, 1.0e6, "angular velocity")?;
        let body = self.body(id).ok_or(PhysicsError::InvalidBody)?;
        if body.motion_type == MotionType::Static {
            return Err(PhysicsError::InvalidParameter("static body velocity"));
        }
        self.wake_connected(id);
        let body = self.bodies.get_mut(id).ok_or(PhysicsError::InvalidBody)?;
        body.linear_velocity = clamp_length(linear, body.max_linear_velocity);
        body.angular_velocity = clamp_length(angular, body.max_angular_velocity);
        body.changed = true;
        Ok(())
    }

    pub fn set_gravity(&mut self, gravity: Vec3) -> Result<(), PhysicsError> {
        check_vector(gravity, 1.0e4, "gravity")?;
        if gravity != self.settings.gravity {
            self.settings.gravity = gravity;
            for body in self.bodies.iter_mut() {
                body.wake();
            }
        }
        Ok(())
    }

    pub fn activate_body(&mut self, id: BodyId) -> Result<(), PhysicsError> {
        if self.body(id).is_none() {
            return Err(PhysicsError::InvalidBody);
        }
        self.wake_connected(id);
        Ok(())
    }

    pub fn apply_impulse(&mut self, id: BodyId, impulse: Vec3) -> Result<(), PhysicsError> {
        let position = self.body(id).ok_or(PhysicsError::InvalidBody)?.position();
        self.apply_impulse_at(id, impulse, position)
    }

    pub fn apply_impulse_at(
        &mut self,
        id: BodyId,
        impulse: Vec3,
        point: Vec3,
    ) -> Result<(), PhysicsError> {
        check_vector(impulse, 1.0e12, "impulse")?;
        check_vector(point, 1.0e6, "impulse point")?;
        let body = self.body(id).ok_or(PhysicsError::InvalidBody)?;
        if !body.is_dynamic() {
            return Err(PhysicsError::InvalidParameter(
                "impulse requires a dynamic body",
            ));
        }
        self.wake_connected(id);
        let body = self.bodies.get_mut(id).ok_or(PhysicsError::InvalidBody)?;
        body.linear_velocity += impulse * body.inverse_mass;
        body.angular_velocity +=
            body.multiply_inverse_inertia((point - body.pose.position).cross(impulse));
        body.clamp_velocities();
        Ok(())
    }

    pub fn add_force(&mut self, id: BodyId, force: Vec3) -> Result<(), PhysicsError> {
        let position = self.body(id).ok_or(PhysicsError::InvalidBody)?.position();
        self.add_force_at(id, force, position)
    }

    /// Accumulated force is applied across every substep, then cleared once per update.
    pub fn add_force_at(
        &mut self,
        id: BodyId,
        force: Vec3,
        point: Vec3,
    ) -> Result<(), PhysicsError> {
        check_vector(force, 1.0e12, "force")?;
        check_vector(point, 1.0e6, "force point")?;
        let body = self.body(id).ok_or(PhysicsError::InvalidBody)?;
        if !body.is_dynamic() {
            return Err(PhysicsError::InvalidParameter(
                "force requires a dynamic body",
            ));
        }
        let total_force = body.force + force;
        let total_torque = body.torque + (point - body.pose.position).cross(force);
        check_vector(total_force, 1.0e12, "accumulated force")?;
        check_vector(total_torque, 1.0e12, "accumulated torque")?;
        self.wake_connected(id);
        let body = self.bodies.get_mut(id).ok_or(PhysicsError::InvalidBody)?;
        body.force = total_force;
        body.torque = total_torque;
        Ok(())
    }

    pub fn add_torque(&mut self, id: BodyId, torque: Vec3) -> Result<(), PhysicsError> {
        check_vector(torque, 1.0e12, "torque")?;
        let body = self.body(id).ok_or(PhysicsError::InvalidBody)?;
        if !body.is_dynamic() {
            return Err(PhysicsError::InvalidParameter(
                "torque requires a dynamic body",
            ));
        }
        let total = body.torque + torque;
        check_vector(total, 1.0e12, "accumulated torque")?;
        self.wake_connected(id);
        self.bodies
            .get_mut(id)
            .ok_or(PhysicsError::InvalidBody)?
            .torque = total;
        Ok(())
    }

    pub fn add_distance_joint(
        &mut self,
        settings: DistanceJointSettings,
    ) -> Result<JointId, PhysicsError> {
        settings.validate()?;
        let a = self
            .body(settings.body_a)
            .ok_or(PhysicsError::InvalidBody)?;
        let b = self
            .body(settings.body_b)
            .ok_or(PhysicsError::InvalidBody)?;
        if !a.is_dynamic() && !b.is_dynamic() {
            return Err(PhysicsError::InvalidParameter(
                "joint requires a dynamic body",
            ));
        }
        let next = self
            .next_joint
            .checked_add(1)
            .ok_or(PhysicsError::JointCapacityExceeded)?;
        self.wake_connected(settings.body_a);
        self.wake_connected(settings.body_b);
        let id = JointId(self.next_joint);
        self.next_joint = next;
        self.joints.insert(
            id,
            DistanceJoint {
                settings,
                lambda: 0.0,
                last_axis: Vec3::X,
            },
        );
        Ok(id)
    }

    pub fn joint_settings(&self, id: JointId) -> Option<&DistanceJointSettings> {
        self.joints.get(&id).map(|joint| &joint.settings)
    }

    pub fn remove_joint(&mut self, id: JointId) -> Result<DistanceJointSettings, PhysicsError> {
        let joint = self.joints.get(&id).ok_or(PhysicsError::InvalidJoint)?;
        let (a, b) = (joint.settings.body_a, joint.settings.body_b);
        self.wake_connected(a);
        self.wake_connected(b);
        Ok(self
            .joints
            .remove(&id)
            .ok_or(PhysicsError::InvalidJoint)?
            .settings)
    }

    pub fn set_joint_distance(&mut self, id: JointId, distance: f32) -> Result<(), PhysicsError> {
        check_range(distance, 0.0, 1.0e4, "joint distance")?;
        let joint = self.joints.get(&id).ok_or(PhysicsError::InvalidJoint)?;
        let (a, b) = (joint.settings.body_a, joint.settings.body_b);
        self.wake_connected(a);
        self.wake_connected(b);
        let joint = self.joints.get_mut(&id).ok_or(PhysicsError::InvalidJoint)?;
        joint.settings.distance = distance;
        joint.lambda = 0.0;
        Ok(())
    }

    pub fn ray_cast(&self, origin: Vec3, direction: Vec3) -> Result<Option<RayHit>, PhysicsError> {
        self.ray_cast_filtered(origin, direction, RayCastOptions::default())
    }

    /// Segment parameterization is origin + direction * fraction; no unit length required.
    pub fn ray_cast_filtered(
        &self,
        origin: Vec3,
        direction: Vec3,
        options: RayCastOptions,
    ) -> Result<Option<RayHit>, PhysicsError> {
        check_vector(origin, 1.0e6, "ray origin")?;
        check_vector(direction, 1.0e6, "ray direction")?;
        check_range(options.max_fraction, 0.0, 1.0e6, "ray max fraction")?;
        let mut closest: Option<RayHit> = None;
        for body in self.bodies.iter() {
            if options.exclude == Some(body.id)
                || (!options.include_sensors && body.is_sensor)
                || !options.filter.allows(body.filter)
            {
                continue;
            }
            let fraction = body.shape.ray_fraction(body.pose, origin, direction);
            if fraction == NO_HIT || fraction < 0.0 || fraction > options.max_fraction {
                continue;
            }
            if let Some(hit) = closest {
                if fraction > hit.fraction || (fraction == hit.fraction && body.id >= hit.body_id) {
                    continue;
                }
            }
            let position = origin + direction * fraction;
            let local = body.pose.inverse_transform_point(position);
            closest = Some(RayHit {
                body_id: body.id,
                fraction,
                position,
                normal: body.pose.rotation.rotate(body.shape.surface_normal(local)),
            });
        }
        Ok(closest)
    }

    /// Advance dt seconds in configured substeps. Prefer fixed dt (e.g. 1/60).
    /// Invalid dt is rejected before mutation. NumericalFailure is fail-stop,
    /// not transactional rollback: discard a numerically failed world.
    pub fn update(&mut self, dt: f32) -> Result<StepStats, PhysicsError> {
        check_range(dt, 1.0e-6, 0.25, "time step")?;
        let h = dt / self.settings.substeps as f32;
        if h < 1.0e-6 {
            return Err(PhysicsError::InvalidParameter("substep duration"));
        }
        if self.numerical_failure {
            return Err(PhysicsError::NumericalFailure);
        }
        self.events = std::mem::take(&mut self.pending_events);
        let mut stats = StepStats {
            substeps: self.settings.substeps,
            ..StepStats::default()
        };
        for _ in 0..self.settings.substeps {
            let current = self.substep(h);
            stats.broad_phase_pairs += current.broad_phase_pairs;
            stats.contact_points = current.contact_points;
            if self.bodies.iter().any(|body| !body.has_valid_state()) {
                self.numerical_failure = true;
                return Err(PhysicsError::NumericalFailure);
            }
        }
        for body in self.bodies.iter_mut() {
            body.force = Vec3::ZERO;
            body.torque = Vec3::ZERO;
            if body.is_dynamic() {
                if body.sleeping {
                    stats.sleeping_bodies += 1;
                } else {
                    stats.active_dynamic_bodies += 1;
                }
            }
        }
        self.simulation_time += dt as f64;
        Ok(stats)
    }
}

mod simulation;
