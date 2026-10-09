#!/bin/sh
# SPDX-FileCopyrightText: 2026 Patrick Quinn
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Runs the %triggerin (re-pin after an OS update) and %postun (removal)
# scriptlets of shipwright-reef.spec against a stub ssu and a fixture release
# file. The scriptlets are taken from `rpmspec -P`, with the installed paths
# (/usr/bin/shipwright-reef-repo, /usr/share/shipwright-reef/repo.conf,
# /var/log/shipwright-reef.log) redirected into a scratch directory.
#
#   sh reef/rpm/tests/test-scriptlets.sh
#
# Needs rpmspec. The detached key removal in %postun is not run (it would
# call the host's rpm); the test checks it is started with the right uid.

set -eu

here=$(cd "$(dirname "$0")" && pwd)
repo_root=$(cd "$here/../../.." && pwd)
spec="$repo_root/reef/rpm/shipwright-reef.spec"
work=$(mktemp -d "${TMPDIR:-/tmp}/reef-scriptlets.XXXXXX")
trap 'rm -rf "$work"' EXIT INT TERM
fails=0

mkdir -p "$work/bin" "$work/root/etc"
printf 'VERSION_ID=5.2.0.18\n' > "$work/root/etc/sailfish-release"
sed -e "s|^REEF_LOG=.*|REEF_LOG=$work/log|" "$repo_root/reef/installer/repo.conf" > "$work/repo.conf"

cat > "$work/bin/ssu" <<'EOF'
#!/bin/sh
echo "ssu $*" >> "$STATE/calls"
case "$1" in
lr) echo "Enabled repositories (user):"
    [ -f "$STATE/url" ] && echo " - shipwright-reef      ... $(cat "$STATE/url")" ;;
rr) rm -f "$STATE/url" ;;
ar) printf '%s' "$3" > "$STATE/url" ;;
esac
exit 0
EOF
# The detached key removal: record its arguments instead of forking.
cat > "$work/bin/setsid" <<'EOF'
#!/bin/sh
echo "setsid $* " | tr '\n' ' ' >> "$STATE/calls"; echo >> "$STATE/calls"
EOF
chmod +x "$work/bin/ssu" "$work/bin/setsid"

scriptlet() { # section header regex, e.g. '^%triggerin'
    rpmspec -P --target aarch64 "$spec" 2>/dev/null |
        awk -v start="$1" '$0 ~ start {on=1; next} on && /^%(files|postun|triggerin|post|pre|preun|changelog)/ {exit} on {print}' |
        sed -e "s|/usr/bin/shipwright-reef-repo|REEF_ROOT=$work/root $repo_root/reef/installer/bin/shipwright-reef-repo|g" \
            -e "s|/usr/share/shipwright-reef/repo.conf|$work/repo.conf|g" \
            -e "s|/var/log/shipwright-reef.log|$work/log|g"
}
scriptlet '^%triggerin' > "$work/trigger.sh"
scriptlet '^%postun' > "$work/postun.sh"
if [ ! -s "$work/trigger.sh" ] || [ ! -s "$work/postun.sh" ]; then
    echo "could not extract scriptlets" >&2
    exit 1
fi

check() { # name expected actual
    if [ "$2" = "$3" ]; then
        echo "ok   $1"
    else
        printf 'FAIL %s\n  expected: %s\n  actual:   %s\n' "$1" "$2" "$3"
        fails=$((fails + 1))
    fi
}

run() { # script [args...]
    s=$1; shift
    : > "$STATE/calls"
    PATH="$work/bin:$PATH" sh "$s" "$@"
}

pinned='https://reefstore.app/sailfishos/5.2.0.18/%(arch)/'
export STATE="$work/state"
mkdir -p "$STATE"

# 1. Already pinned (literal %(arch)): nothing but ssu lr.
printf '%s' "$pinned" > "$STATE/url"
run "$work/trigger.sh"
check "pinned: no change" "ssu lr" "$(cat "$STATE/calls")"

# 2. Pinned, ssu shows the arch expanded: still nothing.
printf '%s' 'https://reefstore.app/sailfishos/5.2.0.18/aarch64/' > "$STATE/url"
run "$work/trigger.sh"
check "pinned (expanded arch): no change" "ssu lr" "$(cat "$STATE/calls")"

# 3. OS updated from 5.2.0.17 to 5.2.0.18: re-pinned.
printf '%s' 'https://reefstore.app/sailfishos/5.2.0.17/%(arch)/' > "$STATE/url"
run "$work/trigger.sh"
check "drifted: re-pinned" "$pinned" "$(cat "$STATE/url")"
check "drifted: ssu rr, ar, ur" "ssu lr|ssu rr shipwright-reef|ssu ar shipwright-reef $pinned|ssu ur" \
    "$(tr '\n' '|' < "$STATE/calls" | sed 's/|$//')"

# 4. Missing: registered.
rm -f "$STATE/url"
run "$work/trigger.sh"
check "missing: registered" "$pinned" "$(cat "$STATE/url" 2>/dev/null)"

# 5. %postun on upgrade ($1 = 1): nothing.
run "$work/postun.sh" 1
check "postun upgrade: no change" "" "$(cat "$STATE/calls")"

# 6. %postun on removal ($1 = 0): repository removed, key removal detached.
run "$work/postun.sh" 0
check "postun removal: repository gone" "no" "$([ -f "$STATE/url" ] && echo yes || echo no)"
check "postun removal: ssu rr, ur" "ssu rr shipwright-reef|ssu ur" "$(grep '^ssu' "$STATE/calls" | tr '\n' '|' | sed 's/|$//')"
case "$(grep '^setsid' "$STATE/calls")" in
    *"--fork sh -c"*"sh_reef_rmkey"*"Shipwright Reef"*) echo "ok   postun removal: key removal detached" ;;
    *) echo "FAIL postun removal: key removal not detached: $(cat "$STATE/calls")"; fails=$((fails + 1)) ;;
esac

# The client's repo.conf keeps the uid the scriptlet greps for.
check "key uid matches repo.conf" "REEF_KEY_UID='Shipwright Reef'" "$(grep '^REEF_KEY_UID=' "$repo_root/reef/installer/repo.conf")"

if [ "$fails" -ne 0 ]; then
    echo "$fails failure(s)"
    exit 1
fi
echo "all scriptlet checks passed"
