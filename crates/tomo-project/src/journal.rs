// SPDX-License-Identifier: MIT
//! Recoverable local document transactions. No scripts, providers or remote operations.
use crate::{Document, Project, ProjectFile};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};
use tomo_scene::Problem;
const MAX_JOURNAL_BYTES: u64 = 4 * 1024 * 1024;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditRequest {
    pub schema_version: u32,
    pub request_id: String,
    pub expected_revision: u64,
    pub candidate: Project,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Phase {
    Prepared,
    Applied,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    schema_version: u32,
    phase: Phase,
    request: EditRequest,
    before: Project,
    after: Project,
}
#[derive(Debug, Serialize)]
pub struct EditResult {
    pub schema_version: u32,
    pub request_id: String,
    pub replayed: bool,
    pub applied_revision: u64,
    pub current_revision: u64,
    pub journal: PathBuf,
}
fn error(path: &Path, e: impl std::fmt::Display) -> Problem {
    Problem::new("JOURNAL", path.display().to_string(), e.to_string())
}
fn read_record(path: &Path) -> Result<Record, Problem> {
    let metadata = fs::symlink_metadata(path).map_err(|e| error(path, e))?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > MAX_JOURNAL_BYTES
    {
        return Err(error(path, "expected regular journal <=4 MiB"));
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|e| error(path, e))?
        .take(MAX_JOURNAL_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| error(path, e))?;
    if bytes.len() as u64 > MAX_JOURNAL_BYTES {
        return Err(error(path, "journal exceeds limit"));
    }
    let r: Record = serde_json::from_slice(&bytes).map_err(|e| error(path, e))?;
    if r.schema_version != 1 {
        return Err(error(path, "unknown journal version"));
    }
    r.request.validate()?;
    r.before.validate()?;
    r.after.validate()?;
    let mut d = Document::new(r.before.clone())?;
    d.apply(r.request.candidate.clone(), r.request.expected_revision)?;
    if d.project() != &r.after {
        return Err(error(path, "journal candidate/revision mismatch"));
    }
    Ok(r)
}
fn write_record(path: &Path, record: &Record, new: bool) -> Result<(), Problem> {
    let bytes = serde_json::to_vec_pretty(record).map_err(|e| error(path, e))?;
    if bytes.len() as u64 > MAX_JOURNAL_BYTES {
        return Err(error(path, "journal exceeds 4 MiB"));
    }
    let output = if new {
        path.to_path_buf()
    } else {
        path.with_extension("tmp")
    };
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output)
        .map_err(|e| error(&output, e))?;
    if let Err(e) = f.write_all(&bytes).and_then(|_| f.sync_all()) {
        drop(f);
        let _ = fs::remove_file(&output);
        return Err(error(&output, e));
    }
    drop(f);
    if !new {
        fs::rename(&output, path).map_err(|e| error(path, e))?;
    }
    Ok(())
}
struct Lock(PathBuf);
impl Drop for Lock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
fn directory(file: &ProjectFile) -> Result<PathBuf, Problem> {
    let path = file
        .path()
        .parent()
        .unwrap_or(Path::new("."))
        .join(".tomo-journal");
    match fs::create_dir(&path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(error(&path, e)),
    }
    let m = fs::symlink_metadata(&path).map_err(|e| error(&path, e))?;
    if !m.is_dir() || m.file_type().is_symlink() {
        return Err(error(&path, "journal must be a real directory"));
    }
    Ok(path)
}
impl EditRequest {
    pub fn from_json(text: &str) -> Result<Self, Problem> {
        if text.len() > tomo_scene::MAX_DOCUMENT_BYTES {
            return Err(Problem::new(
                "LIMIT",
                "/request",
                "edit request exceeds 1 MiB",
            ));
        }
        let r: Self = serde_json::from_str(text)
            .map_err(|e| Problem::new("JSON", "/request", e.to_string()))?;
        r.validate()?;
        Ok(r)
    }
    fn validate(&self) -> Result<(), Problem> {
        if self.schema_version != 1 || !tomo_scene::valid_id(&self.request_id) {
            return Err(Problem::new(
                "REQUEST",
                "/request",
                "expected request v1 and a valid ID",
            ));
        }
        self.candidate.validate()?;
        self.candidate.to_json()?;
        Ok(())
    }
}
/// Retry the same ID with identical data to recover a prepared transaction or return its receipt.
pub fn apply_edit(
    directory_path: impl AsRef<Path>,
    request: &EditRequest,
) -> Result<EditResult, Problem> {
    request.validate()?;
    let mut file = ProjectFile::open(directory_path)?;
    let directory = directory(&file)?;
    let lock_path = directory.join("writer.lock");
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path)
        .map_err(|e| error(&lock_path, e))?;
    let _lock = Lock(lock_path);
    let path = directory.join(format!("{}.json", request.request_id));
    let (mut record, replayed) = if path.try_exists().map_err(|e| error(&path, e))? {
        let record = read_record(&path)?;
        if record.request != *request {
            return Err(Problem::new(
                "CONFLICT",
                "/request_id",
                "request ID already has different data",
            ));
        }
        (record, true)
    } else {
        if fs::read_dir(&directory)
            .map_err(|e| error(&directory, e))?
            .take(66)
            .count()
            > 64
        {
            return Err(error(
                &directory,
                "journal full; archive completed receipts explicitly before adding requests",
            ));
        }
        let before = file.project().clone();
        let mut doc = Document::new(before.clone())?;
        doc.apply(request.candidate.clone(), request.expected_revision)?;
        let record = Record {
            schema_version: 1,
            phase: Phase::Prepared,
            request: request.clone(),
            before,
            after: doc.project().clone(),
        };
        write_record(&path, &record, true)?;
        (record, false)
    };
    if record.phase == Phase::Prepared {
        if file.project() == &record.before {
            file.save(&record.after)?;
        } else if file.project() != &record.after {
            return Err(Problem::new(
                "CONFLICT",
                "/journal",
                "prepared edit conflicts with current project; neither document was overwritten",
            ));
        }
        record.phase = Phase::Applied;
        write_record(&path, &record, false)?;
    }
    Ok(EditResult {
        schema_version: 1,
        request_id: request.request_id.clone(),
        replayed,
        applied_revision: record.after.revision,
        current_revision: file.project().revision,
        journal: path,
    })
}
/// Rollback is another monotonic document edit, permitted only while the original result is current.
pub fn rollback(
    directory_path: impl AsRef<Path>,
    original: &str,
    new_id: &str,
    expected_revision: u64,
) -> Result<EditResult, Problem> {
    if !tomo_scene::valid_id(original) || !tomo_scene::valid_id(new_id) {
        return Err(Problem::new("ID", "/request_id", "invalid request ID"));
    }
    let directory_path = directory_path.as_ref();
    let file = ProjectFile::open(directory_path)?;
    let dir = directory(&file)?;
    let record = read_record(&dir.join(format!("{original}.json")))?;
    let request = EditRequest {
        schema_version: 1,
        request_id: new_id.into(),
        expected_revision,
        candidate: record.before.clone(),
    };
    if !dir
        .join(format!("{new_id}.json"))
        .try_exists()
        .map_err(|e| error(&dir, e))?
        && (file.project() != &record.after || expected_revision != record.after.revision)
    {
        return Err(Problem::new(
            "CONFLICT",
            "/rollback",
            "intervening edits prevent this rollback",
        ));
    }
    apply_edit(directory_path, &request)
}
