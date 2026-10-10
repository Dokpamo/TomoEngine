// SPDX-License-Identifier: MIT
use crate::{Game, Input, Status};
use serde::{Deserialize, Serialize};
use tomo_scene::Problem;

pub const MAX_REPLAY_TICKS: u32 = 36_000;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Replay {
    pub schema_version: u32,
    pub segments: Vec<ReplaySegment>,
    pub expect_win: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplaySegment {
    pub ticks: u32,
    pub input: Input,
}

impl Replay {
    pub fn from_json(text: &str) -> Result<Self, Problem> {
        if text.len() > tomo_scene::MAX_DOCUMENT_BYTES {
            return Err(Problem::new("LIMIT", "/", "replay exceeds 1 MiB"));
        }
        let replay: Self = serde_json::from_str(text)
            .map_err(|error| Problem::new("JSON", "/", error.to_string()))?;
        replay.validate()?;
        Ok(replay)
    }

    fn validate(&self) -> Result<(), Problem> {
        if self.schema_version != 1 {
            return Err(Problem::new(
                "VERSION",
                "/schema_version",
                "only replay v1 is supported",
            ));
        }
        if self.segments.is_empty()
            || self.segments.len() > 1024
            || self.segments.iter().any(|s| s.ticks == 0)
            || self
                .segments
                .iter()
                .map(|s| u64::from(s.ticks))
                .sum::<u64>()
                > u64::from(MAX_REPLAY_TICKS)
        {
            return Err(Problem::new(
                "LIMIT",
                "/segments",
                "use 1..1024 nonempty segments and at most 36000 ticks",
            ));
        }
        Ok(())
    }

    pub fn run(&self, game: &mut Game) -> Result<bool, Problem> {
        // Public Rust callers receive the same limits as JSON callers, before any mutation.
        self.validate()?;
        let bodies = game.scene().bodies.len().max(1) as u64;
        let ticks: u64 = self.segments.iter().map(|s| u64::from(s.ticks)).sum();
        if bodies * bodies * ticks > tomo_runtime::MAX_WORK_UNITS {
            return Err(Problem::new(
                "LIMIT",
                "/segments",
                "replay body-squared work budget exceeded",
            ));
        }
        for segment in &self.segments {
            for _ in 0..segment.ticks {
                game.step(segment.input)?;
            }
        }
        Ok((game.status() == Status::Won) == self.expect_win)
    }
}
