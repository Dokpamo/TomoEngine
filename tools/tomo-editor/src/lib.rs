// SPDX-License-Identifier: MIT
//! Editor session: isolated play world plus a versioned authoring document.
#![forbid(unsafe_code)]
mod logic_ui;
pub mod ui;
use tomo_game::{Game, Input};
use tomo_project::{Document, Project, ProjectFile};
use tomo_scene::Problem;

pub struct Editor {
    pub document: Document,
    pub file: Option<ProjectFile>,
    pub selected: Option<String>,
    pub game: Option<Game>,
    pub paused: bool,
    pub failed: bool,
    pub message: String,
    pub error: bool,
}
impl Editor {
    pub fn new(file: Option<ProjectFile>) -> Result<Self, Problem> {
        let project = match &file {
            Some(file) => file.project().clone(),
            None => Project::template("My Game")?,
        };
        let selected = Some(project.game.player.clone());
        Ok(Self {
            document: Document::new(project)?,
            file,
            selected,
            game: None,
            paused: false,
            failed: false,
            message: "Select a body to edit. Play to test your level.".into(),
            error: false,
        })
    }
    pub fn dirty(&self) -> bool {
        self.file
            .as_ref()
            .is_none_or(|f| f.is_dirty(self.document.project()))
    }
    pub fn report(&mut self, result: Result<(), Problem>, success: &str) {
        match result {
            Ok(()) => {
                self.message = success.into();
                self.error = false;
            }
            Err(e) => {
                self.message = format!("{}: {} ({})", e.code, e.message, e.path);
                self.error = true;
            }
        }
    }
    pub fn start(&mut self) -> Result<(), Problem> {
        self.document.end_gesture();
        let game = self.document.project().game()?;
        self.game = Some(game);
        self.paused = false;
        self.failed = false;
        Ok(())
    }
    pub fn stop(&mut self) {
        self.game = None;
        self.paused = false;
        self.failed = false;
    }
    pub fn step(&mut self, input: Input) -> Result<(), Problem> {
        if self.failed {
            return Err(Problem::new(
                "FAILED_WORLD",
                "/play",
                "stop and rebuild the failed preview",
            ));
        }
        if self.paused {
            return Ok(());
        }
        if let Some(game) = &mut self.game {
            if let Err(e) = game.step(input) {
                self.failed = true;
                self.paused = true;
                return Err(e);
            }
        }
        Ok(())
    }
    pub fn save(&mut self) -> Result<(), Problem> {
        self.document.end_gesture();
        self.file
            .as_mut()
            .ok_or_else(|| Problem::new("PROJECT", "/path", "choose a new project folder first"))?
            .save(self.document.project())
    }
}
