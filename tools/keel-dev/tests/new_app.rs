// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: MIT

//! `keel new` end to end, without building the app (no Qt here): the file
//! set, placeholder substitution, licence headers and the Sailjail section.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const KEEL: &str = env!("CARGO_BIN_EXE_keel");

fn keel(args: &[&str]) -> Output {
    Command::new(KEEL).args(args).output().expect("run keel")
}

fn files(root: &Path) -> Vec<String> {
    fn walk(dir: &Path, root: &Path, out: &mut Vec<String>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, root, out);
            } else {
                out.push(path.strip_prefix(root).unwrap().display().to_string());
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

fn has_placeholder(text: &str) -> bool {
    text.match_indices("{{").any(|(i, _)| {
        let rest = &text[i + 2..];
        let len = rest
            .bytes()
            .take_while(|b| b.is_ascii_lowercase() || *b == b'_')
            .count();
        len > 0 && rest[len..].starts_with("}}")
    })
}

#[test]
fn generates_a_complete_substituted_app() {
    let tmp = tempfile::tempdir().unwrap();
    let dir: PathBuf = tmp.path().join("app");
    let out = keel(&[
        "new",
        "weather-now",
        "--dir",
        dir.to_str().unwrap(),
        "--id",
        "com.example.WeatherNow",
        "--license",
        "Apache-2.0",
        "--copyright",
        "Ada Lovelace",
        "--no-icons",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let expected = [
        ".github/workflows/ci.yml",
        ".gitignore",
        "Cargo.lock",
        "Cargo.toml",
        "LICENSES/Apache-2.0.txt",
        "LICENSES/MIT.txt",
        "README.md",
        "REUSE.toml",
        "build.rs",
        "build/precompile_qml.rs",
        "cpp/launcher.cpp",
        "cpp/launcher.h",
        "icons/render-icons.sh",
        "icons/weather-now.svg",
        "packaging/weather-now.desktop",
        "qml/cover/CoverPage.qml",
        "qml/pages/MainPage.qml",
        "qml/weather-now.qml",
        "rpm/build-in-sdk.sh",
        "rpm/weather-now.spec",
        "rust-toolchain.toml",
        "src/bridge.rs",
        "src/greeter.rs",
        "src/lib.rs",
        "tests/smoke-test.sh",
    ];
    assert_eq!(files(&dir), expected);

    for rel in expected {
        check_header(&dir, rel);
    }

    let desktop = fs::read_to_string(dir.join("packaging/weather-now.desktop")).unwrap();
    assert!(desktop.contains("Exec=/usr/bin/invoker --type=keel -A -- /usr/bin/weather-now\n"));
    assert!(!desktop
        .lines()
        .any(|l| l.starts_with("X-Nemo-Application-Type")));
    assert!(desktop.contains(
        "[X-Sailjail]\nOrganizationName=com.example\nApplicationName=WeatherNow\nPermissions=\n"
    ));
    let spec = fs::read_to_string(dir.join("rpm/weather-now.spec")).unwrap();
    assert!(spec.contains("Name:           weather-now\n"));
    assert!(spec.contains("License:        Apache-2.0\n"));
    assert!(spec.contains("%license LICENSES/Apache-2.0.txt"));
    let page = fs::read_to_string(dir.join("qml/pages/MainPage.qml")).unwrap();
    assert!(page.contains("import com.example.WeatherNow 1.0"));
    let licence = fs::read_to_string(dir.join("LICENSES/Apache-2.0.txt")).unwrap();
    assert!(licence.contains("Apache License"));
    // The CI workflow keeps GitHub's own expression syntax intact.
    let ci = fs::read_to_string(dir.join(".github/workflows/ci.yml")).unwrap();
    assert!(ci.contains("$GITHUB_ENV"));

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for script in [
            "icons/render-icons.sh",
            "rpm/build-in-sdk.sh",
            "tests/smoke-test.sh",
        ] {
            let mode = fs::metadata(dir.join(script)).unwrap().permissions().mode();
            assert_eq!(mode & 0o111, 0o111, "{script} is not executable");
        }
    }

    // A second run into the same, now non-empty, directory is refused.
    let again = keel(&[
        "new",
        "weather-now",
        "--dir",
        dir.to_str().unwrap(),
        "--no-icons",
    ]);
    assert!(!again.status.success());
    assert!(String::from_utf8_lossy(&again.stderr).contains("not empty"));
}

#[test]
fn defaults_derive_from_the_name() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("x");
    let out = keel(&[
        "new",
        "hello-keel",
        "--dir",
        dir.to_str().unwrap(),
        "--copyright",
        "Example",
        "--no-icons",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(dir.join("LICENSES/MIT.txt").is_file());
    let desktop = fs::read_to_string(dir.join("packaging/hello-keel.desktop")).unwrap();
    assert!(desktop.contains("Name=Hello Keel\n"));
    assert!(desktop.contains("OrganizationName=org.example\nApplicationName=HelloKeel\n"));
}

#[test]
fn rejects_bad_input() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("bad");
    let d = dir.to_str().unwrap();
    for args in [
        vec!["new", "Hello", "--dir", d],
        vec!["new", "hello--keel", "--dir", d],
        vec!["new", "hello", "--dir", d, "--id", "org.example.hello-keel"],
        vec!["new", "hello", "--dir", d, "--license", "Beerware"],
        vec!["new", "hello", "--dir", d, "--title", "Say \"hi\""],
    ] {
        let out = keel(&args);
        assert!(!out.status.success(), "{args:?} was accepted");
        assert!(!dir.exists(), "{args:?} wrote files");
    }
}

/// A generated file has no placeholder or template licence left, and the
/// app's licence header (Keel's own MIT one for the copied launcher code).
fn check_header(dir: &Path, rel: &str) {
    let text = fs::read_to_string(dir.join(rel)).unwrap();
    assert!(!has_placeholder(&text), "{rel}: placeholder left");
    // REUSE-IgnoreStart
    assert!(
        !text.contains("SPDX-License-Identifier: MIT-0"),
        "{rel}: template licence left"
    );
    // REUSE-IgnoreEnd
    if rel == "build/precompile_qml.rs" {
        // Keel's launcher code, copied with its own MIT notice.
        // REUSE-IgnoreStart
        assert!(
            text.contains("SPDX-License-Identifier: MIT") && !text.contains("Ada Lovelace"),
            "{rel}: keeps Keel's licence header"
        );
        // REUSE-IgnoreEnd
    } else if rel != "Cargo.lock" && !rel.starts_with("LICENSES/") {
        // REUSE-IgnoreStart
        assert!(
            text.contains("SPDX-FileCopyrightText: ")
                && text.contains("Ada Lovelace")
                && text.contains("SPDX-License-Identifier: Apache-2.0"),
            "{rel}: licence header"
        );
        // REUSE-IgnoreEnd
    }
}
