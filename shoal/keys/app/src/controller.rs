// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

//! Everything the UI object does, without Qt, so it can be unit tested.
//!
//! The bridge (`crate::bridge`) owns one [`Controller`]. Calls that run
//! Argon2 (create, unlock, save, export, import of KDBX, change password)
//! take a [`Shared`] handle and run on a worker thread; the rest are quick
//! and run on the GUI thread. Values cross to QML as JSON strings, which
//! keeps the bridge small and works the same on Qt 6.4 and 6.8.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, TryLockError};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use shoal_keys_core::autolock::{boottime, AutoLock, AutoLockSettings, LockReason};
use shoal_keys_core::generator::{
    self, estimate_entropy, PassphraseOptions, PasswordOptions, Strength,
};
use shoal_keys_core::import::{self, Format};
use shoal_keys_core::licence::{self, LicenceState};
use shoal_keys_core::search::host_of;
use shoal_keys_core::{CompositeKey, CustomField, EntryData, KeyStore, OpenVault, VaultStore};

/// Sailjail's per-app directory names (the desktop file's
/// `OrganizationName` and `ApplicationName`).
pub const ORG_NAME: &str = "org.shipwright";
pub const APP_NAME: &str = "shoal-keys";

/// User settings, `$XDG_CONFIG_HOME/org.shipwright/shoal-keys/settings.json`
/// ([`ORG_NAME`], [`APP_NAME`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Seconds before a copied secret is cleared from the clipboard; 0 never.
    pub clipboard_clear_secs: u32,
    /// Seconds without input before locking; 0 never.
    pub idle_lock_secs: u32,
    /// Seconds in the background before locking; -1 never, 0 at once.
    pub background_lock_secs: i32,
    pub lock_on_screen_lock: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            clipboard_clear_secs: 30,
            idle_lock_secs: 300,
            background_lock_secs: 30,
            lock_on_screen_lock: true,
        }
    }
}

impl Settings {
    pub fn load(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice::<Settings>(&b).ok())
            .map(Settings::normalised)
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let body = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        shoal_keys_core::store::write_atomic(path, &body)
    }

    #[must_use]
    pub fn normalised(mut self) -> Self {
        self.clipboard_clear_secs = self.clipboard_clear_secs.min(600);
        self.idle_lock_secs = self.idle_lock_secs.min(24 * 3600);
        self.background_lock_secs = self.background_lock_secs.clamp(-1, 24 * 3600);
        self
    }

    pub fn autolock(&self) -> AutoLockSettings {
        AutoLockSettings {
            idle_timeout: (self.idle_lock_secs > 0)
                .then(|| Duration::from_secs(self.idle_lock_secs.into())),
            background_timeout: u64::try_from(self.background_lock_secs)
                .ok()
                .map(Duration::from_secs),
            lock_on_screen_lock: self.lock_on_screen_lock,
        }
    }
}

/// Where things live.
#[derive(Debug, Clone)]
pub struct Paths {
    pub data: PathBuf,
    pub config: PathBuf,
    /// Where import candidates are listed and exports are written.
    pub downloads: PathBuf,
}

impl Paths {
    /// Sailjail layout: `~/.local/share/<org>/<app>` and
    /// `~/.config/<org>/<app>`. `SHOAL_KEYS_DIR` puts both in one directory
    /// (tests, development).
    pub fn from_env() -> Self {
        let home = std::env::var_os("HOME").map_or_else(|| PathBuf::from("."), PathBuf::from);
        let xdg = |var: &str, fallback: &str| {
            std::env::var_os(var)
                .filter(|v| !v.is_empty())
                .map_or_else(|| home.join(fallback), PathBuf::from)
        };
        let downloads = std::env::var_os("SHOAL_KEYS_DOWNLOADS")
            .map_or_else(|| home.join("Downloads"), PathBuf::from);
        if let Some(dir) = std::env::var_os("SHOAL_KEYS_DIR").filter(|v| !v.is_empty()) {
            let dir = PathBuf::from(dir);
            return Self {
                data: dir.clone(),
                config: dir,
                downloads,
            };
        }
        Self {
            data: xdg("XDG_DATA_HOME", ".local/share")
                .join(ORG_NAME)
                .join(APP_NAME),
            config: xdg("XDG_CONFIG_HOME", ".config")
                .join(ORG_NAME)
                .join(APP_NAME),
            downloads,
        }
    }
}

/// The key store for this run. `SHOAL_KEYS_INSECURE_KEYSTORE=<dir>` selects
/// the plain-file development store (no protection); otherwise Sailfish
/// Secrets.
pub fn keystore_from_env() -> Arc<dyn KeyStore> {
    if let Some(dir) = std::env::var_os("SHOAL_KEYS_INSECURE_KEYSTORE").filter(|v| !v.is_empty()) {
        eprintln!(
            "shipwright-shoal-keys: WARNING: insecure development key store in {}",
            PathBuf::from(&dir).display()
        );
        return Arc::new(shoal_keys_core::PlainFileKeyStore::new(PathBuf::from(dir)));
    }
    Arc::new(shoal_keys_core::sailfish::SailfishSecretsKeyStore::new())
}

