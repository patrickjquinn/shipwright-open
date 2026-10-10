// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: MIT

//! `keel run`: builds a Keel app and runs it on the desktop in a
//! phone-sized window, against a Keel build.
//!
//! What the app sees (the contract of `keel new`'s cpp/launcher.cpp):
//! `QML_IMPORT_PATH` (Keel's QML modules), `KEEL_SAILFISHAPP_DATADIR` (the
//! project directory, so `SailfishApp::pathToMainQml()` finds
//! `qml/<name>.qml`), `KEEL_RUN_SIZE`, `KEEL_RUN_ORIENTATION`,
//! `KEEL_RUN_SCREENSHOT`, and Keel's own `KEEL_THEME_PIXEL_RATIO` and
//! `KEEL_THEME_COLOR_SCHEME`.

use std::collections::BTreeMap;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::thread::sleep;
use std::time::{Duration, SystemTime};

use crate::devices::Device;

/// Light or dark ambience.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Ambience {
    Dark,
    Light,
}

/// Device orientation, as Silica's `Orientation` names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Orientation {
    Portrait,
    Landscape,
    PortraitInverted,
    LandscapeInverted,
}

impl Orientation {
    fn env_value(self) -> &'static str {
        match self {
            Orientation::Portrait => "portrait",
            Orientation::Landscape => "landscape",
            Orientation::PortraitInverted => "portrait-inverted",
            Orientation::LandscapeInverted => "landscape-inverted",
        }
    }

    fn landscape(self) -> bool {
        matches!(
            self,
            Orientation::Landscape | Orientation::LandscapeInverted
        )
    }
}

/// `keel run` options.
pub struct Options {
    pub path: PathBuf,
    pub keel_build: Option<PathBuf>,
    pub keel_qml: Option<PathBuf>,
    pub shipwright: Option<PathBuf>,
    pub device: String,
    pub size: Option<(u32, u32)>,
    pub pixel_ratio: Option<f64>,
    pub ambience: Ambience,
    pub orientation: Orientation,
    pub scale: Option<f64>,
    pub watch: bool,
    pub screenshot: Option<PathBuf>,
    pub release: bool,
    pub no_compat: bool,
    pub app_args: Vec<OsString>,
}

/// Where Keel's QML modules and SailfishApp library are.
struct Keel {
    qml: PathBuf,
    /// (include dir, library dir) of a Keel build tree; `None` to let the
    /// app find an installed `keel-sailfishapp` through pkg-config.
    sailfishapp: Option<(PathBuf, PathBuf)>,
}

fn log(msg: &str) {
    eprintln!("keel run: {msg}");
}

/// Parses `540x960`.
pub fn parse_size(s: &str) -> Result<(u32, u32), String> {
    let (w, h) = s
        .split_once(['x', 'X'])
        .ok_or_else(|| format!("size {s:?}: expected WIDTHxHEIGHT, e.g. 540x960"))?;
    let w: u32 = w
        .trim()
        .parse()
        .map_err(|_| format!("bad width in {s:?}"))?;
    let h: u32 = h
        .trim()
        .parse()
        .map_err(|_| format!("bad height in {s:?}"))?;
    if !(100..=10_000).contains(&w) || !(100..=10_000).contains(&h) {
        return Err(format!("size {s:?} out of range (100 to 10000 px)"));
    }
    Ok((w, h))
}

/// The `[package] name` of a Cargo manifest.
fn package_name(manifest: &str) -> Option<String> {
    let mut in_package = false;
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_package = line == "[package]";
            continue;
        }
        if in_package {
            if let Some(rest) = line.strip_prefix("name") {
                let value = rest.trim_start().strip_prefix('=')?.trim();
                return Some(value.trim_matches('"').to_owned());
            }
        }
    }
    None
}

/// A value from a CMakeCache.txt (`KEY:TYPE=value`).
fn cmake_cache_value(build: &Path, key: &str) -> Option<PathBuf> {
    let text = fs::read_to_string(build.join("CMakeCache.txt")).ok()?;
    text.lines().find_map(|l| {
        let (k, v) = l.split_once('=')?;
        (k.split(':').next() == Some(key)).then(|| PathBuf::from(v))
    })
}

/// Keel's QML dir and SailfishApp from a Keel CMake build directory.
fn keel_from_build(build: &Path) -> Keel {
    let qml = build.join("qml");
    let lib = build.join("sailfishapp");
    let source = cmake_cache_value(build, "CMAKE_HOME_DIRECTORY");
    let sailfishapp = match source {
        Some(src) if lib.join("libkeel-sailfishapp.so").exists() => {
            Some((src.join("sailfishapp/include"), lib))
        }
        _ => None,
    };
    Keel { qml, sailfishapp }
}

