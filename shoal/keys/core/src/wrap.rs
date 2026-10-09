// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

//! Wrapping of the 32-byte device key with a key held in the platform key
//! store.
//!
//! `vault.wrap` layout (92 bytes):
//!
//! | Offset | Size | Content                                         |
//! | ------ | ---- | ----------------------------------------------- |
//! | 0      | 4    | magic `SKW1`                                     |
//! | 4      | 16   | vault id (random; names the key-store secret)    |
//! | 20     | 24   | XChaCha20-Poly1305 nonce (random)                |
//! | 44     | 48   | ciphertext of the device key plus 16-byte tag    |
//!
//! Bytes 0..20 are the associated data, so the id cannot be swapped to
//! make one vault's wrap file use another vault's key.

use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};
use zeroize::Zeroizing;

use crate::Error;

pub const MAGIC: &[u8; 4] = b"SKW1";
pub const WRAP_LEN: usize = 4 + 16 + 24 + 48;

/// Parsed `vault.wrap`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wrapped {
    pub vault_id: [u8; 16],
    nonce: [u8; 24],
    ciphertext: Vec<u8>,
}

impl Wrapped {
    /// The key-store secret name for this vault's wrapping key.
    pub fn secret_name(&self) -> String {
        secret_name(&self.vault_id)
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(WRAP_LEN);
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&self.vault_id);
        out.extend_from_slice(&self.nonce);
        out.extend_from_slice(&self.ciphertext);
        out
    }

    pub fn from_bytes(b: &[u8]) -> Result<Self, Error> {
        if b.len() != WRAP_LEN || &b[..4] != MAGIC {
            return Err(Error::Wrap("not a Shoal Keys wrap file".into()));
        }
        let mut vault_id = [0u8; 16];
        vault_id.copy_from_slice(&b[4..20]);
        let mut nonce = [0u8; 24];
        nonce.copy_from_slice(&b[20..44]);
        Ok(Self {
            vault_id,
            nonce,
            ciphertext: b[44..].to_vec(),
        })
    }

    fn aad(&self) -> Vec<u8> {
        let mut a = MAGIC.to_vec();
        a.extend_from_slice(&self.vault_id);
        a
    }

    /// Encrypts `device_key` under `wrapping_key`.
    pub fn seal(
        vault_id: [u8; 16],
        wrapping_key: &[u8],
        device_key: &[u8; 32],
    ) -> Result<Self, Error> {
        let cipher = cipher(wrapping_key)?;
        let mut nonce = [0u8; 24];
        getrandom::fill(&mut nonce).map_err(|e| Error::Wrap(e.to_string()))?;
        let mut w = Self {
            vault_id,
            nonce,
            ciphertext: Vec::new(),
        };
        let aad = w.aad();
        w.ciphertext = cipher
            .encrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: device_key,
                    aad: &aad,
                },
            )
            .map_err(|e| Error::Wrap(format!("encryption failed: {e}")))?;
        Ok(w)
    }

    /// Decrypts the device key. Fails if the key or any byte of the file is
    /// wrong.
    pub fn open(&self, wrapping_key: &[u8]) -> Result<Zeroizing<[u8; 32]>, Error> {
        let cipher = cipher(wrapping_key)?;
        let aad = self.aad();
        let plain = Zeroizing::new(
            cipher
                .decrypt(
                    &XNonce::from(self.nonce),
                    Payload {
                        msg: &self.ciphertext,
                        aad: &aad,
                    },
                )
                .map_err(|e| {
                    Error::Wrap(format!("the device key does not match this vault: {e}"))
                })?,
        );
        let mut out = Zeroizing::new([0u8; 32]);
        if plain.len() != 32 {
            return Err(Error::Wrap("wrapped key has the wrong length".into()));
        }
        out.copy_from_slice(&plain);
        Ok(out)
    }
}

fn cipher(key: &[u8]) -> Result<XChaCha20Poly1305, Error> {
    if key.len() != 32 {
        return Err(Error::Wrap("wrapping key must be 32 bytes".into()));
    }
    XChaCha20Poly1305::new_from_slice(key)
        .map_err(|e| Error::Wrap(format!("bad wrapping key: {e}")))
}

pub fn secret_name(vault_id: &[u8; 16]) -> String {
    use std::fmt::Write as _;
    let mut name = String::from("shoalkeys-");
    for b in vault_id {
        // Writing to a String cannot fail.
        let _ = write!(name, "{b:02x}");
    }
    name
}

/// 32 random bytes from the OS.
pub fn random_key() -> Result<Zeroizing<[u8; 32]>, Error> {
    let mut k = Zeroizing::new([0u8; 32]);
    getrandom::fill(k.as_mut()).map_err(|e| Error::Wrap(e.to_string()))?;
    Ok(k)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_open_round_trip() {
        let wk = random_key().unwrap();
        let dk = random_key().unwrap();
        let w = Wrapped::seal([7; 16], wk.as_ref(), &dk).unwrap();
        let bytes = w.to_bytes();
        assert_eq!(bytes.len(), WRAP_LEN);
        let back = Wrapped::from_bytes(&bytes).unwrap();
        assert_eq!(*back.open(wk.as_ref()).unwrap(), *dk);
        assert_eq!(back.secret_name(), format!("shoalkeys-{}", "07".repeat(16)));
    }

    #[test]
    fn tampering_is_detected() {
        let wk = random_key().unwrap();
        let dk = random_key().unwrap();
        let bytes = Wrapped::seal([1; 16], wk.as_ref(), &dk).unwrap().to_bytes();
        for i in 4..WRAP_LEN {
            let mut b = bytes.clone();
            b[i] ^= 1;
            assert!(
                Wrapped::from_bytes(&b).unwrap().open(wk.as_ref()).is_err(),
                "byte {i}"
            );
        }
        let other = random_key().unwrap();
        assert!(Wrapped::from_bytes(&bytes)
            .unwrap()
            .open(other.as_ref())
            .is_err());
        assert!(Wrapped::from_bytes(&bytes[..50]).is_err());
        assert!(Wrapped::from_bytes(&[0; WRAP_LEN]).is_err());
        assert!(Wrapped::seal([0; 16], &[0; 16], &dk).is_err());
    }
}
