// SPDX-License-Identifier: MIT
//! Scene/invariant regression tests, NOT full Jolt differential tests.
use tomo_physics::{BodyId, BodySettings, CollisionFilter, ContactEventKind, DistanceJointSettings, PhysicsError, PhysicsSettings, PhysicsSystem, Quat, RayCastOptions, Shape, SpringSettings, Transform, Vec3};

fn close(actual: f32, expected: f32, tolerance: f32) {
    assert!((actual - expected).abs() <= tolerance, "actual={actual}, expected={expected}, tolerance={tolerance}");
}

fn simulate(world: &mut PhysicsSystem, frames: usize, dt: f32) {
    for frame in 0..frames { world.update(dt).unwrap_or_else(|error| panic!("frame {frame}: {error}")); }
}

fn floor(world: &mut PhysicsSystem, friction: f32) -> BodyId {
    let mut settings = BodySettings::fixed(Shape::cuboid(Vec3::new(20.0, 0.5, 20.0)).unwrap(), Vec3::new(0.0, -0.5, 0.0));
    settings.friction = friction;
    world.add_body(settings).unwrap()
}

fn zero_gravity() -> PhysicsSystem {
    PhysicsSystem::with_settings(PhysicsSettings { gravity: Vec3::ZERO, enable_sleeping: false, ..PhysicsSettings::default() }).unwrap()
}

fn undamped(shape: Shape, position: Vec3, mass: f32) -> BodySettings {
    let mut settings = BodySettings::dynamic(shape, position, mass);
    settings.linear_damping = 0.0;
    settings.angular_damping = 0.0;
    settings
}

#[test]
fn falling_box_settles_and_sleeps_on_real_static_geometry() {
    let mut world = PhysicsSystem::new();
    let floor_id = floor(&mut world, 0.5);
    let id = world.add_body(BodySettings::dynamic(Shape::cuboid(Vec3::splat(0.5)).unwrap(), Vec3::Y * 5.0, 1.0)).unwrap();
    let mut started = false;
    for frame in 0..600 {
        world.update(1.0 / 60.0).unwrap_or_else(|e| panic!("frame={frame}: {e}"));
        started |= world.contact_events().iter().any(|e| e.kind == ContactEventKind::Started);
    }
    let body = world.body(id).unwrap();
    close(body.position().y, 0.5, 0.015);
    assert!(body.linear_velocity().length() < 0.01, "{:?}", body.linear_velocity());
    assert!(body.is_sleeping(), "body did not sleep: {body:?}");
    assert!(started, "no contact-start event was emitted");
    close(world.body(floor_id).unwrap().position().y, -0.5, 0.0);
}

#[test]
fn falling_sphere_settles() {
    let mut world = PhysicsSystem::new();
    floor(&mut world, 0.5);
    let id = world.add_body(BodySettings::dynamic(Shape::sphere(0.5).unwrap(), Vec3::Y * 3.0, 1.0)).unwrap();
    simulate(&mut world, 480, 1.0 / 60.0);
    close(world.body(id).unwrap().position().y, 0.5, 0.01);
    assert!(world.body(id).unwrap().is_sleeping());
}

#[test]
fn rotated_box_lands_without_nan_or_tunneling() {
    let mut world = PhysicsSystem::new();
    floor(&mut world, 0.7);
    let mut settings = BodySettings::dynamic(Shape::cuboid(Vec3::new(0.5, 0.7, 0.4)).unwrap(), Vec3::new(0.0, 4.0, 0.0), 2.0);
    settings.rotation = Quat::from_axis_angle(Vec3::new(1.0, 0.2, 0.4), 0.6).unwrap();
    let id = world.add_body(settings).unwrap();
    simulate(&mut world, 900, 1.0 / 60.0);
    let body = world.body(id).unwrap();
    assert!(body.world_bounds().min.y > -0.025, "{body:?}");
    assert!(body.position().y > 0.35 && body.position().y < 1.0, "{body:?}");
    close(body.rotation().length_squared(), 1.0, 1.0e-5);
    assert!(body.linear_velocity().length() < 0.1);
}

