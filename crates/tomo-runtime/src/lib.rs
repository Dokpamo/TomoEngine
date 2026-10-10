// SPDX-License-Identifier: MIT
//! Fixed-tick scene execution, with bounded tool runs and persistent game stepping.
#![forbid(unsafe_code)]

use serde::Serialize;
use std::collections::BTreeMap;
use tomo_physics::{BodyId, BodySettings, PhysicsSettings, PhysicsSystem, Quat, Shape, Vec3};
use tomo_scene::{Check, Motion, Problem, Scene, ShapeSpec};

pub const MAX_TICKS: u32 = 3600;
pub const MAX_WORK_UNITS: u64 = 5_000_000;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BodyState {
    pub id: String,
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub linear_velocity: [f32; 3],
    pub angular_velocity: [f32; 3],
    pub sleeping: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Snapshot {
    pub tick: u32,
    pub simulation_time: f64,
    pub bodies: Vec<BodyState>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CheckResult {
    pub check: Check,
    pub observed: Vec<f32>,
    pub passed: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct Provenance {
    pub runtime_version: &'static str,
    pub jolt_reference_commit: &'static str,
    pub os: &'static str,
    pub architecture: &'static str,
    /// Runtime feature flag, not a blanket FP/determinism guarantee.
    pub runtime_fma_feature: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct RunReport {
    pub passed: bool,
    pub effective_scene: Scene,
    pub provenance: Provenance,
    pub snapshot: Snapshot,
    pub checks: Vec<CheckResult>,
}

pub struct Runtime {
    scene: Scene,
    world: PhysicsSystem,
    ids: BTreeMap<String, BodyId>,
    tick: u32,
    failed: bool,
}

fn vector(values: [f32; 3]) -> Vec3 {
    Vec3::new(values[0], values[1], values[2])
}

fn physics_error(path: &str, error: tomo_physics::PhysicsError) -> Problem {
    Problem::new("PHYSICS_INPUT", path, format!("{error:?}"))
}

impl Runtime {
    /// Build a separate complete world before exposing it to the caller.
    pub fn from_scene(mut scene: Scene) -> Result<Self, Problem> {
        scene.validate()?;
        scene.bodies.sort_by(|a, b| a.id.cmp(&b.id));
        let settings = PhysicsSettings {
            gravity: vector(scene.gravity),
            max_bodies: tomo_scene::MAX_BODIES,
            ..PhysicsSettings::default()
        };
        let mut world = PhysicsSystem::with_settings(settings)
            .map_err(|error| physics_error("/gravity", error))?;
        let mut ids = BTreeMap::new();
        for (index, spec) in scene.bodies.iter().enumerate() {
            let path = format!("/bodies/{index}");
            let shape = match spec.shape {
                ShapeSpec::Sphere { radius } => Shape::sphere(radius),
                ShapeSpec::Box { half_extents } => Shape::cuboid(vector(half_extents)),
            }
            .map_err(|error| physics_error(&format!("{path}/shape"), error))?;
            let position = vector(spec.position);
            let mut body = match spec.motion {
                Motion::Dynamic => BodySettings::dynamic(shape, position, spec.mass),
                Motion::Static => BodySettings::fixed(shape, position),
                Motion::Kinematic => BodySettings::kinematic(shape, position),
            };
            let [x, y, z, w] = spec.rotation;
            body.rotation = Quat::new(x, y, z, w);
            body.linear_velocity = vector(spec.linear_velocity);
            body.angular_velocity = vector(spec.angular_velocity);
            body.friction = spec.friction;
            body.restitution = spec.restitution;
            body.is_sensor = spec.is_sensor;
            body.allow_sleep = spec.allow_sleep;
            let id = world
                .add_body(body)
                .map_err(|error| physics_error(&path, error))?;
            ids.insert(spec.id.clone(), id);
        }
        Ok(Self {
            scene,
            world,
            ids,
            tick: 0,
            failed: false,
        })
    }

    pub fn scene(&self) -> &Scene {
        &self.scene
    }

    fn ensure_healthy(&self) -> Result<(), Problem> {
        if self.failed {
            Err(Problem::new(
                "FAILED_WORLD",
                "/",
                "discard this runtime after numerical failure",
            ))
        } else {
            Ok(())
        }
    }

    /// Set checked world-space velocities by stable scene ID. Physics owns collision response.
    /// This changes the running world, not the initial scene document.
    pub fn set_body_velocity(
        &mut self,
        name: &str,
        linear: [f32; 3],
        angular: [f32; 3],
    ) -> Result<(), Problem> {
        self.ensure_healthy()?;
        let path = format!("/bodies/{name}");
        let id = self
            .ids
            .get(name)
            .ok_or_else(|| Problem::new("REFERENCE", &path, "unknown body ID"))?;
        self.world
            .set_velocity(*id, vector(linear), vector(angular))
            .map_err(|error| physics_error(&path, error))
    }

    fn advance_one(&mut self) -> Result<(), Problem> {
        self.ensure_healthy()?;
        let next = self
            .tick
            .checked_add(1)
            .ok_or_else(|| Problem::new("LIMIT", "/ticks", "tick count overflow"))?;
        if let Err(error) = self.world.update(self.scene.fixed_dt) {
            self.failed = true;
            return Err(Problem::new(
                "SIMULATION",
                format!("/ticks/{}", self.tick),
                format!("{error:?}"),
            ));
        }
        self.tick = next;
        Ok(())
    }

    /// Advance one fixed tick for an interactive application. No CLI lifetime budget.
    /// Callers must bound catch-up work per frame. Numerical failures remain fail-stop.
    pub fn step(&mut self) -> Result<Snapshot, Problem> {
        self.advance_one()?;
        let result = self.snapshot();
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    pub fn snapshot(&self) -> Result<Snapshot, Problem> {
        if self.failed {
            return Err(Problem::new(
                "FAILED_WORLD",
                "/",
                "discard this runtime after numerical failure",
            ));
        }
        let mut bodies = Vec::with_capacity(self.ids.len());
        for (name, &id) in &self.ids {
            let body = self.world.body(id).ok_or_else(|| {
                Problem::new(
                    "INTERNAL",
                    "/bodies",
                    "scene-to-physics mapping is inconsistent",
                )
            })?;
            let rotation = body.rotation();
            let state = BodyState {
                id: name.clone(),
                position: body.position().to_array(),
                rotation: [rotation.x, rotation.y, rotation.z, rotation.w],
                linear_velocity: body.linear_velocity().to_array(),
                angular_velocity: body.angular_velocity().to_array(),
                sleeping: body.is_sleeping(),
            };
            if state
                .position
                .iter()
                .chain(&state.rotation)
                .chain(&state.linear_velocity)
                .chain(&state.angular_velocity)
                .any(|value| !value.is_finite())
            {
                return Err(Problem::new(
                    "SIMULATION",
                    format!("/bodies/{name}"),
                    "non-finite state",
                ));
            }
            bodies.push(state);
        }
        Ok(Snapshot {
            tick: self.tick,
            simulation_time: self.world.simulation_time(),
            bodies,
        })
    }

    /// Runs actual physics. Invalid budgets are rejected before any state change.
    /// Numerical failures are fail-stop, not transactional simulation rollback.
    pub fn run(&mut self, ticks: u32) -> Result<RunReport, Problem> {
        if self.failed {
            return Err(Problem::new(
                "FAILED_WORLD",
                "/",
                "discard this runtime after numerical failure",
            ));
        }
        let end = self
            .tick
            .checked_add(ticks)
            .ok_or_else(|| Problem::new("LIMIT", "/ticks", "tick count overflow"))?;
        let bodies = self.ids.len().max(1) as u64;
        let work = bodies * bodies * u64::from(end);
        if ticks == 0 || end > MAX_TICKS || work > MAX_WORK_UNITS {
            return Err(Problem::new(
                "LIMIT",
                "/ticks",
                "tick or cumulative body-squared work budget exceeded",
            ));
        }
        while self.tick < end {
            self.advance_one()?;
        }
        let snapshot = match self.snapshot() {
            Ok(snapshot) => snapshot,
            Err(error) => {
                self.failed = true;
                return Err(error);
            }
        };
        let states: BTreeMap<_, _> = snapshot
            .bodies
            .iter()
            .map(|body| (body.id.as_str(), body))
            .collect();
        let mut checks = Vec::with_capacity(self.scene.checks.len());
        for check in &self.scene.checks {
            let body = states.get(check.body_id()).ok_or_else(|| {
                Problem::new("INTERNAL", "/checks", "validated check target is absent")
            })?;
            let (observed, passed) = match check {
                Check::Position { min, max, .. } => {
                    let passed =
                        (0..3).all(|axis| (min[axis]..=max[axis]).contains(&body.position[axis]));
                    (body.position.to_vec(), passed)
                }
                Check::Speed { max, .. } => {
                    let speed = body
                        .linear_velocity
                        .iter()
                        .map(|v| v * v)
                        .sum::<f32>()
                        .sqrt();
                    (vec![speed], speed <= *max)
                }
            };
            checks.push(CheckResult {
                check: check.clone(),
                observed,
                passed,
            });
        }
        Ok(RunReport {
            passed: checks.iter().all(|check| check.passed),
            effective_scene: self.scene.clone(),
            provenance: Provenance {
                runtime_version: env!("CARGO_PKG_VERSION"),
                jolt_reference_commit: tomo_physics::JOLT_REFERENCE_COMMIT,
                os: std::env::consts::OS,
                architecture: std::env::consts::ARCH,
                runtime_fma_feature: cfg!(feature = "fma"),
            },
            snapshot,
            checks,
        })
    }
}
