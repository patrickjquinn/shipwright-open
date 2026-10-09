// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: MIT OR Apache-2.0

//! [`SecretStore`] on Sailfish Secrets. **[device verification]**
//!
//! Route: Sailfish Secrets (github.com/sailfishos/sailfish-secrets,
//! BSD-3-Clause) is a daemon, `sailfishsecretsd`, with a C++/Qt client
//! library. Rather than link that Qt 5 library into a Qt 6 app, this module
//! speaks the daemon's D-Bus protocol directly with zbus, as the library
//! does (`lib/Secrets/secretsdaemonconnection.cpp`):
//!
//! 1. Ask `org.sailfishos.secrets.daemon.discovery` on the session bus
//!    (object `/Sailfish/Secrets/Discovery`, method `peerToPeerAddress`)
//!    for the private socket address; fall back to
//!    `$XDG_RUNTIME_DIR/sailfishsecretsd/p2pSocket`.
//! 2. Open a peer-to-peer D-Bus connection to it and call interface
//!    `org.sailfishos.secrets` on object `/Sailfish/Secrets`.
//!
//! Signatures below are copied from `daemon/SecretsImpl/secrets_p.h` and the
//! marshalling in `lib/Secrets/serialization.cpp` at upstream master.
//!
//! Storage: one collection per app ([`Collection`]), in the default
//! encrypted-storage plugin, device-lock protected (`DeviceLockKeepUnlocked`:
//! readable after the first device unlock since boot) and `OwnerOnlyMode`
//! (only the creating application may read it). No call allows user
//! interaction (`PreventInteraction`): each app has its own user-facing
//! prompt, or none. The app needs the Sailjail `Secrets` permission.
//!
//! This code was moved here unchanged in behaviour from Shoal Keys
//! (`shoal/keys/core/src/sailfish.rs`), where it first lived.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

/// The peer connection type, re-exported so users need no direct zbus
/// dependency.
pub use zbus::blocking::Connection;
/// zvariant, for callers that check signatures in their tests.
pub use zbus::zvariant;
use zbus::zvariant::{OwnedValue, Value};
use zeroize::Zeroizing;

use crate::{Error, Result, SecretStore};

pub const DISCOVERY_SERVICE: &str = "org.sailfishos.secrets.daemon.discovery";
pub const DISCOVERY_PATH: &str = "/Sailfish/Secrets/Discovery";
pub const SECRETS_PATH: &str = "/Sailfish/Secrets";
pub const SECRETS_INTERFACE: &str = "org.sailfishos.secrets";
/// `SecretManager::DefaultEncryptedStoragePluginName`.
pub const ENCRYPTED_STORAGE_PLUGIN: &str = "plugin.encryptedstorage.default";

// Enum values from lib/Secrets/secretmanager.h and result.h.
pub const DEVICE_LOCK_KEEP_UNLOCKED: i32 = 0;
pub const OWNER_ONLY_MODE: i32 = 0;
pub const PREVENT_INTERACTION: i32 = 0;
pub const RESULT_SUCCEEDED: i32 = 0;
pub const RESULT_FAILED: i32 = 2;
pub const ERR_INVALID_SECRET: i32 = 40;
pub const ERR_INVALID_SECRET_IDENTIFIER: i32 = 41;
pub const ERR_INVALID_COLLECTION: i32 = 43;
pub const ERR_COLLECTION_ALREADY_EXISTS: i32 = 46;
pub const ERR_SECRET_ALREADY_EXISTS: i32 = 47;

/// `Result`: `(code, errorCode, errorMessage)`, D-Bus `(iis)`.
pub type DbusResult = (i32, i32, String);
/// `Secret::Identifier`: `(name, collectionName, storagePluginName)`, `(sss)`.
pub type Identifier = (String, String, String);
/// `Secret`: identifier, data, filter data, `((sss)aya{sv})`.
pub type SecretData = (Identifier, Vec<u8>, HashMap<String, OwnedValue>);
/// `InteractionParameters`, `(ssss(i)sa{is}(i)(i))`: secret name, collection
/// name, plugin name, application id, operation, authentication plugin,
/// prompt texts, input type, echo mode. Sent empty: no interaction.
pub type UiParams = (
    String,
    String,
    String,
    String,
    (i32,),
    String,
    HashMap<i32, String>,
    (i32,),
    (i32,),
);