/// Why an operation failed. `Display` is the text the UI shows.
#[derive(Debug, thiserror::Error)]
pub enum KeysError {
    #[error("Busy, try again in a moment.")]
    Busy,
    #[error("The vault is locked.")]
    Locked,
    #[error("The current password is wrong.")]
    WrongPassword,
    #[error("Unrecognised file. Supported: KDBX, Bitwarden JSON, 1Password 1PUX or CSV, Chrome or Firefox CSV.")]
    Unrecognised,
    #[error("The entry is empty.")]
    EmptyEntry,
    #[error("Bad entry: {0}")]
    BadEntry(#[from] serde_json::Error),
    #[error("{}: {source}", path.display())]
    File {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error(transparent)]
    Core(#[from] shoal_keys_core::Error),
    #[error("Could not start a worker thread.")]
    NoWorker,
    #[error("That login is not for this site.")]
    NotForSite,
}

impl KeysError {
    fn file(path: &Path) -> impl FnOnce(std::io::Error) -> KeysError + '_ {
        move |source| KeysError::File {
            path: path.to_owned(),
            source,
        }
    }
}

pub type KeysResult<T> = Result<T, KeysError>;

/// State shared with worker threads.
///
/// Locking never waits on the vault mutex from the GUI thread: a worker
/// holds that mutex for a whole Argon2 save, export or import (seconds on a
/// phone). `lock` instead sets `lock_requested`, a lock-free flag that
/// makes the vault read as locked at once, and drops the vault right away if
/// the mutex is free; otherwise the worker drops it as soon as it finishes
/// (after completing the save, so no edit is lost).
#[derive(Clone)]
pub struct Shared {
    pub store: Arc<VaultStore>,
    pub open: Arc<Mutex<Option<OpenVault>>>,
    lock_requested: Arc<AtomicBool>,
}

impl Shared {
    pub fn new(store: VaultStore) -> Self {
        Self {
            store: Arc::new(store),
            open: Arc::new(Mutex::new(None)),
            lock_requested: Arc::new(AtomicBool::new(false)),
        }
    }

    /// GUI thread, never blocks. The vault reads as locked from now on.
    pub fn request_lock(&self) {
        self.lock_requested.store(true, Ordering::SeqCst);
        match self.open.try_lock() {
            Ok(mut g) => self.honour_lock(&mut g),
            Err(TryLockError::Poisoned(p)) => self.honour_lock(&mut p.into_inner()),
            // A worker has it and drops the vault when it is done.
            Err(TryLockError::WouldBlock) => {}
        }
    }

    pub fn lock_pending(&self) -> bool {
        self.lock_requested.load(Ordering::SeqCst)
    }

    /// Drops the vault if a lock was requested. Called with the mutex held.
    /// With no vault open the request stays pending: an unlock, create or
    /// recover runs Argon2 without the mutex, and the vault it then opens
    /// must be dropped too (`set_open`). The bridge clears the request when
    /// it starts such an opening.
    fn honour_lock(&self, g: &mut Option<OpenVault>) {
        if g.is_some() && self.lock_requested.swap(false, Ordering::SeqCst) {
            *g = None;
        }
    }

    /// Worker: runs `f` on the open vault, then honours a lock requested
    /// meanwhile.
    fn with_open<R>(&self, f: impl FnOnce(&mut OpenVault) -> KeysResult<R>) -> KeysResult<R> {
        let mut g = self.open.lock().unwrap_or_else(PoisonError::into_inner);
        let r = match g.as_mut() {
            Some(v) => f(v),
            None => Err(KeysError::Locked),
        };
        self.honour_lock(&mut g);
        r
    }

    /// Worker: makes `v` the open vault, unless a lock was asked for while
    /// it was being opened.
    fn set_open(&self, v: OpenVault) {
        let mut g = self.open.lock().unwrap_or_else(PoisonError::into_inner);
        *g = Some(v);
        self.honour_lock(&mut g);
    }

    /// GUI thread: the open vault for a quick read or edit, without waiting.
    fn vault(&self) -> KeysResult<MutexGuard<'_, Option<OpenVault>>> {
        if self.lock_pending() {
            return Err(KeysError::Locked);
        }
        match self.open.try_lock() {
            Ok(g) => Ok(g),
            Err(TryLockError::WouldBlock) => Err(KeysError::Busy),
            Err(TryLockError::Poisoned(p)) => Ok(p.into_inner()),
        }
    }

    /// GUI thread, before starting create, unlock or recover: an earlier
    /// lock request must not cancel the new opening.
    pub fn clear_lock_request(&self) {
        self.lock_requested.store(false, Ordering::SeqCst);
    }

    /// Worker: create a new vault and keep it open.
    pub fn create(&self, name: &str, password: &str) -> KeysResult<()> {
        let v = self.store.create(name, password)?;
        self.set_open(v);
        Ok(())
    }

    /// Worker: unlock.
    pub fn unlock(&self, password: &str) -> KeysResult<()> {
        let v = self.store.unlock(password)?;
        self.set_open(v);
        Ok(())
    }

    /// Worker: recover with the recovery key file.
    pub fn recover(&self, password: &str, keyfile: &Path) -> KeysResult<()> {
        let bytes =
            zeroize::Zeroizing::new(std::fs::read(keyfile).map_err(KeysError::file(keyfile))?);
        let v = self.store.recover(password, &bytes)?;
        self.set_open(v);
        Ok(())
    }

    /// Worker: write the vault to disk.
    pub fn save(&self) -> KeysResult<()> {
        self.with_open(|v| Ok(v.save()?))
    }