#[test]
fn five_box_stack_remains_stable_and_sleeps_as_an_island() {
    let mut world = PhysicsSystem::new();
    floor(&mut world, 0.6);
    let shape = Shape::cuboid(Vec3::splat(0.5)).unwrap();
    let ids: Vec<_> = (0..5).map(|i| world.add_body(BodySettings::dynamic(shape, Vec3::new(0.0, 0.51 + i as f32 * 1.01, 0.0), 1.0)).unwrap()).collect();
    simulate(&mut world, 1200, 1.0 / 60.0);
    for (index, &id) in ids.iter().enumerate() {
        let body = world.body(id).unwrap();
        close(body.position().y, 0.5 + index as f32, 0.08);
        assert!(body.position().x.abs() < 0.1 && body.position().z.abs() < 0.1, "stack drift: {body:?}");
        assert!(body.is_sleeping(), "stack member {index} awake: {body:?}");
    }
    world.apply_impulse(*ids.last().unwrap(), Vec3::X).unwrap();
    assert!(ids.iter().all(|&id| !world.body(id).unwrap().is_sleeping()), "activation did not propagate through the stack");
}

#[test]
fn elastic_spheres_conserve_linear_momentum_and_exchange_velocity() {
    let mut world = zero_gravity();
    let shape = Shape::sphere(0.5).unwrap();
    let mut a = undamped(shape, -Vec3::X * 1.5, 1.0);
    let mut b = undamped(shape, Vec3::X * 1.5, 1.0);
    a.linear_velocity = Vec3::X * 2.0;
    b.linear_velocity = -Vec3::X * 2.0;
    a.restitution = 1.0;
    b.restitution = 1.0;
    let a = world.add_body(a).unwrap();
    let b = world.add_body(b).unwrap();
    simulate(&mut world, 120, 1.0 / 120.0);
    let va = world.body(a).unwrap().linear_velocity();
    let vb = world.body(b).unwrap().linear_velocity();
    close(va.x, -2.0, 0.01);
    close(vb.x, 2.0, 0.01);
    assert!((va + vb).length() < 1.0e-4);
    close(0.5 * (va.length_squared() + vb.length_squared()), 4.0, 0.02);
}

#[test]
fn off_center_impulse_changes_angular_velocity_using_inertia() {
    let mut world = zero_gravity();
    let id = world.add_body(undamped(Shape::cuboid(Vec3::ONE).unwrap(), Vec3::ZERO, 12.0)).unwrap();
    world.apply_impulse_at(id, Vec3::X * 2.0, Vec3::Y).unwrap();
    let body = world.body(id).unwrap();
    close(body.linear_velocity().x, 1.0 / 6.0, 1.0e-6);
    close(body.angular_velocity().z, -0.25, 1.0e-6);
    simulate(&mut world, 60, 1.0 / 60.0);
    assert!(world.body(id).unwrap().rotation().z.abs() > 0.1);
}

#[test]
fn force_is_applied_for_every_substep_then_cleared() {
    for substeps in [1, 2, 4, 8] {
        let mut world = PhysicsSystem::with_settings(PhysicsSettings { gravity: Vec3::ZERO, substeps, enable_sleeping: false, ..PhysicsSettings::default() }).unwrap();
        let id = world.add_body(undamped(Shape::sphere(1.0).unwrap(), Vec3::ZERO, 2.0)).unwrap();
        world.add_force(id, Vec3::X * 6.0).unwrap();
        world.update(0.1).unwrap();
        close(world.body(id).unwrap().linear_velocity().x, 0.3, 1.0e-5);
        world.update(0.1).unwrap();
        close(world.body(id).unwrap().linear_velocity().x, 0.3, 1.0e-5);
    }
}

#[test]
fn applied_torque_uses_world_inertia() {
    let mut world = zero_gravity();
    let id = world.add_body(undamped(Shape::cuboid(Vec3::ONE).unwrap(), Vec3::ZERO, 12.0)).unwrap();
    world.add_torque(id, Vec3::Z * 8.0).unwrap();
    world.update(0.1).unwrap();
    close(world.body(id).unwrap().angular_velocity().z, 0.1, 1.0e-5);
}

