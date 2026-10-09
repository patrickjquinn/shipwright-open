// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

//! The vault on disk: `vault.kdbx` plus `vault.wrap`, and the unlock flow.
//!
//! ```text
//!   master password ─┐
//!                    ├─ KDBX 4 composite key ─ Argon2id ─ vault.kdbx
//!   device key ──────┘   (password + key file)
//!       ▲
//!       └─ XChaCha20-Poly1305 unwrap (vault.wrap) ◄─ wrapping key ◄─ KeyStore
//!                                                   (Sailfish Secrets)
//! ```
//!
//! The device key is an ordinary KeePass key file component, so the vault
//! stays a standard KDBX 4 file: with the master password and the recovery
//! key file (see [`OpenVault::recovery_keyfile`]) it opens in KeePassXC.
//! Without the wrapping key, which never leaves Sailfish Secrets, a copy of
//! `vault.kdbx` cannot be attacked by guessing the password alone.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use zeroize::Zeroizing;

use crate::keystore::KeyStore;
use crate::vault::{keyfile_xml, CompositeKey, KdfParams, Vault};
use crate::wrap::{random_key, Wrapped};
use crate::Error;

pub const VAULT_FILE: &str = "vault.kdbx";
pub const WRAP_FILE: &str = "vault.wrap";
pub const BACKUP_FILE: &str = "vault.kdbx.bak";

/// A vault directory and the key store its wrapping key lives in.
pub struct VaultStore {
    dir: PathBuf,
    keystore: Arc<dyn KeyStore>,
    kdf: KdfParams,
}

impl VaultStore {
    pub fn new(dir: impl Into<PathBuf>, keystore: Arc<dyn KeyStore>) -> Self {
        Self {
            dir: dir.into(),
            keystore,
            kdf: KdfParams::default(),
        }
    }

    /// Argon2id parameters for vaults created (or re-keyed) from now on.
    #[must_use]
    pub fn with_kdf(mut self, kdf: KdfParams) -> Self {
        self.kdf = kdf;
        self
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn keystore_name(&self) -> &'static str {
        self.keystore.describe()
    }

    pub fn exists(&self) -> bool {
        self.dir.join(VAULT_FILE).exists()
    }

    /// Creates a new, empty vault protected by `password` and a fresh device
    /// key whose wrapping key is stored in the key store.
    pub fn create(&self, name: &str, password: &str) -> Result<OpenVault, Error> {
        if self.exists() {
            return Err(Error::VaultExists);
        }
        if password.is_empty() {
            return Err(Error::Kdbx("the master password must not be empty".into()));
        }
        fs::create_dir_all(&self.dir)?;
        restrict_dir(&self.dir);
        let device_key = random_key()?;
        self.bind(&device_key)?;
        let mut open = OpenVault {
            vault: Vault::new(name, self.kdf),
            key: composite(password, &device_key),
            device_key,
            dir: self.dir.clone(),
        };
        open.save()?;
        Ok(open)
    }

    /// Opens the vault with the master password and the wrapped device key.
    pub fn unlock(&self, password: &str) -> Result<OpenVault, Error> {
        if !self.exists() {
            return Err(Error::NoVault);
        }
        let wrapped = Wrapped::from_bytes(&fs::read(self.dir.join(WRAP_FILE))?)?;
        let wk = self.keystore.get(&wrapped.secret_name())?.ok_or_else(|| {
            Error::KeyStore(
                "this vault's device key is not in the key store; restore it with the recovery key file"
                    .into(),
            )
        })?;
        let device_key = wrapped.open(&wk)?;
        self.open_with(password, device_key)
    }

