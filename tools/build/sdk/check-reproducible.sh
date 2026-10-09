#!/bin/bash
# SPDX-FileCopyrightText: 2026 Patrick Quinn
# SPDX-License-Identifier: MIT
#
# Checks that a spec builds reproducibly: builds it twice in the SDK
# container with build-rpms.sh, from two copies of the working tree at
# different paths, with SOURCE_DATE_EPOCH set to the last commit's time, and
# compares the RPMs (README.md, "Reproducible builds").
#
#   tools/build/sdk/check-reproducible.sh [--release 5.2.0.15] [--keep] SPEC
#   tools/build/sdk/check-reproducible.sh --compare DIR_A DIR_B
#
# The comparison, per RPM file name present in both runs:
#   - the file list the header records (rpm -qp --dump: path, size, mtime,
#     digest, mode, owner, group, flags), which must match;
#   - the payload itself, unpacked with rpm2cpio | cpio, compared file by
#     file (sha256, and diffoscope on the RPM pair if it is installed),
#     which must match;
#   - the whole .rpm (reported only: identical payloads with different
#     header signatures or build ids are still reported as a difference).
# Exit status 0 means every payload matched.
#
# Environment:
#   SDK_CONTAINER   the SDK container (default shipwright-sdk-<release>, as
#                   sdk.sh leaves it; its target must already be prepared)
#   MB2_SNAPSHOT    mb2 snapshot for both builds (default repro). A snapshot
#                   this script creates is removed afterwards (unless
#                   --keep); one that already existed is left. A snapshot is
#                   a full copy of the target (about 1.6 GB on 5.2.0.15), so
#                   when the disk is short name an existing one (shipwright,
#                   which build-rpms.sh uses anyway). mb2 refuses to build in
#                   a target that has snapshots, so there is no snapshot-less
#                   mode.
#   CARGO_PACKAGES  root-workspace packages the spec needs (passed to
#                   build-rpms.sh); BUILD_WORKSPACE=0 for a spec with no Rust.
#   REEF_KEY_FILE   for the Reef specs, as for sdk.sh.
#
# Disk: one tree is built at a time; the first run's tree is deleted before
# the second starts, and everything is deleted at the end unless --keep.
set -euo pipefail

