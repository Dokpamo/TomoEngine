// SPDX-License-Identifier: MIT
//! Bounded, declarative game rules. These data never invoke scripts or external programs.
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
pub use tomo_runtime::ContactKind;
use tomo_scene::{BodySpec, Motion, Problem, Scene};

pub const MAX_RULES: usize = 64;
pub const MAX_CONDITIONS: usize = 8;
pub const MAX_ACTIONS: usize = 8;
pub const MAX_TIMER_TICKS: u32 = 36_000;
pub const MAX_LOGIC_WORK: u64 = 5_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartMode {
    FirstInput,
    Immediate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Key {
    Up,
    Down,
    Left,
    Right,
    Action,
}

impl Key {
    pub fn held(self, input: super::Input) -> bool {
        match self {
            Self::Up => input.up,
            Self::Down => input.down,
            Self::Left => input.left,
            Self::Right => input.right,
            Self::Action => input.action,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Trigger {
    Start {},
    EveryTick {},
    KeyPressed {
        key: Key,
    },
    Timer {
        ticks: u32,
        repeat: bool,
    },
    Contact {
        body_a: String,
        body_b: String,
        kind: ContactKind,
    },
    GoalReached {},
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Condition {
    KeyHeld { key: Key },
    ScoreAtLeast { value: i32 },
    HeightBelow { body: String, y: f32 },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    MovePlayer {},
    SetVelocity { body: String, linear: [f32; 3] },
    AddScore { amount: i32 },
    Win {},
    Lose {},
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub once: bool,
    pub trigger: Trigger,
    pub conditions: Vec<Condition>,
    pub actions: Vec<Action>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Logic {
    pub schema_version: u32,
    pub start: StartMode,
    pub rules: Vec<Rule>,
}
impl Default for Logic {
    fn default() -> Self {
        Self {
            schema_version: 1,
            start: StartMode::FirstInput,
            rules: vec![
                Rule {
                    id: "movement".into(),
                    name: "Move with WASD / arrows".into(),
                    enabled: true,
                    once: false,
                    trigger: Trigger::EveryTick {},
                    conditions: vec![],
                    actions: vec![Action::MovePlayer {}],
                },
                Rule {
                    id: "finish".into(),
                    name: "Reach the goal to win".into(),
                    enabled: true,
                    once: true,
                    trigger: Trigger::GoalReached {},
                    conditions: vec![],
                    actions: vec![Action::Win {}],
                },
            ],
        }
    }
}

fn body<'a>(scene: &'a Scene, id: &str, path: &str) -> Result<&'a BodySpec, Problem> {
    scene
        .bodies
        .iter()
        .find(|b| b.id == id)
        .ok_or_else(|| Problem::new("REFERENCE", path, format!("unknown body ID: {id}")))
}
impl Logic {
    /// Upper bound on rule/condition/action visits per tick (independent of trigger outcomes).
    pub fn work_per_tick(&self) -> u64 {
        self.rules
            .iter()
            .map(|r| 1 + r.conditions.len() as u64 + r.actions.len() as u64)
            .sum()
    }
    pub fn validate(&self, scene: &Scene) -> Result<(), Problem> {
        if self.schema_version != 1 {
            return Err(Problem::new(
                "VERSION",
                "/game/logic/schema_version",
                "only logic v1 is supported",
            ));
        }
        if self.rules.len() > MAX_RULES {
            return Err(Problem::new(
                "LIMIT",
                "/game/logic/rules",
                "at most 64 rules",
            ));
        }
        let mut ids = BTreeSet::new();
        for (i, rule) in self.rules.iter().enumerate() {
            let path = format!("/game/logic/rules/{i}");
            let error = |field: &str, message: &str| {
                Problem::new("LOGIC", format!("{path}/{field}"), message)
            };
            if rule.id.is_empty()
                || rule.id.len() > 64
                || !rule
                    .id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
                || !ids.insert(&rule.id)
            {
                return Err(error(
                    "id",
                    "use a unique 1..64 character ASCII ID: letters, digits, _ or -",
                ));
            }
            if rule.name.trim().is_empty()
                || rule.name.len() > 128
                || rule.name.chars().any(char::is_control)
            {
                return Err(error(
                    "name",
                    "use 1..128 UTF-8 bytes without control characters",
                ));
            }
            if rule.conditions.len() > MAX_CONDITIONS
                || rule.actions.is_empty()
                || rule.actions.len() > MAX_ACTIONS
            {
                return Err(Problem::new(
                    "LIMIT",
                    &path,
                    "use at most 8 conditions and 1..8 actions",
                ));
            }
            match &rule.trigger {
                Trigger::Timer { ticks, .. } if !(1..=MAX_TIMER_TICKS).contains(ticks) => {
                    return Err(error("trigger/ticks", "timer must be 1..36000 ticks"));
                }
                Trigger::Contact { body_a, body_b, .. } => {
                    let a = body(scene, body_a, &format!("{path}/trigger/body_a"))?;
                    let b = body(scene, body_b, &format!("{path}/trigger/body_b"))?;
                    if a.id == b.id || (a.motion != Motion::Dynamic && b.motion != Motion::Dynamic)
                    {
                        return Err(error(
                            "trigger",
                            "contact needs distinct bodies with at least one dynamic body",
                        ));
                    }
                }
                _ => {}
            }
            for (j, condition) in rule.conditions.iter().enumerate() {
                let p = format!("{path}/conditions/{j}");
                match condition {
                    Condition::HeightBelow { body: id, y } => {
                        body(scene, id, &format!("{p}/body"))?;
                        if !y.is_finite() || !(-10000. ..=10000.).contains(y) {
                            return Err(Problem::new(
                                "LOGIC",
                                p,
                                "height must be finite and within +/-10000 metres",
                            ));
                        }
                    }
                    Condition::ScoreAtLeast { value }
                        if !(-1_000_000..=1_000_000).contains(value) =>
                    {
                        return Err(Problem::new(
                            "LOGIC",
                            p,
                            "score threshold must be within +/-1000000",
                        ));
                    }
                    _ => {}
                }
            }
            for (j, action) in rule.actions.iter().enumerate() {
                let p = format!("{path}/actions/{j}");
                match action {
                    Action::SetVelocity { body: id, linear } => {
                        if body(scene, id, &format!("{p}/body"))?.motion == Motion::Static {
                            return Err(Problem::new(
                                "LOGIC",
                                p,
                                "velocity needs a dynamic or kinematic body",
                            ));
                        }
                        if linear.iter().any(|v| !v.is_finite() || v.abs() > 100.) {
                            return Err(Problem::new(
                                "LOGIC",
                                p,
                                "velocity components must be finite and within +/-100 m/s",
                            ));
                        }
                    }
                    Action::AddScore { amount } if !(-1_000_000..=1_000_000).contains(amount) => {
                        return Err(Problem::new(
                            "LOGIC",
                            p,
                            "score change must be within +/-1000000",
                        ));
                    }
                    Action::Win {} | Action::Lose {} if j + 1 != rule.actions.len() => {
                        return Err(Problem::new(
                            "LOGIC",
                            p,
                            "win/lose must be the last action in a rule",
                        ));
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RuleEvent {
    pub tick: u32,
    pub rule_id: String,
}
