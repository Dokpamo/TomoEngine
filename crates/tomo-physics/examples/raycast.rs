// SPDX-License-Identifier: MIT
use tomo_physics::ray::{NO_HIT, RayInvDirection, ray_aabb, ray_sphere, ray_triangle};
use tomo_physics::{Aabb, Vec3};

fn main() {
    let origin = Vec3::new(-3.0, 0.0, 0.0);
    let direction = Vec3::X * 4.0;
    let sphere = ray_sphere(origin, direction, Vec3::ZERO, 1.0);
    let bounds = Aabb::new(-Vec3::ONE, Vec3::ONE);
    let box_hit = ray_aabb(origin, &RayInvDirection::new(direction), bounds);
    let triangle = ray_triangle(Vec3::new(0.25, 0.25, 1.0), -Vec3::Z, Vec3::ZERO, Vec3::X, Vec3::Y);
    assert_eq!(sphere, 0.5);
    assert_eq!(box_hit, 0.5);
    assert_eq!(triangle, 1.0);
    assert_ne!(sphere, NO_HIT);
    println!("sphere fraction: {sphere}, box fraction: {box_hit}, triangle fraction: {triangle}");
    println!("Foundation query example only: rigid-body simulation is not implemented yet.");
}
