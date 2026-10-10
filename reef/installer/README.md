# shipwright-reef-installer

The one RPM a user downloads from the Reef website and installs by hand, after enabling "Allow untrusted software". It does five things (docs/plan.md, "Reef: Install flow"):

1. Detects the installed Sailfish OS release from `/etc/sailfish-release`, then `/etc/os-release`, then `version`. It never uses `ssu re`.
2. Registers the repository with `ssu ar shipwright-reef 'https://reefstore.app/sailfishos/<release>/%(arch)/'`, then `ssu ur`.
3. Checks that Reef publishes for that release (`shipwright-reef-repo check`: the repository's `repodata/repomd.xml` is there). When it is not (a 404, as for Sailfish OS 5.1 today), it shows a notification saying so and removes itself, which unregisters the repository; offline, it carries on.
4. Trusts the Reef signing key with `rpm --import`.
5. Installs the Reef client unattended through PackageKit (`pkcon -y install shipwright-reef`). The client obsoletes the installer, so the installer disappears once the client is installed.

| File | Installed as | Role |
| --- | --- | --- |
| `repo.conf` | `/usr/share/shipwright-reef-installer/repo.conf` | **The one place to configure the repository**: alias, URL template, release granularity, client package name, key file, key user id |
| `bin/shipwright-reef-repo` | `/usr/bin/shipwright-reef-repo` | `release`, `url`, `check`, `add` (`ssu rr`, `ssu ar`, `ssu ur`), `remove`, `import-key`, `remove-key`. POSIX sh. The Reef client package should ship it too (see below) |
| `bin/shipwright-reef-installer` | `/usr/bin/shipwright-reef-installer` | The detached post-transaction step: release check, key import, repository refresh, client install |
| `rpm/shipwright-reef-installer.spec` | | The package |
| `tests/test-repo.sh` | | Tests the scripts against stub `ssu`/`rpm`/`version`/`curl` and fixture release files |

The repository URL is `REEF_URL_TEMPLATE` in `repo.conf`, with `{release}` substituted literally and `%(arch)` left for ssu to expand. The production host is `reefstore.app` (the website's host, where the reef-web Worker serves `/sailfishos/`). A build with `REEF_URL_TEMPLATE` set in the environment replaces it (`rpm/prebuild.sh`); [`tools/build/reef/lan-test-repo.sh`](../../tools/build/reef/README.md#lan-test-repository) does this for a repository on the local network.

## What was followed from the Chum GUI installer

Reference: [sailfishos-chum/sailfishos-chum-gui-installer](https://github.com/sailfishos-chum/sailfishos-chum-gui-installer) at commit `a757ca4` (`rpm/sailfishos-chum-gui-installer.spec`, `bin/sailfishos-chum-gui-installer`), and [sailfishos-chum/sailfishos-chum-gui](https://github.com/sailfishos-chum/sailfishos-chum-gui) (`rpm/sailfishos-chum-gui.spec`, `src/ssu.cpp`, `sailfishos-chum-gui.desktop`, `mapplauncherd/privileges.d/`), read on 2026-09-30.

Followed closely:

- **`%post` calls `ssu ar` then `ssu ur`** to register the repository, and `exit 0` so a scriptlet problem never leaves a half-installed package. The log goes to `/var/log/<name>.log`.
- **ssu URL variables.** Chum registers `https://repo.sailfishos.org/obs/sailfishos:/chum/%(releaseMajorMinor)_%(arch)/` on 4.6 and later (`%(release)_%(arch)` before 4.6), escaped as `%%(…)` inside the spec. We keep `%(arch)` exactly as Chum does. We do *not* use `%(release)` or `%(releaseMajorMinor)`, because both follow `ssu re`, which the plan forbids. Instead we substitute the literal release, as the Chum GUI does in `Ssu::setRepo` when the user picks a version. Because our URL template lives in `repo.conf` and not in the spec, the spec needs no `%%` escaping.
- **`%posttrans` hands over to a detached script** with `setsid --fork sh -c '(… "$1" "$2")' <name> "$$" "<logfile>"`. PackageKit cannot run a transaction while the RPM transaction that installed us is still open.
- **The detached script** waits for the `%posttrans` PID to exit and syncs with the PackageKit backend. It then refreshes only our repository with `pkcon -pv repo-set-data <alias> refresh-now true`, with the same retry ladder as Chum: retry, then `killall -TERM pkcon`, then INT followed by TERM, then kill, `pkcon quit`, and a restart of `packagekit.service`. Finally it runs `pkcon -pvy install <client>` detached a second time, because installing the client removes the script that is running. Logging goes to both the log file and `systemd-cat`.
- **The client obsoletes the installer.** Chum GUI's spec has `Conflicts:`/`Obsoletes: sailfishos-chum-gui-installer`. The Reef client spec must do the same for `shipwright-reef-installer`.
- **The client runs unsandboxed with the privileged group.** This follows `Sandboxing=Disabled` in the desktop file and `/usr/bin/<app>,r` in `mapplauncherd/privileges.d/`. See `../client/packaging/`.
- **`Requires:`** covers the tools the scriptlets use (ssu, PackageKit, coreutils, util-linux for `setsid`, psmisc for `killall`, sed) plus a `sailfish-version >=` floor.

Where we differ, and why:

- **Key import.** Chum imports no key. We run `rpm --import` in the detached step, after the RPM transaction has ended, because rpm cannot re-enter its database from a scriptlet. The key ships inside the installer RPM rather than being downloaded, so trust comes from the RPM the user chose to install. The detached script imports the key *before* the refresh.
- **Removal.** Chum's installer always runs `ssu rr` in `%postun`. That works for Chum because Chum GUI re-adds its own repository. The Reef client does not, so our `%preun` removes the repository only when `REEF_CLIENT_MARKER` (`/usr/share/shipwright-reef/owns-repository`, shipped by the client) is absent. When the installer is obsoleted by the client, the new client's files are already in place, so the repository and key stay. The step runs in `%preun`, not `%postun`, so `repo.conf` and `shipwright-reef-repo` still exist. The key's `gpg-pubkey` package is erased by a detached step after the transaction, because `rpm -e` cannot run inside one.
- **Release source.** Chum reads `VERSION_ID` from `/etc/os-release` and keeps three fields. We read `/etc/sailfish-release` first and keep all four fields (or two, for `major-minor`).
- **PID wait.** Chum's `ps -eo pid | fgrep -q "$pid"` also matches substrings of other PIDs. We match the whole field.
- **Repair on change.** Before `ar` we run `ssu rr`, as the Chum GUI does when it switches repositories. This makes `add` a safe re-pin.

## Release granularity

Chum publishes Sailfish 5.x repositories per major.minor. `…/chum/5.2_aarch64/repodata/repomd.xml` and `…/5.1_aarch64/` return 200; `…/5.2.0.15_aarch64/` and `…/5.1.0.11_aarch64/` return 404. Older releases (4.6.0.13, 5.0.0.62) use full strings. The plan pins Reef to the full installed release, and that is the default (`REEF_RELEASE_GRANULARITY=full`, giving `…/sailfishos/5.2.0.17/%(arch)/`). Setting it to `major-minor` gives `…/sailfishos/5.2/%(arch)/`, which also suits a Reef mirror of Chum's Qt6 packages. The same choice must be made in `publish-repo.sh --granularity` and in `reef-backend`'s `RepoConfig::granularity`.

## Requirements for the Reef client package

The client spec (`reef/rpm/`, not written yet; it needs the CXX-Qt bridge) must:

- `Obsoletes: shipwright-reef-installer < 1` and `Conflicts: shipwright-reef-installer` (the installer stays below 1.0; rpm warns about an unversioned `Obsoletes`);
- ship `/usr/share/shipwright-reef/owns-repository` (the marker), `/usr/bin/shipwright-reef-repo`, `repo.conf` and the key;
- in `%postun` with `$1 = 0`, remove the repository and detach the key removal, as the installer's `%preun` does;
- re-pin after an OS update. Either add `%triggerin -- sailfish-version` running `shipwright-reef-repo add`, or rely on the client's startup check (`PinState::Drifted`, then repair). Doing both is safest.

## Build and test

```
sh reef/installer/tests/test-repo.sh                 # 12 checks, stub ssu/rpm
shellcheck -s sh reef/installer/bin/* reef/installer/repo.conf reef/installer/tests/test-repo.sh
tools/build/reef/e2e-test.sh                         # builds this spec with rpmbuild and publishes it
```

Under the SDK (tools/build/sdk), `mb2 … -s reef/installer/rpm/shipwright-reef-installer.spec build` runs from the repo root, with `%{_builddir}` set to the repo root. With plain rpmbuild, pass `--define "_builddir <repo root>"`. The signing key is not in git: pass `--define "reef_key_file <path>"`, or place it at `reef/installer/RPM-GPG-KEY-shipwright-reef`. The build fails with a clear message if it is missing. `publish-repo.sh` writes the public key to `<out>/RPM-GPG-KEY-shipwright-reef`.

## Needs device verification

Everything below is Phase 0 probe 7. Nothing here has run on a phone.

1. **Untrusted install of the installer.** Enable Settings > Untrusted software, download the installer RPM with the browser, and install it by tapping it. It should install without a password prompt and without dependency errors.
2. **ssu URL substitution.** After install, `ssu lr | grep shipwright-reef` and `grep -r shipwright-reef /etc/zypp/repos.d/` should show `…/sailfishos/<installed release>/aarch64/`. The release must be literal: run `ssu re 5.9.9.9` in a test, then `ssu ur`, and check that the URL does not change. The arch must be expanded by ssu. Restore with `ssu re <installed>`.
3. **Unattended pkcon install.** Check `/var/log/shipwright-reef-installer.log` for key import, `[step 1/2]` refresh exit 0, `[step 2/2] pkcon -pvy install shipwright-reef`, and exit 0. Then check `rpm -q shipwright-reef` and that `rpm -q shipwright-reef-installer` reports it is not installed (obsoleted). Time it from the tap to the Reef icon appearing (target: under two minutes, docs/plan.md).
4. **`setsid --fork` and `ps -eo pid` under busybox** on 5.2. Both are used in the Chum installer, which is known to work on 4.x. Confirm on the Jolla Phone.
5. **Key import and signature enforcement.** Run `rpm -q gpg-pubkey --qf '%{NAME}-%{VERSION}-%{RELEASE} %{SUMMARY}\n'`; it should list the Reef key. Then install an unsigned package from a test repository with `pkcon install`; it must be refused.
6. **Clean removal before the client exists.** Install the installer with the client unavailable (point `REEF_URL_TEMPLATE` at an empty repository). Then run `pkcon remove shipwright-reef-installer`. Afterwards `ssu lr` must not list `shipwright-reef`, and after about 30 s the Reef `gpg-pubkey` must be gone. Check whether `rpm -e` from the detached step waits for the transaction lock, as expected with rpm 4.14 and later, or fails.
7. **Obsoletes keeps the repository.** After step 3, `ssu lr` must still list `shipwright-reef` and the key must still be present. This depends on the client shipping the marker file.
8. **Installation from PackageKit versus rpm.** Chum's comments note that `%posttrans` may run under packagekitd (libzypp) rather than rpm. Test both paths: tap-to-install (PackageKit) and `devel-su rpm -i` over SSH.
