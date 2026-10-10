// SPDX-License-Identifier: MIT
use tomo_render::{Camera, CaptureOptions, pick_body, plane_position, project_bodies};
use tomo_runtime::Runtime;
use tomo_scene::Scene;
#[test]
fn ray_picking_uses_shapes_and_nearest_depth_and_plane_matches_projection() {
    let mut scene =
        Scene::from_json(include_str!("../../../examples/scenes/box_run.scene.json")).unwrap();
    scene.bodies.retain(|b| b.id == "player");
    scene.bodies[0].position = [0., 0., 0.];
    let options = CaptureOptions {
        camera: Camera {
            eye: [0., 5., 10.],
            target: [0.; 3],
            ..Camera::default()
        },
        ..Default::default()
    };
    let snapshot = Runtime::from_scene(scene.clone())
        .unwrap()
        .snapshot()
        .unwrap();
    let pixel = project_bodies(&scene, &snapshot, &options).unwrap()[0]
        .center_pixel
        .unwrap();
    assert_eq!(
        pick_body(&scene, &snapshot, &options, pixel)
            .unwrap()
            .as_deref(),
        Some("player")
    );
    let plane = plane_position(&options, pixel, 0.).unwrap().unwrap();
    assert!(plane.iter().all(|v| v.abs() < 1e-4));
    assert!(
        pick_body(&scene, &snapshot, &options, [0., 0.])
            .unwrap()
            .is_none()
    );
    let mut farther = scene.bodies[0].clone();
    farther.id = "farther".into();
    farther.position = [0., -1., -2.];
    scene.bodies.push(farther);
    let snapshot = Runtime::from_scene(scene.clone())
        .unwrap()
        .snapshot()
        .unwrap();
    assert_eq!(
        pick_body(&scene, &snapshot, &options, pixel)
            .unwrap()
            .as_deref(),
        Some("player")
    );
    scene.bodies[0].rotation = [0., (0.4_f32).sin(), 0., (0.4_f32).cos()];
    let snapshot = Runtime::from_scene(scene.clone())
        .unwrap()
        .snapshot()
        .unwrap();
    assert_eq!(
        pick_body(&scene, &snapshot, &options, pixel)
            .unwrap()
            .as_deref(),
        Some("player")
    );
    scene.bodies[0].shape = tomo_scene::ShapeSpec::Sphere { radius: 0.5 };
    let snapshot = Runtime::from_scene(scene.clone())
        .unwrap()
        .snapshot()
        .unwrap();
    assert_eq!(
        pick_body(&scene, &snapshot, &options, pixel)
            .unwrap()
            .as_deref(),
        Some("player")
    );
    assert!(pick_body(&scene, &snapshot, &options, [f32::NAN, 0.]).is_err());
}
