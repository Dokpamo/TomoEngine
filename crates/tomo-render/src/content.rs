// SPDX-License-Identifier: MIT
//! Convert CPU-only authoring components to a render frame and normalized HUD hit targets.
use crate::{Camera, CaptureOptions, FrameStyle, OverlayRect, OverlayText};
use std::collections::BTreeMap;
use tomo_runtime::Snapshot;
use tomo_scene::{Content, Problem, ShapeSpec, Transform, UiAction, Visibility, WidgetKind};

#[derive(Clone)]
pub struct VisualInstance {
    pub id: String,
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub shape: ShapeSpec,
    pub color: [f32; 3],
}
#[derive(Clone, Copy)]
pub struct HudState {
    pub score: i32,
    pub seconds: f64,
    pub status: Visibility,
    pub paused: bool,
}
fn rotate(v: [f32; 3], q: [f32; 4]) -> [f32; 3] {
    let [x, y, z, w] = q;
    let t = [
        2. * (y * v[2] - z * v[1]),
        2. * (z * v[0] - x * v[2]),
        2. * (x * v[1] - y * v[0]),
    ];
    [
        v[0] + w * t[0] + y * t[2] - z * t[1],
        v[1] + w * t[1] + z * t[0] - x * t[2],
        v[2] + w * t[2] + x * t[1] - y * t[0],
    ]
}
fn multiply(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let [x, y, z, w] = a;
    let [i, j, k, l] = b;
    let mut q = [
        w * i + x * l + y * k - z * j,
        w * j - x * k + y * l + z * i,
        w * k + x * j - y * i + z * l,
        w * l - x * i - y * j - z * k,
    ];
    let n = q.iter().map(|v| v * v).sum::<f32>().sqrt();
    for v in &mut q {
        *v /= n;
    }
    q
}
fn compose(a: &Transform, b: &Transform) -> Transform {
    let delta = rotate(b.position.map(|v| v * a.scale), a.rotation);
    Transform {
        position: std::array::from_fn(|i| a.position[i] + delta[i]),
        rotation: multiply(a.rotation, b.rotation),
        scale: a.scale * b.scale,
    }
}
pub fn entity_transforms(
    content: &Content,
    snapshot: &Snapshot,
    seconds: f64,
) -> Result<BTreeMap<String, Transform>, Problem> {
    if !seconds.is_finite() || seconds < 0. || content.entities.len() > tomo_scene::MAX_ENTITIES {
        return Err(Problem::new(
            "RENDER_INPUT",
            "/content",
            "invalid time or entity count",
        ));
    }
    let mut result: BTreeMap<String, Transform> = BTreeMap::new();
    for _ in 0..content.entities.len() {
        let before = result.len();
        for e in &content.entities {
            if result.contains_key(&e.id) {
                continue;
            }
            let base = if let Some(id) = &e.parent {
                let Some(p) = result.get(id) else { continue };
                p.clone()
            } else if let Some(id) = &e.body {
                let b = snapshot
                    .bodies
                    .iter()
                    .find(|b| b.id == *id)
                    .ok_or_else(|| {
                        Problem::new("REFERENCE", "/content/body", "missing live body")
                    })?;
                Transform {
                    position: b.position,
                    rotation: b.rotation,
                    scale: 1.,
                }
            } else {
                Transform::default()
            };
            let mut local = e.transform.clone();
            for axis in 0..3 {
                let angle = ((seconds * f64::from(e.spin_degrees_per_second[axis])).rem_euclid(360.)
                    as f32)
                    .to_radians() * 0.5;
                let mut q = [0., 0., 0., angle.cos()];
                q[axis] = angle.sin();
                local.rotation = multiply(local.rotation, q);
            }
            let t = compose(&base, &local);
            if t.position.iter().any(|v| !v.is_finite() || v.abs() > 1.0e7) || !t.scale.is_finite()
            {
                return Err(Problem::new(
                    "RENDER_INPUT",
                    "/content/transform",
                    "world transform out of range",
                ));
            }
            result.insert(e.id.clone(), t);
        }
        if result.len() == before {
            break;
        }
    }
    if result.len() != content.entities.len() {
        return Err(Problem::new(
            "REFERENCE",
            "/content/parent",
            "invalid or cyclic entity hierarchy",
        ));
    }
    Ok(result)
}
pub fn content_camera(
    content: &Content,
    snapshot: &Snapshot,
    seconds: f64,
) -> Result<Camera, Problem> {
    let c = content
        .cameras
        .iter()
        .find(|c| c.id == content.active_camera)
        .ok_or_else(|| Problem::new("REFERENCE", "/content/active_camera", "missing camera"))?;
    let offset = if let Some(id) = &c.follow {
        entity_transforms(content, snapshot, seconds)?
            .get(id)
            .ok_or_else(|| Problem::new("REFERENCE", "/camera/follow", "unknown entity"))?
            .position
    } else {
        [0.; 3]
    };
    Ok(Camera {
        eye: std::array::from_fn(|i| c.eye[i] + offset[i]),
        target: std::array::from_fn(|i| c.target[i] + offset[i]),
        up: c.up,
        vertical_fov_degrees: c.vertical_fov_degrees,
        near: c.near,
        far: c.far,
    })
}
pub fn content_visuals(
    content: &Content,
    snapshot: &Snapshot,
    seconds: f64,
    style: &mut FrameStyle,
) -> Result<(), Problem> {
    let transforms = entity_transforms(content, snapshot, seconds)?;
    for e in &content.entities {
        if let Some(v) = &e.visual {
            if let Some(id) = &e.body {
                style.hidden_bodies.insert(id.clone());
            }
            if !v.visible {
                continue;
            }
            let t = &transforms[&e.id];
            let shape = match v.shape {
                ShapeSpec::Box { half_extents } => ShapeSpec::Box {
                    half_extents: half_extents.map(|v| v * t.scale),
                },
                ShapeSpec::Sphere { radius } => ShapeSpec::Sphere {
                    radius: radius * t.scale,
                },
            };
            style.visuals.push(VisualInstance {
                id: e.id.clone(),
                position: t.position,
                rotation: t.rotation,
                shape,
                color: v.color,
            });
        }
    }
    Ok(())
}
pub fn widget_visible(v: Visibility, state: HudState) -> bool {
    v == Visibility::Always || v == state.status || (v == Visibility::Paused && state.paused)
}
pub fn hud_action(content: &Content, state: HudState, p: [f32; 2]) -> Option<UiAction> {
    content.ui.iter().rev().find_map(|w| {
        if !widget_visible(w.visibility, state)
            || !(0..2).all(|i| p[i] >= w.position[i] && p[i] <= w.position[i] + w.size[i])
        {
            return None;
        }
        if let WidgetKind::Button { action } = w.kind {
            Some(action)
        } else {
            None
        }
    })
}
pub fn content_hud(
    content: &Content,
    state: HudState,
    view: &CaptureOptions,
    style: &mut FrameStyle,
) {
    let dim = [view.width as f32, view.height as f32];
    for w in &content.ui {
        if !widget_visible(w.visibility, state) {
            continue;
        }
        let pos = std::array::from_fn(|i| w.position[i] * dim[i]);
        let size: [f32; 2] = std::array::from_fn(|i| w.size[i] * dim[i]);
        style.rectangles.push(OverlayRect {
            position: pos,
            size,
            color: w.background,
        });
        let text = match w.kind {
            WidgetKind::Score {} => format!("{} {}", w.text, state.score),
            WidgetKind::Timer {} => format!("{} {:.1}", w.text, state.seconds),
            WidgetKind::Status {} => format!(
                "{} {}",
                w.text,
                if state.paused {
                    "PAUSED"
                } else {
                    match state.status {
                        Visibility::Ready => "READY",
                        Visibility::Won => "WON",
                        Visibility::Lost => "LOST",
                        _ => "PLAYING",
                    }
                }
            ),
            WidgetKind::Progress { max_score } => {
                let fraction = (state.score as f32 / max_score.max(1) as f32).clamp(0., 1.);
                style.rectangles.push(OverlayRect {
                    position: pos,
                    size: [size[0] * fraction, size[1]],
                    color: w.color,
                });
                continue;
            }
            _ => w.text.clone(),
        };
        let scale = (size[1] * 0.65 / 7.)
            .min((size[0] - 4.).max(1.) / (text.len().max(1) as f32 * 6.))
            .clamp(0.001, 32.);
        style.text.push(OverlayText {
            position: [pos[0] + 2., pos[1] + size[1] * 0.175],
            scale,
            text,
            color: w.color,
        });
    }
}
