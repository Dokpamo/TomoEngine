// SPDX-FileCopyrightText: 2021 Jorrit Rouwe
// SPDX-FileCopyrightText: 2026 TomoEngine contributors
// SPDX-License-Identifier: MIT
//! Scalar adaptation of Jolt AxisConstraintPart and SpringPart.
//! Contact orchestration, cache representation and friction projection are local
//! integration code, not Jolt's full ContactConstraintManager implementation.

use crate::body::Body;
use crate::collision::ContactPoint;
use crate::joint::{DistanceJoint, SpringSettings};
use crate::{BodyId, Vec3};

#[derive(Clone, Copy, Debug)]
pub(crate) struct AxisConstraint {
    axis: Vec3,
    r1_cross: Vec3,
    r2_cross: Vec3,
    inv_i1_cross: Vec3,
    inv_i2_cross: Vec3,
    effective_mass: f32,
    softness: f32,
    bias: f32,
    pub lambda: f32,
}

impl AxisConstraint {
    pub fn new(a: &Body, b: &Body, r1_plus_u: Vec3, r2: Vec3, axis: Vec3, bias: f32) -> Self {
        let r1_cross = r1_plus_u.cross(axis);
        let r2_cross = r2.cross(axis);
        let inv_i1_cross = a.multiply_inverse_inertia(r1_cross);
        let inv_i2_cross = b.multiply_inverse_inertia(r2_cross);
        let inverse_effective_mass = (a.inverse_mass + inv_i1_cross.dot(r1_cross))
            + (b.inverse_mass + inv_i2_cross.dot(r2_cross));
        Self {
            axis,
            r1_cross,
            r2_cross,
            inv_i1_cross,
            inv_i2_cross,
            effective_mass: if inverse_effective_mass > 0.0 { 1.0 / inverse_effective_mass } else { 0.0 },
            softness: 0.0,
            bias,
            lambda: 0.0,
        }
    }

    pub fn with_spring(mut self, dt: f32, error: f32, spring: SpringSettings) -> Self {
        if self.effective_mass > 0.0 {
            let inverse_effective_mass = 1.0 / self.effective_mass;
            let omega = 2.0 * std::f32::consts::PI * spring.frequency_hz;
            let stiffness = self.effective_mass * omega * omega;
            let damping = 2.0 * self.effective_mass * spring.damping_ratio * omega;
            self.softness = 1.0 / (dt * (damping + dt * stiffness));
            self.bias += dt * stiffness * self.softness * error;
            self.effective_mass = 1.0 / (inverse_effective_mass + self.softness);
        }
        self
    }

    pub fn candidate_lambda(&self, a: &Body, b: &Body) -> f32 {
        // This is -J*v; signs follow AxisConstraintPart::SolveVelocityConstraint.
        let negative_jv = self.axis.dot(a.linear_velocity - b.linear_velocity)
            + self.r1_cross.dot(a.angular_velocity)
            - self.r2_cross.dot(b.angular_velocity);
        self.lambda + self.effective_mass * (negative_jv - self.bias - self.softness * self.lambda)
    }

    pub fn apply(&self, a: &mut Body, b: &mut Body, delta: f32) {
        if a.is_dynamic() {
            a.linear_velocity -= self.axis * (delta * a.inverse_mass);
            a.angular_velocity -= self.inv_i1_cross * delta;
        }
        if b.is_dynamic() {
            b.linear_velocity += self.axis * (delta * b.inverse_mass);
            b.angular_velocity += self.inv_i2_cross * delta;
        }
    }

    pub fn set_and_apply(&mut self, a: &mut Body, b: &mut Body, value: f32) {
        self.apply(a, b, value - self.lambda);
        self.lambda = value;
    }

    pub fn solve(&mut self, a: &mut Body, b: &mut Body, min: f32, max: f32) {
        let value = self.candidate_lambda(a, b).clamp(min, max);
        self.set_and_apply(a, b, value);
    }

    pub fn warm_start(&self, a: &mut Body, b: &mut Body) {
        self.apply(a, b, self.lambda);
    }

