// SPDX-License-Identifier: MIT
//! Distance joint settings. Solver math lives in the source-derived solver module.

use crate::body::{check_range, check_vector};
use crate::error::PhysicsError;
use crate::{BodyId, Vec3};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct JointId(pub(crate) u64);

impl JointId {
    pub fn raw(self) -> u64 {
        self.0
    }
}

/// Frequency/damping specialization of Jolt SpringPart.
#[derive(Clone, Copy, Debug)]
pub struct SpringSettings {
    pub frequency_hz: f32,
    pub damping_ratio: f32,
}

#[derive(Clone, Debug)]
pub struct DistanceJointSettings {
    pub body_a: BodyId,
    pub body_b: BodyId,
    pub local_anchor_a: Vec3,
    pub local_anchor_b: Vec3,
    pub distance: f32,
    pub spring: Option<SpringSettings>,
    pub collide_connected: bool,
}

impl DistanceJointSettings {
    pub fn new(body_a: BodyId, body_b: BodyId, distance: f32) -> Self {
        Self {
            body_a,
            body_b,
            local_anchor_a: Vec3::ZERO,
            local_anchor_b: Vec3::ZERO,
            distance,
            spring: None,
            collide_connected: false,
        }
    }

    pub(crate) fn validate(&self) -> Result<(), PhysicsError> {
        if self.body_a == self.body_b {
            return Err(PhysicsError::InvalidParameter("joint self connection"));
        }
        check_range(self.distance, 0.0, 1.0e4, "joint distance")?;
        check_vector(self.local_anchor_a, 1.0e4, "joint anchor a")?;
        check_vector(self.local_anchor_b, 1.0e4, "joint anchor b")?;
        if let Some(spring) = self.spring {
            check_range(spring.frequency_hz, 1.0e-3, 1000.0, "spring frequency")?;
            check_range(spring.damping_ratio, 0.0, 10.0, "spring damping")?;
        }
        Ok(())
    }
}

#[derive(Clone)]
pub(crate) struct DistanceJoint {
    pub settings: DistanceJointSettings,
    pub lambda: f32,
    pub last_axis: Vec3,
}
