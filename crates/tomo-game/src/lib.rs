// SPDX-License-Identifier: MIT
//! Shared rule-driven physics games, executable through fixed-tick input replays.
#![forbid(unsafe_code)]

mod checkpoint;
mod execution;
pub mod sdk;
pub use checkpoint::Checkpoint;
mod logic;
mod replay;
mod rules;
pub use logic::{
    Action, Condition, ContactKind, Key, Logic, MAX_ACTIONS, MAX_CONDITIONS, MAX_LOGIC_WORK,
    MAX_RULES, MAX_TIMER_TICKS, Rule, RuleEvent, StartMode, Trigger,
};
pub use rules::GameRules;

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
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub action: bool,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Ready,
    Playing,
    Won,
    Lost,
}

/// A session stores only a best time in memory; restart constructs a fresh physics world.
pub struct Game {
    content: tomo_scene::Content,
    rules: GameRules,
    runtime: Runtime,
    snapshot: Snapshot,
    status: Status,
    elapsed_ticks: u32,
    best_ticks: Option<u32>,
    score: i32,
    previous_input: Input,
    rule_fires: Vec<u32>,
    recent_events: Vec<RuleEvent>,
    failed: bool,
    history: Vec<checkpoint::RecordedInput>,
    history_complete: bool,
    manual_start: bool,
    sounds: Vec<tomo_scene::SoundCue>,
}

impl Game {
    pub fn new() -> Result<Self, Problem> {
        Self::from_scene(Scene::from_json(LEVEL)?, GameRules::default())
    }

    pub fn from_scene(scene: Scene, rules: GameRules) -> Result<Self, Problem> {
        Self::with_content(scene, rules, Default::default())
    }
    pub fn with_content(
        scene: Scene,
        rules: GameRules,
        content: tomo_scene::Content,
    ) -> Result<Self, Problem> {
        scene.validate()?;
        content.validate(&scene)?;
        rules.validate(&scene)?;
        let runtime = Runtime::from_scene(scene)?;
        let snapshot = runtime.snapshot()?;
        let rule_fires = vec![0; rules.logic.rules.len()];
        Ok(Self {
            content,
            rules,
            runtime,
            snapshot,
            status: Status::Ready,
            elapsed_ticks: 0,
            best_ticks: None,
            score: 0,
            previous_input: Input::default(),
            rule_fires,
            recent_events: vec![],
            failed: false,
            history: vec![],
            history_complete: true,
            manual_start: false,
            sounds: vec![],
        })
    }

    pub fn take_sounds(&mut self) -> Vec<tomo_scene::SoundCue> {
        std::mem::take(&mut self.sounds)
    }
    pub fn begin(&mut self) {
        self.manual_start = true;
    }
    pub fn content(&self) -> &tomo_scene::Content {
        &self.content
    }

    pub fn rules(&self) -> &GameRules {
        &self.rules
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
    pub fn score(&self) -> i32 {
        self.score
    }
    pub fn recent_events(&self) -> &[RuleEvent] {
        &self.recent_events
    }
    pub fn rule_fires(&self, id: &str) -> u32 {
        self.rules
            .logic
            .rules
            .iter()
            .position(|r| r.id == id)
            .map_or(0, |i| self.rule_fires[i])
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
        // Rules are validated before constructing the world; runtime bodies are never removed.
        self.snapshot
            .bodies
            .iter()
            .find(|body| body.id == self.rules.player)
            .expect("validated player")
    }

    pub fn restart(&mut self) -> Result<(), Problem> {
        let mut fresh = Self::with_content(
            self.scene().clone(),
            self.rules.clone(),
            self.content.clone(),
        )?;
        fresh.best_ticks = self.best_ticks;
        *self = fresh;
        Ok(())
    }

    pub fn step(&mut self, input: Input) -> Result<(), Problem> {
        if self.failed {
            return Err(Problem::new(
                "FAILED_GAME",
                "/game",
                "restart after a rule or physics failure",
            ));
        }
        let before = self.elapsed_ticks;
        let before_score = self.score;
        let before_status = self.status;
        let fires = self.rule_fires.clone();
        self.sounds.clear();
        let result = self.advance(input);
        if result.is_err() {
            self.failed = true;
        } else if self.elapsed_ticks != before {
            self.record(input);
            for cue in &self.content.sounds {
                let fire = match &cue.trigger {
                    tomo_scene::CueTrigger::Start {} => before == 0,
                    tomo_scene::CueTrigger::Score {} => before_score != self.score,
                    tomo_scene::CueTrigger::Won {} => {
                        before_status != Status::Won && self.status == Status::Won
                    }
                    tomo_scene::CueTrigger::Lost {} => {
                        before_status != Status::Lost && self.status == Status::Lost
                    }
                    tomo_scene::CueTrigger::Rule { id } => self
                        .rules
                        .logic
                        .rules
                        .iter()
                        .position(|r| r.id == *id)
                        .is_some_and(|i| fires[i] != self.rule_fires[i]),
                };
                if fire {
                    self.sounds.push(cue.clone());
                }
            }
        }
        result
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
