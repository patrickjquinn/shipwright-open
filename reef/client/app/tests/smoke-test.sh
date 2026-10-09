#!/bin/sh
# SPDX-FileCopyrightText: 2026 Patrick Quinn
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Headless smoke test for shipwright-reef.
#
#   reef/client/app/tests/smoke-test.sh [path/to/shipwright-reef]
#
# Builds three tiny noarch RPMs, generates a catalogue for them with
# tools/build/reef/gen-catalogue.py into a local file:// repository, and runs
# the real binary offscreen against the real UI (reef/client/ui/qml) on the
# real Keel Sailfish.Silica module, from a Keel build:
#
#   cmake -S keel -B <dir> -G Ninja && cmake --build <dir>
#   KEEL_QML_DIR=<dir>/qml reef/client/app/tests/smoke-test.sh
#
# Keel rejects unknown properties and types, so the UI's use of Silica is
# checked too. The harness (tests/harness/smoke.qml.in) checks the Reef
# object. Two runs:
#
#   pinned   ssu reports the repository pinned to the installed release
#   drifted  ssu reports it pinned to an older release; the harness calls
#            repairRepository() and checks the re-pin went through ssu
#
# Nothing touches the host: the release comes from a fixture root
# (REEF_SYSROOT), `ssu` and `pkcon` are stubs on PATH, the system bus points
# at a socket that does not exist (so PackageKit is unreachable and install
# state is empty), and HOME/XDG dirs are scratch.
#
# Also runs `shipwright-reef --check-updates` (the background check) against
# the same repository: it must cache the catalogue and record the check,
# and do nothing when the setting is "Never".
#
# Last, the real main QML must show a main window (tools/window-probe).
#
# Needs: Qt 6 (QtQuick), a Keel build (KEEL_QML_DIR), rpmbuild, python3,
# CMake and the Qt 6 Gui development files (for the window probe).
# Without an argument it uses $CARGO_TARGET_DIR (or <repo>/target)/debug/
# libshipwright_reef.so, the app built with `cargo build` in reef/client/app.

set -eu

here=$(cd "$(dirname "$0")" && pwd)
app=$(cd "$here/.." && pwd)
repo_root=$(cd "$app/../../.." && pwd)
ui_dir="$repo_root/reef/client/ui/qml"
bin=${1:-${CARGO_TARGET_DIR:-$repo_root/target}/debug/libshipwright_reef.so}
arch=$(uname -m)

[ -x "$bin" ] || { echo "smoke-test: no binary at $bin (cargo build first)" >&2; exit 2; }
keel_qml=${KEEL_QML_DIR:-}
if [ -z "$keel_qml" ] || [ ! -f "$keel_qml/Sailfish/Silica/qmldir" ]; then
    echo "smoke-test: set KEEL_QML_DIR to a Keel build's qml directory" \
         "(cmake -S keel -B <dir> && cmake --build <dir>; then <dir>/qml)" >&2
    exit 2
fi

work=$(mktemp -d "${TMPDIR:-/tmp}/reef-smoke.XXXXXX")
trap 'rm -rf "$work"' EXIT INT TERM
# The app is a shared library that is also executable; the RPM installs it as
# /usr/bin/shipwright-reef. Run it under that name: Keel takes the application's
# name from argv[0].
ln -s "$(cd "$(dirname "$bin")" && pwd)/$(basename "$bin")" "$work/shipwright-reef"
bin=$work/shipwright-reef

# ---- fixture RPMs and catalogue ------------------------------------------

mkrpm() { # name version summary
    cat > "$work/$1.spec" <<EOF
Name: $1
Version: $2
Release: 1
Summary: $3
License: MIT
URL: https://example.invalid/$1
BuildArch: noarch
%description
$3.
%install
mkdir -p %{buildroot}/usr/share/$1
printf 'smoke test payload for $1\n' > %{buildroot}/usr/share/$1/README
%files
/usr/share/$1
EOF
    rpmbuild -bb --quiet --define "_topdir $work/rpmbuild" "$work/$1.spec" >/dev/null
}
mkrpm harbour-smoke-bridge 1.0.0 "Headphone bridge"
mkrpm harbour-smoke-notes 0.2.0 "Plain notes"
mkrpm harbour-smoke-messages 0.3.0 "Messages"

