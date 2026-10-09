#!/bin/bash
# SPDX-FileCopyrightText: 2026 Patrick Quinn
# SPDX-License-Identifier: MIT
#
# Installs rustup into ~/.cargo for the build user, with the host's rustup:
# i686 in the SDK container (its userland is 32-bit x86), aarch64 in a native
# Sailfish root (tools/build/native). rustup-init is pinned by version and
# SHA-256 rather than piped from sh.rustup.rs; bump both together (the
# .sha256 file sits next to the binary on static.rust-lang.org). Does
# nothing when ~/.cargo/env already exists. Used by cargo-sailfish.sh and
# the per-spec hooks that need rustup without the workspace build.
set -euo pipefail

rustup_version=1.29.1
# The SDK container's userland is 32-bit x86; a native Sailfish root on an
# aarch64 host (tools/build/native) is aarch64.
case "$(uname -m)" in
    aarch64) host=aarch64-unknown-linux-gnu
             rustup_sha256=15f6e4ce9f583b929c996c91562bad6d4454f3281de858b02cdfdef615fac433 ;;
    *) host=i686-unknown-linux-gnu
       rustup_sha256=5b993042a9f0d3577592f08cb76f8b850951487c2556d90f6e9846abbb083a88 ;;
esac

[ -f "$HOME/.cargo/env" ] && exit 0
# rustup-init picks its mode from its own file name, so keep that name.
dir=$(mktemp -d)
trap 'rm -rf "$dir"' EXIT
init=$dir/rustup-init
curl -sSfo "$init" \
    "https://static.rust-lang.org/rustup/archive/$rustup_version/$host/rustup-init"
echo "$rustup_sha256  $init" | sha256sum -c --quiet
chmod +x "$init"
"$init" -y --profile minimal --default-toolchain none --default-host "$host"
