// SPDX-License-Identifier: MIT
//! A bounded input checkpoint rebuilds the complete physics state, including contact caches.
use crate::{Game, GameRules, Input, Replay, ReplaySegment, Status};
use serde::{Deserialize, Serialize};
use tomo_scene::{Content, Problem, Scene};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RecordedInput {
    ticks: u32,
    input: Input,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Checkpoint {
    pub schema_version: u32,
    engine_version: String,
    fma: bool,
    scene: Scene,
    rules: GameRules,
    content: Content,
    inputs: Vec<RecordedInput>,
    elapsed_ticks: u32,
    status: Status,
    score: i32,
    best_ticks: Option<u32>,
    manual_start: bool,
}
impl Game {
    pub(crate) fn record(&mut self, input: Input) {
        let bodies = self.scene().bodies.len().max(1) as u64;
        let ticks = u64::from(self.elapsed_ticks);
        if ticks > 36000
            || ticks * bodies * bodies > tomo_runtime::MAX_WORK_UNITS
            || ticks * self.rules.logic.work_per_tick() > crate::MAX_LOGIC_WORK
        {
            self.history_complete = false;
            return;
        }
        if let Some(last) = self.history.last_mut() {
            if last.input == input {
                last.ticks += 1;
                return;
            }
        }
        if self.history.len() == 1024 {
            self.history_complete = false;
            return;
        }
        self.history.push(RecordedInput { ticks: 1, input });
    }
    pub fn checkpoint(&self) -> Result<Checkpoint, Problem> {
        if self.failed || !self.history_complete {
            return Err(Problem::new(
                "SAVE_LIMIT",
                "/save",
                "saving requires a healthy declarative game within the bounded replay history; restart to reset",
            ));
        }
        Ok(Checkpoint {
            schema_version: 1,
            engine_version: env!("CARGO_PKG_VERSION").into(),
            fma: cfg!(feature = "fma"),
            scene: self.scene().clone(),
            rules: self.rules.clone(),
            content: self.content.clone(),
            inputs: self.history.clone(),
            elapsed_ticks: self.elapsed_ticks,
            status: self.status,
            score: self.score,
            best_ticks: self.best_ticks,
            manual_start: self.manual_start,
        })
    }
}
impl Checkpoint {
    pub fn from_json(text: &str) -> Result<Self, Problem> {
        if text.len() > tomo_scene::MAX_DOCUMENT_BYTES {
            return Err(Problem::new("LIMIT", "/save", "save exceeds 1 MiB"));
        }
        let value: Self =
            serde_json::from_str(text).map_err(|e| Problem::new("JSON", "/save", e.to_string()))?;
        value.validate_header()?;
        Ok(value)
    }
    fn validate_header(&self) -> Result<(), Problem> {
        if self.schema_version != 1
            || self.engine_version != env!("CARGO_PKG_VERSION")
            || self.fma != cfg!(feature = "fma")
        {
            return Err(Problem::new(
                "SAVE_VERSION",
                "/save",
                "checkpoint engine/schema/FP feature mismatch",
            ));
        }
        if self.elapsed_ticks > 36000
            || self.inputs.len() > 1024
            || self.best_ticks.is_some_and(|v| v == 0)
        {
            return Err(Problem::new(
                "SAVE_LIMIT",
                "/save",
                "invalid checkpoint limits",
            ));
        }
        Ok(())
    }
    pub fn to_json(&self) -> Result<String, Problem> {
        self.validate_header()?;
        let text = serde_json::to_string_pretty(self)
            .map_err(|e| Problem::new("JSON", "/save", e.to_string()))?;
        if text.len() > tomo_scene::MAX_DOCUMENT_BYTES {
            return Err(Problem::new("LIMIT", "/save", "save exceeds 1 MiB"));
        }
        Ok(text)
    }
    pub fn matches(&self, scene: &Scene, rules: &GameRules, content: &Content) -> bool {
        let mut scene = scene.clone();
        scene.bodies.sort_by(|a, b| a.id.cmp(&b.id));
        self.scene == scene && self.rules == *rules && self.content == *content
    }
    /// Rebuild a candidate first; callers replace a live session only after this returns Ok.
    pub fn restore(&self) -> Result<Game, Problem> {
        self.validate_header()?;
        let mut game =
            Game::with_content(self.scene.clone(), self.rules.clone(), self.content.clone())?;
        if self.manual_start {
            game.begin();
        }
        if !self.inputs.is_empty() {
            let replay = Replay {
                schema_version: 2,
                segments: self
                    .inputs
                    .iter()
                    .map(|i| ReplaySegment {
                        ticks: i.ticks,
                        input: i.input,
                    })
                    .collect(),
                expect_win: self.status == Status::Won,
            };
            replay.run(&mut game)?;
        }
        if game.elapsed_ticks != self.elapsed_ticks
            || game.status != self.status
            || game.score != self.score
            || game.history != self.inputs
        {
            return Err(Problem::new(
                "SAVE_REPLAY",
                "/save",
                "checkpoint does not reproduce its recorded outcome",
            ));
        }
        game.best_ticks = self.best_ticks;
        Ok(game)
    }
}
