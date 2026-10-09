<!--
SPDX-FileCopyrightText: 2026 Patrick Quinn
SPDX-License-Identifier: GPL-3.0-or-later
-->

# Shoal Keys

A password manager for Sailfish OS on the platform secrets store (docs/plan.md, "Shoal": free plus optional licence). The vault is a standard KeePass KDBX 4 file. It is locked by the master password plus a device key, and that device key is wrapped by a key kept in Sailfish Secrets. The app works offline and has no network permission.

| Path | What |
| --- | --- |
| `core/` | `shoal-keys-core` (root workspace member): vault, TOTP, generator, search, importers, auto-lock policy, key wrapping, the Sailfish Secrets key store (over the shared client crate `shoal-secrets`, `shoal/secrets`) and the licence. No Qt. |
| `app/` | `shipwright-shoal-keys`: the CXX-Qt 0.10 executable and the `ShoalKeys` QML singleton (`import Shipwright.Keys 1.0`). It is its **own Cargo workspace**; the root workspace must exclude `shoal/keys/app`. |
| `ui/` | Keel QML (`import Sailfish.Silica 1.0`): the app window, the pages and the cover. |
| `rpm/` | `shipwright-shoal-keys.spec` (SDK convention) and `cross-build.sh`, which builds the aarch64 binary in the SDK container. |

## Features

- **Entries:** title, user name, password, several URLs, notes, TOTP, custom fields (plain or hidden), tags, group and history (the previous version is kept on every change, up to 10, as in KeePass). The field mapping follows KeePassXC (`core/src/entry.rs`): extra URLs go in `KP2A_URL_n`, and TOTP in `otp` as an `otpauth://` URI. TOTP settings written by KeePass 2.47 (`TimeOtp-*`) and by KeeOtp (`TOTP Seed`) are also read.
- **TOTP:** RFC 6238 with SHA-1, SHA-256 and SHA-512, 6 to 10 digits and any period, from an `otpauth://` URI or a bare base32 secret. It is tested against all 18 RFC 6238 appendix B vectors and the 10 RFC 4226 HOTP vectors. The entry page shows the code with a countdown bar.
- **Generator:** random passwords (length 8 to 64, character classes, look-alike characters excluded on request, at least one character from each chosen class) and passphrases from the embedded EFF large wordlist (7776 words, 12.9 bits each). Randomness is the OS RNG with rejection sampling, so there is no modulo bias. Entropy is shown for generated values. Typed passwords get a conservative strength estimate that penalises runs, keyboard rows, repeats and dictionary words.
- **Search:** the title, user name, URL host, tags, group, notes and field names, ranked. It never searches passwords, TOTP secrets or field values, so typing part of a password never reveals which entry holds it.
- **Import:** KDBX 3.1 and 4.x (with password history), Bitwarden unencrypted JSON (logins, cards, identities, notes, SSH keys, custom fields, folders, password history), 1Password 1PUX (vaults, sections, TOTP, archived items, password history), 1Password 7 and 8 CSV, Chrome-family CSV and Firefox CSV. Exact duplicates are skipped. The UI lists importable files found in `~/Downloads` and reminds the user to delete plaintext exports.
- **Export:** a portable KDBX 4 copy protected by its own password only, without the device key.
- **Auto-lock:** after a set idle time, after a set time in the background, and when the device screen locks (MCE `tklock_mode_ind`). Timing uses `CLOCK_BOOTTIME`, so time spent suspended counts. Locking drops the decrypted vault, which zeroises the keys and protected values, and clears the clipboard if it still holds a copied secret. Locking never waits: if a save, export or import holds the vault for its Argon2 run, the vault reads as locked at once (a lock-free flag) and is dropped as soon as that work ends, after the save completes.
- **Clipboard:** every copy is marked `x-kde-passwordManagerHint: secret` and cleared after 10 s to 2 min (default 30 s), unless the user has copied something else in the meantime.
- **Cover:** shows the lock state only ("No vault", "Locked", "Unlocked") and offers a lock action. It never shows entry names or codes.
- **Recovery:** the device key can be saved as a standard KeePass XML key file (v2.0). With it and the master password, the vault opens in KeePassXC, or it can be restored and re-bound to a new phone (Unlock page, "Restore with recovery key").

