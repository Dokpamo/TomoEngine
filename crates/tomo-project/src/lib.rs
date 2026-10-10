// SPDX-License-Identifier: MIT
//! Versioned game projects, transactional editing and conflict-aware local persistence.
#![forbid(unsafe_code)]
mod storage;
pub use storage::{PROJECT_FILE, ProjectFile};

use serde::{Deserialize, Serialize};
use tomo_game::{Game, GameRules};
use tomo_runtime::{Runtime, Snapshot};
use tomo_scene::{BodySpec, Motion, Problem, Scene, ShapeSpec};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    pub schema_version: u32,
    pub revision: u64,
    pub name: String,
    pub scene: Scene,
    pub game: GameRules,
}

// Preserve the v1 contract, including rejecting unknown fields inside the old game object.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyProject {
    schema_version: u32,
    revision: u64,
    name: String,
    scene: Scene,
    game: LegacyGame,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyGame {
    player: String,
    goal: String,
    speed: f32,
}
impl Project {
    pub fn template(name: &str) -> Result<Self, Problem> {
        let mut scene = Scene::from_json(tomo_game::LEVEL)?;
        scene.name = name.into();
        let project = Self {
            schema_version: 2,
            revision: 0,
            name: name.into(),
            scene,
            game: GameRules::default(),
        };
        project.validate()?;
        Ok(project)
    }
    pub fn from_json(text: &str) -> Result<Self, Problem> {
        if text.len() > tomo_scene::MAX_DOCUMENT_BYTES {
            return Err(Problem::new("LIMIT", "/", "project exceeds 1 MiB"));
        }
        #[derive(Deserialize)]
        struct Header {
            schema_version: u32,
        }
        let json_error = |e: serde_json::Error| Problem::new("JSON", "/", e.to_string());
        let header: Header = serde_json::from_str(text).map_err(json_error)?;
        let project = match header.schema_version {
            1 => {
                let old: LegacyProject = serde_json::from_str(text).map_err(json_error)?;
                debug_assert_eq!(old.schema_version, 1);
                Self {
                    schema_version: 2,
                    revision: old.revision,
                    name: old.name,
                    scene: old.scene,
                    game: GameRules {
                        player: old.game.player,
                        goal: old.game.goal,
                        speed: old.game.speed,
                        logic: tomo_game::Logic::default(),
                    },
                }
            }
            2 => serde_json::from_str(text).map_err(json_error)?,
            _ => {
                return Err(Problem::new(
                    "VERSION",
                    "/schema_version",
                    "only project v1 and v2 are supported",
                ));
            }
        };
        project.validate()?;
        Ok(project)
    }
    pub fn validate(&self) -> Result<(), Problem> {
        if self.schema_version != 2 {
            return Err(Problem::new(
                "VERSION",
                "/schema_version",
                "in-memory projects must be v2; use from_json to migrate v1",
            ));
        }
        if self.name.trim().is_empty()
            || self.name.len() > 128
            || self.name.chars().any(char::is_control)
        {
            return Err(Problem::new(
                "PROJECT",
                "/name",
                "name must be 1..128 bytes without control characters",
            ));
        }
        self.scene.validate()?;
        self.game.validate(&self.scene)?;
        Runtime::from_scene(self.scene.clone())?;
        Ok(())
    }
    pub fn game(&self) -> Result<Game, Problem> {
        self.validate()?;
        Game::from_scene(self.scene.clone(), self.game.clone())
    }
    pub fn to_json(&self) -> Result<String, Problem> {
        self.validate()?;
        let mut text = serde_json::to_string_pretty(self)
            .map_err(|e| Problem::new("JSON", "/", e.to_string()))?;
        text.push('\n');
        if text.len() > tomo_scene::MAX_DOCUMENT_BYTES {
            return Err(Problem::new(
                "LIMIT",
                "/",
                "serialized project exceeds 1 MiB",
            ));
        }
        Ok(text)
    }
}

