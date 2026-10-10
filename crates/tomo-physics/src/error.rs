// SPDX-License-Identifier: MIT
//! Checked runtime boundary errors. Low-level foundation kernels retain their contracts.

use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PhysicsError {
    InvalidParameter(&'static str),
    InvalidBody,
    InvalidJoint,
    BodyCapacityExceeded,
    JointCapacityExceeded,
    NumericalFailure,
}

impl fmt::Display for PhysicsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidParameter(name) => write!(f, "invalid physics parameter: {name}"),
            Self::InvalidBody => f.write_str("body ID is stale or does not exist in this world"),
            Self::InvalidJoint => f.write_str("joint ID does not exist in this world"),
            Self::BodyCapacityExceeded => f.write_str("BodyId index capacity exhausted"),
            Self::JointCapacityExceeded => f.write_str("joint ID capacity exhausted"),
            Self::NumericalFailure => f.write_str("simulation left its supported numeric range"),
        }
    }
}

impl std::error::Error for PhysicsError {}
