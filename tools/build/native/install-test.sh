#!/bin/bash
# SPDX-FileCopyrightText: 2026 Patrick Quinn
# SPDX-License-Identifier: MIT
#
# Installs built device RPMs into a fresh Sailfish OS root, as a phone would
# get them, and checks what a phone would first trip over:
#
#   tools/build/native/install-test.sh [--release RELEASE] [rpm...]
#
# RELEASE defaults to the newest in tools/build/releases/supported.txt.
#
# With no RPMs, takes the release's build: RPMS/<build root>/*.rpm, the
# build root being the oldest supported release of its MAJOR.MINOR line (not
# debuginfo/debugsource; not the Reef installer when the Reef client is
# there, since the client replaces it).
# The root is the Containerfile's `runtime` stage: the release from Jolla's
# repositories plus Chum's chum:testing, no toolchain and no -devel
# packages. A throwaway container then:
#   1. installs every RPM with zypper (dependencies resolved from Jolla's
#      and Chum's repositories; scriptlets run);
#   2. runs ldd on every ELF file the packages installed and reports
#      libraries that do not resolve (a missing Requires);
#   3. lists scriptlet warnings.
# Exits 1 when the install fails or a library is missing.
#
# What it cannot show: anything needing the phone's hardware, Lipstick or a
# user session. The SDK target root is not a full phone image either; the
# UI packages a phone has come from the same repositories when required.
# Packages from the phone's hardware adaptation repository (not public) are
# stood in for by a generated package providing PHONE_PROVIDES (default:
# droidmedia, which the camera's gstreamer1.0-droid needs). Libraries only
# that repository ships (PHONE_LIBS, default: libhybris's and the Android
# HAL's, which Pacific's engine links against and leaves out of its
# Requires) are listed as notes when they do not resolve, not failures.
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
release_valid "$release" || { echo "install-test.sh: --release must be a four-part release" >&2; exit 2; }
root=$(cd "$here/../../.." && pwd)
# shellcheck source=SCRIPTDIR/lib.sh
. "$here/lib.sh"
image=localhost/shipwright-sailfish-runtime:$release
native_image "$release" runtime "$image" || true

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
if [ $# -gt 0 ]; then
    cp "$@" "$work/"
else
    find "$(release_rpms_dir "$root" "$release")" -maxdepth 1 -name '*.rpm' \
        ! -name '*-debuginfo-*' ! -name '*-debugsource-*' -exec cp {} "$work/" \;
fi
# The Reef client obsoletes and conflicts with the installer, which a phone
# installs first, alone (tools/build/reef/README.md, "LAN test repository",
# rehearses that path); with both here, test the client.
if ls "$work"/shipwright-reef-0*.rpm >/dev/null 2>&1; then
    rm -f "$work"/shipwright-reef-installer-*.rpm
fi
# The stand-in for the hardware adaptation's packages, built with the build
# image's rpmbuild.
provides=${PHONE_PROVIDES:-droidmedia}
phone_libs=${PHONE_LIBS:-libhybris-common.so.1 libhybris-platformcommon.so.1 libhybris-eglplatformcommon.so.1 libgralloc.so.1 libhardware.so.2 libui.so.1 libsync.so.2}
# Any build image's rpmbuild will do: the build root's, which the build made.
build_root=$(release_build_root "$release")
native_image "$build_root" build "localhost/shipwright-sailfish:$build_root" || true
{
    printf 'Name: shipwright-installtest-phone-adaptation\nVersion: 1\nRelease: 1\n'
    printf 'Summary: install-test stand-in for the phone hardware adaptation\nLicense: MIT\nBuildArch: noarch\n'
    for p in $provides; do printf 'Provides: %s\n' "$p"; done
    printf '%%description\nStand-in.\n%%files\n'
} > "$work/phone-adaptation.spec"
podman run --rm -v "$work:/work:Z" "localhost/shipwright-sailfish:$build_root" \
    rpmbuild -bb --quiet --define "_topdir /tmp/rpmbuild" --define "_rpmdir /work" \
    --define "_build_name_fmt %%{NAME}.rpm" /work/phone-adaptation.spec
rm "$work/phone-adaptation.spec"

[ -n "$(find "$work" -name '*.rpm' ! -name 'shipwright-installtest-*')" ] ||
    { echo "install-test: no RPMs (build with native.sh first)" >&2; exit 2; }

# Pacific's engine comes in CPU variants that conflict with each other (a
# phone installs one; 5.1.0.11 carries armv80 for the Xperia 10 II): the set
# is tested with shipwright-pacific-engine, then again with each variant in
# its place (and without the engine's -devel, which needs the default).
variants=$work.variants
trap 'rm -rf "$work" "$variants"' EXIT
mkdir -p "$variants"
find "$work" -maxdepth 1 -name 'shipwright-pacific-engine-armv*.rpm' -exec mv -t "$variants/" {} +

# install_into <dir> [what]: installs every RPM in <dir> into a fresh root
# and checks every ELF file they installed. The RPMs are unsigned until Reef
# publishes them (or carry a published signature: see below).
install_into() {
count=$(find "$1" -name '*.rpm' ! -name 'shipwright-installtest-*' | wc -l)
echo "== installing $count RPM(s) into a fresh Sailfish OS $release root${2:+ ($2)}"
podman run --rm -e PHONE_LIBS="$phone_libs" -v "$1:/rpms:ro,Z" "$image" bash -c '
    set -u
    status=0
    # Unsigned builds, or published ones whose key this root lacks (5.1.0.11
    # republishes its signed engine): signatures are publish-repo.sh'"'"'s check.
    zypper --non-interactive --no-gpg-checks in --no-recommends --allow-unsigned-rpm /rpms/*.rpm >/tmp/install.log 2>&1 || status=$?
    tail -3 /tmp/install.log
    # 107: installed, but a scriptlet failed (listed below).
    if [ "$status" -ne 0 ] && [ "$status" -ne 107 ]; then
        cat /tmp/install.log
        echo "FAIL zypper exited $status"
        exit 1
    fi
    echo "ok   installed"
    grep -E "scriptlet failed|warning: %" /tmp/install.log | sed "s/^/note /" || true

    missing=0
    for pkg in $(rpm -qp --qf "%{NAME}\n" /rpms/*.rpm | grep -v "^shipwright-installtest-" | sort -u); do
        for f in $(rpm -ql "$pkg" 2>/dev/null); do
            [ -f "$f" ] && [ ! -L "$f" ] || continue
            head -c4 "$f" 2>/dev/null | grep -q "ELF" || continue
            out=$(ldd "$f" 2>&1) || true
            for lib in $(echo "$out" | awk "/not found/ { print \$1 }"); do
                case " $PHONE_LIBS " in
                    *" $lib "*) echo "note $pkg: $f: $lib is the phone hardware adaptation'"'"'s" ;;
                    *) echo "FAIL $pkg: $f: $lib => not found"; missing=1 ;;
                esac
            done
        done
    done
    [ "$missing" -eq 0 ] && echo "ok   every ELF file resolves its libraries"
    exit "$missing"
'
}

status=0
install_into "$work" || status=1
for v in "$variants"/*.rpm; do
    [ -e "$v" ] || continue
    alt=$(mktemp -d)
    find "$work" -maxdepth 1 -name '*.rpm' ! -name 'shipwright-pacific-engine-[0-9]*' \
        ! -name 'shipwright-pacific-engine-devel-*' -exec cp -t "$alt/" {} +
    cp "$v" "$alt/"
    install_into "$alt" "engine variant $(basename "$v" .rpm)" || status=1
    rm -rf "$alt"
done
exit "$status"
