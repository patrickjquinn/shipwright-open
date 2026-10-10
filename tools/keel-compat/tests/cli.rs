// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: MIT

use std::process::Command;

use serde_json::Value;

fn run(args: &[&str]) -> (Option<i32>, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_keel-compat"))
        .args(args)
        .output()
        .expect("run keel-compat");
    (out.status.code(), String::from_utf8(out.stdout).unwrap())
}

fn run_json(args: &[&str]) -> Value {
    let mut all = vec!["--json"];
    all.extend_from_slice(args);
    let (code, out) = run(&all);
    assert!(matches!(code, Some(0 | 1)), "exit status {code:?}");
    serde_json::from_str(&out).expect("valid JSON")
}

fn rows<'a>(json: &'a Value, section: &str) -> &'a Vec<Value> {
    json["sections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["title"] == section)
        .unwrap_or_else(|| panic!("no section {section}"))["rows"]
        .as_array()
        .unwrap()
}

fn status<'a>(json: &'a Value, section: &str, name: &str) -> Option<&'a str> {
    rows(json, section)
        .iter()
        .find(|r| r["name"] == name)
        .map(|r| r["status"].as_str().unwrap())
}

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sample-app");
const REGRESSIONS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/corpus-regressions"
);

#[test]
fn reports_fixture_app() {
    let (code, out) = run(&[FIXTURE]);
    assert_eq!(code, Some(0));
    assert!(out.contains("Scanned 1 QML, 1 native, 0 JavaScript, 0 build and 0 desktop files."));
    assert!(out.contains("SailfishApp::main"));
    assert!(out.contains("Blockers: 0"));
    // Everything the sample uses is supported: nothing flagged as partial.
    assert!(out.contains("Partial (reachable, with gaps): 0"));
    assert!(out.contains("supported  Nemo.Notifications"));
    assert!(out.contains("Static tiers: A reached, B reached"));
}

#[test]
fn fail_on_blockers_sets_exit_status() {
    let (code, _) = run(&["--fail-on-blockers", FIXTURE]);
    assert_eq!(code, Some(0));
    let (code, _) = run(&["--fail-on-blockers", REGRESSIONS]);
    assert_eq!(code, Some(1));
}

#[test]
fn missing_path_is_an_error() {
    let (code, _) = run(&["/nonexistent/keel-compat-test"]);
    assert_eq!(code, Some(2));
}

#[test]
fn json_has_score_tiers_and_blockers() {
    let json = run_json(&[FIXTURE]);
    assert_eq!(json["schema"], 2);
    assert_eq!(json["score"], 100);
    assert_eq!(json["tiers"]["A"], "reached");
    assert_eq!(json["tiers"]["B"], "reached");
    assert_eq!(json["partial"].as_array().unwrap().len(), 0);
    assert_eq!(json["tiers"]["C"], "not-measured");
    assert_eq!(json["blockers"].as_array().unwrap().len(), 0);
}