/// The Shipwright checkout: `--shipwright`, `SHIPWRIGHT_DIR`, or the one
/// this tool was built from.
fn shipwright_dir(opts: &Options) -> Option<PathBuf> {
    let built_from = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    [
        opts.shipwright.clone(),
        env::var_os("SHIPWRIGHT_DIR").map(PathBuf::from),
        Some(built_from),
    ]
    .into_iter()
    .flatten()
    .find(|d| d.join("keel/CMakeLists.txt").is_file())
    .and_then(|d| d.canonicalize().ok())
}

fn cache_dir() -> PathBuf {
    env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .unwrap_or_else(env::temp_dir)
        .join("keel-dev")
}

fn run_checked(cmd: &mut Command, what: &str) -> Result<(), String> {
    let status = cmd
        .status()
        .map_err(|e| format!("{what}: cannot start: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{what} failed ({status})"))
    }
}

/// Builds Keel from the Shipwright checkout into the cache (incremental
/// after the first time).
fn build_keel(opts: &Options) -> Result<Keel, String> {
    let src = shipwright_dir(opts).ok_or(
        "no Keel build given and no Shipwright checkout found: pass --keel-build or \
         --keel-qml (or set KEEL_BUILD_DIR / KEEL_QML_DIR), or --shipwright / SHIPWRIGHT_DIR",
    )?;
    let build = cache_dir().join("keel-build");
    if !build.join("CMakeCache.txt").exists() {
        log(&format!(
            "building Keel from {} into {} (first time: several minutes)",
            src.join("keel").display(),
            build.display()
        ));
        let mut cmd = Command::new("cmake");
        cmd.arg("-S")
            .arg(src.join("keel"))
            .arg("-B")
            .arg(&build)
            .arg("-DBUILD_TESTING=OFF");
        if Command::new("ninja")
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
        {
            cmd.args(["-G", "Ninja"]);
        }
        run_checked(&mut cmd, "configuring Keel (cmake)")?;
    }
    run_checked(
        Command::new("cmake").arg("--build").arg(&build),
        "building Keel (cmake --build)",
    )?;
    Ok(keel_from_build(&build))
}

fn locate_keel(opts: &Options) -> Result<Keel, String> {
    let keel = if let Some(build) = opts
        .keel_build
        .clone()
        .or_else(|| env::var_os("KEEL_BUILD_DIR").map(PathBuf::from))
    {
        keel_from_build(&build)
    } else if let Some(qml) = opts
        .keel_qml
        .clone()
        .or_else(|| env::var_os("KEEL_QML_DIR").map(PathBuf::from))
    {
        // A build tree's qml/ also has the library next to it.
        match qml.parent() {
            Some(build) if build.join("CMakeCache.txt").exists() => Keel {
                qml: qml.clone(),
                ..keel_from_build(build)
            },
            _ => Keel {
                qml,
                sailfishapp: None,
            },
        }
    } else {
        build_keel(opts)?
    };
    if !keel.qml.join("Sailfish/Silica/qmldir").is_file() {
        return Err(format!(
            "{} has no Sailfish/Silica/qmldir: not a Keel QML directory",
            keel.qml.display()
        ));
    }
    let qml = keel
        .qml
        .canonicalize()
        .map_err(|e| format!("{}: {e}", keel.qml.display()))?;
    Ok(Keel { qml, ..keel })
}

