// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: MIT OR Apache-2.0

//! A fake `sailfishsecretsd` on a socket pair (feature `test-harness`).
//!
//! It implements the four methods [`crate::sailfish`] calls, with the
//! upstream signatures, and the result and error codes the real daemon
//! returns for the cases the client handles (collection exists, secret
//! exists, no such secret, no such collection). It checks the wire format
//! without a device. It is **not** evidence that the real daemon behaves
//! the same; that is device verification.

use std::collections::HashMap;
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Mutex};

use zbus::blocking::Connection;

use crate::sailfish::{
    Collection, DbusResult, Identifier, SailfishSecrets, SecretData, UiParams,
    ERR_COLLECTION_ALREADY_EXISTS, ERR_INVALID_COLLECTION, ERR_INVALID_SECRET,
    ERR_SECRET_ALREADY_EXISTS, RESULT_FAILED, SECRETS_PATH,
};

/// What the fake daemon holds and what it was asked.
#[derive(Default, Debug)]
pub struct FakeState {
    pub collections: Vec<String>,
    /// `(collection, name) -> data`.
    pub secrets: HashMap<(String, String), Vec<u8>>,
    /// The `app` filter value of each stored secret, `(collection, name)`.
    pub apps: HashMap<(String, String), String>,
    /// One line per call, for example
    /// `setSecret <name> <collection> <plugin> <mode> <Type>`.
    pub calls: Vec<String>,
    /// `setSecret` of a new secret under these names fails (simulates the
    /// daemon failing part-way through a replace).
    pub refuse_set: Vec<String>,
    /// `deleteSecret` of these names fails.
    pub refuse_delete: Vec<String>,
}

impl FakeState {
    /// The secret `name` in `collection`, if stored.
    pub fn secret(&self, collection: &str, name: &str) -> Option<&[u8]> {
        self.secrets
            .get(&(collection.to_string(), name.to_string()))
            .map(Vec::as_slice)
    }
}

struct Fake(Arc<Mutex<FakeState>>);

fn ok() -> DbusResult {
    (0, 0, String::new())
}

#[zbus::interface(name = "org.sailfishos.secrets")]
impl Fake {
    #[zbus(name = "createCollection")]
    fn create_collection(
        &self,
        name: String,
        storage: &str,
        encryption: &str,
        unlock: (i32,),
        access: (i32,),
    ) -> DbusResult {
        let mut d = self.0.lock().unwrap();
        d.calls.push(format!(
            "createCollection {name} {storage} {encryption} {} {}",
            unlock.0, access.0
        ));
        if d.collections.contains(&name) {
            return (
                RESULT_FAILED,
                ERR_COLLECTION_ALREADY_EXISTS,
                "exists".into(),
            );
        }
        d.collections.push(name);
        ok()
    }

    #[zbus(name = "setSecret")]
    fn set_secret(
        &self,
        secret: SecretData,
        ui: UiParams,
        mode: (i32,),
        addr: String,
    ) -> DbusResult {
        // Part of the upstream signature; the fake has no UI to show.
        let _ = (ui, addr);
        let mut d = self.0.lock().unwrap();
        let ((name, coll, plugin), data, filter) = secret;
        let text = |k: &str| -> String {
            filter
                .get(k)
                .and_then(|v| String::try_from(v.clone()).ok())
                .unwrap_or_default()
        };
        let (ty, app) = (text("Type"), text("app"));
        d.calls
            .push(format!("setSecret {name} {coll} {plugin} {} {ty}", mode.0));
        if !d.collections.contains(&coll) {
            return (
                RESULT_FAILED,
                ERR_INVALID_COLLECTION,
                "no collection".into(),
            );
        }
        let key = (coll, name);
        if d.secrets.contains_key(&key) {
            return (RESULT_FAILED, ERR_SECRET_ALREADY_EXISTS, "exists".into());
        }
        if d.refuse_set.contains(&key.1) {
            return (RESULT_FAILED, 99, "refused by the test".into());
        }
        d.apps.insert(key.clone(), app);
        d.secrets.insert(key, data);
        ok()
    }

