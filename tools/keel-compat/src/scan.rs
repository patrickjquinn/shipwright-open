// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: MIT

//! Extracts imports, Silica type usage, SailfishApp calls, build integration
//! and Sailjail permissions from source text.
//!
//! This is a lexical scan, not a QML parser: it strips comments and string
//! literals, then looks for `import` lines, `Name {` object declarations and
//! `Name.member` references.

use std::collections::{BTreeMap, BTreeSet};

use crate::catalog::{self, SILICA_MODULE};

/// Everything found in one app, keyed by name with a use count.
#[derive(Debug, Default)]
pub struct Findings {
    pub modules: BTreeMap<String, usize>,
    pub silica_types: BTreeMap<String, usize>,
    pub sailfishapp_apis: BTreeMap<String, usize>,
    /// qmake/CMake/pkg-config/header integration with the Sailfish platform.
    pub build: BTreeMap<String, usize>,
    /// QML module URIs the app provides itself: string literals in native
    /// code (as passed to `qmlRegisterType`) and `module` lines in `qmldir`.
    pub app_modules: BTreeSet<String>,
    /// Permissions from `[X-Sailjail]` sections of `.desktop` files.
    pub sailjail_permissions: BTreeSet<String>,
    pub sailjail_sections: usize,
    pub qml_files: usize,
    pub native_files: usize,
    /// JavaScript files that import a QML module or use a Silica name.
    pub js_files: usize,
    pub build_files: usize,
    pub desktop_files: usize,
}

impl Findings {
    pub fn add_qml(&mut self, source: &str) {
        self.qml_files += 1;
        let (code, _) = lex(source);
        let scope = self.record_imports(&code, parse_import);
        let mut refs = Vec::new();
        silica_refs(&code, &scope, true, &mut refs);
        for name in refs {
            *self.silica_types.entry(name.to_string()).or_default() += 1;
        }
    }

    /// JavaScript is scanned only where Silica can reach it: through an
    /// explicit `.import Sailfish.Silica ... as X`, or, in a file without
    /// `.pragma library`, through the importing QML file's scope. Files with
    /// neither module imports nor Silica names are skipped and not counted.
    pub fn add_js(&mut self, source: &str) {
        let (code, _) = lex(source);
        let library = code
            .lines()
            .any(|l| l.split_whitespace().collect::<Vec<_>>() == [".pragma", "library"]);
        let before = self.modules.values().sum::<usize>();
        let mut scope = self.record_imports(&code, parse_js_import);
        let imported = self.modules.values().sum::<usize>() > before;
        // A non-library file is evaluated in its importer's scope, which in a
        // Sailfish app imports Silica unqualified, and QtQuick 2.x before it:
        // through keel/qt5compat's shims `Screen` is then Silica's, so the
        // file's own imports decide `qt_quick_screen`.
        scope.unqualified |= !library;
        let mut refs = Vec::new();
        silica_refs(&code, &scope, false, &mut refs);
        if imported || !refs.is_empty() {
            self.js_files += 1;
        }
        for name in refs {
            *self.silica_types.entry(name.to_string()).or_default() += 1;
        }
    }

    /// A CXX-Qt crate's `build.rs`: the QML module it registers
    /// (`QmlModule::new("Shipwright.Keys")`) is the app's own, as a
    /// `qmlRegisterType` URI is in C++.
    pub fn add_cxxqt_build(&mut self, source: &str) {
        let marker = "QmlModule::new(";
        let mut rest = source;
        while let Some(pos) = rest.find(marker) {
            let after = rest[pos + marker.len()..].trim_start();
            if let Some(uri) = after.strip_prefix('"').and_then(|a| a.split('"').next()) {
                if looks_like_module_uri(uri) {
                    self.app_modules.insert(uri.to_string());
                }
            }
            rest = &rest[pos + marker.len()..];
        }
    }

