# SPDX-FileCopyrightText: 2026 Patrick Quinn
# SPDX-License-Identifier: GPL-3.0-only
#
# Shoal Bridge daemon: MagicPodsCore (GPL-3.0) with Shipwright's patches,
# built offline from vendored dependency archives.
#
# Two ways to build, same result:
#
# 1. In the monorepo with mb2 (default, "intree"): run from the repository
#    root, where mb2 sets %%{_builddir} to the repository root:
#        mb2 -t SailfishOS-5.2.0.15-aarch64 \
#            -s shoal/bridge-airpods/rpm/shipwright-shoal-bridge-airpods.spec build
#    mb2 skips %%prep unless given --prepare, so in this mode the copy of
#    daemon/upstream and the patching happen at the start of %%build, into
#    target/rpm/shoal-bridge-airpods/ (never touching the checkout itself).
#
# 2. From tarballs (OBS, distribution builders, and the SRPM that carries the
#    GPL corresponding source): put Source0..Source10 and the patches in
#    %%{_sourcedir} (shoal/bridge-airpods/rpm/make-sources.sh does this), then
#        rpmbuild -ba --without intree shipwright-shoal-bridge-airpods.spec
#
# Either way no network access is needed: CMake uses the archives from
# daemon/vendor (MAGICPODS_OFFLINE=ON makes a missing archive an error).
#
# --without openssl builds the OpenSSL stub (patch 0003): no case-open
# animation, no openssl-devel needed.

%global upstream_commit 173245a14beb40fafef3a276da55747a0c0a1c59
%global daemon_dir shoal/bridge-airpods/daemon
%global unit shoal-bridge-airpods.service
%{!?_userunitdir:%global _userunitdir %{_prefix}/lib/systemd/user}

%bcond_without intree
%bcond_without openssl

%if %{with intree}
%global work %{_builddir}/target/rpm/shoal-bridge-airpods
%global srcdir %{work}/src
%global blddir %{work}/build
%global vendordir %{_builddir}/%{daemon_dir}/vendor
%global patchdir %{_builddir}/%{daemon_dir}/patches
%global unitfile %{_builddir}/%{daemon_dir}/%{unit}
# %%license paths are relative to the build directory (no %%setup here).
%global licrel target/rpm/shoal-bridge-airpods/build/licenses
%else
%global srcdir %{_builddir}/MagicPodsCore-%{upstream_commit}
%global blddir %{srcdir}/_build
%global vendordir %{_sourcedir}
%global unitfile %{SOURCE10}
%global licrel _build/licenses
%endif

Name:           shipwright-shoal-bridge-airpods
Version:        2.0.11
Release:        4
Summary:        Wireless earbuds daemon for Shoal Bridge (MagicPodsCore)
License:        GPL-3.0-only AND LGPL-2.1-or-later AND Apache-2.0 AND MIT
URL:            https://github.com/steam3d/MagicPodsCore
# git -C daemon/upstream archive --prefix=MagicPodsCore-<commit>/ <commit>
Source0:        MagicPodsCore-%{upstream_commit}.tar.gz
# Dependency archives, verified by sha256 in the CMake declarations (patch 0002)
Source1:        sdbus-cpp-1.6.0.tar.gz
Source2:        uWebSockets-20.58.0.tar.gz
Source3:        uSockets-0.8.7.tar.gz
Source4:        nlohmann-json-3.11.3.tar.xz
Source5:        tomlplusplus-3.4.0.tar.gz
Source10:       shoal-bridge-airpods.service
Patch1:         0001-adapter-discovery.patch
Patch2:         0002-offline-vendored-deps.patch
Patch3:         0003-optional-openssl.patch
Patch4:         0004-listen-on-loopback.patch
Patch5:         0005-pulseaudio-startup-timeout.patch
Patch6:         0006-host-device-id.patch
Patch7:         0007-apple-gated-hearing-features.patch
Patch8:         0008-multi-device.patch
Patch9:         0009-ear-detection-pause.patch
Patch10:        0010-build-warnings.patch

