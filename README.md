<!--
SPDX-FileCopyrightText: 2026 Patrick Quinn
SPDX-License-Identifier: MIT
-->

# Shipwright open source

This repository holds the open-source parts of Shipwright's Sailfish OS software: the Keel toolkit, the Reef installer and store client, Shoal Keys, Shoal Messages, the AirPods daemon, the licence and secrets libraries, and the developer documentation. It is the corresponding source for the open-source apps distributed on [reefstore.app](https://reefstore.app).

It is generated. Development happens in a private monorepo that also holds the paid apps and the hosted services; each release, a script copies the open components here, unchanged, as one commit. This tree is the export of monorepo commit `cbcefa8f225f` (`Source-Commit: cbcefa8f225f38c330c7c6287c202b7dde23b8fb` in the commit message). The history here is one commit per export, not the private history.

Shipwright is the trading name of Patrick Quinn, who holds the copyright in everything not marked otherwise.

## What is here

| Path | What | Licence |
| --- | --- | --- |
| `keel/` | Keel: Qt 6 implementation of the Sailfish Silica API, keel-shell, the Nemo compatibility libraries, Keel Actions | MIT; ported Silica QML and keel-shell's upstream parts BSD-3-Clause; Nemo compatibility libraries LGPL-2.1-only |
| `tools/keel-launcher/` | The launcher code every Keel app compiles in | MIT |
| `reef/installer/`, `reef/client/`, `reef/rpm/` | Reef: the installer that adds the signed repository, and the store app | GPL-3.0-or-later |
| `reef/licence/`, `shoal/secrets/` | Licence check and secrets libraries | MIT OR Apache-2.0 |
| `shoal/keys/` | Shoal Keys, the password manager | GPL-3.0-or-later |
| `shoal/messages/` | Shoal Messages, a fork of harbour-xmatic | Apache-2.0 |
| `shoal/bridge-airpods/daemon/`, `shoal/bridge-airpods/rpm/` | Patches to MagicPodsCore (a submodule) and the daemon's build | GPL-3.0-only |
| `tools/build/` | The SDK and native build scripts for the RPMs | MIT |
| `docs/developers/` | Documentation for developers publishing on Reef | CC-BY-4.0 |

[LICENSING.md](LICENSING.md) explains the split; every file states its licence in an SPDX header or in [REUSE.toml](REUSE.toml), and `reuse lint` passes. The names and logos are trademarks ([TRADEMARKS.md](TRADEMARKS.md)): a modified version you distribute must use its own name and icon. The app icons and the paid apps are not part of this repository.

## Building

Clone with the submodule (the AirPods daemon's upstream):

    git clone --recurse-submodules <this repository>

Host builds and tests of the Rust crates in the root workspace (Reef's backend, the licence and secrets libraries, Shoal Keys' core, Keel Actions) need the Rust toolchain pinned in `rust-toolchain.toml`:

    cargo test --workspace

The Qt apps (`reef/client/app`, `shoal/keys/app`) are separate Cargo workspaces built with CXX-Qt against Qt 6; Keel and Shoal Messages build with CMake and qmake. Each directory's README says how.

Device RPMs build with the Sailfish SDK (`tools/build/sdk/README.md`) or natively on an aarch64 Linux host (`tools/build/native/README.md`). The specs are under each component's `rpm/` directory. The AirPods daemon's tarball build, for distributors, is `shoal/bridge-airpods/rpm/make-sources.sh` followed by `rpmbuild --without intree`. Some READMEs mention parts of the monorepo that are not published here; nothing published here needs them to build.

## Contributing

Issues and pull requests are welcome here. Nobody pushes to this repository directly, not even the maintainer: an accepted pull request is applied to the private monorepo with its authorship kept, and it comes back here in the next export, after which the pull request is closed with a link to that export. Please keep pull requests to the published paths, and expect a short delay between merge and appearance.

By contributing you agree that your contribution is licensed under the licence of the files it changes.

Security issues: please write to the address on [reefstore.app](https://reefstore.app) rather than opening a public issue.
