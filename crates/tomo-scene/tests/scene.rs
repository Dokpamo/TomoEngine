// SPDX-License-Identifier: MIT
use tomo_scene::{Operation, Patch, Scene};

const INPUT: &str = include_str!("../../../examples/scenes/falling_box.scene.json");

fn scene() -> Scene {
    Scene::from_json(INPUT).unwrap()
}

#[test]
fn typed_roundtrip_preserves_scene() {
    let original = scene();
    let encoded = serde_json::to_string(&original).unwrap();
    assert_eq!(Scene::from_json(&encoded).unwrap(), original);
}

#[test]
fn unknown_and_duplicate_fields_are_rejected() {
    let unknown = INPUT.replacen(
        "\"schema_version\"",
        "\"typo\": true, \"schema_version\"",
        1,
    );
    assert_eq!(Scene::from_json(&unknown).unwrap_err().code, "JSON");
    let duplicate = INPUT.replacen("\"revision\": 0", "\"revision\": 0, \"revision\": 1", 1);
    assert_eq!(Scene::from_json(&duplicate).unwrap_err().code, "JSON");
}

#[test]
fn unknown_shape_fields_are_rejected() {
    let input = INPUT.replacen("\"half_extents\"", "\"typo\": 1, \"half_extents\"", 1);
    assert_eq!(Scene::from_json(&input).unwrap_err().code, "JSON");
}

#[test]
fn versions_ids_and_references_are_checked() {
    let mut invalid = scene();
    invalid.schema_version = 2;
    assert_eq!(invalid.validate().unwrap_err().code, "VERSION");
    let mut invalid = scene();
    invalid.bodies.push(invalid.bodies[0].clone());
    assert_eq!(invalid.validate().unwrap_err().code, "DUPLICATE_ID");
    let mut invalid = scene();
    invalid.bodies.retain(|body| body.id != "box");
    assert_eq!(invalid.validate().unwrap_err().code, "MISSING_BODY");
}

#[test]
fn nonfinite_zero_rotation_and_bad_dt_are_rejected() {
    for value in [f32::NAN, f32::INFINITY, 0.0, 1.0e-6, 0.3] {
        let mut invalid = scene();
        invalid.fixed_dt = value;
        assert!(invalid.validate().is_err());
    }
    let mut invalid = scene();
    invalid.bodies[0].rotation = [0.0; 4];
    assert_eq!(invalid.validate().unwrap_err().code, "ROTATION");
    invalid.bodies[0].rotation = [0.0, 0.0, 0.0, 1.0];
    invalid.bodies[0].position[0] = f32::NAN;
    assert!(invalid.validate().is_err());
}

#[test]
fn failed_patch_leaves_original_unchanged() {
    let original = scene();
    let before = original.clone();
    let patch = Patch {
        schema_version: 1,
        expected_revision: 0,
        operations: vec![
            Operation::SetMaterial {
                id: "box".into(),
                friction: 0.8,
                restitution: 0.1,
            },
            Operation::Remove {
                id: "missing".into(),
            },
        ],
    };
    assert!(original.patched(&patch).is_err());
    assert_eq!(original, before);
}

#[test]
fn successful_patch_increments_revision_and_rejects_stale_retry() {
    let original = scene();
    let patch = Patch::from_json(include_str!(
        "../../../examples/patches/raise_box.patch.json"
    ))
    .unwrap();
    let modified = original.patched(&patch).unwrap();
    assert_eq!(modified.revision, 1);
    assert_eq!(
        modified
            .bodies
            .iter()
            .find(|b| b.id == "box")
            .unwrap()
            .position[1],
        8.0
    );
    assert_eq!(original.revision, 0);
    assert_eq!(
        modified.patched(&patch).unwrap_err().code,
        "REVISION_CONFLICT"
    );
}

#[test]
fn invalid_final_candidate_does_not_escape() {
    let original = scene();
    let patch = Patch {
        schema_version: 1,
        expected_revision: 0,
        operations: vec![Operation::SetMaterial {
            id: "box".into(),
            friction: -1.0,
            restitution: 0.0,
        }],
    };
    assert_eq!(original.patched(&patch).unwrap_err().code, "RANGE");
}

#[test]
fn document_and_object_limits_are_enforced() {
    let oversized = " ".repeat(tomo_scene::MAX_DOCUMENT_BYTES + 1);
    assert_eq!(Scene::from_json(&oversized).unwrap_err().code, "LIMIT");
    let mut invalid = scene();
    invalid.bodies = vec![invalid.bodies[0].clone(); tomo_scene::MAX_BODIES + 1];
    assert_eq!(invalid.validate().unwrap_err().code, "LIMIT");
}

#[test]
fn revision_overflow_is_an_error() {
    let mut original = scene();
    original.revision = u64::MAX;
    let patch = Patch {
        schema_version: 1,
        expected_revision: u64::MAX,
        operations: vec![Operation::Remove { id: "floor".into() }],
    };
    assert_eq!(original.patched(&patch).unwrap_err().code, "REVISION_LIMIT");
}

#[test]
fn static_velocity_and_dynamic_speed_caps_are_explicit() {
    let mut invalid = scene();
    invalid.bodies[0].linear_velocity = [1.0, 0.0, 0.0];
    assert_eq!(invalid.validate().unwrap_err().code, "STATIC_VELOCITY");
    let mut invalid = scene();
    invalid.bodies[1].linear_velocity = [400.0, 400.0, 0.0];
    assert_eq!(invalid.validate().unwrap_err().code, "RANGE");
}
