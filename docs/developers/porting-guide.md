<!--
SPDX-FileCopyrightText: 2026 Patrick Quinn
SPDX-License-Identifier: CC-BY-4.0
-->

# Porting a Sailfish app to Keel and publishing it on Reef

This guide is for developers with an existing Sailfish OS app (Qt 5.6, `Sailfish.Silica`) who want to ship it on Reef, the independent store. It covers five steps: measure the app with `keel-compat`, build it against Qt6 and Keel in the Sailfish SDK, launch it (Keel apps are ordinary Lipstick apps), package it, and sign and upload it.

Writing a new app rather than porting one? Start with [Getting started](getting-started.md): `keel new` generates a working app on Keel, and `keel run` runs it on your desktop.

Status, October 2026: Keel is in development. The developer programme opens after Reef's public launch; the roadmap puts it at July to September 2027. Anything below marked **[VERIFY]** has not been confirmed on a device yet.

## 1. Measure: keel-compat

`keel-compat` scans your sources and reports which QML imports, Silica types and `SailfishApp` APIs you use, and whether Keel covers each one.

```
cargo run -p keel-compat -- path/to/your-app              # human-readable report
cargo run -p keel-compat -- --json path/to/your-app       # what Reef's review runs
cargo run -p keel-compat -- --fail-on-blockers path/to/your-app   # for CI
```

Read the result in terms of tiers:

| Tier | Meaning | What you do |
| --- | --- | --- |
| A | Every `Sailfish.Silica` import is reachable in Keel | Rebuild against Qt6; expect some type-level work |
| B | A, and every Silica type you use is reachable | Rebuild; usually no QML changes |
| blocked | A missing or unknown type or import | Replace the blocker (the report names it) or wait for Keel to cover it |
| not-applicable | The app does not import Silica | Plain Qt6 QML; Keel's `sailfishapp` library still gives you the Lipstick window, cover and booster |

`planned` means covered by the Keel v1 scope but not yet shipped; `reached` means working today. Reef's review runs the same scan on the QML inside your RPM and shows the resulting tier on your store listing (see the review checklist).

The scan is lexical. It does not see types registered through variables, and it counts a local component that shadows a Silica name as Silica's. See `tools/keel-compat/README.md`.

## 2. Build against Qt6 and Keel in the SDK

Reef builds, and you should build, inside the Sailfish platform SDK, for each release you support:

- Target: `SailfishOS-<release>-aarch64` (aarch64 first; armv7hl only if Reef announces it).
- Qt6 comes from Chum's `chum:testing` repository on 5.2 (ADR-0013). Add it to your SDK target the way `tools/build/sdk/sdk.sh` does.
- Build with `mb2 -t SailfishOS-5.2.0.15-aarch64 -s rpm/<your-app>.spec build`.

Porting checklist for the code:

