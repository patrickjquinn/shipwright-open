# Sailfish SDK builds

Builds device RPMs in the Sailfish platform SDK Docker image (`coderus/sailfishos-platform-sdk-aarch64:<release>`), as the plan requires.

On an aarch64 host, [`tools/build/native`](../native/README.md) runs the same scripts natively in a Sailfish OS root instead (no SDK image, no emulation); `SAILFISH_NATIVE=1` switches `cargo-sailfish.sh`, `cxxqt-sailfish.sh` and the hooks to it.

```
tools/build/sdk/sdk.sh                      # all specs for 5.2.0.15
tools/build/sdk/sdk.sh --release 5.1.0.11   # Xperia line
tools/build/sdk/sdk.sh shoal/push/rpm/shipwright-shoal-push.spec
```

RPMs land in `RPMS/<release>/`.

## How it works

- `sdk.sh` (host) starts the container, adds Chum's `chum:testing` repo to the target (the only Chum repo with Qt6 on 5.2), copies a clean snapshot of the working tree in, and runs `build-rpms.sh`.
- `sdk.sh` also installs Chum's Qt 6 development packages (`qt6-qtbase-devel`, `qt6-qtbase-private-devel`, `qt6-qtdeclarative-devel`) into the target itself, for `cxxqt-sailfish.sh`.
- `build-rpms.sh` (in the container) cross-builds every root-workspace Rust binary with `cargo-sailfish.sh`, then, for each `*/rpm/*.spec`, runs the spec's pre-build hook (if any) and `mb2 -t <target> --snapshot=$MB2_SNAPSHOT -s <spec> build`, as `specs.conf` says (next section).
- `cargo-sailfish.sh` uses an upstream rustup toolchain (from `rust-toolchain.toml`), because the SDK's own Rust is 1.75. It links with the SDK's cross gcc against the target sysroot, so binaries bind to the target's real glibc.
- `cxxqt-sailfish.sh <crate-dir> <out-dir> [cargo args]` cross-builds one CXX-Qt crate (a Qt 6 app, which is a shared library that is also executable, or a static library plus its CXX-Qt export tree) and stages its outputs in `<out-dir>`. CXX-Qt asks `qmake -query` and runs moc, rcc, qmltyperegistrar and qmlcachegen; the target's Qt 6 tools are aarch64 binaries, so the script generates a `qmake` stand-in answering with the target `qmake6`'s values (queried once through `sb2`, paths mapped into the sysroot) and wrappers that run the tools through `sb2` (qemu). Under `sb2` the tools can write outputs with mode 000 (QSaveFile cannot stat its descriptor); the wrappers make them readable again. Crates built with the same `CARGO_TARGET_DIR` reuse cxx-qt-lib's C++ build, which dominates the time.

## Per-spec hooks and specs.conf

`build-rpms.sh` has no knowledge of individual specs. A spec that needs more than `mb2 -s <spec> build` from the repository root says so in two places:

- **`<component>/rpm/prebuild.sh`**, if present and executable, runs before `mb2` for every spec in that `rpm/` directory, from the repository root, with the spec path as `$1` (a hook serving several specs checks it). It gets `SAILFISH_TARGET`, `CARGO_TARGET_DIR` (`<repo>/target`) and `SAILFISH_RUST_OUT` (`target/<triple>/release`, where specs install Rust binaries from), plus the caller's environment (`REEF_KEY_FILE`, `REEF_URL_TEMPLATE`, `REEF_LICENCE_SERVER`: the Reef client's built-in licence server, see reef/client/app/README.md; `SHIPWRIGHT_LICENCE_KEYS`: the licence keys the Reef client, Shoal Keys and Shoal Mail are built to trust, see tools/build/native/README.md, "Licence keys"; `PILOT_RELAY_URL`: pilotd's and Pilot Lite's built-in Pilot relay, see shoal/pilot/README.md). A failing hook fails that spec only. Hooks are where Rust that is not in the root workspace is built: the CXX-Qt apps (through `cxxqt-sailfish.sh`), Keel's native plugin and Shoal Messages' core.
- **`tools/build/sdk/specs.conf`**, one line per spec: `<spec> [dir=<d>] [optin=<VAR>] [needs=<VAR>] [order=<n>] [-- <args>]`. `dir=` runs `mb2` from another directory (spec path relative to it; its `RPMS/` is collected); `optin=` builds only with `<VAR>=1`; `needs=` skips the spec, saying so, unless `<VAR>` is set; `order=` sets the build order (default 50); everything after `--` goes after `mb2 ... build --` with shell quoting, `@ROOT@` and `@RUST_OUT@` substituted.

