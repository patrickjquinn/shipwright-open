// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

//! [`KeyStore`] on Sailfish Secrets. **[device verification]**
//!
//! The client itself lives in the shared crate `shoal-secrets`
//! (`shoal/secrets`, module `sailfish`), which Shoal Mail uses too. It
//! speaks `sailfishsecretsd`'s peer-to-peer D-Bus protocol directly, because
//! libsailfishsecrets is Qt 5 only. This module only fixes Keys' collection
//! and maps errors.
//!
//! Storage: one collection, [`COLLECTION`], in the default encrypted-storage
//! plugin, device-lock protected (`DeviceLockKeepUnlocked`: readable after
//! the first device unlock since boot) and `OwnerOnlyMode` (only this
//! application may read it). Each vault's wrapping key is one secret in it.
//! No calls allow user interaction (`PreventInteraction`): the vault's own
//! master password is the user-facing prompt.

use shoal_secrets::sailfish::{Collection, SailfishSecrets};
use shoal_secrets::SecretStore;
use zeroize::Zeroizing;

use crate::keystore::KeyStore;
use crate::Error;

pub use shoal_secrets::sailfish::{
    Connection, DbusResult, Identifier, UiParams, DISCOVERY_PATH, DISCOVERY_SERVICE,
    ENCRYPTED_STORAGE_PLUGIN, SECRETS_INTERFACE, SECRETS_PATH,
};

pub const COLLECTION: &str = "shoalkeys";

/// Keys' collection in Sailfish Secrets.
pub fn collection() -> Collection {
    Collection::new(COLLECTION, crate::APP_ID)
}

/// The Sailfish Secrets key store.
pub struct SailfishSecretsKeyStore {
    inner: SailfishSecrets,
}

impl SailfishSecretsKeyStore {
    /// A store that connects to the daemon on first use.
    pub fn new() -> Self {
        Self {
            inner: SailfishSecrets::new(collection()),
        }
    }

    /// A store over an already open client (tests, or a caller that
    /// connected itself).
    pub fn with_client(inner: SailfishSecrets) -> Self {
        Self { inner }
    }

    /// A store over an already open peer connection (tests).
    pub fn with_connection(conn: Connection) -> Self {
        Self::with_client(SailfishSecrets::with_connection(collection(), conn))
    }
}

impl Default for SailfishSecretsKeyStore {
    fn default() -> Self {
        Self::new()
    }
}

fn map(e: &shoal_secrets::Error) -> Error {
    Error::KeyStore(e.to_string())
}

impl KeyStore for SailfishSecretsKeyStore {
    fn get(&self, name: &str) -> Result<Option<Zeroizing<Vec<u8>>>, Error> {
        self.inner.get(name).map_err(|e| map(&e))
    }

    fn put(&self, name: &str, secret: &[u8]) -> Result<(), Error> {
        self.inner.put(name, secret).map_err(|e| map(&e))
    }

    fn delete(&self, name: &str) -> Result<(), Error> {
        self.inner.delete(name).map_err(|e| map(&e))
    }

    fn describe(&self) -> &'static str {
        self.inner.describe()
    }
}

#[cfg(test)]
mod tests {
    //! The fake `sailfishsecretsd` from `shoal-secrets` (feature
    //! `test-harness`) checks the wire format without a device.
    use super::*;
    use shoal_secrets::fake::FakeDaemon;

    #[test]
    fn signatures_match_upstream_introspection() {
        use shoal_secrets::sailfish::zvariant::Type;
        use shoal_secrets::sailfish::SecretData;
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

    fn pair() -> (SailfishSecretsKeyStore, FakeDaemon) {
        let (client, daemon) = FakeDaemon::start(collection());
        (SailfishSecretsKeyStore::with_client(client), daemon)
    }

    #[test]
    fn key_store_contract_over_fake_daemon() {
        let (ks, daemon) = pair();
        assert!(ks.get("shoalkeys-a").unwrap().is_none());
        ks.put("shoalkeys-a", &[1, 2, 3]).unwrap();
        assert_eq!(
            ks.get("shoalkeys-a").unwrap().unwrap().as_slice(),
            &[1, 2, 3]
        );
        ks.put("shoalkeys-a", &[4]).unwrap(); // replaces
        assert_eq!(ks.get("shoalkeys-a").unwrap().unwrap().as_slice(), &[4]);
        ks.delete("shoalkeys-a").unwrap();
        ks.delete("shoalkeys-a").unwrap();
        assert!(ks.get("shoalkeys-a").unwrap().is_none());

        let calls = daemon.calls();
        assert_eq!(
            calls[0],
            "createCollection shoalkeys plugin.encryptedstorage.default plugin.encryptedstorage.default 0 0"
        );
        assert_eq!(
            calls
                .iter()
                .filter(|c| c.starts_with("createCollection"))
                .count(),
            1,
            "collection created once per process"
        );
        assert!(
            calls
                .iter()
                .any(|c| c
                    == "setSecret shoalkeys-a shoalkeys plugin.encryptedstorage.default 0 Blob")
        );
        ks.put("shoalkeys-b", &[9]).unwrap();
        assert_eq!(
            daemon
                .state
                .lock()
                .unwrap()
                .apps
                .get(&(COLLECTION.into(), "shoalkeys-b".into()))
                .map(String::as_str),
            Some(crate::APP_ID)
        );
    }

    #[test]
    fn existing_collection_is_fine_and_vault_store_works() {
        let (ks, daemon) = pair();
        daemon
            .state
            .lock()
            .unwrap()
            .collections
            .push(COLLECTION.into());
        let ks: std::sync::Arc<dyn KeyStore> = std::sync::Arc::new(ks);
        let dir = crate::store::tests::tempdir("sfs");
        let s = crate::VaultStore::new(&dir, ks).with_kdf(crate::KdfParams::insecure_for_tests());
        drop(s.create("V", "pw").unwrap());
        assert!(s.unlock("pw").is_ok());
        assert_eq!(daemon.state.lock().unwrap().secrets.len(), 1);
        std::fs::remove_dir_all(dir).ok();
    }
}
