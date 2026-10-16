# SPDX-FileCopyrightText: 2026 Patrick Quinn
# SPDX-License-Identifier: GPL-3.0-or-later
#
# shipwright-reef: the Reef store client (Qt6 + Keel UI, Rust/CXX-Qt bridge
# over reef-backend), plus what it needs to own the Reef repository once the
# installer (reef/installer) has been obsoleted by it.
#
# Build (tools/build/sdk/README.md, "Spec convention for Rust packages"):
# under mb2, %%{_builddir} is the repository root and the binaries are
# prebuilt by tools/build/sdk/cargo-sailfish.sh: shipwright-reef from
# reef/client/app (its own Cargo workspace, so build it there; the app is a
# shared library that is also executable, libshipwright_reef.so, see
# tools/keel-launcher/link_sailfishapp.rs), and
# shipwright-reef-licenced from the root workspace
# (`-p reef-backend --bin shipwright-reef-licenced`). This spec never runs
# cargo. With plain
# rpmbuild, pass --define "_builddir <repo root>".
# The signing key is not in git; pass --define "reef_key_file <path>" or put
# it at reef/installer/RPM-GPG-KEY-shipwright-reef, as for the installer.
#
# What this package must do for the installer is in reef/installer/README.md
# ("Requirements for the Reef client package"); each point is marked below.

Name:           shipwright-reef
Version:        0.1.0
Release:        32
Summary:        The app store for Sailfish OS
License:        GPL-3.0-or-later
URL:            https://reefstore.app/

# Runtime: Keel (Silica for Qt6) and Chum's Qt6 (chum:testing, 6.8.4 on 5.2).
# At least the Keel Reef was built and tested against: updating Reef alone
# (its own Update) must bring Keel along, not leave a new Reef on an older
# Keel whose components it does not know (a forum report of Reef not
# starting after an update, 12 October 2026).
Requires:       shipwright-keel-silica >= 0.1.0-29
# Keel Actions runtime (Keel.Actions 1.0, ADR-0018).
Requires:       shipwright-keel-actions >= 0.1.0-33
# Lipstick starts the app as `invoker --type=keel`: through booster-keel
# when it is installed, otherwise directly, in Keel's direct mode either way;
# keel-shell is the opt-in fallback window path (ADR-0016).
Recommends:     shipwright-keel-booster
Recommends:     shipwright-keel-shell
# The window comes from Keel's SailfishApp library (app/cpp/launcher.cpp).
Requires:       shipwright-keel-sailfishapp >= 0.1.0-14
# Qt6 libraries are picked up as automatic library dependencies; the QML
# modules are not, hence qt6-qtdeclarative.
Requires:       qt6-qtdeclarative
# The client talks to these at run time; the scriptlets call them too.
Requires:       ssu
Requires:       PackageKit
Requires:       sailfish-version >= 5.1.0
Requires(postun): ssu
Requires(postun): util-linux
Requires(postun): coreutils
Requires(postun): grep
Requires:       util-linux
Requires:       coreutils
Requires:       grep
Requires:       sed

# Installer README: the client obsoletes the installer (as sailfishos-chum-gui
# does sailfishos-chum-gui-installer), so the installer disappears once the
# client is installed. The repository and key survive because of the marker
# file below. Every installer version: the installer stays below 1.0 (its
# spec says so), and rpm wants Obsoletes versioned.
Obsoletes:      shipwright-reef-installer < 1
Conflicts:      shipwright-reef-installer

BuildRequires:  desktop-file-utils
BuildRequires:  systemd

# Rust binaries, stripped by the release profile.
%global debug_package %{nil}
%global _build_id_links none

%define reef_alias shipwright-reef
%define reef_share %{_datadir}/%{name}
%define reef_conf %{reef_share}/repo.conf
%define reef_log /var/log/%{name}.log
%define reef_state /var/lib/%{name}
%define reef_key_file_default reef/installer/RPM-GPG-KEY-shipwright-reef

%description
Reef is the Shipwright app store. It lists only builds tested on the Sailfish
OS release this phone runs, installs and updates them through PackageKit
from the signed Reef repository, and keeps app licences, verified offline.
It keeps the Reef repository pinned to the installed release.

