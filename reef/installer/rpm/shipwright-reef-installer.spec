# SPDX-FileCopyrightText: 2026 Patrick Quinn
# SPDX-License-Identifier: GPL-3.0-or-later
#
# shipwright-reef-installer: the one RPM a user downloads and installs by hand
# (after enabling "Allow untrusted software"). It registers the signed Reef
# repository pinned to the installed Sailfish release, trusts the Reef key and
# installs the Reef client through PackageKit, unattended.
#
# Modelled on sailfishos-chum-gui-installer's rpm/sailfishos-chum-gui-installer.spec
# (github.com/sailfishos-chum/sailfishos-chum-gui-installer, commit a757ca4);
# see reef/installer/README.md for what was followed and what differs.
#
# Build: there is no source tarball. Files are taken from the repository
# checkout, which is %%{_builddir} under tools/build/sdk (mb2 run from the
# repo root). Plain rpmbuild: pass --define "_builddir <repo root>".
# The signing key is not in git; pass --define "reef_key_file <path>" or put
# it at reef/installer/RPM-GPG-KEY-shipwright-reef.

Name:           shipwright-reef-installer
# The client obsoletes installers below 1 (reef/rpm/shipwright-reef.spec):
# raise that bound before this version reaches 1.
Version:        0.1.0
Release:        6
Summary:        Installs the Reef store for Sailfish OS
License:        GPL-3.0-or-later
URL:            https://reefstore.app/
BuildArch:      noarch

# The scriptlets and the detached script call these; all are on a stock
# Sailfish OS image, listed so a broken image fails loudly at install time.
Requires:       ssu
Requires(post): ssu
Requires(preun): ssu
Requires:       PackageKit
Requires(posttrans): PackageKit
Requires:       coreutils
Requires(post): coreutils
Requires(posttrans): coreutils
Requires:       sed
Requires(post): sed
Requires:       grep
# setsid --fork
Requires:       util-linux
Requires(posttrans): util-linux
# killall
Requires:       psmisc
# Reef publishes for 5.1 (Xperias) and 5.2 (Jolla Phone) onwards.
Requires:       sailfish-version >= 5.1.0

%define reef_key_file_default reef/installer/RPM-GPG-KEY-shipwright-reef
%define reef_share %{_datadir}/%{name}

%description
Reef Installer registers the signed Shipwright Reef repository for the
installed Sailfish OS release and CPU architecture, imports the Reef signing
key and installs the Reef store client. It removes itself once the client is
installed. Log: /var/log/shipwright-reef-installer.log

%prep
# Nothing to unpack: files come from the checkout (see header).

%build
# Shell scripts only.

%install
# Files come from the checkout: %%{_builddir} under mb2. RPM 4.20+ (Fedora)
# points %%{_builddir} at a per-package directory below that instead, so
# host builds pass the checkout as shipwright_src (CI, e2e-test.sh).
cd %{?shipwright_src}%{!?shipwright_src:%{_builddir}}
key=%{?reef_key_file}%{!?reef_key_file:%{reef_key_file_default}}
if [ ! -s "$key" ]; then
  echo "Reef signing key not found at $key (pass --define 'reef_key_file <path>')" >&2
  exit 1
fi
install -D -m 0755 reef/installer/bin/shipwright-reef-installer %{buildroot}%{_bindir}/shipwright-reef-installer
install -D -m 0755 reef/installer/bin/shipwright-reef-repo %{buildroot}%{_bindir}/shipwright-reef-repo
install -D -m 0644 reef/installer/repo.conf %{buildroot}%{reef_share}/repo.conf
install -D -m 0644 "$key" %{buildroot}%{reef_share}/RPM-GPG-KEY-shipwright-reef

%post
# Runs on install and upgrade. Registers (or re-pins) the repository.
# Only ssu here: rpm --import and pkcon must wait until this RPM transaction
# ends, which is what the detached script in %%posttrans is for.
. %{reef_share}/repo.conf
if [ ! -e "$REEF_LOG" ]; then
  umask 022
  mkdir -p "$(dirname "$REEF_LOG")"
  touch "$REEF_LOG"
fi
%{_bindir}/shipwright-reef-repo add >> "$REEF_LOG" 2>&1
# Scriptlet failures would leave a half-installed package; the detached
# script and the log report problems instead (as the Chum installer does).
exit 0

%posttrans
# Last thing in every install or upgrade transaction: hand over to the
# detached script, which waits for this transaction to end, then imports the
# key, refreshes the repository and installs the client via PackageKit.
. %{reef_share}/repo.conf
umask 022
setsid --fork sh -c '(%{_bindir}/shipwright-reef-installer "$1" "$2")' sh_reef_installer "$$" "$REEF_LOG" >> "$REEF_LOG" 2>&1 </dev/null
exit 0

%preun
# On final removal only ($1 = 0), and only if the Reef client has not taken
# over the repository: the client obsoletes this package, and that removal
# must leave the repository and key in place. %%preun, not %%postun, so
# repo.conf and shipwright-reef-repo still exist.
if [ "$1" = 0 ]; then
  . %{reef_share}/repo.conf
  if [ ! -e "$REEF_CLIENT_MARKER" ]; then
    %{_bindir}/shipwright-reef-repo remove >> "$REEF_LOG" 2>&1
    # The key's gpg-pubkey package cannot be looked up or erased while this
    # transaction holds the RPM database; do both detached once it ends.
    # shipwright-reef-repo is gone by then, so the steps are inline.
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
        all=$(rpm -qa gpg-pubkey --qf "%%{NAME}-%%{VERSION}-%%{RELEASE}\t%%{SUMMARY}\n" 2>&1)
        case "$all" in *error:*) echo "$all"; continue ;; esac
        keys=$(echo "$all" | grep -F "$2" | cut -f 1)
        [ -z "$keys" ] && break
        for k in $keys; do rpm -e "$k" && echo "removed key $k"; done
      done
    ' sh_reef_rmkey "$PPID" "$REEF_KEY_UID" >> "$REEF_LOG" 2>&1 </dev/null
  fi
fi
exit 0

%files
%{_bindir}/shipwright-reef-installer
%{_bindir}/shipwright-reef-repo
%dir %{reef_share}
%{reef_share}/repo.conf
%{reef_share}/RPM-GPG-KEY-shipwright-reef

%changelog
* Fri Oct 09 2026 Shipwright - 0.1.0-6
- The detached steps read from /dev/null: rpm on Sailfish OS cannot open its database with no standard input, so removing the installer left its signing key.

* Fri Oct 09 2026 Shipwright - 0.1.0-5
- Removing the installer before Reef took over also removes the signing key when the removal is part of a long transaction.

* Sun Oct 04 2026 Shipwright <reef@shipwright.example> - 0.1.0-4
- Registers the production repository, https://reefstore.app/sailfishos/.

* Sat Oct 03 2026 Shipwright <reef@shipwright.example> - 0.1.0-3
- Licence: GPL-3.0-or-later (LICENSING.md).

* Thu Oct 01 2026 Shipwright <reef@shipwright.example> - 0.1.0-2
- repo.conf is package data, not configuration; drop Group.
* Wed Sep 30 2026 Shipwright <reef@shipwright.example> - 0.1.0-1
- First installer: pinned repository registration, key import, unattended
  client install via PackageKit, clean removal.
