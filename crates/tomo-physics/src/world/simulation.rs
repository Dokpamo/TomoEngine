// SPDX-License-Identifier: MIT
//! Runtime scheduling, contact persistence and dynamic-island sleeping.
use super::*;

impl PhysicsSystem {
    pub(super) fn substep(&mut self, dt: f32) -> StepStats {
        for body in self.bodies.iter_mut() {
            body.integrate_forces(self.settings.gravity, dt);
        }
        let candidates = broad_phase::pairs(&self.bodies, self.settings.speculative_distance);
        let excluded: BTreeSet<_> = self
            .joints
            .values()
            .filter(|j| !j.settings.collide_connected)
            .map(|j| canonical(j.settings.body_a, j.settings.body_b))
            .collect();
        let mut contacts = Vec::new();
        for &(aid, bid) in &candidates {
            if excluded.contains(&(aid, bid)) {
                continue;
            }
            let a = self.bodies.get(aid).expect("broad-phase body exists");
            let b = self.bodies.get(bid).expect("broad-phase body exists");
            let sensor = a.is_sensor || b.is_sensor;
            let margin = if sensor {
                0.0
            } else {
                self.settings.speculative_distance
            };
            if let Some(manifold) = collide(a.shape, a.pose, b.shape, b.pose, margin) {
                contacts.push(BodyContact {
                    body_a: aid,
                    body_b: bid,
                    is_sensor: sensor,
                    manifold,
                });
            }
        }
        let islands = self.build_islands(&contacts);
        for island in &islands {
            if island.driven
                || island
                    .members
                    .iter()
                    .any(|&id| !self.bodies.get(id).unwrap().sleeping)
            {
                for &id in &island.members {
                    let body = self.bodies.get_mut(id).unwrap();
                    if body.sleeping {
                        body.wake();
                        body.integrate_forces(self.settings.gravity, dt);
                    }
                    if island.driven {
                        body.sleep_time = 0.0;
                    }
                }
            }
        }
        let ratio = if self.settings.warm_start && self.previous_dt > 0.0 {
            (dt / self.previous_dt).min(2.0)
        } else {
            0.0
        };
        let config = SolverConfig {
            dt,
            restitution_threshold: self.settings.restitution_velocity_threshold,
            warm_start_ratio: ratio,
        };
        let mut constraints = Vec::new();
        for contact in &contacts {
            if contact.is_sensor {
                continue;
            }
            let a = self.bodies.get(contact.body_a).unwrap();
            let b = self.bodies.get(contact.body_b).unwrap();
            if !active_pair(a, b) {
                continue;
            }
            let old = self
                .cache
                .get(&(a.id, b.id))
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            let mut used = vec![false; old.len()];
            for &point in &contact.manifold.points {
                let la = a.pose.inverse_transform_point(point.point_a);
                let lb = b.pose.inverse_transform_point(point.point_b);
                let best = old
                    .iter()
                    .enumerate()
                    .filter(|(index, value)| {
                        !used[*index] && value.normal.dot(contact.manifold.normal) > 0.95
                    })
                    .map(|(index, value)| {
                        (
                            index,
                            (value.local_a - la).length_squared()
                                + (value.local_b - lb).length_squared(),
                        )
                    })
                    .filter(|(_, distance)| *distance < 0.0008)
                    .min_by(|a, b| a.1.total_cmp(&b.1));
                let cached = best.map(|(index, _)| {
                    used[index] = true;
                    &old[index]
                });
                constraints.push(ContactConstraint::new(
                    a,
                    b,
                    point,
                    contact.manifold.normal,
                    cached,
                    config,
                ));
            }
        }
        let mut joint_rows = Vec::new();
        for (&id, joint) in &mut self.joints {
            let a = self.bodies.get(joint.settings.body_a).unwrap();
            let b = self.bodies.get(joint.settings.body_b).unwrap();
            if !active_pair(a, b) {
                continue;
            }
            let (mut row, axis, _) = distance_row(joint, a, b, dt);
            if axis.dot(joint.last_axis) > 0.95 {
                row.lambda = joint.lambda * ratio;
            }
            joint.last_axis = axis;
            joint_rows.push((id, row));
        }
        for constraint in &constraints {
            let (a, b) = self
                .bodies
                .pair_mut(constraint.ids.0, constraint.ids.1)
                .unwrap();
            constraint.warm_start(a, b);
        }
        for &(id, row) in &joint_rows {
            let settings = &self.joints[&id].settings;
            let (a, b) = self
                .bodies
                .pair_mut(settings.body_a, settings.body_b)
                .unwrap();
            row.warm_start(a, b);
        }
        for _ in 0..self.settings.velocity_iterations {
            for (id, row) in &mut joint_rows {
                let settings = &self.joints[id].settings;
                let (a, b) = self
                    .bodies
                    .pair_mut(settings.body_a, settings.body_b)
                    .unwrap();
                row.solve(a, b, -f32::MAX, f32::MAX);
            }
            for constraint in &mut constraints {
                let (a, b) = self
                    .bodies
                    .pair_mut(constraint.ids.0, constraint.ids.1)
                    .unwrap();
                constraint.solve_velocity(a, b);
            }
        }
        for body in self.bodies.iter_mut() {
            body.integrate_position(dt);
        }
        let position_config = PositionConfig {
            slop: self.settings.penetration_slop,
            max_correction: self.settings.max_position_correction,
            baumgarte: self.settings.baumgarte,
        };
        for _ in 0..self.settings.position_iterations {
            for (id, _) in &joint_rows {
                let joint = &self.joints[id];
                if joint.settings.spring.is_some() {
                    continue;
                }
                let (a, b) = self
                    .bodies
                    .pair_mut(joint.settings.body_a, joint.settings.body_b)
                    .unwrap();
                let (row, _, error) = distance_row(joint, a, b, dt);
                row.solve_position(
                    a,
                    b,
                    error.clamp(
                        -position_config.max_correction,
                        position_config.max_correction,
                    ),
                    position_config.baumgarte,
                );
            }
            for constraint in &constraints {
                let (a, b) = self
                    .bodies
                    .pair_mut(constraint.ids.0, constraint.ids.1)
                    .unwrap();
                constraint.solve_position(a, b, position_config);
            }
        }
        let mut next_cache: BTreeMap<Pair, Vec<CachedContact>> = BTreeMap::new();
        for constraint in constraints {
            next_cache
                .entry(constraint.ids)
                .or_default()
                .push(constraint.cache());
        }
        for contact in &contacts {
            let pair = (contact.body_a, contact.body_b);
            if !contact.is_sensor && !next_cache.contains_key(&pair) {
                if let Some(old) = self.cache.get(&pair) {
                    next_cache.insert(pair, old.clone());
                }
            }
        }
        self.cache = next_cache;
        for (id, row) in joint_rows {
            self.joints.get_mut(&id).unwrap().lambda = row.lambda;
        }
        self.finish_sleep(&islands, dt);
        self.emit_events(&contacts);
        for body in self.bodies.iter_mut() {
            body.changed = false;
        }
        let contact_points = contacts.iter().map(|c| c.manifold.points.len()).sum();
        self.contacts = contacts;
        self.previous_dt = dt;
        self.substep_index = self.substep_index.saturating_add(1);
        StepStats {
            broad_phase_pairs: candidates.len(),
            contact_points,
            ..StepStats::default()
        }
    }