/// `cargo build` of one package; returns the path of its program: the binary
/// named `package`, or the package's `cdylib`, which is the program of a
/// `keel new` app (a shared library that is also executable, so that Keel's
/// booster can load it; see the app's build.rs). Diagnostics go to the
/// terminal.
fn cargo_build(
    manifest: &Path,
    package: &str,
    release: bool,
    envs: &[(&str, OsString)],
) -> Result<PathBuf, String> {
    let mut cmd = Command::new(env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
    cmd.arg("build")
        .arg("--manifest-path")
        .arg(manifest)
        .args(["-p", package, "--message-format=json-render-diagnostics"])
        .stdout(Stdio::piped());
    // rustup picks the toolchain (rust-toolchain.toml) from the directory.
    if let Some(dir) = manifest.parent() {
        cmd.current_dir(dir);
    }
    if release {
        cmd.arg("--release");
    }
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("cannot start cargo: {e}"))?;
    let mut exe = None;
    if let Some(out) = child.stdout.take() {
        for line in BufReader::new(out).lines().map_while(Result::ok) {
            let Ok(msg) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            if msg["reason"] != "compiler-artifact" {
                continue;
            }
            let kind = |k: &str| {
                msg["target"]["kind"]
                    .as_array()
                    .is_some_and(|ks| ks.iter().any(|x| x == k))
            };
            if kind("bin") && msg["target"]["name"] == package {
                if let Some(path) = msg["executable"].as_str() {
                    exe = Some(PathBuf::from(path));
                }
            } else if kind("cdylib")
                && msg["target"]["name"].as_str() == Some(package.replace('-', "_").as_str())
            {
                let so = msg["filenames"].as_array().and_then(|fs| {
                    fs.iter()
                        .filter_map(|f| f.as_str())
                        .find(|f| Path::new(f).extension().is_some_and(|e| e == "so"))
                });
                if let Some(path) = so {
                    exe = Some(PathBuf::from(path));
                }
            }
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    if !status.success() {
        return Err(format!("cargo build of {package} failed ({status})"));
    }
    exe.ok_or_else(|| format!("cargo built no binary named {package}"))
}

/// The keel-compat binary: `KEEL_COMPAT`, next to this one, on PATH, or
/// built from the Shipwright checkout.
fn keel_compat(opts: &Options) -> Option<PathBuf> {
    if let Some(p) = env::var_os("KEEL_COMPAT") {
        return Some(PathBuf::from(p));
    }
    if let Some(p) = env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.join("keel-compat")))
        .filter(|p| p.is_file())
    {
        return Some(p);
    }
    if let Some(p) = env::var_os("PATH").and_then(|paths| {
        env::split_paths(&paths)
            .map(|d| d.join("keel-compat"))
            .find(|p| p.is_file())
    }) {
        return Some(p);
    }
    let src = shipwright_dir(opts)?;
    cargo_build(&src.join("Cargo.toml"), "keel-compat", false, &[]).ok()
}

/// Runs keel-compat on the app and prints blockers and partial types.
fn check_compat(opts: &Options, app: &Path) {
    let Some(tool) = keel_compat(opts) else {
        log("keel-compat not found (set KEEL_COMPAT); skipping the compatibility check");
        return;
    };
    let out = match Command::new(&tool).arg("--json").arg(app).output() {
        Ok(o) if o.status.success() => o.stdout,
        Ok(o) => {
            log(&format!(
                "keel-compat failed: {}",
                String::from_utf8_lossy(&o.stderr).trim()
            ));
            return;
        }
        Err(e) => {
            log(&format!("cannot run {}: {e}", tool.display()));
            return;
        }
    };
    let Ok(report) = serde_json::from_slice::<serde_json::Value>(&out) else {
        log("keel-compat printed no JSON report");
        return;
    };
    for b in report["blockers"].as_array().into_iter().flatten() {
        eprintln!(
            "keel-compat: warning: {} {} is {}{}",
            b["section"].as_str().unwrap_or("?"),
            b["name"].as_str().unwrap_or("?"),
            b["status"].as_str().unwrap_or("?"),
            b["note"]
                .as_str()
                .filter(|n| !n.is_empty())
                .map(|n| format!(": {n}"))
                .unwrap_or_default()
        );
    }
    let partial: Vec<&str> = report["partial"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| p["name"].as_str())
        .collect();
    if !partial.is_empty() {
        eprintln!(
            "keel-compat: note: partial in Keel (gaps in keel/silica/COMPATIBILITY.md): {}",
            partial.join(", ")
        );
    }
    let blockers = report["blockers"].as_array().map_or(0, Vec::len);
    log(&format!(
        "keel-compat: {} blocker(s), score {}, tier B {}",
        blockers,
        report["score"],
        report["tiers"]["B"].as_str().unwrap_or("?")
    ));
}

/// Files whose change means a restart (QML) or a rebuild (the rest).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Change {
    None,
    Qml,
    Native,
}

type Snapshot = BTreeMap<PathBuf, (SystemTime, u64)>;

fn snapshot_dir(dir: &Path, into: &mut Snapshot) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else { continue };
        if meta.is_dir() {
            snapshot_dir(&path, into);
        } else if let Ok(modified) = meta.modified() {
            into.insert(path, (modified, meta.len()));
        }
    }
}

fn snapshot(app: &Path, which: Change) -> Snapshot {
    let mut s = Snapshot::new();
    if which == Change::Qml {
        snapshot_dir(&app.join("qml"), &mut s);
        return s;
    }
    for dir in ["src", "cpp"] {
        snapshot_dir(&app.join(dir), &mut s);
    }
    for file in ["build.rs", "Cargo.toml"] {
        if let Ok(meta) = fs::metadata(app.join(file)) {
            if let Ok(m) = meta.modified() {
                s.insert(app.join(file), (m, meta.len()));
            }
        }
    }
    s
}

