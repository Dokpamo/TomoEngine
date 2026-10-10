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
