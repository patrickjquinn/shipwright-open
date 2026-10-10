// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: MIT-0

//! Builds the `{{qml_uri}} 1.0` QML module (the Rust `Greeter` singleton)
//! and the C++ launcher, and links Keel's `SailfishApp` library.
//!
//! Where libkeel-sailfishapp comes from, first match wins:
//! 1. `KEEL_SAILFISHAPP_INCLUDEDIR` and `KEEL_SAILFISHAPP_LIBDIR` (`keel run`
//!    sets them for a Keel build tree; the SDK build sets them to the target
//!    sysroot). `KEEL_SAILFISHAPP_RPATH=1` adds an rpath to the library;
//! 2. a Keel `CMake` build tree: `KEEL_BUILD_DIR`, else the one `keel run`
//!    keeps in ~/.cache/keel-dev/keel-build (with an rpath, for running
//!    from the build tree);
//! 3. `pkg-config keel-sailfishapp` (shipwright-keel-sailfishapp-devel).

use std::path::{Path, PathBuf};
use std::process::Command;

use cxx_qt_build::{CxxQtBuilder, QmlModule};

// Compiles the app's QML ahead of time (build/precompile_qml.rs).
include!("build/precompile_qml.rs");

fn pkg_config(args: &[&str]) -> Option<String> {
    let out = Command::new("pkg-config").args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// (include dir, library dir) of a Keel `CMake` build tree.
fn keel_build_tree(build: &Path) -> Option<(PathBuf, PathBuf)> {
    let cache = std::fs::read_to_string(build.join("CMakeCache.txt")).ok()?;
    let source = cache
        .lines()
        .find_map(|l| l.strip_prefix("CMAKE_HOME_DIRECTORY:INTERNAL="))?;
    let lib = build.join("sailfishapp");
    lib.join("libkeel-sailfishapp.so")
        .exists()
        .then(|| (Path::new(source).join("sailfishapp/include"), lib))
}

/// (include dir, library dir if known, add an rpath).
fn sailfishapp() -> (PathBuf, Option<PathBuf>, bool) {
    if let (Some(inc), Some(lib)) = (
        std::env::var_os("KEEL_SAILFISHAPP_INCLUDEDIR"),
        std::env::var_os("KEEL_SAILFISHAPP_LIBDIR"),
    ) {
        let rpath = std::env::var("KEEL_SAILFISHAPP_RPATH").as_deref() == Ok("1");
        return (PathBuf::from(inc), Some(PathBuf::from(lib)), rpath);
    }
    let cache = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .map(|c| c.join("keel-dev/keel-build"));
    let tree = std::env::var_os("KEEL_BUILD_DIR")
        .map(PathBuf::from)
        .or(cache)
        .and_then(|b| keel_build_tree(&b));
    if let Some((inc, lib)) = tree {
        return (inc, Some(lib), true);
    }
    if let Some(inc) = pkg_config(&["--variable=includedir", "keel-sailfishapp"]) {
        let lib = pkg_config(&["--variable=libdir", "keel-sailfishapp"]).map(PathBuf::from);
        return (PathBuf::from(inc), lib, false);
    }
    panic!(
        "Keel's SailfishApp library was not found: set KEEL_BUILD_DIR to a Keel CMake \
         build, run `keel run` once (it builds Keel), or install \
         shipwright-keel-sailfishapp-devel"
    );
}

/// Makes the app's library executable, so that the RPM installs it as
/// /usr/bin/{{name}} and booster-keel (Keel's mapplauncherd booster) can
/// `dlopen()` the same file and call its `main`.
///
/// The booster needs a shared object: a position-independent executable
/// cannot be loaded, and rustc compiles a binary crate with the local-exec
/// TLS model, whose relocations a shared object cannot hold (a release
/// build fails to link with `-shared`). The app is therefore a `cdylib`,
/// which rustc compiles for a shared object and links exporting its
/// `#[no_mangle]` items (`main` in src/lib.rs). These arguments add the C
/// runtime's start code (`Scrt1.o`, whose `_start` calls `main`) and a
/// `.interp` section naming the dynamic linker (the linker then emits
/// `PT_INTERP`), so the kernel runs it too. Shipwright's
/// tools/keel-launcher/link_sailfishapp.rs explains it in full.
fn executable_cdylib() {
    let interp = match std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        _ if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("linux") => return,
        Ok("x86_64") => "/lib64/ld-linux-x86-64.so.2",
        Ok("aarch64") => "/lib/ld-linux-aarch64.so.1",
        Ok("arm") => "/lib/ld-linux-armhf.so.3",
        Ok("x86") => "/lib/ld-linux.so.2",
        _ => return,
    };
    let compiler = cc::Build::new().cargo_metadata(false).get_compiler();
    let Some(scrt1) = compiler
        .to_command()
        .arg("-print-file-name=Scrt1.o")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .filter(|p| Path::new(p).is_absolute())
    else {
        println!("cargo:warning=no Scrt1.o from the C compiler: the app library is not executable");
        return;
    };
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"));
    let interp_c = out.join("interp.c");
    let interp_o = out.join("interp.o");
    std::fs::write(
        &interp_c,
        format!(
            "const char app_interp[] __attribute__((section(\".interp\"), used)) = \"{interp}\";\n"
        ),
    )
    .expect("write interp.c");
    let status = compiler
        .to_command()
        .arg("-c")
        .arg(&interp_c)
        .arg("-o")
        .arg(&interp_o)
        .status()
        .expect("run the C compiler for interp.c");
    assert!(status.success(), "compiling interp.c failed ({status})");
    // --undefined keeps the section through --gc-sections.
    for arg in [
        scrt1,
        interp_o.display().to_string(),
        "-Wl,--undefined=app_interp".to_owned(),
    ] {
        println!("cargo:rustc-cdylib-link-arg={arg}");
    }
}

fn main() {
    for var in [
        "KEEL_SAILFISHAPP_INCLUDEDIR",
        "KEEL_SAILFISHAPP_LIBDIR",
        "KEEL_SAILFISHAPP_RPATH",
        "KEEL_BUILD_DIR",
    ] {
        println!("cargo:rerun-if-env-changed={var}");
    }
    let (include, libdir, rpath) = sailfishapp();
    if let Some(lib) = &libdir {
        println!("cargo:rustc-link-search=native={}", lib.display());
        if rpath {
            println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib.display());
        }
    }
    println!("cargo:rustc-link-lib=keel-sailfishapp");
    // rustc passes these `-l` libraries under `--as-needed` before the static
    // archives with the app's C++ that uses them; GNU ld (not lld) then drops
    // them and the app fails at start with an undefined `SailfishApp::` or
    // `QQuickItem::` symbol. Name them again after every object.
    println!(
        "cargo:rustc-link-arg=-Wl,--push-state,--no-as-needed,-lkeel-sailfishapp,-lQt6Quick,--pop-state"
    );
    executable_cdylib();

    let builder = CxxQtBuilder::new_qml_module(QmlModule::new("{{qml_uri}}").version(1, 0))
        .qt_module("Gui")
        .qt_module("Quick")
        .files(["src/bridge.rs"])
        .include_dir(include)
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
    // The app's QML and JavaScript, compiled ahead of time for where the
    // package installs them (rpm/{{name}}.spec: %{_datadir}/%{app}/qml).
    precompile_app_qml(Path::new("qml"), "/usr/share/{{name}}/qml", &[]);
}
