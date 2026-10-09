// SPDX-License-Identifier: MIT
use tomo_physics::ray::{NO_HIT, RayInvDirection, ray_aabb, ray_aabb_interval, ray_sphere, ray_triangle};
use tomo_physics::{Aabb, Vec3};

fn unit_box() -> Aabb {
    Aabb::new(-Vec3::ONE, Vec3::ONE)
}

#[test]
fn empty_box_and_accumulation() {
    let mut b = Aabb::default();
    assert!(!b.is_valid());
    assert!(!b.overlaps(unit_box()));
    b.encapsulate_point(Vec3::new(1.0, 2.0, 3.0));
    b.encapsulate_point(Vec3::new(-1.0, 5.0, 0.0));
    assert_eq!(b.min, Vec3::new(-1.0, 2.0, 0.0));
    assert_eq!(b.max, Vec3::new(1.0, 5.0, 3.0));
    b.encapsulate_box(unit_box());
    assert!(b.contains_box(unit_box()));
}

#[test]
fn box_measurements() {
    let b = Aabb::new(Vec3::ZERO, Vec3::new(2.0, 4.0, 6.0));
    assert_eq!(b.size(), Vec3::new(2.0, 4.0, 6.0));
    assert_eq!(b.center(), Vec3::new(1.0, 2.0, 3.0));
    assert_eq!(b.extent(), b.center());
    assert_eq!(b.volume(), 48.0);
    assert_eq!(b.surface_area(), 88.0);
    assert!(Aabb::biggest().size().is_finite());
}

#[test]
fn intersection_touching_and_separation() {
    let a = unit_box();
    let b = a.translated(Vec3::new(2.0, 0.0, 0.0));
    assert!(a.overlaps(b));
    assert!(a.intersection(b).is_valid());
    assert_eq!(a.intersection(b).volume(), 0.0);
    let c = b.translated(Vec3::X);
    assert!(!a.overlaps(c));
    assert!(!a.intersection(c).is_valid());
}

#[test]
fn containment_includes_boundary() {
    let b = unit_box();
    assert!(b.contains_point(Vec3::ONE));
    assert!(b.contains_point(-Vec3::ONE));
    assert!(!b.contains_point(Vec3::new(1.001, 0.0, 0.0)));
    assert!(b.contains_box(b));
    assert!(!b.contains_box(b.expanded(Vec3::ONE)));
}

#[test]
fn negative_scaling_and_empty_preservation() {
    let b = Aabb::new(Vec3::new(1.0, 2.0, 3.0), Vec3::new(4.0, 5.0, 6.0));
    let scaled = b.scaled(Vec3::new(-2.0, 0.0, 3.0));
    assert_eq!(scaled.min, Vec3::new(-8.0, 0.0, 9.0));
    assert_eq!(scaled.max, Vec3::new(-2.0, 0.0, 18.0));
    assert!(!Aabb::EMPTY.scaled(-Vec3::ONE).is_valid());
}

#[test]
fn support_and_distance() {
    let b = unit_box();
    assert_eq!(b.support(Vec3::new(0.0, -2.0, 3.0)), Vec3::new(1.0, -1.0, 1.0));
    assert_eq!(b.closest_point(Vec3::new(4.0, 0.0, -5.0)), Vec3::new(1.0, 0.0, -1.0));
    assert_eq!(b.squared_distance_to(Vec3::new(4.0, 0.0, -5.0)), 25.0);
    assert_eq!(b.squared_distance_to(Vec3::ZERO), 0.0);
}

#[test]
fn minimal_edge_length() {
    let mut b = Aabb::new(Vec3::ZERO, Vec3::new(0.0, 1.0, 3.0));
    b.ensure_minimal_edge_length(2.0);
    assert_eq!(b.max, Vec3::new(2.0, 2.0, 3.0));
}

#[test]
fn invalid_interior_box_does_not_overlap() {
    let bad = Aabb::new(Vec3::new(0.5, 0.0, 0.0), Vec3::new(-0.5, 0.0, 0.0));
    assert!(!bad.overlaps(unit_box()));
}

#[test]
fn sphere_ray_nonunit_direction() {
    assert_eq!(ray_sphere(Vec3::new(-3.0, 0.0, 0.0), Vec3::X * 4.0, Vec3::ZERO, 1.0), 0.5);
    assert_eq!(ray_sphere(Vec3::new(-3.0, 0.0, 0.0), Vec3::X, Vec3::ZERO, 1.0), 2.0);
}