## Autofill for other apps

Pacific (the browser) fills sign-in forms from Keys and offers to save new logins, through `org.shipwright.Keys.Autofill1` on the session bus (bus name `org.shipwright.shoal-keys.Autofill`, object `/org/shipwright/Keys/Autofill`; `app/src/autofill.rs`):

- `Logins(origin)`: the titles and user names (never secrets, at most 20) of the entries for a web page's origin, while the vault is open; `...Error.Locked` otherwise.
- `Fill(origin, id)`: Keys comes to the front, asks for the master password if the vault is locked, and shows the logins for the site (`pages/AutofillPage.qml`). Only the one the person taps is sent back (user name and password); going back sends `...Error.Denied`.
- `Save(origin, username, password)`: Keys asks first, then updates the password of the entry with that site and user name, or adds one.

An entry's URL matches a page when both have the same scheme and host, or both are HTTPS on the same registrable domain (public suffix list: `alice.github.io` and `bob.github.io` do not match). Nothing secret leaves Keys without a tap on a page that names the site. Callers need the `ShoalKeysAutofill` Sailjail permission, which this package installs; Keys is D-Bus activated for it. `app/tests/autofill-test.sh` drives the interface end to end on a private bus. **[device verification]**: that Sailjail lets Keys own the extra bus name and activation works through `sailjail -p`.

## Vault format and key hierarchy

```
master password ─┐
                 ├─ KDBX 4 composite key ─ Argon2id ─ vault.kdbx (ChaCha20, HMAC-SHA-256 blocks)
device key ──────┘  (password + key file)
 (32 random bytes)
     ▲
     └─ XChaCha20-Poly1305 unwrap (vault.wrap) ◄─ wrapping key ◄─ Sailfish Secrets
```

- `vault.kdbx` is KDBX 4.1, written by the maintained `keepass` crate (0.14.0, `save_kdbx4`). It uses the settings KeePassXC picks for new databases: ChaCha20 outer cipher, Argon2id KDF (64 MiB, 3 passes, 2 lanes), ChaCha20 for protected fields, and GZip. A KDBX 4.0 file keeps its ciphers and is written back as 4.1. A KDBX 3.1 file is upgraded to these defaults on its first save.
- The device key is an ordinary KeePass key-file component, so the file stays standard KDBX.
- `vault.wrap` is 92 bytes: magic `SKW1`, a 16-byte vault id, a 24-byte nonce, and the XChaCha20-Poly1305 ciphertext of the device key. The magic and vault id are associated data. The vault id names the key-store secret (`shoalkeys-<hex>`).
- Every write is atomic (temporary file, fsync, rename) with mode 0600, in a 0700 directory. The previous file is kept as `vault.kdbx.bak`; changing the password refreshes it.
- Files live in Sailjail's private directories: `~/.local/share/org.shipwright/shoal-keys/` (vault, wrap file, `licence.token`) and `~/.config/org.shipwright/shoal-keys/settings.json`.

### Sailfish Secrets route

Sailfish Secrets (github.com/sailfishos/sailfish-secrets, BSD-3-Clause) ships a Qt 5 C++ client library. Linking that into a Qt 6 app is not an option, so the shared crate `shoal-secrets` (`shoal/secrets/src/sailfish.rs`, used through `core/src/sailfish.rs`) speaks the daemon's D-Bus protocol directly with zbus, as the library itself does:

1. On the session bus, call `org.sailfishos.secrets.daemon.discovery` `/Sailfish/Secrets/Discovery` `peerToPeerAddress`. If that fails, fall back to `$XDG_RUNTIME_DIR/sailfishsecretsd/p2pSocket`.
2. Open a peer-to-peer connection to that address and call `org.sailfishos.secrets` on `/Sailfish/Secrets`. The calls are `createCollection` (the 5-argument form), `setSecret` (4 arguments), `getSecret` and `deleteSecret`.

