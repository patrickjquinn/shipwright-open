#!/bin/bash
# SPDX-FileCopyrightText: 2026 Patrick Quinn
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Pre-build hook for tools/build/sdk/build-rpms.sh (tools/build/sdk/README.md,
# "Per-spec hooks and specs.conf"): run from the repository root with the
# spec as $1. Copies the signing key in (as for the installer) and cross-builds
# the client, reef/client/app (CXX-Qt, its own Cargo workspace), into
# target/<triple>/release/shipwright-reef. shipwright-reef-licenced comes from
# the root workspace, which build-rpms.sh builds first.
set -euo pipefail
reef/installer/rpm/prebuild.sh "$1"
tools/build/sdk/cxxqt-sailfish.sh --target-name "${SAILFISH_TARGET:-SailfishOS-5.2.0.15-aarch64}" \
    reef/client/app "${SAILFISH_RUST_OUT:-target/aarch64-unknown-linux-gnu/release}"
