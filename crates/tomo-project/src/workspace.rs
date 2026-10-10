// SPDX-License-Identifier: MIT
use crate::Project;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use tomo_game::GameRules;
use tomo_scene::{BodySpec, Content, Entity, Problem, Scene, valid_id};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Level {
    pub id: String,
    pub scene: Scene,
    pub game: GameRules,
    pub content: Content,
    pub next_scene: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Prefab {
    pub id: String,
    pub entities: Vec<Entity>,
    pub bodies: Vec<BodySpec>,
}
fn error(message: &str) -> Problem {
    Problem::new("WORKSPACE", "/project", message)
}
impl Project {
    pub(crate) fn validate_workspace(&self) -> Result<(), Problem> {
        if self.scenes.len() > 15 || self.prefabs.len() > 32 {
            return Err(error("at most 16 scenes and 32 prefabs"));
        }
        let mut ids = BTreeSet::new();
        for id in std::iter::once(&self.scene_id).chain(self.scenes.iter().map(|s| &s.id)) {
            if !valid_id(id) || !ids.insert(id) {
                return Err(error("scene IDs must be valid and unique"));
            }
        }
        let active = Level {
            id: self.scene_id.clone(),
            scene: self.scene.clone(),
            game: self.game.clone(),
            content: self.content.clone(),
            next_scene: self.next_scene.clone(),
        };
        for l in std::iter::once(&active).chain(&self.scenes) {
            l.scene.validate()?;
            l.game.validate(&l.scene)?;
            l.content.validate(&l.scene)?;
            if let Some(next) = &l.next_scene {
                if !ids.contains(next) || next == &l.id {
                    return Err(error(
                        "next_scene must reference a different existing scene",
                    ));
                }
            }
            for cue in &l.content.sounds {
                if let tomo_scene::CueTrigger::Rule { id } = &cue.trigger {
                    if !l.game.logic.rules.iter().any(|r| r.id == *id) {
                        return Err(error("sound references unknown rule"));
                    }
                }
            }
        }
        let mut prefab_ids = BTreeSet::new();
        for p in &self.prefabs {
            if !valid_id(&p.id)
                || !prefab_ids.insert(&p.id)
                || p.entities.is_empty()
                || p.entities.len() > 64
                || p.bodies.len() > 64
            {
                return Err(error(
                    "invalid prefab ID or size (1..64 entities, 0..64 bodies)",
                ));
            }
            let mut scene = self.scene.clone();
            scene.bodies = p.bodies.clone();
            scene.checks.clear();
            scene.validate()?;
            let content = Content {
                entities: p.entities.clone(),
                ..Default::default()
            };
            content.validate(&scene)?;
        }
        Ok(())
    }
    /// Swap the editing scene while retaining every scene document, including unsaved edits.
    pub fn switch_scene(&mut self, id: &str) -> Result<(), Problem> {
        if id == self.scene_id {
            return Ok(());
        }
        let index = self
            .scenes
            .iter()
            .position(|s| s.id == id)
            .ok_or_else(|| error("unknown scene"))?;
        let mut candidate = self.clone();
        let l = &mut candidate.scenes[index];
        std::mem::swap(&mut candidate.scene_id, &mut l.id);
        std::mem::swap(&mut candidate.scene, &mut l.scene);
        std::mem::swap(&mut candidate.game, &mut l.game);
        std::mem::swap(&mut candidate.content, &mut l.content);
        std::mem::swap(&mut candidate.next_scene, &mut l.next_scene);
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }
    pub fn duplicate_scene(&mut self, id: &str) -> Result<(), Problem> {
        let mut c = self.clone();
        c.scenes.push(Level {
            id: id.into(),
            scene: c.scene.clone(),
            game: c.game.clone(),
            content: c.content.clone(),
            next_scene: None,
        });
        c.validate()?;
        *self = c;
        Ok(())
    }
    /// Prefabs are explicit value instances. Editing a template does not mutate prior instances.
    pub fn instantiate(
        &mut self,
        prefab: &str,
        prefix: &str,
        offset: [f32; 3],
    ) -> Result<(), Problem> {
        if !valid_id(prefix) || offset.iter().any(|x| !x.is_finite() || x.abs() > 10000.) {
            return Err(error("invalid instance prefix or offset"));
        }
        let p = self
            .prefabs
            .iter()
            .find(|p| p.id == prefab)
            .ok_or_else(|| error("unknown prefab"))?;
        let mut c = self.clone();
        let remap = |id: &str| format!("{prefix}_{id}");
        for source in &p.bodies {
            let mut b = source.clone();
            b.id = remap(&b.id);
            for (v, d) in b.position.iter_mut().zip(offset) {
                *v += d;
            }
            c.scene.bodies.push(b);
        }
        for source in &p.entities {
            let mut e = source.clone();
            e.id = remap(&e.id);
            e.parent = e.parent.as_deref().map(remap);
            e.body = e.body.as_deref().map(remap);
            if e.parent.is_none() && e.body.is_none() {
                for (v, d) in e.transform.position.iter_mut().zip(offset) {
                    *v += d;
                }
            }
            c.content.entities.push(e);
        }
        c.validate()?;
        *self = c;
        Ok(())
    }
    pub fn capture_prefab(&mut self, root: &str, id: &str) -> Result<(), Problem> {
        let mut selected = BTreeSet::from([root.to_string()]);
        if !self.content.entities.iter().any(|e| e.id == root) {
            return Err(error("select an entity first"));
        }
        for _ in 0..self.content.entities.len() {
            for e in &self.content.entities {
                if e.parent.as_ref().is_some_and(|p| selected.contains(p)) {
                    selected.insert(e.id.clone());
                }
            }
        }
        let mut entities: Vec<_> = self
            .content
            .entities
            .iter()
            .filter(|e| selected.contains(&e.id))
            .cloned()
            .collect();
        let root = entities
            .iter_mut()
            .find(|e| e.id == root)
            .expect("selected root");
        if root.parent.is_some() {
            return Err(error("capture a root entity (detach from parent first)"));
        }
        let bodies = self
            .scene
            .bodies
            .iter()
            .filter(|b| entities.iter().any(|e| e.body.as_deref() == Some(&b.id)))
            .cloned()
            .collect();
        let mut c = self.clone();
        c.prefabs.push(Prefab {
            id: id.into(),
            entities,
            bodies,
        });
        c.validate()?;
        *self = c;
        Ok(())
    }
}
