# keel-compat

Scans a Sailfish app's sources and reports which QML imports, Silica types and `SailfishApp` APIs it uses, whether Keel covers each one, a score (the share of distinct items reachable with Keel without source changes), and static estimates of the compatibility tiers A and B from `docs/plan.md`. The measured tiers come from loading the app on Keel (`corpus/harness`, `corpus/README.md`).

```
keel-compat [--json] [--fail-on-blockers] [--exclude <dir>]... <path>...
```

- `--json` prints the report as JSON (below). `corpus/run-corpus.sh` consumes it.
- `--fail-on-blockers` exits with status 1 when any scored item is missing or unknown, for use in CI.
- `--exclude <dir>` skips a directory relative to each scanned path, e.g. a desktop or MeeGo variant of the UI that is not built for Sailfish. Repeatable.

Exit status is 2 for usage and I/O errors.

## What it reads

| Files | What it looks for |
| --- | --- |
| `.qml` | `import` lines, `Name {` object declarations and `Name.member` references (`Theme.paddingLarge`, `PageStatus.Active`, `Orientation.All`) of catalogued Silica names, unqualified or through a Silica import alias |
| `.js` | `.import` directives, and Silica names used through `.import Sailfish.Silica ... as X` or, in files without `.pragma library`, unqualified (such files run in their importing QML file's scope). Files with neither are skipped and not counted |
| `.cpp`, `.cc`, `.cxx`, `.h`, `.hpp` | `SailfishApp::` calls, `#include`s of Sailfish platform libraries, and string literals that look like QML module URIs (as passed to `qmlRegisterType`) |
| `.pro`, `.pri` | `CONFIG += sailfishapp*` features and `PKGCONFIG` platform libraries |
| `CMakeLists.txt`, `.cmake` | pkg-config platform libraries (`pkg_search_module(... sailfishapp)`, `PkgConfig::sailfishapp`) |
| `qmldir` | `module` lines of QML plugins bundled with the app |
| `.desktop` | `Permissions` in the `[X-Sailjail]` section |

Hidden directories, `target`, `build` and `node_modules` are skipped.

## Statuses

| Status | Meaning | Scored |
| --- | --- | --- |
| `supported` | works under Qt6 + Keel today (`implemented` in `keel/silica/COMPATIBILITY.md`) | yes, reachable |
| `partial` | provided by Keel with documented gaps (`partial` in COMPATIBILITY.md); listed under "Partial" with the reason | yes, reachable |
| `missing` | not provided by Keel, or needing a source change | yes, blocker |
| `unknown` | not in the catalogue | yes, blocker |
| `app` | a QML module the app provides itself (its URI appears as a string in the app's native code or in a bundled `qmldir`); it is rebuilt with the app | no |

The **Build integration** section (qmake `sailfishapp` features, platform pkg-config modules and headers such as `sailfishapp`, `mlite5`, `keepalive`) is reported for tier D planning but not scored. Sailjail permissions are reported as found.

`Screen` is `partial`: on Qt 6 an unqualified `Screen` resolves to Qt Quick's attached `Screen` whenever the file imports QtQuick (Qt 6.4: when QtQuick is imported first; Qt 6.8: always), so the Silica-only members are undefined. When a QML file that imports QtQuick or QtQuick.Window (or a non-library JavaScript file, which runs in such a file's scope) uses `Screen.sizeCategory`, `Screen.widthRatio`, `Screen.topCutout` or the size enums `Screen.Small`, `Medium`, `Large`, `ExtraLarge` unqualified, the scan records a `Screen.<member>` row with status `missing`, and the blocker's note gives the one-line fix: `import Sailfish.Silica 1.0 as S` and `S.Screen.<member>`. Uses through a Silica alias (`S.Screen.sizeCategory`, JavaScript `.import Sailfish.Silica 1.0 as Silica`) are fine. `Screen.width` and `Screen.height` exist on both and are not flagged.

Static tiers: **A** is `reached` when `Sailfish.Silica` and any `Sailfish.Silica.*` import are reachable (a private Silica import blocks it), `n/a` when the app does not import Silica. **B** additionally needs every used Silica type reachable. C to E need builds and are always `not-measured`.

## JSON

```
{
  "schema": 2,
  "tool_version": "0.1.0",
  "files": { "qml": 30, "native": 26, "js": 1, "build": 3, "desktop": 1 },
  "sections": [
    { "title": "QML imports", "scored": true,
      "rows": [ { "name": "Nemo.Notifications", "uses": 3, "status": "supported", "note": "keel/nemo-compat" } ] },
    { "title": "Silica types", "scored": true, "rows": [ ... ] },
    { "title": "SailfishApp APIs", "scored": true, "rows": [ ... ] },
    { "title": "Build integration", "scored": false, "rows": [ ... ] }
  ],
  "sailjail": { "sections": 1, "permissions": ["Internet"] },
  "blockers": [ { "section": "Silica types", "name": "Screen.sizeCategory", "status": "missing",
                  "note": "On Qt 6 an unqualified Screen resolves to Qt Quick's Screen ..." } ],
  "partial": [ { "section": "Silica types", "name": "Label", "note": "Keel 0.1, partial: TruncationMode.Fade ..." } ],
  "score": 92,
  "tiers": { "A": "reached", "B": "blocked", "C": "not-measured", "D": "not-measured", "E": "not-measured" }
}
```

`sailjail` is `null` when no `.desktop` file has an `[X-Sailjail]` section; `score` is `null` when nothing was scored. Tier values are `reached`, `blocked`, `not-applicable` and `not-measured`. `schema` changes only on incompatible changes. Schema 2 (2026-10-01) dropped the `planned` status and tier value (Keel has shipped), added the `partial` status and list, and added `note` to blockers.

## Catalogue

`src/catalog.rs` holds public names only. The Silica types are exactly the 101 rows of `keel/silica/COMPATIBILITY.md` (implemented → `supported`, partial → `partial` with the reason as the note, missing → `missing`); a unit test reads that file and fails when the two disagree, so update both together. The rows come from the public Sailfish Silica reference ("All Sailfish Silica types") plus the enum and singleton names its pages document. Names seen in corpus apps but absent from the public reference (`GlassItem`, `PanelBackground`, `IconMenuItem`, `FullscreenContentPage`, `FocusBehavior`, `OpacityRamp`) are not catalogued, so the scan does not see them; the load harness does. `Nemo.DBus`, `Nemo.Notifications`, `Nemo.KeepAlive` and their legacy `org.nemomobile.{dbus,notifications,keepalive}` URIs are `supported` (keel/nemo-compat), as are the five `SailfishApp::` functions and `pkg-config sailfishapp` (keel/sailfishapp).

## Limitations

The scan is lexical, not a QML parse. A Silica name is only counted when the file imports Silica (unqualified or by alias), and only names in the catalogue are counted, so an uncatalogued Silica type is invisible rather than `unknown`. An app-local component that shadows a Silica type name (a `Label.qml` next to a file importing Silica) is still counted as Silica's. `Screen` is counted as Silica's even when `QtQuick.Window` is also imported (and its Silica-only members are flagged as above). URIs registered through a variable rather than a string literal are not seen, and module URIs that happen to appear as strings in native code count as app-provided.
