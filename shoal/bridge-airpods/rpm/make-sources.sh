#!/bin/sh
# SPDX-FileCopyrightText: 2026 Patrick Quinn
# SPDX-License-Identifier: GPL-3.0-only
#
# Collects everything shipwright-shoal-bridge-airpods.spec needs in tarball
# mode (--without intree) into one directory, for rpmbuild -ba / OBS / SRPM:
#
#   shoal/bridge-airpods/rpm/make-sources.sh OUTDIR
#   rpmbuild -ba --without intree --define "_sourcedir OUTDIR" \
#       shoal/bridge-airpods/rpm/shipwright-shoal-bridge-airpods.spec

set -eu
out=${1:?usage: $0 OUTDIR}
here=$(cd "$(dirname "$0")" && pwd)
daemon=$here/../daemon
spec=$here/shipwright-shoal-bridge-airpods.spec
commit=$(sed -n 's/^%global upstream_commit //p' "$spec")

mkdir -p "$out"
"$daemon/vendor/fetch.sh" --check >/dev/null
head=$(git -C "$daemon/upstream" rev-parse HEAD)
[ "$head" = "$commit" ] || {
    echo "error: daemon/upstream is at $head but the spec pins $commit" >&2
    exit 1
}
git -C "$daemon/upstream" archive --format=tar --prefix="MagicPodsCore-$commit/" "$commit" \
    | gzip -n -9 > "$out/MagicPodsCore-$commit.tar.gz"
# The archives only, not their REUSE .license sidecars; nothing else in
# OUTDIR is touched.
for f in "$daemon"/vendor/*.tar.*; do
    case $f in
        *.license) ;;
        *) cp "$f" "$out/" ;;
    esac
done
cp "$daemon"/patches/*.patch "$daemon/shoal-bridge-airpods.service" "$out/"
echo "sources in $out"
