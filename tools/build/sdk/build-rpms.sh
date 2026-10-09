#!/bin/bash
# SPDX-FileCopyrightText: 2026 Patrick Quinn
# SPDX-License-Identifier: MIT
#
# Builds all Shipwright RPMs for one Sailfish target, inside the SDK container.
# Run this INSIDE the SDK container, from a copy of the repository (not a
# shared checkout: it writes target/ and RPMS/ in place).
#
#   build-rpms.sh [--target SailfishOS-5.2.0.15-aarch64] [spec...]
#
# With no specs, builds every */rpm/*.spec except those listed in SKIP_SPECS.
# Root-workspace Rust binaries are cross-built first with cargo-sailfish.sh
# into target/<triple>/release/, where the specs install them from. Per-spec
# needs (a pre-build hook, extra mb2 arguments, a different working
# directory, opt-in) are declared in specs.conf and <component>/rpm/prebuild.sh;
# see README.md, "Per-spec hooks and specs.conf". Nothing spec-specific
# lives in this script.
#
# Environment: SKIP_SPECS (space-separated spec paths), BUILD_WORKSPACE=0
# (skip the root-workspace build, for spec lists that need none),
# CARGO_PACKAGES (space-separated: build only these root-workspace packages
# instead of every binary), MB2_SNAPSHOT (default shipwright),
# SOURCE_DATE_EPOCH (default: the last commit's time, if git can tell; with
# it, build times and file mtimes in the RPMs are fixed, see README.md,
# "Reproducible builds"), MIN_FREE_GB (prune cargo intermediates before a
# hook when less is free; default 4), plus whatever specs.conf names (needs=,
# optin=).
set -euo pipefail

target=${SAILFISH_TARGET:-SailfishOS-5.2.0.15-aarch64}
if [ "${1:-}" = "--target" ]; then
    target=$2
    shift 2
fi
case "$target" in
    *-aarch64) triple=aarch64-unknown-linux-gnu ;;
    *-armv7hl) triple=armv7-unknown-linux-gnueabihf ;;
    *) echo "unsupported target $target" >&2; exit 2 ;;
esac

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
cd "$root"

# What hooks get (README.md): the target, the shared cargo directory and the
# directory specs install Rust binaries from.
export SAILFISH_TARGET=$target
export CARGO_TARGET_DIR=$root/target
export SAILFISH_RUST_OUT=$root/target/$triple/release

# Free space where the build happens, in GB.
free_gb() { df -Pk "$root" | awk 'NR == 2 { print int($4 / 1048576) }'; }

# Delete cargo's intermediate files, keeping the staged outputs (binaries in
# target/<triple>/release/, anything a hook staged elsewhere under target/).
# Each build starts from a fresh copy (sdk.sh), so this only costs a rebuild
# of shared dependencies (cxx-qt-lib) by later hooks.
prune_cargo() {
    echo "== $(free_gb) GB free: pruning cargo intermediates"
    # Cargo marks its target directories with CACHEDIR.TAG (and each
    # target/<triple> inside them, which is not a root).
    find . -name CACHEDIR.TAG -printf '%h\n' | while IFS= read -r t; do
        if [ -f "$(dirname "$t")/CACHEDIR.TAG" ]; then continue; fi
        rm -rf "$t/release" "$t/debug"
        for d in "$t"/*/release "$t"/*/debug; do
            if [ -d "$d" ]; then
                rm -rf "$d/build" "$d/deps" "$d/.fingerprint" "$d/incremental" "$d/examples"
            fi
        done
    done
    echo "== now $(free_gb) GB free"
}

# Reproducible builds: one timestamp for the whole build (README.md).
if [ -z "${SOURCE_DATE_EPOCH:-}" ] && git -C "$root" rev-parse --git-dir >/dev/null 2>&1; then
    SOURCE_DATE_EPOCH=$(git -C "$root" log -1 --format=%ct)
fi
if [ -n "${SOURCE_DATE_EPOCH:-}" ]; then
    export SOURCE_DATE_EPOCH
    echo "== SOURCE_DATE_EPOCH=$SOURCE_DATE_EPOCH"
fi
# Given to rpmbuild for every spec. Without SOURCE_DATE_EPOCH the last two
# do nothing; the build host name is fixed either way.
rpm_defines=(--define "_buildhost shipwright-sdk"
    --define "use_source_date_epoch_as_buildtime 1"
    --define "clamp_mtime_to_source_date_epoch 1")

