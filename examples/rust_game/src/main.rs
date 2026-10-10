// SPDX-License-Identifier: MIT
#![forbid(unsafe_code)]
use tomo_game::{
    Action, Game, Input, StartMode,
    sdk::{Behavior, ScriptedGame},
};
use tomo_scene::Problem;

#[derive(Default)]
struct MyGame {
    ticks: u32,
    was_action: bool,
}
impl Behavior for MyGame {
    fn reset(&mut self) {
        *self = Self::default();
    }
    fn tick(&mut self, game: &Game, input: Input) -> Result<Vec<Action>, Problem> {
        self.ticks += 1;
        let [x, z] = input.direction();
        let mut y = game.player().linear_velocity[1];
        let mut commands = Vec::new();
        if input.action && !self.was_action {
            y = 6.;
            commands.push(Action::AddScore { amount: 1 });
        }
        self.was_action = input.action;
        commands.push(Action::SetVelocity {
            body: game.rules().player.clone(),
            linear: [x * 4., y, z * 4.],
        });
        if game.score() >= 3 {
            commands.push(Action::Win {});
        } else if self.ticks >= 3600 {
            commands.push(Action::Lose {});
        }
        Ok(commands)
    }
}
fn project() -> Result<tomo_project::Project, Problem> {
    let mut p = tomo_project::Project::template("Rust SDK - Jump three times")?;
    p.game.logic.rules.clear();
    p.game.logic.start = StartMode::Immediate;
    let mut title = tomo_scene::Widget::label("instructions");
    title.text = "SPACE THREE TIMES TO WIN".into();
    title.position = [0.2, 0.23];
    title.size = [0.6, 0.06];
    p.content.ui.push(title);
    Ok(p)
}
fn run() -> Result<(), Problem> {
    let p = project()?;
    if std::env::args().any(|a| a == "--headless") {
        let mut game = ScriptedGame::new(p.game()?, MyGame::default());
        for _ in 0..3 {
            game.step(Input {
                action: true,
                ..Default::default()
            })?;
            game.step(Input::default())?;
        }
        if game.game.status() != tomo_game::Status::Won {
            return Err(Problem::new("CHECK", "/sdk", "example did not win"));
        }
        println!(
            "{}",
            serde_json::json!({"ok":true,"status":game.game.status(),"score":game.game.score(),"snapshot":game.game.snapshot()})
        );
    } else {
        tomo_play::desktop::run_scripted(p, Box::<MyGame>::default())?;
    }
    Ok(())
}
fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            println!("{}", serde_json::json!({"error":e}));
            std::process::ExitCode::FAILURE
        }
    }
}
