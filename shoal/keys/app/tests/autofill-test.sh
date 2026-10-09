#!/bin/sh
# SPDX-FileCopyrightText: 2026 Patrick Quinn
# SPDX-License-Identifier: GPL-3.0-or-later
#
# The autofill D-Bus interface (src/autofill.rs) end to end: the real
# binary on a private session bus with a scratch vault (harness/autofill.qml
# answers for the person), called with gdbus as Pacific would.
#
#   shoal/keys/app/tests/autofill-test.sh [path/to/libshipwright_shoal_keys.so]
#
# Needs KEEL_QML (a Keel build's qml/ directory), gdbus and dbus-run-session.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../../../.." && pwd)
bin=${1:-${CARGO_TARGET_DIR:-$repo/target}/debug/libshipwright_shoal_keys.so}
[ -x "$bin" ] || { echo "autofill-test: no binary at $bin" >&2; exit 2; }
: "${KEEL_QML:?set KEEL_QML to the qml directory of a Keel build}"
work=$(mktemp -d "${TMPDIR:-/tmp}/keys-autofill.XXXXXX")
trap 'rm -rf "$work"' EXIT INT TERM
ln -s "$bin" "$work/shipwright-shoal-keys"
mkdir -p "$work/data" "$work/keystore" "$work/home"

cat > "$work/run.sh" <<RUN
#!/bin/sh
# Inside the private bus: the app in the background, then the calls.
env HOME="$work/home" QT_QPA_PLATFORM=offscreen QT_FORCE_STDERR_LOGGING=1 QT_LOGGING_RULES="qml.debug=true;js.debug=true" QML_IMPORT_PATH="$KEEL_QML" \\
    SHOAL_KEYS_DIR="$work/data" SHOAL_KEYS_INSECURE_KEYSTORE="$work/keystore" SHOAL_KEYS_TEST_FAST_KDF=1 \\
    "$work/shipwright-shoal-keys" --qml "$here/harness/autofill.qml" > "$work/app.log" 2>&1 &
app=\$!
i=0
until grep -q "AUTOFILL ready" "$work/app.log"; do
    i=\$((i + 1)); [ \$i -lt 300 ] || { echo "the app did not get ready"; cat "$work/app.log"; kill \$app; exit 1; }
    sleep 0.1
done
call() { gdbus call --session --dest org.shipwright.shoal-keys.Autofill --object-path /org/shipwright/Keys/Autofill \\
    --method org.shipwright.Keys.Autofill1."\$@" 2>&1; }
st=0
check() { # name expected-substring output
    case "\$3" in *"\$2"*) echo "ok: \$1" ;; *) echo "FAIL: \$1: \$3"; st=1 ;; esac
}
out=\$(call Logins "https://github.com"); check "Logins lists the site's login without secrets" octocat "\$out"
case "\$out" in *hunter2*) echo "FAIL: Logins leaked a password"; st=1 ;; esac
out=\$(call Logins "https://evil.example"); check "Logins for another site is empty" "('[]',)" "\$out"
out=\$(call Logins "file:///etc/passwd"); check "Logins refuses a non-web origin" "Error.Invalid" "\$out"
out=\$(call Fill "https://github.com" ""); check "Fill sends the chosen login" hunter2 "\$out"
out=\$(call Fill "https://decline.example" ""); check "Fill declined" "Error.Denied" "\$out"
out=\$(call Save "https://github.com" "octocat" "new-password"); check "Save updates the login" updated "\$out"
out=\$(call Fill "https://github.com" ""); check "the new password is filled" new-password "\$out"
out=\$(call Save "https://shop.example" "me" "pw"); check "Save adds a login" saved "\$out"
kill \$app 2>/dev/null
wait \$app 2>/dev/null
exit \$st
RUN
chmod +x "$work/run.sh"
dbus-run-session -- "$work/run.sh"
