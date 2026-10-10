// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: MIT

//! Turns scan findings into a scored report, as text or JSON.

use std::collections::BTreeMap;
use std::fmt;

use serde::Serialize;

use crate::catalog::{self, Entry, Status, SILICA_MODULE};
use crate::scan::Findings;

/// Bumped when the JSON shape changes incompatibly.
pub const JSON_SCHEMA: u32 = 2;

#[derive(Serialize)]
pub struct Row {
    pub name: String,
    pub uses: usize,
    pub status: Status,
    pub note: &'static str,
}

#[derive(Serialize)]
pub struct Section {
    pub title: &'static str,
    /// Whether rows count towards the score and blocker list.
    pub scored: bool,
    pub rows: Vec<Row>,
}

#[derive(Serialize)]
pub struct Files {
    pub qml: usize,
    pub native: usize,
    /// JavaScript files with a module import or Silica usage; others are skipped.
    pub js: usize,
    pub build: usize,
    pub desktop: usize,
}

#[derive(Serialize)]
pub struct Sailjail {
    pub sections: usize,
    pub permissions: Vec<String>,
}

/// How far an app gets up the plan's compatibility tiers, judged from the
/// catalogue alone. Tiers C to E need builds and device runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Tier {
    /// Every relevant item is supported (or partial) in Keel today.
    Reached,
    /// At least one relevant item is missing or unknown.
    Blocked,
    /// The app does not use Silica, so the tier says nothing about it.
    NotApplicable,
    NotMeasured,
}

impl fmt::Display for Tier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(match self {
            Tier::Reached => "reached",
            Tier::Blocked => "blocked",
            Tier::NotApplicable => "n/a",
            Tier::NotMeasured => "not measured",
        })
    }
}

#[derive(Serialize)]
pub struct Tiers {
    /// `import Sailfish.Silica` resolves.
    #[serde(rename = "A")]
    pub a: Tier,
    /// Every Silica type the app uses is reachable (supported or partial).
    #[serde(rename = "B")]
    pub b: Tier,
    #[serde(rename = "C")]
    pub c: Tier,
    #[serde(rename = "D")]
    pub d: Tier,
    #[serde(rename = "E")]
    pub e: Tier,
}

#[derive(Serialize)]
pub struct Blocker {
    pub section: &'static str,
    pub name: String,
    pub status: Status,
    /// Why, from the catalogue (for `Screen.<member>`, the source change).
    pub note: &'static str,
}

/// A partial item: reachable, so not a blocker, but with documented gaps.
#[derive(Serialize)]
pub struct Flag {
    pub section: &'static str,
    pub name: String,
    pub note: &'static str,
}

#[derive(Serialize)]
pub struct Report {
    pub schema: u32,
    pub tool_version: &'static str,
    pub files: Files,
    pub sections: Vec<Section>,
    pub sailjail: Option<Sailjail>,
    pub blockers: Vec<Blocker>,
    /// Partial items the app uses, with the reason from the catalogue.
    pub partial: Vec<Flag>,
    /// Percentage of distinct scored items reachable at Keel v1.
    pub score: Option<u32>,
    pub tiers: Tiers,
}

const IMPORTS: &str = "QML imports";
const SILICA: &str = "Silica types";