%prep
# Nothing to unpack: files come from the checkout (see header).

%build
# Prebuilt: target/%%{_arch}-unknown-linux-gnu/release/libshipwright_reef.so
# and shipwright-reef-licenced.

%install
key=%{?reef_key_file}%{!?reef_key_file:%{reef_key_file_default}}
if [ ! -s "$key" ]; then
  echo "Reef signing key not found at $key (pass --define 'reef_key_file <path>')" >&2
  exit 1
fi
bin=target/%{_arch}-unknown-linux-gnu/release/libshipwright_reef.so
if [ ! -x "$bin" ]; then
  echo "$bin is missing: build reef/client/app with tools/build/sdk/cargo-sailfish.sh first" >&2
  exit 1
fi

install -D -m 0755 "$bin" %{buildroot}%{_bindir}/shipwright-reef
# Keel Actions (ADR-0018): written by build.rs next to the binary.
install -D -m 0644 $(dirname "$bin")/keel-actions/org.shipwright.reef/actions.json %{buildroot}%{_datadir}/keel/actions/org.shipwright.reef.json
install -D -m 0644 $(dirname "$bin")/keel-actions/org.shipwright.reef/org.shipwright.reef.service %{buildroot}%{_datadir}/dbus-1/services/org.shipwright.reef.service
# Reef runs unsandboxed (Sandboxing=Disabled: it installs packages), and
# Sailjail then matches only Exec, never ExecDBus, so the generated
# `sailjail -p` activation exits 1 ("Command line does not match
# template"). Start it through booster-keel instead, as Lipstick starts it
# from the app grid: only a launch through the booster gets the
# `privileged` group its privileges.d entry asks for, without which
# PackageKit refuses Reef's repository refresh (system-sources-configure).
sed -i 's|^Exec=/usr/bin/sailjail -p shipwright-reef.desktop |Exec=%{_bindir}/invoker --type=keel |' \
    %{buildroot}%{_datadir}/dbus-1/services/org.shipwright.reef.service
grep -q '^Exec=%{_bindir}/invoker --type=keel %{_bindir}/shipwright-reef --keel-actions$' \
    %{buildroot}%{_datadir}/dbus-1/services/org.shipwright.reef.service
install -D -m 0644 $(dirname "$bin")/keel-actions/org.shipwright.reef/org.shipwright.reef.actions.xml %{buildroot}%{_datadir}/dbus-1/interfaces/org.shipwright.reef.actions.xml

# Licence hand-off (reef/licence/src/handoff.rs, reef/licence/README.md): a
# D-Bus-activated session service that gives a sandboxed paid app its own
# licence token, and the Sailjail permission that lets apps call it.
licenced=target/%{_arch}-unknown-linux-gnu/release/shipwright-reef-licenced
if [ ! -x "$licenced" ]; then
  echo "$licenced is missing: build reef-backend's shipwright-reef-licenced with tools/build/sdk/cargo-sailfish.sh first" >&2
  exit 1
fi
install -D -m 0755 "$licenced" %{buildroot}%{_bindir}/shipwright-reef-licenced
install -D -m 0644 reef/client/packaging/dbus/org.shipwright.Reef.Licences.service \
  %{buildroot}%{_datadir}/dbus-1/services/org.shipwright.Reef.Licences.service
install -D -m 0644 reef/client/packaging/sailjail/ShipwrightLicences.permission \
  %{buildroot}%{_sysconfdir}/sailjail/permissions/ShipwrightLicences.permission

# The QML UI, loaded from disk by the app (DEFAULT_QML in src/lib.rs).
install -d %{buildroot}%{reef_share}/qml
cp -R reef/client/ui/qml/. %{buildroot}%{reef_share}/qml/

install -D -m 0644 reef/client/app/packaging/shipwright-reef.desktop \
  %{buildroot}%{_datadir}/applications/shipwright-reef.desktop
# Launcher icons, shared style: icons/render.sh at the repository root.
for s in 86 108 128 172; do
  install -D -m 0644 icons/hicolor/${s}x${s}/apps/shipwright-reef.png \
    %{buildroot}%{_datadir}/icons/hicolor/${s}x${s}/apps/shipwright-reef.png
