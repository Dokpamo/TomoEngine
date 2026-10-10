// SPDX-FileCopyrightText: 2021 Jorrit Rouwe
// SPDX-FileCopyrightText: 2026 TomoEngine contributors
// SPDX-License-Identifier: MIT
//! Primitive narrow phase for the first runtime.
//!
//! Sphere/box closest-point construction follows the inspected BoxShape kernels.
//! Box SAT uses the 15 axes in Jolt's OrientedBox::Overlaps, but penetration-axis
//! selection, polygon clipping and reduction are Tomo-specific. This module is
//! NOT a port of Jolt's general convex GJK/EPA/CollideConvexVsConvex pipeline.

use crate::Vec3;
use crate::body::{check_range, checked_transform};
use crate::error::PhysicsError;
use crate::rotation::Transform;
use crate::shape::{Shape, ShapeKind, axis, lowest_axis};

/// A normal points from body A to body B. Negative separation means penetration.
#[derive(Clone, Copy, Debug)]
pub struct ContactPoint {
    pub point_a: Vec3,
    pub point_b: Vec3,
}

impl ContactPoint {
    pub fn separation(self, normal: Vec3) -> f32 {
        (self.point_b - self.point_a).dot(normal)
    }
}

#[derive(Clone, Debug)]
pub struct ContactManifold {
    pub normal: Vec3,
    pub points: Vec<ContactPoint>,
}

/// Checked standalone collision query, with up to four contact points.
/// Positive margin enables speculative contacts, not continuous collision detection.
pub fn contact_manifold(
    a: Shape,
    transform_a: Transform,
    b: Shape,
    transform_b: Transform,
    margin: f32,
) -> Result<Option<ContactManifold>, PhysicsError> {
    check_range(margin, 0.0, 100.0, "contact margin")?;
    let transform_a = checked_transform(transform_a)?;
    let transform_b = checked_transform(transform_b)?;
    Ok(collide(a, transform_a, b, transform_b, margin))
}

pub(crate) fn collide(
    a: Shape,
    pa: Transform,
    b: Shape,
    pb: Transform,
    margin: f32,
) -> Option<ContactManifold> {
    match (a.kind, b.kind) {
        (ShapeKind::Sphere(ra), ShapeKind::Sphere(rb)) => {
            sphere_sphere(pa.position, ra, pb.position, rb, margin)
        }
        (ShapeKind::Sphere(radius), ShapeKind::Box(half)) => {
            sphere_box(pa.position, radius, BoxFrame::new(pb, half), margin)
        }
        (ShapeKind::Box(half), ShapeKind::Sphere(radius)) => {
            sphere_box(pb.position, radius, BoxFrame::new(pa, half), margin).map(|mut manifold| {
                manifold.normal = -manifold.normal;
                for point in &mut manifold.points {
                    std::mem::swap(&mut point.point_a, &mut point.point_b);
                }
                manifold
            })
        }
        (ShapeKind::Box(ha), ShapeKind::Box(hb)) => {
            box_box(BoxFrame::new(pa, ha), BoxFrame::new(pb, hb), margin)
        }
    }
}

fn sphere_sphere(a: Vec3, ra: f32, b: Vec3, rb: f32, margin: f32) -> Option<ContactManifold> {
    let delta = b - a;
    let squared = delta.length_squared();
    let radius_sum = ra + rb;
    if squared > (radius_sum + margin) * (radius_sum + margin) {
        return None;
    }
    let normal = if squared > 0.0 {
        delta / squared.sqrt()
    } else {
        Vec3::X
    };
    Some(ContactManifold {
        normal,
        points: vec![ContactPoint {
            point_a: a + normal * ra,
            point_b: b - normal * rb,
        }],
    })
}

fn sphere_box(center: Vec3, radius: f32, frame: BoxFrame, margin: f32) -> Option<ContactManifold> {
    let local = frame.pose.inverse_transform_point(center);
    let closest = local.max(-frame.half).min(frame.half);
    let delta = closest - local;
    let squared = delta.length_squared();
    let (normal_local, box_point) = if squared > 0.0 {
        if squared > (radius + margin) * (radius + margin) {
            return None;
        }
        (delta / squared.sqrt(), closest)
    } else {
        // An enclosed sphere must be pushed toward the closest face, not toward
        // the box center. Since A receives -normal, use the inward normal.
        let distances = frame.half - local.abs();
        let index = lowest_axis(distances);
        let outward = axis(index) * if local[index] < 0.0 { -1.0 } else { 1.0 };
        (-outward, local + distances[index] * outward)
    };
    let normal = frame.pose.rotation.rotate(normal_local);
    Some(ContactManifold {
        normal,
        points: vec![ContactPoint {
            point_a: center + normal * radius,
            point_b: frame.pose.transform_point(box_point),
        }],
    })
}

