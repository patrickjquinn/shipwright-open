#!/bin/bash
# SPDX-FileCopyrightText: 2026 Patrick Quinn
# SPDX-License-Identifier: MIT
#
# Builds device RPMs natively on an aarch64 Linux host: the counterpart of
# tools/build/sdk/sdk.sh without the Sailfish SDK's x86 build engine.
#
#   tools/build/native/native.sh [--release RELEASE] [spec...]
#
# RELEASE defaults to the newest in tools/build/releases/supported.txt. The
# build is made in RELEASE's build root, the oldest supported release of its
# MAJOR.MINOR line, and serves every release of the line
# (tools/build/releases/README.md, "One build per line").
#
# Jolla's SDK packaging (the build engine and sb2) is x86-only and emulates
# the phone; its aarch64 *target* image is an ordinary Sailfish OS root that
# runs as it is on aarch64. This script builds a container image from it
# (Containerfile: upgraded to --release with ssu and zypper, Chum's
# chum:testing added, the toolchain installed), keeps one container per
# release, copies a clean snapshot of the working tree in and runs
# tools/build/sdk/build-rpms.sh there with SAILFISH_NATIVE=1, so
# cargo-sailfish.sh and cxxqt-sailfish.sh build natively and
# tools/build/native/mb2 stands in for mb2. RPMs are copied back to
# ./RPMS/<release>/.
#
# Needs: podman (rootless, with a subordinate UID range in /etc/subuid), 7z,
# curl. About 4 GB for the images and container. The base image is
# downloaded once into ~/.cache/shipwright/sailfish-targets.
#
# Environment: NATIVE_CONTAINER (default shipwright-native-<release>);
# REEF_KEY_FILE enables the Reef specs (REEF_URL_TEMPLATE points a test
# installer at another repository, see reef/installer/rpm/prebuild.sh;
# REEF_LICENCE_SERVER, an https URL, replaces the Reef client's built-in
# licence server, see reef/client/app/README.md); PILOT_RELAY_URL, an
# https URL, replaces pilotd's and Pilot Lite's built-in Pilot relay
# (shoal/pilot/README.md; staging phone builds only);
# SHIPWRIGHT_LICENCE_KEYS (kid:base64url[,...]) is the licence public keys
# compiled into Reef, Shoal Keys and Shoal Mail (README.md, "Licence keys");
# PACIFIC_ENGINE_RPMS (a local mirror of Atlantic's engine RPMs, default
# ~/.cache/shipwright/atlantic-mirror/aarch64, copied into the container);
# PACIFIC_CPU (armv8.2, the default, or armv8.0: which build of Pacific's
# engine shipwright-pacific-engine.spec makes, see specs.conf);
# SKIP_SPECS, WITH_VENDORID, WITH_PHASE0, MIN_FREE_GB, BUILD_WORKSPACE, CARGO_PACKAGES and
# SOURCE_DATE_EPOCH (default: the last commit's time) go to build-rpms.sh,
# as with sdk.sh. NATIVE_EXACT_RELEASE=1 builds in RELEASE itself.
# NATIVE_RUST_CACHE (default ~/.cache/shipwright/native-rust): rustup and
# cargo's registry, shared by every build container.
# NATIVE_REBUILD_IMAGE=1 rebuilds the image (a new release's
# updates); the container is replaced with it. NATIVE_MEMORY (default 9g)
# and NATIVE_MEMORY_SWAP (default 11g) cap the container's memory.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
# shellcheck source=SCRIPTDIR/../releases/lib.sh
. "$here/../releases/lib.sh"
if [ "${1:-}" = "--release" ]; then
    release=$2
    shift 2
else
    release=$(releases_newest)
fi
release_valid "$release" || { echo "native.sh: --release must be a four-part release" >&2; exit 2; }
# The point releases of a MAJOR.MINOR line share one build, made in the
# line's oldest supported release (releases/lib.sh, release_build_root):
# --release 5.2.0.22 builds in 5.2.0.15's container into RPMS/5.2.0.15/.
# NATIVE_EXACT_RELEASE=1 builds in the release named instead.
if [ "${NATIVE_EXACT_RELEASE:-0}" != 1 ]; then
    asked=$release
    release=$(release_build_root "$asked")
    [ "$release" = "$asked" ] ||
        echo "native.sh: Sailfish OS $asked shares the $(release_line "$asked") line's build, made in $release (RPMS/$release/)" >&2
fi
[ "$(uname -m)" = aarch64 ] || { echo "native.sh: needs an aarch64 host (use tools/build/sdk/sdk.sh on x86_64)" >&2; exit 2; }

root=$(cd "$here/../../.." && pwd)
target=SailfishOS-$release-aarch64

# A submodule left uninitialised builds as an empty directory and fails late
# (the AirPods daemon's upstream).
if (cd "$root" && git submodule status | grep -q '^-'); then
    echo "$(basename "$0"): uninitialised submodule(s); run: git submodule update --init" >&2
    (cd "$root" && git submodule status | grep '^-') >&2
    exit 2
fi

# shellcheck source=SCRIPTDIR/lib.sh
. "$here/lib.sh"
image=localhost/shipwright-sailfish:$release
name=${NATIVE_CONTAINER:-shipwright-native-$release}
if native_image "$release" build "$image"; then
    podman rm -f "$name" >/dev/null 2>&1 || true
fi
# Rust's toolchains (rustup) and cargo's registry are the same for every
# line: one copy on the host (NATIVE_RUST_CACHE, default
# ~/.cache/shipwright/native-rust), mounted into each build container
# instead of about 2 GB downloaded into each. Builds run one at a time, so
# the containers never write it together.
rust_cache=${NATIVE_RUST_CACHE:-${XDG_CACHE_HOME:-$HOME/.cache}/shipwright/native-rust}
mkdir -p "$rust_cache/rustup" "$rust_cache/cargo"
if ! podman container exists "$name"; then
    podman run -d --name "$name" -v "$rust_cache/rustup:/root/.rustup:z" -v "$rust_cache/cargo:/root/.cargo:z" \
        "$image" sleep infinity >/dev/null