impl Report {
    pub fn new(findings: &Findings) -> Self {
        let mut imports = section(IMPORTS, catalog::MODULES, &findings.modules);
        for row in &mut imports.rows {
            if row.status == Status::Unknown && findings.app_modules.contains(&row.name) {
                row.status = Status::App;
                row.note = "registered by the app's own code or bundled plugin";
            }
        }
        sort_rows(&mut imports.rows);
        let sections = vec![
            imports,
            section(SILICA, catalog::SILICA_TYPES, &findings.silica_types),
            section(
                "SailfishApp APIs",
                catalog::SAILFISHAPP_APIS,
                &findings.sailfishapp_apis,
            ),
            Section {
                scored: false,
                ..section("Build integration", catalog::BUILD, &findings.build)
            },
        ];
        let blockers = sections
            .iter()
            .filter(|s| s.scored)
            .flat_map(|s| s.rows.iter().map(move |r| (s.title, r)))
            .filter(|(_, r)| r.status.scored() && !r.status.reachable())
            .map(|(section, r)| Blocker {
                section,
                name: r.name.clone(),
                status: r.status,
                note: r.note,
            })
            .collect();
        let partial = sections
            .iter()
            .filter(|s| s.scored)
            .flat_map(|s| s.rows.iter().map(move |r| (s.title, r)))
            .filter(|(_, r)| r.status == Status::Partial)
            .map(|(section, r)| Flag {
                section,
                name: r.name.clone(),
                note: r.note,
            })
            .collect();
        let sailjail = (findings.sailjail_sections > 0).then(|| Sailjail {
            sections: findings.sailjail_sections,
            permissions: findings.sailjail_permissions.iter().cloned().collect(),
        });
        let mut report = Report {
            schema: JSON_SCHEMA,
            tool_version: env!("CARGO_PKG_VERSION"),
            files: Files {
                qml: findings.qml_files,
                native: findings.native_files,
                js: findings.js_files,
                build: findings.build_files,
                desktop: findings.desktop_files,
            },
            sections,
            sailjail,
            blockers,
            partial,
            score: None,
            tiers: Tiers {
                a: Tier::NotApplicable,
                b: Tier::NotApplicable,
                c: Tier::NotMeasured,
                d: Tier::NotMeasured,
                e: Tier::NotMeasured,
            },
        };
        report.score = report.compute_score();
        report.tiers.a = report.tier_a();
        report.tiers.b = match report.tiers.a {
            Tier::Blocked => Tier::Blocked,
            Tier::NotApplicable => Tier::NotApplicable,
            a => worst(a, Self::tier_of(report.rows_in(SILICA))),
        };
        report
    }

    fn scored_rows(&self) -> impl Iterator<Item = &Row> {
        self.sections
            .iter()
            .filter(|s| s.scored)
            .flat_map(|s| s.rows.iter())
            .filter(|r| r.status.scored())
    }

    fn rows_in(&self, title: &'static str) -> impl Iterator<Item = &Row> {
        self.sections
            .iter()
            .filter(move |s| s.title == title)
            .flat_map(|s| s.rows.iter())
    }

    fn compute_score(&self) -> Option<u32> {
        let total = self.scored_rows().count();
        if total == 0 {
            return None;
        }
        let reachable = self.scored_rows().filter(|r| r.status.reachable()).count();
        Some(u32::try_from(reachable * 100 / total).unwrap_or(100))
    }

    fn tier_a(&self) -> Tier {
        let silica: Vec<&Row> = self
            .rows_in(IMPORTS)
            .filter(|r| r.name == SILICA_MODULE || r.name.starts_with("Sailfish.Silica."))
            .collect();
        if !silica.iter().any(|r| r.name == SILICA_MODULE) {
            return Tier::NotApplicable;
        }
        Self::tier_of(silica.into_iter())
    }

    fn tier_of<'a>(rows: impl Iterator<Item = &'a Row>) -> Tier {
        rows.fold(Tier::Reached, |tier, row| {
            worst(
                tier,
                match row.status {
                    Status::Supported | Status::Partial | Status::App => Tier::Reached,
                    Status::Missing | Status::Unknown => Tier::Blocked,
                },
            )
        })
    }

    pub fn has_blockers(&self) -> bool {
        !self.blockers.is_empty()
    }
}

fn worst(a: Tier, b: Tier) -> Tier {
    let rank = |t: Tier| match t {
        Tier::Reached => 0,
        _ => 1,
    };
    if rank(b) > rank(a) {
        b
    } else {
        a
    }
}

