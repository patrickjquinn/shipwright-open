#!/bin/bash
# SPDX-FileCopyrightText: 2026 Patrick Quinn
# SPDX-License-Identifier: MIT
#
# Cross-compiles workspace crates for a Sailfish OS target from inside the
# Sailfish platform SDK container (coderus/sailfishos-platform-sdk-aarch64).
#
# The SDK's own Rust is 1.75 (below our MSRV), so this uses an upstream rustup
# toolchain and links with the SDK's cross gcc against the target's sysroot,
# so binaries bind to the target's real glibc.
#
# The SDK userland is 32-bit x86 (i486), so rustup must use the i686 host.
#
# Usage: cargo-sailfish.sh [--target-name SailfishOS-5.2.0.15-aarch64] <cargo args...>
#   e.g. cargo-sailfish.sh build --release --locked -p shoal-avrcp-volume
set -euo pipefail

target_name=${SAILFISH_TARGET:-SailfishOS-5.2.0.15-aarch64}
if [ "${1:-}" = "--target-name" ]; then
    target_name=$2
    shift 2
fi

case "$target_name" in
    *-aarch64) triple=aarch64-unknown-linux-gnu; cross=aarch64-meego-linux-gnu ;;
    *-armv7hl) triple=armv7-unknown-linux-gnueabihf; cross=armv7hl-meego-linux-gnueabi ;;
    *) echo "unsupported target $target_name" >&2; exit 2 ;;
esac

# SAILFISH_NATIVE=1: running inside a native Sailfish OS root of the target's
# architecture (tools/build/native), where the target's own gcc and libraries
# are the system ones: no sysroot, no cross tools.
native=${SAILFISH_NATIVE:-0}
sysroot=/srv/mer/targets/$target_name
[ "$native" = 1 ] || [ -d "$sysroot" ] || { echo "no such SDK target: $sysroot" >&2; exit 2; }

# Install rustup (i686 host) on first use; rust-toolchain.toml picks the version.
if ! command -v rustup >/dev/null 2>&1; then
    "$(dirname "$0")/install-rustup.sh"
    # shellcheck disable=SC1091
    source "$HOME/.cargo/env"
fi
if [ "$native" = 1 ]; then
    rustup set default-host "$triple" >/dev/null
else
    rustup set default-host i686-unknown-linux-gnu >/dev/null
fi
rustup target add "$triple" >/dev/null

if [ "$native" = 1 ]; then
    root=$(cd "$(dirname "$0")/../../.." && pwd)
    cargo_home=${CARGO_HOME:-$HOME/.cargo}
    export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-$HOME/target}
    env_triple=$(echo "$triple" | tr 'a-z-' 'A-Z_')
    # -fuse-ld=bfd: CXX-Qt's build scripts (qt-build-utils) otherwise pick
    # ld.gold for Qt executables, which rustc warns is deprecated. The
    # root's GNU ld 2.46 links them (checked with the Phase 0 hello-world).
    export "CARGO_TARGET_${env_triple}_RUSTFLAGS=--remap-path-prefix=$root=/shipwright --remap-path-prefix=$cargo_home=/cargo --remap-path-prefix=$CARGO_TARGET_DIR=/target -C link-arg=-fuse-ld=bfd"
    env_cc=$(echo "$triple" | tr '-' '_')
    export "CFLAGS_${env_cc}=-ffile-prefix-map=$root=/shipwright -ffile-prefix-map=$cargo_home=/cargo -ffile-prefix-map=$CARGO_TARGET_DIR=/target"
    export "CXXFLAGS_${env_cc}=-ffile-prefix-map=$root=/shipwright -ffile-prefix-map=$cargo_home=/cargo -ffile-prefix-map=$CARGO_TARGET_DIR=/target"
    # --target keeps outputs in target/<triple>/release, where specs install from.
    exec cargo "$@" --target "$triple"
fi

# The cross gcc looks for plain `ld` and `as` and would find the host's; give
# it a directory of the cross ones and pass it with -B. ld.gold too: CXX-Qt's
# build scripts (qt-build-utils) ask for -fuse-ld=gold when ld is GNU bfd.
bindir=$HOME/.cache/shipwright/cross-$cross
mkdir -p "$bindir"
for tool in ld ld.bfd ld.gold as; do
    ln -sf "/opt/cross/bin/$cross-$tool" "$bindir/$tool"
done

export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-$HOME/target}

# Reproducible builds (README.md, "Reproducible builds"): panic locations and
# any remaining debug info carry source paths. Map the checkout, cargo's
# registry and the target directory (build-script output) to fixed names, so
# the binary does not depend on where the tree was copied to. rustc applies
# the last matching mapping, so the target directory (usually inside the
# checkout) comes last.
root=$(cd "$(dirname "$0")/../../.." && pwd)
cargo_home=${CARGO_HOME:-$HOME/.cargo}
remap="--remap-path-prefix=$root=/shipwright --remap-path-prefix=$cargo_home=/cargo"
remap+=" --remap-path-prefix=$CARGO_TARGET_DIR=/target"
cremap="-ffile-prefix-map=$root=/shipwright -ffile-prefix-map=$cargo_home=/cargo"
cremap+=" -ffile-prefix-map=$CARGO_TARGET_DIR=/target"

env_triple=$(echo "$triple" | tr 'a-z-' 'A-Z_')
export "CARGO_TARGET_${env_triple}_LINKER=/opt/cross/bin/$cross-gcc"
export "CARGO_TARGET_${env_triple}_RUSTFLAGS=-C link-arg=--sysroot=$sysroot -C link-arg=-B$bindir $remap"
# Crates with C code (ring, via cc-rs) need the same cross compiler and sysroot.
env_cc=$(echo "$triple" | tr '-' '_')
export "CC_${env_cc}=/opt/cross/bin/$cross-gcc"
export "CXX_${env_cc}=/opt/cross/bin/$cross-g++"
export "AR_${env_cc}=/opt/cross/bin/$cross-ar"
export "CFLAGS_${env_cc}=--sysroot=$sysroot -B$bindir $cremap"
export "CXXFLAGS_${env_cc}=--sysroot=$sysroot -B$bindir $cremap"

exec cargo "$@" --target "$triple"
