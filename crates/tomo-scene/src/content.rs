// SPDX-License-Identifier: MIT
//! Renderer-independent entity, camera, HUD and synthesized audio documents.
use crate::{BodySpec, Motion, Problem, Scene, ShapeSpec};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const MAX_ENTITIES: usize = 256;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Transform {
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: f32,
}
impl Default for Transform {
    fn default() -> Self {
        Self {
            position: [0.; 3],
            rotation: [0., 0., 0., 1.],
            scale: 1.,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Visual {
    pub shape: ShapeSpec,
    pub color: [f32; 3],
    pub visible: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entity {
    pub id: String,
    pub name: String,
    pub parent: Option<String>,
    pub body: Option<String>,
    pub transform: Transform,
    pub visual: Option<Visual>,
    pub spin_degrees_per_second: [f32; 3],
    pub tags: Vec<String>,
}
impl Entity {
    pub fn decoration(id: &str, sphere: bool) -> Self {
        Self {
            id: id.into(),
            name: id.into(),
            parent: None,
            body: None,
            transform: Transform::default(),
            visual: Some(Visual {
                shape: if sphere {
                    ShapeSpec::Sphere { radius: 0.5 }
                } else {
                    ShapeSpec::Box {
                        half_extents: [0.5; 3],
                    }
                },
                color: [0.3, 0.7, 0.9],
                visible: true,
            }),
            spin_degrees_per_second: [0.; 3],
            tags: vec![],
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CameraSpec {
    pub id: String,
    pub eye: [f32; 3],
    pub target: [f32; 3],
    pub up: [f32; 3],
    pub vertical_fov_degrees: f32,
    pub near: f32,
    pub far: f32,
    /// Offsets eye and target by this entity's live world position.
    pub follow: Option<String>,
}
impl Default for CameraSpec {
    fn default() -> Self {
        Self {
            id: "main_camera".into(),
            eye: [0., 18., 12.],
            target: [0.; 3],
            up: [0., 1., 0.],
            vertical_fov_degrees: 46.,
            near: 0.1,
            far: 200.,
            follow: None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    Always,
    Ready,
    Playing,
    Paused,
    Won,
    Lost,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiAction {
    Restart,
    Pause,
    Resume,
    Save,
    Load,
    NextScene,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum WidgetKind {
    Label {},
    Score {},
    Timer {},
    Status {},
    Progress { max_score: i32 },
    Button { action: UiAction },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Widget {
    pub id: String,
    pub kind: WidgetKind,
    /// Fraction of the viewport, independent of window resolution.
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub text: String,
    pub color: [f32; 3],
    pub background: [f32; 3],
    pub visibility: Visibility,
}
impl Widget {
    pub fn label(id: &str) -> Self {
        Self {
            id: id.into(),
            kind: WidgetKind::Label {},
            position: [0.03, 0.04],
            size: [0.3, 0.06],
            text: "MY GAME".into(),
            color: [0.9, 0.95, 1.],
            background: [0.02, 0.04, 0.06],
            visibility: Visibility::Always,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum CueTrigger {
    Start {},
    Score {},
    Won {},
    Lost {},
    Rule { id: String },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SoundCue {
    pub id: String,
    pub trigger: CueTrigger,
    pub frequency: f32,
    pub seconds: f32,
    pub volume: f32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Content {
    pub schema_version: u32,
    pub entities: Vec<Entity>,
    pub cameras: Vec<CameraSpec>,
    pub active_camera: String,
    pub default_hud: bool,
    pub ui: Vec<Widget>,
    pub sounds: Vec<SoundCue>,
}
impl Default for Content {
    fn default() -> Self {
        Self {
            schema_version: 1,
            entities: vec![],
            cameras: vec![CameraSpec::default()],
            active_camera: "main_camera".into(),
            default_hud: true,
            ui: vec![],
            sounds: vec![],
        }
    }
}
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
}
fn finite(v: &[f32], max: f32) -> bool {
    v.iter().all(|n| n.is_finite() && n.abs() <= max)
}
fn color(v: &[f32; 3]) -> bool {
    v.iter().all(|n| n.is_finite() && (0. ..=1.).contains(n))
}
fn fail(path: &str, message: &str) -> Problem {
    Problem::new("CONTENT", path, message)
}
fn unique<'a>(ids: impl Iterator<Item = &'a str>, path: &str) -> Result<(), Problem> {
    let mut seen = BTreeSet::new();
    for id in ids {
        if !valid_id(id) || !seen.insert(id) {
            return Err(fail(
                path,
                "IDs must be unique 1..64 ASCII letters/digits/_/-",
            ));
        }
    }
    Ok(())
}
impl Transform {
    pub fn validate(&self) -> Result<(), Problem> {
        let norm = self.rotation.iter().map(|v| v * v).sum::<f32>();
        if !finite(&self.position, 10000.)
            || !finite(&self.rotation, 1.)
            || (norm - 1.).abs() > 0.001
            || !self.scale.is_finite()
            || !(0.001..=100.).contains(&self.scale)
        {
            return Err(fail(
                "/transform",
                "invalid position, unit quaternion or uniform scale",
            ));
        }
        Ok(())
    }
}
impl Content {
    pub fn validate(&self, scene: &Scene) -> Result<(), Problem> {
        if self.schema_version != 1 {
            return Err(Problem::new(
                "VERSION",
                "/content/schema_version",
                "only content v1 is supported",
            ));
        }
        if self.entities.len() > MAX_ENTITIES
            || self.cameras.is_empty()
            || self.cameras.len() > 16
            || self.ui.len() > 32
            || self.sounds.len() > 32
        {
            return Err(fail(
                "/content",
                "limits: 256 entities, 1..16 cameras, 32 widgets and 32 sounds",
            ));
        }
        unique(
            self.entities.iter().map(|e| e.id.as_str()),
            "/content/entities",
        )?;
        unique(
            self.cameras.iter().map(|e| e.id.as_str()),
            "/content/cameras",
        )?;
        unique(self.ui.iter().map(|e| e.id.as_str()), "/content/ui")?;
        unique(self.sounds.iter().map(|e| e.id.as_str()), "/content/sounds")?;
        for e in &self.entities {
            e.transform.validate()?;
            if e.name.trim().is_empty()
                || e.name.len() > 128
                || e.name.chars().any(char::is_control)
                || !finite(&e.spin_degrees_per_second, 360.)
                || e.tags.len() > 8
                || e.tags.iter().any(|t| !valid_id(t))
            {
                return Err(fail("/content/entities", "invalid name, spin or tags"));
            }
            if let Some(body) = &e.body {
                if e.parent.is_some() || !scene.bodies.iter().any(|b| &b.id == body) {
                    return Err(fail(
                        "/content/entities/body",
                        "body bindings must be root entities referencing an existing body",
                    ));
                }
            }
            let mut parent = e.parent.as_deref();
            let mut seen = BTreeSet::from([e.id.as_str()]);
            let mut scale = e.transform.scale;
            for _ in 0..=MAX_ENTITIES {
                let Some(id) = parent else { break };
                if !seen.insert(id) {
                    return Err(fail(
                        "/content/entities/parent",
                        "entity hierarchy contains a cycle",
                    ));
                }
                let p = self
                    .entities
                    .iter()
                    .find(|p| p.id == id)
                    .ok_or_else(|| fail("/content/entities/parent", "unknown parent"))?;
                scale *= p.transform.scale;
                if !(0.000001..=100.).contains(&scale) {
                    return Err(fail(
                        "/content/entities/parent",
                        "combined hierarchy scale is out of range",
                    ));
                }
                parent = p.parent.as_deref();
            }
            if let Some(v) = &e.visual {
                if !color(&v.color) {
                    return Err(fail(
                        "/content/entities/visual/color",
                        "expected linear RGB in 0..1",
                    ));
                }
                let mut test = scene.clone();
                test.checks.clear();
                test.bodies = vec![BodySpec {
                    id: "visual".into(),
                    motion: Motion::Static,
                    shape: v.shape.clone(),
                    position: [0.; 3],
                    rotation: [0., 0., 0., 1.],
                    mass: 1.,
                    linear_velocity: [0.; 3],
                    angular_velocity: [0.; 3],
                    friction: 0.5,
                    restitution: 0.,
                    is_sensor: false,
                    allow_sleep: true,
                }];
                test.validate()?;
            }
        }
        if !self.cameras.iter().any(|c| c.id == self.active_camera) {
            return Err(fail("/content/active_camera", "unknown camera"));
        }
        for c in &self.cameras {
            let forward = [
                c.target[0] - c.eye[0],
                c.target[1] - c.eye[1],
                c.target[2] - c.eye[2],
            ];
            let cross = [
                forward[1] * c.up[2] - forward[2] * c.up[1],
                forward[2] * c.up[0] - forward[0] * c.up[2],
                forward[0] * c.up[1] - forward[1] * c.up[0],
            ];
            if !finite(&c.eye, 10000.)
                || !finite(&c.target, 10000.)
                || !finite(&c.up, 1.)
                || cross.iter().map(|v| v * v).sum::<f32>() < 0.000001
                || !c.vertical_fov_degrees.is_finite()
                || !(5. ..=150.).contains(&c.vertical_fov_degrees)
                || !c.near.is_finite()
                || !c.far.is_finite()
                || !(0.001..=100.).contains(&c.near)
                || c.far - c.near < 0.001
                || c.far > 30000.
            {
                return Err(fail("/content/cameras", "invalid perspective camera"));
            }
            if c.follow
                .as_ref()
                .is_some_and(|id| !self.entities.iter().any(|e| e.id == *id))
            {
                return Err(fail("/content/cameras/follow", "unknown entity"));
            }
        }
        if self.ui.iter().map(|w| w.text.len() + 24).sum::<usize>() > 1536 {
            return Err(fail("/content/ui", "combined HUD text budget exceeded"));
        }
        for w in &self.ui {
            if !finite(&w.position, 1.)
                || w.position.iter().any(|v| *v < 0.)
                || !finite(&w.size, 1.)
                || w.size.iter().any(|v| *v <= 0.)
                || (0..2).any(|i| w.position[i] + w.size[i] > 1.001)
                || !color(&w.color)
                || !color(&w.background)
                || w.text.len() > 48
                || !w
                    .text
                    .bytes()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || b" .:/-+".contains(&c))
                || matches!(w.kind,WidgetKind::Progress{max_score} if max_score<=0)
            {
                return Err(fail(
                    "/content/ui",
                    "invalid HUD rectangle, color, ASCII text or progress maximum",
                ));
            }
        }
        for s in &self.sounds {
            if !s.frequency.is_finite()
                || !(20. ..=12000.).contains(&s.frequency)
                || !s.seconds.is_finite()
                || !(0.01..=5.).contains(&s.seconds)
                || !s.volume.is_finite()
                || !(0. ..=1.).contains(&s.volume)
            {
                return Err(fail(
                    "/content/sounds",
                    "invalid tone: frequency 20..12000 Hz, duration .01..5 s, volume 0..1",
                ));
            }
        }
        Ok(())
    }
}
