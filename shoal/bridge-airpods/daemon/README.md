# Shoal Bridge daemon (MagicPodsCore, patched)

The AirPods daemon behind Shoal Bridge: [MagicPodsCore](https://github.com/steam3d/MagicPodsCore) by Aleksandr Maslov and Andrei Litvintsev, with our patches applied at build time. It talks to the earbuds over BlueZ (D-Bus) and a raw L2CAP socket (AAP, PSM 0x1001), and serves a JSON API on `ws://127.0.0.1:2020`. It runs as a systemd user service. Plan context: `docs/plan.md`, "Shoal Bridge: AirPods first".

## Licence boundary

- Everything built from this directory is **GPL-3.0** (`GPL-3.0-only`, which is what upstream's `License: GPL-3.0` headers and its LICENSE file mean). Our patches are GPL-3.0-only too, and are offered upstream.
- The binary statically links sdbus-c++ (LGPL-2.1-or-later), uWebSockets and uSockets (Apache-2.0), nlohmann/json and toml++ (MIT). The RPM installs all their licence texts.
- Nothing links the daemon. The UI (`../ui`, `../ui-qt5-fallback`, proprietary) is a separate process that talks to it only over the loopback WebSocket. The volume watcher (`../volume`, proprietary) talks only to BlueZ. Counsel still has to confirm this boundary before commercial launch (plan: **[VERIFY]**).
- Corresponding source is the source RPM (`rpm/shipwright-shoal-bridge-airpods.spec` in tarball mode), which carries the upstream tarball, the patches and the vendored archives. The binary RPM installs a `SOURCE` note next to the licences that points to it. Publish the SRPM in the Reef repository alongside every binary RPM.

## Layout

| Path | What |
| --- | --- |
| `upstream/` | MagicPodsCore, pinned at `173245a14beb40fafef3a276da55747a0c0a1c59` (v2.0.11, 29 Sep 2026). A git submodule in the monorepo; never edited in place. |
| `patches/` | Our changes, `git format-patch` style, applied in order at build time. |
| `vendor/` | Source archives of the five CMake dependencies, `SHA256SUMS`, `fetch.sh`. |
| `build.sh` | Development build on a host: copy, patch, configure offline, build. |
| `shoal-bridge-airpods.service` | systemd user unit. |
| `tests/` | Adapter-selection, loopback and Origin tests against a fake BlueZ (`run-adapter-tests.sh`, with the UI's `Backend.qml` in `tst_ui_link.qml`), the DEBUG self-test runner (`run-self-tests.sh`), the ear detection pause tests against fake MPRIS players (`run-mpris-tests.sh`, `fake_mpris.py`), and a WebSocket probe. |

## Patches

| Patch | What | Upstream |
| --- | --- | --- |
| `0001-adapter-discovery` | Prefer a **powered** Bluetooth adapter, switch to a powered one if the selection is unpowered (when one appears, or when another known adapter is powered on: one match rule follows `Powered` on every adapter), and allow pinning one with `MAGICPODS_ADAPTER=hci1` (or `/org/bluez/hci1`). The selected adapter's properties are read with one `GetAll`, and its state is published under a lock so a removed adapter's values are never published after its removal. Based on the SailfishOS-MagicPods adapter patch by Alzareus (credited in the patch). | Offer. See note below. |
| `0002-offline-vendored-deps` | `MAGICPODS_DEPENDENCY_DIR`: use local archives, verified by SHA256, instead of downloading. `MAGICPODS_OFFLINE=ON`: a missing archive is a configure error, never a silent download. Without these options nothing changes. | Offer. |
| `0003-optional-openssl` | `MAGICPODS_WITH_OPENSSL=OFF` builds a stub `Aes` (no case-open animation, no OpenSSL). | Offer. |
| `0004-listen-on-loopback` | Upstream binds `0.0.0.0:2020` (verified: `/proc/net/tcp` shows `00000000:07E4`), so anyone on the same Wi-Fi could switch Bluetooth off, connect or disconnect headphones or rewrite `config.toml`. Bind `127.0.0.1`; `MAGICPODS_LISTEN_HOST` overrides. Loopback alone does not stop a **web page** in the phone's browser (WebSockets ignore the same-origin policy), so the upgrade handler also refuses (403) any handshake that carries an `Origin` header, which browsers always send and scripts cannot remove; `MAGICPODS_ALLOWED_ORIGINS` lists exceptions for a client that runs in a browser engine. Native clients (QWebSocket, `wsprobe.py`) send no Origin. | Offer; security fix. |
| `0005-pulseaudio-startup-timeout` | `PulseAudioClient::WaitReady()` calls `pa_mainloop_iterate(ml, 1, …)`, which blocks, so its 3 s timeout never fires. With no PulseAudio server the daemon never finishes initialising: the port is bound, but every client hangs. Poll with the remaining time as the limit instead. | Offer; bug fix. |
| `0006-host-device-id` | Track the selected adapter's `org.bluez.Adapter1.Modalias` (BlueZ builds it from `DeviceID` in `main.conf`; read in the same `GetAll` as `Powered`, where its absence means no Device ID, and any D-Bus error is logged) and report whether it is Apple's (`bluetooth:v004C…`): new method `GetHostDevice`, `{"hostdevice":{"appleDeviceId":bool,"modalias":"…"}}`, also in `GetAll` and broadcast on change. A separate object, so `defaultbluetooth` frames are unchanged. `MAGICPODS_HOST_DEVICE_ID` overrides it. | Offer. |
| `0007-apple-gated-hearing-features` | The features AirPods offer only to hosts with Apple's Device ID: `hearingAid`, `hearingAidAdjustments`, `transparencyCustom`, `loudSoundReduction`. Without it, each one the model has is listed as `{"unavailable":"needs Apple Device ID","readonly":true}` and `SetCapabilities` ignores it. Adds a small ATT client on L2CAP PSM 0x1F for the hearing attributes: responses are matched to their request by opcode (and handle, for errors), a request with no answer in 3 s (checked from its deadline) closes the channel as ATT requires, failing everything pending, and the device reconnects it up to three times per connection. Values are validated per attribute (length, flag and header bytes, finite floats), and a write rewrites only the fields in the request, keeping the audiogram, the other ear and any extra bytes. DEBUG self-tests (`TestsAapAppleGated`, 21 cases here, with a fake transport for the channel). Protocol from LibrePods. | Offer. |
| `0008-multi-device` | `multiDevice`: the hosts the AirPods report (packet 0x2E), whether this host has the audio (control command 0x06), and `{"takeOver":true}`. Gated like 0007. Adds the 0x06 command and the 0x2E parser with their two self-tests (23 in all). | Offer. |
| `0009-ear-detection-pause` | Acceptance test 4. The read-only `earDetection` capability (packet 0x06: each bud in the ear, out of it or in the case; left and right once the battery packet says which bud is primary), and pausing media over MPRIS when a bud is taken out: the session-bus player whose `PlaybackStatus` is `Playing` is paused and remembered, and resumed when the buds are back, only if it is still paused and the user has not played or stopped it since. Setting `magicpods.earDetectionPause`: `off`, `oneRemoved` (default, as LibrePods) or `bothRemoved`. The state machine (`EarPauseController`) is apart from D-Bus (`MprisClient`, sdbus-c++ like the rest). DEBUG self-tests `TestsAapEarDetection` (22) and, against fake players on a private bus, `TestsEarPauseMpris` (4). Protocol from LibrePods. | Offer. |
| `0010-build-warnings` | Fixes every compiler warning in the build: enum helpers in headers are `inline` instead of `static` (no unused copy per file), member initialisers follow declaration order, unused variables go, size comparisons use `size_t`, and a failed `send()` on the client socket is logged. In the vendored uSockets only, GCC 12+'s `-Wuse-after-free` is off for one pointer comparison it misreads. | Offer. |

**What 0004 does not cover.** The API still has no authentication: any native process on the phone that can reach 127.0.0.1 (any user's, and a Sailjail app with the Internet permission) can drive it, as before. A per-session token was considered (the daemon writes a 0600 file in `$XDG_RUNTIME_DIR`, the client sends it first) and not done: the UI is QML only, and Qt 6's `XMLHttpRequest` refuses `file://` reads unless `QML_XHR_ALLOW_FILE_READ=1` is set for the process, which keel-shell does not let an app choose; and every other upstream client would need the same change. The Origin check closes the remote case that loopback leaves open (web pages); local apps are the same trust boundary as any other session service.

Patches 0006-0008 are useful only with the opt-in package `shipwright-shoal-bridge-airpods-vendorid` (`../vendorid/README.md`), which sets `DeviceID = bluetooth:004C:0000:0000` for BlueZ. Without it they only add the `hostdevice` object and the `unavailable` entries.

**About 0001.** Upstream added dynamic adapter detection (`DBusBtAdapter`, commit 7f16e3c, 27 Sep 2026) after the Sailfish port was written. It already enumerates `GetManagedObjects`, starts without an adapter and picks one up when BlueZ adds it, which covers most of what the port's patch did and fixes the Jolla Phone's no-`hci0` crash on its own. What upstream still lacks, and 0001 adds, is (a) preferring a powered adapter over the lowest object path and (b) the environment override. The port's patch no longer applies to upstream; 0001 re-implements its policy on the new class.

To change a patch: clone `upstream/` elsewhere, `git am patches/*.patch`, edit, commit, then `git format-patch --no-signature --zero-commit -N 173245a -o patches/` and rename the files back to the names above. The `.license` sidecars carry REUSE data, because a header would break `git am`. Regeneration is byte-reproducible: `git am` of the series followed by `format-patch` gives the same files (checked).

**Current series (regenerated 2026-10-01 after the quality review, `docs/quality/findings/bridge-airpods.md`).** Built by amending each commit in order on a clone at 173245a; 0002 and 0005 were not touched and are byte-identical to the previous series. SHA-256 of each file:

| Patch | SHA-256 | Changed in this regeneration |
| --- | --- | --- |
| `0001-adapter-discovery` | `c1d87017a2db0e91914a7f13e7c66b6e037c40767607ce408f117107beba1b6b` | yes: every adapter's Powered followed (AP-D11), one GetAll, publish lock |
| `0002-offline-vendored-deps` | `99965bd09259936d8506a19ac1400613ad7904c0692a5bde83fdeb67aaf06497` | no (byte-identical) |
| `0003-optional-openssl` | `d9af29421157bc33710654e5d9ecafd6812163427279eff01579095a75600537` | yes: copyright line of the new `AesStub.cpp` (AP-D10) |
| `0004-listen-on-loopback` | `493ec9259186f77d02d73aba63c7a340e76a475562965f4625002011e0646c29` | yes: Origin check (AP-D01) |
| `0005-pulseaudio-startup-timeout` | `2ad627429f719cd1991d6a337e2ff7acfc8222e3750bee80264b315fbd09224e` | no (byte-identical) |
| `0006-host-device-id` | `170e8980e850d1723e11beb52ba8ee657a49612a6a4e3c48250a9ea07b9e4406` | yes: Modalias from GetAll before the re-check, errors logged, copyright (AP-D05, D06, D10) |
| `0007-apple-gated-hearing-features` | `02bde65c2e19837e8f2132ac9ef32de476e10e806753c96d331df08d6166a0fb` | yes: ATT matching, deadline, lifecycle, validation, in-place writes, channel tests (AP-D02, D03, D04, D07, D08, D09, D12) |
| `0008-multi-device` | `21493e870cd86e1bcff5e14dab77c13a29b41fe67f0ed9abc02ed7802605e0d9` | yes: now carries the 0x06 command and 0x2E parser from 0007 (AP-D12) |
| `0009-ear-detection-pause` | `2d0a567a9d112f96ef3edb7ca39fdae1eaea142105b97f4fc878f3ec11706396` | new (2026-10-01, after the review): committed on top of the series above; `format-patch` of the result gives 0001-0008 byte-identical to the files in this table (checked) |
| `0010-build-warnings` | `8101d563d863029ab9923857e1768cb4f53efc0f607ceab7323eb940456a2917` | new (2026-10-03): on top of 0009; 0001-0009 unchanged |

## Vendored dependencies

Versions are the ones upstream pins in `dependencies/*/CMakeLists.txt` at the pinned commit (they match the plan): sdbus-c++ 1.6.0, uWebSockets 20.58.0, uSockets 0.8.7, nlohmann/json 3.11.3, toml++ 3.4.0. The five archives total **1,925,905 bytes (1.9 MB)**; toml++ is 1.3 MB of that.

Integrity model: `fetch.sh` makes each archive with `git archive` from the pinned tag, after checking that the tag resolves to the expected commit id; the commit id is the real anchor, and regeneration is byte-reproducible (checked). nlohmann/json is its static release asset `json.tar.xz`. GitHub's on-the-fly archive downloads (codeload) are neither guaranteed byte-stable nor reachable from every build host (they return 403 here). The SHA256 of each archive is checked twice: by `SHA256SUMS` and by CMake's `URL_HASH` (patch 0002).

```
vendor/fetch.sh --check     # verify the committed archives, no network
vendor/fetch.sh             # re-create any missing archive, then verify
```

## Building

On a development host (needs cmake >= 3.22, g++ >= 10, pkg-config, and development files for libsystemd, bluez, libpulse, zlib and, unless `WITH_OPENSSL=OFF`, OpenSSL):

```
./build.sh [BUILD_DIR]                 # result: BUILD_DIR/build/magicpodscore
NO_NETWORK=1 ./build.sh                # configure and build inside `unshare -n`
WITH_OPENSSL=OFF ./build.sh            # OpenSSL stub
BUILD_TYPE=Debug ./build.sh DIR        # DEBUG build: runs the self-tests at start-up
```

For the RPM, see `../rpm/README.md`.

`build.sh` applies the patches with `--fuzz=0`, as the RPM does.

**Re-proven 2026-10-01 with the regenerated series**, Ubuntu 24.04, gcc 13.3: `NO_NETWORK=1 ./build.sh` (Release) and `BUILD_TYPE=Debug NO_NETWORK=1 ./build.sh` both apply all eight patches with `--fuzz=0` and build inside `unshare -n` with no compiler warnings. **Again with 0009 (nine patches), same day:** both builds apply all nine with `--fuzz=0` and build inside `unshare -n` with no compiler warnings; with that Release build the adapter tests pass 33 of 33 (twice) and `../vendorid/tests/run-daemon-tests.sh` 15 of 15; with the Debug build the self-tests pass 45 of 45 and `tests/run-mpris-tests.sh` 11 of 11, 50 runs in a row. The RPM with 0009 (release 3, `Patch9`) has not been built in the SDK. Not yet rebuilt in the Sailfish SDK (see "Needs device verification", item 13).

**Proven on Ubuntu 24.04 with gcc 13.3 (previous series):** all eight patches apply to the pinned commit, and configure plus build succeed inside `unshare -n` (no network interface at all), with and without OpenSSL, with no compiler warnings. The binary links sdbus-c++ statically (as a subproject it defaults to a static library); the OpenSSL-less build does not link libcrypto. `rpmbuild` of the spec succeeds offline in both in-tree and tarball mode (see `../rpm/README.md`).

**Release 2 (patches 0001-0008) in the Sailfish SDK:** `mb2 --snapshot=airpods` from a scratch copy builds `shipwright-shoal-bridge-airpods-2.0.11-2.aarch64.rpm` (621 KB); all eight patches apply, no compiler warnings in the new or changed files (with the distribution `-Wall` flags), and rpmlint gives 0 errors and the same 4 warnings as release 1. The tarball mode (`make-sources.sh`, `rpmbuild -ba --without intree` inside `unshare -n`) also builds release 2 and its SRPM carries all eight patches.

**Release 1 also built in the Sailfish SDK:** `mb2` with target SailfishOS-5.2.0.15-aarch64 (gcc 13.4, systemd 238, PulseAudio 17, OpenSSL 3.5.7) builds the RPM, and rpmlint reports 0 errors. The aarch64 binary has not run on a phone yet.

### Compiler and toolchain requirements

MagicPodsCore is C++20. What it actually uses: abbreviated function templates (`void f(auto *ws)`, GCC 10), `std::map::contains`/`std::set::contains` (libstdc++ 9), `operator<=>` on `std::string` (libstdc++ 10), and designated initialisers. It does not use `<format>`, ranges, coroutines, concepts or `std::jthread`. So the floor is **GCC 10** and **CMake 3.22**.

- Sailfish 5.2: gcc 13.4 and CMake 3.31.8 in the SDK target (confirmed by the lead in the 5.2.0.15 SDK, and by the port on-device: GCC 13.4.0, CMake 3.31.8). No risk.
- Sailfish 5.1 (Xperia CI target): **[VERIFY]** its gcc and cmake versions. Anything below GCC 10 or CMake 3.22 cannot build the daemon without backporting.
- Sailfish 4.x (GCC 8 era): cannot build it.
- The in-target systemd is 238, above sdbus-c++'s floor of libsystemd 236.

## Running

The RPM installs `/usr/libexec/shoal-bridge-airpods/magicpodscore` and the user unit, and enables it with a symlink in `post-user-session.target.wants`, the Sailfish convention that the system bluez5 package uses for `mpris-proxy`. `%post` starts it with `systemctl-user`.

| Variable | Effect |
| --- | --- |
| `MAGICPODS_ADAPTER` | Pin the adapter: `hci1` or `/org/bluez/hci1`. Until it exists the daemon runs without an adapter. (0001) |
| `MAGICPODS_LISTEN_HOST` | Listen address, default `127.0.0.1`. (0004) |
| `MAGICPODS_ALLOWED_ORIGINS` | Comma-separated origins (`scheme://host[:port]`) allowed to open the WebSocket; by default a handshake with any `Origin` header is refused. (0004) |
| `MAGICPODS_HOST_DEVICE_ID` | Pretend the adapter reports this modalias, e.g. `bluetooth:v004Cp0000d0000`. For tests; it does not change what the AirPods see. (0006) |
| `XDG_CONFIG_HOME` / `HOME` | Settings: `$XDG_CONFIG_HOME/magicpods/config.toml`, else `~/.config/magicpods/config.toml`. Shared with the SailfishOS-MagicPods port if both were ever installed. |

Set a variable with a drop-in: `systemctl --user edit shoal-bridge-airpods`, add `[Service]` and `Environment=MAGICPODS_ADAPTER=hci1`.

The unit, `shoal-bridge-airpods.service`:

- has **no `After=bluetooth.target`**. That is a system target, absent from the user manager (plan, acceptance test 8). The daemon needs no ordering: it starts without an adapter and picks one up when BlueZ adds it (tested, below).
- is ordered `After=pulseaudio.service` (ordering only). The daemon connects to PulseAudio once, at startup, and only profile switching needs it.
- handles the **port-2020 restart loop**. The daemon binds 2020 exclusively and exits with status 1 if that fails. `Conflicts=magicpodscore.service` stops the port's user service, `ExecStartPre` logs any other `magicpodscore` process by PID, and `StartLimitBurst=5` in `StartLimitIntervalSec=120` (with `RestartSec=10`) makes systemd give up and show `failed` instead of looping forever. Recover with `systemctl --user reset-failed shoal-bridge-airpods && systemctl --user start shoal-bridge-airpods`.

## Tests on the host

`tests/run-adapter-tests.sh BUILD_DIR/build/magicpodscore` starts a private D-Bus bus, `tests/fake_bluez.py` (a fake `org.bluez` with ObjectManager and `Adapter1` objects, Python 3 + PyGObject) and the daemon, then checks its log and its API. It waits for log lines with a deadline rather than sleeping a fixed time, and removes the previous scenario's logs first so a stale line cannot satisfy a wait. **Result (2026-10-01, regenerated series, Release build from `NO_NETWORK=1 ./build.sh`): 33 of 33 checks pass, three runs in a row:**

1. hci0 unpowered plus hci1 powered: selects hci1, never hci0; the API reports the adapter enabled; the `init` handshake arrives; it listens on `127.0.0.1:2020`, with no IPv4 wildcard listener and no IPv6 listener (`/proc/net/tcp` and `/proc/net/tcp6`).
2. Only hci1 (the Jolla Phone case): selects hci1.
3. `MAGICPODS_ADAPTER=hci1` with both powered: ignores hci0, selects hci1.
4. `MAGICPODS_ADAPTER=hci1` with only hci0: starts without an adapter, still serves the API, reports `enabled: false`.
5. No adapter at start, hci1 added 2 s later: picks it up.
6. Only an unpowered hci0, then a powered hci1: switches to hci1.
7. hci1 removed, hci0 added: waits, then picks up hci0.
8. The selected adapter is powered on later: the API follows.
9. hci0 and hci1 both unpowered, hci1 powered on later: switches from hci0 to hci1 (patch 0001 now follows every adapter's `Powered`).
10. Origin check (0004): a handshake with `Origin: http://evil.example` gets 403 and is logged; `Origin: null` (a `file://` page) gets 403; an origin listed in `MAGICPODS_ALLOWED_ORIGINS` connects; a handshake with no Origin connects; and the UI's own `Backend.qml` (QtWebSockets, `tests/tst_ui_link.qml` under `qmltestrunner`) links and reads the adapter state. Without `qmltestrunner` that last check is skipped, or fails when `CI` is set.

The Device ID path (0006) is tested end to end by `../vendorid/tests/run-daemon-tests.sh BUILD_DIR/build/magicpodscore` (fake BlueZ `Modalias`, 15 of 15 pass with the regenerated series).

**Self-tests (0007-0009).** The packet, JSON and ATT channel code is covered by `TestsAapAppleGated`, and the ear detection parser and pause/resume state machine by `TestsAapEarDetection`; both run at start-up of a DEBUG build. `tests/run-self-tests.sh` runs such a binary on a private bus and fails unless every test named in `src/tests/TestsAapAppleGated.h` and `src/tests/TestsAapEarDetection.h` reports PASS (so a test that never ran fails too):

```
BUILD_TYPE=Debug NO_NETWORK=1 ./build.sh /tmp/apdebug
tests/run-self-tests.sh /tmp/apdebug/build/magicpodscore      # 45 of 45 pass (23 + 22)
tests/run-mpris-tests.sh /tmp/apdebug/build/magicpodscore     # 11 of 11 pass
```

`TestsAapEarDetection` (22, with fake media players that deliver status reports the way `MprisClient` does): the 0x06 packet (in the ear, out, in the case, unknown bytes; short, wrong-type and wrong-header packets refused), the primary bud from the battery packet (right or left first, the case first, a single component, truncated), the state names and policy strings; and the state machine: the first state after connecting does nothing; one bud out pauses the playing player and remembers it; back in resumes that same player, and a primary swap in between does nothing; a player the user paused is never paused or resumed; a player the user played again while a bud was out, then paused, is not resumed; the player's report of our own pause, or another player starting, changes nothing; a `Playing` report that reaches us only while we wait for the player's answer to our Pause (so it was sent before the pause) does not cancel the resume; a player that stopped, or quit and came back under another owner, is not resumed; a bud straight into the case pauses, and out of the case into the hand does not yet resume; listening with one bud (the other in the case); both out then one back (one removed: stays paused until both are back; both removed: pauses at the second, resumes at the first back); the both-removed policy; off, and turning it off while holding a paused player; a disconnect forgets the player; a bud whose state becomes unknown (flat battery) does not pause; a single-component headset counts only the primary; only the playing player is paused when a paused one sorts first.

`tests/run-mpris-tests.sh` starts a private bus that serves as both system and session bus, two fake players (`tests/fake_mpris.py`: `music` playing, `app` paused; each its own connection, so its own unique name, and every call logged) and the DEBUG daemon with `MAGICPODS_TEST_MPRIS=1`, which makes it run `TestsEarPauseMpris`: `EarPauseController` with the real `MprisClient` finds the playing player, pauses and resumes it, keeps the resume across the player's own `Paused` signal, forgets it when the user plays it again (from the `PropertiesChanged` signal), and pauses nothing when the user paused first. An earlier version handled the signals on a thread of their own; 1 run in about 40 failed, because a late-handled `Playing` from the previous resume cancelled the next pause. The signals now wait in the connection's queue and are handled in order on the caller's thread (below), and 50 runs in a row pass. The script also checks the players' call log (exactly the expected calls to `music`, none to `app`), that `earDetectionPause` is written to `config.toml` with its default and applied at start, that `SetSetting` over the WebSocket applies a new value at once and `GetSetting` reports it, and that an unknown value falls back to `oneRemoved`. Breaking the "user took over" handling in `EarPauseController` makes one self-test and two of these checks fail (tried).

The 23 cover the Device ID check, the ATT PDUs, parsing and validating each attribute (a 104-byte transparency value is refused as hearing aid and the reverse; NaN refused; loud sound reduction exactly 0 or 1), in-place writes (ears set differently by an iPhone keep their difference and all other bytes when only `tone` or only `amplification` is set, extra trailing bytes kept, `selected` alone changes only the flag), rejected bodies changing nothing, the control commands, and the ATT channel with a fake transport: one request at a time, a Write Response or another request's Error Response not taken as the answer to a read, the deadline alone failing the request in flight and the queued ones and closing the channel, a late answer after that ignored, a clean restart, and `Start()` honouring "no longer wanted" after connecting. CI builds and runs this. (Eleven of upstream's own BLE self-tests, `TestBeatsSolo4` and `TestPrivateAirPods2_2` to `_8` among them, fail at the pinned commit with patches 0001-0005 too, with or without OpenSSL; they are upstream's, not ours, and the runner reports their count without judging them.) The gating state machine (`AapAppleGatedCapability` with a device and `HostDeviceId`) is not unit-tested: it needs an `AapDevice`, which needs a BlueZ device proxy; the UI tests cover the same behaviour against the mock daemon.

`tests/wsprobe.py` is a dependency-free WebSocket client (it can ignore pings). Against the real daemon it measured: pings every ~8 s; **without pongs the TCP connection is dropped at ~15.4 s, with no close frame**; answering pings keeps it open indefinitely (40 s tested).

## WebSocket JSON API (as used by the UI)

Upstream reference: `upstream/api-reference.md`. Below is what the Shoal Bridge UI relies on, checked against `upstream/src/main.cpp` and the running daemon. All frames are JSON text, and the API version is `0`.

### Connection

- Connect to `ws://127.0.0.1:2020/` (any path), **without an `Origin` header** (0004): a handshake that carries one gets `403 Forbidden` unless the origin is listed in `MAGICPODS_ALLOWED_ORIGINS`. QWebSocket sends none.
- The daemon first sends `{"init":{"api":0,"version":"2.0.11"}}`. Send requests only after that.
- A connection made **before the daemon has finished initialising is closed at once**, so clients must reconnect with backoff.
- **Idle timeout 16 s:** the daemon sends ping frames and drops clients that do not answer. QWebSocket, as used by the UI, answers pings automatically. Its own sends do not reset the timer (`resetIdleTimeoutOnSend = false`); only data from the client does.
- Every connection receives the broadcasts below.
- An invalid request, or an unknown method, is answered with an **empty text frame**.

### Requests

| Request | Reply |
| --- | --- |
| `{"method":"GetAll"}` | `{"headphones":[…],"defaultbluetooth":{…},"info":{…},"hostdevice":{…}}` (`hostdevice` from 0006) |
| `{"method":"GetDevices"}` | `{"headphones":[{name,address,vendor,model,color,connected}]}` |
| `{"method":"GetActiveDeviceInfo"}` | `{"info":{name,address,vendor,model,color,connected,capabilities:{…}}}`, or `{"info":{}}` if none |
| `{"method":"GetDefaultBluetoothAdapter"}` | `{"defaultbluetooth":{"enabled":bool}}` (`false` also when there is no adapter) |
| `{"method":"GetHostDevice"}` | `{"hostdevice":{"appleDeviceId":bool,"modalias":"usb:v1D6Bp0246d0548"}}` (0006; `modalias` empty without adapter or Device ID) |
| `{"method":"EnableDefaultBluetoothAdapter"}` / `Disable…` | `defaultbluetooth`, after BlueZ answers |
| `{"method":"ConnectDevice","arguments":{"address":"AA:…"}}` / `DisconnectDevice` | `headphones`, after BlueZ answers |
| `{"method":"SetCapabilities","arguments":{"address":"AA:…","capabilities":{"anc":{"selected":16}}}}` | **nothing** (see below) |
| `{"method":"GetSettings","arguments":{"container":"shoalbridge"}}` | `{"settings":{"shoalbridge":{…}}}` (creates the container if missing) |
| `{"method":"GetSetting","arguments":{"container":c,"setting":s}}` | `{"settings":{c:{s:value}}}` or `{"settings":{}}` |
| `{"method":"SetSetting","arguments":{"container":c,"setting":s,"value":v}}` | nothing; an `OnSettingUpdate` broadcast follows. `v` may be bool, int, float, string, array or object. |

### Broadcasts

| Event | Frame |
| --- | --- |
| Capability of the active device changed | `{"info":{…}}` |
| Active device changed | `{"info":{…}}` |
| Any headphone connected or disconnected | `{"headphones":[…]}` |
| Adapter powered on or off | `{"defaultbluetooth":{"enabled":bool}}` |
| The adapter starts or stops presenting Apple's Device ID (0006) | `{"hostdevice":{…}}` |
| Setting saved | `{"settings":{container:{setting:value}}}` |
| Case opened (BLE; needs OpenSSL) | `onAnimationTriggered` payload (not used by the UI) |

### SetCapabilities semantics

There is no reply. The change is confirmed only by a later `info` broadcast, and **only if the state actually changed**. Silence means "already in that state", "refused by the device" or "invalid value", and the UI treats it as a gentle notice, not an error. Values not in `options`, and capabilities marked `readonly`, are ignored.

### Capabilities used by the UI

| Name | Value |
| --- | --- |
| `battery` | `{single,left,right,case}` each `{battery 0–100, charging, status}`; status 0 not available, 1 disconnected, 2 connected, 3 cached. Show only status >= 2, and status 3 as stale. |
| `anc` | `selected`, and `options` as a bitmask: 1 off, 2 transparency, 4 adaptive, 8 wind, 16 noise cancellation (the daemon's own values, not Apple's). AirPods Pro 2 report 23. |
| `adaptiveAudioNoise` | 0–100 (0 more noise, 100 less) |
| `conversationAwareness`, `personalizedVolume`, `ancOneAirPod`, `volumeSwipe` | bool |
| `pressSpeed` (0 default, 1 slower, 2 slowest), `pressAndHoldDuration` (0/1/2 default/shorter/shortest), `volumeSwipeLength` (0/1/2 default/longer/longest) | enum |
| `toneVolume` | 15–125 (percent) |
| `endCall` | 2 double press, 3 single press |
| `conversationAwarenessSpeaking` | bool, read-only |
| `bluetoothCodec` | `selected` profile id; `options` `[[id, description], …]` (PulseAudio card profiles) |
| `earDetection` | read-only, 0009: `{primary, secondary, left?, right?}`, each `"inEar"`, `"outOfEar"`, `"inCase"` or `"unknown"`. `left`/`right` once the battery packet has said which bud is primary. Listed from the first ear detection packet after connecting. |

### Ear detection pause (0009)

When a worn bud is taken out (into the hand or the case), the daemon pauses the MPRIS player (`org.mpris.MediaPlayer2.*` on the session bus) whose `PlaybackStatus` is `Playing`, the first by name if several are, and remembers it. When as many buds are back in the ears as before (one, with `bothRemoved`), it calls `Play` on that same player (by its unique bus name), only if it is still `Paused`. It forgets the player, and so never resumes it, when the player reports `Playing` or `Stopped` in the meantime (the user took over), when the headphones disconnect, and when the policy becomes `off`. Nothing happens on the first ear state after connecting. A bud whose state is unknown is not counted.

The policy is the daemon setting `earDetectionPause` in container `magicpods` (`GetSetting`/`SetSetting` as above; written to `config.toml` with its default): `"off"`, `"oneRemoved"` (default; LibrePods' default too) or `"bothRemoved"`. Any other value means `"oneRemoved"` and is logged. A change applies from the next change of the buds on.

Ordering: `MprisClient` has one session-bus connection and no event-loop thread. The players' `PropertiesChanged` signals wait in its queue and are delivered, in order, when the controller asks (on each ear event and before resuming), and by `Pause` and `Play` up to the player's answer. So whatever the player reported before it answered our `Pause` is known to be older than the pause and is ignored, and a takeover is never confused with a stale report.

Limits: the daemon does not check that the audio actually goes to the AirPods before pausing (on a phone it does while they are connected); and the MPRIS calls run on the AAP reader thread with a 1 s timeout per call, so a hung player delays the next AAP packets by at most that much per player.

### Capabilities that need Apple's Device ID (0007, 0008)

Listed only for models that have them and only while the AAP link is up. Without Apple's Device ID: `{"unavailable":"needs Apple Device ID","readonly":true}`, and `SetCapabilities` ignores them. With it: listed once the AirPods report their state (`multiDevice` at once). Unlike the capabilities above, the adjustments take an object with any subset of their fields, and nothing is sent back: the confirmation is the next `info`.

| Name | Value | Models (as LibrePods lists them) | Wire |
| --- | --- | --- | --- |
| `hearingAid` | `{selected: bool, enrolled: bool}`; turning it on without an audiogram (`enrolled: false`) is refused | Pro 2, Pro 2 USB-C, Pro 3 | AAP control 0x2C (enrolled, enabled) and 0x33 |
| `hearingAidAdjustments` | `{amplification, balance, tone: -100..100, ambientNoiseReduction: 0..100, conversationBoost: bool}`; only the fields sent are written, the audiogram and everything else in the attribute are kept | same | ATT 0x2A |
| `transparencyCustom` | `{selected: bool}` plus the same five fields | Pro, Pro 2, Pro 3, AirPods 4 ANC, Max | ATT 0x18 |
| `loudSoundReduction` | `{selected: bool}`; read back after each write (it does not notify) | Pro 2, Pro 3 | ATT 0x1B |
| `multiDevice` | `{hosts: ["AA:…"], ownsConnection: bool}`; set `{takeOver: true}` | all AAP headphones | AAP 0x2E, control 0x06 |

`amplification` and `balance` map to the ears as `amplification ∓ balance/2`; setting one keeps the other as the ears have it, and only the fields in a request are written (every other byte of the attribute, the audiogram included, is kept as read). The ATT client (PSM 0x1F) starts one second after the AAP link is up, only while the host presents Apple's Device ID, and sends one request at a time. A response is accepted only if it answers the request in flight. A request with no answer within 3 s (measured from its own deadline) closes the channel, as ATT requires after a transaction timeout, failing every pending request; the daemon reconnects it, at most three times per AAP connection. It answers the AirPods' own requests and confirms indications.

### Rename

The daemon has no rename API, and it reports BlueZ's `Name`, not `Alias`. The UI keeps display names in the daemon's settings store: container `shoalbridge`, setting `aliases`, a table keyed by the address without separators (`001122334455 = "Work pods"`). It is a display name only; the system Bluetooth alias is unchanged.

## Known quirks

From the port (plan, "Known device quirks"):

- The Jolla Phone (2026) has no `hci0`; the working controller is `hci1`. Upstream v2.0.11 no longer crashes on this; 0001 adds powered-first selection and `MAGICPODS_ADAPTER`.
- Byte 4 of an incoming AAP frame is the message type: `0x04` battery, `0x09` settings, `0x0f` notifications, `0x4b` conversation awareness.
- Noise-control flags are the daemon's universal values (1/2/4/8/16), not Apple's numbering; `options` is a bitmask (AirPods Pro 2: 23).
- Battery status: 0 not available, 1 disconnected, 2 connected, 3 cached; show >= 2 only.
- Mode 1 (off) is refused on current AirPods firmware: an error tone, and the previous mode is kept.
- `SetCapabilities` returns nothing; confirmation is an asynchronous `info` broadcast, and only on change.
- Idle WebSocket connections are closed after 16 s unless the client answers pings.
- Sailfish's `python3` has no `BTPROTO_L2CAP`; prototype in C or Rust.
- Sailfish 5.2 has a complete enough native toolchain to build the daemon on the phone.

Found here:

- Upstream listens on all interfaces (fixed by 0004).
- Upstream hangs at startup without a PulseAudio server (fixed by 0005). It also connects to PulseAudio only once: if PulseAudio was not up when the daemon started, profile switching stays unavailable until the daemon restarts.
- A connection made during initialisation is closed immediately.
- Headphone names come from BlueZ `Name`, not the user-set `Alias`.

## Needs device verification

On a Jolla Phone with Sailfish 5.2 and AirPods Pro 2. Each item is untested until someone runs it:

1. **Adapter.** `busctl --system tree org.bluez` shows `/org/bluez/hci1` and no `hci0`. Then `journalctl --user -u shoal-bridge-airpods | grep "Using Bluetooth adapter"` shows `/org/bluez/hci1 (powered)`.
2. **AAP end to end.** With the AirPods connected, `python3 tests/wsprobe.py --hold 5 '{"method":"GetAll"}'` (Sailfish's python3 can run it; it needs no Bluetooth support) shows `info.capabilities.battery` with left and right at status 2 within 5 s.
3. **Listening mode.** Send `{"method":"SetCapabilities","arguments":{"address":"<addr>","capabilities":{"anc":{"selected":16}}}}` with `wsprobe.py --hold 3`; an `info` broadcast with `anc.selected` 16 arrives and the change is audible.
4. **Loopback bind and Origin check.** On the phone, `cat /proc/net/tcp | grep :07E4` shows `0100007F:07E4`; from another machine on the Wi-Fi, `nc -vz <phone-ip> 2020` is refused. Then open a page in the Sailfish browser that runs `new WebSocket("ws://127.0.0.1:2020/")` (for example a local HTML file): its `onerror` fires, and `journalctl --user -u shoal-bridge-airpods | grep "Refused a WebSocket connection from origin"` shows the page's origin. The app still connects (battery shows).
5. **User service at boot.** Install the RPM, reboot, and without opening the app run `systemctl --user status shoal-bridge-airpods`: active. Confirm `post-user-session.target` exists in the user manager (`systemctl --user list-units --type=target`).
6. **pulseaudio.service ordering.** `systemctl --user list-units | grep -i pulse` shows the unit name the `After=` line assumes; if not, fix the unit.
7. **Restart loop.** Start a stray copy (`/usr/libexec/shoal-bridge-airpods/magicpodscore &`), then `systemctl --user restart shoal-bridge-airpods`: the journal names the stray PID, and after 5 failures the unit is `failed`, not looping. Kill the stray, `reset-failed`, start: active.
8. **Bluetooth toggled.** Turn Bluetooth off and on in Settings; the app's Bluetooth switch follows, and battery returns after reconnecting.
9. **BlueZ restart.** `devel-su systemctl restart bluetooth`: the daemon logs the adapter removed, then `Using Bluetooth adapter: /org/bluez/hci1`, and devices come back. (Only adapter hot-plug was tested on the host, not a bluetoothd restart with devices.)
10. **Suspend and resume** with the AirPods connected: the service stays active, and battery updates resume.
11. **Profile switching** (needs PulseAudio at daemon start): switching to `headset-head-unit` and back changes `pactl list cards short`.
12. **Case-open animation** (OpenSSL build): untested upstream on Sailfish too; best effort.
13. **The SDK-built binary on the phone.** Install `shipwright-shoal-bridge-airpods-2.0.11-2.aarch64.rpm` from mb2 with `devel-su pkcon install-local`, then repeat items 1 to 5 with it.
14. **Apple Device ID features (0006-0008).** See `../vendorid/README.md`, "Needs device verification": the Modalias the adapter reports, the ATT channel connecting next to bluetoothd, and each feature on AirPods Pro 2.
15. **Ear detection pause (0009, acceptance test 4).** Implemented and tested on the host only; the byte values come from LibrePods and were not seen on our hardware.
    - *Packets.* `journalctl --user -u shoal-bridge-airpods -f | grep "Ear detection"` while taking each bud out, putting it back and putting it in the case: the primary and secondary states follow (`inEar`, `outOfEar`, `inCase`), and taking out the primary bud makes the other primary (a second line). `python3 tests/wsprobe.py --hold 3 '{"method":"GetActiveDeviceInfo"}'` shows `earDetection.left`/`right` matching the real buds.
    - *Session bus.* On the first removal the journal shows `Connected to the session bus for media players`, not `Cannot reach media players`. If the user unit lacks `DBUS_SESSION_BUS_ADDRESS` on Sailfish, add it with a drop-in (`Environment=DBUS_SESSION_BUS_ADDRESS=unix:path=%t/dbus/user_bus_socket`, or whatever `systemctl --user show-environment` reports) and record it here and in the unit.
    - *Players.* While music plays from the Jolla Media player (and once from the browser): `busctl --user list | grep mpris` lists the players, and BlueZ's `mpris-proxy` may add one for the AirPods; check it is never chosen over the real player (it should not report `Playing`). Take one bud out: playback pauses within about a second. Put it back: it resumes. Take both out, put one back: still paused; the second: resumes. Take a bud out, press play on the lock screen, then pause it there, put the bud back: stays paused. Pause first, then take a bud out and put it back: stays paused. Put a bud straight into the case: pauses.
    - *Policy.* In the app's Device settings set "Pause media" to "When both earbuds are removed": one bud out keeps playing, both out pause, one back resumes. "Never": nothing pauses. Restart the daemon: the choice is kept.
    - *AirPods' own behaviour.* Whether the AirPods themselves also send an AVRCP pause to a non-Apple host on removal (then the player is already paused when the daemon looks, nothing is remembered and nothing resumes: record it if so).
    - Best effort: AirPods Max (one component: only the primary state should count) and Beats with ear detection.
