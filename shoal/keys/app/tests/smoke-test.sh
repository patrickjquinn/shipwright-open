#!/bin/sh
# SPDX-FileCopyrightText: 2026 Patrick Quinn
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Headless smoke test for shipwright-shoal-keys.
#
#   shoal/keys/app/tests/smoke-test.sh [path/to/libshipwright_shoal_keys.so]
#
# Runs the real binary offscreen against the real UI (shoal/keys/ui) and the
# Keel Sailfish.Silica module, twice over the same scratch directories:
#
#   first   create a vault, add and edit an entry, TOTP, generator, import
#           Chrome CSV and Bitwarden JSON, export KDBX, recovery key,
#           clipboard, settings, licence refusal, every page and the cover,
#           lock, wrong password
#   reopen  the vault, settings and history persisted; unlock, change the
#           master password, unlock with the new one
#
# and then starts the real main QML and checks that a main window appears
# (tools/window-probe; needs CMake and the Qt 6 Gui development files).
#
# The key store is the plain-file development store
# (SHOAL_KEYS_INSECURE_KEYSTORE) and Argon2 runs with test parameters
# (SHOAL_KEYS_TEST_FAST_KDF=1); nothing touches the host's session.
#
# Needs: Qt 6 (QtQuick) and a built Keel QML tree: KEEL_QML=<dir holding
# Sailfish/Silica and Keel> (a keel CMake build's qml/ directory).

set -eu

here=$(cd "$(dirname "$0")" && pwd)
app=$(cd "$here/.." && pwd)
repo_root=$(cd "$app/../../.." && pwd)
ui_dir="$repo_root/shoal/keys/ui"
bin=${1:-${CARGO_TARGET_DIR:-$repo_root/target}/debug/libshipwright_shoal_keys.so}

[ -x "$bin" ] || { echo "smoke-test: no binary at $bin (cargo build first)" >&2; exit 2; }
if [ -z "${KEEL_QML:-}" ] || [ ! -f "$KEEL_QML/Sailfish/Silica/qmldir" ]; then
    echo "smoke-test: set KEEL_QML to the qml/ directory of a Keel build" >&2
    exit 2
fi

work=$(mktemp -d "${TMPDIR:-/tmp}/keys-smoke.XXXXXX")
trap 'rm -rf "$work"' EXIT INT TERM
# The app is a shared library that is also executable; the RPM installs it as
# /usr/bin/shipwright-shoal-keys. Run it under that name: Keel takes the application's
# name from argv[0].
ln -s "$(cd "$(dirname "$bin")" && pwd)/$(basename "$bin")" "$work/shipwright-shoal-keys"
bin=$work/shipwright-shoal-keys
mkdir -p "$work/home" "$work/data" "$work/keystore" "$work/dl" "$work/runtime"
chmod 700 "$work/runtime"

# Exports to import.
cat > "$work/dl/chrome.csv" <<'CSV'
name,url,username,password,note
Mail,https://mail.example.com/,me@example.com,chrome-pw-1,
,https://shop.example.org/login,buyer,chrome-pw-2,"multi
line"
CSV
cat > "$work/dl/firefox.csv" <<'CSV'
"url","username","password","httpRealm","formActionOrigin","guid","timeCreated","timeLastUsed","timePasswordChanged"
"https://ff.example.net","fox","ff-pw",,"https://ff.example.net","{1}","1700000000000","1700000000000","1700000000000"
CSV
cat > "$work/dl/1password.csv" <<'CSV'
Title,Url,Username,Password,OTPAuth,Favorite,Archived,Tags,Notes
Bank,https://bank.example,alice,op-pw,,false,false,,
CSV
cat > "$work/dl/bitwarden.json" <<'JSON'
{"encrypted": false, "folders": [{"id": "f1", "name": "Work"}],
 "items": [{"id": "1", "folderId": "f1", "type": 1, "name": "VPN", "notes": null,
            "login": {"username": "vpnuser", "password": "bw-pw", "totp": null,
                      "uris": [{"uri": "https://vpn.example.com"}]},
            "passwordHistory": [{"lastUsedDate": "2024-01-01T00:00:00Z", "password": "bw-old"}]}]}
JSON
printf 'not an export\n' > "$work/dl/readme.txt"

