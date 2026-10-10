// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: MIT

//! `keel actions`: the Keel Actions build step (ADR-0018) for an app
//! directory, through `keel-actions-codegen`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use keel_actions_codegen::{missing_sailjail_line, stale, write, write_stubs, Config, Desktop};

pub struct Options {
    pub path: PathBuf,
    pub qml: Vec<PathBuf>,
    pub rust: Vec<PathBuf>,
    pub desktop: Option<PathBuf>,
    pub app_id: Option<String>,
    pub exec: Option<String>,
    pub out: Option<PathBuf>,
    pub tests: Option<PathBuf>,
    pub check: bool,
    pub review: Option<PathBuf>,
}

fn first_desktop(dir: &Path) -> Option<PathBuf> {
    for d in [dir.join("packaging"), dir.to_path_buf()] {
        let mut found: Vec<PathBuf> = fs::read_dir(&d)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "desktop"))
            .collect();
        found.sort();
        if let Some(p) = found.into_iter().next() {
            return Some(p);
        }
    }
    None
}

fn review(path: &Path) -> Result<ExitCode, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let manifest: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let findings = keel_actions_schema::review::review(&manifest, None);
    for f in &findings {
        println!("{}: {f}", path.display());
    }
    if findings.is_empty() {
        println!("{}: passes Reef's review rules", path.display());
        Ok(ExitCode::SUCCESS)
    } else {
        Ok(ExitCode::FAILURE)
    }
}

pub fn run(o: &Options) -> Result<ExitCode, String> {
    if let Some(path) = &o.review {
        return review(path);
    }
    let dir = &o.path;
    let or_default = |given: &[PathBuf], defaults: &[&str]| -> Vec<PathBuf> {
        if given.is_empty() {
            defaults
                .iter()
                .map(|d| dir.join(d))
                .filter(|p| p.exists())
                .take(1)
                .collect()
        } else {
            given.to_vec()
        }
    };
    let config = Config {
        qml: or_default(&o.qml, &["qml", "ui"]),
        rust: or_default(&o.rust, &["src"]),
        desktop: o.desktop.clone().or_else(|| first_desktop(dir)),
        app_id: o.app_id.clone(),
        executable: o.exec.clone(),
    };
    let out_dir = o
        .out
        .clone()
        .unwrap_or_else(|| dir.join("target/keel-actions"));
    let output = match config.generate() {
        Ok(output) => output,
        Err(failed) => {
            eprint!("{}", failed.0);
            return Ok(ExitCode::FAILURE);
        }
    };
    eprint!("{}", output.diagnostics);
    let counts = format!(
        "{}: {} action(s), {} entity type(s), {} shortcut(s){}",
        output.app.app_id,
        output.app.actions.len(),
        output.app.entities.len(),
        output.app.shortcuts.len(),
        if output.app.context { ", context" } else { "" }
    );

    // Activation needs the ExecDBus line in the desktop file.
    let mut ok = true;
    if let Some(desktop_path) = &config.desktop {
        let desktop = Desktop::parse("", &fs::read_to_string(desktop_path).unwrap_or_default());
        let exec = config.executable.clone().or_else(|| desktop.executable());
        if let Some(line) = exec.and_then(|e| missing_sailjail_line(&desktop, &e)) {
            eprintln!(
                "{}: [X-Sailjail] lacks `{line}` (needed to start the app without its window)",
                desktop_path.display()
            );
            ok = !o.check;
        }
    }
    if o.check {
        let stale = stale(&output.artefacts, &out_dir);
        if !stale.is_empty() {
            eprintln!("{}: out of date: {}", out_dir.display(), stale.join(", "));
            return Ok(ExitCode::FAILURE);
        }
        println!("{counts}; {} is up to date", out_dir.display());
    } else {
        let changed = write(&output.artefacts, &out_dir)
            .map_err(|e| format!("{}: {e}", out_dir.display()))?;
        println!(
            "{counts}; wrote {} file(s) to {}",
            changed.len(),
            out_dir.display()
        );
        if let Some(tests) = &o.tests {
            let written = write_stubs(&output.stubs, tests)
                .map_err(|e| format!("{}: {e}", tests.display()))?;
            for name in written {
                println!("test stub: {}", tests.join(name).display());
            }
        }
    }
    Ok(if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}
