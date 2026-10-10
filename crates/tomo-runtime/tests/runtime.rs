// SPDX-License-Identifier: MIT
use tomo_runtime::Runtime;
use tomo_scene::Scene;

const FALLING: &str = include_str!("../../../examples/scenes/falling_box.scene.json");
const FAILING: &str = include_str!("../../../examples/scenes/expected_failure.scene.json");

fn runtime() -> Runtime {
    Runtime::from_scene(Scene::from_json(FALLING).unwrap()).unwrap()
}

#[test]
fn falling_scene_is_computed_and_checks_pass() {
    let mut runtime = runtime();
    let initial = runtime.snapshot().unwrap();
    assert_eq!(initial.tick, 0);
    assert_eq!(initial.bodies[0].id, "box");
    assert_eq!(initial.bodies[0].position[1], 5.0);
    let report = runtime.run(600).unwrap();
    assert!(report.passed, "{report:?}");
    assert_eq!(report.checks.len(), 2);
    assert_eq!(report.snapshot.tick, 600);
    assert!((report.snapshot.simulation_time - 10.0).abs() < 1.0e-5);
    assert!((report.snapshot.bodies[0].position[1] - 0.5).abs() < 0.05);
    assert_eq!(report.snapshot.bodies[1].position[1], -0.5);
}

#[test]
fn missing_floor_is_a_real_failed_check_not_a_success_stub() {
    let mut runtime = Runtime::from_scene(Scene::from_json(FAILING).unwrap()).unwrap();
    let report = runtime.run(120).unwrap();
    assert!(!report.passed);
    assert!(report.snapshot.bodies[0].position[1] < 0.0);
}

#[test]
fn replay_is_equal_in_the_same_process_and_feature_mode() {
    let mut first = runtime();
    let mut second = runtime();
    assert_eq!(
        first.run(600).unwrap().snapshot,
        second.run(600).unwrap().snapshot
    );
}

#[test]
fn body_input_order_does_not_change_build_or_simulation_order() {
    let first = Scene::from_json(FALLING).unwrap();
    let mut second = first.clone();
    second.bodies.reverse();
    let mut first = Runtime::from_scene(first).unwrap();
    let mut second = Runtime::from_scene(second).unwrap();
    assert_eq!(
        first.run(600).unwrap().snapshot,
        second.run(600).unwrap().snapshot
    );
}

#[test]
fn invalid_budget_does_not_advance_state() {
    let mut runtime = runtime();
    let initial = runtime.snapshot().unwrap();
    for ticks in [0, u32::MAX, tomo_runtime::MAX_TICKS + 1] {
        assert_eq!(runtime.run(ticks).unwrap_err().code, "LIMIT");
        assert_eq!(runtime.snapshot().unwrap(), initial);
    }
    assert_eq!(runtime.run(1).unwrap().snapshot.tick, 1);
}

#[test]
fn split_runs_use_the_same_fixed_tick_state() {
    let mut first = runtime();
    let mut second = runtime();
    first.run(300).unwrap();
    assert_eq!(
        first.run(300).unwrap().snapshot,
        second.run(600).unwrap().snapshot
    );
}

#[test]
fn reloading_is_an_explicit_new_world_not_a_physics_checkpoint() {
    let mut used = runtime();
    used.run(120).unwrap();
    let rebuilt = Runtime::from_scene(used.scene().clone()).unwrap();
    assert_eq!(rebuilt.snapshot().unwrap().tick, 0);
    assert_eq!(rebuilt.snapshot().unwrap().bodies[0].position[1], 5.0);
}
