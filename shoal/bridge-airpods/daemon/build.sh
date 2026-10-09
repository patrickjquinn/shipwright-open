#!/bin/sh
# SPDX-FileCopyrightText: 2026 Patrick Quinn
# SPDX-License-Identifier: GPL-3.0-only
#
# Builds the patched daemon on a development host, offline, the same way the
# RPM does: copy upstream/ (the MagicPodsCore submodule) into a build tree,
# apply patches/*.patch in order, configure with the vendored archives and
# MAGICPODS_OFFLINE=ON, build.
#
#   ./build.sh [BUILD_DIR]            default BUILD_DIR: ./_build
#   WITH_OPENSSL=OFF ./build.sh       build with the OpenSSL stub (patch 0003)
#   NO_NETWORK=1 ./build.sh           run configure and build inside `unshare -n`
#                                     (needs root or user namespaces), proving
#                                     that nothing is downloaded
#   BUILD_TYPE=Debug ./build.sh       a DEBUG build, which runs the self-tests
#                                     at start-up (tests/run-self-tests.sh)
#
# The result is BUILD_DIR/build/magicpodscore.

set -eu

here=$(cd "$(dirname "$0")" && pwd)
out=${1:-$here/_build}
with_openssl=${WITH_OPENSSL:-ON}
build_type=${BUILD_TYPE:-Release}
jobs=${JOBS:-$(nproc 2>/dev/null || echo 2)}

[ -f "$here/upstream/CMakeLists.txt" ] || {
    echo "error: $here/upstream is empty; run: git submodule update --init $here/upstream" >&2
    exit 1
}

"$here/vendor/fetch.sh" --check >/dev/null

rm -rf "$out/src"
mkdir -p "$out/src"
# Copy the tracked tree only, so a dirty or previously built upstream/ does
# not leak into the build.
if git -C "$here/upstream" rev-parse --git-dir >/dev/null 2>&1; then
    git -C "$here/upstream" archive --format=tar HEAD | tar -x -C "$out/src"
else
    (cd "$here/upstream" && tar -c --exclude=.git --exclude=build .) | tar -x -C "$out/src"
fi

for p in "$here"/patches/*.patch; do
    echo "applying $(basename "$p")"
    # --fuzz=0, as the RPM's %autopatch: a patch that needs fuzz fails here too.
    patch -d "$out/src" -p1 --quiet --no-backup-if-mismatch --forward --fuzz=0 < "$p"
done

run() {
    if [ "${NO_NETWORK:-0}" = 1 ]; then
        unshare -n "$@"
    else
        "$@"
    fi
}

run cmake -S "$out/src" -B "$out/build" \
    -DCMAKE_BUILD_TYPE="$build_type" \
    -DMAGICPODS_DEPENDENCY_DIR="$here/vendor" \
    -DMAGICPODS_OFFLINE=ON \
    -DFETCHCONTENT_UPDATES_DISCONNECTED=ON \
    -DMAGICPODS_WITH_OPENSSL="$with_openssl"
run cmake --build "$out/build" -j "$jobs"

echo "built $out/build/magicpodscore"
