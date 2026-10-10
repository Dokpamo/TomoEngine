// SPDX-License-Identifier: MIT
use std::collections::BTreeMap;

use serde::Serialize;
use tomo_runtime::{BodyState, Snapshot};
use tomo_scene::{Motion, Problem, Scene, ShapeSpec};

use crate::{Camera, CaptureOptions, FrameStyle, MAX_DIMENSION};

#[derive(Clone, Debug, Serialize)]
pub struct ProjectedBody {
    pub id: String,
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub shape: ShapeSpec,
    /// Pixel coordinates of the centre, if in the camera frustum. Not an occlusion test.
    pub center_pixel: Option<[f32; 2]>,
}

pub(crate) struct Geometry {
    pub triangles: Vec<u8>,
    pub lines: Vec<u8>,
    pub overlay: Vec<u8>,
    pub bodies: Vec<ProjectedBody>,
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a.into_iter().zip(b).map(|(a, b)| a * b).sum()
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| a[i] - b[i])
}
fn unit(v: [f32; 3]) -> Result<[f32; 3], Problem> {
    let length = dot(v, v).sqrt();
    if !length.is_finite() || length < 1.0e-6 {
        return Err(Problem::new(
            "RENDER_INPUT",
            "/camera",
            "camera direction/up must be nonzero and not parallel",
        ));
    }
    Ok(v.map(|v| v / length))
}

struct Projection<'a> {
    camera: &'a Camera,
    right: [f32; 3],
    up: [f32; 3],
    forward: [f32; 3],
    scale: f32,
    aspect: f32,
}

impl<'a> Projection<'a> {
    fn new(options: &'a CaptureOptions) -> Result<Self, Problem> {
        if options.schema_version != 1 {
            return Err(Problem::new(
                "VERSION",
                "/view/schema_version",
                "only capture view v1 is supported",
            ));
        }
        if !(1..=MAX_DIMENSION).contains(&options.width)
            || !(1..=MAX_DIMENSION).contains(&options.height)
        {
            return Err(Problem::new(
                "RENDER_INPUT",
                "/size",
                "width and height must be 1..2048",
            ));
        }
        let camera = &options.camera;
        if camera
            .eye
            .iter()
            .chain(&camera.target)
            .chain(&camera.up)
            .any(|v| !v.is_finite() || v.abs() > 1.0e6)
            || !camera.vertical_fov_degrees.is_finite()
            || !(1.0..=150.0).contains(&camera.vertical_fov_degrees)
            || !camera.near.is_finite()
            || !camera.far.is_finite()
            || camera.near < 0.001
            || camera.far <= camera.near
            || camera.far > 1.0e6
            || camera.far - camera.near < 0.001
        {
            return Err(Problem::new(
                "RENDER_INPUT",
                "/camera",
                "invalid camera coordinates, field of view or clipping planes",
            ));
        }
        let forward = unit(sub(camera.target, camera.eye))?;
        let right = unit(cross(forward, unit(camera.up)?))?;
        let up = cross(right, forward);
        Ok(Self {
            camera,
            right,
            up,
            forward,
            scale: 1.0 / (0.5 * camera.vertical_fov_degrees.to_radians()).tan(),
            aspect: options.width as f32 / options.height as f32,
        })
    }

    fn clip(&self, position: [f32; 3]) -> [f32; 4] {
        let relative = sub(position, self.camera.eye);
        let depth = dot(relative, self.forward);
        let factor = self.camera.far / (self.camera.far - self.camera.near);
        [
            dot(relative, self.right) * self.scale / self.aspect,
            dot(relative, self.up) * self.scale,
            factor * (depth - self.camera.near),
            depth,
        ]
    }

    fn pixel(&self, position: [f32; 3], options: &CaptureOptions) -> Option<[f32; 2]> {
        let [x, y, z, w] = self.clip(position);
        if w <= 0.0 || x.abs() > w || y.abs() > w || z < 0.0 || z > w {
            None
        } else {
            Some([
                (x / w * 0.5 + 0.5) * options.width as f32,
                (0.5 - y / w * 0.5) * options.height as f32,
            ])
        }
    }
}

fn rotate(v: [f32; 3], q: [f32; 4]) -> [f32; 3] {
    let xyz = [q[0], q[1], q[2]];
    let t = cross(xyz, v).map(|v| 2.0 * v);
    let u = cross(xyz, t);
    std::array::from_fn(|i| v[i] + q[3] * t[i] + u[i])
}

pub fn project_bodies(
    scene: &Scene,
    snapshot: &Snapshot,
    options: &CaptureOptions,
) -> Result<Vec<ProjectedBody>, Problem> {
    scene.validate()?;
    let projection = Projection::new(options)?;
    if snapshot.bodies.len() != scene.bodies.len()
        || !snapshot.simulation_time.is_finite()
        || snapshot.simulation_time < 0.0
    {
        return Err(Problem::new(
            "RENDER_INPUT",
            "/snapshot",
            "snapshot and scene must describe the same bodies with finite time",
        ));
    }
    let mut states = BTreeMap::new();
    for state in &snapshot.bodies {
        let norm: f32 = state.rotation.iter().map(|v| v * v).sum();
        if state
            .position
            .iter()
            .chain(&state.rotation)
            .any(|v| !v.is_finite() || v.abs() > 1.0e6)
            || (norm - 1.0).abs() > 1.0e-3
            || states.insert(&state.id, state).is_some()
        {
            return Err(Problem::new(
                "RENDER_INPUT",
                "/snapshot/bodies",
                "invalid pose or duplicate ID",
            ));
        }
    }
    let mut result = Vec::new();
    for spec in &scene.bodies {
        let state = states.get(&spec.id).ok_or_else(|| {
            Problem::new(
                "RENDER_INPUT",
                "/snapshot/bodies",
                format!("missing body {}", spec.id),
            )
        })?;
        result.push(ProjectedBody {
            id: spec.id.clone(),
            position: state.position,
            rotation: state.rotation,
            shape: spec.shape.clone(),
            center_pixel: projection.pixel(state.position, options),
        });
    }
    result.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(result)
}