done
desktop-file-validate %{buildroot}%{_datadir}/applications/shipwright-reef.desktop
# Background update check (src/check.rs): a 4-hourly user timer, with its
# notification category and the D-Bus name its tap opens Reef with. Not
# enabled here: Reef asks the person at first start and enables or disables
# the timer for them (systemctl --user), so nothing runs in the background
# that they did not agree to.
install -D -m 0644 reef/client/app/packaging/systemd/shipwright-reef-check.service \
  %{buildroot}%{_userunitdir}/shipwright-reef-check.service
install -D -m 0644 reef/client/app/packaging/systemd/shipwright-reef-check.timer \
  %{buildroot}%{_userunitdir}/shipwright-reef-check.timer
install -D -m 0644 reef/client/app/packaging/notifications/x-shipwright.reef.updates.conf \
  %{buildroot}%{_datadir}/lipstick/notificationcategories/x-shipwright.reef.updates.conf
install -D -m 0644 reef/client/app/packaging/dbus/org.shipwright.reef.ui.service \
  %{buildroot}%{_datadir}/dbus-1/services/org.shipwright.reef.ui.service
install -D -m 0644 reef/client/packaging/privileges.d/shipwright-reef \
  %{buildroot}%{_datadir}/mapplauncherd/privileges.d/shipwright-reef

# Installer README: ship shipwright-reef-repo, repo.conf and the key. The
# client's repo.conf is the installer's with the key and log pointed at this
# package's files; everything else (alias, URL template, granularity) stays
# single-sourced in reef/installer/repo.conf. Scriptlets pass REEF_CONF,
# because shipwright-reef-repo defaults to the installer's path.
install -D -m 0755 reef/installer/bin/shipwright-reef-repo %{buildroot}%{_bindir}/shipwright-reef-repo
install -D -m 0644 "$key" %{buildroot}%{reef_share}/RPM-GPG-KEY-shipwright-reef
sed -e 's|^REEF_KEY_FILE=.*|REEF_KEY_FILE=%{reef_share}/RPM-GPG-KEY-shipwright-reef|' \
    -e 's|^REEF_LOG=.*|REEF_LOG=%{reef_log}|' \
    reef/installer/repo.conf > %{buildroot}%{reef_conf}
grep -q '^REEF_KEY_FILE=%{reef_share}/' %{buildroot}%{reef_conf}
grep -q '^REEF_ALIAS=%{reef_alias}$' %{buildroot}%{reef_conf}
grep -q '^REEF_CLIENT_MARKER=%{reef_share}/owns-repository$' %{buildroot}%{reef_conf}

# Installer README: the marker. While it exists, the installer's %%preun
# leaves the repository and key alone when the installer is obsoleted.
cat > %{buildroot}%{reef_share}/owns-repository <<'EOF'
The shipwright-reef package owns the Reef repository registration.
EOF

# Where the scriptlets note that the repository was turned off because
# Reef publishes nothing for the installed release (see %%triggerin).
install -d %{buildroot}%{reef_state}
touch %{buildroot}%{reef_state}/no-repository

%post
# The timer is the person's choice (Reef asks): only tell the session
# about the changed units. An upgrade from Reef 27, which enabled it for
# everyone, leaves it running until the next login at most; the check
# itself then turns it off until the person says yes.
systemctl-user daemon-reload >/dev/null 2>&1 || :
exit 0

%preun
# Final removal: stop the background check and remove the person's own
# enablement of it (systemctl --user enable made a link in their
# ~/.config), while the unit files are still there; stopped in the running
# session too (the timer would otherwise stay loaded, and fire at a
# service that is gone).
if [ "$1" = 0 ]; then
  systemctl-user disable --now shipwright-reef-check.timer >/dev/null 2>&1 || :
  systemctl-user stop shipwright-reef-check.timer shipwright-reef-check.service >/dev/null 2>&1 || :
fi
exit 0