The signatures are copied from upstream `daemon/SecretsImpl/secrets_p.h`, and the marshalling from `lib/Secrets/serialization.cpp`. Sailjail's `Secrets.permission` (sailjail-permissions) grants exactly this discovery call and the peer socket directory. The key store uses one collection, `shoalkeys`, in `plugin.encryptedstorage.default`, with `DeviceLockKeepUnlocked` and `OwnerOnlyMode`, and it never allows user interaction. The key store is the `KeyStore` trait:

- `MemoryKeyStore` is for tests.
- `SailfishSecretsKeyStore` is the device backend **[device verification]**.
- `PlainFileKeyStore` is for development only, selected with `SHOAL_KEYS_INSECURE_KEYSTORE=<dir>`. It prints a warning and has no protection.

Tests run the real backend against a fake daemon on a socket pair. That checks the D-Bus signatures, argument order and error-code handling, but not the real daemon.

## Threat model

**At rest.** A copy of `vault.kdbx` alone, from a backup or a stolen file, needs both the master password and the device key. The device key exists only wrapped in `vault.wrap`, under a key held by sailfishsecretsd in its encrypted storage. A guessing attack on the file alone is therefore a search over the 256-bit device key, not over the password. On the phone itself, an attacker who can run code as the user and get past Sailjail's owner-only access control can unwrap the device key. They still face Argon2id on the master password: about one guess per second per core at 64 MiB. So the master password remains the real protection against someone holding an unlocked phone. The recovery key file is as sensitive as the device key: with it, the file is back to password-only strength. The UI says to keep it off the phone. Exports are protected by their own password only. Imported plaintext exports stay in Downloads until the user deletes them, and the UI reminds them to.

**In memory.** While unlocked, the decrypted database is in the process's memory. The `keepass` crate holds protected values in zeroise-on-drop buffers. Our entry copies (`EntryData`), keys (`CompositeKey`, device key and wrapping key) and passwords that pass through the bridge are `Zeroize`/`Zeroizing`. Locking drops all of these. Limits we cannot close: QML and JavaScript strings (a revealed password, the fields of the edit dialog, JSON handed to QML) are managed by the Qt runtime and are not zeroised; the Qt clipboard keeps its own copy; and memory is not locked against swap (Sailfish devices normally have no swap, but zram may be configured). Root can read our memory. Whether Sailjail's profile keeps other processes of the same user from reading it (ptrace) is **[device verification]**. Search never touches secret fields. `Debug` output redacts every secret.

**Clipboard.** On Sailfish, any app in the foreground can read the clipboard, so a copied password is exposed to whichever app is pasted into, and to clipboard-reading apps, until it is cleared. We mark copies as secrets for clipboard managers and clear them after the configured delay (default 30 s), when the vault locks, and on auto-lock. We clear only if the clipboard still holds our value. The window is still real. Users who never copy can read the value from the entry page instead. Whether Lipstick's clipboard or keyboard history keeps copies is **[device verification]**.

**Other.** The cover never shows secrets or entry names. No network access is requested, so nothing leaves the phone. The master password cannot be reset; it is only changed while unlocked, after re-checking the current one.

## Licence

Everything above is free, and a lapsed or missing licence never locks anyone out of their data or out of export. The optional licence (sold one-off; a subscription token for Keys is honoured too) unlocks the **password health report** (Settings, then "Password health"). It lists entries with weak passwords (estimated below 36 bits), passwords reused across entries, passwords unchanged for over a year, and logins that have a URL but no TOTP. It shows titles and issue labels only, never the passwords.