    pub fn add_native(&mut self, source: &str) {
        self.native_files += 1;
        let (code, strings) = lex(source);
        let marker = "SailfishApp::";
        let mut rest = code.as_str();
        while let Some(pos) = rest.find(marker) {
            let after = &rest[pos + marker.len()..];
            let ident: String = after.chars().take_while(|c| is_ident(*c)).collect();
            if !ident.is_empty() {
                *self
                    .sailfishapp_apis
                    .entry(format!("{marker}{ident}"))
                    .or_default() += 1;
            }
            rest = after;
        }
        for s in strings {
            if looks_like_module_uri(&s) {
                self.app_modules.insert(s);
            }
        }
        // `#include <...>` targets were blanked with the strings only when
        // quoted; angle-bracket includes survive in `code`.
        for line in source.lines() {
            let line = line.trim_start();
            let Some(rest) = line.strip_prefix('#') else {
                continue;
            };
            let Some(target) = rest.trim_start().strip_prefix("include") else {
                continue;
            };
            let target = target.trim().trim_matches(|c| matches!(c, '<' | '>' | '"'));
            if let Some(lib) = catalog::library_for_header(target) {
                *self.build.entry(format!("header {lib}")).or_default() += 1;
            }
        }
    }

    /// qmake project files (`.pro`, `.pri`) and CMake lists.
    pub fn add_build(&mut self, source: &str, cmake: bool) {
        self.build_files += 1;
        let text = strip_hash_comments(source);
        if cmake {
            for word in words(&text) {
                let lib = word.strip_prefix("PkgConfig::").unwrap_or(word);
                if catalog::lookup(catalog::BUILD, &format!("pkg-config {lib}")).is_some() {
                    *self.build.entry(format!("pkg-config {lib}")).or_default() += 1;
                }
            }
            return;
        }
        for (var, values) in qmake_assignments(&text) {
            for value in values {
                let key = match var {
                    "CONFIG" if value.starts_with("sailfishapp") => format!("CONFIG {value}"),
                    "PKGCONFIG" => format!("pkg-config {value}"),
                    _ => continue,
                };
                if var == "PKGCONFIG" && catalog::lookup(catalog::BUILD, &key).is_none() {
                    continue;
                }
                *self.build.entry(key).or_default() += 1;
            }
        }
    }

    pub fn add_qmldir(&mut self, source: &str) {
        for line in source.lines() {
            let mut w = line.split_whitespace();
            if w.next() == Some("module") {
                if let Some(uri) = w.next() {
                    self.app_modules.insert(uri.to_string());
                }
            }
        }
    }

    pub fn add_desktop(&mut self, source: &str) {
        self.desktop_files += 1;
        let mut in_sailjail = false;
        for line in source.lines() {
            let line = line.trim();
            if line.starts_with('[') {
                in_sailjail = line == "[X-Sailjail]";
                if in_sailjail {
                    self.sailjail_sections += 1;
                }
                continue;
            }
            if !in_sailjail {
                continue;
            }
            if let Some(value) = line.strip_prefix("Permissions") {
                if let Some(value) = value.trim_start().strip_prefix('=') {
                    for p in value.split(';').map(str::trim).filter(|p| !p.is_empty()) {
                        self.sailjail_permissions.insert(p.to_string());
                    }
                }
            }
        }
    }

    fn record_imports<'a>(
        &mut self,
        code: &'a str,
        parse: fn(&'a str) -> Option<Import<'a>>,
    ) -> SilicaScope {
        let mut scope = SilicaScope::default();
        // The first import that has a `Screen` decides what an unqualified
        // `Screen` is (Qt 6 type lookup): Qt Quick's from QtQuick (without
        // a version, 2.15 or 6.x) or QtQuick.Window; Silica's from
        // Sailfish.Silica, or from QtQuick 2.0 to 2.14, whose keel/qt5compat
        // shims re-export Keel.SilicaScreen after Qt Quick.
        let mut screen_decided = false;
        for line in code.lines() {
            let Some(import) = parse(line) else {
                continue;
            };
            if !screen_decided {
                match import.module {
                    "QtQuick" if import.version.is_some_and(shimmed_qt_quick) => {
                        screen_decided = true;
                    }
                    "QtQuick" | "QtQuick.Window" => {
                        scope.qt_quick_screen = true;
                        screen_decided = true;
                    }
                    SILICA_MODULE if import.alias.is_none() => screen_decided = true,
                    _ => {}
                }
            }
            if import.module == SILICA_MODULE {
                match import.alias {
                    Some(alias) => scope.aliases.push(alias.to_string()),
                    None => scope.unqualified = true,
                }
            }
            *self.modules.entry(import.module.to_string()).or_default() += 1;
        }
        scope
    }
}