BuildRequires:  cmake >= 3.22
BuildRequires:  gcc-c++ >= 10
BuildRequires:  make
BuildRequires:  patch
BuildRequires:  pkgconfig(libsystemd) >= 236
BuildRequires:  pkgconfig(bluez)
BuildRequires:  pkgconfig(libpulse)
BuildRequires:  pkgconfig(zlib)
%if %{with openssl}
BuildRequires:  pkgconfig(libcrypto)
%endif
Requires:       bluez5
# The SailfishOS-MagicPods port installs the same daemon as a user service;
# the unit declares Conflicts=magicpodscore.service so only one holds port 2020.

%description
The Shoal Bridge daemon talks to AirPods, Beats, Galaxy Buds and Pixel Buds
over Bluetooth (battery, noise control, settings) and serves a JSON API on
ws://127.0.0.1:2020 for the Shoal Bridge app. It is MagicPodsCore by
Aleksandr Maslov and Andrei Litvintsev with Shipwright's patches: Bluetooth
adapter discovery (the Jolla Phone has no hci0), offline builds, optional
OpenSSL, loopback-only listening, a PulseAudio startup fix, pausing media
when an earbud is taken out (MPRIS), and the AirPods features that need an
Apple Device ID on the host (hearing aid, transparency customisation, loud
sound reduction, multi-device), which stay unavailable unless
shipwright-shoal-bridge-airpods-vendorid is installed.

Runs as a systemd user service, started with the user session.

The corresponding source is this package's source RPM.

%prep
%if %{without intree}
%setup -q -n MagicPodsCore-%{upstream_commit}
%autopatch -p1
%endif

%build
%if %{with intree}
# mb2 does not run %%prep: take a clean copy of the submodule and patch it here.
test -f %{_builddir}/%{daemon_dir}/upstream/CMakeLists.txt || {
    echo "error: %{daemon_dir}/upstream is empty: git submodule update --init %{daemon_dir}/upstream" >&2
    exit 1
}
rm -rf %{work}
mkdir -p %{srcdir}
# The tracked tree only, as daemon/build.sh does, so untracked or modified
# files in the submodule cannot leak in; a plain copy where git is missing.
if command -v git >/dev/null 2>&1 && git -C %{_builddir}/%{daemon_dir}/upstream rev-parse --git-dir >/dev/null 2>&1; then
    git -C %{_builddir}/%{daemon_dir}/upstream archive --format=tar HEAD | tar -C %{srcdir} -xf -
else
    tar -C %{_builddir}/%{daemon_dir}/upstream --exclude=.git --exclude=./build -cf - . | tar -C %{srcdir} -xf -
