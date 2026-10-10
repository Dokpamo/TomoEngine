// SPDX-License-Identifier: MIT
use serde_json::Value;
use std::{path::PathBuf, process::Command};

fn invoke(args: &[&str]) -> (i32, Value) {
    let output = Command::new(env!("CARGO_BIN_EXE_tomo-play"))
        .args(args)
        .output()
        .unwrap();
    let value = serde_json::from_slice(&output.stdout).expect("structured stdout");
    (output.status.code().unwrap(), value)
}
fn replay_path() -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/replays/box_run_win.replay.json")
        .to_string_lossy()
        .into()
}

#[test]
fn process_replay_wins_without_window_and_bad_input_exits_two() {
    let (code, value) = invoke(&["--replay", &replay_path()]);
    assert_eq!(code, 0, "{value}");
    assert_eq!(value["passed"], true);
    assert_eq!(value["status"], "won");
    assert!(value["snapshot"]["tick"].as_u64().unwrap() > 400);
    let (code, value) = invoke(&["--unknown"]);
    assert_eq!(code, 2);
    assert_eq!(value["error"]["code"], "ARGUMENT");
}

#[test]
fn incomplete_route_fails_expectation_instead_of_reporting_success() {
    let dir = std::env::temp_dir().join(format!("tomo-play-fail-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("fail.json");
    std::fs::write(
        &path,
        r#"{"schema_version":1,"segments":[{"ticks":120,"input":{"up":true}}],"expect_win":true}"#,
    )
    .unwrap();
    let (code, value) = invoke(&["--replay", path.to_str().unwrap()]);
    std::fs::remove_dir_all(dir).unwrap();
    assert_eq!(code, 1, "{value}");
    assert_eq!(value["passed"], false);
    assert_eq!(value["status"], "playing");
}

#[test]
#[ignore = "requires a real GPU adapter; run explicitly in GPU CI"]
fn game_capture_and_existing_output_protection() {
    let path = std::env::temp_dir().join(format!("tomo-play-gpu-{}.png", std::process::id()));
    let args = [
        "--replay",
        &replay_path(),
        "--capture",
        path.to_str().unwrap(),
    ];
    let (code, value) = invoke(&args);
    assert_eq!(code, 0, "{value}");
    let before = std::fs::read(&path).unwrap();
    assert!(before.starts_with(b"\x89PNG\r\n\x1a\n"));
    assert_eq!(
        value["capture"]["metadata"]["tick"],
        value["snapshot"]["tick"]
    );
    let (code, value) = invoke(&args);
    assert_eq!(code, 3, "{value}");
    assert_eq!(value["error"]["code"], "OUTPUT");
    assert_eq!(std::fs::read(&path).unwrap(), before);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn saved_custom_project_runs_with_its_own_roles_and_goal() {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory =
        std::env::temp_dir().join(format!("tomo-custom-play-{}-{nonce}", std::process::id()));
    let mut project = tomo_project::Project::template("Custom Game").unwrap();
    for body in &mut project.scene.bodies {
        if body.id == "player" {
            body.id = "hero".into();
        }
        if body.id == "goal" {
            body.id = "finish".into();
            body.position = [-5., 0.035, 4.];
        }
    }
    project.game.player = "hero".into();
    project.game.goal = "finish".into();
    project.game.speed = 1.;
    tomo_project::ProjectFile::create(&directory, &project).unwrap();
    let replay = directory.join("input.json");
    std::fs::write(
        &replay,
        r#"{"schema_version":1,"segments":[{"ticks":1,"input":{"right":true}}],"expect_win":true}"#,
    )
    .unwrap();
    let (code, value) = invoke(&[
        "--project",
        directory.to_str().unwrap(),
        "--replay",
        replay.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{value}");
    assert_eq!(value["status"], "won");
    assert_eq!(value["snapshot"]["tick"], 1);
    assert!(
        value["snapshot"]["bodies"]
            .as_array()
            .unwrap()
            .iter()
            .any(|b| b["id"] == "hero")
    );
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn authored_logic_project_uses_space_contact_score_timeout_and_goal_in_the_real_player() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let project = root.join("projects/logic_lab");
    let replay = root.join("replays/logic_lab_timeout.replay.json");
    let (code, value) = invoke(&[
        "--project",
        project.to_str().unwrap(),
        "--replay",
        replay.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{value}");
    assert_eq!(value["status"], "lost");
    assert_eq!(value["score"], 1);
    assert_eq!(value["snapshot"]["tick"], 1800);
    assert_eq!(
        value["rule_events"].as_array().unwrap().last().unwrap()["rule_id"],
        "timeout"
    );
    let (code, value) = invoke(&[
        "--project",
        project.to_str().unwrap(),
        "--replay",
        &replay_path(),
    ]);
    assert_eq!(code, 0, "{value}");
    assert_eq!(value["status"], "won");
    assert_eq!(value["score"], 1);
    assert_eq!(value["snapshot"]["tick"], 514);
}