%triggerin -- sailfish-version
# Installer README: re-pin after an OS update. Fires when sailfish-version is
# installed or upgraded (an OS update) and when this package is installed or
# upgraded. Re-pins only when ssu does not list the repository at the URL
# for the now-installed release, so a normal client upgrade (which may be
# running from this very repository) leaves ssu alone. The client's start-up
# check (repoState "drifted", then Repair) is the second line of defence.
# ssu may list %%(arch) literally or expanded; both count as pinned.
#
# A release Reef publishes nothing for yet (the repository's metadata is a
# 404, `shipwright-reef-repo check` exits 4) would leave every refresh and
# OS update failing on Reef's repository: it is pinned to the release all
# the same, then turned off (ssu dr), and the marker records that this
# package turned it off. When a later check finds it published (exit 0),
# it is turned on again; Reef's own refresh does the same once the
# catalogue for the release appears (reef/client/app/src/jobs.rs). Offline
# (exit 5) changes nothing.
umask 022
export REEF_CONF=%{reef_conf}
expected=$(%{_bindir}/shipwright-reef-repo url 2>> %{reef_log})
if [ -n "$expected" ]; then
  expanded=$(printf '%%s' "$expected" | sed 's|%%(arch)|%{_arch}|')
  listed=$(LC_ALL=C LANGUAGE=C ssu lr 2>/dev/null | grep -E '^[[:space:]]*- %{reef_alias}[[:space:]]')
  if ! printf '%%s\n' "$listed" | grep -qF -e "... $expected" -e "... $expanded"; then
    echo "$(date '+%%F %%T') re-pinning %{reef_alias} to $expected" >> %{reef_log}
    %{_bindir}/shipwright-reef-repo add >> %{reef_log} 2>&1
  fi
  %{_bindir}/shipwright-reef-repo check >> %{reef_log} 2>&1
  case $? in
    4)
      echo "$(date '+%%F %%T') no Reef repository for this release yet: turning %{reef_alias} off" >> %{reef_log}
      ssu dr %{reef_alias} >> %{reef_log} 2>&1
      ssu ur >> %{reef_log} 2>&1
      %{_bindir}/shipwright-reef-repo release > %{reef_state}/no-repository 2>> %{reef_log}
      ;;
    0)
      if [ -s %{reef_state}/no-repository ]; then
        echo "$(date '+%%F %%T') the Reef repository for this release is published: turning %{reef_alias} on" >> %{reef_log}
        ssu er %{reef_alias} >> %{reef_log} 2>&1
        ssu ur >> %{reef_log} 2>&1
        : > %{reef_state}/no-repository
      fi
      ;;
  esac
fi
# Scriptlet failures would leave a half-installed package; the log and the
# client's repair banner report problems instead.
exit 0

%postun
# The check's units are gone: tell the session.
systemctl-user daemon-reload >/dev/null 2>&1 || :
# Installer README: on final removal ($1 = 0), unregister the repository and
# detach the key removal, as the installer's %%preun does. repo.conf and
# shipwright-reef-repo are already gone in %%postun, so the steps are inline.
if [ "$1" = 0 ]; then
  umask 022
  ssu rr %{reef_alias} >> %{reef_log} 2>&1
  ssu ur >> %{reef_log} 2>&1
  # The key's gpg-pubkey cannot be looked up or erased while this
  # transaction holds the RPM database; do it once the transaction ends.
  setsid --fork sh -c '
    i=0
    # $1 is the rpm running the transaction (the parent of this scriptlet):
    # wait for it.
    while [ $i -lt 600 ] && kill -0 "$1" 2>/dev/null; do sleep 1; i=$((i + 1)); done
    # Then until the key is gone, for up to a minute: a query made while
    # the database is still busy fails, and is tried again.
    i=0
    while [ $i -lt 30 ]; do
      sleep 2
      i=$((i + 1))
      all=$(LC_ALL=C LANGUAGE=C rpm -qa gpg-pubkey --qf "%%{NAME}-%%{VERSION}-%%{RELEASE}\t%%{SUMMARY}\n" 2>&1)
      case "$all" in *error:*) echo "$all"; continue ;; esac
      keys=$(echo "$all" | grep -F "$2" | cut -f 1)
      [ -z "$keys" ] && break
      for k in $keys; do rpm -e "$k" && echo "removed key $k"; done
    done
    # The log went with the package (%%ghost); this removal wrote it again.
    rm -f "$3"
  ' sh_reef_rmkey "$PPID" 'Shipwright Reef' %{reef_log} >> %{reef_log} 2>&1 </dev/null
