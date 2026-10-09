#!/bin/bash
# SPDX-FileCopyrightText: 2026 Patrick Quinn
# SPDX-License-Identifier: MIT
#
# Host-side driver: starts the Sailfish SDK container, prepares the target and
# builds all RPMs from a clean copy of the working tree.
#
#   tools/build/sdk/sdk.sh [--release 5.2.0.15] [spec...]
#
# SDK_CONTAINER reuses an existing SDK container instead of creating one.
# REEF_KEY_FILE (the Reef repository's public key) enables the Reef specs;
# REEF_URL_TEMPLATE points a test installer at another repository;
# REEF_LICENCE_SERVER (an https URL) replaces the Reef client's built-in
# licence server (reef/client/app/README.md).
# PILOT_RELAY_URL (an https URL) replaces pilotd's and Pilot Lite's built-in
# Pilot relay (shoal/pilot/README.md; staging phone builds only).
# SHIPWRIGHT_LICENCE_KEYS (kid:base64url[,...]) is the licence public keys
# compiled into Reef, Shoal Keys and Shoal Mail (tools/build/native/README.md).
# SKIP_SPECS, MB2_SNAPSHOT, WITH_VENDORID, WITH_PHASE0, MIN_FREE_GB,
# BUILD_WORKSPACE and CARGO_PACKAGES are passed to build-rpms.sh, and so is
# SOURCE_DATE_EPOCH (default: the last commit's time; the copy in the
# container has no git history).
#
# The image is pinned by digest per release (below). RPMs are copied back
# to ./RPMS/<release>/. Needs Docker. Docker Hub
# rate-limits anonymous pulls; set SDK_IMAGE_PREFIX=mirror.gcr.io/ to use the
# Google mirror.
set -euo pipefail

release=5.2.0.15
if [ "${1:-}" = "--release" ]; then
    release=$2
    shift 2
fi
# Images are pinned by digest: a re-pushed tag must not change what we build
# with. Add a line here when a release is added (the digest is the
# Docker-Content-Digest of the tag's manifest); SDK_IMAGE_DIGEST overrides.
case "$release" in
    5.2.0.15) digest=sha256:e5f7596d14502746b308c3dde36f65fc25d7f76c45a01acaed56a1f55b76f592 ;;
    *) digest=${SDK_IMAGE_DIGEST:?"sdk.sh: no pinned image digest for $release; set SDK_IMAGE_DIGEST"} ;;
esac
digest=${SDK_IMAGE_DIGEST:-$digest}
image=${SDK_IMAGE_PREFIX:-}coderus/sailfishos-platform-sdk-aarch64:$release@$digest
target=SailfishOS-$release-aarch64
name=${SDK_CONTAINER:-shipwright-sdk-$release}
root=$(cd "$(dirname "$0")/../../.." && pwd)

# A submodule left uninitialised builds as an empty directory and fails late
# (the AirPods daemon's upstream).
if (cd "$root" && git submodule status | grep -q '^-'); then
    echo "$(basename "$0"): uninitialised submodule(s); run: git submodule update --init" >&2
    (cd "$root" && git submodule status | grep '^-') >&2
    exit 2
fi

docker image inspect "$image" >/dev/null 2>&1 || docker pull "$image"
if ! docker container inspect "$name" >/dev/null 2>&1; then
    docker run -d --name "$name" "$image" sleep infinity >/dev/null
fi
docker start "$name" >/dev/null

# Trust an intercepting proxy's CA if one is configured (CI or sandboxes).
if [ -n "${SDK_EXTRA_CA:-}" ]; then
    docker cp "$SDK_EXTRA_CA" "$name:/tmp/extra-ca.crt"
    docker exec -u root "$name" chmod 0644 /tmp/extra-ca.crt
    docker exec -u root "$name" bash -c "
        for d in /etc/pki/ca-trust/source/anchors /srv/mer/targets/$target/etc/pki/ca-trust/source/anchors; do
            mkdir -p \$d && cp /tmp/extra-ca.crt \$d/; done
        update-ca-trust extract || true
        zypper --non-interactive in libatomic >/dev/null 2>&1 || true"
    docker exec "$name" bash -lc "sb2 -t $target -m sdk-install -R update-ca-trust extract"
