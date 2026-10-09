// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

//! An open (decrypted) KDBX 4 database and the operations the app needs.
//!
//! The file format is KeePass KDBX 4 (see README, "Vault format"), read and
//! written by the `keepass` crate. New vaults use ChaCha20 for the outer
//! cipher, Argon2id for key derivation, ChaCha20 for protected fields and
//! GZip compression: the choices KeePassXC makes for new databases.

use std::collections::HashSet;

use keepass::{
    config::{CompressionConfig, DatabaseConfig, InnerCipherConfig, KdfConfig, OuterCipherConfig},
    db::{EntryId, GroupId, History},
    Database, DatabaseKey,
};
use zeroize::Zeroizing;

use crate::entry::EntryData;
use crate::import::Imported;
use crate::search;
use crate::Error;

/// How many old versions of an entry are kept (KeePass's default is 10).
pub const HISTORY_MAX_ITEMS: usize = 10;

/// Argon2id cost for new vaults. KeePassXC defaults to 64 MiB and a time
/// cost tuned to one second on the desktop; on a phone we take 64 MiB,
/// 2 lanes and 3 passes. [device verification] time this on the Jolla
/// Phone: unlock should stay below about 1.5 s.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KdfParams {
    pub memory_kib: u64,
    pub iterations: u64,
    pub parallelism: u32,
}

impl Default for KdfParams {
    fn default() -> Self {
        Self {
            memory_kib: 64 * 1024,
            iterations: 3,
            parallelism: 2,
        }
    }
}

impl KdfParams {
    /// Cheap parameters for unit tests only.
    pub fn insecure_for_tests() -> Self {
        Self {
            memory_kib: 64,
            iterations: 1,
            parallelism: 1,
        }
    }
}

/// The KDBX composite key: master password plus an optional key file.
#[derive(Clone)]
pub struct CompositeKey {
    password: Option<Zeroizing<String>>,
    keyfile: Option<Zeroizing<Vec<u8>>>,
}

impl std::fmt::Debug for CompositeKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompositeKey")
            .field("password", &self.password.as_ref().map(|_| "[redacted]"))
            .field("keyfile", &self.keyfile.as_ref().map(|_| "[redacted]"))
            .finish()
    }
}

impl CompositeKey {
    pub fn password(password: &str) -> Self {
        Self {
            password: Some(Zeroizing::new(password.to_string())),
            keyfile: None,
        }
    }

    /// Adds a key file. Any KeePass key file form works: XML v1 or v2,
    /// 32 raw bytes, 64 hex characters, or any other file (hashed).
    #[must_use]
    pub fn with_keyfile(mut self, keyfile: &[u8]) -> Self {
        self.keyfile = Some(Zeroizing::new(keyfile.to_vec()));
        self
    }

    pub fn keyfile_only(keyfile: &[u8]) -> Self {
        Self {
            password: None,
            keyfile: Some(Zeroizing::new(keyfile.to_vec())),
        }
    }

    fn to_database_key(&self) -> Result<DatabaseKey, Error> {
        let mut k = DatabaseKey::new();
        if let Some(p) = &self.password {
            k = k.with_password(p);
        }
        if let Some(f) = &self.keyfile {
            k = k.with_keyfile(&mut f.as_slice())?;
        }
        if k.is_empty() {
            return Err(Error::Kdbx("empty key".into()));
        }
        Ok(k)
    }
}

/// Renders 32 bytes as a KeePass 2.x XML key file (version 2.0), the form
/// KeePassXC and KeePass 2 generate. Used for the device key, so a user who
/// exports it can open the device vault in any KeePass client.
pub fn keyfile_xml(key: &[u8; 32]) -> Zeroizing<String> {
    use sha2::Digest;
    use std::fmt::Write as _;
    // Writing to a String cannot fail. The capacity is exact, so the key's
    // hex is never reallocated and left behind unzeroed.
    let mut hex = Zeroizing::new(String::with_capacity(64));
    for b in key {
        let _ = write!(hex, "{b:02X}");
    }
    let hash = sha2::Sha256::digest(key);
    let mut check = String::with_capacity(8);
    for b in &hash[..4] {
        let _ = write!(check, "{b:02X}");
    }
    let mut groups = String::new();
    for (i, chunk) in hex.as_bytes().chunks(8).enumerate() {
        if i > 0 {
            groups.push(' ');
        }
        groups.push_str(std::str::from_utf8(chunk).unwrap_or_default());
    }
    let out = Zeroizing::new(format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<KeyFile>\n\t<Meta>\n\t\t<Version>2.0</Version>\n\t</Meta>\n\t<Key>\n\t\t<Data Hash=\"{check}\">\n\t\t\t{groups}\n\t\t</Data>\n\t</Key>\n</KeyFile>\n"
    ));
    groups.zeroize_string();
    out
}

