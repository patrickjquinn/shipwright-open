// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: MIT

//! keel: the Keel developer tool. `keel new` writes a working Sailfish app
//! skeleton on Keel (Rust over CXX-Qt, Silica QML, SailfishApp start-up,
//! RPM spec, desktop file, icon, smoke test, CI); `keel run` builds an app
//! and runs it on the desktop in a phone-sized window; `keel actions` is the
//! Keel Actions build step (ADR-0018).

mod actions;
mod devices;
mod new;
mod run;
mod template;

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(name = "keel", version, about = "Start and run Sailfish apps on Keel")]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Create a new app: Rust (CXX-Qt) backend, Silica QML on Keel, RPM
    /// spec, desktop file, icon, smoke test and CI workflow.
    New(NewArgs),
    /// Build an app and run it in a phone-sized window against Keel.
    Run(RunArgs),
    /// Generate an app's Keel Actions artefacts (actions.json, D-Bus
    /// introspection and activation, Sailjail lines, adaptor, test stubs)
    /// from its QML and Rust declarations.
    Actions(ActionsArgs),
}

#[derive(Args)]
#[command(after_help = concat!(
    "Defaults, relative to PATH: --qml qml/ (else ui/), --rust src/, --desktop the first\n",
    "packaging/*.desktop (else *.desktop), --out target/keel-actions.\n",
    "From build.rs: keel_actions_codegen::Config { .. }.build_script(), then keel::manifest!()."
))]
struct ActionsArgs {
    /// App directory.
    #[arg(default_value = ".")]
    path: PathBuf,
    /// QML directory or file to read (repeatable).
    #[arg(long, value_name = "DIR")]
    qml: Vec<PathBuf>,
    /// Rust source directory or file to read (repeatable).
    #[arg(long, value_name = "DIR")]
    rust: Vec<PathBuf>,
    /// The app's desktop file (app ID from [X-Sailjail], executable from Exec).
    #[arg(long, value_name = "FILE")]
    desktop: Option<PathBuf>,
    /// App ID, overriding the desktop file's OrganizationName.ApplicationName.
    #[arg(long)]
    app_id: Option<String>,
    /// Executable for D-Bus activation, overriding the desktop file's Exec.
    #[arg(long, value_name = "PATH")]
    exec: Option<String>,
    /// Output directory.
    #[arg(long, value_name = "DIR")]
    out: Option<PathBuf>,
    /// Write test stubs into DIR (only those that do not exist yet).
    #[arg(long, value_name = "DIR")]
    tests: Option<PathBuf>,
    /// Write nothing; fail if the output directory is out of date or the
    /// desktop file lacks the `ExecDBus` line.
    #[arg(long)]
    check: bool,
    /// Only review an actions.json with Reef's rules.
    #[arg(long, value_name = "FILE", conflicts_with_all = ["qml", "rust", "check", "tests"])]
    review: Option<PathBuf>,
}

#[derive(Args)]
struct NewArgs {
    /// App id: package and binary name (lower-case letters, digits,
    /// hyphens), e.g. weather-now.
    name: String,
    /// Directory to create [default: ./<name>].
    #[arg(long)]
    dir: Option<PathBuf>,
    /// Reverse-DNS id: QML module URI and Sailjail OrganizationName.ApplicationName
    /// [default: org.example.<Name>].
    #[arg(long)]
    id: Option<String>,
    /// SPDX licence of the generated files: MIT, Apache-2.0, BSD-3-Clause,
    /// GPL-3.0-or-later, GPL-3.0-only, LGPL-2.1-or-later or `LicenseRef-<name>`.
    #[arg(long, default_value = "MIT")]
    license: String,
    /// Display name [default: from the name, e.g. "Weather Now"].
    #[arg(long)]
    title: Option<String>,
    /// One-line summary for the desktop file and RPM.
    #[arg(long)]
    summary: Option<String>,
    /// Copyright holder in the licence headers [default: git user.name].
    #[arg(long)]
    copyright: Option<String>,
    /// Do not render the icon PNGs (needs rsvg-convert otherwise).
    #[arg(long)]
    no_icons: bool,
}

