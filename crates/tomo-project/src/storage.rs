// SPDX-License-Identifier: MIT
use crate::Project;
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};
use tomo_scene::Problem;

pub const PROJECT_FILE: &str = "project.tomo.json";
fn error(path: &Path, e: impl std::fmt::Display) -> Problem {
    Problem::new("IO", path.display().to_string(), e.to_string())
}
fn read(path: &Path) -> Result<String, Problem> {
    let meta = fs::symlink_metadata(path).map_err(|e| error(path, e))?;
    if !meta.is_file() || meta.file_type().is_symlink() {
        return Err(error(
            path,
            "expected a regular project file, not a symlink",
        ));
    }
    let file = File::open(path).map_err(|e| error(path, e))?;
    let mut text = String::new();
    file.take(tomo_scene::MAX_DOCUMENT_BYTES as u64 + 1)
        .read_to_string(&mut text)
        .map_err(|e| error(path, e))?;
    if text.len() > tomo_scene::MAX_DOCUMENT_BYTES {
        return Err(Problem::new(
            "LIMIT",
            path.display().to_string(),
            "project exceeds 1 MiB",
        ));
    }
    Ok(text)
}
fn write_new(path: &Path, text: &str) -> Result<(), Problem> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| error(path, e))?;
    if let Err(e) = file
        .write_all(text.as_bytes())
        .and_then(|_| file.sync_all())
    {
        drop(file);
        let _ = fs::remove_file(path);
        return Err(error(path, e));
    }
    Ok(())
}
/// One atomic document holds both scene and game settings. No project scripts are executed.
/// Save locks coordinate Tomo writers; byte comparison detects external edits already on disk.
pub struct ProjectFile {
    path: PathBuf,
    saved: Project,
    bytes: String,
}
impl ProjectFile {
    pub fn open(directory: impl AsRef<Path>) -> Result<Self, Problem> {
        let directory = directory.as_ref();
        let path = if directory.is_dir() {
            directory.join(PROJECT_FILE)
        } else {
            directory.to_owned()
        };
        let bytes = read(&path)?;
        let saved = Project::from_json(&bytes)?;
        Ok(Self { path, saved, bytes })
    }
    pub fn create(directory: impl AsRef<Path>, project: &Project) -> Result<Self, Problem> {
        let directory = directory.as_ref();
        let bytes = project.to_json()?;
        // Never overwrite or adopt an existing folder implicitly.
        fs::create_dir(directory).map_err(|e| error(directory, e))?;
        let path = directory.join(PROJECT_FILE);
        if let Err(e) = write_new(&path, &bytes) {
            let _ = fs::remove_dir(directory);
            return Err(e);
        }
        Ok(Self {
            path,
            saved: project.clone(),
            bytes,
        })
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn project(&self) -> &Project {
        &self.saved
    }
    pub fn is_dirty(&self, project: &Project) -> bool {
        project != &self.saved
    }
    pub fn save(&mut self, project: &Project) -> Result<(), Problem> {
        let bytes = project.to_json()?;
        let lock_path = self.path.with_extension("json.lock");
        let _lock = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
            .map_err(|e| {
                Problem::new("SAVE_LOCK", lock_path.display().to_string(), e.to_string())
            })?;
        let result = (|| {
            if read(&self.path)? != self.bytes {
                return Err(Problem::new(
                    "CONFLICT",
                    self.path.display().to_string(),
                    "project changed on disk; reopen it or save to a new folder",
                ));
            }
            let temporary = self.path.with_extension("json.tmp");
            write_new(&temporary, &bytes)?;
            if let Err(e) = fs::rename(&temporary, &self.path) {
                let _ = fs::remove_file(&temporary);
                return Err(error(&self.path, e));
            }
            self.bytes = bytes;
            self.saved = project.clone();
            Ok(())
        })();
        drop(_lock);
        let _ = fs::remove_file(&lock_path);
        result
    }
}