else
    docker exec -u root "$name" bash -c 'zypper --non-interactive in libatomic >/dev/null 2>&1 || true'
fi

docker exec "$name" bash -lc "$(cat <<SETUP
set -e
sb2 -t $target -m sdk-install -R zypper --non-interactive lr chum-testing >/dev/null 2>&1 ||
    sb2 -t $target -m sdk-install -R zypper --non-interactive ar -G -f \
        https://repo.sailfishos.org/obs/sailfishos:/chum:/testing/${release%.*.*}_aarch64/ chum-testing
# The release's own repository, for -devel packages the target image lacks
# (mapplauncherd-devel, systemd-devel, libaccounts-glib-devel).
sb2 -t $target -m sdk-install -R zypper --non-interactive lr sailfishos-release >/dev/null 2>&1 ||
    sb2 -t $target -m sdk-install -R zypper --non-interactive ar -G -f \
        https://releases.jolla.com/releases/${release}/jolla/aarch64/ sailfishos-release
sb2 -t $target -m sdk-install -R zypper --non-interactive --gpg-auto-import-keys ref >/dev/null
# Chum's Qt 6 tools in the target itself: cxxqt-sailfish.sh runs its qmake6,
# moc, qmltyperegistrar and qmlcachegen through sb2 for the CXX-Qt crates.
sb2 -t $target -m sdk-install -R zypper --non-interactive in --no-recommends \
    qt6-qtbase-devel qt6-qtbase-private-devel qt6-qtdeclarative-devel >/dev/null
SETUP
)"

# Clean copy of tracked and new (non-ignored) files.
tarball=$(mktemp)
# Working-tree files plus the contents of submodules (ls-files -o cannot recurse).
(cd "$root" && { git ls-files -co --exclude-standard -z; git ls-files --recurse-submodules -z; } |
    sort -zu | while IFS= read -r -d '' f; do { [ -f "$f" ] || [ -L "$f" ]; } && printf '%s\0' "$f"; done |
    tar --null -T - -cf "$tarball")
# mktemp creates 0600 files; the SDK user (mersdk) must be able to read it.
chmod 0644 "$tarball"
docker cp "$tarball" "$name:/tmp/shipwright-src.tar"
rm -f "$tarball"
docker exec "$name" bash -lc 'rm -rf ~/build && mkdir ~/build && tar -xf /tmp/shipwright-src.tar -C ~/build'

if [ -n "${REEF_KEY_FILE:-}" ]; then
    docker cp "$REEF_KEY_FILE" "$name:/tmp/reef-key.asc"
    docker exec -u root "$name" chmod 0644 /tmp/reef-key.asc
    reef_key_env=(-e REEF_KEY_FILE=/tmp/reef-key.asc)
else
    reef_key_env=()
fi

source_date_epoch=${SOURCE_DATE_EPOCH:-$(git -C "$root" log -1 --format=%ct)}

status=0
docker exec -e SKIP_SPECS="${SKIP_SPECS:-}" -e MB2_SNAPSHOT="${MB2_SNAPSHOT:-shipwright}" \
    -e WITH_VENDORID="${WITH_VENDORID:-0}" -e WITH_PHASE0="${WITH_PHASE0:-0}" \
    -e MIN_FREE_GB="${MIN_FREE_GB:-4}" -e BUILD_WORKSPACE="${BUILD_WORKSPACE:-1}" \
    -e CARGO_PACKAGES="${CARGO_PACKAGES:-}" -e SOURCE_DATE_EPOCH="$source_date_epoch" \
    -e REEF_URL_TEMPLATE="${REEF_URL_TEMPLATE:-}" -e REEF_LICENCE_SERVER="${REEF_LICENCE_SERVER:-}" \
    -e SHIPWRIGHT_LICENCE_KEYS="${SHIPWRIGHT_LICENCE_KEYS:-}" -e PILOT_RELAY_URL="${PILOT_RELAY_URL:-}" \
    "${reef_key_env[@]}" "$name" bash -lc \
    "cd ~/build && tools/build/sdk/build-rpms.sh --target $target $*" || status=$?

mkdir -p "$root/RPMS/$release"
docker cp "$name:/home/mersdk/build/RPMS/." "$root/RPMS/$release/"
ls -1 "$root/RPMS/$release"
exit $status
