# SPDX-FileCopyrightText: 2026 Patrick Quinn
# SPDX-License-Identifier: MIT
#
# shellcheck shell=bash
# Shared by native.sh and install-test.sh: the images, from Jolla's public
# aarch64 SDK target. Source with `here` set to tools/build/native.

# The newest public SDK target image; the Containerfile upgrades it to the
# release. Pinned by SHA-256 (Jolla publishes MD5 sums next to the files).
native_base=5.1.0.11
native_base_sha256=b0425395468254ebaaead29b477d29e446f8dbe7c1ec1c3ac7d80b82757deadc

# native_base_image: imports the base target as
# localhost/shipwright-sailfish-target:<base> unless it exists.
native_base_image() {
    local file=Sailfish_OS-$native_base-Sailfish_SDK_Target-aarch64.tar.7z
    local image=localhost/shipwright-sailfish-target:$native_base
    podman image exists "$image" && return 0
    local cache=${XDG_CACHE_HOME:-$HOME/.cache}/shipwright/sailfish-targets
    mkdir -p "$cache"
    if [ ! -f "$cache/$file" ]; then
        echo "== downloading $file"
        curl -fL --retry 3 -o "$cache/$file.part" "https://releases.sailfishos.org/sdk/targets/$file"
        mv "$cache/$file.part" "$cache/$file"
    fi
    echo "$native_base_sha256  $cache/$file" | sha256sum -c --quiet
    echo "== importing $file as $image"
    7z x -so "$cache/$file" | podman import - "$image" >/dev/null
}

# native_image <release> <stage> <tag>: builds a Containerfile stage
# (runtime or build) as <tag> unless it exists or NATIVE_REBUILD_IMAGE=1.
# Returns 0 when it built, 1 when the image was already there.
native_image() {
    local release=$1 stage=$2 tag=$3
    if [ "${NATIVE_REBUILD_IMAGE:-0}" != 1 ] && podman image exists "$tag"; then
        return 1
    fi
    native_base_image
    echo "== building $tag (Sailfish OS $release, $stage stage)"
    podman build --build-arg BASE="$native_base" --build-arg RELEASE="$release" --target "$stage" \
        -t "$tag" -f "${here:?set by the caller}/Containerfile" "$here"
}