#[test]
fn friction_stops_sliding_box_but_zero_friction_does_not() {
    for friction in [0.0, 0.8] {
        let mut world = PhysicsSystem::new();
        floor(&mut world, friction);
        let mut settings = undamped(Shape::cuboid(Vec3::splat(0.5)).unwrap(), Vec3::Y * 0.5, 1.0);
        settings.friction = friction;
        settings.linear_velocity = Vec3::X * 3.0;
        let id = world.add_body(settings).unwrap();
        simulate(&mut world, 180, 1.0 / 60.0);
        let body = world.body(id).unwrap();
        if friction == 0.0 {
            close(body.linear_velocity().x, 3.0, 0.02);
            assert!(body.position().x > 8.5);
        } else {
            assert!(body.linear_velocity().length() < 0.05, "{body:?}");
            assert!(body.position().x > 0.1 && body.position().x < 2.0, "{body:?}");
        }
    }
}

#[test]
fn sensor_emits_start_and_stop_without_collision_response() {
    let mut world = zero_gravity();
    let mut sensor = BodySettings::fixed(Shape::cuboid(Vec3::splat(0.5)).unwrap(), Vec3::ZERO);
    sensor.is_sensor = true;
    world.add_body(sensor).unwrap();
    let mut settings = undamped(Shape::sphere(0.2).unwrap(), -Vec3::X * 2.0, 1.0);
    settings.linear_velocity = Vec3::X * 2.0;
    let id = world.add_body(settings).unwrap();
    let (mut started, mut stopped) = (false, false);
    for _ in 0..240 {
        world.update(1.0 / 120.0).unwrap();
        for event in world.contact_events() {
            assert!(event.is_sensor);
            started |= event.kind == ContactEventKind::Started;
            stopped |= event.kind == ContactEventKind::Stopped;
        }
    }
    assert!(started && stopped);
    close(world.body(id).unwrap().linear_velocity().x, 2.0, 1.0e-5);
    close(world.body(id).unwrap().position().x, 2.0, 2.0e-4);
}

#[test]
fn collision_filter_is_two_sided() {
    let mut world = zero_gravity();
    let mut fixed = BodySettings::fixed(Shape::cuboid(Vec3::ONE).unwrap(), Vec3::ZERO);
    fixed.filter = CollisionFilter { membership: 1, mask: 1 };
    world.add_body(fixed).unwrap();
    let mut moving = undamped(Shape::sphere(0.2).unwrap(), -Vec3::X * 2.0, 1.0);
    moving.filter = CollisionFilter { membership: 2, mask: u32::MAX };
    moving.linear_velocity = Vec3::X * 2.0;
    let id = world.add_body(moving).unwrap();
    simulate(&mut world, 120, 1.0 / 60.0);
    assert!(world.contacts().is_empty());
    close(world.body(id).unwrap().position().x, 2.0, 2.0e-4);
}

#[test]
fn kinematic_box_pushes_dynamic_sphere_without_losing_its_velocity() {
    let mut world = zero_gravity();
    let mut mover = BodySettings::kinematic(Shape::cuboid(Vec3::splat(0.5)).unwrap(), -Vec3::X * 2.0);
    mover.linear_velocity = Vec3::X * 2.0;
    let mover = world.add_body(mover).unwrap();
    let sphere = world.add_body(undamped(Shape::sphere(0.5).unwrap(), Vec3::ZERO, 1.0)).unwrap();
    simulate(&mut world, 180, 1.0 / 120.0);
    close(world.body(mover).unwrap().position().x, 1.0, 1.0e-4);
    close(world.body(mover).unwrap().linear_velocity().x, 2.0, 0.0);
    assert!(world.body(sphere).unwrap().position().x > 1.9);
}