fn section(
    title: &'static str,
    table: &'static [Entry],
    found: &BTreeMap<String, usize>,
) -> Section {
    let mut rows: Vec<Row> = found
        .iter()
        .map(|(name, &uses)| {
            let entry = catalog::lookup(table, name);
            Row {
                name: name.clone(),
                uses,
                status: entry.map_or(Status::Unknown, |e| e.status),
                note: entry.map_or("not in the Keel catalogue", |e| e.note),
            }
        })
        .collect();
    sort_rows(&mut rows);
    Section {
        title,
        scored: true,
        rows,
    }
}

fn sort_rows(rows: &mut [Row]) {
    rows.sort_by(|a, b| a.status.cmp(&b.status).then_with(|| a.name.cmp(&b.name)));
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let files = &self.files;
        writeln!(
            f,
            "Scanned {} QML, {} native, {} JavaScript, {} build and {} desktop files.",
            files.qml, files.native, files.js, files.build, files.desktop
        )?;
        for section in &self.sections {
            let suffix = if section.scored { "" } else { " (not scored)" };
            writeln!(f, "\n{}{suffix}", section.title)?;
            if section.rows.is_empty() {
                writeln!(f, "  (none)")?;
            }
            for row in &section.rows {
                writeln!(
                    f,
                    "  {:<10} {:<34} {:>4}x  {}",
                    row.status, row.name, row.uses, row.note
                )?;
            }
        }
        if let Some(sailjail) = &self.sailjail {
            writeln!(
                f,
                "\nSailjail: {} section(s); permissions: {}",
                sailjail.sections,
                if sailjail.permissions.is_empty() {
                    "(none)".to_string()
                } else {
                    sailjail.permissions.join(", ")
                }
            )?;
        } else {
            writeln!(f, "\nSailjail: no [X-Sailjail] section")?;
        }

        writeln!(f, "\nBlockers: {}", self.blockers.len())?;
        for b in &self.blockers {
            if b.name.starts_with("Screen.") {
                writeln!(f, "  {} ({}): {}", b.name, b.status, b.note)?;
            } else {
                writeln!(f, "  {} ({})", b.name, b.status)?;
            }
        }
        writeln!(f, "Partial (reachable, with gaps): {}", self.partial.len())?;
        for p in &self.partial {
            writeln!(f, "  {}: {}", p.name, p.note)?;
        }
        match self.score {
            Some(score) => writeln!(f, "Score: {score}% of distinct items reachable with Keel")?,
            None => writeln!(f, "Score: n/a (nothing to score)")?,
        }
        writeln!(
            f,
            "Static tiers: A {}, B {}, C-E not measured (need builds; see corpus/harness for load tests)",
            self.tiers.a, self.tiers.b
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scores_reachable_share_and_lists_blockers() {
        let mut findings = Findings::default();
        findings.add_qml(
            "import QtQuick\nimport Sailfish.Silica 1.0\nimport Foo.Bar 1.0\n\
             Page { Button { } width: Screen.sizeCategory }",
        );
        let report = Report::new(&findings);
        // Reachable: QtQuick, Sailfish.Silica, Page, Button, Screen. Blocked:
        // Foo.Bar, Screen.sizeCategory.
        assert_eq!(report.score, Some(71));
        assert!(report.has_blockers());
        let text = report.to_string();
        assert!(text.contains("Foo.Bar (unknown)"));
        assert!(text.contains("Screen.sizeCategory (missing)"));
        assert_eq!(report.tiers.a, Tier::Reached);
        assert_eq!(report.tiers.b, Tier::Blocked);
    }

    #[test]
    fn partial_items_are_reachable_but_flagged() {
        let mut findings = Findings::default();
        findings.add_qml(
            "import QtQuick 2.0\nimport QtMultimedia 5.6\nimport Sailfish.Silica 1.0\n\
             CoverBackground { Label { } Cover { } width: Screen.width; Palette { } }",
        );
        let report = Report::new(&findings);
        assert!(!report.has_blockers());
        assert_eq!(report.score, Some(100));
        let names: Vec<_> = report.partial.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["QtMultimedia"]);
        let text = report.to_string();
        assert!(text.contains("Partial (reachable, with gaps): 1"));
        assert!(text.contains("QtMultimedia: 5.x imports via keel/qt5compat"));
        let json = serde_json::to_value(&report).unwrap();
        assert_eq!(json["schema"], 2);
        assert_eq!(json["partial"][0]["section"], "QML imports");
        assert_eq!(json["sections"][1]["rows"][0]["status"], "supported");
    }

    #[test]
    fn unqualified_silica_screen_members_block() {
        let mut findings = Findings::default();
        findings.add_qml(
            "import QtQuick\nimport Sailfish.Silica 1.0\n\
             Page { width: Screen.sizeCategory > Screen.Medium ? Screen.width : 0 }",
        );
        let report = Report::new(&findings);
        let blockers: Vec<_> = report.blockers.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(blockers, ["Screen.Medium", "Screen.sizeCategory"]);
        assert!(report.blockers[0].note.contains("S.Screen"));
        assert_eq!(report.tiers.b, Tier::Blocked);
        let text = report.to_string();
        assert!(text.contains("Screen.sizeCategory (missing): On Qt 6 an unqualified Screen"));
    }

    #[test]
    fn empty_scan_has_no_score() {
        let report = Report::new(&Findings::default());
        assert_eq!(report.score, None);
        assert!(!report.has_blockers());
        assert_eq!(report.tiers.a, Tier::NotApplicable);
    }

    #[test]
    fn v1_types_are_supported() {
        let mut findings = Findings::default();
        findings.add_qml(
            "import QtQuick 2.0\nimport Sailfish.Silica 1.0\n\
             Page { SilicaListView { header: PageHeader { } \
             delegate: ListItem { Label { } BackgroundItem { } } \
             VerticalScrollDecorator { } } }",
        );
        let report = Report::new(&findings);
        assert!(!report.has_blockers());
        assert_eq!(report.score, Some(100));
        assert_eq!(report.tiers.b, Tier::Reached);
        assert!(report.partial.is_empty());
    }

    #[test]
    fn private_silica_blocks_tier_a() {
        let mut findings = Findings::default();
        findings.add_qml(
            "import QtQuick 2.0\nimport Sailfish.Silica 1.0\n\
             import Sailfish.Silica.private 1.0\nPage { }",
        );
        let report = Report::new(&findings);
        assert_eq!(report.tiers.a, Tier::Blocked);
        assert_eq!(report.tiers.b, Tier::Blocked);
    }

    // Regression (corpus): app-registered modules are neither blockers nor scored.
    #[test]
    fn app_modules_are_not_blockers() {
        let mut findings = Findings::default();
        findings.add_native("qmlRegisterType<Api>(\"harbour.sailhn\", 1, 0, \"Api\");");
        findings.add_qml("import QtQuick 2.0\nimport harbour.sailhn 1.0\nItem { }");
        let report = Report::new(&findings);
        assert!(!report.has_blockers());
        assert_eq!(report.score, Some(100));
        assert!(report.to_string().contains("app        harbour.sailhn"));
    }

    #[test]
    fn build_rows_are_not_scored() {
        let mut findings = Findings::default();
        findings.add_build("CONFIG += sailfishapp_qml\n", false);
        let report = Report::new(&findings);
        assert!(!report.has_blockers());
        assert_eq!(report.score, None);
        let json = serde_json::to_value(&report).unwrap();
        assert_eq!(json["sections"][3]["scored"], false);
        assert_eq!(
            json["sections"][3]["rows"][0]["name"],
            "CONFIG sailfishapp_qml"
        );
        assert_eq!(json["tiers"]["C"], "not-measured");
    }
}