| Spec | Hook | specs.conf |
| --- | --- | --- |
| `keel/rpm/shipwright-keel-silica.spec` | `keel/rpm/prebuild.sh` → `build-native.sh target/keel-native` | `--define "keel_native_dir @ROOT@/target/keel-native"` |
| `reef/installer/rpm/…-reef-installer.spec` | copies `REEF_KEY_FILE` in; `REEF_URL_TEMPLATE`, if set, replaces `repo.conf`'s | `needs=REEF_KEY_FILE` |
| `reef/rpm/shipwright-reef.spec` | key, then `reef/client/app` | `needs=REEF_KEY_FILE` |
| `shoal/bridge-airpods/rpm/…-vendorid.spec` | | `optin=WITH_VENDORID` |
| `shoal/bridge-icloud/rpm/…-icloud.spec` | `app` and `check` | `--with prebuilt` |
| `shoal/keys/rpm/shipwright-shoal-keys.spec` | `cross-build.sh` | |
| `shoal/mail/rpm/shipwright-shoal-mail.spec` | `shoal/mail/app` | `--define "mail_bin @RUST_OUT@/shipwright-shoal-mail"` |
| `shoal/migrate/rpm/shipwright-shoal-migrate.spec` | `shoal/migrate/app` → `shoal/migrate/prebuilt/` | `--with gui` |
| `shoal/messages/rpm/shipwright-shoal-messages.spec` | `scripts/build-core.sh` (cbindgen and the pinned toolchain installed on first use) | `dir=shoal/messages order=90` |
| `tools/phase0/…/shipwright-phase0-hello-qt6.spec` | | `optin=WITH_PHASE0` |

Disk: before each hook, if less than `MIN_FREE_GB` (default 4) is free, `build-rpms.sh` deletes cargo's intermediate files (every directory marked with `CACHEDIR.TAG`), keeping staged binaries. Every `sdk.sh` run starts from a fresh copy, so this only costs later hooks a rebuild of shared dependencies.

## rpmlint

```
tools/build/sdk/rpmlint-rpms.sh [--release 5.2.0.15] RPMS/5.2.0.15/*.rpm
```

The SDK image has no rpmlint in its own userland, but its tooling has rpmlint 2.0.0 with Jolla's configuration (`/etc/xdg/rpmlint/sailfish.toml`), which `mb2 build` already runs after each build without failing on errors. `rpmlint-rpms.sh` copies the RPMs into the SDK container (`SDK_CONTAINER`, default the one `sdk.sh` leaves running) and runs that rpmlint through `sb2`, adding `rpmlint.toml` with `-c`. The CI `sdk-rpms` jobs run it after uploading their RPMs, so a lint failure still leaves the artefact.

`rpmlint.toml` turns `TreatErrorsAsWarnings` back off (Sailfish sets it, which makes rpmlint exit 0 on errors), so **errors fail the job and warnings do not**. It also adds the SPDX licence identifiers our specs use to `ValidLicenses`, and filters a few false positives. Each filter names its package and message and says why. Sailfish's config already filters `no-documentation`, `no-manual-page-for-binary` and `no-signature`.

Last run, 2026-10-01, on the 19 RPMs of the device build (`docs/release/device-build-2026-10-01.md`):

- Sailfish's config alone: 4 errors, 51 warnings.
  - `shoal-push` and `unifiedpush-permission` had no `%changelog`; the current spec has one (0.1.0-2).
  - `keel-sailfishapp`: `shlib-policy-name-error`.
  - `shoal-migrate-cli`: `explicit-lib-dependency libcommhistory-qt5-tools`, a tools package.
- With `rpmlint.toml`: only the two `%changelog` errors from those stale RPMs remain, with 17 warnings, and the exit status is 64.
- Fresh builds from the current tree: `shipwright-shoal-push` and `shipwright-unifiedpush-permission` 0.1.0-2 give 0 errors and 0 warnings (exit 0), and `shipwright-reef-installer` 0.1.0-2 gives 0 errors.

Warnings left visible on purpose:

- unstripped Keel plugins, `keel-shell` and `magicpodscore`;
- PIE suggested for `keel-shell` and `shoal-icloud-account`;
- `no-url-tag` on two AirPods packages;
- `missing-dependency-on` for `keel-sailfishapp-devel`.

## Reproducible builds

```
tools/build/sdk/check-reproducible.sh [--release 5.2.0.15] [--keep] <spec>
tools/build/sdk/check-reproducible.sh --compare <dir-a> <dir-b>
```

What makes a build repeatable:

