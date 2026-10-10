// SPDX-License-Identifier: MIT
use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;

fn fixture(path: &str) -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples").join(path).to_str().unwrap().to_owned()
}

fn invoke(args: &[&str]) -> (i32, Value) {
    let output = Command::new(env!("CARGO_BIN_EXE_tomo")).args(args).output().unwrap();
    assert!(output.stderr.is_empty(), "stderr={:?}", output.stderr);
    let value = serde_json::from_slice(&output.stdout).unwrap();
    (output.status.code().unwrap(), value)
}

#[test]
fn capabilities_are_explicit_about_missing_features() {
    let (code, value) = invoke(&["capabilities"]);
    assert_eq!(code, 0);
    assert_eq!(value["rendering"], false);
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
    assert_eq!(modified.bodies.iter().find(|body| body.id == "box").unwrap().position[1], 8.0);
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn unknown_command_missing_file_and_invalid_tick_fail_structurally() {
    assert_eq!(invoke(&["build", "--target", "ios"]).0, 2);
    assert_eq!(invoke(&["scene", "validate", "__tomo_missing_scene__.json"]).0, 2);
    let path = fixture("scenes/falling_box.scene.json");
    for ticks in ["NaN", "-1", "0", "999999"] {
        let (code, value) = invoke(&["run", &path, "--ticks", ticks]);
        assert_eq!(code, 2, "{ticks}: {value}");
        assert!(value["error"]["code"].is_string());
    }
}