    /// Opens the vault with the master password and the recovery key file,
    /// then binds it to this device again with a new wrapping key. For use
    /// after a factory reset or a restore from backup.
    pub fn recover(&self, password: &str, recovery_keyfile: &[u8]) -> Result<OpenVault, Error> {
        if !self.exists() {
            return Err(Error::NoVault);
        }
        let bytes = Zeroizing::new(fs::read(self.dir.join(VAULT_FILE))?);
        let key = CompositeKey::password(password).with_keyfile(recovery_keyfile);
        let vault = Vault::open(&bytes, &key)?;
        let device_key = parse_recovery_keyfile(recovery_keyfile)?;
        // Replace the old wrapping (if any) with a new one.
        if let Ok(old) = fs::read(self.dir.join(WRAP_FILE))
            .map_err(Error::from)
            .and_then(|b| Wrapped::from_bytes(&b))
        {
            self.keystore.delete(&old.secret_name()).ok();
        }
        self.bind(&device_key)?;
        Ok(OpenVault {
            vault,
            key: composite(password, &device_key),
            device_key,
            dir: self.dir.clone(),
        })
    }

    /// Deletes the vault files and its wrapping key. Irreversible.
    pub fn destroy(&self) -> Result<(), Error> {
        if let Ok(b) = fs::read(self.dir.join(WRAP_FILE)) {
            if let Ok(w) = Wrapped::from_bytes(&b) {
                self.keystore.delete(&w.secret_name())?;
            }
        }
        for f in [VAULT_FILE, WRAP_FILE, BACKUP_FILE] {
            match fs::remove_file(self.dir.join(f)) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
                _ => {}
            }
        }
        Ok(())
    }

    fn open_with(
        &self,
        password: &str,
        device_key: Zeroizing<[u8; 32]>,
    ) -> Result<OpenVault, Error> {
        let bytes = Zeroizing::new(fs::read(self.dir.join(VAULT_FILE))?);
        let key = composite(password, &device_key);
        let vault = Vault::open(&bytes, &key)?;
        Ok(OpenVault {
            vault,
            key,
            device_key,
            dir: self.dir.clone(),
        })
    }

    /// New vault id and wrapping key; writes `vault.wrap`.
    fn bind(&self, device_key: &[u8; 32]) -> Result<(), Error> {
        let mut vault_id = [0u8; 16];
        getrandom::fill(&mut vault_id).map_err(|e| Error::Wrap(e.to_string()))?;
        let wk = random_key()?;
        let wrapped = Wrapped::seal(vault_id, wk.as_ref(), device_key)?;
        self.keystore.put(&wrapped.secret_name(), wk.as_ref())?;
        if let Err(e) = write_atomic(&self.dir.join(WRAP_FILE), &wrapped.to_bytes()) {
            self.keystore.delete(&wrapped.secret_name()).ok();
            return Err(e.into());
        }
        Ok(())
    }
}

fn composite(password: &str, device_key: &[u8; 32]) -> CompositeKey {
    CompositeKey::password(password).with_keyfile(keyfile_xml(device_key).as_bytes())
}

/// Reads the 32-byte key back out of a recovery key file (our XML v2 form,
/// or 32 raw bytes, or 64 hex characters).
fn parse_recovery_keyfile(b: &[u8]) -> Result<Zeroizing<[u8; 32]>, Error> {
    let mut out = Zeroizing::new([0u8; 32]);
    if b.len() == 32 {
        out.copy_from_slice(b);
        return Ok(out);
    }
    let text =
        std::str::from_utf8(b).map_err(|e| Error::Wrap(format!("unrecognised key file: {e}")))?;
    let hex_part = match (text.find("<Data"), text.find("</Data>")) {
        (Some(s), Some(e)) if s < e => {
            let inner = &text[s..e];
            inner.split_once('>').map_or("", |(_, h)| h)
        }
        _ => text,
    };
    let hex: Zeroizing<String> =
        Zeroizing::new(hex_part.chars().filter(char::is_ascii_hexdigit).collect());
    if hex.len() != 64 {
        return Err(Error::Wrap("unrecognised key file".into()));
    }
    for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
        let s = std::str::from_utf8(chunk).map_err(|e| Error::Wrap(format!("bad hex: {e}")))?;
        out[i] = u8::from_str_radix(s, 16).map_err(|e| Error::Wrap(format!("bad hex: {e}")))?;
    }
    Ok(out)
}

/// An unlocked vault. Dropping it (locking) zeroises the keys; the
/// decrypted entries are held in `keepass` values, protected fields in
/// zeroise-on-drop boxes.
pub struct OpenVault {
    pub vault: Vault,
    key: CompositeKey,
    device_key: Zeroizing<[u8; 32]>,
    dir: PathBuf,
}

