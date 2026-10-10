// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: MIT-0

//! {{name}}: {{title}}. The QML UI (installed to /usr/share/{{name}}/qml)
//! runs on Keel with the `Greeter` singleton from [`bridge`]. On a phone
//! Lipstick starts it with `invoker --type=keel` (booster-keel when it runs,
//! else directly), in Keel's direct mode. On a desktop: `keel run` in the
//! project directory.
//!
//! Built as a shared object that is also executable (build.rs says why):
//! the RPM installs it as /usr/bin/{{name}}, and booster-keel loads it and
//! calls its C [`main`].

// cxx does not carry the `# Safety` doc of an extern fn into the wrapper it
// generates, and CXX-Qt allows no lint attribute on the bridge itself.
#[allow(clippy::missing_safety_doc)]
pub mod bridge;
pub mod greeter;

use std::ffi::{c_char, c_int};

fn usage() -> ! {
    eprintln!(
        "usage: {{name}} [--version]\n\
         \n\
         Loads /usr/share/{{name}}/qml/{{name}}.qml through Keel's SailfishApp.\n\
         Development overrides (`keel run` sets them):\n\
         KEEL_SAILFISHAPP_DATADIR=<dir>  data directory instead of /usr/share/{{name}}\n\
         KEEL_RUN_SIZE=<w>x<h>           desktop window size (default 540x960)\n\
         KEEL_RUN_ORIENTATION=<o>        portrait, landscape, portrait-inverted, landscape-inverted\n\
         KEEL_RUN_SCREENSHOT=<file.png>  save the first page to a PNG and exit"
    );
    std::process::exit(2)
}

/// The program's entry point, for the C start code and booster-keel. Rust's
/// start code for a binary does not run, so this does what it does: SIGPIPE
/// is ignored (a closed pipe or socket is an `EPIPE` error), and a panic
/// ends the program with status 101 instead of unwinding into C.
#[cfg_attr(not(test), no_mangle)]
pub extern "C" fn main(_argc: c_int, _argv: *const *const c_char) -> c_int {
    extern "C" {
        fn signal(signum: c_int, handler: usize) -> usize;
    }
    // SIGPIPE is 13 and SIG_IGN is 1 on every Linux architecture.
    // SAFETY: setting a signal's disposition to "ignore" has no
    // preconditions.
    unsafe {
        signal(13, 1);
    }
    std::panic::catch_unwind(run).unwrap_or(101)
}

/// Runs the app; returns its exit status. The arguments come from Keel's
/// launcher, which has the right ones under booster-keel too.
fn run() -> c_int {
    let args = bridge::qobject::keel_launch_arguments();
    for arg in args.iter().skip(1) {
        match arg.as_str() {
            "--version" => {
                println!("{{name}} {}", env!("CARGO_PKG_VERSION"));
                return 0;
            }
            "-h" | "--help" => usage(),
            _ => {}
        }
    }
    // The application object and window come from Keel's SailfishApp
    // library, in C++ (cpp/launcher.cpp).
    bridge::qobject::keel_app_run(&args)
}
