#!/bin/sh
# SPDX-FileCopyrightText: 2026 Patrick Quinn
# SPDX-License-Identifier: GPL-3.0-only
#
# Runs the patched daemon against fake_bluez.py on a private D-Bus bus and
# checks adapter selection (patch 0001), the loopback bind and the Origin
# check (patch 0004) and the WebSocket handshake, including with the UI's
# own Backend.qml. No Bluetooth hardware or real BlueZ involved.
#
#   tests/run-adapter-tests.sh path/to/magicpodscore
#
# Needs dbus-daemon and python3 with PyGObject (for fake_bluez.py; PYTHON_GI
# names the interpreter, default python3.12, else python3), and for the UI check
# qmltestrunner (Qt 6) with the QtTest and QtWebSockets QML modules
# (QMLTESTRUNNER overrides the path). Without qmltestrunner that check is
# skipped, or fails when CI is set. Port 2020 must be free.

set -u

bin=$(readlink -f "${1:?usage: $0 path/to/magicpodscore}")
here=$(cd "$(dirname "$0")" && pwd)
py=${PYTHON_GI:-$(command -v python3.12 || echo python3)}
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

start_bus() {
    rm -f "$work/bus.sock"
    cat > "$work/bus.conf" <<CONF
<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>system</type>
  <listen>unix:path=$work/bus.sock</listen>
  <auth>EXTERNAL</auth>
  <policy context="default">
    <allow user="*"/><allow own="*"/>
    <allow send_type="method_call"/><allow send_type="signal"/>
    <allow send_requested_reply="true" send_type="method_return"/>
    <allow send_requested_reply="true" send_type="error"/>
    <allow receive_type="method_call"/><allow receive_type="signal"/>
    <allow receive_requested_reply="true" receive_type="method_return"/>
    <allow receive_requested_reply="true" receive_type="error"/>
  </policy>
</busconfig>
CONF
    dbus-daemon --config-file="$work/bus.conf" --nofork 2>/dev/null &
    bpid=$!
    wait_for_socket "$work/bus.sock"
    export DBUS_SYSTEM_BUS_ADDRESS="unix:path=$work/bus.sock"
}

wait_for_socket() {
    tries=0
    while [ ! -S "$1" ] && [ "$tries" -lt 50 ]; do sleep 0.1; tries=$((tries + 1)); done
}

# wait_for FILE REGEX: poll for up to 15 s instead of sleeping a fixed time,
# so a slow runner gets more time and a fast one does not wait for nothing.
wait_for() {
    tries=0
    while ! grep -qE "$2" "$work/$1" 2>/dev/null; do
        [ "$tries" -ge 150 ] && return 1
        sleep 0.1
        tries=$((tries + 1))
    done
}

# start "fake_bluez args" "daemon env" UNTIL: starts the bus, the fake and the
# daemon, then waits until daemon.log matches UNTIL.
start() {
    fargs=$1 denv=$2 until=$3
    # A log left from the previous scenario must not satisfy wait_for before
    # the new process has truncated it.
    rm -f "$work/fake.log" "$work/daemon.log" "$work/ws.log"
    start_bus
    # shellcheck disable=SC2086 # the argument lists are word-split on purpose
    "$py" "$here/fake_bluez.py" $fargs > "$work/fake.log" 2>&1 &
    fpid=$!
    wait_for fake.log 'owns org\.bluez'
    mkdir -p "$work/home"
    # shellcheck disable=SC2086
    env HOME="$work/home" $denv "$bin" > "$work/daemon.log" 2>&1 &
    dpid=$!
    wait_for daemon.log 'Initialization complete'
    wait_for daemon.log "$until"
}

probe() {
    python3 "$here/wsprobe.py" --hold 1 "$@" '{"method":"GetDefaultBluetoothAdapter"}' > "$work/ws.log" 2>&1
}