Keys follows the shared "Licence policy for Shipwright apps" in `reef/licence/README.md`: any valid token for the app, one-off or subscription; fetched from Reef automatically; paste-in as the fallback; nothing locked beyond this section. The licence is a `reef-licence` v1 token for the app id `shipwright-shoal-keys` (`shoal_keys_core::APP_ID`). It is verified offline with `Licence::verify` against `licence::EMBEDDED_KEYS`, with the default policy: 5 minutes of clock skew, and 7 days of grace for subscriptions. An invalid token is never stored. At start-up, if no fully valid licence is stored, Keys asks Reef for its token over the session bus (`licence::adopt_from_reef`, `org.shipwright.Reef.Licences1.GetLicence`, Sailjail permission `ShipwrightLicences`; design in `reef/licence/README.md`, "Licence hand-off") and installs it if it verifies. A subscription in its grace period counts as not fully valid, so Reef is asked for the renewal while the report keeps working. `licence::adopt` asks nothing while a fully valid licence is stored, so the app can also call it when it returns to the foreground (after a purchase in Reef); the app layer does not do that yet (it needs a call from `app/` when the application becomes active, off the UI thread, since the D-Bus call can take up to 5 s). Without Reef or the permission nothing changes and the user pastes the token in Settings, as before. Either way it is kept as `licence.token` in the app's data directory. Removing the licence in Settings lasts until the next start if Reef still holds one. The tests sign tokens with a throwaway key.

**Licence keys.** `core/build.rs` fills `licence::EMBEDDED_KEYS` at build time from `SHIPWRIGHT_LICENCE_KEYS`, `kid:base64url` entries separated by commas, as `shipwright-licence-service public-key` prints them; it is the same variable and parser (`reef_licence::build`) as the Reef client's. Without it no key is trusted and every token is rejected. A malformed value fails the build, and so does a key id starting with `staging` or `test` in a release build (`--cfg release_build`). Device builds pass the variable through `tools/build/native/native.sh` or `tools/build/sdk/sdk.sh` (tools/build/native/README.md, "Licence keys").

## UI

`ui/shipwright-shoal-keys.qml` holds the window, the lock handling (any lock replaces the whole page stack with the unlock page), the clipboard clear timer, the auto-lock tick and activity tracking. The pages are:

