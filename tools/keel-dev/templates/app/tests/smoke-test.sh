#!/bin/sh
# SPDX-FileCopyrightText: 2026 Patrick Quinn
# SPDX-License-Identifier: MIT-0
#
# Headless smoke test: runs the real binary offscreen against the real QML
# (qml/) and a Keel build, waits for the first page, checks that it is the
# main page, that the cover loads and that no QML warning was printed, and
# saves a screenshot.
#
#   tests/smoke-test.sh [path/to/lib{{crate}}.so]
#
# Needs a Keel build: KEEL_QML_DIR (its qml/ directory, holding
# Sailfish/Silica) or KEEL_BUILD_DIR (the build directory). The app
# defaults to target/debug/lib{{crate}}.so (honouring CARGO_TARGET_DIR).
# SMOKE_SCREENSHOT keeps the PNG at that path.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
app=$(cd "$here/.." && pwd)
bin=${1:-"${CARGO_TARGET_DIR:-$app/target}/debug/lib{{crate}}.so"}

[ -x "$bin" ] || { echo "smoke-test: no binary at $bin (cargo build first)" >&2; exit 2; }
if [ -z "${KEEL_QML_DIR:-}" ] && [ -n "${KEEL_BUILD_DIR:-}" ]; then
    KEEL_QML_DIR=$KEEL_BUILD_DIR/qml
fi
if [ -z "${KEEL_QML_DIR:-}" ] || [ ! -f "$KEEL_QML_DIR/Sailfish/Silica/qmldir" ]; then
    echo "smoke-test: set KEEL_BUILD_DIR to a Keel build (or KEEL_QML_DIR to its qml/)" >&2
    exit 2
fi

work=$(mktemp -d "${TMPDIR:-/tmp}/{{name}}-smoke.XXXXXX")
trap 'rm -rf "$work"' EXIT INT TERM
# The app is a shared library that is also executable; the RPM installs it as
# /usr/bin/{{name}}. Run it under that name: Keel takes the application's
# name from argv[0].
ln -s "$(cd "$(dirname "$bin")" && pwd)/$(basename "$bin")" "$work/{{name}}"
bin="$work/{{name}}"
mkdir -p "$work/home" "$work/runtime"
chmod 700 "$work/runtime"
shot=${SMOKE_SCREENSHOT:-$work/first-page.png}

status=0
env -i PATH="/usr/bin:/bin" HOME="$work/home" XDG_RUNTIME_DIR="$work/runtime" \
    LANG=C.UTF-8 QT_QPA_PLATFORM=offscreen QT_FORCE_STDERR_LOGGING=1 QT_LOGGING_RULES="qml.debug=true;js.debug=true;default.debug=true;qt.qml.invalidOverride.warning=false;qt.qml.propertyCache.append.warning=false" QT_QUICK_BACKEND=software \
    QML_IMPORT_PATH="$KEEL_QML_DIR" KEEL_SAILFISHAPP_DATADIR="$app" \
    LD_LIBRARY_PATH="${LD_LIBRARY_PATH:-}" \
    DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent/{{name}}-smoke-no-bus" \
    KEEL_RUN_SCREENSHOT="$shot" \
    timeout 60 "$bin" > "$work/out" 2>&1 || status=$?
cat "$work/out"

fail() { echo "smoke-test: FAIL: $*" >&2; exit 1; }
[ "$status" -eq 0 ] || fail "exit status $status"
grep -q '^keel-run: first page: mainPage ' "$work/out" || fail "the main page did not load"
grep -q '^keel-run: cover ok' "$work/out" || fail "the cover did not load"
# PNG signature.
[ "$(od -A n -t x1 -N 4 "$shot" | tr -d ' ')" = "89504e47" ] || fail "no PNG at $shot"
# Any warning pointing into the app's QML is a failure.
if grep -E "file://$app/qml/.*:[0-9]+" "$work/out"; then
    fail "QML warnings above"
fi
echo "smoke-test: PASS"