    /// Worker: import a file and save. Returns a summary line.
    pub fn import_file(&self, path: &Path, password: &str) -> KeysResult<String> {
        let bytes = zeroize::Zeroizing::new(std::fs::read(path).map_err(KeysError::file(path))?);
        let format = import::detect(&bytes).ok_or(KeysError::Unrecognised)?;
        let key = (!password.is_empty()).then(|| CompositeKey::password(password));
        let parsed = import::import(format, &bytes, key.as_ref())?;
        let r = self.with_open(|v| {
            let r = v.vault.merge_imported(&parsed.items);
            v.save()?;
            Ok(r)
        })?;
        let mut msg = format!("{}: {} added", format.label(), r.added);
        if r.duplicates > 0 {
            let _ = write!(msg, ", {} already present", r.duplicates);
        }
        if r.skipped_blank + parsed.skipped > 0 {
            let _ = write!(msg, ", {} skipped", r.skipped_blank + parsed.skipped);
        }
        msg.push('.');
        if format != Format::Kdbx {
            msg.push_str(" Delete the unencrypted export file now.");
        }
        Ok(msg)
    }

    /// Worker: portable KDBX copy protected by `password` alone.
    pub fn export_kdbx(&self, path: &Path, password: &str) -> KeysResult<()> {
        let bytes = self.with_open(|v| Ok(v.export_kdbx(password)?))?;
        shoal_keys_core::store::write_atomic(path, &bytes).map_err(KeysError::file(path))
    }

    /// Worker: change the master password.
    pub fn change_password(&self, current: &str, new: &str) -> KeysResult<()> {
        // Check the current password by unlocking a second copy.
        self.store
            .unlock(current)
            .map_err(|_| KeysError::WrongPassword)?;
        self.with_open(|v| Ok(v.change_password(new)?))
    }
}

/// Files in `downloads` that look importable, as JSON
/// `[{path, name, format, needsPassword}]`. Opens and reads 4 KiB of each
/// file: call it on a worker thread.
pub fn import_candidates(downloads: &Path) -> String {
    let mut rows = Vec::new();
    if let Ok(rd) = std::fs::read_dir(downloads) {
        let mut files: Vec<PathBuf> = rd
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_file())
            .collect();
        files.sort();
        for p in files.into_iter().take(MAX_IMPORT_PROBES) {
            let Ok(meta) = std::fs::metadata(&p) else {
                continue;
            };
            if meta.len() > 64 * 1024 * 1024 {
                continue;
            }
            let mut head = vec![0u8; 4096];
            let n = std::fs::File::open(&p)
                .and_then(|mut f| std::io::Read::read(&mut f, &mut head))
                .unwrap_or(0);
            head.truncate(n);
            // CSV headers and JSON starts fit in 4 KiB; detect() needs
            // only the header row for CSV. Cut at the last newline so a
            // partial row cannot confuse the CSV reader.
            let probe = match head.iter().rposition(|b| *b == b'\n') {
                Some(i)
                    if !head.starts_with(b"PK")
                        && head[..4.min(head.len())] != [0x03, 0xd9, 0xa2, 0x9a] =>
                {
                    &head[..=i]
                }
                _ => &head[..],
            };
            if let Some(f) = import::detect(probe) {
                rows.push(json!({
                    "path": p.to_string_lossy(),
                    "name": p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
                    "format": f.label(),
                    "needsPassword": f.needs_password(),
                }));
            }
        }
    }
    Value::Array(rows).to_string()
}

/// At most this many files in Downloads are opened to look for exports.
pub const MAX_IMPORT_PROBES: usize = 500;

/// The GUI-side controller.
pub struct Controller {
    pub shared: Shared,
    pub paths: Paths,
    pub settings: Settings,
    autolock: AutoLock,
    pub licence: LicenceState,
    licence_keys: reef_licence::KeySet,
    /// Goes up whenever the licence changes on the GUI thread, so a Reef
    /// hand-off started earlier does not overwrite a newer choice.
    licence_generation: u64,
    /// The last value copied, to clear only what we put there.
    pub copied: Option<zeroize::Zeroizing<String>>,
    pub last_lock_reason: Option<LockReason>,
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

fn entry_row(e: &EntryData) -> Value {
    json!({
        "id": e.id,
        "title": if e.title.trim().is_empty() { "(untitled)" } else { e.title.as_str() },
        "username": e.username,
        "host": e.urls.first().map(|u| host_of(u)).unwrap_or_default(),
        "group": e.group,
        "hasTotp": !e.otp.trim().is_empty(),
    })
}

fn entry_detail(e: &EntryData, history_len: usize) -> Value {
    json!({
        "id": e.id,
        "title": e.title,
        "username": e.username,
        "password": e.password,
        "urls": e.urls,
        "notes": e.notes,
        "otp": e.otp,
        "hasTotp": !e.otp.trim().is_empty(),
        "fields": e.fields.iter().map(|f| json!({"name": f.name, "value": f.value, "protected": f.protected})).collect::<Vec<_>>(),
        "tags": e.tags,
        "group": e.group,
        "modified": e.modified,
        "historyCount": history_len,
    })
}

fn strength_json(bits: f64) -> Value {
    let s = Strength::from_bits(bits);
    json!({"bits": (bits * 10.0).round() / 10.0, "label": s.label(), "level": s as i32})
}

impl Controller {
    pub fn new(
        paths: Paths,
        keystore: Arc<dyn KeyStore>,
        kdf: Option<shoal_keys_core::KdfParams>,
    ) -> Self {
        let settings = Settings::load(&paths.config.join("settings.json"));
        let mut store = VaultStore::new(&paths.data, keystore);
        if let Some(k) = kdf {
            store = store.with_kdf(k);
        }
        let licence_keys = licence::embedded_keys();
        let licence = licence::load(&paths.data, &licence_keys, now_unix());
        Self {
            shared: Shared::new(store),
            autolock: AutoLock::new(settings.autolock(), boottime()),
            settings,
            paths,
            licence,
            licence_keys,
            licence_generation: 0,
            copied: None,
            last_lock_reason: None,
        }
    }

