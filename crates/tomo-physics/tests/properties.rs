// SPDX-License-Identifier: MIT
//! Fixed-seed invariant tests, not a substitute for upstream differential tests.
use tomo_physics::ray::{NO_HIT, RayInvDirection, ray_aabb, ray_sphere};
use tomo_physics::{Aabb, Vec3};

struct Rng(u64);

impl Rng {
    fn scalar(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1);
        let value = (self.0 >> 40) as u32;
        value as f32 / 16_777_216.0 * 20.0 - 10.0
    }

    fn vector(&mut self) -> Vec3 {
        Vec3::new(self.scalar(), self.scalar(), self.scalar())
    }
}

#[test]
fn ten_thousand_box_union_intersection_and_support_invariants() {
    let mut rng = Rng(0x746f_6d6f_2026);
    for case in 0..10_000 {
        let a = Aabb::from_two_points(rng.vector(), rng.vector());
        let b = Aabb::from_two_points(rng.vector(), rng.vector());
        let mut union = a;
        union.encapsulate_box(b);
        assert!(union.contains_box(a) && union.contains_box(b), "case {case}: {a:?} {b:?}");
        assert_eq!(a.overlaps(b), b.overlaps(a));
        assert_eq!(a.overlaps(b), a.intersection(b).is_valid());
        let direction = rng.vector();
        let support = a.support(direction);
        for bits in 0..8 {
            let corner = Vec3::new(
                if bits & 1 == 0 { a.min.x } else { a.max.x },
                if bits & 2 == 0 { a.min.y } else { a.max.y },
                if bits & 4 == 0 { a.min.z } else { a.max.z },
            );
            assert!(support.dot(direction) + 1.0e-4 >= corner.dot(direction), "case {case}: {a:?} {direction:?}");
        }
    }
}

#[test]
fn ten_thousand_sphere_hit_and_parameterization_invariants() {
    let mut rng = Rng(0x6a6f_6c74_2026);
    for case in 0..10_000 {
        let center = rng.vector();
        let radius = rng.scalar().abs() + 0.25;
        let axis = rng.vector().normalized_or(Vec3::X);
        let origin = center + axis * (radius + 3.0);
        let direction = -axis * 6.0;
        let t = ray_sphere(origin, direction, center, radius);
        assert!(t != NO_HIT && (t - 0.5).abs() < 5.0e-4, "case {case}: {origin:?} {direction:?} {center:?} {radius} => {t}");
        let hit = origin + direction * t;
        assert!(((hit - center).length() - radius).abs() < 5.0e-4);
        let half = ray_sphere(origin, direction * 2.0, center, radius);
        assert!((2.0 * half - t).abs() < 5.0e-5);
    }
}

#[test]
fn ten_thousand_box_ray_known_face_hits() {
    let mut rng = Rng(0x7261_7973_2026);
    for case in 0..10_000 {
        let center = rng.vector();
        let half = rng.vector().abs() + Vec3::splat(0.25);
        let bounds = Aabb::new(center - half, center + half);
        let origin = Vec3::new(bounds.min.x - 4.0, center.y, center.z);
        let inverse = RayInvDirection::new(Vec3::X * 8.0);
        let fraction = ray_aabb(origin, &inverse, bounds);
        assert!((fraction - 0.5).abs() < 1.0e-6, "case {case}: {bounds:?} {origin:?} => {fraction}");
    }
}