# scenario "fake_bluez args" "daemon env" UNTIL: start, probe once, stop.
scenario() {
    start "$1" "$2" "$3"
    probe
    awk '$2 ~ /:07E4$/ && $4 == "0A" {print $2}' /proc/net/tcp > "$work/listen.log"
    if [ -r /proc/net/tcp6 ]; then
        awk '$2 ~ /:07E4$/ && $4 == "0A" {print $2}' /proc/net/tcp6 > "$work/listen6.log"
    else
        : > "$work/listen6.log"
    fi
    cleanup
}

check() {
    what=$1 file=$2 pattern=$3
    if grep -qE "$pattern" "$work/$file"; then
        echo "  ok    $what"
        pass=$((pass + 1))
    else
        echo "  FAIL  $what (no /$pattern/ in $file)"
        sed 's/^/        /' "$work/$file"
        fail=$((fail + 1))
    fi
}

check_absent() {
    what=$1 file=$2 pattern=$3
    if grep -qE "$pattern" "$work/$file"; then
        echo "  FAIL  $what (found /$pattern/ in $file)"
        fail=$((fail + 1))
    else
        echo "  ok    $what"
        pass=$((pass + 1))
    fi
}

echo "1. hci0 unpowered, hci1 powered (Jolla Phone-like, plus a dead hci0)"
scenario "hci0:off hci1:on" "" 'Using Bluetooth adapter'
check "selects hci1" daemon.log 'Using Bluetooth adapter: /org/bluez/hci1 \(powered\)'
check_absent "never selects hci0" daemon.log 'Using Bluetooth adapter: /org/bluez/hci0'
check "API reports adapter enabled" ws.log '"defaultbluetooth":\{"enabled":true\}'
check "init handshake" ws.log '"init":\{"api":0'
check "listens on 127.0.0.1" listen.log '^0100007F:07E4$'
check_absent "no IPv4 wildcard listener" listen.log '^00000000:07E4$'
check_absent "no IPv6 listener" listen6.log ':07E4$'

echo "2. only hci1, powered (the Jolla Phone (2026) case)"
scenario "hci1:on" "" 'Using Bluetooth adapter'
check "selects hci1" daemon.log 'Using Bluetooth adapter: /org/bluez/hci1 \(powered\)'

echo "3. MAGICPODS_ADAPTER=hci1 with hci0 and hci1 both powered"
scenario "hci0:on hci1:on" "MAGICPODS_ADAPTER=hci1" 'Using Bluetooth adapter'
check "pinned message" daemon.log 'pinned by MAGICPODS_ADAPTER: /org/bluez/hci1'
check "ignores hci0" daemon.log 'Ignoring Bluetooth adapter /org/bluez/hci0'
check "selects hci1" daemon.log 'Using Bluetooth adapter: /org/bluez/hci1'

echo "4. MAGICPODS_ADAPTER=hci1 but only hci0 exists"
scenario "hci0:on" "MAGICPODS_ADAPTER=hci1" 'Initialization complete'
check "starts without adapter" daemon.log 'No usable Bluetooth adapter yet'
check "still serves the API" daemon.log 'Initialization complete'
check "API reports adapter disabled" ws.log '"defaultbluetooth":\{"enabled":false\}'

echo "5. no adapter at start, hci1 appears after 2 s"
scenario "--add 2:hci1:on" "" 'Using Bluetooth adapter: /org/bluez/hci1'
check "starts without adapter" daemon.log 'No usable Bluetooth adapter yet'
check "picks up hci1" daemon.log 'Using Bluetooth adapter: /org/bluez/hci1 \(powered\)'
check "API reports adapter enabled" ws.log '"defaultbluetooth":\{"enabled":true\}'