fn empty_ui_params() -> UiParams {
    (
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        (0,),
        String::new(),
        HashMap::new(),
        (0,),
        (0,),
    )
}

/// Which collection an app keeps its secrets in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Collection {
    /// Collection name, for example `shoalkeys`.
    pub name: String,
    /// Written into each secret's filter data as `app`, for diagnostics.
    pub app_id: String,
    /// Storage plugin; [`ENCRYPTED_STORAGE_PLUGIN`] unless testing.
    pub storage_plugin: String,
}

impl Collection {
    pub fn new(name: &str, app_id: &str) -> Self {
        Self {
            name: name.to_string(),
            app_id: app_id.to_string(),
            storage_plugin: ENCRYPTED_STORAGE_PLUGIN.to_string(),
        }
    }
}

/// Where to look for the daemon. `None` fields come from the environment
/// (`DBUS_SESSION_BUS_ADDRESS`, `$XDG_RUNTIME_DIR/sailfishsecretsd/p2pSocket`)
/// when the store first connects.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Addresses {
    /// The session bus, where the discovery service answers.
    pub session_bus: Option<String>,
    /// The daemon's peer-to-peer address if discovery fails.
    pub p2p_fallback: Option<String>,
}

/// Suffix of the name that holds a new value while `put` replaces an
/// existing secret. `_` followed by non-hex never comes out of
/// [`crate::name::encode`], so it cannot clash with an encoded name.
const PENDING_SUFFIX: &str = "_new";

fn pending_name(name: &str) -> String {
    format!("{name}{PENDING_SUFFIX}")
}

/// The Sailfish Secrets store for one collection.
pub struct SailfishSecrets {
    conn: Mutex<Option<Connection>>,
    collection: Collection,
    collection_ready: AtomicBool,
    addresses: Addresses,
}

impl SailfishSecrets {
    /// A store that connects to the daemon on first use.
    pub fn new(collection: Collection) -> Self {
        Self::with_addresses(collection, Addresses::default())
    }

    /// A store that looks for the daemon at `addresses` instead of the
    /// environment's (tests, or a non-default session).
    pub fn with_addresses(collection: Collection, addresses: Addresses) -> Self {
        Self {
            conn: Mutex::new(None),
            collection,
            collection_ready: false.into(),
            addresses,
        }
    }

    /// A store over an already open peer connection (tests).
    pub fn with_connection(collection: Collection, conn: Connection) -> Self {
        let s = Self::new(collection);
        if let Ok(mut g) = s.conn.lock() {
            *g = Some(conn);
        }
        s
    }

    pub fn collection(&self) -> &Collection {
        &self.collection
    }

    /// Connects and makes sure the collection exists. `Ok` means the store
    /// is usable now; [`Error::Unavailable`] means there is no daemon.
    pub fn probe(&self) -> Result<()> {
        self.ensure_collection()
    }

    fn connect(&self) -> Result<Connection> {
        let fallback = self.addresses.p2p_fallback.clone().unwrap_or_else(|| {
            std::env::var("XDG_RUNTIME_DIR")
                .map(|d| format!("unix:path={d}/sailfishsecretsd/p2pSocket"))
                .unwrap_or_default()
        });
        let session = match &self.addresses.session_bus {
            Some(a) => zbus::blocking::connection::Builder::address(a.as_str())
                .and_then(zbus::blocking::connection::Builder::build),
            None => Connection::session(),
        };
        let address = session
            .and_then(|bus| {
                bus.call_method(
                    Some(DISCOVERY_SERVICE),
                    DISCOVERY_PATH,
                    Some(DISCOVERY_SERVICE),
                    "peerToPeerAddress",
                    &(),
                )
            })
            .and_then(|reply| reply.body().deserialize::<String>())
            .unwrap_or(fallback);
        if address.is_empty() {
            return Err(Error::Unavailable(
                "Sailfish Secrets daemon not found".into(),
            ));
        }
        zbus::blocking::connection::Builder::address(address.as_str())
            .and_then(|b| b.p2p().build())
            .map_err(|e| Error::Unavailable(format!("cannot connect to Sailfish Secrets: {e}")))
    }

