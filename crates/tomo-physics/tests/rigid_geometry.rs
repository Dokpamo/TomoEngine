// SPDX-License-Identifier: MIT
use tomo_physics::collision::contact_manifold;
use tomo_physics::{Quat, Shape, Transform, Vec3};

#[test]
fn quaternion_hamilton_product_and_rotation_order() {
    let x = Quat::from_axis_angle(Vec3::X, std::f32::consts::FRAC_PI_2).unwrap();
    let y = Quat::from_axis_angle(Vec3::Y, std::f32::consts::FRAC_PI_2).unwrap();
    assert!(((y * x).rotate(Vec3::Y) - Vec3::X).length() < 1.0e-5);
    assert!(
        (x.inverse_rotate(x.rotate(Vec3::new(1.0, 2.0, 3.0))) - Vec3::new(1.0, 2.0, 3.0)).length()
            < 1.0e-5
    );
    assert_eq!(Quat::IDENTITY * x, x);
    assert!(Quat::from_axis_angle(Vec3::ZERO, 1.0).is_none());
    assert!(Quat::from_axis_angle(Vec3::X, f32::NAN).is_none());
}

#[test]
fn repeated_rotation_steps_stay_normalized() {
    let mut q = Quat::IDENTITY;
    for _ in 0..10000 {
        q = q.rotation_step(Vec3::Y * 0.001);
    }
    assert!((q.length_squared() - 1.0).abs() < 1.0e-5);
    let expected = Quat::from_axis_angle(Vec3::Y, 10.0)
        .unwrap()
        .rotate(Vec3::X);
    assert!((q.rotate(Vec3::X) - expected).length() < 0.002);
}

#[test]
fn solid_primitive_mass_properties_match_analytic_values() {
    let box_properties = Shape::cuboid(Vec3::new(1.0, 2.0, 3.0))
        .unwrap()
        .mass_properties(12.0)
        .unwrap();
    assert_eq!(box_properties.inertia_diagonal, Vec3::new(52.0, 40.0, 20.0));
    assert_eq!(
        Shape::sphere(2.0)
            .unwrap()
            .mass_properties(5.0)
            .unwrap()
            .inertia_diagonal,
        Vec3::splat(8.0)
    );
    assert_eq!(
        Shape::cuboid(Vec3::new(1.0, 2.0, 3.0)).unwrap().volume(),
        48.0
    );
}

#[test]
fn face_contacts_have_four_points_and_consistent_normal_and_depth() {
    let shape = Shape::cuboid(Vec3::ONE).unwrap();
    let a = Transform::IDENTITY;
    let b = Transform::new(Vec3::Y * 1.5, Quat::IDENTITY);
    let contact = contact_manifold(shape, a, shape, b, 0.0).unwrap().unwrap();
    assert_eq!(contact.points.len(), 4);
    assert!(contact.normal.dot(Vec3::Y) > 0.999);
    for point in &contact.points {
        assert!((point.separation(contact.normal) + 0.5).abs() < 1.0e-5);
    }
    let reverse = contact_manifold(shape, b, shape, a, 0.0).unwrap().unwrap();
    assert!(reverse.normal.dot(contact.normal) < -0.999);
}

#[test]
fn rotated_boxes_can_have_overlapping_aabbs_without_touching() {
    let shape = Shape::cuboid(Vec3::new(2.0, 0.05, 0.05)).unwrap();
    let rotation = Quat::from_axis_angle(Vec3::Z, std::f32::consts::FRAC_PI_4).unwrap();
    let a = Transform::new(Vec3::ZERO, rotation);
    let b = Transform::new(rotation.rotate(Vec3::Y) * 0.2, rotation);
    assert!(shape.world_bounds(a).overlaps(shape.world_bounds(b)));
    assert!(contact_manifold(shape, a, shape, b, 0.0).unwrap().is_none());
}