if [ "${BUILD_WORKSPACE:-1}" = 1 ]; then
    echo "== Rust binaries for $target"
    if [ -n "${CARGO_PACKAGES:-}" ]; then
        pkgs=()
        for p in $CARGO_PACKAGES; do pkgs+=(-p "$p"); done
        "$here/cargo-sailfish.sh" --target-name "$target" \
            build --release --locked --bins "${pkgs[@]}"
    else
        "$here/cargo-sailfish.sh" --target-name "$target" \
            build --release --locked --workspace --bins
    fi
fi

# --- specs.conf --------------------------------------------------------------
declare -A conf_dir=() conf_optin=() conf_needs=() conf_order=() conf_args=()
while IFS= read -r line || [ -n "$line" ]; do
    line=${line%%#*}
    [ -n "${line//[[:space:]]/}" ] || continue
    opts=$line rest=
    case "$line" in *" -- "*) opts=${line%% -- *} rest=${line#* -- } ;; esac
    read -r -a fields <<<"$opts"
    spec=${fields[0]#./}
    conf_order[$spec]=50
    for f in "${fields[@]:1}"; do
        case "$f" in
            dir=*) conf_dir[$spec]=${f#dir=} ;;
            optin=*) conf_optin[$spec]=${f#optin=} ;;
            needs=*) conf_needs[$spec]=${f#needs=} ;;
            order=*) conf_order[$spec]=${f#order=} ;;
            *) echo "specs.conf: unknown field '$f' for $spec" >&2; exit 2 ;;
        esac
    done
    conf_args[$spec]=$rest
done < "$here/specs.conf"

if [ $# -gt 0 ]; then
    specs=("${@#./}")
else
    # tools/keel-dev/templates holds the `keel new` app template, whose spec
    # is full of {{placeholders}}: not a package.
    mapfile -t specs < <(find . \( -path ./target -o -path ./RPMS -o -path '*/.mb2' -o -path ./tools/keel-dev/templates \) -prune -o \
        -path '*/rpm/*.spec' -print | sed 's|^\./||' |
        while IFS= read -r s; do printf '%s\t%s\n' "${conf_order[$s]:-50}" "$s"; done |
        sort -t $'\t' -k1,1n -k2,2 | cut -f2)
fi
skip=" $(for s in ${SKIP_SPECS:-}; do printf '%s ' "${s#./}"; done)"

mkdir -p RPMS
failed=() built=() skipped=()
for spec in "${specs[@]}"; do
    case "$skip" in *" $spec "*) echo "== skip $spec (SKIP_SPECS)"; skipped+=("$spec"); continue ;; esac
    optin=${conf_optin[$spec]:-}
    if [ -n "$optin" ] && [ "${!optin:-0}" != 1 ]; then
        echo "== skip $spec (opt-in: set $optin=1)"; skipped+=("$spec"); continue
    fi
    needs=${conf_needs[$spec]:-}
    if [ -n "$needs" ] && [ -z "${!needs:-}" ]; then
        echo "== skip $spec (needs $needs)"; skipped+=("$spec"); continue
    fi
    echo "== $spec"

    rpmdir=$(dirname "$spec")
    if [ -x "$rpmdir/prebuild.sh" ]; then
        [ "$(free_gb)" -ge "${MIN_FREE_GB:-4}" ] || prune_cargo
        echo "== prebuild: $rpmdir/prebuild.sh $spec"
        if ! "$rpmdir/prebuild.sh" "$spec"; then
            failed+=("$spec (prebuild)")
            continue
        fi
    fi

    args_str=${conf_args[$spec]:-}
    args_str=${args_str//@ROOT@/$root}
    args_str=${args_str//@RUST_OUT@/$SAILFISH_RUST_OUT}
    args=()
    eval "args=($args_str)"
    dir=${conf_dir[$spec]:-.}
    spec_in_dir=${spec#"$dir"/}
    [ "$dir" = . ] && spec_in_dir=$spec
    # A private snapshot: builds sharing the default one reset it under each other.
    if (cd "$dir" && mb2 -t "$target" --snapshot="${MB2_SNAPSHOT:-shipwright}" -s "$spec_in_dir" \
            build -- "${rpm_defines[@]}" ${args[@]+"${args[@]}"}); then
        built+=("$spec")
    else
        failed+=("$spec")
    fi
    # mb2 leaves RPMs in <dir>/RPMS.
    if [ "$dir" != . ] && [ -d "$dir/RPMS" ]; then
        mv -f "$dir"/RPMS/*.rpm RPMS/ 2>/dev/null || true
    fi
done

echo
echo "built:   ${#built[@]} spec(s)"
echo "skipped: ${skipped[*]:-none}"
ls -1 RPMS/
if [ ${#failed[@]} -gt 0 ]; then
    printf 'FAILED: %s\n' "${failed[@]}" >&2
    exit 1
fi
