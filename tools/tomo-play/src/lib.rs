// SPDX-License-Identifier: MIT
//! Box Run: a small physics game, also executable through fixed-tick input replays.
#![forbid(unsafe_code)]

mod replay;
pub mod view;

pub use replay::{Replay, ReplaySegment};
use serde::{Deserialize, Serialize};
use tomo_runtime::{BodyState, Runtime, Snapshot};
use tomo_scene::{Problem, Scene};

pub const LEVEL: &str = include_str!("../../../examples/scenes/box_run.scene.json");
pub const SPEED: f32 = 4.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    #[serde(default)]
    pub up: bool,
    #[serde(default)]
    pub down: bool,
    #[serde(default)]
    pub left: bool,
    #[serde(default)]
    pub right: bool,
}

impl Input {
    /// World X/Z input, normalized so diagonals have the same speed.
    pub fn direction(self) -> [f32; 2] {
        let x = i32::from(self.right) - i32::from(self.left);
        let z = i32::from(self.down) - i32::from(self.up);
        let length = ((x * x + z * z) as f32).sqrt().max(1.0);
        [x as f32 / length, z as f32 / length]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Ready,
    Playing,
    Won,
}

/// A session stores only a best time in memory; restart constructs a fresh physics world.
pub struct Game {
    runtime: Runtime,
    snapshot: Snapshot,
    status: Status,
    elapsed_ticks: u32,
    best_ticks: Option<u32>,
}

impl Game {
    pub fn new() -> Result<Self, Problem> {
        let runtime = Runtime::from_scene(Scene::from_json(LEVEL)?)?;
        let snapshot = runtime.snapshot()?;
        Ok(Self {
            runtime,
            snapshot,
            status: Status::Ready,
            elapsed_ticks: 0,
            best_ticks: None,
        })
    }

    pub fn scene(&self) -> &Scene {
        self.runtime.scene()
    }
    pub fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }
    pub fn status(&self) -> Status {
        self.status
    }
    pub fn elapsed_ticks(&self) -> u32 {
        self.elapsed_ticks
    }
    pub fn seconds(&self) -> f64 {
        f64::from(self.elapsed_ticks) * f64::from(self.scene().fixed_dt)
    }
    pub fn best_seconds(&self) -> Option<f64> {
        self.best_ticks
            .map(|ticks| f64::from(ticks) * f64::from(self.scene().fixed_dt))
    }
    pub fn player(&self) -> &BodyState {
        // This built-in validated level always contains player; the runtime never removes bodies.
        self.snapshot
            .bodies
            .iter()
            .find(|body| body.id == "player")
            .expect("built-in player")
    }

    pub fn restart(&mut self) -> Result<(), Problem> {
        let mut fresh = Self::new()?;
        fresh.best_ticks = self.best_ticks;
        *self = fresh;
        Ok(())
    }

    pub fn step(&mut self, input: Input) -> Result<(), Problem> {
        if self.status == Status::Won {
            return Ok(());
        }
        let [x, z] = input.direction();
        if self.status == Status::Ready && x == 0.0 && z == 0.0 {
            return Ok(());
        }
        self.status = Status::Playing;
        let next = self
            .elapsed_ticks
            .checked_add(1)
            .ok_or_else(|| Problem::new("LIMIT", "/ticks", "game clock overflow"))?;
        let player = self.player();
        // An arcade velocity controller preserves gravity and contact response. An angular
        // servo gently returns the cube upright; it never teleports through obstacles.
        let [qx, qy, qz, qw] = player.rotation;
        let gain = -12.0 * if qw < 0.0 { -1.0 } else { 1.0 };
        self.runtime.set_body_velocity(
            "player",
            [x * SPEED, player.linear_velocity[1], z * SPEED],
            [qx * gain, qy * gain, qz * gain],
        )?;
        self.snapshot = self.runtime.step()?;
        self.elapsed_ticks = next;
        let player = self.player();
        // The cube's bounding sphere must fit entirely inside the sensor pad horizontally.
        // This is conservative for every orientation, including a tipped cube.
        let margin = 1.2 - 0.42_f32 * 3.0_f32.sqrt();
        if (player.position[0] - 5.0).abs() <= margin
            && (player.position[2] + 4.0).abs() <= margin
            && (0.0..1.2).contains(&player.position[1])
        {
            self.status = Status::Won;
            self.best_ticks = Some(self.best_ticks.map_or(next, |best| best.min(next)));
        }
        Ok(())
    }
}

/// Bounds work after a slow frame and drops excess elapsed time instead of spiralling.
/// Focus/minimize transitions call reset, so returning to the game cannot replay held keys.
#[derive(Default)]
pub struct FrameClock {
    remainder: f64,
}

impl FrameClock {
    pub const MAX_STEPS: u32 = 8;
    pub fn reset(&mut self) {
        self.remainder = 0.0;
    }
    pub fn advance(&mut self, elapsed: f64, dt: f64) -> u32 {
        if !elapsed.is_finite() || elapsed < 0.0 || !dt.is_finite() || dt <= 0.0 {
            self.reset();
            return 0;
        }
        self.remainder += elapsed.min(dt * f64::from(Self::MAX_STEPS));
        let steps = ((self.remainder / dt).floor() as u32).min(Self::MAX_STEPS);
        self.remainder -= f64::from(steps) * dt;
        steps
    }
}
