// SPDX-License-Identifier: MIT
#![forbid(unsafe_code)]
mod desktop;

use serde_json::json;
use std::{
    fs::OpenOptions,
    io::{Read, Write},
    process::ExitCode,
};
use tomo_play::{Game, Replay, view};
use tomo_scene::Problem;

fn read_replay(path: &str) -> Result<Replay, Problem> {
    let file = std::fs::File::open(path).map_err(|e| Problem::new("IO", path, e.to_string()))?;
    if !file
        .metadata()
        .map_err(|e| Problem::new("IO", path, e.to_string()))?
        .is_file()
    {
        return Err(Problem::new("IO", path, "expected a regular file"));
    }
    let mut text = String::new();
    file.take(tomo_scene::MAX_DOCUMENT_BYTES as u64 + 1)
        .read_to_string(&mut text)
        .map_err(|e| Problem::new("IO", path, e.to_string()))?;
    Replay::from_json(&text)
}

fn capture(game: &Game, path: &str) -> Result<serde_json::Value, Problem> {
    let options = view::options(1120, 760, false);
    let image = tomo_render::capture_styled(
        game.scene(),
        game.snapshot(),
        &options,
        &view::style(game, &options, false),
    )?;
    let bytes = image.png()?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| Problem::new("OUTPUT", path, e.to_string()))?;
    if let Err(error) = file.write_all(&bytes).and_then(|_| file.sync_all()) {
        drop(file);
        let _ = std::fs::remove_file(path);
        return Err(Problem::new("OUTPUT", path, error.to_string()));
    }
    Ok(json!({"path":path,"metadata":image.metadata}))
}

fn execute(args: &[String]) -> Result<u8, Problem> {
    let (game, args) = if let [flag, directory, tail @ ..] = args {
        if flag == "--project" {
            (
                tomo_project::ProjectFile::open(directory)?
                    .project()
                    .game()?,
                tail,
            )
        } else {
            (Game::new()?, args)
        }
    } else {
        (Game::new()?, args)
    };
    match args {
        [] => desktop::run(game, None),
        [flag] if flag == "--help" => {
            println!(
                "{}",
                json!({"schema_version":1,"game":"Box Run","commands":[
                "tomo-play [--project DIRECTORY]", "tomo-play [--project DIRECTORY] --replay REPLAY.json [--capture NEW.png]",
                "tomo-play --capture NEW.png", "tomo-play --smoke-test"],
                "controls":{"move":"WASD / arrows","restart":"R","pause":"P","colliders":"C","quit":"Escape"}})
            );
            Ok(0)
        }
        [flag] if flag == "--smoke-test" => desktop::run(game, Some(120)),
        [flag, path] if flag == "--capture" => {
            println!(
                "{}",
                json!({"schema_version":1,"status":game.status(),"capture":capture(&game,path)?})
            );
            Ok(0)
        }
        [flag, path, tail @ ..] if flag == "--replay" => {
            let output = match tail {
                [] => None,
                [flag, path] if flag == "--capture" => Some(path),
                _ => {
                    return Err(Problem::new(
                        "ARGUMENT",
                        "/",
                        "use --replay FILE [--capture NEW.png]",
                    ));
                }
            };
            let replay = read_replay(path)?;
            let mut game = game;
            let passed = replay.run(&mut game)?;
            let capture = output.map(|path| capture(&game, path)).transpose()?;
            println!(
                "{}",
                json!({"schema_version":1,"passed":passed,"status":game.status(),
                "seconds":game.seconds(),"score":game.score(),"rule_events":game.recent_events(),"snapshot":game.snapshot(),"capture":capture})
            );
            Ok(u8::from(!passed))
        }
        _ => Err(Problem::new("ARGUMENT", "/", "use --help for commands")),
    }
}

fn main() -> ExitCode {
    match execute(&std::env::args().skip(1).collect::<Vec<_>>()) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            let code = match error.code.as_str() {
                "RENDER" | "GPU_UNAVAILABLE" | "WINDOW" | "SIMULATION" | "OUTPUT"
                | "RULE_EXECUTION" | "FAILED_GAME" | "FAILED_WORLD" | "INTERNAL" => 3,
                _ => 2,
            };
            println!("{}", json!({"schema_version":1,"error":error}));
            ExitCode::from(code)
        }
    }
}
