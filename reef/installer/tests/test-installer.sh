#!/bin/sh
# SPDX-FileCopyrightText: 2026 Patrick Quinn
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Tests shipwright-reef-installer's refresh and install steps against stub
# commands (pkcon, ssu tools, notifications, setsid run in place, sleep
# instant). Needs only a POSIX shell; run from anywhere:
#   sh reef/installer/tests/test-installer.sh
#
# Checks: a refresh that keeps failing is retried with nothing of anyone
# else's stopped (no killall, no PackageKit restart: the Chum installer's
# escalation aborted other people's installs and OS updates), and the
# client is installed anyway; a refresh that succeeds goes straight to the
# install; a failed install says so on the screen; a release with no
# repository removes the installer and says why.

set -u
here=$(cd "$(dirname "$0")/.." && pwd)
script=$here/bin/shipwright-reef-installer
work=$(mktemp -d "${TMPDIR:-/tmp}/reef-installer-test.XXXXXX")
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

# run REFRESH_FAILS INSTALL_STATUS CHECK_STATUS: one installer run with a
# fresh set of stubs; calls go to $work/calls.
run() {
  rm -rf "${work:?}/bin" "$work/calls" "$work/n" "$work/run"
  mkdir -p "$work/run/100000/dbus"
  python3 -c 'import socket,sys; socket.socket(socket.AF_UNIX).bind(sys.argv[1])' "$work/run/100000/dbus/user_bus_socket"
  mkdir -p "$work/bin"
  : >"$work/calls"
  echo 0 >"$work/n"
  for cmd in killall systemctl ssu systemd-cat; do
    printf '#!/bin/sh\necho "%s $*" >> "%s/calls"\n' "$cmd" "$work" >"$work/bin/$cmd"
  done
  # pkcon: refresh fails the first $1 times; install exits $2.
  cat >"$work/bin/pkcon" <<EOF
#!/bin/sh
echo "pkcon \$*" >> "$work/calls"
case "\$*" in
  *repo-set-data*)
    n=\$((\$(cat "$work/n") + 1)); echo \$n > "$work/n"
    [ \$n -gt $1 ] ;;
  *install*) exit $2 ;;
esac
EOF
  printf '#!/bin/sh\nexit 0\n' >"$work/bin/sleep"
  printf '#!/bin/sh\necho "defaultuser:x:100000:100000::/home/defaultuser:/bin/sh"\n' >"$work/bin/getent"
  # runuser -u USER -- CMD...: CMD in place.
  # shellcheck disable=SC2016 # the stub's own $@
  printf '#!/bin/sh\nshift 3\n"$@"\n' >"$work/bin/runuser"
  # setsid --fork CMD...: in place, so the test sees what the detached part does.
  # shellcheck disable=SC2016 # the stub's own $1 and $@
  printf '#!/bin/sh\n[ "$1" = --fork ] && shift\n"$@"\n' >"$work/bin/setsid"
  # The repo tool: release, check (exit $3), import-key.
  cat >"$work/bin/repo-tool" <<EOF
#!/bin/sh
echo "repo-tool \$*" >> "$work/calls"
case "\$1" in
  release) echo 5.2.0.18 ;;
  check) exit $3 ;;
esac
EOF
  # The notification: recorded, as notify_user's gdbus would be.
  printf '#!/bin/sh\necho "notify $*" >> "%s/calls"\n' "$work" >"$work/bin/gdbus"
  chmod +x "$work/bin/"*
  cp "$script" "$work/installer"
  chmod +x "$work/installer"
  cat >"$work/repo.conf" <<'EOF'
REEF_ALIAS=shipwright-reef
REEF_CLIENT_PACKAGE=shipwright-reef
REEF_INSTALLER_PACKAGE=shipwright-reef-installer
REEF_KEY_FILE=/nonexistent
EOF
  PATH="$work/bin:$PATH" REEF_RUN_USER="$work/run" REEF_CONF="$work/repo.conf" REPO_TOOL="$work/bin/repo-tool" \
    "$work/installer" 999999 "$work/log" >"$work/out" 2>&1
  echo $? >"$work/status"
}

count() { grep -c "^pkcon.*$1" "$work/calls"; }

# A refresh that never succeeds.
run 99 0 0
check "refresh retried five times" 5 "$(count 'repo-set-data')"
check "no pkcon of anyone else's killed" 0 "$(grep -c '^killall' "$work/calls")"
check "PackageKit never restarted" 0 "$(grep -c '^systemctl' "$work/calls")"
check "never pkcon quit" 0 "$(count ' quit')"
check "the client installed anyway" 1 "$(count '-pvy install shipwright-reef')"

# A refresh that succeeds the second time.
run 1 0 0
check "refresh: one retry" 2 "$(count 'repo-set-data')"
check "then the client" 1 "$(count '-pvy install shipwright-reef')"
check "a successful install shows nothing" 0 "$(grep -c '^notify' "$work/calls")"

# The install fails: the person is told.
run 0 1 0
check "a failed install is shown" 1 "$(grep -c 'notify.*Reef could not be installed' "$work/calls")"

# No repository for this release: removed, with the reason on the screen.
run 0 0 4
check "no repository: exit 4" 4 "$(cat "$work/status")"
check "no repository: the installer removes itself" 1 "$(count '-py remove shipwright-reef-installer')"
check "no repository: shown" 1 "$(grep -c 'notify.*Reef cannot be installed' "$work/calls")"
check "no repository: nothing installed" 0 "$(count '-pvy install')"

if [ "$fails" -gt 0 ]; then
  echo "test-installer: $fails failed"
  exit 1
fi
echo "test-installer: all passed"
