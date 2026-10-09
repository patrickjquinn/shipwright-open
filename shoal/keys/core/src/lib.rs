// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

//! Shoal Keys core: a KeePass KDBX 4 password vault whose key is bound to
//! the device through Sailfish Secrets.
//!
//! - [`vault`]: open, edit, save and export KDBX 4 databases.
//! - [`store`]: the vault on disk and the unlock flow (master password plus
//!   a device key wrapped by a key held in a [`keystore::KeyStore`]).
//! - [`sailfish`]: the Sailfish Secrets key store (feature `sailfish-secrets`).
//! - [`totp`], [`generator`], [`search`], [`autolock`]: entry features.
//! - [`import`]: KDBX, Bitwarden JSON, 1Password 1PUX and CSV, browser CSV.
//! - [`licence`]: the optional licence, verified offline with `reef-licence`.

pub mod autolock;
pub mod entry;
mod error;
pub mod generator;
pub mod import;
pub mod keystore;
pub mod licence;
#[cfg(feature = "sailfish-secrets")]
pub mod sailfish;
pub mod search;
pub mod store;
pub mod totp;
pub mod vault;
pub mod wrap;

pub use entry::{CustomField, EntryData};
pub use error::Error;
pub use keystore::{KeyStore, MemoryKeyStore, PlainFileKeyStore};
pub use store::{OpenVault, VaultStore};
pub use vault::{CompositeKey, KdfParams, MergeReport, Vault};

/// The app id licences are issued for.
pub const APP_ID: &str = "shipwright-shoal-keys";
