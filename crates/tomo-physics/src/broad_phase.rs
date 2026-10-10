// SPDX-License-Identifier: MIT
//! Deterministic single-axis sweep-and-prune integration scaffold.
//! This is NOT Jolt's multithreaded QuadTree broad-phase port.

use crate::body::{BodyArena, CollisionFilter, MotionType};
use crate::{Aabb, BodyId, Vec3};

struct Proxy {
    id: BodyId,
    bounds: Aabb,
    motion: MotionType,
    filter: CollisionFilter,
    sensor: bool,
}

pub(crate) fn pairs(bodies: &BodyArena, margin: f32) -> Vec<(BodyId, BodyId)> {
    let mut proxies: Vec<_> = bodies.iter().map(|body| Proxy {
        id: body.id,
        bounds: body.world_bounds().expanded(Vec3::splat(0.5 * margin)),
        motion: body.motion_type,
        filter: body.filter,
        sensor: body.is_sensor,
    }).collect();
    proxies.sort_by(|a, b| a.bounds.min.x.total_cmp(&b.bounds.min.x).then(a.id.cmp(&b.id)));
    let mut output = Vec::new();
    for (index, a) in proxies.iter().enumerate() {
        for b in &proxies[index + 1..] {
            if b.bounds.min.x > a.bounds.max.x { break; }
            let physical = a.motion == MotionType::Dynamic || b.motion == MotionType::Dynamic;
            let sensor_pair = (a.sensor && b.motion == MotionType::Kinematic)
                || (b.sensor && a.motion == MotionType::Kinematic);
            if (physical || sensor_pair) && a.filter.allows(b.filter) && a.bounds.overlaps(b.bounds) {
                output.push(if a.id < b.id { (a.id, b.id) } else { (b.id, a.id) });
            }
        }
    }
    output.sort_unstable();
    output
}
