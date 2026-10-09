#!/bin/bash
# SPDX-FileCopyrightText: 2026 Patrick Quinn
# SPDX-License-Identifier: MIT
#
# Installs built device RPMs into a fresh Sailfish OS root, as a phone would
# get them, and checks what a phone would first trip over:
#
#   tools/build/native/install-test.sh [--release 5.2.0.15] [rpm...]
#
# With no RPMs, takes RPMS/<release>/*.rpm (not debuginfo/debugsource; not
# the Reef installer when the Reef client is there, since the client
# replaces it).
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
# droidmedia, which the camera's gstreamer1.0-droid needs).
set -euo pipefail
release=5.2.0.15
if [ "${1:-}" = "--release" ]; then
    release=$2
    shift 2
fi
here=$(cd "$(dirname "$0")" && pwd)
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
    find "$root/RPMS/$release" -maxdepth 1 -name '*.rpm' \
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
native_image "$release" build "localhost/shipwright-sailfish:$release" || true
{
    printf 'Name: shipwright-installtest-phone-adaptation\nVersion: 1\nRelease: 1\n'
    printf 'Summary: install-test stand-in for the phone hardware adaptation\nLicense: MIT\nBuildArch: noarch\n'
    for p in $provides; do printf 'Provides: %s\n' "$p"; done
    printf '%%description\nStand-in.\n%%files\n'
} > "$work/phone-adaptation.spec"
podman run --rm -v "$work:/work:Z" "localhost/shipwright-sailfish:$release" \
    rpmbuild -bb --quiet --define "_topdir /tmp/rpmbuild" --define "_rpmdir /work" \
    --define "_build_name_fmt %%{NAME}.rpm" /work/phone-adaptation.spec
rm "$work/phone-adaptation.spec"

count=$(find "$work" -name '*.rpm' ! -name 'shipwright-installtest-*' | wc -l)
[ "$count" -gt 0 ] || { echo "install-test: no RPMs (build with native.sh first)" >&2; exit 2; }
echo "== installing $count RPM(s) into a fresh Sailfish OS $release root"

# The RPMs are unsigned until Reef publishes them.
podman run --rm -v "$work:/rpms:ro,Z" "$image" bash -c '
    set -u
    status=0
    zypper --non-interactive in --no-recommends --allow-unsigned-rpm /rpms/*.rpm >/tmp/install.log 2>&1 || status=$?
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
            if echo "$out" | grep -q "not found"; then
                echo "$out" | grep "not found" | sed "s|^|FAIL $pkg: $f:|"
                missing=1
            fi
        done
    done
    [ "$missing" -eq 0 ] && echo "ok   every ELF file resolves its libraries"
    exit "$missing"
'
