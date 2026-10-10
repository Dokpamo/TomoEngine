// SPDX-License-Identifier: MIT
//! Compiled Rust game behavior. This API runs trusted native code, never project JSON scripts.
use crate::{Action, Game, Input, Logic, Rule, StartMode, Trigger};
use tomo_scene::Problem;

/// Called before each fixed physics tick. Return at most eight validated actions.
/// Keep persistent behavior state in your implementing struct.
pub trait Behavior {
    fn reset(&mut self);
    fn tick(&mut self, game: &Game, input: Input) -> Result<Vec<Action>, Problem>;
}
/// Owns a game and its native behavior, usable from a custom host or a headless integration test.
pub struct ScriptedGame<B: Behavior> {
    pub game: Game,
    pub behavior: B,
}
impl<B: Behavior> ScriptedGame<B> {
    pub fn new(game: Game, behavior: B) -> Self {
        Self { game, behavior }
    }
    pub fn step(&mut self, input: Input) -> Result<(), Problem> {
        self.game.step_behavior(&mut self.behavior, input)
    }
    pub fn restart(&mut self) -> Result<(), Problem> {
        self.game.restart()?;
        self.behavior.reset();
        Ok(())
    }
}
impl Game {
    pub fn step_behavior(
        &mut self,
        behavior: &mut dyn Behavior,
        input: Input,
    ) -> Result<(), Problem> {
        if self.failed {
            return Err(Problem::new(
                "FAILED_GAME",
                "/sdk",
                "rebuild the game and behavior after a failure",
            ));
        }
        if matches!(self.status(), crate::Status::Won | crate::Status::Lost) {
            return Ok(());
        }
        self.history_complete = false;
        let before_score = self.score;
        let before_status = self.status;
        let result = (|| {
            let actions = behavior.tick(self, input)?;
            if !actions.is_empty() {
                Logic {
                    schema_version: 1,
                    start: StartMode::Immediate,
                    rules: vec![Rule {
                        id: "sdk".into(),
                        name: "SDK commands".into(),
                        enabled: true,
                        once: false,
                        trigger: Trigger::Start {},
                        conditions: vec![],
                        actions: actions.clone(),
                    }],
                }
                .validate(self.scene())?;
                if actions.iter().any(|a| matches!(a, Action::MovePlayer {}))
                    && self.rules.player.is_empty()
                {
                    return Err(Problem::new(
                        "GAME_RULES",
                        "/sdk",
                        "MovePlayer requires a player role",
                    ));
                }
                for action in actions {
                    self.apply_action(action, input)?;
                }
            }
            self.step(input)
        })();
        if result.is_err() {
            self.failed = true;
        } else {
            for cue in &self.content.sounds {
                let fire = match cue.trigger {
                    tomo_scene::CueTrigger::Score {} => self.score != before_score,
                    tomo_scene::CueTrigger::Won {} => {
                        before_status != crate::Status::Won && self.status == crate::Status::Won
                    }
                    tomo_scene::CueTrigger::Lost {} => {
                        before_status != crate::Status::Lost && self.status == crate::Status::Lost
                    }
                    _ => false,
                };
                if fire && !self.sounds.iter().any(|existing| existing.id == cue.id) {
                    self.sounds.push(cue.clone());
                }
            }
        }
        result
    }
}
