// SPDX-License-Identifier: MIT
//! Editor session: isolated play world plus a versioned authoring document.
#![forbid(unsafe_code)]
mod logic_ui;
mod studio_ui;
pub mod ui;
use tomo_game::Input;
use tomo_project::{Document, Project, ProjectFile};
use tomo_scene::Problem;

pub struct Editor {
    pub document: Document,
    pub file: Option<ProjectFile>,
    pub selected: Option<String>,
    pub game: Option<tomo_project::Session>,
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
        let game = tomo_project::Session::new(self.document.project().clone())?;
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

impl Editor {
    pub fn game_action(&mut self, action: tomo_scene::UiAction) -> Result<(), Problem> {
        let path = self
            .file
            .as_ref()
            .map(|f| f.path().with_file_name("progress.tomo.json"));
        let Some(game) = &mut self.game else {
            return Ok(());
        };
        match action {
            tomo_scene::UiAction::Restart => game.restart()?,
            tomo_scene::UiAction::Pause => self.paused = true,
            tomo_scene::UiAction::Resume => {
                self.paused = false;
                game.begin();
            }
            tomo_scene::UiAction::NextScene => game.next_scene()?,
            tomo_scene::UiAction::Save => game.save_progress(path.ok_or_else(|| {
                Problem::new(
                    "SAVE_PATH",
                    "/save",
                    "save the project before saving progress",
                )
            })?)?,
            tomo_scene::UiAction::Load => game.load_progress(path.ok_or_else(|| {
                Problem::new(
                    "SAVE_PATH",
                    "/save",
                    "save the project before loading progress",
                )
            })?)?,
        }
        Ok(())
    }
}

pub fn hud_state(game: &tomo_game::Game, paused: bool) -> tomo_render::HudState {
    tomo_render::HudState {
        score: game.score(),
        seconds: game.seconds(),
        paused,
        status: match game.status() {
            tomo_game::Status::Ready => tomo_scene::Visibility::Ready,
            tomo_game::Status::Playing => tomo_scene::Visibility::Playing,
            tomo_game::Status::Won => tomo_scene::Visibility::Won,
            tomo_game::Status::Lost => tomo_scene::Visibility::Lost,
        },
    }
}
