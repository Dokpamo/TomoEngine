// SPDX-License-Identifier: MIT
use std::{fs, path::PathBuf};
use tomo_game::{Input, Replay, Status};
use tomo_project::{Document, Project, ProjectFile};

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("tomo-project-{}-{nonce}", std::process::id()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn child(&self) -> PathBuf {
        self.0.join("game")
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn template() -> Project {
    Project::template("Test game").unwrap()
}

#[test]
fn version_fields_references_numbers_and_file_size_are_strict() {
    let original = template();
    let json = original.to_json().unwrap();
    assert_eq!(Project::from_json(&json).unwrap(), original);
    for path in ["schema_version", "unknown"] {
        let mut value = serde_json::to_value(&original).unwrap();
        value[path] = serde_json::json!(99);
        assert!(Project::from_json(&value.to_string()).is_err());
    }
    let mut p = original.clone();
    p.game.player = "absent".into();
    assert!(p.validate().is_err());
    p = original.clone();
    p.game.speed = f32::NAN;
    assert!(p.validate().is_err());
    p = original.clone();
    p.scene.bodies[0].position[0] = f32::INFINITY;
    assert!(p.validate().is_err());
    let mut value = serde_json::to_value(&original).unwrap();
    value["game"]["execute"] = serde_json::json!("anything");
    assert!(Project::from_json(&value.to_string()).is_err());
    assert!(Project::from_json(&" ".repeat(tomo_scene::MAX_DOCUMENT_BYTES + 1)).is_err());
}
#[test]
fn edits_validate_before_mutation_conflict_and_overflow_do_not_change_history() {
    let original = template();
    let mut document = Document::new(original.clone()).unwrap();
    let snapshot = document.snapshot().clone();
    let mut candidate = original.clone();
    candidate.scene.bodies.clear();
    assert!(document.apply(candidate, 0).is_err());
    assert!(!document.can_undo());
    assert_eq!(document.project(), &original);
    assert_eq!(document.snapshot(), &snapshot);
    let mut candidate = original.clone();
    candidate.name = "new".into();
    assert!(document.apply(candidate, 42).is_err());
    let mut full = original.clone();
    full.revision = u64::MAX;
    let mut document = Document::new(full.clone()).unwrap();
    let mut next = full.clone();
    next.name = "new".into();
    assert!(document.apply(next, u64::MAX).is_err());
    assert_eq!(document.project(), &full);
}
#[test]
fn drag_is_one_undo_and_redo_keeps_revisions_monotonic() {
    let mut doc = Document::new(template()).unwrap();
    let original = doc.project().scene.bodies.clone();
    doc.begin_gesture();
    for _ in 0..10 {
        let mut c = doc.project().clone();
        c.scene.bodies[0].position[0] += 0.25;
        doc.apply(c, doc.project().revision).unwrap();
    }
    doc.end_gesture();
    let moved = doc.project().scene.bodies.clone();
    let revision = doc.project().revision;
    assert!(doc.undo().unwrap());
    assert_eq!(doc.project().scene.bodies, original);
    assert_eq!(doc.project().revision, revision + 1);
    assert!(!doc.can_undo());
    assert!(doc.redo().unwrap());
    assert_eq!(doc.project().scene.bodies, moved);
    assert_eq!(doc.project().revision, revision + 2);
    doc.undo().unwrap();
    doc.add(true).unwrap();
    assert!(!doc.can_redo());
}
#[test]
fn bodies_have_unique_ids_and_referenced_game_roles_are_protected() {
    let mut doc = Document::new(template()).unwrap();
    let id = doc.add(false).unwrap();
    let copy = doc.duplicate(&id).unwrap();
    assert_ne!(id, copy);
    doc.remove(&id).unwrap();
    doc.undo().unwrap();
    assert!(doc.project().scene.bodies.iter().any(|b| b.id == id));
    let before = doc.project().clone();
    assert!(doc.remove("player").is_err());
    assert!(doc.remove("goal").is_err());
    assert_eq!(doc.project(), &before);
}
#[test]
fn save_roundtrip_and_external_same_revision_edits_are_not_overwritten() {
    let tmp = Temp::new();
    let original = template();
    let mut file = ProjectFile::create(tmp.child(), &original).unwrap();
    let mut doc = Document::new(original.clone()).unwrap();
    doc.add(true).unwrap();
    assert!(file.is_dirty(doc.project()));
    file.save(doc.project()).unwrap();
    assert!(!file.is_dirty(doc.project()));
    assert_eq!(
        ProjectFile::open(tmp.child()).unwrap().project(),
        doc.project()
    );
    let changed = format!("{} ", fs::read_to_string(file.path()).unwrap());
    fs::write(file.path(), &changed).unwrap();
    assert_eq!(file.save(doc.project()).unwrap_err().code, "CONFLICT");
    assert_eq!(fs::read_to_string(file.path()).unwrap(), changed);
    assert!(!file.path().with_extension("json.lock").exists());
    assert!(ProjectFile::create(tmp.child(), &original).is_err());
}
#[test]
fn busy_or_stale_temp_and_missing_original_leave_saved_bytes_untouched() {
    let tmp = Temp::new();
    let mut file = ProjectFile::create(tmp.child(), &template()).unwrap();
    let original = fs::read(file.path()).unwrap();
    let lock = file.path().with_extension("json.lock");
    fs::write(&lock, "another writer").unwrap();
    assert_eq!(file.save(&template()).unwrap_err().code, "SAVE_LOCK");
    assert_eq!(fs::read(&lock).unwrap(), b"another writer");
    fs::remove_file(lock).unwrap();
    let temp = file.path().with_extension("json.tmp");
    fs::write(&temp, "recovery data").unwrap();
    assert!(file.save(&template()).is_err());
    assert_eq!(fs::read(file.path()).unwrap(), original);
    assert_eq!(fs::read(temp).unwrap(), b"recovery data");
    fs::remove_file(file.path()).unwrap();
    assert!(file.save(&template()).is_err());
    assert!(!file.path().exists());
}
#[cfg(unix)]
#[test]
fn symlink_project_is_rejected() {
    let tmp = Temp::new();
    let file = ProjectFile::create(tmp.child(), &template()).unwrap();
    let link = tmp.0.join(tomo_project::PROJECT_FILE);
    std::os::unix::fs::symlink(file.path(), &link).unwrap();
    assert!(ProjectFile::open(link).is_err());
}
#[test]
fn configurable_game_uses_new_ids_goal_position_speed_and_restart_document() {
    let mut project = template();
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
    project.game.speed = 2.;
    let mut game = project.game().unwrap();
    let initial = game.snapshot().clone();
    game.step(Input {
        right: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(game.status(), Status::Won);
    game.restart().unwrap();
    assert_eq!(game.snapshot(), &initial);
    assert_eq!(game.rules(), &project.game);
}
#[test]
fn project_template_keeps_box_run_replay_and_bounds_large_project_replay_work() {
    let replay = Replay::from_json(include_str!(
        "../../../examples/replays/box_run_win.replay.json"
    ))
    .unwrap();
    let mut game = template().game().unwrap();
    assert!(replay.run(&mut game).unwrap());
    let mut p = template();
    let body = p.scene.bodies[0].clone();
    for i in 0..110 {
        let mut b = body.clone();
        b.id = format!("extra_{i}");
        p.scene.bodies.push(b);
    }
    let mut game = p.game().unwrap();
    let initial = game.snapshot().clone();
    assert_eq!(replay.run(&mut game).unwrap_err().code, "LIMIT");
    assert_eq!(game.snapshot(), &initial);
}

#[test]
fn legacy_files_migrate_explicitly_in_memory_and_only_save_as_v3_when_requested() {
    let tmp = Temp::new();
    let old = template();
    let file = ProjectFile::create(tmp.child(), &old).unwrap();
    let mut value = serde_json::to_value(&old).unwrap();
    for key in ["scene_id", "content", "next_scene", "scenes", "prefabs"] {
        value.as_object_mut().unwrap().remove(key);
    }
    value["schema_version"] = serde_json::json!(1);
    value["game"].as_object_mut().unwrap().remove("logic");
    let legacy = value.to_string();
    fs::write(file.path(), &legacy).unwrap();
    let mut file = ProjectFile::open(tmp.child()).unwrap();
    assert_eq!(file.project().schema_version, 3);
    assert_eq!(file.project().revision, old.revision);
    assert!(!file.is_dirty(file.project()));
    assert_eq!(fs::read_to_string(file.path()).unwrap(), legacy);
    let migrated = file.project().clone();
    let mut game = migrated.game().unwrap();
    let replay = Replay::from_json(include_str!(
        "../../../examples/replays/box_run_win.replay.json"
    ))
    .unwrap();
    assert!(replay.run(&mut game).unwrap());
    assert_eq!(game.elapsed_ticks(), 514);
    file.save(&migrated).unwrap();
    let saved: serde_json::Value = serde_json::from_slice(&fs::read(file.path()).unwrap()).unwrap();
    assert_eq!(saved["schema_version"], 3);
    assert_eq!(saved["game"]["logic"]["schema_version"], 1);
    value["game"]["logic"] = serde_json::to_value(tomo_game::Logic::default()).unwrap();
    assert!(
        Project::from_json(&value.to_string()).is_err(),
        "v1 must reject a v2-only field"
    );
    value["schema_version"] = serde_json::json!(2);
    value["game"].as_object_mut().unwrap().remove("logic");
    assert!(
        Project::from_json(&value.to_string()).is_err(),
        "v2 must not silently restore template rules"
    );
}

#[test]
fn rule_references_and_invalid_logic_keep_document_history_and_world_unchanged() {
    let mut project = template();
    project.game.logic.rules.push(tomo_game::Rule {
        id: "wall".into(),
        name: "Wall collision".into(),
        enabled: false,
        once: true,
        trigger: tomo_game::Trigger::Contact {
            body_a: "player".into(),
            body_b: "gate_a".into(),
            kind: tomo_game::ContactKind::Started,
        },
        conditions: vec![],
        actions: vec![tomo_game::Action::Lose {}],
    });
    let mut doc = Document::new(project.clone()).unwrap();
    let snapshot = doc.snapshot().clone();
    assert!(doc.remove("gate_a").is_err());
    let mut invalid = project.clone();
    invalid.game.logic.rules[0].actions.clear();
    assert!(doc.apply(invalid, 0).is_err());
    assert_eq!(doc.project(), &project);
    assert_eq!(doc.snapshot(), &snapshot);
    assert!(!doc.can_undo());
    let mut valid = project;
    valid.game.logic.rules.pop();
    doc.apply(valid, 0).unwrap();
    doc.remove("gate_a").unwrap();
}
