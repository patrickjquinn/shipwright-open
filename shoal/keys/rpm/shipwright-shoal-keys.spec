# SPDX-FileCopyrightText: 2026 Patrick Quinn
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Shoal Keys: password manager. Rust core (shoal/keys/core) and CXX-Qt front
# end (shoal/keys/app) on Keel (direct mode, ADR-0016), sandboxed by
# Sailjail with the Secrets, Downloads and ShipwrightLicences permissions only.
#
# SDK convention (tools/build/sdk/README.md): no cargo in the spec. Build the
# app first (shoal/keys/app is its own Cargo workspace; CXX-Qt needs the
# target's Qt 6 tools, see shoal/keys/rpm/cross-build.sh), so it lands at
#     target/aarch64-unknown-linux-gnu/release/libshipwright_shoal_keys.so
# (a shared library that is also executable, installed as %%{_bindir}/%%{app};
# tools/keel-launcher/link_sailfishapp.rs says why), or pass
# --define "keys_bin <path>"; then from the repository root:
#     mb2 -t SailfishOS-5.2.0.15-aarch64 --snapshot=keys \
#         -s shoal/keys/rpm/shipwright-shoal-keys.spec build

%global app shipwright-shoal-keys
%global srcroot shoal/keys
%{!?keys_bin: %global keys_bin target/%{_arch}-unknown-linux-gnu/release/libshipwright_shoal_keys.so}
%{!?keel_actions_dir: %global keel_actions_dir %(dirname %{keys_bin})/keel-actions/org.shipwright.shoal-keys}
%global debug_package %{nil}
%global _build_id_links none

Name:           shipwright-shoal-keys
Version:        0.1.0
Release:        11
Summary:        Passwords and one-time codes
License:        GPL-3.0-or-later
URL:            https://example.invalid/shipwright/shoal-keys

# Runtime: Keel (Sailfish.Silica for Qt 6) and its launcher, Chum Qt 6, and
# the Sailfish Secrets daemon that holds the device key's wrapping key.
Requires:       shipwright-keel-silica
# Keel Actions runtime (Keel.Actions 1.0, ADR-0018).
Requires:       shipwright-keel-actions
# The app starts in its own booster, booster-keel@<app> (shipwright-keel-booster),
# through `invoker --type=keel -A` in its desktop file, in Keel's direct mode;
# keel-shell is the opt-in fallback window path (ADR-0016).
Requires:       shipwright-keel-booster
Recommends:     shipwright-keel-shell
# The window comes from Keel's SailfishApp library (app/cpp/launcher.cpp).
Requires:       shipwright-keel-sailfishapp
Requires:       qt6-qtdeclarative
Requires:       sailfishsecretsdaemon
Requires:       sailjail-permissions

BuildRequires:  desktop-file-utils

%description
Password manager for Sailfish OS on the platform secrets store. The vault is
a standard KeePass KDBX 4 file, locked by your master password and a device
key held in Sailfish Secrets. Entries with user names, passwords, websites,
notes, one-time passwords (TOTP), custom fields, tags and history; search;
password and passphrase generator; import from KeePass, Bitwarden,
1Password, Chrome and Firefox; KDBX export; auto-lock and clipboard
clearing. Works offline; no network access. Free; an optional licence adds
the password health report.

%prep

%build

%install
if [ ! -x %{keys_bin} ]; then
    echo "%{keys_bin} is missing: build shoal/keys/app first (shoal/keys/rpm/cross-build.sh)" >&2
    exit 1
fi
install -D -m 0755 %{keys_bin} %{buildroot}%{_bindir}/%{app}
install -d %{buildroot}%{_datadir}/%{app}/qml
cp -R %{srcroot}/ui/. %{buildroot}%{_datadir}/%{app}/qml/
install -D -m 0644 %{srcroot}/app/packaging/%{app}.desktop \
    %{buildroot}%{_datadir}/applications/%{app}.desktop
# The app's own booster, booster-keel@shipwright-shoal-keys (inside its sandbox),
# starts with the session, as Jolla's camera and email boosters do
# (user-session.target.d/50-jolla-camera.conf); the desktop file reaches it
# with `invoker --type=keel -A`.
mkdir -p %{buildroot}%{_prefix}/lib/systemd/user/user-session.target.d
printf '[Unit]\nWants=booster-keel@shipwright-shoal-keys.service\n' \
    > %{buildroot}%{_prefix}/lib/systemd/user/user-session.target.d/50-shipwright-shoal-keys.conf
desktop-file-validate %{buildroot}%{_datadir}/applications/%{app}.desktop
# Launcher icons, shared style: icons/render.sh at the repository root.
for s in 86 108 128 172; do
    install -D -m 0644 icons/hicolor/${s}x${s}/apps/%{app}.png \
        %{buildroot}%{_datadir}/icons/hicolor/${s}x${s}/apps/%{app}.png
