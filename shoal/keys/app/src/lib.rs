// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

//! shipwright-shoal-keys: the Shoal Keys password manager. Loads the QML UI
//! (installed to /usr/share/shipwright-shoal-keys/qml) on Keel with the
//! `Keys` singleton from [`bridge`]. On a phone Lipstick starts it
//! with `invoker --type=keel` (booster-keel, else directly), in Keel's direct mode.
//!
//! Built as a shared object that is also executable: the RPM installs it as
//! /usr/bin/shipwright-shoal-keys, and booster-keel loads it and calls its C
//! [`main`] (tools/keel-launcher/link_sailfishapp.rs says why).

// cxx does not carry the `# Safety` doc of an extern fn into the wrapper
// it generates, and CXX-Qt allows no lint attribute on the bridge itself.
#[allow(clippy::missing_safety_doc)]
pub mod autofill;
pub mod bridge;
pub mod controller;

// The generated Keel Actions manifest (build.rs), enforced by the
// Keel.Actions runtime.
keel::manifest!();

use std::ffi::{c_char, c_int};
use std::path::PathBuf;

use cxx_qt_lib::QString;

const DEFAULT_QML: &str = "/usr/share/shipwright-shoal-keys/qml/shipwright-shoal-keys.qml";

fn usage() -> ! {
    eprintln!(
        "usage: shipwright-shoal-keys [--qml FILE]\n\
         \n\
         --qml FILE   main QML file (default {DEFAULT_QML}, or $SHOAL_KEYS_QML)\n\
         \n\
         Development and test overrides (never set on a phone):\n\
         SHOAL_KEYS_DIR=<dir>                 vault and settings directory\n\
         SHOAL_KEYS_DOWNLOADS=<dir>           import/export directory (default ~/Downloads)\n\
         SHOAL_KEYS_INSECURE_KEYSTORE=<dir>   plain-file key store instead of Sailfish Secrets"
    );
    std::process::exit(2)
}

/// The program's entry point, for the C start code and booster-keel: runs
/// `run` through `bridge::keel_main`. `run` takes the arguments from Keel's
/// launcher, which has the right ones under booster-keel too.
#[cfg_attr(not(test), no_mangle)]
pub extern "C" fn main(_argc: c_int, _argv: *const *const c_char) -> c_int {
    bridge::keel_main(run)
}

fn run() {
    let mut qml = std::env::var_os("SHOAL_KEYS_QML")
        .filter(|v| !v.is_empty())
        .map_or_else(|| PathBuf::from(DEFAULT_QML), PathBuf::from);
    // Keel's launcher has the real command line, also under booster-keel.
    let launch_args = bridge::launch_args();
    let mut args = launch_args.iter().skip(1).cloned();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--qml" => qml = args.next().map_or_else(|| usage(), PathBuf::from),
            "--version" => {
                println!("shipwright-shoal-keys {}", env!("CARGO_PKG_VERSION"));
                return;
            }
            "-h" | "--help" => usage(),
            _ => {}
        }
    }
    let qml = std::fs::canonicalize(&qml).unwrap_or_else(|e| {
        eprintln!("shipwright-shoal-keys: cannot open {}: {e}", qml.display());
        std::process::exit(1)
    });

    // Keel's launcher owns the application: booster-keel's, created before
    // the launch, when boosted (ADR-0016); otherwise a new one.
    let Some(mut app) = bridge::keel_application() else {
        eprintln!("shipwright-shoal-keys: cannot create the Qt application");
        std::process::exit(1)
    };
    app.as_mut()
        .set_application_name(&QString::from("shipwright-shoal-keys"));
    app.as_mut()
        .set_application_version(&QString::from(env!("CARGO_PKG_VERSION")));

    // Keel's ApplicationWindow is an Item, not a window: SailfishApp's view
    // (cpp/launcher.cpp) gives it one, full screen on a phone.
    if !bridge::qobject::keel_show_main_view(&QString::from(qml.to_string_lossy().as_ref())) {
        eprintln!("shipwright-shoal-keys: could not load {}", qml.display());
        std::process::exit(1);
    }
    let code = app.exec();
    // The engine's threads (asynchronous image decoding, native QML
    // backends) must stop before exit() runs Qt's global destructors.
    bridge::qobject::keel_destroy_main_view();
    std::process::exit(code);
}