    /// What a worker needs for the Reef licence hand-off
    /// ([`adopt_reef_licence`]), and the licence generation to hand back to
    /// [`Controller::apply_reef_licence`].
    pub fn licence_handoff(&self) -> (PathBuf, reef_licence::KeySet, u64) {
        (
            self.paths.data.clone(),
            self.licence_keys.clone(),
            self.licence_generation,
        )
    }

    /// The hand-off's result, applied only if the licence did not change
    /// (installed, removed) since it started. Returns whether it applied.
    pub fn apply_reef_licence(&mut self, generation: u64, state: LicenceState) -> bool {
        if generation != self.licence_generation {
            return false;
        }
        self.licence = state;
        true
    }

    /// Replace the trusted licence keys (tests).
    pub fn set_licence_keys(&mut self, keys: reef_licence::KeySet) {
        self.licence_generation += 1;
        self.licence_keys = keys;
        self.licence = licence::load(&self.paths.data, &self.licence_keys, now_unix());
    }

    pub fn vault_exists(&self) -> bool {
        self.shared.store.exists()
    }

    /// Never blocks. A requested lock counts as locked straight away.
    pub fn is_locked(&self) -> bool {
        if self.shared.lock_pending() {
            return true;
        }
        match self.shared.open.try_lock() {
            Ok(g) => g.is_none(),
            // A worker holds it: it is saving an open vault, or unlocking.
            Err(_) => false,
        }
    }

    pub fn keystore_name(&self) -> &'static str {
        self.shared.store.keystore_name()
    }

    /// Locks: drops the decrypted vault (zeroising keys and protected
    /// values), now or, if a worker is saving, as soon as it finishes. Never
    /// waits on the GUI thread.
    pub fn lock(&mut self) {
        self.shared.request_lock();
    }

    pub fn entry_count(&self) -> i32 {
        self.shared
            .vault()
            .ok()
            .and_then(|g| {
                g.as_ref()
                    .map(|v| i32::try_from(v.vault.len()).unwrap_or(i32::MAX))
            })
            .unwrap_or(0)
    }

    pub fn vault_name(&self) -> String {
        self.shared
            .vault()
            .ok()
            .and_then(|g| g.as_ref().map(|v| v.vault.name().to_string()))
            .unwrap_or_default()
    }

    /// Entries matching `query` as a JSON array of list rows (no secrets).
    pub fn search(&self, query: &str) -> String {
        let rows: Vec<Value> = match self.shared.vault() {
            Ok(g) => g
                .as_ref()
                .map(|v| v.vault.search(query).iter().map(entry_row).collect())
                .unwrap_or_default(),
            Err(_) => Vec::new(),
        };
        Value::Array(rows).to_string()
    }

    /// One entry with its secrets, as JSON (`{}` if absent).
    pub fn entry(&self, id: &str) -> String {
        let Ok(g) = self.shared.vault() else {
            return "{}".into();
        };
        let Some(v) = g.as_ref() else {
            return "{}".into();
        };
        match v.vault.entry(id) {
            Some(e) => entry_detail(&e, v.vault.history(id).len()).to_string(),
            None => "{}".into(),
        }
    }

    /// Earlier versions, newest first: `[{modified, password, username}]`.
    pub fn history(&self, id: &str) -> String {
        let Ok(g) = self.shared.vault() else {
            return "[]".into();
        };
        let rows: Vec<Value> = g
            .as_ref()
            .map(|v| {
                v.vault
                    .history(id)
                    .iter()
                    .map(|h| json!({"modified": h.modified, "password": h.password, "username": h.username, "title": h.title}))
                    .collect()
            })
            .unwrap_or_default();
        Value::Array(rows).to_string()
    }