/// How Silica names are visible in one file.
#[derive(Default)]
struct SilicaScope {
    unqualified: bool,
    aliases: Vec<String>,
    /// An unqualified `Screen` is Qt Quick's attached `Screen`, imported
    /// before Silica's (keel/silica/COMPATIBILITY.md, Screen).
    qt_quick_screen: bool,
}

struct Import<'a> {
    module: &'a str,
    version: Option<&'a str>,
    alias: Option<&'a str>,
}

/// `import QtQuick 2.0` to `2.14` goes through keel/qt5compat's shims.
fn shimmed_qt_quick(version: &str) -> bool {
    version
        .strip_prefix("2.")
        .and_then(|minor| minor.parse::<u32>().ok())
        .is_some_and(|minor| minor <= 14)
}

/// Parses `import Module.Name [1.0] [as Alias]`. Directory and JavaScript
/// imports are quoted and have already been blanked, so they yield `None`.
fn parse_import(line: &str) -> Option<Import<'_>> {
    parse_import_with(line, "import")
}

/// Parses a JavaScript `.import Module.Name 1.0 as Alias` directive.
fn parse_js_import(line: &str) -> Option<Import<'_>> {
    parse_import_with(line, ".import")
}

fn parse_import_with<'a>(line: &'a str, keyword: &str) -> Option<Import<'a>> {
    let mut words = line.trim().trim_end_matches(';').split_whitespace();
    if words.next()? != keyword {
        return None;
    }
    let module = words.next()?;
    // `import "file.js" as Name` leaves `as` here once the string is blanked.
    if module == "as" || !module.chars().next()?.is_ascii_alphabetic() {
        return None;
    }
    let mut alias = None;
    let mut version = None;
    while let Some(word) = words.next() {
        if word == "as" {
            alias = words.next().map(|a| a.trim_end_matches(';'));
        } else if version.is_none() && word.starts_with(|c: char| c.is_ascii_digit()) {
            version = Some(word.trim_end_matches(';'));
        }
    }
    Some(Import {
        module,
        version,
        alias,
    })
}

/// Collects catalogued Silica names used in `code`: object declarations
/// (`Name {`, when `declarations` is set) and member references
/// (`Name.member`, e.g. `Theme.paddingLarge`, `PageStatus.Active`), either
/// unqualified or behind a Silica import alias. An unqualified
/// `Screen.<member>` of a Silica-only member, in a file where `Screen` is Qt
/// Quick's, is recorded as `Screen.<member>` as well.
fn silica_refs<'a>(code: &'a str, scope: &SilicaScope, declarations: bool, out: &mut Vec<&'a str>) {
    if !scope.unqualified && scope.aliases.is_empty() {
        return;
    }
    let bytes = code.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        let starts_chain = is_ident_start(c)
            && (i == 0 || !(is_ident(bytes[i - 1] as char) || bytes[i - 1] == b'.'));
        if !starts_chain {
            i += 1;
            continue;
        }
        let start = i;
        while i < bytes.len() && (is_ident(bytes[i] as char) || bytes[i] == b'.') {
            i += 1;
        }
        let chain = code[start..i].trim_end_matches('.');
        let mut j = i;
        while j < bytes.len() && (bytes[j] as char).is_whitespace() {
            j += 1;
        }
        let declares = j < bytes.len() && bytes[j] == b'{';

        let mut segments = chain.split('.');
        let first = segments.next().unwrap_or("");
        let second = segments.next();
        let (name, has_member) = if scope.aliases.iter().any(|a| a == first) {
            match second {
                Some(name) => (name, segments.next().is_some()),
                None => continue,
            }
        } else if scope.unqualified {
            if let (true, "Screen", Some(member)) = (scope.qt_quick_screen, first, second) {
                if catalog::SCREEN_SILICA_MEMBERS.contains(&member) {
                    out.push(&chain[..first.len() + 1 + member.len()]);
                }
            }
            (first, second.is_some())
        } else {
            continue;
        };
        let is_type = name.chars().next().is_some_and(|c| c.is_ascii_uppercase());
        let used = has_member || (declarations && declares);
        if is_type && used && catalog::lookup(catalog::SILICA_TYPES, name).is_some() {
            out.push(name);
        }
    }
}

