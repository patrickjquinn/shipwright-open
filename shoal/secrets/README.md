<!--
SPDX-FileCopyrightText: 2026 Patrick Quinn
SPDX-License-Identifier: MIT OR Apache-2.0
-->

# shoal-secrets

One Sailfish Secrets client for every Shipwright app. Sailfish Secrets' own client library (libsailfishsecrets) is Qt 5 only, so a Qt 6 app cannot link it. This crate speaks `sailfishsecretsd`'s peer-to-peer D-Bus protocol directly with zbus 5 (async-io, no tokio), behind a small blocking trait. The code started in Shoal Keys (`shoal/keys/core/src/sailfish.rs`) and moved here unchanged in behaviour.

| Item | What |
| --- | --- |
| `SecretStore` | `get`, `put` (replaces), `delete` (idempotent), `describe`. Blocking; async callers run it on a blocking thread. |
| `Error` | `Unavailable` (no daemon, connection lost: callers may fall back), `Failed` (the daemon refused), `BadName`. Messages never hold secret material. |
| `MemoryStore` | In-process backend for tests and host runs. `set_failing(true)` makes every call return `Unavailable`. |
| `sailfish::SailfishSecrets` | The real backend, feature `sailfish` (default). **[device verification]** One collection per app (`Collection::new(name, app_id)`), in `plugin.encryptedstorage.default`, `DeviceLockKeepUnlocked`, `OwnerOnlyMode`, no user interaction. `probe()` connects and creates the collection. |
| `fake::FakeDaemon` | Feature `test-harness`: a fake `sailfishsecretsd` on a socket pair with the upstream signatures and error codes; `stop()` simulates the daemon going away. For apps' tests (dev-dependency only). |
| `name::encode` / `decode` | Maps any key to `[A-Za-z0-9._-]` reversibly (`account/a1/password` becomes `account_2Fa1_2Fpassword`). Names already safe are unchanged. |
| `check_contract` | The trait contract as a test helper. |

Users:

- **Shoal Keys** (`shoal/keys/core/src/sailfish.rs`): collection `shoalkeys`, the vault's wrapping key. Behaviour unchanged by the move.
- **Shoal Mail** (`shoal/mail/core/src/secrets.rs`, `PlatformSecretStore`): collection `shoalmail`, account passwords and OAuth2 refresh tokens, with the old 0600 file as fallback and a one-time migration out of it.

Both apps need the Sailjail `Secrets` permission (desktop file).

## Protocol

1. Ask `org.sailfishos.secrets.daemon.discovery` on the session bus (object `/Sailfish/Secrets/Discovery`, method `peerToPeerAddress`) for the private socket address; fall back to `$XDG_RUNTIME_DIR/sailfishsecretsd/p2pSocket`.
2. Open a peer-to-peer D-Bus connection and call `org.sailfishos.secrets` on `/Sailfish/Secrets`: `createCollection(sss(i)(i)) -> (iis)`, `setSecret(((sss)aya{sv})(ssss(i)sa{is}(i)(i))(i)s) -> (iis)`, `getSecret((sss)(i)s) -> ((iis)((sss)aya{sv}))`, `deleteSecret((sss)(i)s) -> (iis)`.

Signatures and enum values are copied from upstream `daemon/SecretsImpl/secrets_p.h`, `lib/Secrets/serialization.cpp`, `secretmanager.h` and `result.h`. A call that fails at the transport level drops the connection, so the next call reconnects and re-creates the collection.

## Build and test

```
export CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_TARGET_DIR=$PWD/target
cargo test -p shoal-secrets                     # 10 tests
cargo clippy -p shoal-secrets --all-targets -- -D warnings
cargo clippy -p shoal-secrets --no-default-features --all-targets -- -D warnings
```

The tests check the D-Bus signatures, the store contract over the fake daemon (collection created once, `Blob` type and `app` filter written, replace on `SecretAlreadyExists` through a pending `<name>_new` copy so an interrupted replace never loses the secret, missing secrets read as `None`), an existing collection, a daemon that goes away (`Unavailable`), no daemon at all (`Unavailable`), the in-memory backend and the name encoding.

## After a restart, on Sailfish OS 5.2

Measured on the Jolla Phone (5.2.0.18, sailfish-secrets 0.2.44, polkit 125)
with Shoal Messages, which uses the same kind of collection: a device-lock
collection in the encrypted-storage plugin is open in the boot it was created
in and locked after every restart. Unlocking it needs secretsd's device-lock
check, which does not work for a sandboxed app:

- The password-agent plugin asks polkit, which has no agent for an app
  started by the launcher (it runs outside the login session). The plugin
  should then fall back to the device lock, but it tests "the reply has
  details", and polkit 125 always adds `polkit.result`, so it reports
  `IncorrectAuthenticationCodeError` (11, "Password Agent was unable to verify
  the authenticity of the user") instead.
- With that test corrected (built and tried on the phone), the fallback asks
  the device-lock service for a confirmation, and no dialog appears; the
  request times out after two minutes.

This crate never asks for interaction (`PreventInteraction`), so after a
restart a read gets `CollectionIsLockedError` (errorCode 60): Keys and
Mail must not rely on reading back what they stored before the restart. Shoal
Messages keeps its key in its own data directory for this reason
(`shoal/messages/src/secretskeeper.cpp`).

## Needs device verification

The fake daemon proves the client matches what we read upstream, not what the phone's daemon does.

1. **Round trip.** Install Shoal Keys or Shoal Mail with the `Secrets` permission. Create a vault (Keys) or add an account (Mail), restart the app, and check that it still unlocks or signs in. `journalctl --user -u sailfish-secretsd` (or the daemon's log) should show `createCollection` and `setSecret` from the app.
2. **Discovery under Sailjail.** Check that `peerToPeerAddress` answers inside the sandbox, or that the fallback socket under `$XDG_RUNTIME_DIR/sailfishsecretsd/` is reachable.
3. **Owner-only access.** From another app (or `sailjail -p` another desktop file) try to read the `shoalkeys` or `shoalmail` collection: it must be refused.
4. **Names.** Check that a name with `_` and `.` (Mail's `account_2F<id>_2Fpassword`) is stored and read back.
5. **Error codes.** Read a missing secret and create an existing collection; record the `(code, errorCode)` the daemon returns and compare with the constants in `src/sailfish.rs` (40/41 missing, 46 collection exists, 47 secret exists).
6. **Daemon restart.** With the app open, `systemctl --user restart sailfish-secretsd` (or kill it) and use the app again: the next call should reconnect.