struct Watcher {
    app: PathBuf,
    qml: Snapshot,
    native: Snapshot,
}

impl Watcher {
    fn new(app: &Path) -> Self {
        Watcher {
            app: app.to_owned(),
            qml: snapshot(app, Change::Qml),
            native: snapshot(app, Change::Native),
        }
    }

    fn poll(&mut self) -> Change {
        let native = snapshot(&self.app, Change::Native);
        let qml = snapshot(&self.app, Change::Qml);
        let change = if native != self.native {
            Change::Native
        } else if qml != self.qml {
            Change::Qml
        } else {
            Change::None
        };
        self.native = native;
        self.qml = qml;
        change
    }
}

/// Whether Qt will find a display, or `QT_QPA_PLATFORM` is set.
fn has_display() -> bool {
    ["QT_QPA_PLATFORM", "DISPLAY", "WAYLAND_DISPLAY"]
        .iter()
        .any(|v| env::var_os(v).is_some_and(|x| !x.is_empty()))
}

fn spawn_app(
    opts: &Options,
    exe: &Path,
    name: &str,
    app: &Path,
    keel: &Keel,
) -> Result<Child, String> {
    let device = Device::find(&opts.device).ok_or_else(|| {
        format!(
            "unknown device {:?}; known:\n{}",
            opts.device,
            crate::devices::list()
        )
    })?;
    // The screen in portrait; the app turns it for the other orientations.
    let (w, h) = opts.size.unwrap_or((device.width, device.height));
    let (w, h) = (w.min(h), w.max(h));
    let ratio = opts
        .pixel_ratio
        .unwrap_or_else(|| crate::devices::keel_pixel_ratio(w.min(h)));

    let mut cmd = Command::new(exe);
    // The program can be the app's library (cargo_build); Keel names the
    // application after argv[0], so pass the name it is installed under.
    #[cfg(unix)]
    std::os::unix::process::CommandExt::arg0(&mut cmd, name);
    cmd.args(&opts.app_args);
    let mut import = OsString::from(&keel.qml);
    if let Some(old) = env::var_os("QML_IMPORT_PATH").filter(|o| !o.is_empty()) {
        import.push(":");
        import.push(old);
    }
    cmd.env("QML_IMPORT_PATH", import)
        .env("KEEL_SAILFISHAPP_DATADIR", app)
        .env("KEEL_RUN_SIZE", format!("{w}x{h}"))
        .env("KEEL_RUN_ORIENTATION", opts.orientation.env_value())
        .env("KEEL_THEME_PIXEL_RATIO", ratio.to_string())
        .env(
            "KEEL_THEME_COLOR_SCHEME",
            match opts.ambience {
                Ambience::Dark => "dark",
                Ambience::Light => "light",
            },
        );
    if let Some((_, lib)) = &keel.sailfishapp {
        let mut path = OsString::from(lib);
        if let Some(old) = env::var_os("LD_LIBRARY_PATH").filter(|o| !o.is_empty()) {
            path.push(":");
            path.push(old);
        }
        cmd.env("LD_LIBRARY_PATH", path);
    }

    let offscreen = if has_display() {
        env::var("QT_QPA_PLATFORM").is_ok_and(|p| p.starts_with("offscreen"))
    } else {
        if opts.screenshot.is_none() {
            log("no display (DISPLAY, WAYLAND_DISPLAY): running offscreen, so no window shows");
        }
        cmd.env("QT_QPA_PLATFORM", "offscreen");
        true
    };
    if offscreen && env::var_os("QT_QUICK_BACKEND").is_none() {
        // Keel's screenshot tests use the software scene graph offscreen.
        cmd.env("QT_QUICK_BACKEND", "software");
    }
    // Fit tall phones on a desktop screen; screenshots stay at full size.
    let scale = opts
        .scale
        .unwrap_or(if offscreen || opts.screenshot.is_some() {
            1.0
        } else {
            (1000.0 / f64::from(h.max(w))).min(1.0)
        });
    if (scale - 1.0).abs() > f64::EPSILON {
        cmd.env("QT_SCALE_FACTOR", format!("{scale:.3}"));
    }
    if let Some(shot) = &opts.screenshot {
        cmd.env("KEEL_RUN_SCREENSHOT", shot);
    }
    let (win_w, win_h) = if opts.orientation.landscape() {
        (h, w)
    } else {
        (w, h)
    };
    log(&format!(
        "{} ({}): {win_w}x{win_h} px, pixel ratio {ratio}, {:?} ambience, {}{}",
        device.id,
        device.name,
        opts.ambience,
        opts.orientation.env_value(),
        if (scale - 1.0).abs() > f64::EPSILON {
            format!(", window scaled by {scale:.2}")
        } else {
            String::new()
        }
    ));
    cmd.spawn()
        .map_err(|e| format!("cannot start {}: {e}", exe.display()))
}