/// Reverse-DNS or capitalised identifiers that could be a QML module URI.
fn looks_like_module_uri(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 100
        && s.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && s.chars().all(|c| is_ident(c) || c == '.')
        && !s.ends_with('.')
        && !s.contains("..")
}

fn qmake_assignments(text: &str) -> Vec<(&str, Vec<&str>)> {
    let mut out = Vec::new();
    for statement in split_qmake_statements(text) {
        let Some((lhs, rhs)) = statement.split_once('=') else {
            continue;
        };
        if lhs.ends_with('-') {
            continue;
        }
        let lhs = lhs.trim_end_matches(['+', '*', '~']).trim_end();
        let var = lhs
            .rsplit(|c: char| c.is_whitespace() || c == '{' || c == ':')
            .next()
            .unwrap_or(lhs);
        let values = rhs.split_whitespace().filter(|v| *v != "\\").collect();
        out.push((var, values));
    }
    out
}

/// Splits qmake text into statements, joining backslash-continued lines.
fn split_qmake_statements(text: &str) -> Vec<&str> {
    let mut statements = Vec::new();
    let mut start = 0;
    let bytes = text.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'\n' {
            let line = text[start..i].trim_end();
            if !line.ends_with('\\') {
                statements.push(&text[start..i]);
                start = i + 1;
            }
        }
    }
    statements.push(&text[start..]);
    statements
}

fn strip_hash_comments(source: &str) -> String {
    source
        .lines()
        .map(|l| l.split_once('#').map_or(l, |(code, _)| code))
        .collect::<Vec<_>>()
        .join("\n")
}

fn words(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| c.is_whitespace() || matches!(c, '(' | ')' | '"' | '{' | '}'))
        .filter(|w| !w.is_empty())
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_ident(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Replaces comments and the contents of string literals with spaces,
/// keeping newlines so line structure survives. Also returns the contents
/// of double-quoted string literals.
fn lex(source: &str) -> (String, Vec<String>) {
    let mut out = String::with_capacity(source.len());
    let mut strings = Vec::new();
    let mut chars = source.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '/' if chars.peek() == Some(&'/') => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut prev = '\0';
                for c in chars.by_ref() {
                    if c == '\n' {
                        out.push('\n');
                    }
                    if prev == '*' && c == '/' {
                        break;
                    }
                    prev = c;
                }
                out.push(' ');
            }
            '"' | '\'' | '`' => {
                out.push(' ');
                let quote = c;
                let mut content = String::new();
                while let Some(c) = chars.next() {
                    match c {
                        '\\' => {
                            chars.next();
                        }
                        '\n' => {
                            out.push('\n');
                            // C/C++ strings cannot span lines; a stray quote
                            // (e.g. an apostrophe in a C++ comment we missed)
                            // must not swallow the rest of the file.
                            if quote != '`' {
                                break;
                            }
                        }
                        c if c == quote => break,
                        c => content.push(c),
                    }
                }
                if quote == '"' {
                    strings.push(content);
                }
                out.push(' ');
            }
            c => out.push(c),
        }
    }
    (out, strings)
}

#[cfg(test)]
mod tests {
    #[test]
    fn cxxqt_build_registers_the_apps_own_module() {
        let mut f = super::Findings::default();
        f.add_cxxqt_build(
            r#"let builder = CxxQtBuilder::new_qml_module(QmlModule::new("Shipwright.Keys").version(1, 0));
               let other = QmlModule::new( "Shipwright.Keys.Extra" );"#,
        );
        assert!(f.app_modules.contains("Shipwright.Keys"));
        assert!(f.app_modules.contains("Shipwright.Keys.Extra"));
    }

    use super::*;

