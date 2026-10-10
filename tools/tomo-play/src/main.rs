// SPDX-License-Identifier: MIT
#![forbid(unsafe_code)]
use tomo_play::desktop;

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
    let (options, style) = view::frame(game, 1120, 760, false, false)?;
    let image = tomo_render::capture_styled(game.scene(), game.snapshot(), &options, &style)?;
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
    let bundled = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|p| p.to_owned()))
        .and_then(|p| {
            [p.join("game"), p.join("../Resources/game")]
                .into_iter()
                .find(|p| p.join(tomo_project::PROJECT_FILE).is_file())
        });
    let default_project = || -> Result<tomo_project::Project, Problem> {
        match &bundled {
            Some(path) => Ok(tomo_project::ProjectFile::open(path)?.project().clone()),
            None => tomo_project::Project::template("Box Run"),
        }
    };
    let default_save = || -> std::path::PathBuf {
        if bundled.is_none() {
            return "progress.tomo.json".into();
        }
        let base = if cfg!(target_os = "windows") {
            std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from)
        } else {
            std::env::var_os("HOME")
                .map(std::path::PathBuf::from)
                .map(|p| {
                    p.join(if cfg!(target_os = "ios") {
                        "Documents"
                    } else if cfg!(target_os = "macos") {
                        "Library/Application Support"
                    } else {
                        ".local/share"
                    })
                })
        };
        use std::hash::{Hash, Hasher};
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        if let Ok(p) = default_project() {
            p.name.hash(&mut hash);
        }
        base.unwrap_or_else(|| ".".into())
            .join("TomoEngine")
            .join(format!("{:016x}", hash.finish()))
            .join("progress.tomo.json")
    };
    let (project, args, save_path) = if let [flag, directory, tail @ ..] = args {
        if flag == "--project" {
            let file = tomo_project::ProjectFile::open(directory)?;
            let save = file.path().with_file_name("progress.tomo.json");
            (file.project().clone(), tail, save)
        } else {
            (default_project()?, args, default_save())
        }
    } else {
        (default_project()?, args, default_save())
    };
    let game = tomo_project::Session::new(project)?;
    match args {
        [] => desktop::run(game, None, save_path),
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
        [flag] if flag == "--smoke-test" => desktop::run(game, Some(120), save_path),
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
