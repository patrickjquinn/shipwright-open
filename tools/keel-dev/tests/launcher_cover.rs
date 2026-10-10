// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: MIT

//! The launcher's cover check (`coverLoads` in `keel new`'s
//! cpp/launcher.cpp and in the in-repo apps' copies, see
//! tools/keel-launcher/README.md) must create the cover in the context it was
//! written in. Created in the engine's root context, an inline
//! `cover: Component { ... }` that reads the window's properties fails with
//! `ReferenceError`s that the real cover (created by Keel's `ApplicationWindow`)
//! never has, so `keel run` reported working covers as broken (found by
//! Shoal Camera).

use std::fs;
use std::path::PathBuf;

const IN_CONTEXT: &str = "component->create(component->creationContext())";
const ROOT_CONTEXT: &str = "item(component->create())";

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn check(path: &str) {
    let text = fs::read_to_string(repo().join(path)).unwrap_or_else(|e| panic!("{path}: {e}"));
    assert!(text.contains("bool coverLoads("), "{path}: no cover check");
    assert!(
        text.contains(IN_CONTEXT),
        "{path}: the cover check must create the cover in its creation context"
    );
    assert!(
        !text.contains(ROOT_CONTEXT),
        "{path}: the cover check creates the cover in the root context"
    );
}

#[test]
fn template_creates_the_cover_in_its_context() {
    check("tools/keel-dev/templates/app/cpp/launcher.cpp");
}

#[test]
fn app_copies_create_the_cover_in_its_context() {
    for app in [
        "reef/client/app",
        "shoal/keys/app",
        "shoal/mail/app",
        "shoal/migrate/app",
        "shoal/bridge-icloud/app",
    ] {
        check(&format!("{app}/cpp/launcher.cpp"));
    }
}