    /// Adds (empty `id`) or updates an entry from the edit page's JSON.
    /// Returns the entry id. The caller then saves on a worker.
    pub fn put_entry(&mut self, json_text: &str) -> KeysResult<String> {
        let input: Value = serde_json::from_str(json_text)?;
        let text = |k: &str| {
            input
                .get(k)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let list = |k: &str| -> Vec<String> {
            input
                .get(k)
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str())
                        .map(|x| x.trim().to_string())
                        .filter(|x| !x.is_empty())
                        .collect()
                })
                .unwrap_or_default()
        };
        let mut e = EntryData::default();
        e.id = text("id");
        e.title = text("title");
        e.username = text("username");
        e.password = text("password");
        e.urls = list("urls");
        e.notes = text("notes");
        e.otp = text("otp").trim().to_string();
        e.tags = list("tags");
        e.group = text("group");
        e.fields = input
            .get("fields")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .map(|f| CustomField {
                        name: f
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .trim()
                            .to_string(),
                        value: f
                            .get("value")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        protected: f.get("protected").and_then(Value::as_bool).unwrap_or(false),
                    })
                    .filter(|f| !f.name.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        if !e.otp.is_empty() {
            shoal_keys_core::totp::Totp::parse(&e.otp)?;
        }
        if e.is_blank() {
            return Err(KeysError::EmptyEntry);
        }
        let mut g = self.shared.vault()?;
        let v = g.as_mut().ok_or(KeysError::Locked)?;
        if e.id.is_empty() {
            Ok(v.vault.add(&e))
        } else {
            v.vault.update(&e)?;
            Ok(e.id.clone())
        }
    }

    /// Every entry, with secrets, for autofill matching (GUI thread).
    pub fn autofill_entries(&self) -> KeysResult<Vec<EntryData>> {
        let g = self.shared.vault()?;
        Ok(g.as_ref().ok_or(KeysError::Locked)?.vault.entries())
    }

    /// The user name and password of entry `id`, if it is a login for
    /// `origin`.
    pub fn autofill_credentials(&self, origin: &str, id: &str) -> KeysResult<(String, String)> {
        let g = self.shared.vault()?;
        let v = g.as_ref().ok_or(KeysError::Locked)?;
        let e = v.vault.entry(id).ok_or(KeysError::NotForSite)?;
        if !crate::autofill::entry_matches_origin(&e, origin) {
            return Err(KeysError::NotForSite);
        }
        Ok((e.username.clone(), e.password.clone()))
    }

    /// Saves a login from a page: updates the password of the entry for
    /// this site and user name, or adds one. Returns (id, updated). The
    /// caller then saves on a worker.
    pub fn autofill_save(
        &mut self,
        origin: &str,
        username: &str,
        password: &str,
    ) -> KeysResult<(String, bool)> {
        let mut g = self.shared.vault()?;
        let v = g.as_mut().ok_or(KeysError::Locked)?;
        let entries = v.vault.entries();
        if let Some(id) = crate::autofill::save_target(&entries, origin, username) {
            let mut e = v.vault.entry(&id).ok_or(KeysError::NotForSite)?;
            password.clone_into(&mut e.password);
            v.vault.update(&e)?;
            Ok((id, true))
        } else {
            let e = crate::autofill::new_entry(origin, username, password);
            if e.is_blank() {
                return Err(KeysError::EmptyEntry);
            }
            Ok((v.vault.add(&e), false))
        }
    }

    pub fn delete_entry(&mut self, id: &str) -> KeysResult<()> {
        let mut g = self.shared.vault()?;
        let v = g.as_mut().ok_or(KeysError::Locked)?;
        Ok(v.vault.delete(id)?)
    }

    /// `{code, remaining, period}` for an entry, or `{}`.
    pub fn totp(&self, id: &str) -> String {
        let Ok(g) = self.shared.vault() else {
            return "{}".into();
        };
        let Some(e) = g.as_ref().and_then(|v| v.vault.entry(id)) else {
            return "{}".into();
        };
        match e.totp() {
            Some(Ok(t)) => {
                let c = t.now();
                json!({"code": c.code, "remaining": c.remaining, "period": c.period}).to_string()
            }
            Some(Err(err)) => json!({"error": err.to_string()}).to_string(),
            None => "{}".into(),
        }
    }

    /// A random password for `opts`, its length clamped to 4..=128, as the
    /// generator page's JSON.
    pub fn generate_password(&self, opts: PasswordOptions) -> String {
        let opts = PasswordOptions {
            length: opts.length.clamp(4, 128),
            ..opts
        };
        match generator::password(&opts) {
            Ok(g) => {
                let mut v = strength_json(g.entropy_bits);
                v["value"] = Value::String(g.value.to_string());
                v.to_string()
            }
            Err(e) => json!({"error": e.to_string()}).to_string(),
        }
    }

    pub fn generate_passphrase(
        &self,
        words: i32,
        separator: &str,
        capitalize: bool,
        add_digit: bool,
    ) -> String {
        let opts = PassphraseOptions {
            words: usize::try_from(words.clamp(3, 20)).unwrap_or(3),
            separator: separator.to_string(),
            capitalize,
            add_digit,
        };
        match generator::passphrase(&opts) {
            Ok(g) => {
                let mut v = strength_json(g.entropy_bits);
                v["value"] = Value::String(g.value.to_string());
                v.to_string()
            }
            Err(e) => json!({"error": e.to_string()}).to_string(),
        }
    }

    pub fn strength(&self, password: &str) -> String {
        strength_json(estimate_entropy(password)).to_string()
    }

    /// Default export path: `<downloads>/shoal-keys-<date>.kdbx`.
    pub fn default_export_path(&self) -> String {
        let days = now_unix() / 86_400;
        let (y, m, d) = civil_from_days(days);
        self.paths
            .downloads
            .join(format!("shoal-keys-{y:04}-{m:02}-{d:02}.kdbx"))
            .to_string_lossy()
            .to_string()
    }

    /// Writes the recovery key file to `path` (mode 0600).
    pub fn write_recovery_key(&self, path: &Path) -> KeysResult<()> {
        let g = self.shared.vault()?;
        let v = g.as_ref().ok_or(KeysError::Locked)?;
        let xml = v.recovery_keyfile();
        shoal_keys_core::store::write_atomic(path, xml.as_bytes()).map_err(KeysError::file(path))
    }

    pub fn set_settings(&mut self, s: Settings) -> KeysResult<()> {
        let s = s.normalised();
        self.autolock.settings = s.autolock();
        let path = self.paths.config.join("settings.json");
        s.save(&path).map_err(KeysError::file(&path))?;
        self.settings = s;
        Ok(())
    }

    pub fn touch(&mut self) {
        self.autolock.touch(boottime());
    }

    pub fn set_active(&mut self, active: bool) {
        self.autolock.set_active(active, boottime());
    }

    pub fn set_screen_locked(&mut self, locked: bool) {
        self.autolock.set_screen_locked(locked);
    }

    /// Locks if a timer ran out. Returns true when it locked.
    pub fn check_autolock(&mut self) -> bool {
        self.check_autolock_at(boottime())
    }

    pub fn check_autolock_at(&mut self, now: Duration) -> bool {
        if self.is_locked() {
            return false;
        }
        if let Some(reason) = self.autolock.should_lock(now) {
            self.lock();
            self.last_lock_reason = Some(reason);
            // Start the next unlock with fresh timers.
            self.autolock = AutoLock::new(self.settings.autolock(), now);
            return true;
        }
        false
    }

    pub fn idle_remaining_secs(&self) -> i32 {
        self.autolock
            .idle_remaining(boottime())
            .map_or(-1, |d| i32::try_from(d.as_secs()).unwrap_or(i32::MAX))
    }

    pub fn lock_reason_text(&self) -> &'static str {
        match self.last_lock_reason {
            Some(LockReason::Idle) => "Locked after inactivity.",
            Some(LockReason::Background) => "Locked while in the background.",
            Some(LockReason::ScreenLock) => "Locked with the device.",
            None => "",
        }
    }

    pub fn install_licence(&mut self, token: &str) -> KeysResult<()> {
        self.licence_generation += 1;
        self.licence = licence::install(&self.paths.data, token, &self.licence_keys, now_unix())?;
        Ok(())
    }

    pub fn remove_licence(&mut self) -> KeysResult<()> {
        self.licence_generation += 1;
        licence::remove(&self.paths.data)?;
        self.licence = LicenceState::Free;
        Ok(())
    }

    /// `[{id, title, issues: [label]}]`, or `{"error": ...}` when unlicensed.
    pub fn health_report(&self) -> String {
        let Ok(g) = self.shared.vault() else {
            return json!({"error": "Busy"}).to_string();
        };
        let Some(v) = g.as_ref() else {
            return json!({"error": "The vault is locked."}).to_string();
        };
        match licence::health_report(&self.licence, &v.vault.entries(), now_unix()) {
            Ok(items) => Value::Array(
                items
                    .iter()
                    .map(|h| json!({"id": h.id, "title": h.title, "issues": h.issues.iter().map(|i| i.label()).collect::<Vec<_>>()}))
                    .collect(),
            )
            .to_string(),
            Err(e) => json!({"error": e.to_string()}).to_string(),
        }
    }
}