    #[zbus(name = "getSecret")]
    fn get_secret(&self, id: Identifier, mode: (i32,), addr: String) -> (DbusResult, SecretData) {
        // Part of the upstream signature; the fake never prompts.
        let _ = (mode, addr);
        let mut d = self.0.lock().unwrap();
        d.calls.push(format!("getSecret {} {}", id.0, id.1));
        match d.secrets.get(&(id.1.clone(), id.0.clone())) {
            Some(v) => (ok(), (id, v.clone(), HashMap::new())),
            None => (
                (RESULT_FAILED, ERR_INVALID_SECRET, "no such secret".into()),
                (id, Vec::new(), HashMap::new()),
            ),
        }
    }

    #[zbus(name = "deleteSecret")]
    fn delete_secret(&self, id: Identifier, mode: (i32,), addr: String) -> DbusResult {
        // Part of the upstream signature; the fake never prompts.
        let _ = (mode, addr);
        let mut d = self.0.lock().unwrap();
        d.calls.push(format!("deleteSecret {} {}", id.0, id.1));
        if d.refuse_delete.contains(&id.0) {
            return (RESULT_FAILED, 99, "refused by the test".into());
        }
        match d.secrets.remove(&(id.1.clone(), id.0.clone())) {
            Some(_) => {
                d.apps.remove(&(id.1, id.0));
                ok()
            }
            None => (RESULT_FAILED, ERR_INVALID_SECRET, "no such secret".into()),
        }
    }
}

/// A client store wired to a fake daemon.
pub struct FakeDaemon {
    pub state: Arc<Mutex<FakeState>>,
    server: Option<Connection>,
}

impl FakeDaemon {
    /// Starts a fake daemon and returns it with a [`SailfishSecrets`] store
    /// for `collection` connected to it.
    ///
    /// # Panics
    ///
    /// If the socket pair or either end of the peer connection cannot be
    /// set up (a broken test host).
    // `unix_stream` is deprecated only because it clashes with zbus's tokio
    // feature, which this crate never enables.
    #[allow(deprecated)]
    pub fn start(collection: Collection) -> (SailfishSecrets, FakeDaemon) {
        let state = Arc::new(Mutex::new(FakeState::default()));
        let (a, b) = UnixStream::pair().expect("socket pair");
        let guid = zbus::Guid::generate();
        let server = std::thread::spawn({
            let state = state.clone();
            move || {
                zbus::blocking::connection::Builder::unix_stream(a)
                    .server(guid)
                    .expect("server")
                    .p2p()
                    .serve_at(SECRETS_PATH, Fake(state))
                    .expect("serve_at")
                    .build()
                    .expect("fake daemon")
            }
        });
        let client = zbus::blocking::connection::Builder::unix_stream(b)
            .p2p()
            .build()
            .expect("client");
        let server = server.join().expect("fake daemon thread");
        (
            SailfishSecrets::with_connection(collection, client),
            FakeDaemon {
                state,
                server: Some(server),
            },
        )
    }

    /// Simulates the daemon going away: the peer connection closes, so the
    /// client's next call fails with [`crate::Error::Unavailable`].
    pub fn stop(&mut self) {
        if let Some(c) = self.server.take() {
            let _ = c.close();
        }
    }