echo "6. only hci0 unpowered, powered hci1 appears after 2 s"
scenario "hci0:off --add 2:hci1:on" "" 'Using Bluetooth adapter: /org/bluez/hci1'
check "first uses hci0" daemon.log 'Using Bluetooth adapter: /org/bluez/hci0 \(not powered\)'
check "switches to hci1" daemon.log 'Switching from unpowered Bluetooth adapter /org/bluez/hci0 to powered adapter /org/bluez/hci1'
check "API reports adapter enabled" ws.log '"defaultbluetooth":\{"enabled":true\}'

echo "7. hci1 removed after 2 s, hci0 appears after 3 s"
scenario "hci1:on --remove 2:hci1 --add 3:hci0:on" "" 'Using Bluetooth adapter: /org/bluez/hci0'
check "adapter removed" daemon.log 'Bluetooth adapter removed: /org/bluez/hci1'
check "waits for a new one" daemon.log 'No Bluetooth adapter available; waiting'
check "picks up hci0" daemon.log 'Using Bluetooth adapter: /org/bluez/hci0 \(powered\)'

echo "8. selected adapter powered on later"
scenario "hci1:off --power 2:hci1:on" "" 'Bluetooth adapter powered ON'
check "API reports adapter enabled" ws.log '"defaultbluetooth":\{"enabled":true\}'

echo "9. hci0 and hci1 unpowered, hci1 powered on after 2 s"
scenario "hci0:off hci1:off --power 2:hci1:on" "" 'Using Bluetooth adapter: /org/bluez/hci1'
check "first uses hci0" daemon.log 'Using Bluetooth adapter: /org/bluez/hci0 \(not powered\)'
check "switches to hci1" daemon.log 'Switching from unpowered Bluetooth adapter /org/bluez/hci0 to powered adapter /org/bluez/hci1'
check "API reports adapter enabled" ws.log '"defaultbluetooth":\{"enabled":true\}'

echo "10. web pages are refused, native clients and the UI are not"
start "hci1:on" "MAGICPODS_ALLOWED_ORIGINS=https://allowed.example" 'Using Bluetooth adapter'
probe --origin http://evil.example
cp "$work/ws.log" "$work/ws-evil.log"
probe --origin null
cp "$work/ws.log" "$work/ws-null.log"
probe --origin https://allowed.example
cp "$work/ws.log" "$work/ws-allowed.log"
probe
runner=${QMLTESTRUNNER:-}
if [ -z "$runner" ]; then
    for c in /usr/lib/qt6/bin/qmltestrunner /usr/lib64/qt6/bin/qmltestrunner qmltestrunner6 qmltestrunner; do
        if command -v "$c" >/dev/null 2>&1; then runner=$c; break; fi
    done
fi
if [ -n "$runner" ]; then
    QT_QPA_PLATFORM=offscreen QT_FORCE_STDERR_LOGGING=1 QT_LOGGING_RULES="qml.debug=true;js.debug=true;default.debug=true;qt.qml.invalidOverride.warning=false;qt.qml.propertyCache.append.warning=false" "$runner" -input "$here/tst_ui_link.qml" > "$work/ui.log" 2>&1
fi
cleanup
check "a page's Origin is refused (403)" ws-evil.log 'handshake failed: HTTP/1.1 403'
check "the refusal is logged" daemon.log 'Refused a WebSocket connection from origin http://evil.example'
check "Origin: null (file:// pages) is refused" ws-null.log 'handshake failed: HTTP/1.1 403'
check "an allowed origin connects" ws-allowed.log '"init":\{"api":0'
check "no Origin (native client) connects" ws.log '"defaultbluetooth":\{"enabled":true\}'
if [ -n "$runner" ]; then
    check "the UI's Backend.qml connects" ui.log 'Totals: 3 passed, 0 failed'
elif [ -n "${CI:-}" ]; then
    echo "  FAIL  the UI's Backend.qml connects (qmltestrunner not found)"
    fail=$((fail + 1))
else
    echo "  skip  the UI's Backend.qml connects (qmltestrunner not found)"
fi

echo
echo "passed $pass, failed $fail"
[ "$fail" -eq 0 ]