impl std::fmt::Debug for OpenVault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenVault")
            .field("vault", &self.vault)
            .field("dir", &self.dir)
            .finish_non_exhaustive()
    }
}

impl OpenVault {
    /// Encrypts and writes `vault.kdbx` atomically, keeping the previous
    /// file as `vault.kdbx.bak`.
    pub fn save(&mut self) -> Result<(), Error> {
        let bytes = self.vault.save(&self.key)?;
        let path = self.dir.join(VAULT_FILE);
        if path.exists() {
            fs::copy(&path, self.dir.join(BACKUP_FILE))?;
        }
        write_atomic(&path, &bytes)?;
        Ok(())
    }

    /// Re-encrypts the vault under a new master password (same device key).
    pub fn change_password(&mut self, new_password: &str) -> Result<(), Error> {
        if new_password.is_empty() {
            return Err(Error::Kdbx("the master password must not be empty".into()));
        }
        self.key = composite(new_password, &self.device_key);
        self.save()?;
        // The backup holds the old password's file; replace it.
        fs::copy(self.dir.join(VAULT_FILE), self.dir.join(BACKUP_FILE))?;
        Ok(())
    }

    /// The KeePass XML key file holding the device key. With it and the
    /// master password the vault opens anywhere; the user keeps it offline.
    pub fn recovery_keyfile(&self) -> Zeroizing<String> {
        keyfile_xml(&self.device_key)
    }

    /// A portable KDBX 4 copy protected only by `password`, for KeePassXC,
    /// KeePassDX or another Shoal Keys.
    pub fn export_kdbx(&mut self, password: &str) -> Result<Vec<u8>, Error> {
        if password.is_empty() {
            return Err(Error::Kdbx("the export password must not be empty".into()));
        }
        self.vault.export_kdbx(&CompositeKey::password(password))
    }
}

