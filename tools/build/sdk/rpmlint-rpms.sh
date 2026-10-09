#!/bin/bash
# SPDX-FileCopyrightText: 2026 Patrick Quinn
# SPDX-License-Identifier: MIT
#
# Runs the Sailfish SDK's rpmlint (2.0.0, in the SDK tooling) on built RPMs,
# with Sailfish's configuration plus tools/build/sdk/rpmlint.toml. Exits
# non-zero if rpmlint reports any error; warnings are printed only.
#
#   tools/build/sdk/rpmlint-rpms.sh [--release 5.2.0.15] RPM...
#
# Host side, like sdk.sh: the RPMs are copied into the SDK container
# (SDK_CONTAINER, default shipwright-sdk-<release>, the one sdk.sh leaves
# running) and checked there through sb2, as `mb2 build` does. CI runs it
# after each sdk-rpms build.
set -euo pipefail

release=5.2.0.15
if [ "${1:-}" = "--release" ]; then
    release=$2
    shift 2
fi
[ $# -gt 0 ] || { echo "rpmlint-rpms: no RPMs given" >&2; exit 2; }
target=SailfishOS-$release-aarch64
name=${SDK_CONTAINER:-shipwright-sdk-$release}
here=$(cd "$(dirname "$0")" && pwd)

# Under sb2 /tmp is the target's, so work in the SDK user's home.
work=/home/mersdk/rpmlint.$$
cleanup() { docker exec -u root "$name" rm -rf "$work" >/dev/null 2>&1 || true; }
trap cleanup EXIT

docker exec "$name" mkdir -p "$work"
docker cp "$here/rpmlint.toml" "$name:$work/rpmlint.toml"
for f in "$@"; do
    docker cp "$f" "$name:$work/"
done
docker exec -u root "$name" chown -R mersdk: "$work"

# rpmlint returns 64 on errors once rpmlint.toml turns TreatErrorsAsWarnings off.
docker exec -w "$work" "$name" bash -lc \
    "sb2 -t $target -m sdk-build+pp rpmlint -c rpmlint.toml *.rpm"