#[derive(Clone, Copy)]
struct BoxFrame {
    pose: Transform,
    half: Vec3,
    axes: [Vec3; 3],
}

impl BoxFrame {
    fn new(pose: Transform, half: Vec3) -> Self {
        Self {
            pose,
            half,
            axes: pose.rotation.axes(),
        }
    }

    fn projection_radius(self, direction: Vec3) -> f32 {
        (self.half.x * self.axes[0].dot(direction).abs()
            + self.half.y * self.axes[1].dot(direction).abs())
            + self.half.z * self.axes[2].dot(direction).abs()
    }

    fn face(self, index: usize, outward: Vec3) -> Vec<Vec3> {
        let center = self.pose.position
            + self.axes[index] * self.half[index] * sign(self.axes[index].dot(outward));
        let u = (index + 1) % 3;
        let v = (index + 2) % 3;
        let du = self.axes[u] * self.half[u];
        let dv = self.axes[v] * self.half[v];
        vec![
            center - du - dv,
            center + du - dv,
            center + du + dv,
            center - du + dv,
        ]
    }

    fn support_edge(self, index: usize, direction: Vec3) -> (Vec3, Vec3) {
        let mut center = self.pose.position;
        for other in 0..3 {
            if other != index {
                center +=
                    self.axes[other] * self.half[other] * sign(self.axes[other].dot(direction));
            }
        }
        let half_edge = self.axes[index] * self.half[index];
        (center - half_edge, center + half_edge)
    }
}

fn sign(value: f32) -> f32 {
    if value < 0.0 { -1.0 } else { 1.0 }
}

#[derive(Clone, Copy)]
enum Feature {
    FaceA(usize),
    FaceB(usize),
    Edge(usize, usize),
}

fn box_box(a: BoxFrame, b: BoxFrame, margin: f32) -> Option<ContactManifold> {
    let delta = b.pose.position - a.pose.position;
    let mut selected = Feature::FaceA(0);
    let mut normal = a.axes[0];
    let mut best_separation = -f32::MAX;
    let mut test_axis = |candidate: Vec3, feature: Feature, edge: bool| -> bool {
        let squared = candidate.length_squared();
        // Near-parallel edge axes cannot be normalized reliably.
        if squared < 1.0e-12 {
            return true;
        }
        let candidate = candidate / squared.sqrt();
        let center_distance = delta.dot(candidate);
        let separation =
            center_distance.abs() - a.projection_radius(candidate) - b.projection_radius(candidate);
        if separation > margin {
            return false;
        }
        // Prefer face contacts in near-ties to avoid unstable edge/face switching.
        let preference = if edge { 1.0e-4 } else { 0.0 };
        if separation > best_separation + preference {
            best_separation = separation;
            normal = candidate * sign(center_distance);
            selected = feature;
        }
        true
    };
    for index in 0..3 {
        if !test_axis(a.axes[index], Feature::FaceA(index), false) {
            return None;
        }
    }
    for index in 0..3 {
        if !test_axis(b.axes[index], Feature::FaceB(index), false) {
            return None;
        }
    }
    for ai in 0..3 {
        for bi in 0..3 {
            if !test_axis(a.axes[ai].cross(b.axes[bi]), Feature::Edge(ai, bi), true) {
                return None;
            }
        }
    }
    let mut points = match selected {
        Feature::FaceA(index) => face_contacts(a, b, index, normal, margin),
        Feature::FaceB(index) => {
            let mut points = face_contacts(b, a, index, -normal, margin);
            for point in &mut points {
                std::mem::swap(&mut point.point_a, &mut point.point_b);
            }
            points
        }
        Feature::Edge(ai, bi) => {
            let (a0, a1) = a.support_edge(ai, normal);
            let (b0, b1) = b.support_edge(bi, -normal);
            let (point_a, point_b) = closest_segments(a0, a1, b0, b1);
            vec![ContactPoint { point_a, point_b }]
        }
    };
    points.retain(|point| point.separation(normal) <= margin + 1.0e-5);
    if points.is_empty() {
        return None;
    }
    reduce_points(&mut points, normal);
    Some(ContactManifold { normal, points })
}

