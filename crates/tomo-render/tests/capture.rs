// SPDX-License-Identifier: MIT
use tomo_render::{Camera, CaptureOptions, capture, project_bodies};
use tomo_runtime::Runtime;
use tomo_scene::Scene;

fn runtime() -> Runtime {
    Runtime::from_scene(
        Scene::from_json(include_str!(
            "../../../examples/scenes/falling_box.scene.json"
        ))
        .unwrap(),
    )
    .unwrap()
}

#[test]
fn metadata_uses_the_simulated_pose_and_preserves_world() {
    let mut runtime = runtime();
    runtime.run(600).unwrap();
    let snapshot = runtime.snapshot().unwrap();
    let bodies = project_bodies(runtime.scene(), &snapshot, &CaptureOptions::default()).unwrap();
    let projected = bodies.iter().find(|b| b.id == "box").unwrap();
    let actual = snapshot.bodies.iter().find(|b| b.id == "box").unwrap();
    assert_eq!(projected.position, actual.position);
    assert_eq!(projected.rotation, actual.rotation);
    assert!(projected.position[1] < 0.55);
    assert!(projected.center_pixel.is_some());
    assert_eq!(runtime.snapshot().unwrap(), snapshot);
}

#[test]
fn invalid_camera_and_dimensions_are_rejected_without_gpu() {
    let runtime = runtime();
    let snapshot = runtime.snapshot().unwrap();
    let defaults = CaptureOptions::default();
    let mut cases = vec![
        CaptureOptions {
            width: 0,
            ..defaults.clone()
        },
        CaptureOptions {
            height: 2049,
            ..defaults.clone()
        },
        CaptureOptions {
            schema_version: 2,
            ..defaults.clone()
        },
    ];
    for camera in [
        Camera {
            eye: defaults.camera.target,
            ..defaults.camera.clone()
        },
        Camera {
            up: [0.0; 3],
            ..defaults.camera.clone()
        },
        Camera {
            eye: [0.0, 1.0, 0.0],
            target: [0.0; 3],
            ..defaults.camera.clone()
        },
        Camera {
            near: -1.0,
            ..defaults.camera.clone()
        },
        Camera {
            far: 0.01,
            ..defaults.camera.clone()
        },
        Camera {
            vertical_fov_degrees: f32::NAN,
            ..defaults.camera.clone()
        },
    ] {
        cases.push(CaptureOptions {
            camera,
            ..defaults.clone()
        });
    }
    for options in cases {
        assert!(project_bodies(runtime.scene(), &snapshot, &options).is_err());
    }
}

#[test]
fn mismatched_duplicate_and_nonfinite_snapshot_bodies_are_rejected() {
    let runtime = runtime();
    let original = runtime.snapshot().unwrap();
    for mode in 0..4 {
        let mut snapshot = original.clone();
        match mode {
            0 => {
                snapshot.bodies.pop();
            }
            1 => snapshot.bodies[0].id = "missing".into(),
            2 => snapshot.bodies[0] = snapshot.bodies[1].clone(),
            _ => snapshot.bodies[0].position[0] = f32::NAN,
        }
        assert!(project_bodies(runtime.scene(), &snapshot, &CaptureOptions::default()).is_err());
    }
}

#[test]
fn view_json_rejects_unknown_fields() {
    let mut value = serde_json::to_value(CaptureOptions::default()).unwrap();
    value["camera"]["typo"] = true.into();
    assert!(serde_json::from_value::<CaptureOptions>(value).is_err());
}