cat > "$work/meta.json" <<'EOF'
{"packages": {
  "harbour-smoke-bridge": {
    "title": "Bridge", "category": "Audio", "description": "Battery and noise modes.",
    "publisher": "Smoke Co", "added": "2026-01-02",
    "icon": "assets/smoke-bridge-0a1b2c3d.png",
    "screenshots": ["assets/shots/bridge-1-0a1b2c3d.png", "assets/shots/bridge-2-fake.png"],
    "permissions": ["Bluetooth"], "sandboxed": true,
    "licence": {"model": "one_off", "app_id": "smoke-bridge",
                "purchase_url": "https://example.invalid/buy"},
    "tested_on": {"1.0.0-1": ["5.2.0.17", "5.2.0.18"]}},
  "harbour-smoke-notes": {
    "title": "Notes", "category": "Office", "licence": {"model": "free"},
    "tested_on": {"0.2.0-1": ["5.2.0.17"]}},
  "harbour-smoke-messages": {
    "title": "Messages", "category": "Communication",
    "licence": {"model": "subscription", "app_id": "smoke-messages",
                "purchase_url": "https://example.invalid/sub"},
    "tested_on": {"0.3.0-1": ["5.2.0.18"]}}
}}
EOF

repo="$work/repo/sailfishos/5.2.0.17/$arch"
mkdir -p "$repo/assets/shots"
cp "$work"/rpmbuild/RPMS/noarch/*.rpm "$repo/"
# Assets: a 1x1 PNG as the icon and as a screenshot, and a "screenshot" that
# is not an image (the client must refuse it).
printf '\211PNG\r\n\032\n\0\0\0\rIHDR\0\0\0\001\0\0\0\001\010\006\0\0\0\037\025\304\211\0\0\0\nIDATx\234c\0\001\0\0\005\0\001\r\n-\264\0\0\0\0IEND\256B`\202' \
    > "$repo/assets/smoke-bridge-0a1b2c3d.png"
cp "$repo/assets/smoke-bridge-0a1b2c3d.png" "$repo/assets/shots/bridge-1-0a1b2c3d.png"
printf '<html>not an image</html>' > "$repo/assets/shots/bridge-2-fake.png"
python3 "$repo_root/tools/build/reef/gen-catalogue.py" --repo-dir "$repo" \
    --release 5.2.0.17 --arch "$arch" --meta "$work/meta.json" --out "$repo/catalogue.json" \
    --featured harbour-smoke-notes --featured harbour-smoke-nope

# ---- fixture phone ---------------------------------------------------------

mkdir -p "$work/root/etc" "$work/bin"
printf 'NAME="Sailfish OS"\nVERSION_ID=5.2.0.17\n' > "$work/root/etc/sailfish-release"

template="file://$work/repo/sailfishos/{release}/{arch}/"
pinned_url="file://$work/repo/sailfishos/5.2.0.17/%(arch)/"
drifted_url="file://$work/repo/sailfishos/5.2.0.15/%(arch)/"

# ssu: lr prints the registered URL kept in $REEF_SMOKE_STATE/url; rr, ar, er
# and ur edit it. Every call is logged.
cat > "$work/bin/ssu" <<'EOF'
#!/bin/sh
state=$REEF_SMOKE_STATE
echo "ssu $*" >> "$state/log"
case "$1" in
lr)
    echo "Enabled repositories (global):"
    echo " - adaptation0          ... https://store-repository.example/5.2.0.17/aarch64/"
    echo "Enabled repositories (user):"
    [ -f "$state/url" ] && echo " - shipwright-reef      ... $(cat "$state/url")"
    exit 0 ;;
rr) rm -f "$state/url" ;;
ar) printf '%s' "$3" > "$state/url" ;;
er|ur) ;;
*) echo "ssu stub: unexpected $*" >&2; exit 1 ;;
esac
EOF
cat > "$work/bin/pkcon" <<'EOF'
#!/bin/sh
echo "pkcon $*" >> "$REEF_SMOKE_STATE/log"
EOF
chmod +x "$work/bin/ssu" "$work/bin/pkcon"

# ---- runs ------------------------------------------------------------------

run() { # mode registered-url
    mode=$1
    state="$work/state-$mode"
    home="$work/home-$mode"
    mkdir -p "$state" "$home"
    chmod 700 "$state"
    printf '%s' "$2" > "$state/url"
    sed -e "s|@UI_DIR@|$ui_dir|g" -e "s|@MODE@|$mode|g" -e "s|@ARCH@|$arch|g" \
        -e "s|@PINNED_URL@|$pinned_url|g" -e "s|@DRIFTED_URL@|$drifted_url|g" \
        "$here/harness/smoke.qml.in" > "$work/smoke-$mode.qml"

    echo "== $mode"
    status=0
    env -i PATH="$work/bin:/usr/bin:/bin" HOME="$home" \
        XDG_CONFIG_HOME="$home/config" XDG_CACHE_HOME="$home/cache" \
        XDG_DATA_HOME="$home/data" XDG_RUNTIME_DIR="$state" \
        LANG=C.UTF-8 QT_QPA_PLATFORM=offscreen QT_FORCE_STDERR_LOGGING=1 QT_LOGGING_RULES="qml.debug=true;js.debug=true;default.debug=true;qt.qml.invalidOverride.warning=false;qt.qml.propertyCache.append.warning=false" \
        QML_IMPORT_PATH="$keel_qml" \
        DBUS_SYSTEM_BUS_ADDRESS="unix:path=/nonexistent/reef-smoke-no-bus" \
        REEF_SYSROOT="$work/root" REEF_URL_TEMPLATE="$template" \
        REEF_SMOKE_STATE="$state" \
        timeout 120 "$bin" --qml "$work/smoke-$mode.qml" > "$state/out" 2>&1 || status=$?
    cat "$state/out"

    [ "$status" -eq 0 ] || { echo "smoke-test: $mode: exit status $status" >&2; exit 1; }
    grep -q '^.*SMOKE PASS' "$state/out" || { echo "smoke-test: $mode: no PASS" >&2; exit 1; }
    # Any QML warning from the UI or the harness is a failure. Warnings
    # inside Keel's own module (qrc:/qt/qml/...) are Keel's to fix: they
    # are counted and shown, not failed on.
    if grep -E '\.qml:[0-9]+' "$state/out" | grep -v 'SMOKE' | grep -v '^qrc:/qt/qml/' ; then
        echo "smoke-test: $mode: QML warnings above" >&2
        exit 1
    fi
    keel_warnings=$(grep -c '^qrc:/qt/qml/' "$state/out" || true)
    echo "smoke-test: $mode: $keel_warnings warning(s) from inside Keel's module (not ours)"
    # Settings were written through from the QML side.
    settings="$home/config/shipwright-reef/settings.json"
    if ! grep -q '"background_check_days": 1' "$settings" ||
        ! grep -q '"licence_server": "https://licences.smoke.invalid"' "$settings"; then
        echo "smoke-test: $mode: settings not saved: $(cat "$settings" 2>&1)" >&2
        exit 1
    fi
    # Buy left one pending claim code, owner-only, beside the licences, and
    # the unanswered redemption kept it.
    claims="$home/data/shipwright-reef/claims"
    codes=$(ls "$claims" 2>/dev/null | grep -E '^[0-9a-f]{32}$' || true)
    if [ "$(echo "$codes" | grep -c .)" -ne 1 ] ||
        [ "$(stat -c %a "$claims/$codes")" != 600 ] ||
        [ "$(stat -c %a "$claims")" != 700 ] ||
        ! grep -q '"app_id":"smoke-bridge"' "$claims/$codes"; then
        echo "smoke-test: $mode: no private pending claim: $(ls -la "$claims" 2>&1)" >&2
        exit 1
    fi
    # The downloaded catalogue was cached for the next start.
    [ -s "$home/cache/shipwright-reef/catalogue.json" ] ||
        { echo "smoke-test: $mode: catalogue not cached" >&2; exit 1; }
}

run pinned "$pinned_url"
run drifted "$drifted_url"

# ---- background check ------------------------------------------------------

check_run() { # mode
    home="$work/home-check-$1"
    state="$work/state-check"
    mkdir -p "$home/config/shipwright-reef" "$state"
    printf '%s' "$pinned_url" > "$state/url"
    env -i PATH="$work/bin:/usr/bin:/bin" HOME="$home" \
        XDG_CONFIG_HOME="$home/config" XDG_CACHE_HOME="$home/cache" \
        XDG_DATA_HOME="$home/data" XDG_RUNTIME_DIR="$state" LANG=C.UTF-8 \
        DBUS_SYSTEM_BUS_ADDRESS="unix:path=/nonexistent/reef-smoke-no-bus" \
        DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent/reef-smoke-no-bus" \
        REEF_SYSROOT="$work/root" REEF_URL_TEMPLATE="$template" \
        REEF_SMOKE_STATE="$state" \
        timeout 60 "$bin" --check-updates > "$state/out-$1" 2>&1
}
echo "== background check"
check_run never-unset   # no settings file: the default (daily) applies
[ -s "$work/home-check-never-unset/cache/shipwright-reef/catalogue.json" ] ||
    { cat "$work/state-check/out-never-unset"; echo "smoke-test: --check-updates cached nothing" >&2; exit 1; }
[ -s "$work/home-check-never-unset/cache/shipwright-reef/last-check" ] ||
    { echo "smoke-test: --check-updates did not record the check" >&2; exit 1; }
mkdir -p "$work/home-check-never/config/shipwright-reef"
printf '{"background_check_days": 0}' > "$work/home-check-never/config/shipwright-reef/settings.json"
check_run never
if [ -e "$work/home-check-never/cache/shipwright-reef/last-check" ]; then
    echo "smoke-test: --check-updates ran although the setting is Never" >&2
    exit 1
fi
grep -q 'not due' "$work/state-check/out-never" ||
    { cat "$work/state-check/out-never"; echo "smoke-test: no 'not due' message" >&2; exit 1; }
echo "smoke-test: background check ok"

# The repair ran the backend's commands, in order, through ssu and pkcon.
expected="ssu rr shipwright-reef
ssu ar shipwright-reef $pinned_url
ssu ur
pkcon -p repo-set-data shipwright-reef refresh-now true"
actual=$(grep -v '^ssu lr$' "$work/state-drifted/log")
if [ "$actual" != "$expected" ]; then
    printf 'smoke-test: repair commands differ.\nexpected:\n%s\nactual:\n%s\n' "$expected" "$actual" >&2
    exit 1
fi
echo "smoke-test: repair ran: $(echo "$actual" | tr '\n' ';')"
# The real main QML through the real binary must show a main window: Keel's
# ApplicationWindow is an Item and needs SailfishApp's view (cpp/launcher.cpp).
echo "== main window"
mkdir -p "$work/home-window" "$work/state-window"
chmod 700 "$work/state-window"
env -i PATH="$work/bin:/usr/bin:/bin" HOME="$work/home-window" \
    XDG_RUNTIME_DIR="$work/state-window" LANG=C.UTF-8 QT_QPA_PLATFORM=offscreen QT_FORCE_STDERR_LOGGING=1 QT_LOGGING_RULES="qml.debug=true;js.debug=true;default.debug=true;qt.qml.invalidOverride.warning=false;qt.qml.propertyCache.append.warning=false" \
    QML_IMPORT_PATH="$keel_qml" \
    DBUS_SYSTEM_BUS_ADDRESS="unix:path=/nonexistent/reef-smoke-no-bus" \
    REEF_SYSROOT="$work/root" REEF_URL_TEMPLATE="$template" \
    "$repo_root/tools/window-probe/window-probe.sh" "$bin" --qml "$repo_root/reef/client/ui/qml/shipwright-reef.qml" ||
    { echo "smoke-test: no main window" >&2; exit 1; }
echo "smoke-test: PASS"