    #[test]
    fn finds_unqualified_silica_types() {
        let mut f = Findings::default();
        f.add_qml(
            r#"
            import QtQuick 2.0
            import Sailfish.Silica 1.0
            import "../components"
            Page {
                // Button { } is commented out
                SilicaListView {
                    header: PageHeader { title: "Button {" }
                    delegate: ListItem { width: Theme.itemSizeLarge }
                }
            }
            "#,
        );
        assert_eq!(f.modules.get("QtQuick"), Some(&1));
        assert_eq!(f.modules.get("Sailfish.Silica"), Some(&1));
        assert_eq!(f.modules.len(), 2);
        assert_eq!(f.silica_types.get("Page"), Some(&1));
        assert_eq!(f.silica_types.get("SilicaListView"), Some(&1));
        assert_eq!(f.silica_types.get("PageHeader"), Some(&1));
        assert_eq!(f.silica_types.get("ListItem"), Some(&1));
        assert_eq!(f.silica_types.get("Theme"), Some(&1));
        assert_eq!(f.silica_types.get("Button"), None);
    }

    #[test]
    fn honours_silica_alias() {
        let mut f = Findings::default();
        f.add_qml(
            "import QtQuick 2.6\nimport Sailfish.Silica 1.0 as S\n\
             Item { S.Button { x: S.Theme.paddingLarge } Page { } }",
        );
        assert_eq!(f.silica_types.get("Button"), Some(&1));
        assert_eq!(f.silica_types.get("Theme"), Some(&1));
        // Unqualified `Page` is not Silica's when Silica is only imported with an alias.
        assert_eq!(f.silica_types.get("Page"), None);
    }

    #[test]
    fn ignores_silica_names_without_silica_import() {
        let mut f = Findings::default();
        f.add_qml("import QtQuick 2.0\nItem { Button { } }");
        assert!(f.silica_types.is_empty());
    }

    #[test]
    fn finds_sailfishapp_calls() {
        let mut f = Findings::default();
        f.add_native(
            "#include <sailfishapp.h>\n\
             // SailfishApp::createView() not used\n\
             int main(int argc, char *argv[]) {\n\
                 return SailfishApp::main(argc, argv);\n\
             }\n\
             QUrl u = SailfishApp::pathTo(\"qml/x.qml\");",
        );
        assert_eq!(f.sailfishapp_apis.get("SailfishApp::main"), Some(&1));
        assert_eq!(f.sailfishapp_apis.get("SailfishApp::pathTo"), Some(&1));
        assert_eq!(f.sailfishapp_apis.len(), 2);
        assert_eq!(f.build.get("header sailfishapp"), Some(&1));
    }

    // Regression (corpus: foilauth, bitsailor): `import "x.js" as X` was
    // reported as an unknown module named `as`.
    #[test]
    fn quoted_js_import_is_not_a_module() {
        let mut f = Findings::default();
        f.add_qml("import QtQuick 2.0\nimport \"Constants.js\" as Constants\nItem { }");
        assert_eq!(f.modules.keys().collect::<Vec<_>>(), ["QtQuick"]);
    }

    // Regression (corpus: all 19 apps): enum and singleton references such as
    // `Orientation.All`, `PageStatus.Active` and `Screen.width` were not
    // counted; only `Theme` was special-cased.
    #[test]
    fn counts_silica_enum_and_singleton_references() {
        let mut f = Findings::default();
        f.add_qml(
            "import QtQuick 2.0\nimport Sailfish.Silica 1.0\n\
             Page { allowedOrientations: Orientation.All\n\
             onStatusChanged: if (status === PageStatus.Active) x = Screen.width\n\
             Label { truncationMode: TruncationMode.Fade; font.weight: Font.Bold } }",
        );
        assert_eq!(f.silica_types.get("Orientation"), Some(&1));
        assert_eq!(f.silica_types.get("PageStatus"), Some(&1));
        assert_eq!(f.silica_types.get("Screen"), Some(&1));
        assert_eq!(f.silica_types.get("TruncationMode"), Some(&1));
        // `font.weight` is a property path and `Font` is QtQuick's.
        assert_eq!(f.silica_types.get("Font"), None);
    }

