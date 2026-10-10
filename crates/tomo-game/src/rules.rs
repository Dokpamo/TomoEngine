// SPDX-License-Identifier: MIT
use serde::{Deserialize, Serialize};
use tomo_scene::{Motion, Problem, Scene, ShapeSpec};

/// The first game template: an upright velocity-controlled body reaches a rectangular pad.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameRules {
    pub player: String,
    pub goal: String,
    pub speed: f32,
}
impl Default for GameRules {
    fn default() -> Self {
        Self {
            player: "player".into(),
            goal: "goal".into(),
            speed: super::SPEED,
        }
    }
}
fn radius(shape: &ShapeSpec) -> f32 {
    match shape {
        ShapeSpec::Box { half_extents } => half_extents.iter().map(|x| x * x).sum::<f32>().sqrt(),
        ShapeSpec::Sphere { radius } => *radius,
    }
}
impl GameRules {
    pub fn validate(&self, scene: &Scene) -> Result<(), Problem> {
        let error = |message| Problem::new("GAME_RULES", "/game", message);
        if !self.speed.is_finite() || !(0.1..=30.0).contains(&self.speed) {
            return Err(error("speed must be finite and 0.1..30 metres/second"));
        }
        let player = scene
            .bodies
            .iter()
            .find(|b| b.id == self.player)
            .ok_or_else(|| error("player must reference an existing body"))?;
        let goal = scene
            .bodies
            .iter()
            .find(|b| b.id == self.goal)
            .ok_or_else(|| error("goal must reference an existing body"))?;
        if self.player == self.goal || player.motion != Motion::Dynamic || player.is_sensor {
            return Err(error(
                "player must be a dynamic solid body distinct from the goal",
            ));
        }
        if goal.motion != Motion::Static
            || !goal.is_sensor
            || goal.rotation[..3].iter().any(|v| v.abs() > 1e-6)
        {
            return Err(error("goal must be a static, axis-aligned sensor box"));
        }
        match goal.shape {
            ShapeSpec::Box { half_extents }
                if half_extents[0] > radius(&player.shape)
                    && half_extents[2] > radius(&player.shape) =>
            {
                Ok(())
            }
            _ => Err(error(
                "goal must be a box wider and deeper than the player's bounding sphere",
            )),
        }
    }
    pub(crate) fn reached(&self, scene: &Scene, position: [f32; 3]) -> bool {
        let player = scene
            .bodies
            .iter()
            .find(|b| b.id == self.player)
            .expect("validated player");
        let goal = scene
            .bodies
            .iter()
            .find(|b| b.id == self.goal)
            .expect("validated goal");
        let ShapeSpec::Box { half_extents: h } = goal.shape else {
            return false;
        };
        let r = radius(&player.shape);
        // Conservative horizontal containment for every player orientation; limited vertical range.
        (position[0] - goal.position[0]).abs() <= h[0] - r
            && (position[2] - goal.position[2]).abs() <= h[2] - r
            && position[1] >= goal.position[1] - h[1]
            && position[1] <= goal.position[1] + h[1] + r * 1.55
    }
}
