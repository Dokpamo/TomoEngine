// SPDX-License-Identifier: MIT
use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;

fn fixture(path: &str) -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(path)
        .to_str()
        .unwrap()
        .to_owned()
}

fn invoke(args: &[&str]) -> (i32, Value) {
    let output = Command::new(env!("CARGO_BIN_EXE_tomo"))
        .args(args)
        .output()
        .unwrap();
    assert!(output.stderr.is_empty(), "stderr={:?}", output.stderr);
    let value = serde_json::from_slice(&output.stdout).unwrap();
    (output.status.code().unwrap(), value)
}

#[test]
fn capabilities_are_explicit_about_missing_features() {
    let (code, value) = invoke(&["capabilities"]);
    assert_eq!(code, 0);
    assert_eq!(value["rendering"], cfg!(feature = "render"));
    assert_eq!(value["jolt_complete"], false);
    assert_eq!(value["autonomous_agent"], false);
}

#[test]
fn validation_and_inspection_work_without_advancing_time() {
    let path = fixture("scenes/falling_box.scene.json");
    let (code, value) = invoke(&["scene", "validate", &path]);
    assert_eq!(code, 0);
    assert_eq!(value["body_count"], 2);
    let (code, value) = invoke(&["inspect", &path]);
    assert_eq!(code, 0);
    assert_eq!(value["snapshot"]["tick"], 0);
}

#[test]
fn run_is_a_real_physics_process_with_structured_checks() {
    let path = fixture("scenes/falling_box.scene.json");
    let (code, value) = invoke(&["run", &path, "--ticks", "600"]);
    assert_eq!(code, 0, "{value}");
    assert_eq!(value["ok"], true);
    assert_eq!(value["report"]["snapshot"]["tick"], 600);
    assert_eq!(value["report"]["checks"].as_array().unwrap().len(), 2);
}

#[test]
fn failed_checks_have_exit_one_not_input_error_or_success() {
    let path = fixture("scenes/expected_failure.scene.json");
    let (code, value) = invoke(&["run", &path, "--ticks", "120"]);
    assert_eq!(code, 1);
    assert_eq!(value["ok"], false);
    assert_eq!(value["report"]["checks"][0]["passed"], false);
}

#[test]
fn patch_returns_reusable_scene_and_never_overwrites_input() {
    let path = fixture("scenes/falling_box.scene.json");
    let patch = fixture("patches/raise_box.patch.json");
    let before = std::fs::read(&path).unwrap();
    let (code, value) = invoke(&["scene", "patch", &path, &patch]);
    assert_eq!(code, 0, "{value}");
    assert_eq!(value["revision"], 1);
    let modified = tomo_scene::Scene::from_json(&value.to_string()).unwrap();
    assert_eq!(
        modified
            .bodies
            .iter()
            .find(|body| body.id == "box")
            .unwrap()
            .position[1],
        8.0
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn unknown_command_missing_file_and_invalid_tick_fail_structurally() {
    assert_eq!(invoke(&["build", "--target", "ios"]).0, 2);
    assert_eq!(
        invoke(&["scene", "validate", "__tomo_missing_scene__.json"]).0,
        2
    );
    let path = fixture("scenes/falling_box.scene.json");
    for ticks in ["NaN", "-1", "0", "999999"] {
        let (code, value) = invoke(&["run", &path, "--ticks", ticks]);
        assert_eq!(code, 2, "{ticks}: {value}");
        assert!(value["error"]["code"].is_string());
    }
}

#[test]
#[cfg(not(feature = "render"))]
fn capture_without_renderer_reports_unsupported() {
    let (code, value) = invoke(&[
        "capture",
        "missing.json",
        "--ticks",
        "0",
        "--output",
        "new.png",
    ]);
    assert_eq!(code, 2);
    assert_eq!(value["error"]["code"], "UNSUPPORTED");
}

#[test]
#[cfg(feature = "render")]
fn capture_rejects_bad_arguments_before_gpu_work() {
    let path = fixture("scenes/falling_box.scene.json");
    for args in [
        vec!["capture", &path],
        vec!["capture", &path, "--ticks", "-1", "--output", "new.png"],
        vec![
            "capture", &path, "--ticks", "0", "--ticks", "1", "--output", "new.png",
        ],
        vec!["capture", &path, "--ticks", "0", "--output", &path],
    ] {
        let (code, value) = invoke(&args);
        assert_eq!(code, 2, "{value}");
        assert_eq!(value["error"]["code"], "ARGUMENT");
    }
}

#[test]
#[cfg(feature = "render")]
#[ignore = "requires a real hardware or software GPU adapter"]
fn capture_process_preserves_checks_snapshot_and_existing_files() {
    let directory = std::env::temp_dir().join(format!(
        "tomo-capture-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&directory).unwrap();
    let image = directory.join("frame.png");
    let output = image.to_str().unwrap();
    let scene = fixture("scenes/falling_box.scene.json");
    let (_, numeric) = invoke(&["run", &scene, "--ticks", "600"]);
    let (code, rendered) = invoke(&["capture", &scene, "--ticks", "600", "--output", output]);
    assert_eq!(code, 0, "{rendered}");
    assert_eq!(rendered["snapshot"], numeric["report"]["snapshot"]);
    assert_eq!(rendered["capture"]["tick"], 600);
    let bytes = std::fs::read(&image).unwrap();
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(
        invoke(&["capture", &scene, "--ticks", "0", "--output", output]).0,
        2
    );
    assert_eq!(std::fs::read(&image).unwrap(), bytes);
    let failed = directory.join("failed.png");
    let failing_scene = fixture("scenes/expected_failure.scene.json");
    let (code, value) = invoke(&[
        "capture",
        &failing_scene,
        "--ticks",
        "120",
        "--output",
        failed.to_str().unwrap(),
    ]);
    assert_eq!(code, 1, "{value}");
    assert!(failed.is_file());
    assert_eq!(value["report"]["passed"], false);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn project_creation_validation_and_existing_folder_protection() {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory =
        std::env::temp_dir().join(format!("tomo-cli-project-{}-{nonce}", std::process::id()));
    let path = directory.to_str().unwrap();
    let (code, value) = invoke(&["project", "new", path, "--name", "New Game"]);
    assert_eq!(code, 0, "{value}");
    assert_eq!(value["project"]["name"], "New Game");
    let file = directory.join(tomo_project::PROJECT_FILE);
    let before = std::fs::read(&file).unwrap();
    let (code, value) = invoke(&["project", "validate", path]);
    assert_eq!(code, 0, "{value}");
    assert_eq!(value["project"]["game"]["player"], "player");
    let (code, _) = invoke(&["project", "new", path, "--name", "Other"]);
    assert_ne!(code, 0);
    assert_eq!(std::fs::read(&file).unwrap(), before);
    let mut project: Value = serde_json::from_slice(&before).unwrap();
    project["game"]["goal"] = serde_json::json!("missing");
    std::fs::write(&file, project.to_string()).unwrap();
    let (code, value) = invoke(&["project", "validate", path]);
    assert_eq!(code, 2);
    assert_eq!(value["error"]["code"], "GAME_RULES");
    std::fs::remove_dir_all(directory).unwrap();
}