/// Worker: the Reef licence hand-off. If no valid licence is stored, take
/// the one Reef holds (session bus, with activation; Sailjail permission
/// `ShipwrightLicences`). Pasting in Settings stays the fallback. Blocking:
/// never on the GUI thread.
pub fn adopt_reef_licence(data: &Path, keys: &reef_licence::KeySet) -> LicenceState {
    licence::adopt_from_reef(data, keys, now_unix())
}

/// Days since 1970-01-01 to (year, month, day), proleptic Gregorian
/// (Howard Hinnant's algorithm).
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoal_keys_core::{KdfParams, MemoryKeyStore};

    fn tempdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("shoal-keys-app-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn controller(dir: &Path) -> Controller {
        let paths = Paths {
            data: dir.join("data"),
            config: dir.join("config"),
            downloads: dir.join("dl"),
        };
        Controller::new(
            paths,
            Arc::new(MemoryKeyStore::new()),
            Some(KdfParams::insecure_for_tests()),
        )
    }

    #[test]
    fn create_edit_search_lock() {
        let dir = tempdir("flow");
        let mut c = controller(&dir);
        assert!(!c.vault_exists());
        assert!(c.is_locked());
        c.shared.create("Mine", "pw").unwrap();
        assert!(!c.is_locked());
        let id = c
            .put_entry(r#"{"title":"GitHub","username":"octo","password":"x","urls":["https://github.com/login",""],"otp":"JBSWY3DPEHPK3PXP","fields":[{"name":"PIN","value":"1","protected":true}],"tags":["dev"]}"#)
            .unwrap();
        c.shared.save().unwrap();
        let rows: Value = serde_json::from_str(&c.search("git")).unwrap();
        assert_eq!(rows[0]["host"], "github.com");
        assert!(
            rows[0].get("password").is_none(),
            "list rows carry no secrets"
        );
        let e: Value = serde_json::from_str(&c.entry(&id)).unwrap();
        assert_eq!(e["password"], "x");
        assert_eq!(e["urls"].as_array().unwrap().len(), 1);
        let t: Value = serde_json::from_str(&c.totp(&id)).unwrap();
        assert_eq!(t["code"].as_str().unwrap().len(), 6);

        let mut upd = e.clone();
        upd["password"] = "y".into();
        c.put_entry(&upd.to_string()).unwrap();
        let h: Value = serde_json::from_str(&c.history(&id)).unwrap();
        assert_eq!(h[0]["password"], "x");
        assert!(c.put_entry(r#"{"title":"bad","otp":"!!"}"#).is_err());
        assert!(c.put_entry("{}").is_err());

        c.lock();
        assert!(c.is_locked());
        assert_eq!(c.search(""), "[]");
        assert!(c.shared.unlock("nope").is_err());
        c.shared.unlock("pw").unwrap();
        assert_eq!(c.entry_count(), 1);
        c.delete_entry(&id).unwrap();
        assert_eq!(c.entry_count(), 0);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn autofill_only_for_the_site() {
        let dir = tempdir("autofill");
        let mut c = controller(&dir);
        assert!(matches!(c.autofill_entries(), Err(KeysError::Locked)));
        c.shared.create("Mine", "pw").unwrap();
        let id = c
            .put_entry(r#"{"title":"GitHub","username":"octo","password":"x","urls":["https://github.com/login"]}"#)
            .unwrap();
        assert_eq!(
            c.autofill_credentials("https://github.com", &id).unwrap(),
            ("octo".to_string(), "x".to_string())
        );
        assert!(matches!(
            c.autofill_credentials("https://evil.example", &id),
            Err(KeysError::NotForSite)
        ));
        // Saving the same user name updates; another adds an entry.
        assert!(
            c.autofill_save("https://github.com", "octo", "y")
                .unwrap()
                .1
        );
        assert_eq!(
            c.autofill_credentials("https://github.com", &id).unwrap().1,
            "y"
        );
        let (new_id, updated) = c.autofill_save("https://shop.example", "me", "z").unwrap();
        assert!(!updated);
        assert_eq!(
            c.autofill_credentials("https://shop.example", &new_id)
                .unwrap()
                .0,
            "me"
        );
        assert_eq!(
            crate::autofill::logins_for(&c.autofill_entries().unwrap(), "https://github.com").len(),
            1
        );
        c.lock();
        assert!(c.autofill_credentials("https://github.com", &id).is_err());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn autolock_idle_and_background() {
        let dir = tempdir("lock");
        let mut c = controller(&dir);
        c.shared.create("V", "pw").unwrap();
        let t0 = boottime();
        c.autolock = AutoLock::new(c.settings.autolock(), t0);
        assert!(!c.check_autolock_at(t0 + Duration::from_secs(10)));
        assert!(c.check_autolock_at(t0 + Duration::from_secs(301)));
        assert!(c.is_locked());
        assert_eq!(c.lock_reason_text(), "Locked after inactivity.");

        c.shared.unlock("pw").unwrap();
        let t1 = t0 + Duration::from_secs(400);
        c.autolock = AutoLock::new(c.settings.autolock(), t1);
        c.autolock.set_active(false, t1);
        assert!(!c.check_autolock_at(t1 + Duration::from_secs(29)));
        assert!(c.check_autolock_at(t1 + Duration::from_secs(30)));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn settings_round_trip_and_clamp() {
        let dir = tempdir("settings");
        let mut c = controller(&dir);
        c.set_settings(Settings {
            clipboard_clear_secs: 9999,
            idle_lock_secs: 0,
            background_lock_secs: -5,
            lock_on_screen_lock: false,
        })
        .unwrap();
        let back = Settings::load(&dir.join("config/settings.json"));
        assert_eq!(back.clipboard_clear_secs, 600);
        assert_eq!(back.background_lock_secs, -1);
        assert_eq!(back.autolock().idle_timeout, None);
        assert_eq!(back.autolock().background_timeout, None);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn import_candidates_and_import_export() {
        let dir = tempdir("import");
        let mut c = controller(&dir);
        std::fs::create_dir_all(dir.join("dl")).unwrap();
        std::fs::write(
            dir.join("dl/chrome.csv"),
            "name,url,username,password,note\nSite,https://s.example,u,p,\n",
        )
        .unwrap();
        std::fs::write(dir.join("dl/notes.txt"), "hello\n").unwrap();
        let cands: Value = serde_json::from_str(&import_candidates(&c.paths.downloads)).unwrap();
        assert_eq!(cands.as_array().unwrap().len(), 1);
        assert_eq!(cands[0]["format"], "Chrome (CSV)");

        c.shared.create("V", "pw").unwrap();
        let msg = c
            .shared
            .import_file(&dir.join("dl/chrome.csv"), "")
            .unwrap();
        assert!(msg.starts_with("Chrome (CSV): 1 added"), "{msg}");
        let again = c
            .shared
            .import_file(&dir.join("dl/chrome.csv"), "")
            .unwrap();
        assert!(again.contains("1 already present"), "{again}");

        let out = dir.join("dl/export.kdbx");
        c.shared.export_kdbx(&out, "share").unwrap();
        let cands: Value = serde_json::from_str(&import_candidates(&c.paths.downloads)).unwrap();
        assert!(cands
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["format"] == "KeePass (KDBX)"));
        let v = shoal_keys_core::Vault::open(
            &std::fs::read(&out).unwrap(),
            &CompositeKey::password("share"),
        )
        .unwrap();
        assert_eq!(v.len(), 1);

        c.write_recovery_key(&dir.join("dl/recovery.keyx")).unwrap();
        c.shared.change_password("pw", "new").unwrap();
        assert!(c.shared.change_password("wrong", "x").is_err());
        c.lock();
        assert!(c.shared.unlock("new").is_ok());
        assert!(Path::new(&c.default_export_path())
            .extension()
            .is_some_and(|e| e == "kdbx"));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn generator_and_licence_gate() {
        let dir = tempdir("gen");
        let mut c = controller(&dir);
        let p: Value = serde_json::from_str(&c.generate_password(PasswordOptions {
            length: 24,
            lower: true,
            upper: true,
            digits: true,
            symbols: false,
            exclude_ambiguous: true,
        }))
        .unwrap();
        assert_eq!(p["value"].as_str().unwrap().len(), 24);
        assert!(p["bits"].as_f64().unwrap() > 100.0);
        let pp: Value = serde_json::from_str(&c.generate_passphrase(5, " ", true, false)).unwrap();
        assert_eq!(pp["value"].as_str().unwrap().split(' ').count(), 5);
        let s: Value = serde_json::from_str(&c.strength("password1")).unwrap();
        assert!(s["level"].as_i64().unwrap() <= 1);

        c.shared.create("V", "pw").unwrap();
        let r: Value = serde_json::from_str(&c.health_report()).unwrap();
        assert!(r.get("error").is_some(), "free build has no health report");
        assert!(c.install_licence("v1.garbage").is_err());

        let signer = reef_licence::Signer::from_seed("t", [3; 32]).unwrap();
        let mut keys = reef_licence::KeySet::new();
        keys.insert_text(&signer.public_key_text()).unwrap();
        c.set_licence_keys(keys);
        let token = signer
            .issue(&reef_licence::Claims {
                app: shoal_keys_core::APP_ID.into(),
                exp: None,
                iat: now_unix() - 10,
                kid: "t".into(),
                lid: "lic_1".into(),
                plan: reef_licence::Plan::OneOff,
            })
            .unwrap();
        c.install_licence(&token).unwrap();
        assert!(c.licence.is_licensed());
        c.put_entry(r#"{"title":"weak","password":"password1"}"#)
            .unwrap();
        let r: Value = serde_json::from_str(&c.health_report()).unwrap();
        assert_eq!(r[0]["title"], "weak");
        c.remove_licence().unwrap();
        assert!(!c.licence.is_licensed());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn civil_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20_726), (2026, 9, 30));
    }

    #[test]
    fn lock_never_waits_for_a_worker() {
        let dir = tempdir("asynclock");
        let mut c = controller(&dir);
        c.shared.create("V", "pw").unwrap();
        c.put_entry(r#"{"title":"kept","password":"x"}"#).unwrap();
        let worker = c.shared.clone();
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        let saver = std::thread::spawn(move || {
            // Stands in for an Argon2 save: holds the vault mutex.
            worker.with_open(|v| {
                started_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                v.save()?;
                Ok(())
            })
        });
        started_rx.recv().unwrap();
        let t = std::time::Instant::now();
        c.lock();
        assert!(
            t.elapsed() < Duration::from_millis(100),
            "lock did not block"
        );
        assert!(c.is_locked(), "reads as locked at once");
        assert_eq!(c.search(""), "[]");
        assert_eq!(c.entry_count(), 0);
        release_tx.send(()).unwrap();
        saver.join().unwrap().unwrap();
        assert!(
            c.shared.open.lock().unwrap().is_none(),
            "dropped when the save ended"
        );
        assert!(c.is_locked());
        // The save completed before the lock: the entry is on disk.
        c.shared.clear_lock_request();
        c.shared.unlock("pw").unwrap();
        assert_eq!(c.entry_count(), 1);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn unlock_after_a_lock_request_stays_open() {
        let dir = tempdir("relock");
        let mut c = controller(&dir);
        c.shared.create("V", "pw").unwrap();
        c.lock();
        assert!(c.is_locked());
        c.shared.clear_lock_request();
        c.shared.unlock("pw").unwrap();
        assert!(!c.is_locked());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_lock_during_an_unlock_is_not_lost() {
        let dir = tempdir("midunlock");
        let mut c = controller(&dir);
        c.shared.create("V", "pw").unwrap();
        c.lock();
        assert!(c.is_locked());
        // The bridge clears old requests as the unlock starts...
        c.shared.clear_lock_request();
        // ...then, while Argon2 runs (no mutex held), the screen locks.
        c.lock();
        c.shared.unlock("pw").unwrap();
        assert!(c.is_locked(), "the vault opened by the unlock is dropped");
        assert!(c.shared.open.lock().unwrap().is_none());
        // The next unlock works.
        c.shared.clear_lock_request();
        c.shared.unlock("pw").unwrap();
        assert!(!c.is_locked());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_late_reef_handoff_does_not_overwrite_a_newer_licence_choice() {
        let dir = tempdir("handoff");
        let mut c = controller(&dir);
        let (_, _, generation) = c.licence_handoff();
        // Meanwhile the user removes the licence in Settings.
        c.remove_licence().unwrap();
        let stale = LicenceState::Licensed {
            licence_id: "lic_reef".into(),
            grace_ends: None,
        };
        assert!(!c.apply_reef_licence(generation, stale.clone()));
        assert!(!c.licence.is_licensed());
        // With no change in between, the hand-off applies.
        let (_, _, generation) = c.licence_handoff();
        assert!(c.apply_reef_licence(generation, stale));
        assert!(c.licence.is_licensed());
        std::fs::remove_dir_all(dir).ok();
    }
}
