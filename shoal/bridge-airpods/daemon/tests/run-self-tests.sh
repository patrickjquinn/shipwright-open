#!/bin/sh
# SPDX-FileCopyrightText: 2026 Patrick Quinn
# SPDX-License-Identifier: GPL-3.0-only
#
# Runs the self-tests our patches add (TestsAapAppleGated: Device ID,
# ATT codec, hearing settings, control commands and the ATT channel with a
# fake transport; TestsAapEarDetection: the ear detection parser and the
# pause/resume state machine with fake media players) and fails unless every
# one of them reports PASS. They run at start-up of a DEBUG build only:
#
#   BUILD_TYPE=Debug ./build.sh /tmp/apdebug
#   tests/run-self-tests.sh /tmp/apdebug/build/magicpodscore
#
# The test names come from the headers of those classes in the patched tree
# next to the binary's build directory, so a test that never ran fails too. Upstream's own self-tests (TestsAapBle and others) are reported but
# not judged: some of them fail upstream.
#
# Needs dbus-daemon. Port 2020 must be free.

set -u

bin=$(readlink -f "${1:?usage: $0 path/to/debug/magicpodscore}")
tests_dir=$(dirname "$bin")/../src/src/tests
headers="$tests_dir/TestsAapAppleGated.h $tests_dir/TestsAapEarDetection.h"
for header in $headers; do
    [ -r "$header" ] || { echo "cannot read $header (build with build.sh)" >&2; exit 1; }
done
work=$(mktemp -d)
trap 'cleanup; rm -rf "${work:?}"' EXIT

cleanup() {
    [ -n "${dpid:-}" ] && kill "$dpid" 2>/dev/null
    [ -n "${bpid:-}" ] && kill "$bpid" 2>/dev/null
    dpid='' bpid=''
    wait 2>/dev/null
}

# A private, empty bus, so the daemon touches nothing on this machine.
dbus-daemon --session --nofork --address="unix:path=$work/bus.sock" 2>/dev/null &
bpid=$!
tries=0
while [ ! -S "$work/bus.sock" ] && [ "$tries" -lt 50 ]; do sleep 0.1; tries=$((tries + 1)); done

# shellcheck disable=SC2086 # word splitting of the header list is intended
names=$(sed -n 's/^ *bool \(Test[A-Za-z0-9_]*\)();.*/\1/p' $headers)

mkdir -p "$work/home"
DBUS_SYSTEM_BUS_ADDRESS="unix:path=$work/bus.sock" HOME="$work/home" "$bin" > "$work/daemon.log" 2>&1 &
dpid=$!
tries=0
all_reported() {
    for name in $names; do
        grep -qE "^DBG $name +: (PASS|FAIL)" "$work/daemon.log" || return 1
    done
}
while ! all_reported && [ "$tries" -lt 300 ]; do
    sleep 0.1
    tries=$((tries + 1))
done
cleanup

pass=0
fail=0
for name in $names; do
    if grep -qE "^DBG $name +: PASS$" "$work/daemon.log"; then
        echo "  ok    $name"
        pass=$((pass + 1))
    else
        echo "  FAIL  $name ($(grep -E "^DBG $name +:" "$work/daemon.log" | sed 's/.*: //' || true))"
        fail=$((fail + 1))
    fi
done
all_fail=$(grep -cE '^DBG [A-Za-z0-9_.]+ +: FAIL$' "$work/daemon.log")
upstream_fail=$((all_fail - fail))
echo
echo "passed $pass, failed $fail (upstream self-tests failing, not judged: $upstream_fail)"
[ "$fail" -eq 0 ] && [ "$pass" -gt 0 ]