#[test]
fn sphere_inside_surface_and_stationary() {
    assert_eq!(ray_sphere(Vec3::ZERO, Vec3::X, Vec3::ZERO, 1.0), 0.0);
    assert_eq!(ray_sphere(Vec3::X, Vec3::X, Vec3::ZERO, 1.0), 0.0);
    assert_eq!(ray_sphere(Vec3::ZERO, Vec3::ZERO, Vec3::ZERO, 1.0), 0.0);
    assert_eq!(ray_sphere(Vec3::X * 2.0, Vec3::ZERO, Vec3::ZERO, 1.0), NO_HIT);
}

#[test]
fn sphere_tangent_miss_and_behind() {
    assert_eq!(ray_sphere(Vec3::new(-2.0, 1.0, 0.0), Vec3::X, Vec3::ZERO, 1.0), 2.0);
    assert_eq!(ray_sphere(Vec3::new(-2.0, 2.0, 0.0), Vec3::X, Vec3::ZERO, 1.0), NO_HIT);
    assert_eq!(ray_sphere(Vec3::X * 3.0, Vec3::X, Vec3::ZERO, 1.0), NO_HIT);
}

#[test]
fn zero_radius_sphere() {
    assert_eq!(ray_sphere(-Vec3::X, Vec3::X, Vec3::ZERO, 0.0), 1.0);
    assert_eq!(ray_sphere(Vec3::ZERO, Vec3::ZERO, Vec3::ZERO, 0.0), 0.0);
}

#[test]
fn box_ray_entry_exit_and_inside() {
    let inv = RayInvDirection::new(Vec3::X * 2.0);
    assert_eq!(ray_aabb_interval(-Vec3::X * 3.0, &inv, unit_box()), Some((1.0, 2.0)));
    assert_eq!(ray_aabb(Vec3::ZERO, &inv, unit_box()), -0.5);
    assert_eq!(ray_aabb(Vec3::X * 3.0, &inv, unit_box()), NO_HIT);
}

#[test]
fn box_parallel_boundaries_and_near_parallel() {
    let b = unit_box();
    let inv = RayInvDirection::new(Vec3::new(1.0, -0.0, 1.0e-21));
    assert_eq!(ray_aabb(Vec3::new(-3.0, 1.0, 1.0), &inv, b), 2.0);
    assert_eq!(ray_aabb(Vec3::new(-3.0, 1.1, 0.0), &inv, b), NO_HIT);
    assert_eq!(ray_aabb(Vec3::ZERO, &inv, Aabb::EMPTY), NO_HIT);
}

#[test]
fn box_stationary_and_negative_direction() {
    let still = RayInvDirection::new(Vec3::ZERO);
    assert_eq!(ray_aabb_interval(Vec3::ZERO, &still, unit_box()), Some((-f32::MAX, f32::MAX)));
    assert_eq!(ray_aabb(Vec3::X * 2.0, &still, unit_box()), NO_HIT);
    let inv = RayInvDirection::new(-Vec3::X * 2.0);
    assert_eq!(ray_aabb(Vec3::X * 3.0, &inv, unit_box()), 1.0);
}

#[test]
fn triangle_front_back_and_edge() {
    let p = Vec3::new(0.25, 0.25, 1.0);
    assert_eq!(ray_triangle(p, -Vec3::Z * 2.0, Vec3::ZERO, Vec3::X, Vec3::Y), 0.5);
    assert_eq!(ray_triangle(p, -Vec3::Z, Vec3::ZERO, Vec3::Y, Vec3::X), 1.0);
    assert_eq!(ray_triangle(Vec3::Z, -Vec3::Z, Vec3::ZERO, Vec3::X, Vec3::Y), 1.0);
}

#[test]
fn triangle_miss_parallel_degenerate_and_behind() {
    let p = Vec3::new(0.25, 0.25, 1.0);
    assert_eq!(ray_triangle(p, Vec3::X, Vec3::ZERO, Vec3::X, Vec3::Y), NO_HIT);
    assert_eq!(ray_triangle(p, Vec3::Z, Vec3::ZERO, Vec3::X, Vec3::Y), NO_HIT);
    assert_eq!(ray_triangle(p, -Vec3::Z, Vec3::ZERO, Vec3::X, Vec3::X), NO_HIT);
    assert_eq!(ray_triangle(Vec3::ONE, -Vec3::Z, Vec3::ZERO, Vec3::X, Vec3::Y), NO_HIT);
}