#[test]
fn distance_joint_keeps_a_pendulum_attached() {
    let mut world = PhysicsSystem::new();
    let anchor = world.add_body(BodySettings::fixed(Shape::sphere(0.1).unwrap(), Vec3::ZERO)).unwrap();
    let bob = world.add_body(BodySettings::dynamic(Shape::sphere(0.2).unwrap(), Vec3::X * 2.0, 1.0)).unwrap();
    world.add_distance_joint(DistanceJointSettings::new(anchor, bob, 2.0)).unwrap();
    let mut moved = false;
    for frame in 0..600 {
        world.update(1.0 / 120.0).unwrap();
        let body = world.body(bob).unwrap();
        assert!((body.position().length() - 2.0).abs() < 0.03, "frame {frame}: {body:?}");
        moved |= body.position().y < -1.0;
    }
    assert!(moved, "pendulum never moved");
}

#[test]
fn damped_distance_spring_converges_to_rest_length() {
    let mut world = zero_gravity();
    let anchor = world.add_body(BodySettings::fixed(Shape::sphere(0.1).unwrap(), Vec3::ZERO)).unwrap();
    let bob = world.add_body(undamped(Shape::sphere(0.2).unwrap(), Vec3::X * 3.0, 1.0)).unwrap();
    let mut settings = DistanceJointSettings::new(anchor, bob, 1.0);
    settings.spring = Some(SpringSettings { frequency_hz: 2.0, damping_ratio: 1.0 });
    let joint = world.add_distance_joint(settings).unwrap();
    simulate(&mut world, 600, 1.0 / 120.0);
    close(world.body(bob).unwrap().position().length(), 1.0, 0.01);
    assert!(world.body(bob).unwrap().linear_velocity().length() < 0.01);
    world.set_joint_distance(joint, 2.0).unwrap();
    simulate(&mut world, 600, 1.0 / 120.0);
    close(world.body(bob).unwrap().position().length(), 2.0, 0.01);
}

#[test]
fn removed_body_invalidates_handle_and_removes_attached_joints() {
    let mut world = PhysicsSystem::new();
    let shape = Shape::sphere(0.5).unwrap();
    let a = world.add_body(BodySettings::fixed(shape, Vec3::ZERO)).unwrap();
    let b = world.add_body(BodySettings::dynamic(shape, Vec3::X * 2.0, 1.0)).unwrap();
    let joint = world.add_distance_joint(DistanceJointSettings::new(a, b, 2.0)).unwrap();
    world.remove_body(b).unwrap();
    assert_eq!(world.body_count(), 1);
    assert_eq!(world.joint_count(), 0);
    assert!(world.body(b).is_none());
    assert!(world.joint_settings(joint).is_none());
    assert_eq!(world.apply_impulse(b, Vec3::ONE), Err(PhysicsError::InvalidBody));
    let replacement = world.add_body(BodySettings::dynamic(shape, Vec3::Y, 1.0)).unwrap();
    assert_ne!(b, replacement);
}

#[test]
fn body_slot_is_retired_before_sequence_wrap_can_alias_a_stale_id() {
    let mut world = zero_gravity();
    let settings = BodySettings::dynamic(Shape::sphere(0.5).unwrap(), Vec3::ZERO, 1.0);
    let original = world.add_body(settings.clone()).unwrap();
    world.remove_body(original).unwrap();
    for iteration in 0..600 {
        let current = world.add_body(settings.clone()).unwrap();
        assert_ne!(current, original, "ABA alias at allocation {iteration}");
        assert!(world.body(original).is_none());
        world.remove_body(current).unwrap();
    }
}

#[test]
fn removing_support_wakes_a_sleeping_body() {
    let mut world = PhysicsSystem::new();
    let ground = floor(&mut world, 0.5);
    let id = world.add_body(BodySettings::dynamic(Shape::cuboid(Vec3::splat(0.5)).unwrap(), Vec3::Y, 1.0)).unwrap();
    simulate(&mut world, 360, 1.0 / 60.0);
    assert!(world.body(id).unwrap().is_sleeping());
    world.remove_body(ground).unwrap();
    assert!(!world.body(id).unwrap().is_sleeping());
    simulate(&mut world, 30, 1.0 / 60.0);
    assert!(world.body(id).unwrap().position().y < -0.5);
}