trait ZeroizeString {
    fn zeroize_string(&mut self);
}
impl ZeroizeString for String {
    fn zeroize_string(&mut self) {
        zeroize::Zeroize::zeroize(self);
    }
}

/// Summary of an import into the vault.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MergeReport {
    pub added: usize,
    pub duplicates: usize,
    pub skipped_blank: usize,
}

/// A decrypted vault.
pub struct Vault {
    db: Database,
}

impl std::fmt::Debug for Vault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Vault")
            .field("entries", &self.db.num_entries())
            .finish_non_exhaustive()
    }
}

impl Vault {
    /// A new empty vault.
    pub fn new(name: &str, kdf: KdfParams) -> Self {
        let mut config = DatabaseConfig::default();
        config.outer_cipher_config = OuterCipherConfig::ChaCha20;
        config.inner_cipher_config = InnerCipherConfig::ChaCha20;
        config.compression_config = CompressionConfig::GZip;
        config.kdf_config = KdfConfig::Argon2id {
            iterations: kdf.iterations,
            memory: kdf.memory_kib * 1024,
            parallelism: kdf.parallelism,
            version: argon2::Version::Version13,
        };
        let mut db = Database::with_config(config);
        db.meta.database_name = Some(name.to_string());
        db.meta.generator = Some("Shoal Keys".to_string());
        db.meta.history_max_items = isize::try_from(HISTORY_MAX_ITEMS).ok();
        db.meta.recyclebin_enabled = Some(false);
        db.root_mut().name = name.to_string();
        Self { db }
    }

    /// Decrypts a KDBX file (3.1 or 4.x; KDB 1.x is refused).
    pub fn open(bytes: &[u8], key: &CompositeKey) -> Result<Self, Error> {
        let db = Database::parse(bytes, key.to_database_key()?).map_err(map_open_error)?;
        if matches!(db.config.version, keepass::config::DatabaseVersion::KDB(_)) {
            return Err(Error::Kdbx(
                "KeePass 1 (.kdb) files are not supported".into(),
            ));
        }
        Ok(Self { db })
    }

    /// Encrypts the vault to KDBX 4.1 bytes. A vault opened from KDBX 3.1 is
    /// upgraded to KDBX 4 with this vault's default settings; one opened
    /// from KDBX 4.0 keeps its settings and is written as 4.1.
    pub fn save(&mut self, key: &CompositeKey) -> Result<Vec<u8>, Error> {
        if !matches!(
            self.db.config.version,
            keepass::config::DatabaseVersion::KDB4(_)
        ) {
            let upgraded = Vault::new("", KdfParams::default()).db.config;
            self.db.config = upgraded;
        } else if self.db.config.version != DatabaseConfig::default().version {
            // KDBX 4.0 (KeePassXC before 2.7, KeePassDX): same ciphers and
            // KDF, written as 4.1, which every current client reads.
            self.db.config.version = DatabaseConfig::default().version;
        }
        let mut out = Vec::new();
        self.db
            .save(&mut out, key.to_database_key()?)
            .map_err(|e| Error::Kdbx(e.to_string()))?;
        Ok(out)
    }

    /// A portable copy for other KeePass clients, protected by `key` only.
    pub fn export_kdbx(&mut self, key: &CompositeKey) -> Result<Vec<u8>, Error> {
        self.save(key)
    }

    pub fn name(&self) -> &str {
        self.db.meta.database_name.as_deref().unwrap_or("")
    }