    fn build_islands(&self, contacts: &[BodyContact]) -> Vec<Island> {
        let mut parents: Vec<_> = (0..self.bodies.slot_count()).collect();
        let edges = contacts
            .iter()
            .filter(|c| !c.is_sensor)
            .map(|c| (c.body_a, c.body_b))
            .chain(
                self.joints
                    .values()
                    .map(|j| (j.settings.body_a, j.settings.body_b)),
            );
        let edges: Vec<_> = edges.collect();
        for &(aid, bid) in &edges {
            let a = self.bodies.get(aid).unwrap();
            let b = self.bodies.get(bid).unwrap();
            if a.is_dynamic() && b.is_dynamic() {
                let ar = find_root(&mut parents, aid.index() as usize);
                let br = find_root(&mut parents, bid.index() as usize);
                parents[ar.max(br)] = ar.min(br);
            }
        }
        let mut groups: BTreeMap<usize, Island> = BTreeMap::new();
        for body in self.bodies.iter().filter(|body| body.is_dynamic()) {
            let root = find_root(&mut parents, body.id.index() as usize);
            groups.entry(root).or_default().members.push(body.id);
        }
        for (aid, bid) in edges {
            let a = self.bodies.get(aid).unwrap();
            let b = self.bodies.get(bid).unwrap();
            for (dynamic, other) in [(a, b), (b, a)] {
                if dynamic.is_dynamic()
                    && !other.is_dynamic()
                    && (other.changed
                        || (other.motion_type == MotionType::Kinematic
                            && (other.linear_velocity.length_squared() > 1.0e-12
                                || other.angular_velocity.length_squared() > 1.0e-12)))
                {
                    let root = find_root(&mut parents, dynamic.id.index() as usize);
                    groups.get_mut(&root).unwrap().driven = true;
                }
            }
        }
        groups.into_values().collect()
    }

