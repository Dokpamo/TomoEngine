// SPDX-License-Identifier: MIT
use tomo_game::{Input, Status};
use tomo_project::{
    Document, Project, ProjectFile, Session,
    journal::{EditRequest, apply_edit, rollback},
};
use tomo_scene::{CameraSpec, Entity, UiAction, Visibility, Widget, WidgetKind};
fn project() -> Project {
    Project::template("Authoring test").unwrap()
}
fn temp() -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!(
        "tomo-authoring-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&p).unwrap();
    p
}
#[test]
fn no_body_entity_hierarchy_camera_and_prefab_roundtrip() {
    let mut p = project();
    let body_count = p.scene.bodies.len();
    let mut root = Entity::decoration("root", false);
    root.transform.position = [3., 1., 0.];
    let mut child = Entity::decoration("child", true);
    child.parent = Some("root".into());
    child.transform.position = [0., 1., 0.];
    p.content.entities = vec![root, child];
    p.content.cameras[0].follow = Some("child".into());
    p.capture_prefab("root", "tree").unwrap();
    p.instantiate("tree", "copy", [4., 0., 0.]).unwrap();
    assert_eq!(p.scene.bodies.len(), body_count);
    assert_eq!(p.content.entities.len(), 4);
    assert_eq!(p.content.entities[3].parent.as_deref(), Some("copy_root"));
    assert_eq!(Project::from_json(&p.to_json().unwrap()).unwrap(), p);
    let before = p.clone();
    assert!(p.instantiate("tree", "copy", [0.; 3]).is_err());
    assert_eq!(p, before);
}
#[test]
fn physics_bound_prefab_remaps_body_links_and_rejects_invalid_hierarchy_atomically() {
    let mut p = project();
    let mut entity = Entity::decoration("hero_visual", false);
    entity.body = Some("player".into());
    p.content.entities.push(entity);
    p.capture_prefab("hero_visual", "hero").unwrap();
    p.instantiate("hero", "copy", [2., 0., 0.]).unwrap();
    assert_eq!(p.content.entities[1].body.as_deref(), Some("copy_player"));
    let mut doc = Document::new(p.clone()).unwrap();
    let mut bad = p.clone();
    bad.content.entities[0].parent = Some("copy_hero_visual".into());
    assert!(doc.apply(bad, 0).is_err());
    assert_eq!(doc.project(), &p);
    assert!(!doc.can_undo());
    let mut bad = p.clone();
    bad.content.entities[0].transform.scale = f32::NAN;
    assert!(bad.validate().is_err());
    let mut bad = p.clone();
    bad.content.cameras[0].target = bad.content.cameras[0].eye;
    assert!(bad.validate().is_err());
    let mut bad = p.clone();
    bad.content.entities[0].body = Some("missing".into());
    assert!(bad.validate().is_err());
}
#[test]
fn cycle_and_unknown_fields_are_rejected() {
    let mut p = project();
    let mut a = Entity::decoration("a", false);
    let mut b = Entity::decoration("b", true);
    a.parent = Some("b".into());
    b.parent = Some("a".into());
    p.content.entities = vec![a, b];
    assert!(p.validate().is_err());
    p.content.entities.clear();
    let mut value = serde_json::to_value(&p).unwrap();
    value["content"]["run_script"] = true.into();
    assert!(Project::from_json(&value.to_string()).is_err());
    let mut p = project();
    p.content.cameras.push(CameraSpec::default());
    assert!(p.validate().is_err());
    let mut p = project();
    let mut w = Widget::label("bar");
    w.kind = WidgetKind::Progress { max_score: 0 };
    p.content.ui.push(w);
    assert!(p.validate().is_err());
}
#[test]
fn scenes_swap_preserve_edits_and_play_transition_is_isolated() {
    let mut p = project();
    p.duplicate_scene("other").unwrap();
    p.next_scene = Some("other".into());
    p.switch_scene("other").unwrap();
    p.scene.name = "Second room".into();
    p.switch_scene("main").unwrap();
    let before = p.clone();
    let mut session = Session::new(p.clone()).unwrap();
    session.next_scene().unwrap();
    assert_eq!(session.scene().name, "Second room");
    assert_eq!(p, before);
    session.load_scene("main").unwrap();
    assert_eq!(session.project(), &p);
    let mut bad = p.clone();
    bad.next_scene = Some("unknown".into());
    assert!(bad.validate().is_err());
}
#[test]
fn role_free_games_do_not_need_a_player_or_goal() {
    let mut p = project();
    p.scene.bodies.clear();
    p.game.player.clear();
    p.game.goal.clear();
    p.game.logic.rules.clear();
    p.game.logic.start = tomo_game::StartMode::Immediate;
    p.content.entities.push(Entity::decoration("clock", true));
    let mut game = p.game().unwrap();
    game.step(Input::default()).unwrap();
    assert_eq!(game.status(), Status::Playing);
    assert_eq!(game.elapsed_ticks(), 1);
    assert!(game.snapshot().bodies.is_empty());
    p.game.logic = tomo_game::Logic::default();
    assert!(p.game().is_err());
}
#[test]
fn save_rebuilds_contact_and_rule_state_and_rejects_tampering_before_replacement() {
    let p = project();
    let mut live = Session::new(p.clone()).unwrap();
    let input = Input {
        right: true,
        ..Default::default()
    };
    for _ in 0..180 {
        live.step(input).unwrap();
    }
    let text = live.save_json().unwrap();
    let mut loaded = Session::new(p.clone()).unwrap();
    loaded.restore_json(&text).unwrap();
    assert_eq!(loaded.snapshot(), live.snapshot());
    assert_eq!(loaded.score(), live.score());
    for _ in 0..120 {
        live.step(input).unwrap();
        loaded.step(input).unwrap();
        assert_eq!(loaded.snapshot(), live.snapshot());
    }
    let initial = loaded.snapshot().clone();
    let mut bad: serde_json::Value = serde_json::from_str(&text).unwrap();
    bad["checkpoint"]["score"] = 999.into();
    assert!(loaded.restore_json(&bad.to_string()).is_err());
    assert_eq!(loaded.snapshot(), &initial);
    let mut changed = p.clone();
    changed.name = "Another game".into();
    let mut other = Session::new(changed).unwrap();
    assert!(other.restore_json(&text).is_err());
}
#[test]
fn manual_start_save_and_multi_scene_progress_roundtrip() {
    let mut p = project();
    p.duplicate_scene("second").unwrap();
    p.next_scene = Some("second".into());
    let mut s = Session::new(p.clone()).unwrap();
    s.next_scene().unwrap();
    s.begin();
    for _ in 0..30 {
        s.step(Input::default()).unwrap();
    }
    let text = s.save_json().unwrap();
    let mut other = Session::new(p).unwrap();
    other.restore_json(&text).unwrap();
    assert_eq!(other.project().scene_id, "second");
    assert_eq!(s.snapshot(), other.snapshot());
    let path = temp();
    let file = path.join("slot.json");
    s.save_progress(&file).unwrap();
    other.restart().unwrap();
    other.load_progress(&file).unwrap();
    assert_eq!(s.snapshot(), other.snapshot());
    std::fs::write(&file, "unrelated file").unwrap();
    assert!(s.save_progress(&file).is_err());
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "unrelated file");
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn edit_retry_rollback_and_prepared_receipt_recovery() {
    let dir = temp();
    let path = dir.join("game");
    let p = project();
    ProjectFile::create(&path, &p).unwrap();
    let mut candidate = p.clone();
    candidate.name = "Edited".into();
    let request = EditRequest {
        schema_version: 1,
        request_id: "request_1".into(),
        expected_revision: 0,
        candidate,
    };
    let result = apply_edit(&path, &request).unwrap();
    assert!(!result.replayed);
    assert_eq!(result.applied_revision, 1);
    let result = apply_edit(&path, &request).unwrap();
    assert!(result.replayed);
    assert_eq!(ProjectFile::open(&path).unwrap().project().revision, 1);
    let mut different = request.clone();
    different.candidate.name = "Other".into();
    assert!(apply_edit(&path, &different).is_err());
    let r = rollback(&path, "request_1", "rollback_1", 1).unwrap();
    assert_eq!(r.applied_revision, 2);
    assert_eq!(ProjectFile::open(&path).unwrap().project().name, p.name);
    assert!(
        rollback(&path, "request_1", "rollback_1", 1)
            .unwrap()
            .replayed
    );
    let mut r = request.clone();
    r.request_id = "request_2".into();
    r.expected_revision = 2;
    r.candidate = ProjectFile::open(&path).unwrap().project().clone();
    r.candidate.name = "Recovered".into();
    let tmp = path.join(".tomo-journal/request_2.tmp");
    std::fs::write(&tmp, "owned by another writer").unwrap();
    assert!(apply_edit(&path, &r).is_err());
    assert_eq!(
        ProjectFile::open(&path).unwrap().project().name,
        "Recovered"
    );
    std::fs::remove_file(&tmp).unwrap();
    assert!(apply_edit(&path, &r).unwrap().replayed);
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn migration_rejects_new_fields_under_old_versions_and_creator_lab_is_executable() {
    let p = Project::from_json(include_str!(
        "../../../examples/projects/creator_lab/project.tomo.json"
    ))
    .unwrap();
    assert_eq!(p.schema_version, 3);
    assert!(p.content.ui.iter().any(|w| w.kind
        == WidgetKind::Button {
            action: UiAction::Save
        }
        && w.visibility == Visibility::Always));
    let mut value = serde_json::to_value(&p).unwrap();
    value["schema_version"] = 2.into();
    assert!(Project::from_json(&value.to_string()).is_err());
    let mut s = Session::new(p).unwrap();
    s.step(Input {
        action: true,
        ..Default::default()
    })
    .unwrap();
    assert!(s.take_sounds().iter().any(|s| s.id == "jump"));
}