    // Keel on Qt 6: an unqualified `Screen` is Qt Quick's when Qt Quick (not
    // through the 2.x shims) or QtQuick.Window is imported before Silica, so
    // Silica-only members need `S.Screen` or Silica imported first
    // (keel/silica/COMPATIBILITY.md, Screen).
    #[test]
    fn flags_silica_only_members_of_unqualified_screen() {
        let mut f = Findings::default();
        f.add_qml(
            "import QtQuick\nimport Sailfish.Silica 1.0\n\
             Page { property bool big: Screen.sizeCategory >= Screen.Large\n\
             width: Screen.width * Screen.widthRatio\n\
             y: Screen.topCutout.height; x: Screen.height }",
        );
        assert_eq!(f.silica_types.get("Screen"), Some(&6));
        assert_eq!(f.silica_types.get("Screen.sizeCategory"), Some(&1));
        assert_eq!(f.silica_types.get("Screen.Large"), Some(&1));
        assert_eq!(f.silica_types.get("Screen.widthRatio"), Some(&1));
        assert_eq!(f.silica_types.get("Screen.topCutout"), Some(&1));
        // Qt Quick's Screen has width and height.
        assert_eq!(f.silica_types.get("Screen.width"), None);
        assert_eq!(f.silica_types.get("Screen.height"), None);
    }

    #[test]
    fn screen_is_silicas_when_imported_first_or_through_the_shims() {
        for imports in [
            "import Sailfish.Silica 1.0\nimport QtQuick\n",
            "import QtQuick 2.6\nimport Sailfish.Silica 1.0\n",
            "import QtQuick 2.0\nimport QtQuick.Window 2.2\nimport Sailfish.Silica 1.0\n",
            "import Sailfish.Silica 1.0\nimport QtQuick.Window 2.2\nimport QtQuick 6.5\n",
        ] {
            let mut f = Findings::default();
            f.add_qml(&format!(
                "{imports}Page {{ property bool big: Screen.sizeCategory >= Screen.Large }}"
            ));
            assert_eq!(f.silica_types.get("Screen"), Some(&2), "{imports}");
            assert!(f.silica_types.keys().all(|k| !k.contains('.')), "{imports}");
        }
        for imports in [
            "import QtQuick 2.15\nimport Sailfish.Silica 1.0\n",
            "import QtQuick 6.5\nimport Sailfish.Silica 1.0\n",
            "import QtQuick.Window 2.2\nimport QtQuick 2.6\nimport Sailfish.Silica 1.0\n",
        ] {
            let mut f = Findings::default();
            f.add_qml(&format!(
                "{imports}Page {{ property bool big: Screen.sizeCategory >= Screen.Large }}"
            ));
            assert_eq!(
                f.silica_types.get("Screen.sizeCategory"),
                Some(&1),
                "{imports}"
            );
        }
    }

    #[test]
    fn qualified_screen_members_are_not_flagged() {
        let mut f = Findings::default();
        f.add_qml(
            "import QtQuick 2.6\nimport Sailfish.Silica 1.0\n\
             import Sailfish.Silica 1.0 as S\n\
             Page { property bool big: S.Screen.sizeCategory >= S.Screen.Large }",
        );
        f.add_js(
            ".pragma library\n.import Sailfish.Silica 1.0 as Silica\n\
             var h = Silica.Screen.topCutout.height",
        );
        assert_eq!(f.silica_types.get("Screen"), Some(&3));
        assert!(f.silica_types.keys().all(|k| !k.contains('.')));
    }

    #[test]
    fn screen_members_flagged_only_when_qt_quick_screen_is_in_scope() {
        // No QtQuick import: Silica's Screen is the only one.
        let mut f = Findings::default();
        f.add_qml("import Sailfish.Silica 1.0\nPage { x: Screen.sizeCategory }");
        assert_eq!(f.silica_types.get("Screen.sizeCategory"), None);
        // QtQuick.Window also brings Qt Quick's Screen.
        f.add_qml(
            "import QtQuick.Window 2.2\nimport Sailfish.Silica 1.0\n\
             Page { x: Screen.sizeCategory }",
        );
        assert_eq!(f.silica_types.get("Screen.sizeCategory"), Some(&1));
        // Non-library JavaScript runs in its importer's scope, which in a
        // Sailfish app imports QtQuick 2.x (shimmed) or Silica first.
        f.add_js("function big() { return Screen.sizeCategory > Screen.Medium }");
        assert_eq!(f.silica_types.get("Screen.sizeCategory"), Some(&1));
        assert_eq!(f.silica_types.get("Screen.Medium"), None);
    }

