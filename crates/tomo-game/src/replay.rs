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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyReplay {
    schema_version: u32,
    segments: Vec<LegacySegment>,
    expect_win: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacySegment {
    ticks: u32,
    input: LegacyInput,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyInput {
    #[serde(default)]
    up: bool,
    #[serde(default)]
    down: bool,
    #[serde(default)]
    left: bool,
    #[serde(default)]
    right: bool,
}

impl Replay {
    pub fn from_json(text: &str) -> Result<Self, Problem> {
        if text.len() > tomo_scene::MAX_DOCUMENT_BYTES {
            return Err(Problem::new("LIMIT", "/", "replay exceeds 1 MiB"));
        }
        #[derive(Deserialize)]
        struct Header {
            schema_version: u32,
        }
        let json_error = |error: serde_json::Error| Problem::new("JSON", "/", error.to_string());
        let header: Header = serde_json::from_str(text).map_err(json_error)?;
        let replay: Self = match header.schema_version {
            1 => {
                let old: LegacyReplay = serde_json::from_str(text).map_err(json_error)?;
                Self {
                    schema_version: old.schema_version,
                    expect_win: old.expect_win,
                    segments: old
                        .segments
                        .into_iter()
                        .map(|s| ReplaySegment {
                            ticks: s.ticks,
                            input: Input {
                                up: s.input.up,
                                down: s.input.down,
                                left: s.input.left,
                                right: s.input.right,
                                action: false,
                            },
                        })
                        .collect(),
                }
            }
            2 => serde_json::from_str(text).map_err(json_error)?,
            _ => {
                return Err(Problem::new(
                    "VERSION",
                    "/schema_version",
                    "only replay v1 and v2 are supported",
                ));
            }
        };
        replay.validate()?;
        Ok(replay)
    }

    fn validate(&self) -> Result<(), Problem> {
        if ![1, 2].contains(&self.schema_version)
            || (self.schema_version == 1 && self.segments.iter().any(|s| s.input.action))
        {
            return Err(Problem::new(
                "VERSION",
                "/schema_version",
                "use replay v1 for direction inputs or v2 for the Space/action key",
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
        // Bound rule work independently of the physics budget, before the first mutation.
        if game.rules().logic.work_per_tick() * ticks > crate::MAX_LOGIC_WORK {
            return Err(Problem::new(
                "LIMIT",
                "/segments",
                "replay rule work budget exceeded",
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