/// Runs `keel run`.
pub fn run(opts: &Options) -> Result<ExitCode, String> {
    if Device::find(&opts.device).is_none() {
        return Err(format!(
            "unknown device {:?}; known:\n{}",
            opts.device,
            crate::devices::list()
        ));
    }
    let app = opts
        .path
        .canonicalize()
        .map_err(|e| format!("{}: {e}", opts.path.display()))?;
    let manifest = app.join("Cargo.toml");
    let name = fs::read_to_string(&manifest)
        .ok()
        .and_then(|m| package_name(&m))
        .ok_or_else(|| format!("{}: no Cargo.toml with a package name", app.display()))?;
    if !app.join("qml").join(format!("{name}.qml")).is_file() {
        log(&format!(
            "warning: {} is missing; SailfishApp::pathToMainQml() will not find the UI",
            app.join("qml").join(format!("{name}.qml")).display()
        ));
    }
    let keel = locate_keel(opts)?;
    log(&format!("Keel QML: {}", keel.qml.display()));

    let mut build_env: Vec<(&str, OsString)> = Vec::new();
    if let Some((include, lib)) = &keel.sailfishapp {
        build_env.push(("KEEL_SAILFISHAPP_INCLUDEDIR", include.into()));
        build_env.push(("KEEL_SAILFISHAPP_LIBDIR", lib.into()));
        build_env.push(("KEEL_SAILFISHAPP_RPATH", "1".into()));
    }

    if !opts.no_compat {
        check_compat(opts, &app);
    }
    let mut exe = cargo_build(&manifest, &name, opts.release, &build_env)?;

    if opts.screenshot.is_some() && !opts.watch {
        let status = spawn_app(opts, &exe, &name, &app, &keel)?
            .wait()
            .map_err(|e| e.to_string())?;
        let shot = opts.screenshot.as_deref().unwrap_or(Path::new(""));
        if status.success() && shot.is_file() {
            log(&format!("screenshot: {}", shot.display()));
            return Ok(ExitCode::SUCCESS);
        }
        return Err(format!(
            "no screenshot: the app ended with {status} (see its output above). Apps from \
             `keel new` take the screenshot in cpp/launcher.cpp (KEEL_RUN_SCREENSHOT); \
             other apps need the same"
        ));
    }

    let mut child = spawn_app(opts, &exe, &name, &app, &keel)?;
    if !opts.watch {
        let status = child.wait().map_err(|e| e.to_string())?;
        return Ok(status
            .code()
            .and_then(|c| u8::try_from(c).ok())
            .map_or(ExitCode::FAILURE, ExitCode::from));
    }

    log("watching qml/ (restart) and src/, cpp/, build.rs, Cargo.toml (rebuild); Ctrl-C stops");
    let mut watcher = Watcher::new(&app);
    let mut running = true;
    loop {
        sleep(Duration::from_millis(300));
        if running {
            if let Ok(Some(status)) = child.try_wait() {
                log(&format!("app exited ({status}); waiting for a change"));
                running = false;
            }
        }
        let change = watcher.poll();
        if change == Change::None {
            continue;
        }
        if running {
            let _ = child.kill();
            let _ = child.wait();
        }
        if change == Change::Native {
            log("sources changed: rebuilding");
            match cargo_build(&manifest, &name, opts.release, &build_env) {
                Ok(e) => exe = e,
                Err(e) => {
                    log(&format!("{e}; waiting for a change"));
                    running = false;
                    continue;
                }
            }
        } else {
            log("QML changed: restarting");
            if !opts.no_compat {
                check_compat(opts, &app);
            }
        }
        child = spawn_app(opts, &exe, &name, &app, &keel)?;
        running = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes() {
        assert_eq!(parse_size("540x960"), Ok((540, 960)));
        assert_eq!(parse_size("720X1600"), Ok((720, 1600)));
        assert!(parse_size("540").is_err());
        assert!(parse_size("5x5").is_err());
    }

    #[test]
    fn manifest_name() {
        let m = "[workspace]\n\n[package]\nname = \"hello-keel\"\nversion = \"0.1.0\"\n";
        assert_eq!(package_name(m).as_deref(), Some("hello-keel"));
        assert_eq!(package_name("[workspace]\nname = \"x\"\n"), None);
    }
}
