#!/bin/bash
# SPDX-FileCopyrightText: 2026 Patrick Quinn
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Pre-build hook for tools/build/sdk/build-rpms.sh (tools/build/sdk/README.md,
# "Per-spec hooks and specs.conf"): run from the repository root with the
# spec as $1. Cross-builds the CXX-Qt app into target/<triple>/release/.
set -euo pipefail
shoal/keys/rpm/cross-build.sh --target-name "${SAILFISH_TARGET:-SailfishOS-5.2.0.15-aarch64}"