- `sdk.sh` passes `SOURCE_DATE_EPOCH` (the last commit's time) into the container; `build-rpms.sh` derives it from git itself when it runs in a checkout.
- `build-rpms.sh` gives every `mb2` build `--define "_buildhost shipwright-sdk"`, `--define "use_source_date_epoch_as_buildtime 1"` and `--define "clamp_mtime_to_source_date_epoch 1"` (rpm 4.16 in the target). The RPM's build time is then the commit time, its build host is fixed, and no file in the payload is newer than the commit.
- `cargo-sailfish.sh` passes `--remap-path-prefix` for the checkout (`/shipwright`), cargo's home (`/cargo`) and the target directory (`/target`), and the matching `-ffile-prefix-map` to C/C++ code built by cc-rs. Panic locations and any debug info then do not depend on where the tree was copied. CXX-Qt crates get the same flags through `cxxqt-sailfish.sh`.

`check-reproducible.sh` builds one spec twice in the SDK container with `build-rpms.sh`. The two builds use two copies of the working tree at different paths (`~/repro/a`, `~/repro/build-two`), one after the other, and the first tree is deleted before the second starts. It then compares the RPMs:

- the whole file;
- the header's file list (`rpm -qp --dump`: size, mtime, digest, mode, owner);
- the unpacked payload, file by file (`rpm2cpio | cpio`, sha256);
- diffoscope on a differing pair, if it is installed (it is not in the SDK image).

Options and limits:

- `CARGO_PACKAGES` builds only the root-workspace packages the spec needs (`build-rpms.sh` honours it everywhere); `BUILD_WORKSPACE=0` builds none.
- `MB2_SNAPSHOT` defaults to `repro`, which is removed afterwards. On 5.2.0.15 a snapshot is a full 1.6 GB copy of the target. When the disk is short, name an existing snapshot such as `shipwright`; one that existed before is never removed. mb2 refuses to build in a target that has snapshots, so there is no snapshot-less mode.

Last run, 2026-10-01, `SOURCE_DATE_EPOCH=1790837512` (commit `1e07cf4`), snapshot `shipwright`:

- `reef/installer/rpm/shipwright-reef-installer.spec` (`BUILD_WORKSPACE=0`, throwaway `REEF_KEY_FILE`): **the two RPMs are byte-identical.** The header shows `BUILDTIME=1790837512` and `BUILDHOST=shipwright-sdk`, and all five payload files have that mtime.
- The comparator was also tested on a pair known to differ: one byte changed in the payload. It reports `DIFFERENT` and exits 1.
- `shoal/push/rpm/shipwright-shoal-push.spec` (`CARGO_PACKAGES=shoal-push-distributor`, a clean cargo build each time, about 10 and 7 minutes): **`shipwright-shoal-push-0.1.0-2.aarch64.rpm` and `shipwright-unifiedpush-permission-0.1.0-2.noarch.rpm` are byte-identical** across the two source paths. The stripped, LTO-built binary has no `/home/mersdk` path left; its 301 source paths read `/cargo/registry/...` and `/shipwright/...`.

## Spec convention for Rust packages

Under `mb2`, `%{_builddir}` is the repository root. Rust specs therefore have an empty `%build`, and install prebuilt binaries:

```
install -D -m 0755 target/%{_arch}-unknown-linux-gnu/release/<bin> %{buildroot}%{_bindir}/<bin>
```

Never call cargo in a spec.

## Facts verified in the 5.2.0.15 image (2026-09-30)

- The only target is `SailfishOS-5.2.0.15-aarch64`. It has gcc 13.4, glibc 2.41, Qt 5.6.3, Rust 1.75, systemd 238 and OpenSSL 3.5.7.
- The SDK userland is 32-bit x86 (i486). x86_64 rustc cannot run in it, so rustup uses the `i686-unknown-linux-gnu` host. rustup-init needs `libatomic`.
- The cross gcc (`/opt/cross/bin/aarch64-meego-linux-gnu-gcc`) does not find `ld` on its own; `cargo-sailfish.sh` passes `-B` to a directory with an `ld` symlink.
- The binaries built this way require at most `GLIBC_2.39` and run under `sb2` (qemu).
- Behind a TLS-intercepting proxy, set `SDK_EXTRA_CA=/path/to/ca.crt`; the CA must be trusted in the container and in the target.
- Chum's OBS repositories publish no `repomd.xml.asc`, so zypper's metadata signature check fails. The target adds `chum:testing` with `-G` (no GPG check), as ssu-managed repos do. Reef should not inherit this: it mirrors the Qt6 packages it needs into its own signed repository.
- Docker Hub rate-limits anonymous pulls. `SDK_IMAGE_PREFIX=mirror.gcr.io/` works.
- (2026-10-01) CXX-Qt's build scripts (qt-build-utils) ask for `-fuse-ld=gold` when `ld` is GNU bfd; `cargo-sailfish.sh` puts `ld.gold` (and `ld.bfd`) next to `ld` in its `-B` directory. Native builds pass `-fuse-ld=bfd` instead, which the root's binutils 2.46 handles, so rustc's gold deprecation warning does not appear.
- (2026-10-01) Under `sb2`, the target's `qmlcachegen` writes its outputs with mode 000. `cxxqt-sailfish.sh`'s wrappers repair them; CMake builds (Keel's Silica spec) retry after a `chmod`.

## Not built by default

- `reef/installer` and `reef` (the client): skipped unless `REEF_KEY_FILE` points at the Reef repository's public signing key (CI: the `REEF_PUBLIC_KEY` secret).
- `shoal/bridge-airpods` vendorid: `WITH_VENDORID=1`.
- `tools/phase0/hello-qt6-rust`: `WITH_PHASE0=1` after a manual build; see `tools/phase0/README.md`.