fi
exit 0

%files
%{_bindir}/shipwright-reef
%{_datadir}/keel/actions/org.shipwright.reef.json
%{_datadir}/dbus-1/services/org.shipwright.reef.service
%{_datadir}/dbus-1/interfaces/org.shipwright.reef.actions.xml
%{_bindir}/shipwright-reef-licenced
%{_datadir}/dbus-1/services/org.shipwright.Reef.Licences.service
%{_datadir}/dbus-1/services/org.shipwright.reef.ui.service
%{_datadir}/lipstick/notificationcategories/x-shipwright.reef.updates.conf
%config(noreplace) %{_sysconfdir}/sailjail/permissions/ShipwrightLicences.permission
%{_datadir}/icons/hicolor/*/apps/shipwright-reef.png
%{_bindir}/shipwright-reef-repo
%dir %{reef_share}
%{reef_share}/qml
%{reef_conf}
%{reef_share}/RPM-GPG-KEY-shipwright-reef
%{reef_share}/owns-repository
%{_datadir}/applications/shipwright-reef.desktop
%{_datadir}/mapplauncherd/privileges.d/shipwright-reef
%{_userunitdir}/shipwright-reef-check.service
%{_userunitdir}/shipwright-reef-check.timer
%dir %{reef_state}
%ghost %{reef_state}/no-repository
# The logs of Reef's scriptlets and of the installer it replaces (whose
# removal must not take the installer's log while the installer is still
# writing it): owned, so that removing Reef removes them.
%ghost %attr(0644,root,root) %{reef_log}
%ghost %attr(0644,root,root) /var/log/shipwright-reef-installer.log

%changelog
* Fri Oct 16 2026 Shipwright - 0.1.0-32
- Pilot (and other Keel Actions clients) can bring Reef to the front, starting it if it is not running, and open its installed apps (with the updates waiting), licences or settings.
* Wed Oct 14 2026 Shipwright - 0.1.0-31
- Opening Installed (or any page) from the store's first page no longer shows the next featured banner sliding across it during the page change; the same for the screenshot viewer.
- The store's first page keeps where you were when you open an app and come back: its banners collapsed while the page was hidden under the app's page, which scrolled it back up. Banners that appear near the top (updates) now push what is below down instead of moving the page, so the title stays clear of the camera cut-out, and a change to one app updates only its row.

* Wed Oct 14 2026 Shipwright - 0.1.0-30
- When updates turn up while the store's first page is open, the page stays at its top: the page title and the new updates banner no longer slide up under the camera cut-out.
- The updates banner's title takes the lines it needs ("3 updates for libraries and services" was cut off beside Update all).

* Tue Oct 13 2026 Shipwright - 0.1.0-29
- A name too long for its tile, such as with a larger text size, starts at the left and fades out instead of being cut off at both ends.
- Search keeps up with typing: it filters the list instead of rebuilding every list and checking every licence on each key.
- An app's page keeps its name clear of the camera cut-out, and a name that takes several lines at a larger text size grows down instead of up off the top.
- Installed: when Reef cannot find out what is installed, it says why instead of also saying nothing is installed. Licences: the empty-list text stays below the page's header at large text sizes.
- At large text sizes, list rows take the room their lines need instead of running into the next row.
- The cover keeps clear of its refresh button: with many apps open (smaller covers) at a large text size, "Up to date" no longer wraps into the button and the app's name; the number shrinks to the room there is and the name gives way first.
- When Pilot asks Reef for something while Reef is closed, Reef now starts with the rights it has when opened from the app grid, so it can refresh its repository then too (the refresh was refused).
- An app's page lists "Runs scripts as the system administrator when installed or removed" once, with what each part of the app does below it, instead of once per part; separate lines in "Runs in the background" and "Changes to your phone" have a gap between them.

* Mon Oct 12 2026 Shipwright - 0.1.0-28
- Updating Reef brings the Keel it needs along with it.
- Reef asks before every install, update and removal, and shows everything that comes with it: each add-on, library and background service, what the services do and when they start, what changes on the phone, the download size, and for a removal what else goes (apps that would stop working highlighted). Nothing is installed or removed without a tap on that screen, and if what it would do changes before the tap, Reef asks again.
- Update all lists every package it will update, apps and the libraries and services they use, instead of a bare count.
- App pages show the background services an app brings and the changes it makes to the phone, worked out from the packages themselves.
- Assistants asking Reef to install, update or remove an app (Keel Actions) bring up the same screen; nothing happens until you tap it.
- Reef never removes a Jolla Store app or a package installed from another source, even with the same name; with an app it removes only what Reef installed or what only Reef offers (Qt from Chum stays).
- Removing an app never takes a package something staying needs, and when the phone still refuses, the app goes with its parts and the rest stays. Removing, reinstalling and removing again work without a refresh in between.
- Reef's record of what it installed is now private to your account; it was readable and writable by every user of the phone.
- Background update checks start only once you say yes, asked the first time Reef opens; "Never" turns the check off rather than waking the phone to do nothing, and it can be limited to Wi-Fi.
- On a Sailfish OS release Reef has no apps for yet, its source is turned off instead of failing every refresh, and turned back on once there are apps for it. Removing Reef stops its background check.
- Install and update refresh Reef's source and try once more when the phone's copy of its package list is out of date, instead of failing with "not found on medium". The installer no longer stops other installs or system updates when its own refresh fails.
- Checking and repairing the store's repository works on phones not in English.

* Mon Oct 12 2026 Shipwright - 0.1.0-27
- Removing Reef stops its update check at once, not at the next restart.
- Removing Reef removes its logs, /var/log/shipwright-reef.log and the installer's /var/log/shipwright-reef-installer.log. Removing an app that leaves data on the phone (Pilot's voice models) or takes data with it (iCloud accounts, with their contacts and events) first says so.
- Phones not set to English can install and update again: Reef read the store's signature check in English only, so it failed on every other language ("Firma correcta" on a Spanish phone).
- Removing an app removes what came with it: background services and add-ons it installed, when nothing else needs them. Removing AirPods left its service running.

* Sun Oct 11 2026 Shipwright - 0.1.0-26
- A new home: the featured apps as banners, updates waiting at the top with Update all, a row of new and updated apps, and every app from A to Z. Search is in the pull-down menu.
- App pages show what's new in the latest version, with the version history folded away, several screenshots, the price, size and date at a glance, the website and source as links, and licences in words.
- Permissions are the ones the app really asks for, each in words; before, every app said it asked for none.
- Update notifications arrive: Reef now checks every few hours from the moment it is installed (before, the check only started after the phone restarted), names the apps, pops up once per new version, and opens Reef on its updates when tapped.

* Sun Oct 11 2026 Shipwright - 0.1.0-25
- Apps built for a newer processor than your phone has are not offered (Pacific crashed at start on an Xperia 10 II).
- An app's link opens its page on reefstore.app (it pointed at placeholder addresses).

* Sun Oct 11 2026 Shipwright - 0.1.0-24
- Reef checks the store's signed package list itself before installing or updating anything, and refuses one that is altered or older than one it has seen (Sailfish OS does not check it for third-party stores).

* Sun Oct 11 2026 Shipwright - 0.1.0-23
- Removing an app removes its add-ons with it (Pilot and Pilot gesture): Pilot could not be removed while its gesture was installed ("error 13"), and removing it another way left the gesture behind.
- When other software needs a package, Reef says so instead of showing an error number.

* Fri Oct 09 2026 Shipwright - 0.1.0-22
- Tap a screenshot on an app's page to see it full screen; swipe between the screenshots, tap to show or hide the close button, and close with it or the back swipe.

* Fri Oct 09 2026 Shipwright - 0.1.0-21
- On a Sailfish release the apps are built for but not yet tested on a phone, an app's page says so instead of listing the release as tested.

* Fri Oct 09 2026 Shipwright - 0.1.0-20
- The progress bar of an install or update moves steadily forward: downloading is its first half and installing its second, across all the packages, instead of jumping between each step's and each package's own percentages.

* Fri Oct 09 2026 Shipwright - 0.1.0-19
- Removing Reef removes its signing key: the key removal ran with no standard input, and rpm on Sailfish OS cannot open its database then. It now reads from /dev/null.

* Fri Oct 09 2026 Shipwright - 0.1.0-18
- The key removal of 0.1.0-17 waited for the wrong process and gave up on a busy package database: it now waits for the transaction's rpm and tries again.

* Fri Oct 09 2026 Shipwright - 0.1.0-17
- Removing Reef also removes its signing key when the removal is part of a long transaction: the key removal now waits until the package database is free.

* Fri Oct 09 2026 Shipwright - 0.1.0-16
- Its QML is compiled when the app is built, part of it to native code, so the phone never compiles it.

* Thu Oct 08 2026 Shipwright - 0.1.0-15
- An app's Update button names the version it updates to. With an old catalogue
  it named the catalogue's version, which could be older than the one installed.

* Thu Oct 08 2026 Shipwright - 0.1.0-14
- The store list shows Installed or Update on each app, and summaries wrap to two lines.
- Plainer wording for licences and permissions.

* Thu Oct 08 2026 Shipwright - 0.1.0-13
- Reef's actions (search, install) start for Pilot and other assistants:
  D-Bus activation runs Reef directly, since Sailjail rejects ExecDBus for
  an unsandboxed app.

* Mon Oct 05 2026 Shipwright - 0.1.0-12
- Apps installed at the repository's version show as installed; the
  package page offers one action at a time.

* Sun Oct 04 2026 Shipwright <reef@shipwright.example> - 0.1.0-11
- Production hosts: the repository at reefstore.app and the licence service
  at licences.reefstore.app. REEF_LICENCE_SERVER (https) at build time
  replaces the built-in licence server, for staging phone builds.

* Sun Oct 04 2026 Shipwright <reef@shipwright.example> - 0.1.0-10
- Licence keys: trusts the licence public keys given at build time in
  SHIPWRIGHT_LICENCE_KEYS; a release build refuses staging and test keys.

* Sun Oct 04 2026 Shipwright <reef@shipwright.example> - 0.1.0-9
- Licences bought on the web arrive by themselves: Buy puts a claim code in
  the checkout URL, and Reef fetches the licence with it on return.

* Sat Oct 03 2026 Shipwright <reef@shipwright.example> - 0.1.0-8
- Keel grades: the store shows grade A for full Keel coverage.

* Sat Oct 03 2026 Shipwright <reef@shipwright.example> - 0.1.0-7
- Store front: app icons, a search box, a Featured row, "New" marks,
  publisher names and screenshots, from the repository's assets.

* Sat Oct 03 2026 Shipwright <reef@shipwright.example> - 0.1.0-6
- Licence: GPL-3.0-or-later (LICENSING.md).

* Sat Oct 03 2026 Shipwright <reef@shipwright.example> - 0.1.0-5
- Obsoletes the installer with a version bound (below 1), as rpm asks.
* Fri Oct 02 2026 Shipwright <reef@shipwright.example> - 0.1.0-4
- Keel Actions (ADR-0018): search the store, install apps (confirmed).
  Installs actions.json and the D-Bus activation and interface files.
* Thu Oct 01 2026 Shipwright <reef@shipwright.example> - 0.1.0-3
- Show the main window: start through Keel's SailfishApp view (it was loaded
  without one, so only the cover appeared); Requires shipwright-keel-sailfishapp.
* Thu Oct 01 2026 Shipwright <reef@shipwright.example> - 0.1.0-2
- Daily background update check (user timer); validate the desktop file;
  the Sailjail permission is %%config(noreplace); repo.conf is package data,
  not configuration; drop Group and the explicit qt6-qtbase dependency.
* Wed Sep 30 2026 Shipwright <reef@shipwright.example> - 0.1.0-1
- First client package: Qt6/Keel UI over reef-backend, launched through
  keel-shell; obsoletes the installer and takes over the repository.
