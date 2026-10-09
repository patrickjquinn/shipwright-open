<!--
SPDX-FileCopyrightText: 2026 Patrick Quinn
SPDX-License-Identifier: MIT
-->

# Native device builds (aarch64 hosts)

Builds the device RPMs on an aarch64 Linux machine, such as an Apple silicon Mac running Asahi Fedora, with no emulation:

```
tools/build/native/native.sh                     # every spec, Sailfish OS 5.2.0.15
tools/build/native/native.sh shoal/push/rpm/shipwright-shoal-push.spec
tools/build/native/native.sh --release 5.1.0.11  # Xperia line
```

RPMs land in `RPMS/<release>/`, as with [`tools/build/sdk/sdk.sh`](../sdk/README.md).

## Why this works

Jolla's Sailfish SDK has two parts:

- **Build engine and scratchbox (`sb2`).** These are x86-only. Their job is to run the phone's software on a PC by emulating it.
- **Build target.** An ordinary aarch64 Sailfish OS root (glibc, rpm, Qt) that the SDK runs under emulation.

On an aarch64 host the target runs as it is, so the build engine is not needed. The phone's libraries still are: Fedora's glibc 2.43 and Qt 6.11 would make binaries the phone (glibc 2.41, Chum's Qt 6.8.4) cannot run, so everything is built inside the Sailfish root. The root runs on Asahi's 16 KiB-page kernel too.

## How it works

- **Images.** `native.sh` imports Jolla's public `Sailfish_OS-5.1.0.11-Sailfish_SDK_Target-aarch64.tar.7z` (pinned by SHA-256, cached in `~/.cache/shipwright/sailfish-targets`) as `localhost/shipwright-sailfish-target:5.1.0.11`. [`Containerfile`](Containerfile) builds `localhost/shipwright-sailfish:<release>` from it:
  - it upgrades the root to the release with `ssu release` and `zypper dup`, through Jolla's repositories, which carry every release, including 5.2.0.15 that has no public SDK target;
  - it adds Chum's `chum:testing` (Qt 6.8);
  - it installs gcc, rpm-build, CMake, Ninja and Qt 6's development packages.
- **Container.** One per release (`shipwright-native-<release>`), kept between runs. Each run copies in a clean snapshot of the working tree (tracked and new files, as `sdk.sh`) and keeps only `target/`, so cargo builds are incremental.
- **Build.** It runs [`tools/build/sdk/build-rpms.sh`](../sdk/build-rpms.sh) with `SAILFISH_NATIVE=1`:
  - `cargo-sailfish.sh` builds natively with the root's gcc and an aarch64 rustup toolchain (pinned by `install-rustup.sh`);
  - `cxxqt-sailfish.sh` uses the root's own `qmake6`, moc and qmlcachegen;
  - [`mb2`](mb2) stands in for the SDK's `mb2`: it installs a spec's `BuildRequires` with zypper, then runs `rpmbuild --build-in-place` from the tree, so specs, hooks and `specs.conf` are the same as for the SDK.
- **Output.** The RPMs are copied back to `RPMS/<release>/`.

Updating: `NATIVE_REBUILD_IMAGE=1` rebuilds the image (new release updates, Chum updates) and replaces the container. Other variables are as for `sdk.sh` (`REEF_KEY_FILE`, `REEF_URL_TEMPLATE`, `REEF_LICENCE_SERVER`, `SHIPWRIGHT_LICENCE_KEYS`, `PILOT_RELAY_URL`, `SKIP_SPECS`, `WITH_VENDORID`, `WITH_PHASE0`, `CARGO_PACKAGES`, `SOURCE_DATE_EPOCH`).

## Licence keys

The Reef client (`reef-backend`, which also serves the licence hand-off), Shoal Keys and Shoal Mail trust only the licence public keys compiled into them, and a build without keys rejects every licence token, pasted or delivered by Reef. `SHIPWRIGHT_LICENCE_KEYS` names them, as `kid:base64url` entries separated by commas (the output of `shipwright-licence-service public-key`). `native.sh` and `sdk.sh` pass it into the container with `podman exec -e` (`docker exec -e`), and `build-rpms.sh` and the pre-build hooks keep it in the environment of the cargo builds they run. The specs run no cargo, so rpmbuild's environment does not matter. The crates' build scripts (`reef_licence::build`) fail the build on a malformed value and, in a release build (`--cfg release_build`), on a key id starting with `staging` or `test`. For a phone that talks to the staging licence service:

```
SHIPWRIGHT_LICENCE_KEYS="$(services/pilot-relay/target/debug/examples/mint public-key staging-k1 "$(cat /path/to/your/licence-seed)")" \
REEF_KEY_FILE=<Reef repository public key> CARGO_PACKAGES=reef-backend \
tools/build/native/native.sh reef/rpm/shipwright-reef.spec \
    shoal/keys/rpm/shipwright-shoal-keys.spec shoal/mail/rpm/shipwright-shoal-mail.spec
```

(`cargo build --manifest-path services/pilot-relay/Cargo.toml --example mint` first. The public key is not secret, the seed in `licence.key` is.) Changing the variable rebuilds the three crates, even in the container's kept `target/`.

## Requirements

- **podman, rootless.** Sailfish has system users at UID 500000 and above (Android app support), so the user's subordinate ID range in `/etc/subuid` and `/etc/subgid` must cover at least 1,048,576 IDs, then run `podman system migrate`. For example: `user:524288:1048576`. The default 65,536 is not enough.
- **Tools and disk.** `7z` and `curl`. About 3.5 GB for the images, plus the container's `target/`.