fi
for p in %{patchdir}/*.patch; do
    echo "Applying $(basename "$p")"
    patch -d %{srcdir} -p1 -s --no-backup-if-mismatch --fuzz=0 < "$p"
done
(cd %{vendordir} && sha256sum -c SHA256SUMS)
%endif

# CMAKE_BUILD_TYPE=None: use the distribution's %%{optflags} only. Upstream's
# Release flags add -s (strip, which defeats debuginfo) and Debug defines
# DEBUG (runs self-tests at startup).
cmake -S %{srcdir} -B %{blddir} \
    -DCMAKE_BUILD_TYPE=None \
    -DCMAKE_C_FLAGS="%{optflags}" \
    -DCMAKE_CXX_FLAGS="%{optflags}" \
    -DCMAKE_POSITION_INDEPENDENT_CODE=ON \
    -DCMAKE_EXE_LINKER_FLAGS="%{?build_ldflags}%{!?build_ldflags:%{?__global_ldflags}} -pie" \
    -DMAGICPODS_DEPENDENCY_DIR=%{vendordir} \
    -DMAGICPODS_OFFLINE=ON \
    -DFETCHCONTENT_UPDATES_DISCONNECTED=ON \
    -DMAGICPODS_WITH_OPENSSL=%{?with_openssl:ON}%{!?with_openssl:OFF}
cmake --build %{blddir} -- %{?_smp_mflags}

%install
install -D -m 0755 %{blddir}/magicpodscore %{buildroot}%{_libexecdir}/shoal-bridge-airpods/magicpodscore
install -D -m 0644 %{unitfile} %{buildroot}%{_userunitdir}/%{unit}
# Sailfish convention for user services (as bluez5 does for mpris-proxy):
# ship the enablement symlink instead of running `systemctl --user enable`.
mkdir -p %{buildroot}%{_userunitdir}/post-user-session.target.wants
ln -s ../%{unit} %{buildroot}%{_userunitdir}/post-user-session.target.wants/%{unit}

licdir=%{blddir}/licenses
rm -rf $licdir
install -D -m 0644 %{srcdir}/LICENSE $licdir/LICENSE.MagicPodsCore
install -m 0644 %{blddir}/_deps/sdbus-cpp-src/COPYING $licdir/COPYING.sdbus-c++
install -m 0644 %{blddir}/_deps/sdbus-cpp-src/COPYING-LGPL-Exception $licdir/COPYING-LGPL-Exception.sdbus-c++
# uWebSockets and uSockets ship the same Apache-2.0 text.
cmp -s %{blddir}/_deps/uwebsockets_content-src/LICENSE %{blddir}/_deps/usockets_content-src/LICENSE
install -m 0644 %{blddir}/_deps/uwebsockets_content-src/LICENSE $licdir/LICENSE.uWebSockets-uSockets
install -m 0644 %{blddir}/_deps/nlohmann_json-src/LICENSE.MIT $licdir/LICENSE.nlohmann-json
install -m 0644 %{blddir}/_deps/tomlplusplus-src/LICENSE $licdir/LICENSE.tomlplusplus
cat > $licdir/SOURCE <<EOF
MagicPodsCore https://github.com/steam3d/MagicPodsCore
commit %{upstream_commit}, with the patches in the source RPM
%{name}-%{version}-%{release}.src.rpm, which is the complete corresponding
source for this package (GPL-3.0).
EOF

%post
# systemctl-user runs systemctl --user for the device owner (Sailfish).
systemctl-user daemon-reload || :
if [ "$1" -eq 1 ]; then
    systemctl-user start %{unit} || :
else
    systemctl-user try-restart %{unit} || :
fi

%preun
if [ "$1" -eq 0 ]; then
    systemctl-user stop %{unit} || :
fi

%postun
systemctl-user daemon-reload || :

%files
%license %{licrel}/*
%dir %{_libexecdir}/shoal-bridge-airpods
%{_libexecdir}/shoal-bridge-airpods/magicpodscore
%{_userunitdir}/%{unit}
%{_userunitdir}/post-user-session.target.wants/%{unit}

%changelog
* Sat Oct 03 2026 Shipwright - 2.0.11-4
- Patch 0010 fixes every compiler warning in the daemon build (inline enum
  helpers, initialiser order, unused variables, size comparisons) and logs
  a failed socket send.

* Thu Oct 01 2026 Shipwright - 2.0.11-3
- Patch 0004 refuses WebSocket handshakes that carry an Origin header (web
  pages) unless MAGICPODS_ALLOWED_ORIGINS lists it.
- Patch 0007: ATT responses are matched to their request, a timeout closes
  the channel, values are validated per attribute, and writes change only
  the requested fields. Patches 0001, 0003, 0006 and 0008: review fixes.
- Link with the distribution's linker flags (relro, now) as well as -pie.
- Patch 0009: the earDetection capability, and pausing the playing MPRIS
  player when an earbud is taken out and resuming it when it is back
  (setting magicpods.earDetectionPause: off, oneRemoved, bothRemoved).

* Wed Sep 30 2026 Shipwright - 2.0.11-2
- Patches 0006-0008: report whether the adapter presents Apple's Device ID
  (appleDeviceId), and the capabilities gated on it: hearingAid,
  hearingAidAdjustments, transparencyCustom, loudSoundReduction and
  multiDevice, listed as "unavailable: needs Apple Device ID" without it.

* Wed Sep 30 2026 Shipwright - 2.0.11-1
- MagicPodsCore 2.0.11 (173245a) with patches 0001-0005: powered-adapter
  preference and MAGICPODS_ADAPTER, offline vendored dependencies, optional
  OpenSSL, loopback-only listening, PulseAudio startup timeout.