done

# Keel Actions (ADR-0018): written by build.rs next to the binary.
install -D -m 0644 %{keel_actions_dir}/actions.json %{buildroot}%{_datadir}/keel/actions/org.shipwright.shoal-keys.json
install -D -m 0644 %{keel_actions_dir}/org.shipwright.shoal-keys.service %{buildroot}%{_datadir}/dbus-1/services/org.shipwright.shoal-keys.service
# Autofill for other apps (app/src/autofill.rs): activation and the Sailjail
# permission callers list.
install -D -m 0644 %{srcroot}/app/packaging/org.shipwright.shoal-keys.Autofill.service \
    %{buildroot}%{_datadir}/dbus-1/services/org.shipwright.shoal-keys.Autofill.service
install -D -m 0644 %{srcroot}/app/packaging/sailjail/ShoalKeysAutofill.permission \
    %{buildroot}%{_sysconfdir}/sailjail/permissions/ShoalKeysAutofill.permission
install -D -m 0644 %{keel_actions_dir}/org.shipwright.shoal-keys.actions.xml %{buildroot}%{_datadir}/dbus-1/interfaces/org.shipwright.shoal-keys.actions.xml

%post
# The app's booster, started now rather than at the next session: the
# desktop file's `invoker -A` starts nothing without it. On an upgrade
# the restart also closes the app if it is open.
systemctl-user daemon-reload >/dev/null 2>&1 || :
systemctl-user restart booster-keel@shipwright-shoal-keys.service >/dev/null 2>&1 || :

%preun
if [ $1 -eq 0 ]; then
    systemctl-user stop booster-keel@shipwright-shoal-keys.service >/dev/null 2>&1 || :
fi

%postun
systemctl-user daemon-reload >/dev/null 2>&1 || :

%files
%license LICENSES/GPL-3.0-or-later.txt
%{_bindir}/%{app}
%{_datadir}/%{app}
%{_datadir}/applications/%{app}.desktop
%{_prefix}/lib/systemd/user/user-session.target.d/50-shipwright-shoal-keys.conf
%{_datadir}/icons/hicolor/*/apps/%{app}.png
%{_datadir}/keel/actions/org.shipwright.shoal-keys.json
%{_datadir}/dbus-1/services/org.shipwright.shoal-keys.service
%{_datadir}/dbus-1/interfaces/org.shipwright.shoal-keys.actions.xml
%{_datadir}/dbus-1/services/org.shipwright.shoal-keys.Autofill.service
%config %{_sysconfdir}/sailjail/permissions/ShoalKeysAutofill.permission

%changelog
* Fri Oct 09 2026 Shipwright - 0.1.0-11
- Opens right after it is installed. Its launch booster starts with the install, not at the next restart: before, an app installed from Reef did nothing when tapped until the phone was restarted.

* Fri Oct 09 2026 Shipwright - 0.1.0-10
- Starts in its own launch booster inside its sandbox, which keeps Qt 6 and Silica ready: the app opens in about half the time (on the Jolla Phone, about 300 ms instead of 600 ms to the first frame).
- Its QML is compiled when the app is built, part of it to native code, so the phone never compiles it.

* Thu Oct 08 2026 Shipwright - 0.1.0-9
- Passphrases wrap instead of running off the screen. Readable dates and plainer wording.

* Mon Oct 05 2026 Shipwright - 0.1.0-8
- A shorter first-vault introduction.

* Sun Oct 04 2026 Shipwright - 0.1.0-7
- Autofill for other apps (Pacific): org.shipwright.Keys.Autofill1, every
  login sent only after the person picks it in Keys; the
  ShoalKeysAutofill Sailjail permission for callers.

* Sun Oct 04 2026 Shipwright - 0.1.0-6
- Licence keys: trusts the licence public keys given at build time in
  SHIPWRIGHT_LICENCE_KEYS; a release build refuses staging and test keys.

* Sat Oct 03 2026 Shipwright - 0.1.0-5
- Licence: GPL-3.0-or-later (LICENSING.md).

* Fri Oct 02 2026 Shipwright - 0.1.0-4
- Keel Actions (ADR-0018): search and copy entries, lock; actions.json
  and the D-Bus activation and interface files.
* Thu Oct 01 2026 Shipwright - 0.1.0-3
- Show the main window: start through Keel's SailfishApp view (it was loaded
  without one, so only the cover appeared); Requires shipwright-keel-sailfishapp.
* Thu Oct 01 2026 Shipwright - 0.1.0-2
- Licence file; validate the desktop file; Qt6 libraries come from
  automatic dependencies.
* Wed Sep 30 2026 Shipwright - 0.1.0-1
- First package.
