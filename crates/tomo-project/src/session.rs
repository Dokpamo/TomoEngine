// SPDX-License-Identifier: MIT
use crate::Project;
use serde::{Deserialize, Serialize};
use std::ops::{Deref, DerefMut};
use tomo_game::{Checkpoint, Game};
use tomo_scene::Problem;

pub struct Session {
    project: Project,
    game: Game,
}
impl Deref for Session {
    type Target = Game;
    fn deref(&self) -> &Game {
        &self.game
    }
}
impl DerefMut for Session {
    fn deref_mut(&mut self) -> &mut Game {
        &mut self.game
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Save {
    schema_version: u32,
    project: Project,
    checkpoint: Checkpoint,
}
impl Session {
    pub fn new(project: Project) -> Result<Self, Problem> {
        let game = project.game()?;
        Ok(Self { project, game })
    }
    pub fn project(&self) -> &Project {
        &self.project
    }
    pub fn next_scene(&mut self) -> Result<(), Problem> {
        let id = self
            .project
            .next_scene
            .clone()
            .ok_or_else(|| Problem::new("SCENE", "/next_scene", "no next scene configured"))?;
        self.load_scene(&id)
    }
    pub fn load_scene(&mut self, id: &str) -> Result<(), Problem> {
        let mut project = self.project.clone();
        project.switch_scene(id)?;
        let game = project.game()?;
        self.project = project;
        self.game = game;
        Ok(())
    }
    pub fn save_json(&self) -> Result<String, Problem> {
        let save = Save {
            schema_version: 1,
            project: self.project.clone(),
            checkpoint: self.game.checkpoint()?,
        };
        let json = serde_json::to_string_pretty(&save)
            .map_err(|e| Problem::new("JSON", "/save", e.to_string()))?;
        if json.len() > tomo_scene::MAX_DOCUMENT_BYTES {
            return Err(Problem::new("LIMIT", "/save", "save exceeds 1 MiB"));
        }
        Ok(json)
    }
    pub fn restore_json(&mut self, json: &str) -> Result<(), Problem> {
        if json.len() > tomo_scene::MAX_DOCUMENT_BYTES {
            return Err(Problem::new("LIMIT", "/save", "save exceeds 1 MiB"));
        }
        let save: Save =
            serde_json::from_str(json).map_err(|e| Problem::new("JSON", "/save", e.to_string()))?;
        if save.schema_version != 1 {
            return Err(Problem::new(
                "VERSION",
                "/save",
                "only save v1 is supported",
            ));
        }
        save.project.validate()?;
        let mut expected = self.project.clone();
        expected.switch_scene(&save.project.scene_id)?;
        if expected != save.project
            || !save
                .checkpoint
                .matches(&expected.scene, &expected.game, &expected.content)
        {
            return Err(Problem::new(
                "SAVE_PROJECT",
                "/save",
                "save belongs to a different project revision",
            ));
        }
        let game = save.checkpoint.restore()?;
        self.project = expected;
        self.game = game;
        Ok(())
    }
    pub fn save_progress(&self, path: impl AsRef<std::path::Path>) -> Result<(), Problem> {
        let path = path.as_ref();
        if std::fs::symlink_metadata(path).is_ok() {
            let old: Save = serde_json::from_str(&crate::storage::read(path)?)
                .map_err(|e| Problem::new("SAVE_PROJECT", "/save", e.to_string()))?;
            let mut expected = self.project.clone();
            expected.switch_scene(&old.project.scene_id)?;
            if old.schema_version != 1 || old.project != expected {
                return Err(Problem::new(
                    "SAVE_PROJECT",
                    "/save",
                    "existing slot belongs to another project revision; choose a different slot",
                ));
            }
        }
        crate::storage::replace_data(path, &self.save_json()?)
    }
    pub fn load_progress(&mut self, path: impl AsRef<std::path::Path>) -> Result<(), Problem> {
        self.restore_json(&crate::storage::read(path.as_ref())?)
    }
}
