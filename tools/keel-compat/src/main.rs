// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: MIT

//! keel-compat: reports which imports, Silica types and SailfishApp APIs an app
//! uses, whether Keel covers them, and a score for how much of the app is
//! reachable with Keel without source changes.

mod catalog;
mod report;
mod scan;

use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;
use std::{fs, io};

use scan::Findings;

const USAGE: &str = "\
usage: keel-compat [--json] [--fail-on-blockers] [--exclude <dir>]... <path>...

Scans QML (.qml), JavaScript (.js, only where Silica can reach it), native
(.cpp, .cc, .cxx, .h, .hpp), qmake (.pro, .pri), CMake (CMakeLists.txt,
.cmake), qmldir and .desktop files under each path.

  --json              print the report as JSON instead of text
  --fail-on-blockers  exit with status 1 if any scored item is missing or unknown
  --exclude <dir>     skip this directory, relative to each scanned path
                      (repeatable; e.g. a desktop or MeeGo variant of the UI)";

fn main() -> ExitCode {
    let mut fail_on_blockers = false;
    let mut json = false;
    let mut roots = Vec::new();
    let mut excludes = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--fail-on-blockers" => fail_on_blockers = true,
            "--json" => json = true,
            "--exclude" => {
                let Some(dir) = args.next() else {
                    eprintln!("keel-compat: --exclude needs a directory\n\n{USAGE}");
                    return ExitCode::from(2);
                };
                excludes.push(normalise(Path::new(&dir)));
            }
            "-h" | "--help" => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            flag if flag.starts_with('-') => {
                eprintln!("keel-compat: unknown option {flag}\n\n{USAGE}");
                return ExitCode::from(2);
            }
            path => roots.push(PathBuf::from(path)),
        }
    }
    if roots.is_empty() {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }

    let mut findings = Findings::default();
    for root in &roots {
        let walk = Walk {
            root,
            excludes: &excludes,
        };
        if let Err(err) = walk.scan(root, &mut findings) {
            eprintln!("keel-compat: {}: {err}", root.display());
            return ExitCode::from(2);
        }
    }

    let report = report::Report::new(&findings);
    if json {
        match serde_json::to_string_pretty(&report) {
            Ok(text) => println!("{text}"),
            Err(err) => {
                eprintln!("keel-compat: {err}");
                return ExitCode::from(2);
            }
        }
    } else {
        print!("{report}");
    }
    if fail_on_blockers && report.has_blockers() {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

/// `./desktop`, `desktop/` and `desktop` name the same directory.
fn normalise(path: &Path) -> PathBuf {
    path.components()
        .filter(|c| !matches!(c, Component::CurDir))
        .collect()
}

struct Walk<'a> {
    root: &'a Path,
    excludes: &'a [PathBuf],
}

impl Walk<'_> {
    fn scan(&self, path: &Path, findings: &mut Findings) -> io::Result<()> {
        let meta = fs::metadata(path)?;
        if meta.is_dir() {
            let relative = normalise(path.strip_prefix(self.root).unwrap_or(path));
            if !relative.as_os_str().is_empty() && self.excludes.contains(&relative) {
                return Ok(());
            }
            let mut entries: Vec<_> = fs::read_dir(path)?.collect::<io::Result<_>>()?;
            entries.sort_by_key(fs::DirEntry::file_name);
            for entry in entries {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name.starts_with('.')
                    || matches!(name.as_ref(), "target" | "build" | "node_modules")
                {
                    continue;
                }
                // Follow symlinks to files only: a symlinked directory would
                // be scanned twice, or forever in a cycle; skip dangling ones.
                if entry.file_type()?.is_symlink()
                    && !fs::metadata(entry.path()).is_ok_and(|m| m.is_file())
                {
                    continue;
                }
                self.scan(&entry.path(), findings)?;
            }
            return Ok(());
        }

        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        match (name, ext) {
            ("qmldir", _) => findings.add_qmldir(&read_lossy(path)?),
            ("build.rs", _) => findings.add_cxxqt_build(&read_lossy(path)?),
            ("CMakeLists.txt", _) | (_, "cmake") => findings.add_build(&read_lossy(path)?, true),
            (_, "pro" | "pri") => findings.add_build(&read_lossy(path)?, false),
            (_, "qml") => findings.add_qml(&read_lossy(path)?),
            (_, "js") => findings.add_js(&read_lossy(path)?),
            (_, "cpp" | "cc" | "cxx" | "h" | "hpp") => findings.add_native(&read_lossy(path)?),
            (_, "desktop") => findings.add_desktop(&read_lossy(path)?),
            _ => {}
        }
        Ok(())
    }
}

fn read_lossy(path: &Path) -> io::Result<String> {
    Ok(String::from_utf8_lossy(&fs::read(path)?).into_owned())
}