/// Writes `data` to `path` via a temporary file, fsync and rename, with
/// mode 0600.
pub fn write_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let tmp = dir.join(format!(
        ".{}.tmp",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("file")
    ));
    {
        let mut opts = fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut f = opts.open(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    if let Ok(d) = fs::File::open(dir) {
        d.sync_all().ok();
    }
    Ok(())
}

fn restrict_dir(dir: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700)).ok();
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::fmt::Write as _;

    use super::*;
    use crate::entry::EntryData;
    use crate::keystore::MemoryKeyStore;

    pub(crate) fn tempdir(tag: &str) -> PathBuf {
        let mut n = [0u8; 8];
        getrandom::fill(&mut n).unwrap();
        let hex: String = n.iter().fold(String::new(), |mut hex, b| {
            let _ = write!(hex, "{b:02x}");
            hex
        });
        let d = std::env::temp_dir().join(format!("shoal-keys-{tag}-{hex}"));
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn store(dir: &Path, ks: Arc<MemoryKeyStore>) -> VaultStore {
        VaultStore::new(dir, ks).with_kdf(KdfParams::insecure_for_tests())
    }

    fn entry() -> EntryData {
        {
            let mut e = EntryData::default();
            e.title = "Mail".into();
            e.username = "me".into();
            e.password = "pw".into();
            e
        }
    }

    #[test]
    fn create_unlock_and_persist() {
        let dir = tempdir("store");
        let ks = Arc::new(MemoryKeyStore::new());
        let s = store(&dir, ks.clone());
        assert!(!s.exists());
        assert!(matches!(s.unlock("x"), Err(Error::NoVault)));
        let mut v = s.create("Mine", "master").unwrap();
        assert!(matches!(
            s.create("Again", "master"),
            Err(Error::VaultExists)
        ));
        v.vault.add(&entry());
        v.save().unwrap();
        drop(v);

        let v = s.unlock("master").unwrap();
        assert_eq!(v.vault.entries()[0].title, "Mail");
        assert!(matches!(s.unlock("wrong"), Err(Error::WrongKey)));
        assert!(dir.join(BACKUP_FILE).exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(dir.join(VAULT_FILE))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn file_alone_is_not_enough() {
        let dir = tempdir("theft");
        let ks = Arc::new(MemoryKeyStore::new());
        let s = store(&dir, ks);
        s.create("Mine", "master").unwrap();
        // Someone copies the files to another phone: its key store is empty.
        let other = store(&dir, Arc::new(MemoryKeyStore::new()));
        assert!(matches!(other.unlock("master"), Err(Error::KeyStore(_))));
        // And the password alone does not open the KDBX.
        let bytes = fs::read(dir.join(VAULT_FILE)).unwrap();
        assert!(matches!(
            Vault::open(&bytes, &CompositeKey::password("master")),
            Err(Error::WrongKey)
        ));
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn recovery_keyfile_restores_on_a_new_device() {
        let dir = tempdir("recover");
        let s = store(&dir, Arc::new(MemoryKeyStore::new()));
        let mut v = s.create("Mine", "master").unwrap();
        v.vault.add(&entry());
        v.save().unwrap();
        let recovery = v.recovery_keyfile();
        drop(v);

        // Opens in any KeePass client with password + key file.
        let bytes = fs::read(dir.join(VAULT_FILE)).unwrap();
        let k = CompositeKey::password("master").with_keyfile(recovery.as_bytes());
        assert_eq!(Vault::open(&bytes, &k).unwrap().len(), 1);

        let fresh = Arc::new(MemoryKeyStore::new());
        let s2 = store(&dir, fresh.clone());
        assert!(s2.unlock("master").is_err());
        assert!(s2.recover("master", b"garbage").is_err());
        let v = s2.recover("master", recovery.as_bytes()).unwrap();
        assert_eq!(v.vault.len(), 1);
        drop(v);
        assert_eq!(s2.unlock("master").unwrap().vault.len(), 1);
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn change_password_and_export() {
        let dir = tempdir("chpw");
        let s = store(&dir, Arc::new(MemoryKeyStore::new()));
        let mut v = s.create("Mine", "old").unwrap();
        v.vault.add(&entry());
        v.change_password("new").unwrap();
        let exported = v.export_kdbx("share").unwrap();
        drop(v);
        assert!(s.unlock("old").is_err());
        assert!(s.unlock("new").is_ok());
        let e = Vault::open(&exported, &CompositeKey::password("share")).unwrap();
        assert_eq!(e.entries()[0].password, "pw");
        // The backup was refreshed, so it does not open with the old password.
        let bak = fs::read(dir.join(BACKUP_FILE)).unwrap();
        let fresh_ks_store = store(&dir, Arc::new(MemoryKeyStore::new()));
        drop(fresh_ks_store);
        assert!(Vault::open(&bak, &CompositeKey::password("old")).is_err());
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn keystore_down_is_an_error_not_a_panic() {
        let dir = tempdir("down");
        let ks = Arc::new(MemoryKeyStore::new());
        let s = store(&dir, ks.clone());
        s.create("Mine", "pw").unwrap();
        ks.fail.store(true, std::sync::atomic::Ordering::SeqCst);
        assert!(matches!(s.unlock("pw"), Err(Error::KeyStore(_))));
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn destroy_removes_everything() {
        let dir = tempdir("destroy");
        let ks = Arc::new(MemoryKeyStore::new());
        let s = store(&dir, ks.clone());
        let mut v = s.create("Mine", "pw").unwrap();
        v.save().unwrap();
        let name = Wrapped::from_bytes(&fs::read(dir.join(WRAP_FILE)).unwrap())
            .unwrap()
            .secret_name();
        assert!(ks.get(&name).unwrap().is_some());
        s.destroy().unwrap();
        assert!(!s.exists());
        assert!(ks.get(&name).unwrap().is_none());
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn recovery_keyfile_parsing() {
        let k = [0xABu8; 32];
        assert_eq!(
            *parse_recovery_keyfile(keyfile_xml(&k).as_bytes()).unwrap(),
            k
        );
        assert_eq!(*parse_recovery_keyfile(&k).unwrap(), k);
        assert_eq!(
            *parse_recovery_keyfile("ab".repeat(32).as_bytes()).unwrap(),
            k
        );
        assert!(parse_recovery_keyfile(b"short").is_err());
    }
}
