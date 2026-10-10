// SPDX-License-Identifier: MIT
#![forbid(unsafe_code)]
use tomo_game::{
    Action, Game, Input, Status,
    sdk::{Behavior, ScriptedGame},
};
use tomo_scene::Problem;

struct Counter {
    ticks: u32,
    invalid: bool,
}
impl Behavior for Counter {
    fn reset(&mut self) {
        self.ticks = 0;
    }
    fn tick(&mut self, _: &Game, _: Input) -> Result<Vec<Action>, Problem> {
        self.ticks += 1;
        if self.invalid {
            return Ok(vec![Action::SetVelocity {
                body: "missing".into(),
                linear: [0.; 3],
            }]);
        }
        Ok(if self.ticks == 3 {
            vec![Action::AddScore { amount: 2 }, Action::Win {}]
        } else {
            vec![Action::MovePlayer {}]
        })
    }
}
#[test]
fn native_behavior_executes_validated_actions_resets_and_rejects_unsupported_saves() {
    let base = Game::new().unwrap();
    let mut content = tomo_scene::Content::default();
    content.sounds.push(tomo_scene::SoundCue {
        id: "won".into(),
        trigger: tomo_scene::CueTrigger::Won {},
        frequency: 440.,
        seconds: 0.1,
        volume: 0.1,
    });
    let game = Game::with_content(base.scene().clone(), base.rules().clone(), content).unwrap();
    let mut host = ScriptedGame::new(
        game,
        Counter {
            ticks: 0,
            invalid: false,
        },
    );
    for _ in 0..3 {
        host.step(Input {
            right: true,
            ..Input::default()
        })
        .unwrap();
    }
    assert_eq!(host.game.status(), Status::Won);
    assert_eq!(host.game.score(), 2);
    assert_eq!(host.game.take_sounds()[0].id, "won");
    assert!(host.game.checkpoint().is_err());
    host.restart().unwrap();
    assert_eq!(host.behavior.ticks, 0);
    assert_eq!(host.game.score(), 0);
    assert!(host.game.checkpoint().is_ok());
}
#[test]
fn invalid_native_command_fails_before_mutation_and_stays_failed() {
    let mut host = ScriptedGame::new(
        Game::new().unwrap(),
        Counter {
            ticks: 0,
            invalid: true,
        },
    );
    let before = host.game.snapshot().clone();
    assert!(host.step(Input::default()).is_err());
    assert_eq!(host.game.snapshot(), &before);
    assert!(host.step(Input::default()).is_err());
    assert_eq!(host.behavior.ticks, 1);
    assert!(host.game.checkpoint().is_err());
}
