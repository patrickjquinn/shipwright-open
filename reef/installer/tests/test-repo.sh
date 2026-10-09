#!/bin/sh
# SPDX-FileCopyrightText: 2026 Patrick Quinn
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Tests shipwright-reef-repo against stub ssu/rpm/version commands and
# fixture release files. Needs only a POSIX shell; run from anywhere:
#   sh reef/installer/tests/test-repo.sh
# Also runs the scripts through `sh -n` (and busybox ash when installed).

set -eu

here=$(cd "$(dirname "$0")/.." && pwd)
tool="$here/bin/shipwright-reef-repo"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
fails=0

check() {
  # check <description> <expected> <actual>
  if [ "$2" = "$3" ]; then
    echo "ok   $1"
  else
    echo "FAIL $1"
    echo "     expected: $2"
    echo "     actual:   $3"
    fails=$((fails + 1))
  fi
}

# Stubs record their argv, one call per line.
mkdir -p "$work/bin" "$work/root/etc"
for cmd in ssu rpm; do
  cat >"$work/bin/$cmd" <<EOF
#!/bin/sh
echo "$cmd \$*" >> "$work/calls"
EOF
  chmod +x "$work/bin/$cmd"
done
printf '#!/bin/sh\necho "Sailfish OS 5.1.0.11 (Tampella)"\n' >"$work/bin/version"
chmod +x "$work/bin/version"

run() {
  REEF_CONF="$here/repo.conf" REEF_ROOT="$work/root" \
    SSU="$work/bin/ssu" RPM="$work/bin/rpm" VERSION_CMD="$work/bin/version" \
    sh "$tool" "$@"
}

# Syntax.
for s in "$tool" "$here/bin/shipwright-reef-installer"; do
  sh -n "$s" && echo "ok   sh -n $(basename "$s")"
  if command -v busybox >/dev/null 2>&1; then
    busybox ash -n "$s" && echo "ok   busybox ash -n $(basename "$s")"
  fi
done

# Release detection order: sailfish-release, os-release, `version`.
check "falls back to version output" "5.1.0.11" "$(run release)"
printf 'NAME="Sailfish OS"\nVERSION_ID="5.2.0.15"\n' >"$work/root/etc/os-release"
check "reads quoted os-release" "5.2.0.15" "$(run release)"
printf 'NAME=Sailfish OS\nVERSION="5.2.0.17 (Finlayson)"\nVERSION_ID=5.2.0.17\n' >"$work/root/etc/sailfish-release"
check "prefers sailfish-release" "5.2.0.17" "$(run release)"

# URL: literal release, %(arch) left for ssu.
check "full-release URL" \
  "https://reefstore.app/sailfishos/5.2.0.17/%(arch)/" "$(run url)"
sed 's/^REEF_RELEASE_GRANULARITY=full/REEF_RELEASE_GRANULARITY=major-minor/' \
  "$here/repo.conf" >"$work/mm.conf"
check "major-minor URL" \
  "https://reefstore.app/sailfishos/5.2/%(arch)/" \
  "$(REEF_CONF="$work/mm.conf" REEF_ROOT="$work/root" sh "$tool" url)"

# add: rr, ar, ur in that order, exact arguments.
: >"$work/calls"
run add >/dev/null
check "add command sequence" \
  "ssu rr shipwright-reef|ssu ar shipwright-reef https://reefstore.app/sailfishos/5.2.0.17/%(arch)/|ssu ur" \
  "$(paste -sd '|' "$work/calls")"

: >"$work/calls"
run remove >/dev/null
check "remove command sequence" "ssu rr shipwright-reef|ssu ur" "$(paste -sd '|' "$work/calls")"

: >"$work/calls"
run import-key
check "import-key" \
  "rpm --import /usr/share/shipwright-reef-installer/RPM-GPG-KEY-shipwright-reef" \
  "$(cat "$work/calls")"

# remove-key erases only gpg-pubkeys whose summary names the Reef key.
cat >"$work/bin/rpm" <<EOF
#!/bin/sh
echo "rpm \$*" >> "$work/calls"
case "\$1" in
  -q) printf 'gpg-pubkey-11111111-60000000\tgpg(Jolla <release@jolla.com>)\n'
      printf 'gpg-pubkey-22222222-66000000\tgpg(Shipwright Reef <reef@shipwright.example>)\n' ;;
esac
EOF
: >"$work/calls"
run remove-key >/dev/null
check "remove-key erases only the Reef key" \
  "rpm -e gpg-pubkey-22222222-66000000" "$(grep -- ' -e ' "$work/calls")"

# A failing ssu step: non-zero exit, no "registered" line, and nothing after
# the failed step runs (I-01).
cat >"$work/bin/ssu-fails-ar" <<EOF
#!/bin/sh
echo "ssu \$*" >> "$work/calls"
[ "\$1" = ar ] && { echo "ssu: cannot add repository" >&2; exit 1; }
exit 0
EOF
chmod +x "$work/bin/ssu-fails-ar"
: >"$work/calls"
status=0
out=$(REEF_CONF="$here/repo.conf" REEF_ROOT="$work/root" SSU="$work/bin/ssu-fails-ar" \
  RPM="$work/bin/rpm" VERSION_CMD="$work/bin/version" sh "$tool" add 2>"$work/err") || status=$?
check "add fails when ssu ar fails" "1" "$status"
check "no registered line after a failed ssu ar" "" "$out"
check "ssu ur does not run after a failed ssu ar" \
  "ssu rr shipwright-reef|ssu ar shipwright-reef https://reefstore.app/sailfishos/5.2.0.17/%(arch)/" \
  "$(paste -sd '|' "$work/calls")"
check "the failed step is named" "1" "$(grep -c 'ssu ar shipwright-reef .* failed' "$work/err")"
printf '#!/bin/sh\necho "rpm $*" >> "%s/calls"\nexit 1\n' "$work" >"$work/bin/rpm-fails"
chmod +x "$work/bin/rpm-fails"
status=0
REEF_CONF="$here/repo.conf" REEF_ROOT="$work/root" RPM="$work/bin/rpm-fails" \
  sh "$tool" import-key 2>/dev/null || status=$?
check "import-key fails when rpm --import fails" "1" "$status"

# No release anywhere: fail rather than register a wrong URL.
rm "$work/root/etc/sailfish-release" "$work/root/etc/os-release"
printf '#!/bin/sh\necho unknown\n' >"$work/bin/version"
: >"$work/calls"
if run add >/dev/null 2>&1; then
  check "add fails without a release" "failure" "success"
else
  check "add fails without a release" "" "$(cat "$work/calls")"
fi

if [ $fails -ne 0 ]; then
  echo "$fails failure(s)"
  exit 1
fi
echo "all installer script tests passed"
