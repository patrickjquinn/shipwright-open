<!--
SPDX-FileCopyrightText: 2026 Patrick Quinn
SPDX-License-Identifier: CC-BY-4.0
-->

# Getting started with a new app on Keel

This guide creates a Sailfish app, runs it in a window on your desktop and
packages it as an RPM. The app has a Rust core (CXX-Qt) and a Silica UI on Keel,
Shipwright's Qt 6 `Sailfish.Silica`. Porting an existing app instead? See
the [porting guide](porting-guide.md). Tool reference:
`tools/keel-dev/README.md`.

Checked on Ubuntu 24.04 (Qt 6.4.2), October 2026. Steps marked **[VERIFY]**
have not been run yet.

## 1. Prerequisites

A Shipwright checkout, rustup, and Qt 6 with CMake and Ninja:

```
apt-get install -y cmake ninja-build pkg-config librsvg2-bin rpm \
    qt6-base-dev qt6-base-private-dev qt6-declarative-dev qt6-declarative-private-dev \
    qt6-declarative-dev-tools qml6-module-qtquick qml6-module-qtquick-window \
    qml6-module-qtqml-workerscript qml6-module-qttest
```

Build the `keel` tool and keel-compat, and put them on your `PATH`:

```
cd Shipwright
cargo build -p keel-dev -p keel-compat
export PATH="$PWD/target/debug:$PATH"
```

## 2. Create an app

```
cd ..
keel new weather-now --id org.example.WeatherNow
cd weather-now
```

The name is your app id on Reef (package and binary name: lower-case
letters, digits, hyphens). `--id` is the QML module URI and the Sailjail
organisation and application name. `--license` picks the licence (default
MIT) and puts its text in `LICENSES/`. You get a CXX-Qt crate with one
QObject and its unit tests, the QML (a page with a header, a list and a
pull-down menu, and a cover with an action), start-up through Keel's
SailfishApp library, a desktop file with a Sailjail section, an icon, an RPM
spec, a smoke test and a CI workflow; `README.md` in the app lists them.

The crate is a `cdylib` that is also an executable: `src/lib.rs` exports
`main`, and `build.rs` links it so the same `lib<crate>.so` runs directly
and loads into Keel's booster (Rust's thread-locals need the shared-library
model for that; `tools/keel-launcher/README.md` explains). So there is no
`cargo run`: use `keel run`, which builds and starts the right file. The
spec installs `lib<crate>.so` as `/usr/bin/<name>`.

## 3. Run it on your desktop

```
keel run
```

The first time, this builds Keel from your Shipwright checkout into
`~/.cache/keel-dev/keel-build` (several minutes). Then it builds the app,
checks its QML with keel-compat (blockers and partial types are printed),
and opens a 540x960 window. If you already have a Keel build, pass
`--keel-build <dir>` or set `KEEL_BUILD_DIR`.

Look at it as other phones and settings:

```
keel run --list-devices
keel run --device jolla-c2 --ambience light
keel run --orientation landscape
```

Edit while it runs: `--watch` restarts the app when a file in `qml/`
changes, and rebuilds first when `src/`, `cpp/`, `build.rs` or
`Cargo.toml` change.

```
keel run --watch
```

Without a display (SSH, CI), `keel run` runs offscreen. Take a screenshot of
the first page instead of a window:

```
keel run --screenshot first-page.png
```

### Platform modules and Qt 5 code

Import Sailfish and Nemo modules by their usual URIs (`Sailfish.Pickers`,
`Sailfish.Share`, `Nemo.Notifications`, `Amber.Web.Authorization` and so
on); Keel ships Qt 6 builds of them, one RPM each, listed with what is
partial in `keel/silica/COMPATIBILITY.md` ("Platform modules"). Add a
`Requires:` for each module your QML imports (for example
`shipwright-keel-platform-pickers`). On the desktop, `keel run` uses them
from your Keel build. Qt 5-only QML that Qt 6 dropped is covered by
`keel/qt5compat` (see the [porting guide](porting-guide.md), step 2).

### Keel Actions

Declare what your app can do next to the code that does it, and every MCP
client on the phone can call it: Pilot, or a paired desktop agent
(ADR-0018). In QML, `import Keel.Actions 1.0`:

```qml
KeelAction {
    name: "weather.forecast"
    title: qsTr("Forecast")
    description: "Get the forecast for a place for the next days. Returns one entry per day."
    readOnly: true
    openWorld: true                        // it fetches from the network
    parameters: [
        KeelParam { name: "place"; type: "string"; required: true; maxLength: 100 },
        KeelParam { name: "days"; type: "integer"; minimum: 1; maximum: 7; defaultValue: 3 }
    ]
    returns: KeelResult {
        type: "array"; itemType: "object"; maxItems: 7
        KeelParam { name: "date"; type: "string"; format: "date"; maxLength: 10; required: true }
        KeelParam { name: "summary"; type: "string"; maxLength: 200; required: true }
    }
    onInvoked: (args, call) => forecast.fetch(args.place, args.days || 3, days => call.reply(days))
}
```