    pub fn solve_position(&self, a: &mut Body, b: &mut Body, error: f32, baumgarte: f32) {
        if self.softness != 0.0 || error == 0.0 { return; }
        let lambda = -self.effective_mass * baumgarte * error;
        // Split position correction: do NOT add this impulse to real velocities.
        if a.is_dynamic() {
            a.pose.position -= self.axis * (lambda * a.inverse_mass);
            a.pose.rotation = a.pose.rotation.rotation_step(-lambda * self.inv_i1_cross);
        }
        if b.is_dynamic() {
            b.pose.position += self.axis * (lambda * b.inverse_mass);
            b.pose.rotation = b.pose.rotation.rotation_step(lambda * self.inv_i2_cross);
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct SolverConfig {
    pub dt: f32,
    pub restitution_threshold: f32,
    pub warm_start_ratio: f32,
}

#[derive(Clone, Copy)]
pub(crate) struct PositionConfig {
    pub slop: f32,
    pub max_correction: f32,
    pub baumgarte: f32,
}

#[derive(Clone, Debug)]
pub(crate) struct CachedContact {
    pub local_a: Vec3,
    pub local_b: Vec3,
    pub normal: Vec3,
    pub normal_lambda: f32,
    pub friction_impulse: Vec3,
}

pub(crate) struct ContactConstraint {
    pub ids: (BodyId, BodyId),
    local_a: Vec3,
    local_b: Vec3,
    normal: AxisConstraint,
    tangents: [AxisConstraint; 2],
    friction: f32,
}

impl ContactConstraint {
    pub fn new(a: &Body, b: &Body, point: ContactPoint, normal: Vec3, cached: Option<&CachedContact>, config: SolverConfig) -> Self {
        let midpoint = (point.point_a + point.point_b) * 0.5;
        let r1 = midpoint - a.pose.position;
        let r2 = midpoint - b.pose.position;
        let relative_velocity = b.point_velocity(midpoint) - a.point_velocity(midpoint);
        let incoming = relative_velocity.dot(normal);
        let separation = point.separation(normal);
        let mut bias = separation.max(0.0) / config.dt;
        let restitution = a.restitution.max(b.restitution);
        if restitution > 0.0
            && incoming < -config.restitution_threshold
            && -incoming * config.dt >= separation.max(0.0)
        {
            bias = restitution * incoming;
        }
        let mut normal_row = AxisConstraint::new(a, b, r1, r2, normal, bias);
        let tangent = if normal.x.abs() > normal.y.abs() {
            Vec3::new(-normal.z, 0.0, normal.x).normalized_or(Vec3::Y)
        } else {
            Vec3::new(0.0, normal.z, -normal.y).normalized_or(Vec3::X)
        };
        let mut tangents = [
            AxisConstraint::new(a, b, r1, r2, tangent, 0.0),
            AxisConstraint::new(a, b, r1, r2, normal.cross(tangent), 0.0),
        ];
        let friction = (a.friction * b.friction).sqrt();
        if let Some(old) = cached {
            normal_row.lambda = (old.normal_lambda * config.warm_start_ratio).max(0.0);
            let impulse = old.friction_impulse * config.warm_start_ratio;
            let first = impulse.dot(tangents[0].axis);
            let second = impulse.dot(tangents[1].axis);
            let (first, second) = friction_disk(first, second, friction * normal_row.lambda);
            tangents[0].lambda = first;
            tangents[1].lambda = second;
        }
        Self {
            ids: (a.id, b.id),
            local_a: a.pose.inverse_transform_point(point.point_a),
            local_b: b.pose.inverse_transform_point(point.point_b),
            normal: normal_row,
            tangents,
            friction,
        }
    }

    pub fn warm_start(&self, a: &mut Body, b: &mut Body) {
        self.normal.warm_start(a, b);
        self.tangents[0].warm_start(a, b);
        self.tangents[1].warm_start(a, b);
    }

    pub fn solve_velocity(&mut self, a: &mut Body, b: &mut Body) {
        self.normal.solve(a, b, 0.0, f32::MAX);
        let first = self.tangents[0].candidate_lambda(a, b);
        let second = self.tangents[1].candidate_lambda(a, b);
        let (first, second) = friction_disk(first, second, self.friction * self.normal.lambda);
        self.tangents[0].set_and_apply(a, b, first);
        self.tangents[1].set_and_apply(a, b, second);
    }

    pub fn solve_position(&self, a: &mut Body, b: &mut Body, config: PositionConfig) {
        let point_a = a.pose.transform_point(self.local_a);
        let point_b = b.pose.transform_point(self.local_b);
        let error = ((point_b - point_a).dot(self.normal.axis) + config.slop).clamp(-config.max_correction, 0.0);
        if error < 0.0 {
            let midpoint = (point_a + point_b) * 0.5;
            let row = AxisConstraint::new(a, b, midpoint - a.pose.position, midpoint - b.pose.position, self.normal.axis, 0.0);
            row.solve_position(a, b, error, config.baumgarte);
        }
    }

    pub fn cache(&self) -> CachedContact {
        CachedContact {
            local_a: self.local_a,
            local_b: self.local_b,
            normal: self.normal.axis,
            normal_lambda: self.normal.lambda,
            friction_impulse: self.tangents[0].axis * self.tangents[0].lambda
                + self.tangents[1].axis * self.tangents[1].lambda,
        }
    }
}

fn friction_disk(first: f32, second: f32, radius: f32) -> (f32, f32) {
    let squared = first * first + second * second;
    if squared > radius * radius && squared > 0.0 {
        let scale = radius / squared.sqrt();
        (first * scale, second * scale)
    } else {
        (first, second)
    }
}

pub(crate) fn distance_row(joint: &DistanceJoint, a: &Body, b: &Body, dt: f32) -> (AxisConstraint, Vec3, f32) {
    let point_a = a.pose.transform_point(joint.settings.local_anchor_a);
    let point_b = b.pose.transform_point(joint.settings.local_anchor_b);
    let delta = point_b - point_a;
    let length = delta.length();
    let axis = if length > 1.0e-6 { delta / length } else { joint.last_axis };
    let error = length - joint.settings.distance;
    // AxisConstraintPart's r1 + u = point_b - center_a convention.
    let row = AxisConstraint::new(a, b, point_b - a.pose.position, point_b - b.pose.position, axis, 0.0);
    let row = if let Some(spring) = joint.settings.spring { row.with_spring(dt, error, spring) } else { row };
    (row, axis, error)
}