/// History contains at most 64 complete documents. Play worlds never mutate this document.
pub struct Document {
    project: Project,
    snapshot: Snapshot,
    undo: Vec<Project>,
    redo: Vec<Project>,
    gesture: Option<Project>,
}
impl Document {
    pub fn new(project: Project) -> Result<Self, Problem> {
        project.validate()?;
        let snapshot = Runtime::from_scene(project.scene.clone())?.snapshot()?;
        Ok(Self {
            project,
            snapshot,
            undo: vec![],
            redo: vec![],
            gesture: None,
        })
    }
    pub fn project(&self) -> &Project {
        &self.project
    }
    pub fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty() || self.gesture.as_ref().is_some_and(|p| p != &self.project)
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn begin_gesture(&mut self) {
        if self.gesture.is_none() {
            self.gesture = Some(self.project.clone());
        }
    }
    pub fn end_gesture(&mut self) {
        if let Some(before) = self.gesture.take() {
            if before != self.project {
                Self::push(&mut self.undo, before);
            }
        }
    }
    fn push(history: &mut Vec<Project>, project: Project) {
        if history.len() == 64 {
            history.remove(0);
        }
        history.push(project);
    }
    fn prepare(&self, mut candidate: Project) -> Result<(Project, Snapshot), Problem> {
        candidate.revision = self
            .project
            .revision
            .checked_add(1)
            .ok_or_else(|| Problem::new("LIMIT", "/revision", "project revision overflow"))?;
        candidate.scene.revision =
            self.project.scene.revision.checked_add(1).ok_or_else(|| {
                Problem::new("LIMIT", "/scene/revision", "scene revision overflow")
            })?;
        candidate.validate()?;
        candidate.to_json()?;
        let snapshot = Runtime::from_scene(candidate.scene.clone())?.snapshot()?;
        Ok((candidate, snapshot))
    }
    /// Validate the complete candidate and build its initial world before changing anything.
    pub fn apply(&mut self, candidate: Project, expected_revision: u64) -> Result<bool, Problem> {
        if expected_revision != self.project.revision {
            return Err(Problem::new(
                "CONFLICT",
                "/revision",
                "document changed; read the current revision",
            ));
        }
        if candidate == self.project {
            return Ok(false);
        }
        let (candidate, snapshot) = self.prepare(candidate)?;
        if self.gesture.is_none() {
            Self::push(&mut self.undo, self.project.clone());
        }
        self.redo.clear();
        self.project = candidate;
        self.snapshot = snapshot;
        Ok(true)
    }
    pub fn undo(&mut self) -> Result<bool, Problem> {
        self.end_gesture();
        let Some(previous) = self.undo.last() else {
            return Ok(false);
        };
        let (project, snapshot) = self.prepare(previous.clone())?;
        self.undo.pop();
        Self::push(&mut self.redo, self.project.clone());
        self.project = project;
        self.snapshot = snapshot;
        Ok(true)
    }
    pub fn redo(&mut self) -> Result<bool, Problem> {
        self.end_gesture();
        let Some(next) = self.redo.last() else {
            return Ok(false);
        };
        let (project, snapshot) = self.prepare(next.clone())?;
        self.redo.pop();
        Self::push(&mut self.undo, self.project.clone());
        self.project = project;
        self.snapshot = snapshot;
        Ok(true)
    }
    fn unique_id(&self, prefix: &str) -> String {
        (1..)
            .map(|i| format!("{prefix}_{i}"))
            .find(|id| !self.project.scene.bodies.iter().any(|b| &b.id == id))
            .expect("bounded scene")
    }
    pub fn add(&mut self, sphere: bool) -> Result<String, Problem> {
        let mut candidate = self.project.clone();
        let id = self.unique_id(if sphere { "sphere" } else { "box" });
        candidate.scene.bodies.push(BodySpec {
            id: id.clone(),
            motion: Motion::Static,
            shape: if sphere {
                ShapeSpec::Sphere { radius: 0.5 }
            } else {
                ShapeSpec::Box {
                    half_extents: [0.5; 3],
                }
            },
            position: [0.0, 0.5, 3.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            mass: 1.0,
            linear_velocity: [0.0; 3],
            angular_velocity: [0.0; 3],
            friction: 0.5,
            restitution: 0.0,
            is_sensor: false,
            allow_sleep: true,
        });
        self.apply(candidate, self.project.revision)?;
        Ok(id)
    }
    pub fn duplicate(&mut self, id: &str) -> Result<String, Problem> {
        let mut body = self
            .project
            .scene
            .bodies
            .iter()
            .find(|b| b.id == id)
            .cloned()
            .ok_or_else(|| Problem::new("REFERENCE", "/selection", "body not found"))?;
        body.id = self.unique_id("copy");
        body.position[0] += 1.0;
        let id = body.id.clone();
        let mut candidate = self.project.clone();
        candidate.scene.bodies.push(body);
        self.apply(candidate, self.project.revision)?;
        Ok(id)
    }
    pub fn remove(&mut self, id: &str) -> Result<(), Problem> {
        if self.project.game.player == id || self.project.game.goal == id {
            return Err(Problem::new(
                "REFERENCE",
                "/selection",
                "assign another player or goal before deleting this body",
            ));
        }
        let mut candidate = self.project.clone();
        if !candidate.scene.bodies.iter().any(|b| b.id == id) {
            return Err(Problem::new("REFERENCE", "/selection", "body not found"));
        }
        // Checks are references, not silently discarded. Validation rejects a dangling check.
        candidate.scene.bodies.retain(|b| b.id != id);
        self.apply(candidate, self.project.revision)?;
        Ok(())
    }
}