The handler answers through `call` (`call.reply(value)` or
`call.fail(code, message)`), at once or later. Arguments arrive validated
against the declared schema; your result is checked against it too.
Set these flags where they apply: `destructive` for anything that deletes or
overwrites, `openWorld` for network access, `confirm` to always ask first,
`sensitive` for results that must never reach a model. `KeelEntity`
(objects your app owns), `KeelContext` (what the current page shows) and
`KeelShortcut` (suggested sequences) work the same way; Rust functions use
`#[keel::action]` from the `keel` crate. `keel/README.md`, "Keel Actions",
has the full reference.

Test actions like any other QML: the `KeelActions` singleton calls them the
way D-Bus does.

```qml
const call = KeelActions.invoke("weather.forecast", { place: "Tampere" })
tryCompare(call, "finished", true)
verify(call.succeeded, call.errorMessage)
compare(KeelActions.invoke("weather.forecast", {}).errorCode, "InvalidArguments")
```

Run the QML test with `KEEL_ACTIONS_APP_ID=org.example.weather-now` (your
`OrganizationName.ApplicationName`).

The build step turns the declarations into what the phone needs:

```
keel actions --tests tests            # target/keel-actions/*, test stubs in tests/
keel actions --check                  # in CI: fails when the output is stale
```

It writes `actions.json` (the MCP tool definitions; package it as
`/usr/share/keel/actions/<app-id>.json`), the D-Bus activation file, and
an `ExecDBus=` line to add to your desktop file's `[X-Sailjail]` section.
Errors point at the file and line (for example a property that is not a
literal, or an action that says "delete" but is not marked `destructive`).
The test stubs check each action's schema and call it once; fill in what
the result should be.

## 4. Test

```
cargo test
KEEL_BUILD_DIR=~/.cache/keel-dev/keel-build tests/smoke-test.sh
```

`cargo test` runs the Rust unit tests (`src/greeter.rs`). The smoke test
starts the app offscreen against Keel and passes when the main page and the
cover load without QML warnings. `.github/workflows/ci.yml` runs both, plus
`cargo fmt` and `clippy`, on GitHub Actions; set `SHIPWRIGHT_GIT` in it
first.

## 5. Package

Replace the placeholder icon `icons/weather-now.svg` with yours, then
render the launcher sizes and check the spec:

```
icons/render-icons.sh
rpmspec -P rpm/weather-now.spec
```

The device RPM is built in the Sailfish platform SDK container that
Shipwright's `tools/build/sdk/sdk.sh` sets up, after Keel's own RPMs
(`shipwright-keel-sailfishapp` and `-devel` are needed in the target):
`SHIPWRIGHT_DIR=<checkout> rpm/build-in-sdk.sh` cross-builds the binary with
`tools/build/sdk/cxxqt-sailfish.sh` and runs `mb2 ... -s
rpm/weather-now.spec build`; the RPMs land in `RPMS/5.2.0.15/`.
**[VERIFY]** This SDK step has not been run yet (no Docker where the guide
was written); the spec was checked with `rpmspec -P` and a host
`rpmbuild -bb`.

On a phone, the app starts from its launcher icon in its own booster,
inside its Sailjail sandbox: the desktop file's
`Exec=/usr/bin/invoker --type=keel -A -- /usr/bin/weather-now` reaches
booster-keel@weather-now, which the package starts with the session
(`user-session.target.d/50-weather-now.conf`). It draws straight into
Lipstick (Keel's direct mode, ADR-0016). It needs `shipwright-keel-silica`,
`-sailfishapp`, `-booster` and Chum's Qt 6 (with `qt6-qtwayland`);
`-shell` is the fallback (`Exec=/usr/bin/keel-shell -- /usr/bin/weather-now`
with `X-Nemo-Application-Type=no-invoker`). **[VERIFY]** A generated app has
not run on a phone yet; on a host it runs in direct mode against Keel's
Lipstick stand-in, also through the booster (`keel/README.md`, "Runtime on
the phone").

## 6. Publish

Set the Sailjail `Permissions=` line in `packaging/weather-now.desktop` to
what the app uses, then sign and upload as in the
[porting guide](porting-guide.md), sections 4 and 5. Uploads are reviewed
asynchronously: the upload answers `202`, and you poll it for the review
result.