    fn call<B, R>(&self, method: &str, body: &B) -> Result<R>
    where
        B: serde::Serialize + zbus::zvariant::DynamicType,
        R: for<'d> serde::Deserialize<'d> + zbus::zvariant::Type,
    {
        let mut guard = self
            .conn
            .lock()
            .map_err(|_| Error::Failed("lock poisoned".into()))?;
        if guard.is_none() {
            *guard = Some(self.connect()?);
        }
        let conn = guard
            .as_ref()
            .ok_or_else(|| Error::Unavailable("not connected".into()))?;
        let reply = conn
            .call_method(
                None::<&str>,
                SECRETS_PATH,
                Some(SECRETS_INTERFACE),
                method,
                body,
            )
            .map_err(|e| {
                // Drop the connection so the next call reconnects (daemon
                // restart), and let the next call re-create the collection.
                let msg = format!("Sailfish Secrets {method}: {e}");
                *guard = None;
                self.collection_ready.store(false, Ordering::SeqCst);
                Error::Unavailable(msg)
            })?;
        reply
            .body()
            .deserialize::<R>()
            .map_err(|e| Error::Failed(format!("Sailfish Secrets {method} reply: {e}")))
    }

    fn ident(&self, name: &str) -> Identifier {
        (
            name.to_string(),
            self.collection.name.clone(),
            self.collection.storage_plugin.clone(),
        )
    }

    fn ensure_collection(&self) -> Result<()> {
        if self.collection_ready.load(Ordering::SeqCst) {
            return Ok(());
        }
        let plugin = self.collection.storage_plugin.as_str();
        let r: DbusResult = self.call(
            "createCollection",
            &(
                self.collection.name.as_str(),
                plugin,
                plugin,
                (DEVICE_LOCK_KEEP_UNLOCKED,),
                (OWNER_ONLY_MODE,),
            ),
        )?;
        if r.0 == RESULT_SUCCEEDED || r.1 == ERR_COLLECTION_ALREADY_EXISTS {
            self.collection_ready.store(true, Ordering::SeqCst);
            Ok(())
        } else {
            Err(failed("createCollection", &r))
        }
    }

    fn set_secret(&self, name: &str, secret: &[u8]) -> Result<DbusResult> {
        let mut filter: HashMap<&str, Value<'_>> = HashMap::new();
        filter.insert("Type", Value::from("Blob"));
        filter.insert("app", Value::from(self.collection.app_id.as_str()));
        self.call(
            "setSecret",
            &(
                (self.ident(name), secret, filter),
                empty_ui_params(),
                (PREVENT_INTERACTION,),
                "",
            ),
        )
    }
}

fn failed(method: &str, r: &DbusResult) -> Error {
    Error::Failed(format!(
        "Sailfish Secrets {method} failed ({}): {}",
        r.1, r.2
    ))
}

fn is_missing(r: &DbusResult) -> bool {
    r.1 == ERR_INVALID_SECRET || r.1 == ERR_INVALID_SECRET_IDENTIFIER
}

impl SailfishSecrets {
    fn get_one(&self, name: &str) -> Result<Option<Zeroizing<Vec<u8>>>> {
        let (r, secret): (DbusResult, SecretData) =
            self.call("getSecret", &(self.ident(name), (PREVENT_INTERACTION,), ""))?;
        let data = Zeroizing::new(secret.1);
        if r.0 == RESULT_SUCCEEDED {
            Ok(Some(data))
        } else if is_missing(&r) {
            Ok(None)
        } else {
            Err(failed("getSecret", &r))
        }
    }

