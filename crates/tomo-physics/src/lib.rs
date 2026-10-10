// SPDX-License-Identifier: MIT
//! Pure Rust Jolt source-port project with a primitive rigid-body runtime.
//! This is not the complete Jolt PhysicsSystem; see docs/PORTING.md.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]

pub mod aabb;
pub mod body;
pub mod body_id;
mod broad_phase;
pub mod collision;
pub mod error;
pub mod joint;
pub mod math;
pub mod ray;
pub mod rotation;
pub mod shape;
mod solver;
pub mod world;

pub use aabb::Aabb;
pub use body::{Body, BodySettings, CollisionFilter, MotionType};
pub use body_id::BodyId;
pub use error::PhysicsError;
pub use joint::{DistanceJointSettings, JointId, SpringSettings};
pub use math::Vec3;
pub use rotation::{Quat, Transform};
pub use shape::{MassProperties, Shape};
pub use world::{
    BodyContact, ContactEvent, ContactEventKind, PhysicsSettings, PhysicsSystem, RayCastOptions,
    RayHit, StepStats,
};

/// Pinned upstream commit; not a declaration of full feature compatibility.
pub const JOLT_REFERENCE_COMMIT: &str = "e77f175595e64cb44218cc9d9d56fc365ad0e36a";
