// SPDX-License-Identifier: MIT
//! Public API regressions. These are not differential tests against C++ Jolt.
use tomo_physics::{
    BodySettings, CollisionFilter, ContactEventKind, PhysicsSettings, PhysicsSystem,
    RayCastOptions, Shape, Vec3,
};

fn free_world() -> PhysicsSystem {
    PhysicsSystem::with_settings(PhysicsSettings {
        gravity: Vec3::ZERO,
        enable_sleeping: false,
        ..PhysicsSettings::default()
    })
    .unwrap()
}

fn sphere(position: Vec3, mass: f32) -> BodySettings {
    let mut body = BodySettings::dynamic(Shape::sphere(0.5).unwrap(), position, mass);
    body.linear_damping = 0.0;
    body.angular_damping = 0.0;
    body.allow_sleep = false;
    body
}

fn close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 1.0e-5,
        "actual={actual}, expected={expected}"
    );
}

#[test]
fn invalid_steps_preserve_pending_force_pose_and_clock() {
    let mut world = free_world();
    let id = world.add_body(sphere(Vec3::ZERO, 2.0)).unwrap();
    world.add_force(id, Vec3::X * 4.0).unwrap();
    for dt in [0.0, -0.1, f32::NAN, f32::INFINITY, 0.5, 1.0e-6] {
        assert!(world.update(dt).is_err(), "accepted invalid dt={dt}");
        assert_eq!(world.body(id).unwrap().position(), Vec3::ZERO);
        assert_eq!(world.body(id).unwrap().linear_velocity(), Vec3::ZERO);
        assert_eq!(world.simulation_time(), 0.0);
    }
    world.update(0.1).unwrap();
    close(world.body(id).unwrap().linear_velocity().x, 0.2);
    world.update(0.1).unwrap();
    close(world.body(id).unwrap().linear_velocity().x, 0.2);
}

#[test]
fn gravity_has_no_hard_coded_floor_or_height_clamp() {
    let mut world = PhysicsSystem::new();
    let id = world
        .add_body(sphere(Vec3::new(0.0, -10.0, 0.0), 1.0))
        .unwrap();
    for _ in 0..60 {
        world.update(1.0 / 60.0).unwrap();
    }
    let body = world.body(id).unwrap();
    assert!(body.position().y < -14.0, "unexpected floor: {body:?}");
    assert!(body.linear_velocity().y < -9.0);
    assert!(world.contacts().is_empty());
}

#[test]
fn zero_length_world_ray_queries_have_finite_results() {
    let mut world = free_world();
    let id = world
        .add_body(BodySettings::fixed(Shape::sphere(0.5).unwrap(), Vec3::ZERO))
        .unwrap();
    let hit = world.ray_cast(Vec3::ZERO, Vec3::ZERO).unwrap().unwrap();
    assert_eq!(hit.body_id, id);
    assert_eq!(hit.fraction, 0.0);
    assert!(hit.normal.is_finite());
    assert!(hit.position.is_finite());
    assert!(world.ray_cast(Vec3::X * 2.0, Vec3::ZERO).unwrap().is_none());
}

#[test]
fn ray_options_respect_segment_length_exclusion_and_sensor_flag() {
    let mut world = free_world();
    let shape = Shape::sphere(0.5).unwrap();
    let mut settings = BodySettings::fixed(shape, Vec3::X * 2.0);
    settings.is_sensor = true;
    let sensor = world.add_body(settings).unwrap();
    let solid = world
        .add_body(BodySettings::fixed(shape, Vec3::X * 6.0))
        .unwrap();
    assert!(world.ray_cast(Vec3::ZERO, Vec3::X).unwrap().is_none());
    let direction = Vec3::X * 10.0;
    let hit = world.ray_cast(Vec3::ZERO, direction).unwrap().unwrap();
    assert_eq!(hit.body_id, sensor);
    close(hit.fraction, 0.15);
    for options in [
        RayCastOptions {
            include_sensors: false,
            ..RayCastOptions::default()
        },
        RayCastOptions {
            exclude: Some(sensor),
            ..RayCastOptions::default()
        },
    ] {
        let hit = world
            .ray_cast_filtered(Vec3::ZERO, direction, options)
            .unwrap()
            .unwrap();
        assert_eq!(hit.body_id, solid);
        close(hit.fraction, 0.55);
    }
}

