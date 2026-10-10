// SPDX-License-Identifier: MIT
//! Versioned scene data. No physics, renderer, filesystem or network dependency.
#![forbid(unsafe_code)]

mod content;
pub use content::*;

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt;

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_DOCUMENT_BYTES: usize = 1_048_576;
pub const MAX_BODIES: usize = 256;
pub const MAX_CHECKS: usize = 512;
pub const MAX_PATCH_OPERATIONS: usize = 256;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Problem {
    pub code: String,
    pub path: String,
    pub message: String,
}

impl Problem {
    pub fn new(code: &str, path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.to_owned(),
            path: path.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at {}: {}", self.code, self.path, self.message)
    }
}

impl std::error::Error for Problem {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Motion {
    Static,
    Kinematic,
    Dynamic,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ShapeSpec {
    Sphere { radius: f32 },
    Box { half_extents: [f32; 3] },
}

fn identity() -> [f32; 4] {
    [0.0, 0.0, 0.0, 1.0]
}
fn one() -> f32 {
    1.0
}
fn half() -> f32 {
    0.5
}
fn yes() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BodySpec {
    pub id: String,
    pub motion: Motion,
    pub shape: ShapeSpec,
    pub position: [f32; 3],
    #[serde(default = "identity")]
    pub rotation: [f32; 4],
    #[serde(default = "one")]
    pub mass: f32,
    #[serde(default)]
    pub linear_velocity: [f32; 3],
    #[serde(default)]
    pub angular_velocity: [f32; 3],
    #[serde(default = "half")]
    pub friction: f32,
    #[serde(default)]
    pub restitution: f32,
    #[serde(default)]
    pub is_sensor: bool,
    #[serde(default = "yes")]
    pub allow_sleep: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Check {
    Position {
        body: String,
        min: [f32; 3],
        max: [f32; 3],
    },
    Speed {
        body: String,
        max: f32,
    },
}

impl Check {
    pub fn body_id(&self) -> &str {
        match self {
            Self::Position { body, .. } | Self::Speed { body, .. } => body,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scene {
    pub schema_version: u32,
    pub revision: u64,
    pub name: String,
    pub gravity: [f32; 3],
    pub fixed_dt: f32,
    pub bodies: Vec<BodySpec>,
    #[serde(default)]
    pub checks: Vec<Check>,
}

fn decode<T: serde::de::DeserializeOwned>(text: &str) -> Result<T, Problem> {
    if text.len() > MAX_DOCUMENT_BYTES {
        return Err(Problem::new("LIMIT", "/", "document exceeds 1 MiB"));
    }
    serde_json::from_str(text).map_err(|error| Problem::new("JSON", "/", error.to_string()))
}

fn range(value: f32, min: f32, max: f32, path: &str) -> Result<(), Problem> {
    if value.is_finite() && (min..=max).contains(&value) {
        Ok(())
    } else {
        Err(Problem::new(
            "RANGE",
            path,
            format!("expected finite {min}..={max}"),
        ))
    }
}

fn vector(values: &[f32], max: f32, path: &str) -> Result<(), Problem> {
    for (index, &value) in values.iter().enumerate() {
        range(value, -max, max, &format!("{path}/{index}"))?;
    }
    Ok(())
}

fn identifier(id: &str, path: &str) -> Result<(), Problem> {
    if id.is_empty()
        || id.len() > 64
        || !id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
    {
        return Err(Problem::new(
            "ID",
            path,
            "use 1..64 ASCII letters, digits, _ or -",
        ));
    }
    Ok(())
}

impl BodySpec {
    pub fn validate(&self, path: &str) -> Result<(), Problem> {
        identifier(&self.id, &format!("{path}/id"))?;
        vector(&self.position, 1.0e6, &format!("{path}/position"))?;
        vector(&self.rotation, 1.0, &format!("{path}/rotation"))?;
        let norm: f32 = self.rotation.iter().map(|v| v * v).sum();
        if (norm - 1.0).abs() > 1.0e-3 {
            return Err(Problem::new(
                "ROTATION",
                format!("{path}/rotation"),
                "expected unit xyzw quaternion",
            ));
        }
        range(self.mass, 1.0e-6, 1.0e9, &format!("{path}/mass"))?;
        if self.motion != Motion::Dynamic && self.mass != 1.0 {
            return Err(Problem::new(
                "MASS",
                format!("{path}/mass"),
                "v1 non-dynamic bodies use the default mass marker 1",
            ));
        }
        range(self.friction, 0.0, 100.0, &format!("{path}/friction"))?;
        range(self.restitution, 0.0, 1.0, &format!("{path}/restitution"))?;
        for (values, max, field) in [
            (&self.linear_velocity, 500.0, "linear_velocity"),
            (&self.angular_velocity, 100.0, "angular_velocity"),
        ] {
            let field_path = format!("{path}/{field}");
            vector(values, max, &field_path)?;
            let length = values.iter().map(|v| v * v).sum::<f32>().sqrt();
            range(length, 0.0, max, &field_path)?;
            if self.motion == Motion::Static && length != 0.0 {
                return Err(Problem::new(
                    "STATIC_VELOCITY",
                    field_path,
                    "static bodies cannot move",
                ));
            }
        }
        match self.shape {
            ShapeSpec::Sphere { radius } => {
                range(radius, 1.0e-4, 1.0e4, &format!("{path}/shape/radius"))?;
            }
            ShapeSpec::Box { half_extents } => {
                for (axis, value) in half_extents.into_iter().enumerate() {
                    range(
                        value,
                        1.0e-4,
                        1.0e4,
                        &format!("{path}/shape/half_extents/{axis}"),
                    )?;
                }
            }
        }
        Ok(())
    }
}

impl Scene {
    pub fn from_json(text: &str) -> Result<Self, Problem> {
        let scene: Self = decode(text)?;
        scene.validate()?;
        Ok(scene)
    }

    pub fn validate(&self) -> Result<(), Problem> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(Problem::new(
                "VERSION",
                "/schema_version",
                "only scene v1 is supported",
            ));
        }
        if self.name.is_empty() || self.name.len() > 256 {
            return Err(Problem::new(
                "NAME",
                "/name",
                "name must be 1..256 UTF-8 bytes",
            ));
        }
        vector(&self.gravity, 1.0e4, "/gravity")?;
        // Physics defaults to two substeps, each requiring at least 1e-6 seconds.
        range(self.fixed_dt, 2.0e-6, 0.25, "/fixed_dt")?;
        if self.bodies.len() > MAX_BODIES || self.checks.len() > MAX_CHECKS {
            return Err(Problem::new("LIMIT", "/", "too many bodies or checks"));
        }
        let mut ids = BTreeSet::new();
        for (index, body) in self.bodies.iter().enumerate() {
            body.validate(&format!("/bodies/{index}"))?;
            if !ids.insert(body.id.as_str()) {
                return Err(Problem::new(
                    "DUPLICATE_ID",
                    format!("/bodies/{index}/id"),
                    &body.id,
                ));
            }
        }
        for (index, check) in self.checks.iter().enumerate() {
            let path = format!("/checks/{index}");
            if !ids.contains(check.body_id()) {
                return Err(Problem::new("MISSING_BODY", path, check.body_id()));
            }
            match check {
                Check::Position { min, max, .. } => {
                    vector(min, 1.0e6, &format!("{path}/min"))?;
                    vector(max, 1.0e6, &format!("{path}/max"))?;
                    if min.iter().zip(max).any(|(a, b)| a > b) {
                        return Err(Problem::new("RANGE", path, "min exceeds max"));
                    }
                }
                Check::Speed { max, .. } => range(*max, 0.0, 1.0e6, &format!("{path}/max"))?,
            }
        }
        Ok(())
    }

    /// Transaction on a document clone. The caller's document is never modified.
    pub fn patched(&self, patch: &Patch) -> Result<Self, Problem> {
        self.validate()?;
        if patch.schema_version != SCHEMA_VERSION {
            return Err(Problem::new(
                "VERSION",
                "/schema_version",
                "only patch v1 is supported",
            ));
        }
        if patch.expected_revision != self.revision {
            return Err(Problem::new(
                "REVISION_CONFLICT",
                "/expected_revision",
                "reload the current document before retrying",
            ));
        }
        if patch.operations.is_empty() || patch.operations.len() > MAX_PATCH_OPERATIONS {
            return Err(Problem::new(
                "LIMIT",
                "/operations",
                "expected 1..256 operations",
            ));
        }
        let mut candidate = self.clone();
        candidate.revision = self.revision.checked_add(1).ok_or_else(|| {
            Problem::new(
                "REVISION_LIMIT",
                "/revision",
                "revision cannot be incremented",
            )
        })?;
        for (index, operation) in patch.operations.iter().enumerate() {
            let path = format!("/operations/{index}");
            match operation {
                Operation::Add { body } => {
                    if candidate.bodies.iter().any(|b| b.id == body.id) {
                        return Err(Problem::new("DUPLICATE_ID", path, &body.id));
                    }
                    candidate.bodies.push(body.clone());
                }
                Operation::Remove { id } => {
                    let index = candidate
                        .bodies
                        .iter()
                        .position(|body| &body.id == id)
                        .ok_or_else(|| Problem::new("MISSING_BODY", path, id))?;
                    candidate.bodies.remove(index);
                }
                Operation::SetTransform {
                    id,
                    position,
                    rotation,
                } => {
                    let body = candidate
                        .bodies
                        .iter_mut()
                        .find(|body| &body.id == id)
                        .ok_or_else(|| Problem::new("MISSING_BODY", path, id))?;
                    body.position = *position;
                    body.rotation = *rotation;
                }
                Operation::SetMaterial {
                    id,
                    friction,
                    restitution,
                } => {
                    let body = candidate
                        .bodies
                        .iter_mut()
                        .find(|body| &body.id == id)
                        .ok_or_else(|| Problem::new("MISSING_BODY", path, id))?;
                    body.friction = *friction;
                    body.restitution = *restitution;
                }
            }
        }
        candidate.validate()?;
        candidate.bodies.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(candidate)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Patch {
    pub schema_version: u32,
    pub expected_revision: u64,
    pub operations: Vec<Operation>,
}

impl Patch {
    pub fn from_json(text: &str) -> Result<Self, Problem> {
        decode(text)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    Add {
        body: BodySpec,
    },
    Remove {
        id: String,
    },
    SetTransform {
        id: String,
        position: [f32; 3],
        rotation: [f32; 4],
    },
    SetMaterial {
        id: String,
        friction: f32,
        restitution: f32,
    },
}
