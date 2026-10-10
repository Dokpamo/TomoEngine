// SPDX-License-Identifier: MIT
use std::{collections::BTreeSet, fs::OpenOptions, io::Write, path::Path};

use serde_json::{Value, json};
use tomo_render::CaptureOptions;
use tomo_runtime::Runtime;
use tomo_scene::Problem;

pub(super) fn dispatch(path: &str, args: &[&str]) -> Result<(Value, u8), Problem> {
    let mut ticks = None;
    let mut output = None;
    let mut options = CaptureOptions::default();
    let mut colliders = false;
    let mut remaining = args;
    let mut flags = BTreeSet::new();
    while let Some((&flag, rest)) = remaining.split_first() {
        if !flags.insert(flag) {
            return Err(Problem::new("ARGUMENT", flag, "duplicate capture option"));
        }
        if flag == "--colliders" {
            colliders = true;
            remaining = rest;
            continue;
        }
        let (&value, rest) = rest
            .split_first()
            .ok_or_else(|| Problem::new("ARGUMENT", flag, "missing value"))?;
        match flag {
            "--ticks" => {
                ticks = Some(value.parse::<u32>().map_err(|_| {
                    Problem::new("ARGUMENT", "/ticks", "expected a nonnegative integer")
                })?)
            }
            "--output" => output = Some(value),
            "--view" => {
                options = serde_json::from_str(&super::read_document(value)?)
                    .map_err(|e| Problem::new("JSON", "/view", e.to_string()))?
            }
            _ => return Err(Problem::new("ARGUMENT", flag, "unknown capture option")),
        }
        remaining = rest;
    }
    options.colliders |= colliders;
    let ticks = ticks.ok_or_else(|| {
        Problem::new(
            "ARGUMENT",
            "/ticks",
            "--ticks is required; use 0 for the initial state",
        )
    })?;
    let output = output
        .ok_or_else(|| Problem::new("ARGUMENT", "/output", "--output NEW.png is required"))?;
    if Path::new(output).extension().and_then(|s| s.to_str()) != Some("png")
        || Path::new(output).exists()
    {
        return Err(Problem::new(
            "ARGUMENT",
            "/output",
            "output must be a new .png file; existing files are not overwritten",
        ));
    }
    let mut runtime = Runtime::from_scene(super::load_scene(path)?)?;
    // Reject an invalid view before running physics or contacting a GPU driver.
    tomo_render::project_bodies(runtime.scene(), &runtime.snapshot()?, &options)?;
    let report = if ticks == 0 {
        None
    } else {
        Some(runtime.run(ticks)?)
    };
    let snapshot = runtime.snapshot()?;
    let frame = tomo_render::capture(runtime.scene(), &snapshot, &options)?;
    let png = frame.png()?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .map_err(|e| Problem::new("OUTPUT", output, e.to_string()))?;
    if let Err(error) = file.write_all(&png).and_then(|_| file.sync_all()) {
        drop(file);
        let _ = std::fs::remove_file(output);
        return Err(Problem::new("OUTPUT", output, error.to_string()));
    }
    let passed = report.as_ref().is_none_or(|r| r.passed);
    Ok((
        json!({
            "schema_version": 1, "ok": passed, "image": output,
            "capture": frame.metadata, "snapshot": snapshot,
            "effective_scene": runtime.scene(), "report": report,
        }),
        u8::from(!passed),
    ))
}