fn face_contacts(
    reference: BoxFrame,
    incident: BoxFrame,
    index: usize,
    normal: Vec3,
    margin: f32,
) -> Vec<ContactPoint> {
    let face_center = reference.pose.position
        + reference.axes[index] * reference.half[index] * sign(reference.axes[index].dot(normal));
    let mut incident_index = 0;
    for axis in 1..3 {
        if incident.axes[axis].dot(normal).abs() > incident.axes[incident_index].dot(normal).abs() {
            incident_index = axis;
        }
    }
    let mut polygon = incident.face(incident_index, -normal);
    for side in 0..3 {
        if side != index {
            polygon = clip_polygon(
                polygon,
                face_center,
                reference.axes[side],
                reference.half[side],
            );
            polygon = clip_polygon(
                polygon,
                face_center,
                -reference.axes[side],
                reference.half[side],
            );
        }
    }
    let mut result = Vec::new();
    for point_b in polygon {
        let separation = (point_b - face_center).dot(normal);
        if separation <= margin + 1.0e-5 {
            let contact = ContactPoint {
                point_a: point_b - normal * separation,
                point_b,
            };
            if result.iter().all(|existing: &ContactPoint| {
                (existing.point_b - point_b).length_squared() > 1.0e-12
            }) {
                result.push(contact);
            }
        }
    }
    result
}

/// Sutherland-Hodgman clipping against dot(point - origin, normal) <= offset.
fn clip_polygon(input: Vec<Vec3>, origin: Vec3, normal: Vec3, offset: f32) -> Vec<Vec3> {
    let Some(&last) = input.last() else {
        return input;
    };
    let mut output = Vec::with_capacity(input.len() + 1);
    let mut previous = last;
    let mut previous_distance = (previous - origin).dot(normal) - offset;
    for current in input {
        let distance = (current - origin).dot(normal) - offset;
        if (distance <= 0.0) != (previous_distance <= 0.0) {
            let fraction = previous_distance / (previous_distance - distance);
            output.push(previous + (current - previous) * fraction);
        }
        if distance <= 0.0 {
            output.push(current);
        }
        previous = current;
        previous_distance = distance;
    }
    output
}

fn closest_segments(a0: Vec3, a1: Vec3, b0: Vec3, b1: Vec3) -> (Vec3, Vec3) {
    let da = a1 - a0;
    let db = b1 - b0;
    let relative = a0 - b0;
    let aa = da.dot(da);
    let bb = db.dot(db);
    let ab = da.dot(db);
    let ar = da.dot(relative);
    let br = db.dot(relative);
    let denominator = aa * bb - ab * ab;
    let mut s = if denominator > 1.0e-8 * aa * bb {
        ((ab * br - ar * bb) / denominator).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let mut t = (ab * s + br) / bb;
    if t < 0.0 {
        t = 0.0;
        s = (-ar / aa).clamp(0.0, 1.0);
    } else if t > 1.0 {
        t = 1.0;
        s = ((ab - ar) / aa).clamp(0.0, 1.0);
    }
    (a0 + da * s, b0 + db * t)
}

fn reduce_points(points: &mut Vec<ContactPoint>, normal: Vec3) {
    if points.len() <= 4 {
        return;
    }
    let mut selected = Vec::with_capacity(4);
    let deepest = (0..points.len())
        .min_by(|&a, &b| {
            points[a]
                .separation(normal)
                .total_cmp(&points[b].separation(normal))
        })
        .unwrap();
    selected.push(points.remove(deepest));
    while selected.len() < 4 {
        let score = |point: ContactPoint| -> f32 {
            if selected.len() == 2 {
                let edge = selected[1].point_a - selected[0].point_a;
                edge.cross(point.point_a - selected[0].point_a)
                    .dot(normal)
                    .abs()
            } else {
                selected
                    .iter()
                    .map(|s| (point.point_a - s.point_a).length_squared())
                    .fold(f32::MAX, f32::min)
            }
        };
        let next = (0..points.len())
            .max_by(|&a, &b| score(points[a]).total_cmp(&score(points[b])))
            .unwrap();
        selected.push(points.remove(next));
    }
    *points = selected;
}