#[test]
fn ray_filter_requires_both_query_and_body_to_accept() {
    let mut world = free_world();
    let mut settings = BodySettings::fixed(Shape::sphere(0.5).unwrap(), Vec3::X * 2.0);
    settings.filter = CollisionFilter {
        membership: 1,
        mask: 1,
    };
    let id = world.add_body(settings).unwrap();
    let options = RayCastOptions {
        filter: CollisionFilter {
            membership: 2,
            mask: 1,
        },
        ..RayCastOptions::default()
    };
    assert!(
        world
            .ray_cast_filtered(Vec3::ZERO, Vec3::X * 4.0, options)
            .unwrap()
            .is_none()
    );
    let options = RayCastOptions {
        filter: CollisionFilter {
            membership: 1,
            mask: 1,
        },
        ..RayCastOptions::default()
    };
    let hit = world
        .ray_cast_filtered(Vec3::ZERO, Vec3::X * 4.0, options)
        .unwrap()
        .unwrap();
    assert_eq!(hit.body_id, id);
}

#[test]
fn removing_sensor_queues_exactly_one_stopped_event() {
    let mut world = free_world();
    let mut settings = BodySettings::fixed(Shape::sphere(2.0).unwrap(), Vec3::ZERO);
    settings.is_sensor = true;
    let sensor = world.add_body(settings).unwrap();
    world.add_body(sphere(Vec3::ZERO, 1.0)).unwrap();
    world.update(1.0 / 60.0).unwrap();
    assert!(
        world
            .contact_events()
            .iter()
            .any(|event| event.kind == ContactEventKind::Started && event.is_sensor)
    );
    world.remove_body(sensor).unwrap();
    world.update(1.0 / 60.0).unwrap();
    let stopped: Vec<_> = world
        .contact_events()
        .iter()
        .filter(|event| event.kind == ContactEventKind::Stopped)
        .collect();
    assert_eq!(stopped.len(), 1);
    assert!(stopped[0].is_sensor);
    assert!(stopped[0].body_a == sensor || stopped[0].body_b == sensor);
    world.update(1.0 / 60.0).unwrap();
    assert!(world.contact_events().is_empty());
}

#[test]
fn impulse_obeys_inverse_mass_and_leaves_static_body_unchanged() {
    let mut world = free_world();
    let light = world.add_body(sphere(-Vec3::X * 10.0, 2.0)).unwrap();
    let heavy = world.add_body(sphere(Vec3::X * 10.0, 4.0)).unwrap();
    let fixed = world
        .add_body(BodySettings::fixed(Shape::sphere(0.5).unwrap(), Vec3::ZERO))
        .unwrap();
    world.apply_impulse(light, Vec3::X * 8.0).unwrap();
    world.apply_impulse(heavy, Vec3::X * 8.0).unwrap();
    assert!(world.apply_impulse(fixed, Vec3::X * 8.0).is_err());
    close(world.body(light).unwrap().linear_velocity().x, 4.0);
    close(world.body(heavy).unwrap().linear_velocity().x, 2.0);
    world.update(0.1).unwrap();
    assert_eq!(world.body(fixed).unwrap().position(), Vec3::ZERO);
    assert_eq!(world.body(fixed).unwrap().linear_velocity(), Vec3::ZERO);
}

#[test]
fn waking_one_island_does_not_wake_disconnected_sleepers() {
    let mut world = PhysicsSystem::with_settings(PhysicsSettings {
        gravity: Vec3::ZERO,
        ..PhysicsSettings::default()
    })
    .unwrap();
    let shape = Shape::sphere(0.5).unwrap();
    let first = world
        .add_body(BodySettings::dynamic(shape, -Vec3::X * 10.0, 1.0))
        .unwrap();
    let second = world
        .add_body(BodySettings::dynamic(shape, Vec3::X * 10.0, 1.0))
        .unwrap();
    for _ in 0..60 {
        world.update(1.0 / 60.0).unwrap();
    }
    assert!(world.body(first).unwrap().is_sleeping());
    assert!(world.body(second).unwrap().is_sleeping());
    world.apply_impulse(first, Vec3::Y).unwrap();
    world.update(1.0 / 60.0).unwrap();
    assert!(!world.body(first).unwrap().is_sleeping());
    assert!(world.body(second).unwrap().is_sleeping());
    assert_eq!(world.body(second).unwrap().position(), Vec3::X * 10.0);
}
