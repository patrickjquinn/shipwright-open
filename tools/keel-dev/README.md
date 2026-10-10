# keel-dev: the `keel` developer tool

`keel` starts a Sailfish app on Keel and runs it on a desktop. The walk-through
is `docs/developers/getting-started.md`; this file is the reference.

```
cargo build -p keel-dev -p keel-compat     # target/debug/keel, target/debug/keel-compat
```

## `keel new <name>`

Writes a complete app into `--dir` (default `./<name>`), which must not
exist or be empty:

| Generated | What |
| --- | --- |
| `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `build.rs` | CXX-Qt 0.10.0 crate, its own Cargo workspace; links Keel's SailfishApp library |
| `src/greeter.rs`, `src/bridge.rs`, `src/lib.rs` | Plain-Rust logic with unit tests; the `Greeter` QML singleton (module URI = `--id`); `main`. The app is a library, `lib<name>.so` (dashes as underscores), that is also executable, so that booster-keel can load it (`build.rs`); the RPM installs it as `/usr/bin/<name>` |
| `cpp/launcher.{h,cpp}` | Start-up through `SailfishApp::application()`, `createView()`, `pathToMainQml()`, plus the `keel run` contract below |
| `qml/<name>.qml`, `qml/pages/MainPage.qml`, `qml/cover/CoverPage.qml` | ApplicationWindow; Page with PageHeader, SilicaListView, PullDownMenu, ViewPlaceholder; CoverBackground with CoverAction |
| `packaging/<name>.desktop` | `Exec=/usr/bin/keel-shell -- /usr/bin/<name>`, `no-invoker`, `[X-Sailjail]` |
| `icons/<name>.svg`, `icons/render-icons.sh`, `icons/hicolor/*` | Placeholder icon; PNGs rendered at 86/108/128/172 px when `rsvg-convert` is installed |
| `rpm/<name>.spec`, `rpm/build-in-sdk.sh` | Spec in the Shipwright SDK convention (prebuilt binary, `%license`, `desktop-file-validate`, Keel `Requires`); the SDK build driver |
| `tests/smoke-test.sh`, `.github/workflows/ci.yml` | Offscreen smoke test; GitHub Actions workflow |
| `README.md`, `LICENSES/<licence>.txt`, `REUSE.toml`, `.gitignore` | Every file carries `--license` (default MIT) and `--copyright` (default `git config user.name`) headers; `reuse lint` passes |

Options: `--id org.example.Name` (reverse-DNS; QML module URI, Sailjail
`OrganizationName` = all but the last segment, `ApplicationName` = the last;
default `org.example.<Name>`), `--license` (MIT, Apache-2.0, BSD-3-Clause,
GPL-3.0-or-later, GPL-3.0-only, LGPL-2.1-or-later, or `LicenseRef-<name>`
with a stub text), `--title`, `--summary`, `--copyright`, `--no-icons`.
The name must be a Reef app id: lower-case letters, digits and single
hyphens, starting with a letter.

Templates are the files under `templates/app/`, embedded with
`include_str!` (no network). Placeholders are `{{name}}` style (lower case
and underscores only, so GitHub's `${{ expr }}` passes through); an unknown
or left-over placeholder is an error. Templates carry Shipwright's own
licence header, which `keel new` rewrites to the app's.

## `keel run [PATH]`

Builds the app in `PATH` (default `.`) with `cargo build`, runs keel-compat
on it and prints blockers and partial types, then starts it: the binary
named after the package or, for a `keel new` app, the package's `cdylib`,
with `argv[0]` set to the package name:

| Option | Effect |
| --- | --- |
| `--keel-build DIR` / `KEEL_BUILD_DIR` | A Keel CMake build: QML from `DIR/qml`, SailfishApp from `DIR/sailfishapp` |
| `--keel-qml DIR` / `KEEL_QML_DIR` | Keel's QML directory only (an installed Keel; SailfishApp from pkg-config) |
| neither | Builds `keel/` from the Shipwright checkout (`--shipwright`, `SHIPWRIGHT_DIR`, or the one `keel` was built from) into `~/.cache/keel-dev/keel-build`, `-DBUILD_TESTING=OFF` |
| `--device ID`, `--list-devices` | Screen size and pixel ratio of a phone (below) |
| `--size WxH`, `--pixel-ratio R` | Override them |
| `--ambience dark\|light` | `KEEL_THEME_COLOR_SCHEME` |
| `--orientation portrait\|landscape\|portrait-inverted\|landscape-inverted` | `ApplicationWindow.deviceOrientation`; the window takes the turned shape and the page reads upright |
| `--scale F` | `QT_SCALE_FACTOR`; by default tall phones are scaled to fit 1000 px |
| `--watch` | Restart when `qml/` changes; rebuild and restart when `src/`, `cpp/`, `build.rs` or `Cargo.toml` change |
| `--screenshot FILE` | Wait for the first page, check that the cover loads, save a PNG, exit (non-zero on failure) |
| `-- ARGS` | Passed to the app |

Without `DISPLAY`/`WAYLAND_DISPLAY` (and no `QT_QPA_PLATFORM`) the app runs
with `QT_QPA_PLATFORM=offscreen`; offscreen runs use the software scene
graph, as Keel's screenshot tests do. keel-compat is found through
`KEEL_COMPAT`, next to `keel`, on `PATH`, or built from the Shipwright
checkout.

Devices (`keel run --list-devices`):

| Id | Screen | Pixel ratio |
| --- | --- | --- |
| `default`, `jolla-1` | 540x960 | 1 |
| `jolla-c` | 720x1280 | 1.25 |
| `jolla-c2` | 720x1600 | 1.25 |
| `jolla-phone` (2026) | 1080x2260 | 2 |
| `xperia-x` | 1080x1920 | 2 |
| `xperia-10-iii` | 1080x2520 | 2 |

Screens are the vendors' panel resolutions. Ratios are what Keel's Theme
computes under keel-shell when the ambience forwards none (short side / 540
in quarter steps). The phones' own `theme_pixel_ratio` dconf values have not
been read yet (keel/README.md, "Needs device verification", step 4).

### The launcher contract

What `keel run` sets, and `cpp/launcher.cpp` of a generated app reads. An
app with its own start-up still gets Keel's QML and the theme variables; it
must read the `KEEL_RUN_*` variables itself for window size, orientation and
screenshots:

| Variable | Meaning |
| --- | --- |
| `QML_IMPORT_PATH` | Keel's QML directory first |
| `KEEL_SAILFISHAPP_DATADIR` | The project directory, so `pathToMainQml()` loads `qml/<name>.qml` from the source tree |
| `KEEL_THEME_PIXEL_RATIO`, `KEEL_THEME_COLOR_SCHEME` | Keel's own Theme overrides |
| `KEEL_RUN_SIZE=WxH` | Screen size in portrait |
| `KEEL_RUN_ORIENTATION` | Device orientation |
| `KEEL_RUN_SCREENSHOT=FILE` | Screenshot mode: prints `keel-run: first page: <objectName> <type>`, `keel-run: cover ok`, `keel-run: screenshot ...`; exit 0 on success, 3 when no page appears (with the page's compile error), 4 when the PNG cannot be written, 5 when the cover fails |

## Tests

```
cargo test -p keel-dev     # unit tests, and tests/new_app.rs: generates apps, checks the
                           # file set, substitution, headers, Sailjail section, bad input
```

The generated app's Qt build is not part of `cargo test`; it was checked by
hand (getting-started guide).

## Limits

- `keel deploy` (copy to a phone over SSH and launch through keel-shell) and
  an emulator target do not exist yet.
- `keel run` stops the app when it is stopped with Ctrl-C (same process
  group); killed any other way, it leaves the app running.
- `rpm/build-in-sdk.sh` has not been run end to end: Docker was not
  available when it was written. The spec was checked with `rpmspec -P` and a
  host `rpmbuild -bb` with a host binary.

## `keel actions [PATH]`

The Keel Actions build step (ADR-0018; reference in `keel/README.md`,
"Keel Actions"): reads the app's `KeelAction`/`KeelEntity`/`KeelContext`/
`KeelShortcut` declarations and `#[keel::action]` functions and writes
`actions.json`, the D-Bus introspection and activation files, the
`[X-Sailjail]` line and the C adaptor.

| Option | Effect |
| --- | --- |
| `--qml DIR`, `--rust DIR` (repeatable) | What to read [default: `qml/` or `ui/`; `src/`] |
| `--desktop FILE` | App ID (`OrganizationName.ApplicationName`) and executable [default: the first `packaging/*.desktop`, else `*.desktop`] |
| `--app-id ID`, `--exec PATH` | Override them |
| `--out DIR` | Output [default: `target/keel-actions`] |
| `--tests DIR` | Also write test stubs there, when missing |
| `--check` | Write nothing; fail if `--out` is stale or `ExecDBus` is missing |
| `--review FILE` | Apply Reef's review rules to an `actions.json` |