fi
podman start "$name" >/dev/null
# A cap on the build's memory, so a large compile fails on its own instead of
# taking the host down with it (a 16 GB machine ran out of memory on
# 5 October 2026). NATIVE_MEMORY=0 lifts it.
if [ "${NATIVE_MEMORY:-9g}" != 0 ]; then
    podman update --memory "${NATIVE_MEMORY:-9g}" --memory-swap "${NATIVE_MEMORY_SWAP:-11g}" "$name" >/dev/null
fi

# Clean copy of tracked and new (non-ignored) files, as sdk.sh makes; symbolic
# links go in as links (a link to a directory is not a regular file, and
# dropping those left Shoal Bridge's Qt 5 UI without its pages). The
# container keeps target/ between runs (incremental cargo builds) and
# nothing else.
tarball=$(mktemp)
trap 'rm -f "$tarball"' EXIT
(cd "$root" && { git ls-files -co --exclude-standard -z; git ls-files --recurse-submodules -z; } |
    sort -zu | while IFS= read -r -d '' f; do { [ -f "$f" ] || [ -L "$f" ]; } && printf '%s\0' "$f"; done |
    tar --null -T - -cf "$tarball")
podman cp "$tarball" "$name:/tmp/shipwright-src.tar"
podman exec "$name" sh -c 'mkdir -p /root/build && cd /root/build &&
    find . -mindepth 1 -maxdepth 1 ! -name target -exec rm -rf {} + &&
    tar -xf /tmp/shipwright-src.tar && rm /tmp/shipwright-src.tar'

reef_key_env=()
if [ -n "${REEF_KEY_FILE:-}" ]; then
    podman cp "$REEF_KEY_FILE" "$name:/tmp/reef-key.asc"
    reef_key_env=(-e REEF_KEY_FILE=/tmp/reef-key.asc)
fi
source_date_epoch=${SOURCE_DATE_EPOCH:-$(git -C "$root" log -1 --format=%ct)}

# Pacific's engine repackages Atlantic's build 746.1, which its repository
# no longer serves in full (libepoxy-1.5.11-746.1: 404 since 13 October
# 2026): a new container cannot fetch it. A local mirror of those RPMs
# (PACIFIC_ENGINE_RPMS, default ~/.cache/shipwright/atlantic-mirror/aarch64)
# goes into the container for shoal/pacific/engine/fetch-rpms.sh, which
# still checks every file against its SHA256SUMS.
engine_env=()
engine_mirror=${PACIFIC_ENGINE_RPMS:-$HOME/.cache/shipwright/atlantic-mirror/aarch64}
if [ -d "$engine_mirror" ]; then
    podman exec "$name" rm -rf /tmp/atlantic-mirror
    podman cp "$engine_mirror" "$name:/tmp/atlantic-mirror"
    engine_env=(-e PACIFIC_ENGINE_RPMS=/tmp/atlantic-mirror)
fi

status=0
podman exec -e SAILFISH_NATIVE=1 -e SKIP_SPECS="${SKIP_SPECS:-}" \
    -e WITH_VENDORID="${WITH_VENDORID:-0}" -e WITH_PHASE0="${WITH_PHASE0:-0}" \
    -e MIN_FREE_GB="${MIN_FREE_GB:-4}" -e BUILD_WORKSPACE="${BUILD_WORKSPACE:-1}" \
    -e CARGO_PACKAGES="${CARGO_PACKAGES:-}" -e SOURCE_DATE_EPOCH="$source_date_epoch" \
    -e REEF_URL_TEMPLATE="${REEF_URL_TEMPLATE:-}" -e REEF_LICENCE_SERVER="${REEF_LICENCE_SERVER:-}" \
    -e SHIPWRIGHT_LICENCE_KEYS="${SHIPWRIGHT_LICENCE_KEYS:-}" -e PILOT_RELAY_URL="${PILOT_RELAY_URL:-}" \
    -e PACIFIC_CPU="${PACIFIC_CPU:-armv8.2}" \
    "${reef_key_env[@]}" "${engine_env[@]}" "$name" bash -lc \
    'export PATH=/root/build/tools/build/native:/root/.cargo/bin:$PATH; cd /root/build && tools/build/sdk/build-rpms.sh --target "$0" "$@"' \
    "$target" "$@" || status=$?

# This run's RPMs replace everything built from the same source packages
# (older releases, and debuginfo packages a spec no longer makes), so the
# directory holds one build of each package for install-test.sh and Reef.
out="$root/RPMS/$release"
mkdir -p "$out"
new=$(mktemp -d)
trap 'rm -f "$tarball"; rm -rf "$new"' EXIT
podman cp "$name:/root/build/RPMS/." "$new/"
srcname() { rpm -qp --qf '%{SOURCERPM}\n' "$@" 2>/dev/null | sed 's/-[^-]*-[^-]*\.src\.rpm$//'; }
mapfile -t built < <(for f in "$new"/*.rpm; do [ -e "$f" ] && srcname "$f"; done | sort -u)
for old in "$out"/*.rpm; do
    [ -e "$old" ] || continue
    for s in "${built[@]}"; do
        if [ "$(srcname "$old")" = "$s" ]; then rm -f "$old"; break; fi
    done
done
find "$new" -maxdepth 1 -name '*.rpm' -exec mv -t "$out/" {} +
ls -1 "$out"
exit $status
