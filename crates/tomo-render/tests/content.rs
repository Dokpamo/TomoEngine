// SPDX-License-Identifier: MIT
use tomo_render::{CaptureOptions, FrameStyle, HudState};
use tomo_runtime::Runtime;
use tomo_scene::{Content, Entity, Scene, UiAction, Visibility, Widget, WidgetKind};
#[test]
fn hierarchy_body_follow_spin_and_ui_share_the_same_runtime_frame() {
    let scene =
        Scene::from_json(include_str!("../../../examples/scenes/box_run.scene.json")).unwrap();
    let snapshot = Runtime::from_scene(scene).unwrap().snapshot().unwrap();
    let mut parent = Entity::decoration("parent", false);
    parent.body = Some("player".into());
    parent.spin_degrees_per_second = [0., 90., 0.];
    let mut child = Entity::decoration("child", true);
    child.parent = Some("parent".into());
    child.transform.position = [1., 0., 0.];
    let mut c = Content {
        entities: vec![child, parent],
        ..Default::default()
    };
    c.cameras[0].follow = Some("child".into());
    let transforms = tomo_render::entity_transforms(&c, &snapshot, 1.).unwrap();
    let p = &transforms["parent"].position;
    let q = &transforms["child"].position;
    assert!((p[0] - q[0]).abs() < 0.0001);
    assert!((p[2] - 1. - q[2]).abs() < 0.0001);
    let camera = tomo_render::content_camera(&c, &snapshot, 1.).unwrap();
    assert_eq!(camera.target, *q);
    let mut style = FrameStyle::default();
    tomo_render::content_visuals(&c, &snapshot, 1., &mut style).unwrap();
    assert_eq!(style.visuals.len(), 2);
    assert!(style.hidden_bodies.contains("player"));
    let mut button = Widget::label("save");
    button.kind = WidgetKind::Button {
        action: UiAction::Save,
    };
    button.visibility = Visibility::Paused;
    c.ui.push(button);
    let mut state = HudState {
        score: 3,
        seconds: 2.,
        status: Visibility::Playing,
        paused: false,
    };
    assert_eq!(tomo_render::hud_action(&c, state, [0.1, 0.05]), None);
    state.paused = true;
    assert_eq!(
        tomo_render::hud_action(&c, state, [0.1, 0.05]),
        Some(UiAction::Save)
    );
    tomo_render::content_hud(&c, state, &CaptureOptions::default(), &mut style);
    assert_eq!(style.text.len(), 1);
}
#[test]
#[ignore = "requires real GPU adapter"]
fn visual_only_entity_changes_real_pixels_without_a_physics_body() {
    let scene =
        Scene::from_json(include_str!("../../../examples/scenes/box_run.scene.json")).unwrap();
    let snapshot = Runtime::from_scene(scene.clone())
        .unwrap()
        .snapshot()
        .unwrap();
    let view = CaptureOptions {
        width: 320,
        height: 240,
        ..Default::default()
    };
    let base = tomo_render::capture(&scene, &snapshot, &view).unwrap();
    let mut e = Entity::decoration("visual", true);
    e.transform.position = [0., 3., 0.];
    e.transform.scale = 3.;
    e.visual.as_mut().unwrap().color = [0., 1., 0.];
    let c = Content {
        entities: vec![e],
        ..Default::default()
    };
    let mut style = FrameStyle::default();
    tomo_render::content_visuals(&c, &snapshot, 0., &mut style).unwrap();
    let decorated = tomo_render::capture_styled(&scene, &snapshot, &view, &style).unwrap();
    assert_ne!(base.rgba, decorated.rgba);
    assert_eq!(snapshot.bodies.len(), scene.bodies.len());
}
