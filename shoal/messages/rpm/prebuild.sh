#!/bin/bash
# SPDX-FileCopyrightText: 2026 Patrick Quinn
# SPDX-License-Identifier: Apache-2.0
#
# Pre-build hook for tools/build/sdk/build-rpms.sh (tools/build/sdk/README.md,
# "Per-spec hooks and specs.conf"): run from the repository root with the
# spec as $1. Builds Shoal Messages' Rust core with upstream's own cross-build,
# scripts/build-core.sh, which the qmake project then links; specs.conf runs
# mb2 from shoal/messages.
#
# build-core.sh expects the installer-style SDK layout (targets/ and
# toolings/ under SAILFISH_SDK_ROOT); in the platform SDK container that is
# /srv/mer. It also needs cbindgen on PATH, and the toolchain pinned in
# shoal/messages/rust-toolchain.toml (installed here through rustup).
set -euo pipefail

target=${SAILFISH_TARGET:-SailfishOS-5.2.0.15-aarch64}
release=${target#SailfishOS-}
export SAILFISH_SDK_ROOT=${SAILFISH_SDK_ROOT:-/srv/mer}
export SFOS_RELEASE=${release%-*}
export SFOS_ARCH=${release##*-}
sdk_tools=$(cd "$(dirname "$0")/../../../tools/build/sdk" && pwd)
cd shoal/messages

# rustup (pinned, i686 host) when build-rpms.sh skipped the workspace build
# (BUILD_WORKSPACE=0, as in CI's messages job).
if ! command -v rustup >/dev/null 2>&1; then
    "$sdk_tools/install-rustup.sh"
    # shellcheck disable=SC1091
    source "$HOME/.cargo/env"
fi
# The SDK container's userland is i686; a native root (tools/build/native)
# is the target's own architecture.
if [ "${SAILFISH_NATIVE:-0}" = 1 ]; then
    rustup set default-host "$(uname -m)-unknown-linux-gnu" >/dev/null
else
    rustup set default-host i686-unknown-linux-gnu >/dev/null
fi
rustup toolchain install "$(sed -n 's/^channel = "\(.*\)"/\1/p' rust-toolchain.toml)" \
    --profile minimal --target aarch64-unknown-linux-gnu >/dev/null

# cbindgen: a host tool, built once into ~/.cache (outside the build tree).
cbindgen_version=${CBINDGEN_VERSION:-0.29.2}
cbindgen_root=$HOME/.cache/shipwright/cbindgen-$cbindgen_version
if [ ! -x "$cbindgen_root/bin/cbindgen" ]; then
    CARGO_TARGET_DIR=$PWD/core/target/cbindgen-build \
        cargo install --locked --root "$cbindgen_root" --version "$cbindgen_version" cbindgen
    rm -rf core/target/cbindgen-build
fi
export PATH=$cbindgen_root/bin:$PATH

# The core's own target directory (the .pro links core/target/...).
unset CARGO_TARGET_DIR
CARGO_INCREMENTAL=0 scripts/build-core.sh