1. `import Sailfish.Silica 1.0` stays. Keel provides the module under the same URI for Qt6. Use only the types keel-compat reports as reachable.
2. QtQuick imports: Qt6 accepts versionless imports. `import QtQuick 2.0` still works; `QtGraphicalEffects` does not exist in Qt6 (use `Qt5Compat.GraphicalEffects`). When you move to `import QtQuick` (no version), write Silica-only `Screen` members qualified (`import Sailfish.Silica 1.0 as S`, then `S.Screen.sizeCategory`): Qt 6 takes an unqualified `Screen` from the first import that has one, Qt Quick's here (keel-compat flags this).
3. `SailfishApp::main`, `SailfishApp::pathTo` and friends come from Keel's `sailfishapp` compatibility library. Link it instead of the Qt5 `sailfishapp` pkg-config module.
4. Nemo and Sailfish platform modules keep their URIs. Keel ships Qt 6 builds of the ones apps use; the full list with what is partial is in `keel/silica/COMPATIBILITY.md` ("Platform modules") and `keel/platform/README.md`:
   - `keel/nemo-compat`: `Nemo.DBus`, `Nemo.Notifications`, `Nemo.KeepAlive`, `Nemo.Configuration`, `Nemo.Ngf` (vibration and sound feedback through ngfd), `Nemo.Policy` (the system resource policy), with their `org.nemomobile.*` aliases.
   - `keel/platform`: `Sailfish.Media`, `Sailfish.Pickers`, `Sailfish.Secrets` and `Sailfish.Crypto`, `Sailfish.WebView` (with `.Popups`, `.Pickers` and `.Controls`) and `Sailfish.WebEngine` (on Qt WebEngine; frame scripts run without Gecko's XPCOM), `org.nemomobile.accounts`, `org.nemomobile.mpris`, `Amber.Mpris`, `Amber.Web.Authorization`, `Nemo.Thumbnailer`, `Nemo.Mce`.
   - `Sailfish.Share` opens the system share dialog, and `ShareProvider` makes your app a share target.
   - `Sailfish.Accounts` (accounts, sign-in through signond, sync options) is provided too.
   - `Sailfish.Policy` reads the device policies an MDM sets (read only; a policy Keel cannot read counts as disabled).
   Each module is its own RPM; `Requires:` the ones you import (for example `shipwright-keel-platform-pickers`), or `shipwright-keel-platform` for all of them.
5. Qt 5 QML that Qt 6 dropped: `keel/qt5compat` provides `QtGraphicalEffects`, `QtFeedback`, Qt 5's `QtMultimedia` (including `Camera` with `VideoOutput { source: camera }`), `RegExpValidator` and `VisualItemModel`, so most apps run before you port those imports. Keel also rewrites the Qt 5 JavaScript Qt 6 refuses to compile (`const x;` without a value) in your own QML and JS when it loads them; set `KEEL_QT5COMPAT_SOURCE=0` once you have ported. Both are temporary: port to the Qt 6 API when you can.
6. C++: the usual Qt5-to-Qt6 changes (`QRegExp` to `QRegularExpression`, `QStringRef` removed, and so on). Rust apps use a Qt6 QML UI with a Rust core over CXX-Qt. A Rust app is built as a `cdylib` that is also executable (see [Getting started](getting-started.md) and `tools/keel-launcher/README.md`) so Keel's booster can load it; `keel new` sets this up.
7. Binary name: the executable must be `/usr/bin/<app id>`, and the app id follows Reef's naming rules (lowercase letters, digits and hyphens; see Store rules). Reverse-DNS names such as `org.example.weather` are not accepted as app ids.
8. Start-up: create the application and view with `SailfishApp::application()` and `SailfishApp::createView()` (or `SailfishApp::main()`). That is what puts the app straight into Lipstick and lets the booster hand it a prepared view.

## 3. Launch

Keel apps are normal Lipstick apps: they draw straight into Lipstick, Sailfish's compositor, as native Silica apps do (ADR-0016): the main window and the cover are Lipstick windows of your own process, with no extra compositor in between. Your desktop file:

```
[Desktop Entry]
Type=Application
Name=Weather Now
Icon=weather-now
Exec=/usr/bin/invoker --type=keel -A -- /usr/bin/weather-now
X-Nemo-Single-Instance=yes

[X-Sailjail]
OrganizationName=org.example
ApplicationName=weather-now
Permissions=Internet;Location
```

- `invoker --type=keel -A` starts the app in its own booster, `booster-keel@weather-now`, which keeps Qt 6, Silica and Keel loaded inside your app's sandbox, so it starts in about half the time (on the Jolla Phone, about 300 instead of 600 ms). Your package starts that booster with the session (`/usr/lib/systemd/user/user-session.target.d/50-weather-now.conf` with `[Unit]` and `Wants=booster-keel@weather-now.service`) and requires `shipwright-keel-booster`: without the booster running, `invoker -A` cannot start the app. `keel new` generates all of this. (`X-Nemo-Application-Type=keel` alone does not boost a sandboxed app: Lipstick then forces the sandbox, which the shared booster is not in.)
- Covers, orientation, activation, ambience and close events work through Keel; `keel/README.md` ("Runtime on the phone") describes how.
- Sailjail runs your binary under the permissions you declare. **[VERIFY]** Direct mode, the booster and Sailjail on a 5.2 phone are the first device checks (`tools/phase0/keel-runtime-test.sh`).
- Fallback: if direct mode does not work on a device, `keel-shell` (a nested compositor) still does: `Exec=/usr/bin/keel-shell -- /usr/bin/weather-now` with `X-Nemo-Application-Type=no-invoker`. See `keel/shell/README.md`.
- Test locally on your desktop with `keel run` (Getting started, section 3), or on a developer-mode phone over SSH by starting `/usr/bin/weather-now`.

### Optional: Keel Actions

A ported app can expose what it does to Pilot and other MCP clients with
Keel Actions: `import Keel.Actions 1.0` and declare `KeelAction`s whose
`onInvoked` handlers call the code your pages already use (ADR-0018;
reference in `keel/README.md`, "Keel Actions"; a worked example in
[Getting started](getting-started.md), section 3). Silica apps that stay on
Qt 5 cannot: actions are a Keel feature, and Pilot does not scrape UIs.

## 4. Package

Your spec's `Requires` names what you need at runtime: Qt6 modules, `shipwright-keel-silica`, `shipwright-keel-sailfishapp`, and the platform modules you import; add `Recommends: shipwright-keel-booster`. Reef mirrors all of them into its own signed repository, so users never add Chum themselves.

Rules the automated review enforces (full list in the review checklist):

- Package `Name` equals your app id. No `Epoch`, no `Obsoletes`.
- Files only under `/usr/bin/<app id>`, `/usr/share/<app id>/`, `/usr/lib*/<app id>/`, `/usr/libexec/<app id>/`, `/usr/share/applications/<app id>.desktop`, `/usr/share/icons/hicolor/<N>x<N>|scalable/apps/<app id>.png|svg`, `/usr/share/metainfo/<app id>.metainfo.xml`, and your licence and doc directories. With Keel Actions also `/usr/share/keel/actions/<K>.json`, `/usr/share/dbus-1/services/<K>.service` and `/usr/share/dbus-1/interfaces/<K>.actions.xml`, where `<K>` is your `OrganizationName.ApplicationName` and `ApplicationName` is your app id.
- Private libraries go under `/usr/lib64/<app id>/` or `/usr/share/<app id>/lib/` and must not be exported as `Provides`: add `%global __provides_exclude_from ^%{_datadir}/<app id>/.*$` (and the same for `%{_libdir}/<app id>`).
- Scriptlets: only the cache refreshes on the allow-list (`update-desktop-database`, `gtk-update-icon-cache` on `/usr/share/icons/hicolor`, `ldconfig`). No `%pretrans`, no triggers.
- A `[X-Sailjail]` section with `OrganizationName`, `ApplicationName` and `Permissions` (empty for none).

Paid apps check their licence offline with `reef-licence` (reef/licence/README.md) against the Reef public key embedded in the app, using your app id as the `app` claim. Hosted-service apps refresh their token from the licence service when online.

## 5. Sign and upload

1. Create an OpenPGP signing key (RSA 3072 is safest for the device's rpm version) and register its public half: `POST /v1/me/signing-keys`.
2. Sign: `rpmsign --addsign --define "_gpg_name <key id>" weather-now-1.0.0-1.aarch64.rpm`.
3. Upload once per release and architecture, saying where you tested it. Small builds can go in one request:

```
curl -H "Authorization: Bearer $REEF_API_KEY" -H 'Content-Type: application/x-rpm' \
  --data-binary @weather-now-1.0.0-1.aarch64.rpm \
  'https://dev.reefstore.app/v1/apps/weather-now/builds?release=5.2.0.17&arch=aarch64&tested_on=5.2.0.17,5.2.0.18'
```

   Larger builds (up to 100 MiB) use a multipart upload: `POST /v1/apps/{app}/uploads` with the size and the same query fields, `PUT` each part to the URL the response gives you, then complete it. The OpenAPI file has the exact calls.

Review is asynchronous on Reef's hosted portal. Either way the upload answers `202` with an upload to poll: `GET /v1/uploads/{id}` reports `state` `uploading`, then `reviewing`, then `done` with the build and its automated review report (build `status` `pending_review` when it passed and waits for a reviewer, `rejected` with the reasons otherwise), or `failed` with an `error`. The full API is in `upload-api.openapi.yaml` next to this guide.

A store listing shows only the releases a build was tested on. A new build never inherits the previous build's testing, so list every release you tested each time.
