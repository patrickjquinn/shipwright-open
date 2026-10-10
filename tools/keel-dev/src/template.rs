// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: MIT

//! The app templates (`templates/app/`, embedded at build time) and their
//! `{{placeholder}}` substitution.
//!
//! Template files carry Shipwright's own licence header, so the Shipwright
//! tree stays REUSE-clean; [`render`] rewrites those two header lines to the
//! new app's copyright holder and licence.

use std::collections::BTreeMap;

/// One file of the generated app.
pub struct Template {
    /// Output path, relative to the app directory; may hold placeholders.
    pub path: &'static str,
    /// File contents with placeholders.
    pub text: &'static str,
    /// Installed with mode 0755.
    pub executable: bool,
}

macro_rules! template {
    ($out:expr, $src:expr) => {
        Template {
            path: $out,
            text: include_str!(concat!("../templates/app/", $src)),
            executable: false,
        }
    };
    ($out:expr, $src:expr, exec) => {
        Template {
            path: $out,
            text: include_str!(concat!("../templates/app/", $src)),
            executable: true,
        }
    };
}

/// Every file `keel new` writes, besides `LICENSES/<licence>.txt` and the
/// rendered icon PNGs.
pub const APP: &[Template] = &[
    template!("Cargo.toml", "Cargo.toml.in"),
    template!("Cargo.lock", "Cargo.lock.in"),
    template!("rust-toolchain.toml", "rust-toolchain.toml"),
    template!("build.rs", "build.rs"),
    // Compiles the app's QML ahead of time (shared with Shipwright's apps).
    template!(
        "build/precompile_qml.rs",
        "../../../keel-launcher/precompile_qml.rs"
    ),
    template!("src/lib.rs", "src/lib.rs"),
    template!("src/bridge.rs", "src/bridge.rs"),
    template!("src/greeter.rs", "src/greeter.rs"),
    template!("cpp/launcher.h", "cpp/launcher.h"),
    template!("cpp/launcher.cpp", "cpp/launcher.cpp"),
    template!("qml/{{name}}.qml", "qml/app.qml"),
    template!("qml/pages/MainPage.qml", "qml/pages/MainPage.qml"),
    template!("qml/cover/CoverPage.qml", "qml/cover/CoverPage.qml"),
    template!("icons/{{name}}.svg", "icons/app.svg"),
    template!("icons/render-icons.sh", "icons/render-icons.sh", exec),
    template!("packaging/{{name}}.desktop", "packaging/app.desktop"),
    template!("rpm/{{name}}.spec", "rpm/app.spec"),
    template!("rpm/build-in-sdk.sh", "rpm/build-in-sdk.sh", exec),
    template!("tests/smoke-test.sh", "tests/smoke-test.sh", exec),
    template!(".github/workflows/ci.yml", "github/workflows/ci.yml"),
    template!(".gitignore", "gitignore"),
    template!("REUSE.toml", "REUSE.toml.in"),
    template!("README.md", "README.md"),
];

// REUSE-IgnoreStart
const TEMPLATE_COPYRIGHT: &str = "SPDX-FileCopyrightText: 2026 Patrick Quinn";
/// The templates' own licence (MIT-0: no attribution, so a generated app is
/// its author's to license), rewritten to the app's.
const TEMPLATE_LICENSE: &str = "SPDX-License-Identifier: MIT-0";
/// The licence of the open-source files an app copies verbatim.
pub const OPEN_LICENSE: &str = "SPDX-License-Identifier: MIT";

/// Whether `text` carries [`OPEN_LICENSE`] as a header line of its own
/// (not the templates' MIT-0, which merely starts the same).
pub fn has_open_licence(text: &str) -> bool {
    text.lines().any(|l| l.trim_end().ends_with(OPEN_LICENSE))
}
// REUSE-IgnoreEnd

/// Placeholder values, by name (without the braces).
pub type Vars = BTreeMap<&'static str, String>;

/// Substitutes `{{name}}` placeholders and rewrites the licence header.
///
/// Only `{{` directly followed by a lower-case name and `}}` is a
/// placeholder, so `${{ github.expressions }}` pass through. Fails on a
/// placeholder without a value.
pub fn render(text: &str, vars: &Vars) -> Result<String, String> {
    // Only the template's own (MIT-0) header is the app's to rewrite;
    // an open-source file the app copies (build/precompile_qml.rs, MIT) keeps
    // its notice, and `keel new` ships that licence's text too.
    // REUSE-IgnoreStart
    let text = if text.contains(TEMPLATE_LICENSE) {
        text.replace(
            TEMPLATE_COPYRIGHT,
            "SPDX-FileCopyrightText: {{year}} {{copyright}}",
        )
        .replace(TEMPLATE_LICENSE, "SPDX-License-Identifier: {{license}}")
    } else {
        text.to_string()
    };
    // REUSE-IgnoreEnd
    let mut out = String::with_capacity(text.len());
    let mut rest = text.as_str();
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        if let Some(name) = placeholder(after) {
            let value = vars
                .get(name)
                .ok_or_else(|| format!("template placeholder {{{{{name}}}}} has no value"))?;
            out.push_str(value);
            rest = &after[name.len() + 2..];
        } else {
            out.push_str("{{");
            rest = after;
        }
    }
    out.push_str(rest);
    Ok(out)
}

/// The placeholder name at the start of `s` (after `{{`), if `s` continues
/// with `[a-z_]+}}`.
fn placeholder(s: &str) -> Option<&str> {
    let len = s
        .bytes()
        .take_while(|b| b.is_ascii_lowercase() || *b == b'_')
        .count();
    (len > 0 && s[len..].starts_with("}}")).then(|| &s[..len])
}

/// Placeholders left in `text` (names), for checks after rendering.
pub fn leftover_placeholders(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        rest = &rest[start + 2..];
        if let Some(name) = placeholder(rest) {
            found.push(name.to_owned());
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars() -> Vars {
        let mut v = Vars::new();
        v.insert("name", "hello".into());
        v.insert("year", "2030".into());
        v.insert("copyright", "Ada".into());
        v.insert("license", "MIT".into());
        v
    }

    #[test]
    fn substitutes_and_keeps_github_expressions() {
        let out = render("a {{name}} ${{ github.ref }} {{ x }} {{name}}", &vars()).unwrap();
        assert_eq!(out, "a hello ${{ github.ref }} {{ x }} hello");
        assert!(leftover_placeholders(&out).is_empty());
    }

    #[test]
    fn unknown_placeholder_is_an_error() {
        let err = render("{{nope}}", &vars()).unwrap_err();
        assert!(err.contains("{{nope}}"), "{err}");
    }

    #[test]
    fn rewrites_the_licence_header() {
        let text = format!("// {TEMPLATE_COPYRIGHT}\n// {TEMPLATE_LICENSE}\n");
        let out = render(&text, &vars()).unwrap();
        // REUSE-IgnoreStart
        assert_eq!(
            out,
            "// SPDX-FileCopyrightText: 2030 Ada\n// SPDX-License-Identifier: MIT\n"
        );
        // REUSE-IgnoreEnd
    }

    #[test]
    fn every_template_has_a_header() {
        for t in APP {
            if t.path == "Cargo.lock" {
                continue;
            }
            // REUSE-IgnoreStart
            let open = has_open_licence(t.text);
            // REUSE-IgnoreEnd
            assert!(
                t.text.contains(TEMPLATE_COPYRIGHT) && (t.text.contains(TEMPLATE_LICENSE) || open),
                "{} has no licence header",
                t.path
            );
        }
    }
}
