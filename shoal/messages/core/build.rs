// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: Apache-2.0
//
// New file, added by Shipwright in 2026 (not part of upstream xmatic).

//! Keel Actions (ADR-0018): actions.json and the D-Bus files from the
//! declarations in src/actions.rs and the `KeelContext` in
//! qml/pages/RoomPage.qml, into $OUT_DIR (keel::manifest!() in
//! src/actions.rs) and target/[<triple>/]<profile>/keel-actions/<app-id>/,
//! where the .pro installs them from.

use std::path::Path;

fn main() {
    let app = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    keel_actions_codegen::Config {
        qml: vec![app.join("qml")],
        rust: vec![app.join("core/src")],
        desktop: Some(app.join("shipwright-shoal-messages.desktop")),
        // The desktop file's Exec is a bare name (the launcher resolves it).
        executable: Some("/usr/bin/shipwright-shoal-messages".into()),
        ..Default::default()
    }
    .build_script();
}