run() { # mode
    mode=$1
    sed -e "s|@UI_DIR@|$ui_dir|g" -e "s|@MODE@|$mode|g" -e "s|@DOWNLOADS@|$work/dl|g" \
        "$here/harness/smoke.qml.in" > "$work/smoke-$mode.qml"
    echo "== $mode"
    status=0
    env -i PATH="/usr/bin:/bin" HOME="$work/home" XDG_RUNTIME_DIR="$work/runtime" \
        LANG=C.UTF-8 QT_QPA_PLATFORM=offscreen QT_FORCE_STDERR_LOGGING=1 QT_LOGGING_RULES="qml.debug=true;js.debug=true;default.debug=true;qt.qml.invalidOverride.warning=false;qt.qml.propertyCache.append.warning=false" QML_IMPORT_PATH="$KEEL_QML" \
        DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent/keys-smoke-no-bus" \
        DBUS_SYSTEM_BUS_ADDRESS="unix:path=/nonexistent/keys-smoke-no-bus" \
        SHOAL_KEYS_DIR="$work/data" SHOAL_KEYS_DOWNLOADS="$work/dl" \
        SHOAL_KEYS_INSECURE_KEYSTORE="$work/keystore" SHOAL_KEYS_TEST_FAST_KDF=1 \
        timeout 180 "$bin" --qml "$work/smoke-$mode.qml" > "$work/out-$mode" 2>&1 || status=$?
    cat "$work/out-$mode"
    [ "$status" -eq 0 ] || { echo "smoke-test: $mode: exit status $status" >&2; exit 1; }
    grep -q 'SMOKE PASS' "$work/out-$mode" || { echo "smoke-test: $mode: no PASS" >&2; exit 1; }
    # Any QML warning from the UI or the harness is a failure.
    if grep -E '\.qml:[0-9]+' "$work/out-$mode" | grep -v 'SMOKE' ; then
        echo "smoke-test: $mode: QML warnings above" >&2
        exit 1
    fi
}

run first
# The files on disk: KDBX 4 signature, wrap file, 0600 modes.
kdbx="$work/data/vault.kdbx"
[ "$(od -A n -t x1 -N 4 "$kdbx" | tr -d ' ')" = "03d9a29a" ] || { echo "smoke-test: vault.kdbx is not KDBX" >&2; exit 1; }
[ "$(stat -c %a "$kdbx")" = 600 ] || { echo "smoke-test: vault.kdbx mode $(stat -c %a "$kdbx")" >&2; exit 1; }
[ -s "$work/data/vault.wrap" ] || { echo "smoke-test: no vault.wrap" >&2; exit 1; }
[ "$(find "$work/keystore" -mindepth 1 -maxdepth 1 | wc -l)" -eq 1 ] || { echo "smoke-test: expected one wrapping key" >&2; exit 1; }
grep -q '<KeyFile>' "$work/dl/recovery.keyx" || { echo "smoke-test: recovery key file malformed" >&2; exit 1; }
if grep -q 'second-pw' "$kdbx"; then echo "smoke-test: plaintext password in vault file" >&2; exit 1; fi
echo "smoke-test: files on disk ok"

run reopen

# The real main QML through the real binary must show a main window: Keel's
# ApplicationWindow is an Item and needs SailfishApp's view (cpp/launcher.cpp).
echo "== main window"
env -i PATH="/usr/bin:/bin" HOME="$work/home" XDG_RUNTIME_DIR="$work/runtime" \
    LANG=C.UTF-8 QT_QPA_PLATFORM=offscreen QT_FORCE_STDERR_LOGGING=1 QT_LOGGING_RULES="qml.debug=true;js.debug=true;default.debug=true;qt.qml.invalidOverride.warning=false;qt.qml.propertyCache.append.warning=false" QML_IMPORT_PATH="$KEEL_QML" \
    DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent/keys-smoke-no-bus" \
    DBUS_SYSTEM_BUS_ADDRESS="unix:path=/nonexistent/keys-smoke-no-bus" \
    SHOAL_KEYS_DIR="$work/data" SHOAL_KEYS_DOWNLOADS="$work/dl" \
    SHOAL_KEYS_INSECURE_KEYSTORE="$work/keystore" SHOAL_KEYS_TEST_FAST_KDF=1 \
    "$repo_root/tools/window-probe/window-probe.sh" "$bin" --qml "$ui_dir/shipwright-shoal-keys.qml" ||
    { echo "smoke-test: no main window" >&2; exit 1; }
echo "smoke-test: PASS"