- Unlock (create on first run); the pull-down menu restores with the recovery key (a dialog with a file picker)
- The entry list, grouped by KeePass group, with search, context-menu copy and delete; the push-up menu locks (and opens Password health when licensed)
- Entry detail: tap a value to copy, the eye button shows or hides secrets, TOTP code with a countdown circle, fields, notes; history in the pull-down menu when there is any
- Edit or add (dialog), with TOTP validation, custom fields, and the generator as a picker dialog (`GeneratorDialog`, accept: Use)
- Generator (`GeneratorPage`: tap the result to copy, pull down for a new one; both share `GeneratorForm`)
- Password history
- Import and export: file list from Downloads, export as a dialog (`ExportDialog`), recovery key
- Settings: lock timers, lock with the device, clipboard delay; changing the master password and adding a licence are dialogs
- Password health (with an empty state that says how to get a licence)
- The cover: lock state and entry count; while unlocked, cover actions lock and search (the window's `showSearch()`)

The pages follow Jolla's Sailfish UI rules; `tools/lint/lint.sh style` checks the mechanical ones.

It uses only Keel types. Structured values cross the bridge as JSON strings. Argon2-bound work runs on a worker thread and reports through `finished(op, ok, message)`. Those operations are create, unlock, recover, the save after an edit, import, export and change password. The Reef licence hand-off at start-up and the Downloads scan for importable files (`scanImports()`, answered by the `importCandidates(json)` signal, at most 500 files probed) run on worker threads too; the GUI thread never takes the vault mutex with a blocking lock. Entry titles, groups, notes, file names and generated passwords are shown with `Text.PlainText`; Keel's `PageHeader` has no text format yet, so titles there go through `ui/pages/text.js` `escape`.

## Build and test

Use the shared settings (house rules):

```
export CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_TARGET_DIR=<repo>/target

# core (root workspace)
cargo fmt -p shoal-keys-core
cargo clippy -p shoal-keys-core --all-targets -- -D warnings
cargo clippy -p shoal-keys-core --no-default-features --all-targets -- -D warnings
cargo test -p shoal-keys-core                        # 61 tests

# app (own workspace)
cd shoal/keys/app
export QMAKE=/usr/lib/qt6/bin/qmake
cargo fmt && cargo clippy --all-targets -- -D warnings
cargo test                                           # 8 tests
cargo build
KEEL_QML=<keel build>/qml sh tests/smoke-test.sh     # offscreen, real UI on real Keel
```

The results on the host (Qt 6.4.2, Rust 1.94.1) were: core 57/57, app 6/6, fmt and clippy clean. The smoke test passes. It runs the real binary with `QT_QPA_PLATFORM=offscreen` against the real UI and Keel's `Sailfish.Silica`, using the development key store and test Argon2 parameters. It makes two runs over the same directories:

- **first.** Create a vault. Add, then edit an entry and check its history. Check TOTP, search (including that passwords are never matched), the generator and strength estimates. Check import-candidate detection, then import Chrome CSV and Bitwarden JSON with duplicate handling. Export KDBX and check it is detected as KDBX. Write the recovery key. Copy to the clipboard and clear it. Round-trip the settings, and check that opening Settings changes nothing. Check the licence gate and that a garbage token is refused. Instantiate every page and the cover. Lock, then check the cover state and that nothing is readable, and that a wrong password is refused. After the run the script checks the files on disk: the KDBX signature, mode 0600, the wrap file, one wrapping key, the key-file XML, and no plaintext password in the file.
- **reopen.** The vault, history and settings persist. Unlock, change the master password, and unlock with the new one.

Any QML warning fails the test.

**Unit coverage (core).** The RFC 6238 and 4226 vectors. Every `otpauth` form, plus KeePass 2.47 and KeeOtp fields. The KDBX 4 round trip, including groups, fields, tags, extra URLs, and TOTP normalisation. Opening and re-saving the upstream keepass-rs KeePassXC fixtures (KDBX 4.0 Argon2id/ChaCha20, and one with TOTP). A wrong key, the key-file requirement, history trimming, no history for a no-op save, and delete. Tamper detection on every byte of the wrap file. The store: create, unlock, "file alone is not enough", recovery onto a fresh key store, change password with a refreshed backup, key store down, and destroy. Every importer, and imported history ending up in KDBX history. The generator's uniformity and options, entropy ordering and search ranking. Licence verification (wrong app, untrusted key, grace) and the health report. The Sailfish Secrets wire protocol against a fake daemon.

**Device build.** In the SDK container, from a scratch copy of the repository, run `shoal/keys/rpm/cross-build.sh`, then `mb2 -t SailfishOS-5.2.0.15-aarch64 --snapshot=keys -s shoal/keys/rpm/shipwright-shoal-keys.spec build`. `cross-build.sh` is Keel's `build-native.sh` method applied to this binary. On 2026-09-30 it got through the Rust dependencies, including `shoal-keys-core`, for aarch64. It then failed while compiling cxx-qt-lib's C++, because the shared disk filled up (`No space left on device`), so no aarch64 binary has been built yet.

The spec itself was checked with host `rpmbuild` using the x86_64 debug binary (`--define "keys_bin ..."`). It produced the expected file list: the binary, the QML under `/usr/share/shipwright-shoal-keys/qml` (matching `DEFAULT_QML`), and the desktop file.

## Needs device verification

Nothing here has run on a phone.

1. **Sailfish Secrets from the sandbox.** Install the RPM on a 5.2 device. Open Keys and create a vault. It should succeed, and Settings, About should say "Sailfish Secrets". Check `ls ~/.local/share/org.shipwright/shoal-keys/` for `vault.kdbx` and `vault.wrap`. Close and reopen Keys, then unlock. If create fails with "platform key store", run `journalctl --user -u sailfish-secretsd -b` (or `devel-su journalctl -b | grep -i secrets`). Also run `dbus-send --session --print-reply --dest=org.sailfishos.secrets.daemon.discovery /Sailfish/Secrets/Discovery org.sailfishos.secrets.daemon.discovery.peerToPeerAddress` from inside `sailjail -p shipwright-shoal-keys.desktop /bin/sh`. Things to confirm:
   - The daemon replies synchronously; a `Pending` result code is treated as failure.
   - A missing secret returns error 40 or 41.
   - `setSecret` on an existing name returns 47.
   - `OwnerOnlyMode` identifies the app correctly when it runs under keel-shell. The app id is derived from the process, so check that a second sandboxed app cannot read the `shoalkeys` collection.
2. **Reboot behaviour.** Reboot, then open Keys before and after the first device unlock. With `DeviceLockKeepUnlocked` the collection should be readable only after the first device unlock. Unlock should then work with no extra prompt.
3. **Argon2 time.** Time an unlock of a freshly created vault on a Jolla phone or Xperia 10 (64 MiB, 3 passes, 2 lanes). The target is under about 1.5 s. If it is slower, lower the passes in `KdfParams::default`, not the memory.
4. **Lock with the device.** With "Lock with the device" on, unlock Keys, press power to lock the screen, then unlock the phone. Keys should show the unlock page with "Locked with the device.". This needs MCE's `tklock_mode_ind` signal on the system bus to reach the sandboxed app.
5. **Background lock and suspend.** Set "Lock in the background after" to 30 s. Unlock, go to the home screen, and wait a minute with the screen off. On return Keys should be locked. This checks that `CLOCK_BOOTTIME` (via `/proc/uptime`) counts through suspend, and that keel-shell reports `applicationActive`.
6. **Clipboard.** Copy a password, paste it into Notes (it should work), wait 30 s, and paste again (it should be empty). Check that copying other text after our copy is left alone. Check whether the Sailfish keyboard's clipboard suggestion keeps the value.
7. **Import from Downloads under Sailjail.** Put a Bitwarden JSON export in `~/Downloads`. It should be listed on Import and export. Import it, then export a KDBX to Downloads and open that in KeePassXC on a desktop.
8. **Recovery.** Save the recovery key, copy `vault.kdbx` and the key file to a desktop, and open them in KeePassXC with the master password. On the phone, rename `vault.wrap` to simulate a lost device key (or factory reset and copy `vault.kdbx` back). Unlock should fail, and "Restore with recovery key" with the key file should succeed and write a new `vault.wrap`.
9. **Cover and theme icons.** Check the cover in the app grid: the lock state, and the lock action with `icon-m-device-lock`. The theme has no `icon-cover-lock`, so the monochrome device-lock icon is used. Check the launcher icon `shipwright-shoal-keys` (`icons/hicolor/*/apps/`, from `icons/src`) at the phone's scale.
10. **Launch.** Tap Shoal Keys in the app grid. keel-shell should start the app under Sailjail with only Secrets, Downloads and ShipwrightLicences. Check `cat /proc/$(pgrep -f /usr/bin/shipwright-shoal-keys)/status | grep -i seccomp` and that `curl` from `sailjail -p shipwright-shoal-keys.desktop` fails.

## Root changes and open questions

- The root `Cargo.toml` must add `"shoal/keys/app"` to `exclude`.
- `LICENSES/CC-BY-3.0-US.txt` must be added for the EFF wordlist (`core/data/eff_large_wordlist.txt`, provenance in its `.license` file). The keepass-rs test fixtures are MIT, which is already present.
- `Cargo.lock` gained `rust-argon2` and `uuid` as direct dependencies of `shoal-keys-core`, both already in the tree through `keepass`, and zbus's `p2p` feature.
- Licence hand-off: implemented as a D-Bus service in Reef (`reef/licence/README.md`); its device checks are listed there.
- The Sailfish Secrets client moved to `shoal/secrets` (root workspace member `shoal-secrets`); `core/src/sailfish.rs` is now a thin adapter. Behaviour and the `shoalkeys` collection are unchanged.