    fn set_one(&self, name: &str, secret: &[u8]) -> Result<()> {
        let r = self.set_secret(name, secret)?;
        if r.0 == RESULT_SUCCEEDED {
            Ok(())
        } else {
            Err(failed("setSecret", &r))
        }
    }

    fn delete_one(&self, name: &str) -> Result<()> {
        let r: DbusResult = self.call(
            "deleteSecret",
            &(self.ident(name), (PREVENT_INTERACTION,), ""),
        )?;
        if r.0 == RESULT_SUCCEEDED || is_missing(&r) {
            Ok(())
        } else {
            Err(failed("deleteSecret", &r))
        }
    }
}

impl SecretStore for SailfishSecrets {
    /// Falls back to the pending copy a `put` interrupted between deleting
    /// the old value and writing the new one leaves behind.
    fn get(&self, name: &str) -> Result<Option<Zeroizing<Vec<u8>>>> {
        self.ensure_collection()?;
        match self.get_one(name)? {
            Some(v) => Ok(Some(v)),
            None => self.get_one(&pending_name(name)),
        }
    }

    /// The daemon has no replace or rename, so a replace goes through a
    /// pending copy: write the new value under `<name>_new`, delete the old
    /// one, write the new one under `name`, drop the copy. If this stops
    /// part-way (crash, daemon gone), `get` still finds either the old or
    /// the new value; it never finds nothing.
    fn put(&self, name: &str, secret: &[u8]) -> Result<()> {
        self.ensure_collection()?;
        let pending = pending_name(name);
        let r = self.set_secret(name, secret)?;
        if r.0 == RESULT_SUCCEEDED {
            // A copy an interrupted replace left behind is stale now.
            if let Err(e) = self.delete_one(&pending) {
                log::warn!("Sailfish Secrets: could not remove a stale pending copy: {e}");
            }
            return Ok(());
        }
        if r.1 != ERR_SECRET_ALREADY_EXISTS {
            return Err(failed("setSecret", &r));
        }
        // A copy left by an earlier interrupted replace.
        self.delete_one(&pending)?;
        self.set_one(&pending, secret)?;
        self.delete_one(name)?;
        self.set_one(name, secret)?;
        if let Err(e) = self.delete_one(&pending) {
            // Same value under both names; the next put or delete tidies up.
            log::warn!("Sailfish Secrets: could not remove the pending copy: {e}");
        }
        Ok(())
    }

    /// The pending copy goes first: if a later step fails, `get` must not
    /// find a copy of a secret whose main name is already gone.
    fn delete(&self, name: &str) -> Result<()> {
        self.ensure_collection()?;
        self.delete_one(&pending_name(name))?;
        self.delete_one(name)
    }

    fn describe(&self) -> &'static str {
        "Sailfish Secrets"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::Type;

    #[test]
    fn signatures_match_upstream_introspection() {
        assert_eq!(<DbusResult as Type>::SIGNATURE.to_string(), "(iis)");
        assert_eq!(<Identifier as Type>::SIGNATURE.to_string(), "(sss)");
        assert_eq!(
            <UiParams as Type>::SIGNATURE.to_string(),
            "(ssss(i)sa{is}(i)(i))"
        );
        assert_eq!(
            <SecretData as Type>::SIGNATURE.to_string(),
            "((sss)aya{sv})"
        );
    }

    #[test]
    fn no_daemon_is_unavailable() {
        // Neither discovery nor the fallback socket can be reached.
        let store = SailfishSecrets::with_addresses(
            Collection::new("t", "t"),
            Addresses {
                session_bus: Some("unix:path=/nonexistent/bus".into()),
                p2p_fallback: Some("unix:path=/nonexistent/p2pSocket".into()),
            },
        );
        let e = store.get("x").unwrap_err();
        assert!(e.is_unavailable(), "{e:?}");
    }

    #[test]
    fn pending_names_are_never_encodings() {
        assert_eq!(crate::name::decode(&pending_name("abc")), None);
        assert_ne!(crate::name::encode("abc_new"), pending_name("abc"));
    }
}
