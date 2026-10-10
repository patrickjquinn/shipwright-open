#!/bin/bash
# SPDX-FileCopyrightText: 2026 Patrick Quinn
# SPDX-License-Identifier: MIT-0
#
# Builds the {{name}} RPM in the Sailfish platform SDK container that
# Shipwright's tools/build/sdk/sdk.sh sets up (target with Chum's Qt 6
# development packages). Run on the host, from anywhere:
#
#   SHIPWRIGHT_DIR=~/src/Shipwright rpm/build-in-sdk.sh
#
# Steps, in a scratch copy in the container (/home/mersdk/{{name}}-build,
# deleted afterwards):
#   1. install Keel's SailfishApp RPMs (shipwright-keel-sailfishapp and
#      -devel, from $KEEL_RPMS) into the target, for the headers and library;
#   2. cross-build the app with Shipwright's
#      tools/build/sdk/cxxqt-sailfish.sh into
#      target/aarch64-unknown-linux-gnu/release/lib{{crate}}.so;
#   3. mb2 -t <target> --snapshot={{name}} -s rpm/{{name}}.spec build.
# The RPMs are copied to RPMS/<release>/.
#
# Environment:
#   SHIPWRIGHT_DIR   a Shipwright checkout (required)
#   SDK_RELEASE      Sailfish release (default 5.2.0.15)
#   SDK_CONTAINER    SDK container (default shipwright-sdk-<release>, the one
#                    sdk.sh creates)
#   KEEL_RPMS        directory with the Keel RPMs (default
#                    $SHIPWRIGHT_DIR/RPMS/<release>, where sdk.sh puts them)
set -euo pipefail

app=$(cd "$(dirname "$0")/.." && pwd)
shipwright=$(cd "${SHIPWRIGHT_DIR:?set SHIPWRIGHT_DIR to a Shipwright checkout}" && pwd)
release=${SDK_RELEASE:-5.2.0.15}
target=SailfishOS-$release-aarch64
container=${SDK_CONTAINER:-shipwright-sdk-$release}
keel_rpms=${KEEL_RPMS:-$shipwright/RPMS/$release}
work="/home/mersdk/{{name}}-build"

shopt -s nullglob
rpms=("$keel_rpms"/shipwright-keel-sailfishapp-[0-9]*.aarch64.rpm
      "$keel_rpms"/shipwright-keel-sailfishapp-devel-[0-9]*.aarch64.rpm)
shopt -u nullglob
if [ "${#rpms[@]}" -lt 2 ]; then
    echo "build-in-sdk.sh: no shipwright-keel-sailfishapp and -devel RPMs in $keel_rpms" >&2
    echo "  build them: $shipwright/tools/build/sdk/sdk.sh keel/rpm/shipwright-keel-sailfishapp.spec" >&2
    exit 2
fi
docker container inspect "$container" >/dev/null 2>&1 || {
    echo "build-in-sdk.sh: no container $container; run $shipwright/tools/build/sdk/sdk.sh once, or set SDK_CONTAINER" >&2
    exit 2
}

# Scratch copy: the app without build output, and Shipwright's SDK scripts.
tarball=$(mktemp)
trap 'rm -f "$tarball"' EXIT
tar -cf "$tarball" --exclude=./target --exclude=./RPMS -C "$app" . \
    --transform 's,^\./,app/,'
tar -rf "$tarball" -C "$shipwright" tools/build/sdk rust-toolchain.toml \
    --transform 's,^,shipwright/,'
chmod 0644 "$tarball"
docker cp "$tarball" "$container:/tmp/{{name}}-src.tar"
for rpm in "${rpms[@]}"; do
    docker cp "$rpm" "$container:/tmp/$(basename "$rpm")"
done
names=$(for rpm in "${rpms[@]}"; do printf '/tmp/%s ' "$(basename "$rpm")"; done)

status=0
docker exec "$container" bash -lc "$(cat <<SCRIPT
set -euo pipefail
rm -rf $work && mkdir -p $work && tar -xf /tmp/{{name}}-src.tar -C $work
sb2 -t $target -m sdk-install -R zypper --non-interactive in --allow-unsigned-rpm $names
sysroot=/srv/mer/targets/$target
export KEEL_SAILFISHAPP_INCLUDEDIR=\$sysroot/usr/include/keel/sailfishapp
export KEEL_SAILFISHAPP_LIBDIR=\$sysroot/usr/lib64
export CARGO_TARGET_DIR=$work/app/target
$work/shipwright/tools/build/sdk/cxxqt-sailfish.sh --target-name $target \
    $work/app $work/app/target/aarch64-unknown-linux-gnu/release
cd $work/app
mb2 -t $target --snapshot={{name}} -s rpm/{{name}}.spec build
SCRIPT
)" || status=$?

mkdir -p "$app/RPMS/$release"
docker cp "$container:$work/app/RPMS/." "$app/RPMS/$release/" || true
# shellcheck disable=SC2086 # $names is a list of paths without spaces
docker exec "$container" rm -rf "$work" "/tmp/{{name}}-src.tar" $names || true
ls -1 "$app/RPMS/$release"
exit $status
