#!/bin/sh
# SPDX-FileCopyrightText: 2026 Patrick Quinn
# SPDX-License-Identifier: GPL-3.0-only
#
# Re-creates the dependency source tarballs that MagicPodsCore's CMake would
# otherwise fetch at configure time, and verifies them against SHA256SUMS.
# The tarballs are committed next to this script, so builds never run this;
# it exists to audit or refresh them.
#
#   ./fetch.sh          create anything missing, then verify everything
#   ./fetch.sh --check  verify only (no network)
#
# Versions match the FetchContent declarations in
# upstream/dependencies/*/CMakeLists.txt at the pinned upstream commit.
#
# Source archives are produced with `git archive` from a pinned tag, and the
# tag's commit id is checked before archiving. That commit id is the real
# integrity anchor: GitHub's on-the-fly archive downloads are not guaranteed
# byte-stable (and codeload.github.com is not reachable from every build
# host), whereas a git commit id is. nlohmann/json uses the release asset
# json.tar.xz, a static file that upstream recommends for FetchContent.
#
# If a regenerated tarball's sha256 differs from SHA256SUMS while the commit
# id matched, the difference is in tar/gzip tooling, not in the source; keep
# the committed file.

set -eu

here=$(cd "$(dirname "$0")" && pwd)
cd "$here"

# file | git url | tag | expected commit
GIT_LIST='
sdbus-cpp-1.6.0.tar.gz|https://github.com/Kistler-Group/sdbus-cpp|v1.6.0|7450515d0bc632b871d0d3f549ddb24783dd008f
uWebSockets-20.58.0.tar.gz|https://github.com/uNetworking/uWebSockets|v20.58.0|fc2b7be765983fbce8a3839bc3ff16b51c81b44b
uSockets-0.8.7.tar.gz|https://github.com/uNetworking/uSockets|v0.8.7|a15d9bbdea68fd02dab40d2394200deb1b883aa6
tomlplusplus-3.4.0.tar.gz|https://github.com/marzer/tomlplusplus|v3.4.0|30172438cee64926dc41fdd9c11fb3ba5b2ba9de
'
# file | url
URL_LIST='
nlohmann-json-3.11.3.tar.xz|https://github.com/nlohmann/json/releases/download/v3.11.3/json.tar.xz
'

if [ "${1:-}" != "--check" ]; then
    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"' EXIT

    echo "$GIT_LIST" | while IFS='|' read -r file url tag commit; do
        [ -n "$file" ] || continue
        [ -f "$file" ] && continue
        echo "creating $file from $url $tag"
        dir="$tmp/${file%.tar.gz}"
        git -c advice.detachedHead=false clone -q --depth 1 --branch "$tag" "$url" "$dir"
        got=$(git -C "$dir" rev-parse HEAD)
        if [ "$got" != "$commit" ]; then
            echo "error: $url $tag is $got, expected $commit" >&2
            exit 1
        fi
        # Fixed prefix, no submodules, deterministic gzip (no name/timestamp).
        git -C "$dir" archive --format=tar --prefix="${file%.tar.gz}/" HEAD \
            | gzip -n -9 > "$file.part"
        mv "$file.part" "$file"
    done

    echo "$URL_LIST" | while IFS='|' read -r file url; do
        [ -n "$file" ] || continue
        [ -f "$file" ] && continue
        echo "fetching $file"
        curl -fsSL --retry 3 -o "$file.part" "$url"
        mv "$file.part" "$file"
    done
fi

sha256sum -c SHA256SUMS