struct MeshBuilder<'a> {
    projection: &'a Projection<'a>,
    state: &'a BodyState,
    color: [f32; 3],
    triangles: &'a mut Vec<u8>,
    lines: &'a mut Vec<u8>,
    colliders: bool,
}

impl MeshBuilder<'_> {
    fn vertex(&mut self, local: [f32; 3], normal: [f32; 3], line: bool) {
        let rotated = rotate(local, self.state.rotation);
        let world = std::array::from_fn(|i| rotated[i] + self.state.position[i]);
        let clip = self.projection.clip(world);
        let normal = rotate(normal, self.state.rotation);
        let color = if line { [0.1, 1.0, 0.65] } else { self.color };
        let output = if line {
            &mut self.lines
        } else {
            &mut self.triangles
        };
        for value in clip.into_iter().chain(normal).chain(color) {
            output.extend_from_slice(&value.to_le_bytes());
        }
    }

    fn cuboid(&mut self, half: [f32; 3]) {
        let corners: Vec<[f32; 3]> = (0..8)
            .map(|i| {
                std::array::from_fn(|axis| {
                    if i & (1 << axis) == 0 {
                        -half[axis]
                    } else {
                        half[axis]
                    }
                })
            })
            .collect();
        for (face, normal) in [
            ([0, 4, 6, 2], [-1.0, 0.0, 0.0]),
            ([1, 3, 7, 5], [1.0, 0.0, 0.0]),
            ([0, 1, 5, 4], [0.0, -1.0, 0.0]),
            ([2, 6, 7, 3], [0.0, 1.0, 0.0]),
            ([0, 2, 3, 1], [0.0, 0.0, -1.0]),
            ([4, 5, 7, 6], [0.0, 0.0, 1.0]),
        ] {
            for index in [0, 1, 2, 0, 2, 3] {
                self.vertex(corners[face[index]], normal, false);
            }
        }
        if self.colliders {
            for i in 0..8 {
                for axis in 0..3 {
                    let j = i ^ (1 << axis);
                    if i < j {
                        self.vertex(corners[i], [0.0; 3], true);
                        self.vertex(corners[j], [0.0; 3], true);
                    }
                }
            }
        }
    }

    fn sphere(&mut self, radius: f32) {
        const RINGS: usize = 12;
        const SLICES: usize = 24;
        let point = |ring: usize, slice: usize| {
            let phi = std::f32::consts::PI * ring as f32 / RINGS as f32;
            let theta = std::f32::consts::TAU * slice as f32 / SLICES as f32;
            [phi.sin() * theta.cos(), phi.cos(), phi.sin() * theta.sin()]
        };
        for ring in 0..RINGS {
            for slice in 0..SLICES {
                for (r, s) in [
                    (ring, slice),
                    (ring + 1, slice),
                    (ring + 1, slice + 1),
                    (ring, slice),
                    (ring + 1, slice + 1),
                    (ring, slice + 1),
                ] {
                    let normal = point(r, s);
                    self.vertex(normal.map(|v| v * radius), normal, false);
                }
            }
        }
        if self.colliders {
            for axis in 0..3 {
                for step in 0..48 {
                    for end in [step, step + 1] {
                        let angle = std::f32::consts::TAU * end as f32 / 48.0;
                        let mut v = [0.0; 3];
                        v[(axis + 1) % 3] = radius * angle.cos();
                        v[(axis + 2) % 3] = radius * angle.sin();
                        self.vertex(v, [0.0; 3], true);
                    }
                }
            }
        }
    }
}

pub(crate) fn build(
    scene: &Scene,
    snapshot: &Snapshot,
    options: &CaptureOptions,
) -> Result<Geometry, Problem> {
    build_styled(scene, snapshot, options, &FrameStyle::default())
}

pub(crate) fn build_styled(
    scene: &Scene,
    snapshot: &Snapshot,
    options: &CaptureOptions,
    style: &FrameStyle,
) -> Result<Geometry, Problem> {
    let bodies = project_bodies(scene, snapshot, options)?;
    style.validate(scene)?;
    let projection = Projection::new(options)?;
    let states: BTreeMap<_, _> = snapshot.bodies.iter().map(|s| (&s.id, s)).collect();
    let mut triangles = Vec::new();
    let mut lines = Vec::new();
    for spec in &scene.bodies {
        let color = style
            .body_colors
            .get(&spec.id)
            .copied()
            .unwrap_or(match spec.motion {
                Motion::Static => [0.17, 0.23, 0.32],
                Motion::Kinematic => [0.25, 0.5, 0.9],
                Motion::Dynamic => [0.95, 0.38, 0.09],
            });
        let mut builder = MeshBuilder {
            projection: &projection,
            state: states[&spec.id],
            color,
            triangles: &mut triangles,
            lines: &mut lines,
            colliders: options.colliders,
        };
        match spec.shape {
            ShapeSpec::Box { half_extents } => builder.cuboid(half_extents),
            ShapeSpec::Sphere { radius } => builder.sphere(radius),
        }
    }
    Ok(Geometry {
        triangles,
        lines,
        overlay: style.vertices(options),
        bodies,
    })
}
