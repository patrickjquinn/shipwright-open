// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

//! Where the key that wraps the vault key lives.
//!
//! On a phone this is Sailfish Secrets ([`crate::sailfish`]). Tests use
//! [`MemoryKeyStore`]. [`PlainFileKeyStore`] exists only so the app can be
//! run on a development host; it gives no protection and the app refuses
//! it unless explicitly asked for.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use zeroize::Zeroizing;

use crate::Error;

/// A store of small named secrets (32-byte wrapping keys).
pub trait KeyStore: Send + Sync {
    /// The secret stored under `name`, or `None` if there is none.
    fn get(&self, name: &str) -> Result<Option<Zeroizing<Vec<u8>>>, Error>;
    /// Stores `secret` under `name`, replacing any existing one.
    fn put(&self, name: &str, secret: &[u8]) -> Result<(), Error>;
    /// Removes `name`. Removing a missing name is not an error.
    fn delete(&self, name: &str) -> Result<(), Error>;
    /// Short description for the settings page ("Sailfish Secrets", ...).
    fn describe(&self) -> &'static str;
}

/// In-process store for tests. Contents vanish with the process.
#[derive(Default)]
pub struct MemoryKeyStore {
    items: Mutex<HashMap<String, Zeroizing<Vec<u8>>>>,
    /// When set, every call fails: simulates a daemon that is not running.
    pub fail: std::sync::atomic::AtomicBool,
}

impl MemoryKeyStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn check(&self) -> Result<(), Error> {
        if self.fail.load(std::sync::atomic::Ordering::SeqCst) {
            Err(Error::KeyStore("simulated failure".into()))
        } else {
            Ok(())
        }
    }
}

impl KeyStore for MemoryKeyStore {
    fn get(&self, name: &str) -> Result<Option<Zeroizing<Vec<u8>>>, Error> {
        self.check()?;
        Ok(self
            .items
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(name)
            .cloned())
    }

    fn put(&self, name: &str, secret: &[u8]) -> Result<(), Error> {
        self.check()?;
        self.items
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(name.to_string(), Zeroizing::new(secret.to_vec()));
        Ok(())
    }

    fn delete(&self, name: &str) -> Result<(), Error> {
        self.check()?;
        self.items
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(name);
        Ok(())
    }

    fn describe(&self) -> &'static str {
        "in-memory (tests)"
    }
}

/// Development-host store: one file per secret, mode 0600, **unencrypted**.
/// Never used on a device (the app only selects it with
/// `SHOAL_KEYS_INSECURE_KEYSTORE=<dir>`).
pub struct PlainFileKeyStore {
    dir: PathBuf,
}

impl PlainFileKeyStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    fn path(&self, name: &str) -> Result<PathBuf, Error> {
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return Err(Error::KeyStore(format!("bad secret name {name:?}")));
        }
        Ok(self.dir.join(format!("{name}.key")))
    }
}

impl KeyStore for PlainFileKeyStore {
    fn get(&self, name: &str) -> Result<Option<Zeroizing<Vec<u8>>>, Error> {
        match std::fs::read(self.path(name)?) {
            Ok(b) => Ok(Some(Zeroizing::new(b))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn put(&self, name: &str, secret: &[u8]) -> Result<(), Error> {
        std::fs::create_dir_all(&self.dir)?;
        let path = self.path(name)?;
        crate::store::write_atomic(&path, secret)?;
        Ok(())
    }

    fn delete(&self, name: &str) -> Result<(), Error> {
        match std::fs::remove_file(self.path(name)?) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
            _ => Ok(()),
        }
    }

    fn describe(&self) -> &'static str {
        "plain file (insecure, development only)"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exercise(ks: &dyn KeyStore) {
        assert!(ks.get("a").unwrap().is_none());
        ks.put("a", &[1, 2, 3]).unwrap();
        assert_eq!(ks.get("a").unwrap().unwrap().as_slice(), &[1, 2, 3]);
        ks.put("a", &[4]).unwrap();
        assert_eq!(ks.get("a").unwrap().unwrap().as_slice(), &[4]);
        ks.delete("a").unwrap();
        ks.delete("a").unwrap();
        assert!(ks.get("a").unwrap().is_none());
    }

    #[test]
    fn memory() {
        let m = MemoryKeyStore::new();
        exercise(&m);
        m.fail.store(true, std::sync::atomic::Ordering::SeqCst);
        assert!(m.get("a").is_err());
    }

    #[test]
    fn plain_file() {
        let dir = crate::store::tests::tempdir("ks");
        let ks = PlainFileKeyStore::new(&dir);
        exercise(&ks);
        assert!(ks.put("../x", &[1]).is_err());
        std::fs::remove_dir_all(dir).ok();
    }
}