    pub fn len(&self) -> usize {
        self.visible_ids().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Entries outside the recycle bin.
    fn visible_ids(&self) -> Vec<EntryId> {
        let bin = self.db.meta.recyclebin_uuid;
        self.db
            .iter_all_entries()
            .filter(|e| {
                let mut g = Some(e.parent().id());
                while let Some(gid) = g {
                    if Some(gid.uuid()) == bin {
                        return false;
                    }
                    g = self
                        .db
                        .group(gid)
                        .and_then(|gr| gr.parent().map(|p| p.id()));
                }
                true
            })
            .map(|e| e.id())
            .collect()
    }

    fn group_path(&self, id: EntryId) -> String {
        let Some(e) = self.db.entry(id) else {
            return String::new();
        };
        let root = self.db.root().id();
        let mut names = Vec::new();
        let mut g = Some(e.parent().id());
        while let Some(gid) = g {
            if gid == root {
                break;
            }
            let Some(gr) = self.db.group(gid) else { break };
            names.push(gr.name.clone());
            g = gr.parent().map(|p| p.id());
        }
        names.reverse();
        names.join("/")
    }

    fn read(&self, id: EntryId) -> Option<EntryData> {
        let e = self.db.entry(id)?;
        Some(EntryData::from_kdbx(
            &e,
            id.to_string(),
            self.group_path(id),
        ))
    }

    /// All entries, sorted by title (case-insensitive).
    pub fn entries(&self) -> Vec<EntryData> {
        let mut v: Vec<EntryData> = self
            .visible_ids()
            .into_iter()
            .filter_map(|id| self.read(id))
            .collect();
        v.sort_by(|a, b| {
            a.title
                .to_lowercase()
                .cmp(&b.title.to_lowercase())
                .then_with(|| a.username.cmp(&b.username))
        });
        v
    }

    pub fn entry(&self, id: &str) -> Option<EntryData> {
        self.read(parse_id(id)?)
    }

    /// Entries matching `query`, best match first (see [`search`]).
    pub fn search(&self, query: &str) -> Vec<EntryData> {
        search::search(self.entries(), query)
    }

    /// Past versions of an entry, newest first.
    pub fn history(&self, id: &str) -> Vec<EntryData> {
        let Some(eid) = parse_id(id) else {
            return Vec::new();
        };
        let Some(e) = self.db.entry(eid) else {
            return Vec::new();
        };
        let group = self.group_path(eid);
        e.history
            .as_ref()
            .map(|h| {
                h.get_entries()
                    .iter()
                    .map(|old| EntryData::from_kdbx(old, id.to_string(), group.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn ensure_group(&mut self, path: &str) -> GroupId {
        let mut current = self.db.root().id();
        for name in path.split('/').map(str::trim).filter(|s| !s.is_empty()) {
            let found = self
                .db
                .group(current)
                .and_then(|g| g.groups().find(|c| c.name == name).map(|c| c.id()));
            current = if let Some(id) = found {
                id
            } else {
                #[allow(clippy::expect_used, reason = "`current` was just looked up")]
                let mut parent = self.db.group_mut(current).expect("group exists");
                let mut child = parent.add_group();
                child.name = name.to_string();
                child.id()
            };
        }
        current
    }

    /// Adds an entry and returns its id. `data.id` is ignored.
    pub fn add(&mut self, data: &EntryData) -> String {
        self.add_with_history(data, &[])
    }

    /// Adds an entry whose earlier passwords (oldest first) become its
    /// history, as an importer does with Bitwarden or 1Password password
    /// history. At most [`HISTORY_MAX_ITEMS`] of the newest are kept.
    ///
    /// # Panics
    ///
    /// Never in practice: the entry's group is looked up or created just
    /// before it is used.
    pub fn add_with_history(&mut self, data: &EntryData, old_passwords: &[String]) -> String {
        let gid = self.ensure_group(&data.group);
        #[allow(clippy::expect_used, reason = "ensure_group returns an existing group")]
        let mut group = self.db.group_mut(gid).expect("group exists");
        let mut e = group.add_entry();
        data.write_kdbx(&mut e);
        e.times.last_modification = Some(keepass::db::Times::now());
        let skip = old_passwords.len().saturating_sub(HISTORY_MAX_ITEMS);
        let olds: Vec<&String> = old_passwords
            .iter()
            .skip(skip)
            .filter(|p| **p != data.password)
            .collect();
        if !olds.is_empty() {
            let mut h = History::default();
            for p in olds {
                let mut old = (*e).clone();
                old.fields.insert(
                    crate::entry::PASSWORD.to_string(),
                    keepass::db::Value::protected(p.clone()),
                );
                h.add_entry(old);
            }
            e.history = Some(h);
        }
        e.id().to_string()
    }

    /// Saves changes to an existing entry, keeping the previous version in
    /// its history. A save that changes nothing records no history.
    pub fn update(&mut self, data: &EntryData) -> Result<(), Error> {
        let eid = parse_id(&data.id).ok_or_else(|| Error::NotFound(data.id.clone()))?;
        let mut e = self
            .db
            .entry_mut(eid)
            .ok_or_else(|| Error::NotFound(data.id.clone()))?;
        // Check first, so a no-op save leaves no history item.
        let mut probe = e.clone();
        if !data.write_kdbx(&mut probe) {
            return Ok(());
        }
        {
            let mut tracked = e.track_changes();
            data.write_kdbx(&mut tracked);
            tracked.times.last_modification = Some(keepass::db::Times::now());
        }
        if let Some(h) = e.history.take() {
            let mut trimmed = History::default();
            for old in h.get_entries().iter().take(HISTORY_MAX_ITEMS).rev() {
                trimmed.add_entry(old.clone());
            }
            e.history = Some(trimmed);
        }
        Ok(())
    }

    /// Deletes an entry permanently (the UI offers a remorse timer first).
    pub fn delete(&mut self, id: &str) -> Result<(), Error> {
        let eid = parse_id(id).ok_or_else(|| Error::NotFound(id.to_string()))?;
        let mut e = self
            .db
            .entry_mut(eid)
            .ok_or_else(|| Error::NotFound(id.to_string()))?;
        e.track_changes().remove();
        Ok(())
    }

    /// Adds imported entries, skipping blanks and exact duplicates of
    /// entries already in the vault (same title, user name, password and
    /// first URL).
    pub fn merge(&mut self, entries: &[EntryData]) -> MergeReport {
        let items: Vec<Imported> = entries
            .iter()
            .map(|e| Imported {
                entry: e.clone(),
                password_history: Vec::new(),
            })
            .collect();
        self.merge_imported(&items)
    }

    /// As [`Vault::merge`], keeping each item's password history.
    pub fn merge_imported(&mut self, items: &[Imported]) -> MergeReport {
        let mut seen: HashSet<_> = self.entries().iter().map(EntryData::dedup_key).collect();
        let mut report = MergeReport::default();
        for item in items {
            let e = &item.entry;
            if e.is_blank() {
                report.skipped_blank += 1;
                continue;
            }
            if !seen.insert(e.dedup_key()) {
                report.duplicates += 1;
                continue;
            }
            self.add_with_history(e, &item.password_history);
            report.added += 1;
        }
        report
    }
}

fn parse_id(id: &str) -> Option<EntryId> {
    uuid::Uuid::parse_str(id).ok().map(EntryId::from_uuid)
}

fn map_open_error(e: keepass::db::DatabaseOpenError) -> Error {
    use keepass::db::DatabaseOpenError as E;
    match e {
        E::Key(_) => Error::WrongKey,
        E::Io(io) => Error::Io(io),
        other => {
            let msg = other.to_string();
            // A KDBX 4 header HMAC mismatch is how a wrong key shows up
            // before the key check in some files.
            if msg.contains("HMAC") || msg.contains("hash mismatch") {
                Error::WrongKey
            } else {
                Error::Kdbx(msg)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::CustomField;

    fn key() -> CompositeKey {
        CompositeKey::password("correct horse")
    }

    fn sample() -> EntryData {
        {
            let mut e = EntryData::default();
            e.title = "Example".into();
            e.username = "alice".into();
            e.password = "s3cret!".into();
            e.urls = vec![
                "https://example.com".into(),
                "https://login.example.com".into(),
            ];
            e.notes = "line1\nline2".into();
            e.otp = "JBSWY3DPEHPK3PXP".into();
            e.fields = vec![CustomField {
                name: "PIN".into(),
                value: "1234".into(),
                protected: true,
            }];
            e.tags = vec!["work".into()];
            e.group = "Internet/Mail".into();
            e
        }
    }

    #[test]
    fn round_trip_through_kdbx4() {
        let mut v = Vault::new("Test", KdfParams::insecure_for_tests());
        let id = v.add(&sample());
        let bytes = v.save(&key()).unwrap();
        assert_eq!(&bytes[..4], &[0x03, 0xd9, 0xa2, 0x9a], "KDBX signature");
        assert_eq!(bytes[10..12], [0x04, 0x00], "major version 4");

        let v2 = Vault::open(&bytes, &key()).unwrap();
        let e = v2.entry(&id).unwrap();
        assert_eq!(e.title, "Example");
        assert_eq!(e.password, "s3cret!");
        assert_eq!(e.urls, sample().urls);
        assert_eq!(e.notes, "line1\nline2");
        assert!(
            e.otp.starts_with("otpauth://totp/"),
            "bare secret stored as URI"
        );
        assert_eq!(e.totp().unwrap().unwrap().at(59).code.len(), 6);
        assert_eq!(e.fields, sample().fields);
        assert_eq!(e.tags, vec!["work".to_string()]);
        assert_eq!(e.group, "Internet/Mail");
        assert!(e.modified > 0);
    }

    #[test]
    fn wrong_password_is_wrong_key() {
        let mut v = Vault::new("Test", KdfParams::insecure_for_tests());
        v.add(&sample());
        let bytes = v.save(&key()).unwrap();
        assert!(matches!(
            Vault::open(&bytes, &CompositeKey::password("nope")),
            Err(Error::WrongKey)
        ));
        assert!(Vault::open(b"not a kdbx file at all", &key()).is_err());
    }

    #[test]
    fn keyfile_component_is_required() {
        let device_key = [0x42u8; 32];
        let xml = keyfile_xml(&device_key);
        let full = CompositeKey::password("pw").with_keyfile(xml.as_bytes());
        let mut v = Vault::new("Test", KdfParams::insecure_for_tests());
        v.add(&sample());
        let bytes = v.save(&full).unwrap();
        assert!(Vault::open(&bytes, &full).is_ok());
        assert!(matches!(
            Vault::open(&bytes, &CompositeKey::password("pw")),
            Err(Error::WrongKey)
        ));
        // The XML form and the raw 32 bytes are the same key element.
        assert!(Vault::open(
            &bytes,
            &CompositeKey::password("pw").with_keyfile(&device_key)
        )
        .is_ok());
    }

    #[test]
    fn update_keeps_history_and_noop_does_not() {
        let mut v = Vault::new("Test", KdfParams::insecure_for_tests());
        let id = v.add(&sample());
        let mut e = v.entry(&id).unwrap();
        v.update(&e).unwrap();
        assert!(v.history(&id).is_empty(), "no-op save records nothing");

        e.password = "new password".into();
        v.update(&e).unwrap();
        let h = v.history(&id);
        assert_eq!(h.len(), 1);
        assert_eq!(h[0].password, "s3cret!");
        assert_eq!(v.entry(&id).unwrap().password, "new password");

        for i in 0..15 {
            e.password = format!("p{i}");
            v.update(&e).unwrap();
        }
        let h = v.history(&id);
        assert_eq!(h.len(), HISTORY_MAX_ITEMS);
        assert_eq!(h[0].password, "p13", "newest first");

        let bytes = v.save(&key()).unwrap();
        let v2 = Vault::open(&bytes, &key()).unwrap();
        assert_eq!(v2.history(&id).len(), HISTORY_MAX_ITEMS);
    }

    #[test]
    fn delete_and_not_found() {
        let mut v = Vault::new("Test", KdfParams::insecure_for_tests());
        let id = v.add(&sample());
        assert_eq!(v.len(), 1);
        v.delete(&id).unwrap();
        assert!(v.is_empty());
        assert!(matches!(v.delete(&id), Err(Error::NotFound(_))));
        assert!(matches!(v.update(&sample()), Err(Error::NotFound(_))));
    }

    #[test]
    fn merge_skips_duplicates_and_blanks() {
        let mut v = Vault::new("Test", KdfParams::insecure_for_tests());
        v.add(&sample());
        let r = v.merge(&[sample(), EntryData::default(), {
            let mut e = sample();
            e.title = "Other".into();
            e
        }]);
        assert_eq!(
            r,
            MergeReport {
                added: 1,
                duplicates: 1,
                skipped_blank: 1
            }
        );
        assert_eq!(v.len(), 2);
    }

    #[test]
    fn reads_keepassxc_fixture_with_totp() {
        let bytes = include_bytes!("../tests/fixtures/test_db_kdbx4_with_totp_entry.kdbx");
        let v = Vault::open(bytes, &CompositeKey::password("test")).unwrap();
        let e = v
            .entries()
            .into_iter()
            .find(|e| e.title == "this entry has totp")
            .unwrap();
        assert_eq!(
            e.otp,
            "otpauth://totp/KeePassXC:none?secret=JBSWY3DPEHPK3PXP&period=30&digits=6&issuer=KeePassXC"
        );
        let t = e.totp().unwrap().unwrap();
        assert_eq!(t.issuer, "KeePassXC");
    }

    #[test]
    fn reads_argon2id_chacha20_fixture_and_resaves() {
        let bytes =
            include_bytes!("../tests/fixtures/test_db_kdbx4_with_password_argon2id_chacha20.kdbx");
        let k = CompositeKey::password("demopass");
        let mut v = Vault::open(bytes, &k).unwrap();
        assert_eq!(v.len(), 1);
        let before = v.entries();
        let again = v.save(&k).unwrap();
        let v2 = Vault::open(&again, &k).unwrap();
        assert_eq!(v2.entries(), before);
    }
}