#[test]
fn teleporting_static_geometry_wakes_newly_overlapped_sleeping_body() {
    let mut world = PhysicsSystem::with_settings(PhysicsSettings { gravity: Vec3::ZERO, ..PhysicsSettings::default() }).unwrap();
    let ground = world.add_body(BodySettings::fixed(Shape::cuboid(Vec3::splat(0.5)).unwrap(), Vec3::ZERO)).unwrap();
    let id = world.add_body(BodySettings::dynamic(Shape::sphere(0.5).unwrap(), Vec3::X * 5.0, 1.0)).unwrap();
    simulate(&mut world, 120, 1.0 / 60.0);
    assert!(world.body(id).unwrap().is_sleeping());
    world.set_transform(ground, Transform::new(Vec3::X * 5.0, Quat::IDENTITY)).unwrap();
    world.update(1.0 / 60.0).unwrap();
    assert!(!world.body(id).unwrap().is_sleeping());
    assert!((world.body(id).unwrap().position() - Vec3::X * 5.0).length() > 0.1);
}

#[test]
fn invalid_parameters_do_not_mutate_existing_body() {
    let mut world = PhysicsSystem::new();
    let id = world.add_body(BodySettings::dynamic(Shape::sphere(0.5).unwrap(), Vec3::Y, 1.0)).unwrap();
    let pose = world.body(id).unwrap().transform();
    for dt in [0.0, -0.1, f32::NAN, f32::INFINITY, 1.0] { assert!(world.update(dt).is_err()); }
    assert!(world.set_transform(id, Transform::new(Vec3::splat(f32::NAN), Quat::IDENTITY)).is_err());
    assert!(world.set_velocity(id, Vec3::splat(f32::INFINITY), Vec3::ZERO).is_err());
    assert!(world.add_force(id, Vec3::splat(f32::NAN)).is_err());
    assert_eq!(world.body(id).unwrap().transform(), pose);
    assert_eq!(world.body(id).unwrap().linear_velocity(), Vec3::ZERO);
    assert!(Shape::sphere(0.0).is_err());
    assert!(Shape::cuboid(Vec3::new(-1.0, 1.0, 1.0)).is_err());
    let mut invalid = BodySettings::dynamic(Shape::sphere(1.0).unwrap(), Vec3::ZERO, 0.0);
    assert!(world.add_body(invalid.clone()).is_err());
    invalid.mass = 1.0;
    invalid.rotation = Quat::new(0.0, 0.0, 0.0, 0.0);
    assert!(world.add_body(invalid).is_err());
    assert_eq!(world.body_count(), 1);
}

#[test]
fn static_bodies_reject_forces_and_capacity_errors_preserve_world() {
    let mut world = PhysicsSystem::with_settings(PhysicsSettings { max_bodies: 1, ..PhysicsSettings::default() }).unwrap();
    let settings = BodySettings::fixed(Shape::sphere(0.5).unwrap(), Vec3::ZERO);
    let id = world.add_body(settings.clone()).unwrap();
    assert!(world.apply_impulse(id, Vec3::ONE).is_err());
    assert!(world.add_force(id, Vec3::ONE).is_err());
    assert!(world.set_velocity(id, Vec3::ONE, Vec3::ZERO).is_err());
    assert!(matches!(world.add_body(settings), Err(PhysicsError::BodyCapacityExceeded)));
    assert_eq!(world.body_count(), 1);
}

#[test]
fn world_ray_cast_uses_current_rotation_and_nearest_hit() {
    let mut world = zero_gravity();
    let mut settings = BodySettings::fixed(Shape::cuboid(Vec3::new(1.0, 0.5, 0.5)).unwrap(), Vec3::ZERO);
    settings.rotation = Quat::from_axis_angle(Vec3::Y, std::f32::consts::FRAC_PI_2).unwrap();
    let box_id = world.add_body(settings).unwrap();
    let sphere_id = world.add_body(BodySettings::fixed(Shape::sphere(0.5).unwrap(), Vec3::X * 3.0)).unwrap();
    let origin = -Vec3::X * 3.0;
    let direction = Vec3::X * 10.0;
    let hit = world.ray_cast(origin, direction).unwrap().unwrap();
    assert_eq!(hit.body_id, box_id);
    close(hit.fraction, 0.25, 1.0e-5);
    assert!(hit.normal.dot(-Vec3::X) > 0.999);
    let options = RayCastOptions { exclude: Some(box_id), ..RayCastOptions::default() };
    let hit = world.ray_cast_filtered(origin, direction, options).unwrap().unwrap();
    assert_eq!(hit.body_id, sphere_id);
    close(hit.fraction, 0.55, 1.0e-5);
    assert!(world.ray_cast(Vec3::Y * 20.0, direction).unwrap().is_none());
}