    fn finish_sleep(&mut self, islands: &[Island], dt: f32) {
        if !self.settings.enable_sleeping {
            return;
        }
        for island in islands {
            let mut ready = !island.driven;
            for &id in &island.members {
                let body = self.bodies.get_mut(id).unwrap();
                if body.sleeping {
                    continue;
                }
                let point_speed_bound = body.linear_velocity.length()
                    + body.angular_velocity.length() * body.shape.bounding_radius();
                if body.allow_sleep
                    && point_speed_bound <= self.settings.sleep_velocity_threshold
                    && !island.driven
                {
                    body.sleep_time += dt;
                } else {
                    body.sleep_time = 0.0;
                }
                ready &= body.allow_sleep && body.sleep_time >= self.settings.time_before_sleep;
            }
            if ready {
                for &id in &island.members {
                    let body = self.bodies.get_mut(id).unwrap();
                    body.sleeping = true;
                    body.linear_velocity = Vec3::ZERO;
                    body.angular_velocity = Vec3::ZERO;
                }
            }
        }
    }

    fn emit_events(&mut self, contacts: &[BodyContact]) {
        let mut current = BTreeMap::new();
        for contact in contacts {
            if contact
                .manifold
                .points
                .iter()
                .any(|p| p.separation(contact.manifold.normal) <= self.settings.penetration_slop)
            {
                current.insert((contact.body_a, contact.body_b), contact.is_sensor);
            }
        }
        for (&pair, &sensor) in &current {
            let kind = if self.previous_contacts.contains_key(&pair) {
                ContactEventKind::Persisted
            } else {
                ContactEventKind::Started
            };
            self.events.push(self.event(pair, sensor, kind));
        }
        for (&pair, &sensor) in &self.previous_contacts {
            if !current.contains_key(&pair) {
                self.events
                    .push(self.event(pair, sensor, ContactEventKind::Stopped));
            }
        }
        self.previous_contacts = current;
    }

    pub(super) fn event(
        &self,
        pair: Pair,
        is_sensor: bool,
        kind: ContactEventKind,
    ) -> ContactEvent {
        ContactEvent {
            body_a: pair.0,
            body_b: pair.1,
            kind,
            is_sensor,
            substep_index: self.substep_index,
        }
    }

    pub(super) fn invalidate_cache(&mut self, id: BodyId) {
        self.cache.retain(|pair, _| pair.0 != id && pair.1 != id);
        for joint in self.joints.values_mut() {
            if joint.settings.body_a == id || joint.settings.body_b == id {
                joint.lambda = 0.0;
            }
        }
    }

    pub(super) fn wake_connected(&mut self, start: BodyId) {
        let mut queue = VecDeque::from([start]);
        let mut seen = BTreeSet::new();
        while let Some(id) = queue.pop_front() {
            if !seen.insert(id) {
                continue;
            }
            if let Some(body) = self.bodies.get_mut(id) {
                body.wake();
            }
            let edges = self
                .contacts
                .iter()
                .filter(|c| !c.is_sensor)
                .map(|c| (c.body_a, c.body_b))
                .chain(
                    self.joints
                        .values()
                        .map(|j| (j.settings.body_a, j.settings.body_b)),
                );
            for (a, b) in edges {
                let other = if a == id {
                    Some(b)
                } else if b == id {
                    Some(a)
                } else {
                    None
                };
                if let Some(other) = other {
                    if self.bodies.get(other).is_some_and(Body::is_dynamic) {
                        queue.push_back(other);
                    }
                }
            }
        }
    }
}

#[derive(Default)]
struct Island {
    members: Vec<BodyId>,
    driven: bool,
}

fn active_pair(a: &Body, b: &Body) -> bool {
    (a.is_dynamic() && !a.sleeping) || (b.is_dynamic() && !b.sleeping)
}

fn canonical(a: BodyId, b: BodyId) -> Pair {
    if a < b { (a, b) } else { (b, a) }
}

fn find_root(parents: &mut [usize], mut index: usize) -> usize {
    while parents[index] != index {
        parents[index] = parents[parents[index]];
        index = parents[index];
    }
    index
}
