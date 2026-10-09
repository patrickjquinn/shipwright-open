// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: MIT OR Apache-2.0

//! shoal-secrets: one Sailfish Secrets client for every Shipwright app.
//!
//! Sailfish Secrets' own client library (libsailfishsecrets) is Qt 5 only,
//! so a Qt 6 app cannot link it. This crate speaks `sailfishsecretsd`'s
//! peer-to-peer D-Bus protocol directly instead ([`sailfish`]), behind a
//! small blocking trait, [`SecretStore`], that apps adapt to their own
//! error types:
//!
//! - [`SecretStore`]: get, put (replacing), delete (idempotent) of small
//!   named secrets.
//! - [`MemoryStore`]: in-process backend for tests and host runs, with a
//!   switch that makes every call fail like a missing daemon.
//! - [`sailfish::SailfishSecrets`] (feature `sailfish`, default): the real
//!   backend. **[device verification]**: the wire format is checked against
//!   a fake daemon, never against the real one.
//! - `fake` (feature `test-harness`): that fake daemon, for the tests of
//!   apps that use this crate.
//! - [`name`]: turns arbitrary keys into conservative secret names.
//!
//! Users: Shoal Keys (the vault's wrapping key, collection `shoalkeys`) and
//! Shoal Mail (account passwords and OAuth 2 refresh tokens, collection
//! `shoalmail`).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

pub use zeroize::Zeroizing;

#[cfg(all(feature = "sailfish", any(test, feature = "test-harness")))]
pub mod fake;
pub mod name;
#[cfg(feature = "sailfish")]
pub mod sailfish;

/// Errors from a secret store. Messages never contain secret material.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The store cannot be reached at all (daemon not installed, not
    /// running, or the connection dropped). Callers may fall back to
    /// another store.
    #[error("{0}")]
    Unavailable(String),
    /// The store answered but refused or failed the operation.
    #[error("{0}")]
    Failed(String),
    /// A name the store does not accept.
    #[error("bad secret name {0:?}")]
    BadName(String),
}

impl Error {
    /// True when the store is not there, as opposed to refusing a request.
    pub fn is_unavailable(&self) -> bool {
        matches!(self, Error::Unavailable(_))
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// A store of small named secrets (keys, passwords, tokens).
///
/// Calls block (the Sailfish backend does a D-Bus round trip); async
/// callers should run them on a blocking thread.
pub trait SecretStore: Send + Sync {
    /// The secret stored under `name`, or `None` if there is none.
    fn get(&self, name: &str) -> Result<Option<Zeroizing<Vec<u8>>>>;
    /// Stores `secret` under `name`, replacing any existing one. A failed
    /// or interrupted replace leaves either the old or the new value
    /// readable, never neither.
    fn put(&self, name: &str, secret: &[u8]) -> Result<()>;
    /// Removes `name`. Removing a missing name is not an error.
    fn delete(&self, name: &str) -> Result<()>;
    /// Short description for a settings page ("Sailfish Secrets", ...).
    fn describe(&self) -> &'static str;
}

/// In-process store. Contents vanish with the process.
#[derive(Default)]
pub struct MemoryStore {
    items: Mutex<HashMap<String, Zeroizing<Vec<u8>>>>,
    /// When set, every call fails with [`Error::Unavailable`]: simulates a
    /// daemon that is not running.
    pub fail: AtomicBool,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Makes every later call fail (or succeed again).
    pub fn set_failing(&self, fail: bool) {
        self.fail.store(fail, Ordering::SeqCst);
    }

    pub fn len(&self) -> usize {
        self.lock().map(|m| m.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, HashMap<String, Zeroizing<Vec<u8>>>>> {
        if self.fail.load(Ordering::SeqCst) {
            return Err(Error::Unavailable("simulated failure".into()));
        }
        self.items
            .lock()
            .map_err(|_| Error::Failed("lock poisoned".into()))
    }
}

impl SecretStore for MemoryStore {
    fn get(&self, name: &str) -> Result<Option<Zeroizing<Vec<u8>>>> {
        Ok(self.lock()?.get(name).cloned())
    }

    fn put(&self, name: &str, secret: &[u8]) -> Result<()> {
        self.lock()?
            .insert(name.to_string(), Zeroizing::new(secret.to_vec()));
        Ok(())
    }

    fn delete(&self, name: &str) -> Result<()> {
        self.lock()?.remove(name);
        Ok(())
    }

    fn describe(&self) -> &'static str {
        "in-memory (tests)"
    }
}

/// Runs the [`SecretStore`] contract against `store`: missing names read as
/// `None`, put replaces, delete is idempotent. Panics on a violation. Used
/// by this crate's tests and available to apps' tests of their adapters.
///
/// # Panics
///
/// On any contract violation, and on any error from `store`.
pub fn check_contract(store: &dyn SecretStore, name: &str) {
    assert!(store.get(name).unwrap().is_none(), "fresh name is empty");
    store.put(name, &[1, 2, 3]).unwrap();
    assert_eq!(store.get(name).unwrap().unwrap().as_slice(), &[1, 2, 3]);
    store.put(name, &[4]).unwrap();
    assert_eq!(
        store.get(name).unwrap().unwrap().as_slice(),
        &[4],
        "put replaces"
    );
    store.delete(name).unwrap();
    store.delete(name).unwrap();
    assert!(store.get(name).unwrap().is_none(), "deleted");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_contract_and_failure() {
        let m = MemoryStore::new();
        check_contract(&m, "a");
        m.put("b", b"x").unwrap();
        assert_eq!(m.len(), 1);
        m.set_failing(true);
        assert!(m.get("b").unwrap_err().is_unavailable());
        assert!(m.put("b", b"y").is_err());
        m.set_failing(false);
        assert_eq!(m.get("b").unwrap().unwrap().as_slice(), b"x");
    }

    #[test]
    fn errors_display_plainly() {
        assert_eq!(Error::Unavailable("gone".into()).to_string(), "gone");
        assert_eq!(
            Error::BadName("a/b".into()).to_string(),
            "bad secret name \"a/b\""
        );
    }
}