    // Regression (corpus: foilauth, foilnotes, yubikey, wordle, counter):
    // JavaScript files importing Silica were not scanned.
    #[test]
    fn scans_javascript_that_imports_silica() {
        let mut f = Findings::default();
        f.add_js(
            ".pragma library\n.import Sailfish.Silica 1.0 as Silica\n\
             var h = ('topCutout' in Silica.Screen) ? Silica.Screen.topCutout.height : 0",
        );
        assert_eq!(f.js_files, 1);
        assert_eq!(f.modules.get("Sailfish.Silica"), Some(&1));
        assert_eq!(f.silica_types.get("Screen"), Some(&1));
    }

    // Regression (corpus: bitsailor helpers.js): a non-library JavaScript file
    // runs in its importer's scope and can use Silica names unqualified.
    #[test]
    fn scans_non_library_javascript_in_importer_scope() {
        let mut f = Findings::default();
        f.add_js("function ok(page) { return page.status === PageStatus.Active }");
        assert_eq!(f.silica_types.get("PageStatus"), Some(&1));
        assert_eq!(f.js_files, 1);
    }

    #[test]
    fn skips_irrelevant_javascript() {
        let mut f = Findings::default();
        f.add_js(".pragma library\nfunction add(a, b) { return Math.max(a, b) }");
        f.add_js("function f() { return Theme.x }\n.pragma library\n");
        assert_eq!(f.js_files, 0);
        assert!(f.silica_types.is_empty());
    }

    // Regression (corpus: foilauth, bitsailor, mashka and others): app-provided
    // modules registered with qmlRegisterType were reported as unknown blockers.
    #[test]
    fn records_app_registered_module_uris() {
        let mut f = Findings::default();
        f.add_native(
            "#define APP_QML_IMPORT \"harbour.wordle\"\n\
             qmlRegisterType<Mashka>(\"harbour.mashka\", 1, 0, \"Mashka\");\n\
             QString s = \"not a uri!\";",
        );
        f.add_qmldir("module Communi\nplugin communiplugin\n");
        assert!(f.app_modules.contains("harbour.wordle"));
        assert!(f.app_modules.contains("harbour.mashka"));
        assert!(f.app_modules.contains("Communi"));
        assert!(!f.app_modules.contains("not a uri!"));
    }

    #[test]
    fn reads_qmake_sailfishapp_config_and_pkgconfig() {
        let mut f = Findings::default();
        f.add_build(
            "TARGET = harbour-x\n\
             CONFIG += sailfishapp link_pkgconfig # comment sailfishapp_qml\n\
             PKGCONFIG += sailfishapp mlite5 \\\n    glib-2.0 keepalive\n\
             CONFIG += sailfishapp_i18n\n",
            false,
        );
        assert_eq!(f.build.get("CONFIG sailfishapp"), Some(&1));
        assert_eq!(f.build.get("CONFIG sailfishapp_i18n"), Some(&1));
        assert_eq!(f.build.get("CONFIG sailfishapp_qml"), None);
        assert_eq!(f.build.get("pkg-config sailfishapp"), Some(&1));
        assert_eq!(f.build.get("pkg-config mlite5"), Some(&1));
        assert_eq!(f.build.get("pkg-config keepalive"), Some(&1));
        // Not a Sailfish platform library.
        assert_eq!(f.build.get("pkg-config glib-2.0"), None);
    }

    #[test]
    fn reads_cmake_sailfishapp() {
        let mut f = Findings::default();
        f.add_build(
            "pkg_search_module(SAILFISH sailfishapp REQUIRED)\n\
             target_link_libraries(app PkgConfig::sailfishapp Qt5::Quick)\n",
            true,
        );
        assert_eq!(f.build.get("pkg-config sailfishapp"), Some(&2));
    }

    #[test]
    fn reads_sailjail_permissions() {
        let mut f = Findings::default();
        f.add_desktop(
            "[Desktop Entry]\nName=X\nPermissions=Ignored\n\n\
             [X-Sailjail]\nPermissions=Internet;Pictures;\nOrganizationName=org\n",
        );
        assert_eq!(f.sailjail_sections, 1);
        assert_eq!(
            f.sailjail_permissions.iter().collect::<Vec<_>>(),
            ["Internet", "Pictures"]
        );
    }
}