compare() {
    local a b status=0 work f name
    a=$(cd "$1" && pwd)
    b=$(cd "$2" && pwd)
    work=$(mktemp -d)
    # shellcheck disable=SC2064 # expand now: $work is local
    trap "rm -rf '$work'" RETURN
    local names_a names_b
    names_a=$(cd "$a" && ls -1 -- *.rpm)
    names_b=$(cd "$b" && ls -1 -- *.rpm)
    if [ "$names_a" != "$names_b" ]; then
        echo "DIFFERENT file sets:"
        diff <(echo "$names_a") <(echo "$names_b") || true
        status=1
    fi
    for f in "$a"/*.rpm; do
        name=$(basename "$f")
        [ -e "$b/$name" ] || continue
        if cmp -s "$f" "$b/$name"; then
            echo "identical  $name (whole file, sha256 $(sha256sum <"$f" | cut -c1-16)...)"
            continue
        fi
        local ok=1
        rpm -qp --nosignature --dump "$f" >"$work/dump-a" 2>&1 || ok=0
        rpm -qp --nosignature --dump "$b/$name" >"$work/dump-b" 2>&1 || ok=0
        if ! diff -u "$work/dump-a" "$work/dump-b" >"$work/dump.diff"; then
            ok=0
        fi
        rm -rf "$work/x-a" "$work/x-b"
        mkdir "$work/x-a" "$work/x-b"
        # A payload that does not unpack is a difference, not a crash.
        (cd "$work/x-a" && rpm2cpio "$f" | cpio -idm --quiet --no-absolute-filenames) \
            >"$work/unpack.log" 2>&1 || ok=0
        (cd "$work/x-b" && rpm2cpio "$b/$name" | cpio -idm --quiet --no-absolute-filenames) \
            >>"$work/unpack.log" 2>&1 || ok=0
        (cd "$work/x-a" && find . -type f -exec sha256sum {} + | sort -k2) >"$work/sum-a"
        (cd "$work/x-b" && find . -type f -exec sha256sum {} + | sort -k2) >"$work/sum-b"
        if ! diff -u "$work/sum-a" "$work/sum-b" >"$work/sum.diff"; then
            ok=0
        fi
        if [ "$ok" = 1 ]; then
            echo "same       $name (payload and file metadata identical; the .rpm differs:" \
                "$(rpm -qp --nosignature --qf '%{BUILDTIME} %{BUILDHOST}' "$f") vs" \
                "$(rpm -qp --nosignature --qf '%{BUILDTIME} %{BUILDHOST}' "$b/$name"))"
        else
            echo "DIFFERENT  $name"
            sed 's/^/    /' "$work/unpack.log" "$work/dump.diff" "$work/sum.diff" | head -40
            if command -v diffoscope >/dev/null 2>&1; then
                diffoscope --text - "$f" "$b/$name" | head -200 || true
            fi
            status=1
        fi
    done
    return $status
}

if [ "${1:-}" = "--compare" ]; then
    [ $# -eq 3 ] || { echo "usage: $0 --compare DIR_A DIR_B" >&2; exit 2; }
    compare "$2" "$3"
    exit
fi

release=5.2.0.15 keep=0
while [ $# -gt 0 ]; do
    case "$1" in
        --release) release=$2; shift 2 ;;
        --keep) keep=1; shift ;;
        -h|--help) sed -n '5,40p' "$0"; exit 0 ;;
        -*) echo "check-reproducible: unknown option $1" >&2; exit 2 ;;
        *) break ;;
    esac
done
[ $# -eq 1 ] || { echo "usage: $0 [--release R] [--keep] SPEC" >&2; exit 2; }
spec=$1
target=SailfishOS-$release-aarch64
name=${SDK_CONTAINER:-shipwright-sdk-$release}
snapshot=${MB2_SNAPSHOT:-repro}
root=$(cd "$(dirname "$0")/../../.." && pwd)
[ -f "$root/$spec" ] || { echo "check-reproducible: no such spec: $spec" >&2; exit 2; }
sde=${SOURCE_DATE_EPOCH:-$(git -C "$root" log -1 --format=%ct)}

work=/home/mersdk/repro
had_snapshot=0
if docker exec "$name" bash -lc "sdk-assistant target list --snapshots" |
        grep -q -x -F -- "$target.$snapshot"; then
    had_snapshot=1
fi
cleanup() {
    if [ "$keep" = 0 ]; then
        docker exec -u root "$name" rm -rf "$work" /tmp/shipwright-repro-src.tar
        if [ "$had_snapshot" = 0 ]; then
            docker exec "$name" bash -lc "sdk-assistant target remove -y $target.$snapshot" \
                >/dev/null 2>&1 || true
        fi
    fi
}
trap cleanup EXIT

# The same clean copy sdk.sh makes: tracked and new (non-ignored) files.
tarball=$(mktemp)
(cd "$root" && { git ls-files -co --exclude-standard -z; git ls-files --recurse-submodules -z; } |
    sort -zu | while IFS= read -r -d '' f; do { [ -f "$f" ] || [ -L "$f" ]; } && printf '%s\0' "$f"; done |
    tar --null -T - -cf "$tarball")
chmod 0644 "$tarball"
docker cp "$tarball" "$name:/tmp/shipwright-repro-src.tar"
rm -f "$tarball"
docker exec "$name" bash -c "rm -rf $work && mkdir -p $work/out"
docker cp "$0" "$name:$work/check-reproducible.sh"
if [ -n "${REEF_KEY_FILE:-}" ]; then
    docker cp "$REEF_KEY_FILE" "$name:/tmp/reef-key.asc"
    docker exec -u root "$name" chmod 0644 /tmp/reef-key.asc
    reef_key_env=(-e REEF_KEY_FILE=/tmp/reef-key.asc)
else
    reef_key_env=()
fi
docker exec -u root "$name" chown -R mersdk: "$work"

# Two different paths (and lengths), so a path leaking into a binary shows.
for run in a build-two; do
    echo "== build $run: $spec (SOURCE_DATE_EPOCH=$sde, snapshot $snapshot)"
    src=$work/$run
    docker exec "$name" bash -c "mkdir -p $src $work/out/$run && tar -xf /tmp/shipwright-repro-src.tar -C $src"
    docker exec -e SOURCE_DATE_EPOCH="$sde" -e MB2_SNAPSHOT="$snapshot" \
        -e CARGO_PACKAGES="${CARGO_PACKAGES:-}" -e BUILD_WORKSPACE="${BUILD_WORKSPACE:-1}" \
        -e MIN_FREE_GB=0 "${reef_key_env[@]}" -w "$src" "$name" bash -lc \
        "tools/build/sdk/build-rpms.sh --target $target $spec"
    docker exec "$name" bash -c "mv $src/RPMS/*.rpm $work/out/$run/ && rm -rf $src"
done

echo "== comparing"
docker exec "$name" bash -c "$work/check-reproducible.sh --compare $work/out/a $work/out/build-two"