#[test]
fn initial_overlap_is_resolved_without_injecting_kinetic_energy() {
    let mut world = zero_gravity();
    let a = world.add_body(undamped(Shape::sphere(0.5).unwrap(), Vec3::ZERO, 1.0)).unwrap();
    let b = world.add_body(undamped(Shape::sphere(0.5).unwrap(), Vec3::X * 0.5, 1.0)).unwrap();
    simulate(&mut world, 30, 1.0 / 60.0);
    assert!((world.body(a).unwrap().position() - world.body(b).unwrap().position()).length() > 0.995);
    assert!(world.body(a).unwrap().linear_velocity().length() < 1.0e-5);
    assert!(world.body(b).unwrap().linear_velocity().length() < 1.0e-5);
}

#[test]
fn gyroscopic_motion_stays_finite_and_rotation_normalized() {
    let mut world = zero_gravity();
    let mut settings = undamped(Shape::cuboid(Vec3::new(0.2, 0.4, 0.8)).unwrap(), Vec3::ZERO, 2.0);
    settings.apply_gyroscopic_force = true;
    settings.angular_velocity = Vec3::new(0.2, 8.0, 1.0);
    let id = world.add_body(settings).unwrap();
    simulate(&mut world, 1200, 1.0 / 120.0);
    let body = world.body(id).unwrap();
    assert!(body.angular_velocity().is_finite());
    assert!(body.angular_velocity().length() < 30.0);
    close(body.rotation().length_squared(), 1.0, 1.0e-5);
}

#[test]
fn same_process_replay_is_bit_reproducible() {
    fn scene() -> PhysicsSystem {
        let mut world = PhysicsSystem::new();
        floor(&mut world, 0.5);
        for i in 0..4 {
            let mut body = BodySettings::dynamic(Shape::cuboid(Vec3::splat(0.4)).unwrap(), Vec3::new(0.1 * i as f32, 1.0 + i as f32, 0.0), 1.0);
            body.rotation = Quat::from_axis_angle(Vec3::Z, 0.1 * i as f32).unwrap();
            world.add_body(body).unwrap();
        }
        world
    }
    let (mut a, mut b) = (scene(), scene());
    simulate(&mut a, 300, 1.0 / 60.0);
    simulate(&mut b, 300, 1.0 / 60.0);
    for (a, b) in a.bodies().zip(b.bodies()) {
        assert_eq!(a.position().to_array().map(f32::to_bits), b.position().to_array().map(f32::to_bits));
        assert_eq!(a.rotation(), b.rotation());
        assert_eq!(a.linear_velocity(), b.linear_velocity());
    }
}

#[test]
fn empty_world_steps_and_gravity_change_wakes_sleepers() {
    let mut world = PhysicsSystem::with_settings(PhysicsSettings { gravity: Vec3::ZERO, ..PhysicsSettings::default() }).unwrap();
    let stats = world.update(1.0 / 60.0).unwrap();
    assert_eq!(stats.contact_points, 0);
    let id = world.add_body(BodySettings::dynamic(Shape::sphere(0.5).unwrap(), Vec3::Y, 1.0)).unwrap();
    simulate(&mut world, 120, 1.0 / 60.0);
    assert!(world.body(id).unwrap().is_sleeping());
    world.set_gravity(-Vec3::Y).unwrap();
    assert!(!world.body(id).unwrap().is_sleeping());
    world.update(0.1).unwrap();
    assert!(world.body(id).unwrap().linear_velocity().y < -0.09);
}