#[derive(Args)]
#[command(after_help = concat!(
    "Devices (--device):\n",
    "  run `keel run --list-devices`\n\n",
    "Keel: --keel-build DIR (a Keel CMake build), else --keel-qml DIR (its qml/),\n",
    "else KEEL_BUILD_DIR / KEEL_QML_DIR, else Keel is built from the Shipwright\n",
    "checkout into ~/.cache/keel-dev/keel-build."
))]
#[allow(
    clippy::struct_excessive_bools,
    reason = "each bool is an independent command-line switch (clap derive)"
)]
struct RunArgs {
    /// App directory (with Cargo.toml and qml/<name>.qml).
    #[arg(default_value = ".")]
    path: PathBuf,
    /// A Keel CMake build directory (qml/ and sailfishapp/ inside).
    #[arg(long, value_name = "DIR")]
    keel_build: Option<PathBuf>,
    /// Keel's QML import directory (the one holding Sailfish/Silica).
    #[arg(long, value_name = "DIR")]
    keel_qml: Option<PathBuf>,
    /// Shipwright checkout to build Keel and keel-compat from.
    #[arg(long, value_name = "DIR")]
    shipwright: Option<PathBuf>,
    /// Phone to imitate (window size and pixel ratio).
    #[arg(long, default_value = "default")]
    device: String,
    /// List the --device presets and exit.
    #[arg(long)]
    list_devices: bool,
    /// Window size in device pixels, overriding the device's.
    #[arg(long, value_name = "WxH", value_parser = run::parse_size)]
    size: Option<(u32, u32)>,
    /// Theme.pixelRatio, overriding the device's.
    #[arg(long)]
    pixel_ratio: Option<f64>,
    /// Ambience colour scheme.
    #[arg(long, value_enum, default_value = "dark")]
    ambience: run::Ambience,
    /// Device orientation.
    #[arg(long, value_enum, default_value = "portrait")]
    orientation: run::Orientation,
    /// Window scale on the desktop (`QT_SCALE_FACTOR`) [default: fit 1000 px].
    #[arg(long)]
    scale: Option<f64>,
    /// Restart when a QML file changes; rebuild when Rust or C++ changes.
    #[arg(long)]
    watch: bool,
    /// Save the first page to this PNG and exit (offscreen when there is
    /// no display).
    #[arg(long, value_name = "FILE")]
    screenshot: Option<PathBuf>,
    /// Build with --release.
    #[arg(long)]
    release: bool,
    /// Skip the keel-compat check.
    #[arg(long)]
    no_compat: bool,
    /// Arguments for the app, after `--`.
    #[arg(last = true)]
    app_args: Vec<OsString>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Cmd::New(a) => new_app(a),
        Cmd::Run(a) => run_app(a),
        Cmd::Actions(a) => actions::run(&actions::Options {
            path: a.path,
            qml: a.qml,
            rust: a.rust,
            desktop: a.desktop,
            app_id: a.app_id,
            exec: a.exec,
            out: a.out,
            tests: a.tests,
            check: a.check,
            review: a.review,
        }),
    };
    result.unwrap_or_else(|e| {
        eprintln!("keel: {e}");
        ExitCode::FAILURE
    })
}

fn new_app(a: NewArgs) -> Result<ExitCode, String> {
    let opts = new::Options {
        name: a.name,
        dir: a.dir,
        id: a.id,
        license: a.license,
        title: a.title,
        summary: a.summary,
        copyright: a.copyright,
        render_icons: !a.no_icons,
    };
    let dir = new::generate(&opts)?;
    println!(
        "Created {} in {}.\n\nNext:\n  cd {}\n  keel run                 # build and run it in a window\n  \
         cargo test               # unit tests\n\nSee README.md there and \
         docs/developers/getting-started.md in Shipwright.",
        opts.name,
        dir.display(),
        dir.display()
    );
    Ok(ExitCode::SUCCESS)
}

fn run_app(a: RunArgs) -> Result<ExitCode, String> {
    if a.list_devices {
        println!("{}", devices::list());
        return Ok(ExitCode::SUCCESS);
    }
    if a.watch && a.screenshot.is_some() {
        return Err("--watch and --screenshot do not go together".into());
    }
    if let Some(s) = a.scale {
        if !(0.1..=4.0).contains(&s) {
            return Err("--scale must be between 0.1 and 4".into());
        }
    }
    if let Some(r) = a.pixel_ratio {
        if !(0.5..=4.0).contains(&r) {
            return Err("--pixel-ratio must be between 0.5 and 4".into());
        }
    }
    let screenshot = match a.screenshot {
        // The app runs in its own directory's terms; make the path absolute.
        Some(p) if p.is_relative() => {
            Some(std::env::current_dir().map_err(|e| e.to_string())?.join(p))
        }
        other => other,
    };
    run::run(&run::Options {
        path: a.path,
        keel_build: a.keel_build,
        keel_qml: a.keel_qml,
        shipwright: a.shipwright,
        device: a.device,
        size: a.size,
        pixel_ratio: a.pixel_ratio,
        ambience: a.ambience,
        orientation: a.orientation,
        scale: a.scale,
        watch: a.watch,
        screenshot,
        release: a.release,
        no_compat: a.no_compat,
        app_args: a.app_args,
    })
}
