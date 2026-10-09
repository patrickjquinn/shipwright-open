// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

use cxx_qt_build::{CxxQtBuilder, QmlModule};

// Links Keel's SailfishApp library (libkeel-sailfishapp) for cpp/launcher.cpp;
// brings `Path` and `PathBuf` into scope.
include!("../../../tools/keel-launcher/link_sailfishapp.rs");
// Compiles the UI's QML into the app (tools/keel-launcher/precompile_qml.rs).
include!("../../../tools/keel-launcher/precompile_qml.rs");

fn main() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let sailfishapp_include = link_keel_sailfishapp(&repo);
    // `Shipwright.Keys 1.0` holds only the Rust `Keys` singleton; the UI's
    // .qml files are installed to /usr/share/shipwright-shoal-keys/qml and
    // loaded from disk.
    let builder = CxxQtBuilder::new_qml_module(QmlModule::new("Shipwright.Keys").version(1, 0))
        .qt_module("Gui")
        .qt_module("Quick")
        .files(["src/bridge.rs"])
        .cpp_file("cpp/clipboard.cpp")
        .include_dir(sailfishapp_include)
        .cpp_file("cpp/launcher.cpp");
    // GCC 16 warns (-Wsfinae-incomplete) inside Qt's own headers (qchar.h,
    // qmetatype.h probe completeness on purpose); older compilers ignore
    // the flag.
    // SAFETY: a warning flag only; it changes no generated code or ABI.
    let builder = unsafe {
        builder.cc_builder(|cc| {
            cc.flag_if_supported("-Wno-sfinae-incomplete");
        })
    };
    builder.build();
    // The UI's QML and JavaScript, compiled ahead of time for where the
    // package installs them.
    precompile_app_qml(&repo.join("shoal/keys/ui"), "/usr/share/shipwright-shoal-keys/qml", &[]);
    // Keel Actions (ADR-0018): actions.json and the D-Bus files from the QML
    // declarations, into $OUT_DIR (keel::manifest!() in src/lib.rs) and
    // target/[<triple>/]<profile>/keel-actions/<app-id>/ for the RPM.
    keel_actions_codegen::Config {
        qml: vec![repo.join("shoal/keys/ui")],
        desktop: Some(repo.join("shoal/keys/app/packaging/shipwright-shoal-keys.desktop")),
        ..Default::default()
    }
    .build_script();
}
