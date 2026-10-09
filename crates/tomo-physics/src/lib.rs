// SPDX-License-Identifier: MIT
//! Early, scalar foundation of the TomoEngine Jolt Physics port.
//!
//! This crate is not yet a rigid-body simulator. See `docs/PORTING.md` in the
//! repository for implemented scope, numeric preconditions and upstream mapping.
//! No native dependencies, FFI or unsafe code are used.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]

pub mod aabb;
pub mod body_id;
pub mod math;
pub mod ray;

pub use aabb::Aabb;
pub use body_id::BodyId;
pub use math::Vec3;

/// Pinned upstream commit; not a declaration of full feature compatibility.
pub const JOLT_REFERENCE_COMMIT: &str = "e77f175595e64cb44218cc9d9d56fc365ad0e36a";