#[test]
fn enclosed_sphere_gets_an_escape_normal_and_valid_contact() {
    let sphere = Shape::sphere(0.2).unwrap();
    let cuboid = Shape::cuboid(Vec3::ONE).unwrap();
    let center = Vec3::new(0.9, 0.0, 0.0);
    let contact = contact_manifold(
        sphere,
        Transform::new(center, Quat::IDENTITY),
        cuboid,
        Transform::IDENTITY,
        0.0,
    )
    .unwrap()
    .unwrap();
    assert!(contact.normal.dot(-Vec3::X) > 0.999);
    assert!((contact.points[0].separation(contact.normal) + 0.3).abs() < 1.0e-5);
}

struct Rng(u64);
impl Rng {
    fn scalar(&mut self) -> f32 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 40) as f32 / 16_777_216.0) * 2.0 - 1.0
    }
    fn vector(&mut self) -> Vec3 {
        Vec3::new(self.scalar(), self.scalar(), self.scalar())
    }
    fn rotation(&mut self) -> Quat {
        Quat::from_axis_angle(self.vector().normalized_or(Vec3::X), self.scalar() * 3.0).unwrap()
    }
}

fn corners(half: Vec3, pose: Transform) -> Vec<Vec3> {
    (0..8)
        .map(|bits| {
            pose.transform_point(Vec3::new(
                half.x * if bits & 1 == 0 { -1.0 } else { 1.0 },
                half.y * if bits & 2 == 0 { -1.0 } else { 1.0 },
                half.z * if bits & 4 == 0 { -1.0 } else { 1.0 },
            ))
        })
        .collect()
}

#[test]
fn two_thousand_oriented_box_queries_match_independent_vertex_projection_oracle() {
    // Independently expressed SAT, not an upstream C++ executable comparison.
    let mut rng = Rng(0x746f_6d6f_7361_7421);
    for case in 0..2000 {
        let ha = rng.vector().abs() + Vec3::splat(0.1);
        let hb = rng.vector().abs() + Vec3::splat(0.1);
        let pa = Transform::new(rng.vector() * 2.0, rng.rotation());
        let pb = Transform::new(rng.vector() * 2.0, rng.rotation());
        let va = corners(ha, pa);
        let vb = corners(hb, pb);
        let aa = pa.rotation.axes();
        let ab = pb.rotation.axes();
        let mut axes = aa.to_vec();
        axes.extend(ab);
        for a in aa {
            for b in ab {
                let cross = a.cross(b);
                if cross.length_squared() > 1.0e-10 {
                    axes.push(cross.normalized_or(Vec3::X));
                }
            }
        }
        let mut largest_gap = -f32::MAX;
        for axis in axes {
            let projection = |vertices: &[Vec3]| {
                vertices
                    .iter()
                    .map(|v| v.dot(axis))
                    .fold((f32::MAX, -f32::MAX), |(min, max), value| {
                        (min.min(value), max.max(value))
                    })
            };
            let (amin, amax) = projection(&va);
            let (bmin, bmax) = projection(&vb);
            largest_gap = largest_gap.max(amin - bmax).max(bmin - amax);
        }
        if largest_gap.abs() < 1.0e-4 {
            continue;
        }
        let result = contact_manifold(
            Shape::cuboid(ha).unwrap(),
            pa,
            Shape::cuboid(hb).unwrap(),
            pb,
            0.0,
        )
        .unwrap();
        assert_eq!(
            result.is_some(),
            largest_gap < 0.0,
            "case {case}: gap={largest_gap}, ha={ha:?}, hb={hb:?}, pa={pa:?}, pb={pb:?}"
        );
        if let Some(manifold) = result {
            assert!((manifold.normal.length_squared() - 1.0).abs() < 1.0e-4);
            assert!((1..=4).contains(&manifold.points.len()));
            for point in manifold.points {
                assert!(point.point_a.is_finite() && point.point_b.is_finite());
                assert!(point.separation(manifold.normal) <= 2.0e-5);
            }
        }
    }
}