fn single_body() -> Runtime {
    Runtime::from_scene(Scene::from_json(r#"{
        "schema_version":1,"revision":0,"name":"GPU projection test",
        "gravity":[0,0,0],"fixed_dt":0.016666668,
        "bodies":[{"id":"box","motion":"dynamic","shape":{"type":"box","half_extents":[0.8,0.2,0.2]},"position":[0,0,0]}]
    }"#).unwrap()).unwrap()
}

fn test_view() -> CaptureOptions {
    CaptureOptions {
        width: 257,
        height: 193,
        camera: Camera {
            eye: [0.0, 0.0, 6.0],
            target: [0.0; 3],
            ..Camera::default()
        },
        ..CaptureOptions::default()
    }
}

fn orange_bounds(rgba: &[u8], width: u32) -> (u32, u32, u32, u32) {
    let mut result = (u32::MAX, u32::MAX, 0, 0);
    let mut count = 0;
    for (i, pixel) in rgba.chunks_exact(4).enumerate() {
        if u16::from(pixel[0]) > u16::from(pixel[1]) + 30 && pixel[1] > pixel[2] {
            let (x, y) = (i as u32 % width, i as u32 / width);
            result.0 = result.0.min(x);
            result.1 = result.1.min(y);
            result.2 = result.2.max(x);
            result.3 = result.3.max(y);
            count += 1;
        }
    }
    assert!(
        count > 20,
        "capture must contain rendered geometry, not just background"
    );
    result
}

#[test]
#[ignore = "requires a Metal/Vulkan/DX12 adapter; failure is never treated as a skip"]
fn gpu_pixels_follow_translation_rotation_and_png_row_alignment() {
    let runtime = single_body();
    let mut snapshot = runtime.snapshot().unwrap();
    let options = test_view();
    let first = capture(runtime.scene(), &snapshot, &options).unwrap();
    assert_eq!(first.rgba.len(), 257 * 193 * 4);
    let a = orange_bounds(&first.rgba, 257);
    assert!(a.2 - a.0 > 2 * (a.3 - a.1));
    let center = first.metadata.bodies[0].center_pixel.unwrap();
    assert!((center[0] - 128.5).abs() < 0.01);
    assert!((center[1] - 96.5).abs() < 0.01);
    snapshot.bodies[0].position = [1.0, 0.6, 0.0];
    snapshot.bodies[0].rotation = [
        0.0,
        0.0,
        std::f32::consts::FRAC_1_SQRT_2,
        std::f32::consts::FRAC_1_SQRT_2,
    ];
    let second = capture(runtime.scene(), &snapshot, &options).unwrap();
    let b = orange_bounds(&second.rgba, 257);
    assert!(b.3 - b.1 > 2 * (b.2 - b.0));
    assert!(b.0 + b.2 > a.0 + a.2);
    assert!(b.1 + b.3 < a.1 + a.3);
    let png = second.png().unwrap();
    let decoder = png::Decoder::new(std::io::Cursor::new(png));
    let mut reader = decoder.read_info().unwrap();
    let mut pixels = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut pixels).unwrap();
    assert_eq!((info.width, info.height), (257, 193));
    assert_eq!(&pixels[..info.buffer_size()], second.rgba);
}

#[test]
#[ignore = "requires a Metal/Vulkan/DX12 adapter; failure is never treated as a skip"]
fn gpu_sphere_depth_and_collider_overlay_are_real() {
    let runtime = single_body();
    let mut scene = runtime.scene().clone();
    scene.bodies[0].shape = tomo_scene::ShapeSpec::Sphere { radius: 0.7 };
    let mut back = scene.bodies[0].clone();
    back.id = "occluder".into();
    back.motion = tomo_scene::Motion::Static;
    back.position[2] = 2.0;
    scene.bodies.push(back);
    let world = Runtime::from_scene(scene.clone()).unwrap();
    let options = test_view();
    let hidden = capture(&scene, &world.snapshot().unwrap(), &options).unwrap();
    let index = (96 * 257 + 128) * 4;
    assert!(
        hidden.rgba[index] < hidden.rgba[index + 2],
        "near static sphere must occlude orange body"
    );
    scene.bodies.pop();
    let world = Runtime::from_scene(scene.clone()).unwrap();
    let plain = capture(&scene, &world.snapshot().unwrap(), &options).unwrap();
    orange_bounds(&plain.rgba, 257);
    let overlay = capture(
        &scene,
        &world.snapshot().unwrap(),
        &CaptureOptions {
            colliders: true,
            ..options
        },
    )
    .unwrap();
    assert_ne!(plain.rgba, overlay.rgba);
    assert!(
        overlay
            .rgba
            .chunks_exact(4)
            .filter(|p| p[1] > 240 && p[0] < 120)
            .count()
            > 10
    );
}
