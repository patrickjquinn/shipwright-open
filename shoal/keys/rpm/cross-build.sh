#!/bin/bash
# SPDX-FileCopyrightText: 2026 Patrick Quinn
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Cross-compiles shipwright-shoal-keys (shoal/keys/app, CXX-Qt) for a
# Sailfish target inside the Sailfish platform SDK container, and puts the
# app where shipwright-shoal-keys.spec looks for it:
#     target/<triple>/release/libshipwright_shoal_keys.so (the app: a shared
#     library that is also executable, installed as /usr/bin/shipwright-shoal-keys)
# with the shared CXX-Qt cross-build, tools/build/sdk/cxxqt-sailfish.sh.
# tools/build/sdk/build-rpms.sh runs it through shoal/keys/rpm/prebuild.sh.
#
# Usage (in the container, from a copy of the repository):
#   shoal/keys/rpm/cross-build.sh [--target-name SailfishOS-5.2.0.15-aarch64]
# Then:
#   mb2 -t SailfishOS-5.2.0.15-aarch64 --snapshot=keys \
#       -s shoal/keys/rpm/shipwright-shoal-keys.spec build
set -euo pipefail

target_name=${SAILFISH_TARGET:-SailfishOS-5.2.0.15-aarch64}
if [ "${1:-}" = "--target-name" ]; then
    target_name=$2
    shift 2
fi
case "$target_name" in
    *-aarch64) triple=aarch64-unknown-linux-gnu ;;
    *-armv7hl) triple=armv7-unknown-linux-gnueabihf ;;
    *) echo "unsupported target $target_name" >&2; exit 2 ;;
esac
repo=$(cd "$(dirname "$0")/../../.." && pwd)
"$repo/tools/build/sdk/cxxqt-sailfish.sh" --target-name "$target_name" \
    "$repo/shoal/keys/app" "$repo/target/$triple/release"
