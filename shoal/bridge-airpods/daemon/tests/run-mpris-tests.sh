#!/bin/sh
# SPDX-FileCopyrightText: 2026 Patrick Quinn
# SPDX-License-Identifier: GPL-3.0-only
#
# Ear detection pause and resume (patch 0009) over a real D-Bus: starts a
# private bus, two fake MPRIS players (fake_mpris.py: "music" playing, "app"
# paused) and a DEBUG daemon with MAGICPODS_TEST_MPRIS set, which runs
# TestsEarPauseMpris (EarPauseController with the real MprisClient) at
# start-up. Then checks:
#
# - every test named in src/tests/TestsEarPauseMpris.h reports PASS;
# - which player was asked to do what (the fake logs every call): "app",
#   which was paused all along, got no call at all;
# - the "earDetectionPause" setting: written to config.toml with its
#   default, and changed over the WebSocket API (SetSetting), which the
#   daemon applies at once.
#
#   BUILD_TYPE=Debug ./build.sh /tmp/apdebug
#   tests/run-mpris-tests.sh /tmp/apdebug/build/magicpodscore
#
# Needs dbus-daemon and python3 with PyGObject (PYTHON_GI names the
# interpreter, default python3.12, else python3). Port 2020 must be free.

set -u

bin=$(readlink -f "${1:?usage: $0 path/to/debug/magicpodscore}")
here=$(cd "$(dirname "$0")" && pwd)
py=${PYTHON_GI:-$(command -v python3.12 || echo python3)}
header=$(dirname "$bin")/../src/src/tests/TestsEarPauseMpris.h
[ -r "$header" ] || { echo "cannot read $header (build with build.sh)" >&2; exit 1; }
work=$(mktemp -d)
trap 'cleanup; rm -rf "${work:?}"' EXIT
pass=0
fail=0

cleanup() {
    [ -n "${dpid:-}" ] && kill "$dpid" 2>/dev/null
    [ -n "${fpid:-}" ] && kill "$fpid" 2>/dev/null
    [ -n "${bpid:-}" ] && kill "$bpid" 2>/dev/null
    dpid='' fpid='' bpid=''
    wait 2>/dev/null
}

check() {
    if eval "$2"; then
        echo "  ok    $1"
        pass=$((pass + 1))
    else
        echo "  FAIL  $1"
        fail=$((fail + 1))
    fi
}

# wait_for FILE REGEX: up to 15 s.
wait_for() {
    tries=0
    while ! grep -qE "$2" "$1" 2>/dev/null; do
        [ "$tries" -ge 150 ] && return 1
        sleep 0.1
        tries=$((tries + 1))
    done
}

# One private bus serves as both the system bus (no BlueZ on it: the daemon
# runs without an adapter) and the session bus (the players).
dbus-daemon --session --nofork --address="unix:path=$work/bus.sock" 2>/dev/null &
bpid=$!
tries=0
while [ ! -S "$work/bus.sock" ] && [ "$tries" -lt 50 ]; do sleep 0.1; tries=$((tries + 1)); done
export DBUS_SYSTEM_BUS_ADDRESS="unix:path=$work/bus.sock"
export DBUS_SESSION_BUS_ADDRESS="unix:path=$work/bus.sock"

"$py" "$here/fake_mpris.py" music=Playing app=Paused > "$work/fake.log" 2>&1 &
fpid=$!
wait_for "$work/fake.log" "^fake-mpris: ready" || { cat "$work/fake.log" >&2; echo "fake players did not start" >&2; exit 1; }

mkdir -p "$work/home"
HOME="$work/home" XDG_CONFIG_HOME="$work/home/.config" MAGICPODS_TEST_MPRIS=1 \
    "$bin" > "$work/daemon.log" 2>&1 &
dpid=$!

names=$(sed -n 's/^ *bool \(Test[A-Za-z0-9_]*\)();.*/\1/p' "$header")
all_reported() {
    for name in $names; do
        grep -qE "^DBG $name +: (PASS|FAIL)" "$work/daemon.log" || return 1
    done
}
tries=0
while ! all_reported && [ "$tries" -lt 300 ]; do sleep 0.1; tries=$((tries + 1)); done

echo "TestsEarPauseMpris (controller with the real MprisClient):"
for name in $names; do
    check "$name" "grep -qE '^DBG $name +: PASS$' '$work/daemon.log'"
done

echo "Calls the players received:"
check "app (paused all along) got no call" "! grep -qE '^fake-mpris: app ' '$work/fake.log'"
# Pause and resume; pause, the user's Play and Pause, our nothing, the test's
# Play back; the user's Pause, then nothing from us.
check "music got exactly the expected calls" \
    "[ \"\$(sed -n 's/^fake-mpris: music //p' '$work/fake.log' | tr '\n' ' ')\" = 'Pause Play Pause Play Pause Play Pause ' ]"

echo "The earDetectionPause setting:"
wait_for "$work/daemon.log" "Initialization complete" || true
check "default oneRemoved applied at start" "grep -q 'Ear detection pause: oneRemoved' '$work/daemon.log'"
check "default written to config.toml" \
    "grep -qE \"^earDetectionPause = ['\\\"]oneRemoved['\\\"]\" '$work/home/.config/magicpods/config.toml'"
"$py" "$here/wsprobe.py" --hold 2 \
    '{"method":"SetSetting","arguments":{"container":"magicpods","setting":"earDetectionPause","value":"bothRemoved"}}' \
    '{"method":"GetSetting","arguments":{"container":"magicpods","setting":"earDetectionPause"}}' \
    > "$work/probe.log" 2>&1
check "SetSetting over the API applies at once" "wait_for '$work/daemon.log' 'Ear detection pause: bothRemoved'"
check "GetSetting reports it" "grep -q '\"earDetectionPause\":\"bothRemoved\"' '$work/probe.log'"
"$py" "$here/wsprobe.py" --hold 1 \
    '{"method":"SetSetting","arguments":{"container":"magicpods","setting":"earDetectionPause","value":"sometimes"}}' \
    > /dev/null 2>&1
check "an unknown value falls back to oneRemoved" "wait_for '$work/daemon.log' 'Unknown earDetectionPause \"sometimes\"; using oneRemoved'"

cleanup
echo
echo "passed $pass, failed $fail"
if [ "$fail" -ne 0 ]; then
    echo "--- daemon log (MPRIS lines) ---"
    grep -iE "mpris|media player|bud|ear detection|Test" "$work/daemon.log" | tail -n 60
    echo "--- fake players ---"
    cat "$work/fake.log"
fi
[ "$fail" -eq 0 ] && [ "$pass" -gt 0 ]