// Each assertion below is a false positive or negative seen on the corpus.
#[test]
fn corpus_regressions() {
    let json = run_json(&["--exclude", "meego", REGRESSIONS]);

    // `import "x.js" as X` is not a module named `as`.
    assert_eq!(status(&json, "QML imports", "as"), None);
    // A module registered by the app's own C++ is not an unknown blocker.
    assert_eq!(status(&json, "QML imports", "harbour.regress"), Some("app"));
    // Silica enum and singleton references count as Silica usage.
    assert_eq!(
        status(&json, "Silica types", "Orientation"),
        Some("supported")
    );
    assert_eq!(
        status(&json, "Silica types", "PageStatus"),
        Some("supported")
    );
    assert_eq!(status(&json, "Silica types", "Screen"), Some("supported"));
    assert_eq!(
        status(&json, "Silica types", "Screen.sizeCategory"),
        Some("missing")
    );
    assert_eq!(status(&json, "Silica types", "Font"), None);
    // JavaScript: Constants.js imports Silica, helpers.js runs in the
    // importer's scope, Unrelated.js is skipped.
    assert_eq!(json["files"]["js"], 2);
    let theme = rows(&json, "Silica types")
        .iter()
        .find(|r| r["name"] == "Theme")
        .unwrap();
    assert_eq!(theme["uses"], 1);
    // qmake sailfishapp integration and platform libraries are reported,
    // not scored; glib is not a platform library.
    assert_eq!(
        status(&json, "Build integration", "CONFIG sailfishapp"),
        Some("missing")
    );
    assert_eq!(
        status(&json, "Build integration", "CONFIG sailfishapp_i18n"),
        Some("missing")
    );
    assert_eq!(
        status(&json, "Build integration", "CONFIG sailfishapp_qml"),
        None
    );
    assert_eq!(
        status(&json, "Build integration", "pkg-config sailfishapp"),
        Some("supported")
    );
    assert_eq!(
        status(&json, "Build integration", "pkg-config mlite5"),
        Some("missing")
    );
    assert_eq!(
        status(&json, "Build integration", "header mlite5"),
        Some("missing")
    );
    assert_eq!(
        status(&json, "Build integration", "pkg-config glib-2.0"),
        None
    );
    // Sailjail permissions come from the [X-Sailjail] section.
    assert_eq!(
        json["sailjail"]["permissions"],
        serde_json::json!(["Internet", "Pictures"])
    );
    // The excluded MeeGo variant contributes nothing.
    assert_eq!(status(&json, "QML imports", "com.nokia.meego"), None);
    // Blockers: only the Silica-only members of an unqualified Screen where
    // it is Qt Quick's (WindowScreen.qml imports QtQuick.Window first; under
    // QtQuick 2.0 harbour-regress.qml's are Silica's), with the source change
    // in the note.
    let blockers = json["blockers"].as_array().unwrap();
    let names: Vec<_> = blockers
        .iter()
        .map(|b| b["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Screen.Large", "Screen.sizeCategory"]);
    assert!(blockers[1]["note"]
        .as_str()
        .unwrap()
        .contains("import Sailfish.Silica 1.0 as S"));
    assert_eq!(json["tiers"]["B"], "blocked");
    // SailfishApp::main is provided by keel/sailfishapp.
    assert_eq!(
        status(&json, "SailfishApp APIs", "SailfishApp::main"),
        Some("supported")
    );
}

#[test]
fn exclude_is_relative_to_the_scanned_path() {
    let json = run_json(&[REGRESSIONS]);
    assert_eq!(
        status(&json, "QML imports", "com.nokia.meego"),
        Some("unknown")
    );
}

#[test]
fn exclude_accepts_dot_and_trailing_slash() {
    let plain = run_json(&["--exclude", "meego", REGRESSIONS]);
    for spelling in ["./meego", "meego/", "./meego/"] {
        let json = run_json(&["--exclude", spelling, REGRESSIONS]);
        assert_eq!(json["sections"], plain["sections"], "--exclude {spelling}");
    }
}

#[cfg(unix)]
#[test]
fn symlinked_directories_are_skipped_and_files_followed() {
    use std::os::unix::fs::symlink;
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("keel-compat-symlinks");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("qml")).unwrap();
    std::fs::write(
        dir.join("qml/Main.qml"),
        "import QtQuick 2.0\nimport Sailfish.Silica 1.0\nPage {}\n",
    )
    .unwrap();
    // A cycle, a second route into the same directory, a dangling link and
    // a link to a file.
    symlink("..", dir.join("qml/loop")).unwrap();
    symlink("qml", dir.join("again")).unwrap();
    symlink("nowhere", dir.join("dangling")).unwrap();
    symlink("qml/Main.qml", dir.join("Linked.qml")).unwrap();
    let (code, out) = run(&[dir.to_str().unwrap()]);
    assert_eq!(code, Some(0), "{out}");
    assert!(out.contains("Scanned 2 QML,"), "{out}");
}