    /// Every call the daemon has received, oldest first.
    ///
    /// # Panics
    ///
    /// If a daemon method panicked while holding the state lock.
    pub fn calls(&self) -> Vec<String> {
        self.state.lock().unwrap().calls.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SecretStore;

    #[test]
    fn contract_over_fake_daemon() {
        let (store, daemon) = FakeDaemon::start(Collection::new("shoaltest", "shipwright-test"));
        crate::check_contract(&store, "shoaltest-a");
        store.put("kept", b"v").unwrap();
        let d = daemon.state.lock().unwrap();
        assert_eq!(
            d.calls[0],
            "createCollection shoaltest plugin.encryptedstorage.default plugin.encryptedstorage.default 0 0"
        );
        assert_eq!(
            d.calls
                .iter()
                .filter(|c| c.starts_with("createCollection"))
                .count(),
            1,
            "collection created once per connection"
        );
        assert!(
            d.calls
                .iter()
                .any(|c| c
                    == "setSecret shoaltest-a shoaltest plugin.encryptedstorage.default 0 Blob")
        );
        assert_eq!(d.secret("shoaltest", "kept"), Some(&b"v"[..]));
        assert_eq!(
            d.apps
                .get(&("shoaltest".into(), "kept".into()))
                .map(String::as_str),
            Some("shipwright-test")
        );
    }

    #[test]
    fn existing_collection_and_probe() {
        let (store, daemon) = FakeDaemon::start(Collection::new("c", "a"));
        daemon.state.lock().unwrap().collections.push("c".into());
        store.probe().unwrap();
        store.probe().unwrap();
        assert_eq!(daemon.calls().len(), 1, "probe caches the collection");
    }

    #[test]
    fn stopped_daemon_is_unavailable() {
        let (store, mut daemon) = FakeDaemon::start(Collection::new("c", "a"));
        store.put("x", b"1").unwrap();
        daemon.stop();
        let e = store.get("x").unwrap_err();
        assert!(e.is_unavailable(), "{e:?}");
    }

    #[test]
    fn interrupted_replace_never_loses_the_secret() {
        let (store, daemon) = FakeDaemon::start(Collection::new("c", "a"));
        store.put("x", b"old").unwrap();

        // Fails before anything is deleted: the old value stays.
        daemon.state.lock().unwrap().refuse_set = vec!["x_new".into()];
        assert!(store.put("x", b"new").is_err());
        assert_eq!(store.get("x").unwrap().unwrap().as_slice(), b"old");

        // Fails after the old value is gone: the pending copy is found.
        daemon.state.lock().unwrap().refuse_set = vec!["x".into()];
        assert!(store.put("x", b"new").is_err());
        assert_eq!(store.get("x").unwrap().unwrap().as_slice(), b"new");

        // The next put completes and tidies up; delete removes both names.
        daemon.state.lock().unwrap().refuse_set.clear();
        store.put("x", b"newer").unwrap();
        assert_eq!(store.get("x").unwrap().unwrap().as_slice(), b"newer");
        assert_eq!(daemon.state.lock().unwrap().secret("c", "x_new"), None);
        store.put("x", b"newest").unwrap();
        daemon.state.lock().unwrap().refuse_set = vec!["x".into()];
        assert!(store.put("x", b"lost?").is_err());
        daemon.state.lock().unwrap().refuse_set.clear();
        store.delete("x").unwrap();
        assert!(store.get("x").unwrap().is_none());
        assert!(daemon.state.lock().unwrap().secrets.is_empty());
    }

    #[test]
    fn a_failed_delete_never_brings_back_the_pending_copy() {
        let (store, daemon) = FakeDaemon::start(Collection::new("c", "a"));
        store.put("x", b"old").unwrap();
        // An interrupted replace leaves "new" under the pending name.
        daemon.state.lock().unwrap().refuse_set = vec!["x".into()];
        assert!(store.put("x", b"new").is_err());
        {
            let mut d = daemon.state.lock().unwrap();
            d.refuse_set.clear();
            d.secrets.insert(("c".into(), "x".into()), b"old".to_vec());
            d.secrets
                .insert(("c".into(), "x_new".into()), b"new".to_vec());
            d.refuse_delete = vec!["x_new".into()];
        }
        assert!(store.delete("x").is_err());
        // Nothing was removed, so nothing stale is resurrected either.
        assert_eq!(store.get("x").unwrap().unwrap().as_slice(), b"old");
        daemon.state.lock().unwrap().refuse_delete = vec!["x".into()];
        assert!(store.delete("x").is_err());
        // The pending copy went first; the main name is still there.
        assert_eq!(store.get("x").unwrap().unwrap().as_slice(), b"old");
        assert_eq!(daemon.state.lock().unwrap().secret("c", "x_new"), None);
    }
}
