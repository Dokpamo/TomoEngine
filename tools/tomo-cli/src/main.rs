// SPDX-License-Identifier: MIT
//! Machine-oriented CLI. It does not execute external code or call providers.
#![forbid(unsafe_code)]

use serde_json::{Value, json};
use std::fs::File;
use std::io::{Read, Write};
use std::process::ExitCode;
use tomo_runtime::Runtime;
use tomo_scene::{Patch, Problem, Scene};

#[cfg(feature = "render")]
mod capture;

fn read_document(path: &str) -> Result<String, Problem> {
    let file = File::open(path).map_err(|error| Problem::new("IO", path, error.to_string()))?;
    let metadata = file
        .metadata()
        .map_err(|error| Problem::new("IO", path, error.to_string()))?;
    if !metadata.is_file() {
        return Err(Problem::new("IO", path, "expected a regular file"));
    }
    let mut bytes = Vec::new();
    file.take((tomo_scene::MAX_DOCUMENT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| Problem::new("IO", path, error.to_string()))?;
    if bytes.len() > tomo_scene::MAX_DOCUMENT_BYTES {
        return Err(Problem::new("LIMIT", path, "document exceeds 1 MiB"));
    }
    String::from_utf8(bytes).map_err(|error| Problem::new("UTF8", path, error.to_string()))
}

fn load_scene(path: &str) -> Result<Scene, Problem> {
    Scene::from_json(&read_document(path)?)
}

fn dispatch(args: &[String]) -> Result<(Value, u8), Problem> {
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["capabilities"] => Ok((
            json!({
                "schema_version": 1,
                "ok": true,
                "stage": if cfg!(feature = "render") { "M2-capture" } else { "M1-headless" },
                "commands": if cfg!(feature = "render") {
                    vec!["scene validate", "scene patch", "inspect", "run", "capabilities", "capture"]
                } else { vec!["scene validate", "scene patch", "inspect", "run", "capabilities"] },
                "supported_shapes": ["box", "sphere"],
                "rendering": cfg!(feature = "render"),
                "rendering_requires_adapter": true,
                "interactive_window": false,
                "companion_game": "cargo run -p tomo-play --locked --offline -- [--project DIRECTORY]",
                "companion_editor": "cargo run -p tomo-editor --locked --offline -- [--project DIRECTORY]",
                "project_commands": ["project new", "project validate", "project edit", "project rollback", "project inspect"],
                "project_schema": 3,
                "project_import_versions": [1, 2, 3],
                "declarative_game_logic": true,
                "content_schema": 1,
                "general_entities": true,
                "cameras": true,
                "scene_switching": true,
                "prefab_copies": true,
                "game_ui": true,
                "synthesized_audio": true,
                "progress_save": true,
                "project_edit_journal": true,
                "compiled_rust_sdk": "examples/rust_game",
                "code_edit_tool": "scripts/code-edit.py",
                "package_tool": "scripts/package-game.py",
                "arbitrary_game_scripts": false,
                "autonomous_agent": false,
                "tripo": false,
                "jolt_complete": false,
                "physics_external_dependencies": 0,
                "limits": {"document_bytes": tomo_scene::MAX_DOCUMENT_BYTES,
                    "bodies": tomo_scene::MAX_BODIES, "ticks": tomo_runtime::MAX_TICKS,
                    "work_units": tomo_runtime::MAX_WORK_UNITS}
            }),
            0,
        )),
        [] | ["--help"] | ["help"] => Ok((
            json!({
                "schema_version": 1,
                "ok": true,
                "usage": ["tomo capabilities", "tomo scene validate SCENE.json",
                    "tomo scene patch SCENE.json PATCH.json", "tomo inspect SCENE.json",
                    "tomo run SCENE.json --ticks N",
                    "tomo project new NEW_DIRECTORY --name NAME", "tomo project validate DIRECTORY",
                    "tomo project inspect DIRECTORY", "tomo project edit DIRECTORY REQUEST.json",
                    "tomo project rollback DIRECTORY ORIGINAL_ID --request-id NEW_ID --expected-revision N",
                    "tomo capture SCENE.json --ticks N --output NEW.png [--view VIEW.json] [--colliders] (requires render feature)"],
                "exit_codes": {"0": "success", "1": "check_failed", "2": "input_error", "3": "runtime_or_output_error"}
            }),
            0,
        )),
        ["project", "new", directory, "--name", name] => {
            let project = tomo_project::Project::template(name)?;
            let file = tomo_project::ProjectFile::create(directory, &project)?;
            Ok((
                json!({"schema_version":1,"ok":true,"path":file.path(),"project":file.project()}),
                0,
            ))
        }
        ["project", "inspect", path] => {
            let file = tomo_project::ProjectFile::open(path)?;
            Ok((
                serde_json::to_value(file.project())
                    .map_err(|e| Problem::new("JSON", "/", e.to_string()))?,
                0,
            ))
        }
        ["project", "edit", directory, request] => {
            let request = tomo_project::journal::EditRequest::from_json(&read_document(request)?)?;
            Ok((
                json!({"schema_version":1,"ok":true,"receipt":tomo_project::journal::apply_edit(directory,&request)?}),
                0,
            ))
        }
        [
            "project",
            "rollback",
            directory,
            original,
            "--request-id",
            new_id,
            "--expected-revision",
            revision,
        ] => {
            let revision = revision
                .parse()
                .map_err(|_| Problem::new("ARGUMENT", "/revision", "expected u64 revision"))?;
            Ok((
                json!({"schema_version":1,"ok":true,"receipt":tomo_project::journal::rollback(directory,original,new_id,revision)?}),
                0,
            ))
        }
        ["project", "validate", path] => {
            let file = tomo_project::ProjectFile::open(path)?;
            Ok((
                json!({"schema_version":1,"ok":true,"project":file.project()}),
                0,
            ))
        }
        ["scene", "validate", path] => {
            let runtime = Runtime::from_scene(load_scene(path)?)?;
            Ok((
                json!({"schema_version": 1, "ok": true,
                    "name": runtime.scene().name, "revision": runtime.scene().revision,
                    "body_count": runtime.scene().bodies.len()}),
                0,
            ))
        }
        ["scene", "patch", scene_path, patch_path] => {
            let scene = load_scene(scene_path)?;
            let patch = Patch::from_json(&read_document(patch_path)?)?;
            let candidate = scene.patched(&patch)?;
            let runtime = Runtime::from_scene(candidate)?;
            // Direct scene JSON can be reused as input. Patch never overwrites scenes.
            let value = serde_json::to_value(runtime.scene())
                .map_err(|error| Problem::new("INTERNAL", "/", error.to_string()))?;
            Ok((value, 0))
        }
        ["inspect", path] => {
            let runtime = Runtime::from_scene(load_scene(path)?)?;
            Ok((
                json!({"schema_version": 1, "ok": true, "snapshot": runtime.snapshot()?}),
                0,
            ))
        }
        ["run", path, "--ticks", ticks] => {
            let ticks: u32 = ticks
                .parse()
                .map_err(|_| Problem::new("ARGUMENT", "/ticks", "expected a positive integer"))?;
            let mut runtime = Runtime::from_scene(load_scene(path)?)?;
            let report = runtime.run(ticks)?;
            let exit = u8::from(!report.passed);
            Ok((
                json!({"schema_version": 1, "ok": report.passed, "report": report}),
                exit,
            ))
        }
        #[cfg(feature = "render")]
        ["capture", path, args @ ..] => capture::dispatch(path, args),
        #[cfg(not(feature = "render"))]
        ["capture", ..] => Err(Problem::new(
            "UNSUPPORTED",
            "/capture",
            "build tomo-cli with --features render",
        )),
        _ => Err(Problem::new(
            "USAGE",
            "/arguments",
            "run tomo --help for supported commands",
        )),
    }
}

fn main() -> ExitCode {
    let args: Result<Vec<_>, _> = std::env::args_os()
        .skip(1)
        .map(|arg| {
            arg.into_string()
                .map_err(|_| Problem::new("ARGUMENT", "/arguments", "expected UTF-8 arguments"))
        })
        .collect();
    let outcome = args.and_then(|args| dispatch(&args));
    let (value, exit) = match outcome {
        Ok(result) => result,
        Err(error) => {
            let exit = if matches!(
                error.code.as_str(),
                "SIMULATION"
                    | "FAILED_WORLD"
                    | "INTERNAL"
                    | "RENDER"
                    | "GPU_UNAVAILABLE"
                    | "OUTPUT"
            ) {
                3
            } else {
                2
            };
            (
                json!({"schema_version": 1, "ok": false, "error": error}),
                exit,
            )
        }
    };
    let mut stdout = std::io::stdout().lock();
    if serde_json::to_writer(&mut stdout, &value).is_err() || stdout.write_all(b"\n").is_err() {
        eprintln!("tomo: failed to write JSON output");
        return ExitCode::from(3);
    }
    ExitCode::from(exit)
}
