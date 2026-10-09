// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

//! Package operations through `PackageKit`.
//!
//! On Sailfish, `PackageKit` (zypp backend) is the supported way for an app to
//! install packages; the Chum GUI uses it through PackageKit-Qt, and the
//! Chum GUI installer through `pkcon`. Reef talks to the same daemon
//! directly over D-Bus with zbus.
//!
//! [`PackageManager`] is the seam: the store logic is written against it,
//! [`dbus::PackageKitClient`] implements it for the real daemon, and tests
//! use a fake (or the mock `PackageKit` in `tests/packagekit_mock.rs`).

pub mod dbus;
mod types;

use std::fmt;

pub use types::{
    filter_bits, Exit, Filter, Info, PackageId, PackageInfo, Progress, Status,
    TRANSACTION_FLAG_ONLY_TRUSTED,
};

#[derive(Debug)]
pub enum Error {
    /// Talking to `PackageKit` failed (daemon missing, access denied, ...).
    DBus(zbus::Error),
    /// The transaction ran and did not succeed.
    Transaction {
        exit: Exit,
        /// `PkErrorEnum` value and `PackageKit`'s message, when it sent one.
        error: Option<(u32, String)>,
    },
    /// A name did not resolve to any package.
    NotFound(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::DBus(e) => write!(f, "PackageKit unavailable: {e}"),
            Error::Transaction {
                exit,
                error: Some((code, details)),
            } => write!(f, "{exit:?} (error {code}): {details}"),
            Error::Transaction { exit, error: None } => write!(f, "transaction ended {exit:?}"),
            Error::NotFound(name) => write!(f, "no package named {name}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<zbus::Error> for Error {
    fn from(e: zbus::Error) -> Self {
        Error::DBus(e)
    }
}

impl From<zbus::fdo::Error> for Error {
    fn from(e: zbus::fdo::Error) -> Self {
        Error::DBus(e.into())
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// What the store needs from a package manager.
///
/// Installs and updates always run with `ONLY_TRUSTED`, so a package whose
/// RPM signature does not verify is refused by `PackageKit` itself.
#[allow(
    async_fn_in_trait,
    reason = "used with concrete types only; no Send bound needed"
)]
pub trait PackageManager {
    /// Resolves package names to ids, filtered by `filters`.
    async fn resolve(&self, names: &[&str], filters: &[Filter]) -> Result<Vec<PackageInfo>>;

    async fn install(&self, ids: &[&str], progress: &mut dyn FnMut(Progress)) -> Result<()>;

    async fn update(&self, ids: &[&str], progress: &mut dyn FnMut(Progress)) -> Result<()>;

    /// Removes packages. Fails if other installed packages depend on them
    /// (removing Keel must not silently take every Keel app with it), and
    /// never autoremoves, which could take out packages the user relies on.
    async fn remove(&self, ids: &[&str], progress: &mut dyn FnMut(Progress)) -> Result<()>;

    /// Available updates for everything installed, from every repository.
    async fn get_updates(&self) -> Result<Vec<PackageInfo>>;

    /// Refreshes one repository's metadata (`RepoSetData(alias,
    /// "refresh-now", "true")`), as the Chum GUI installer does.
    async fn refresh_repo(&self, alias: &str) -> Result<()>;
}

/// Updates Reef may offer: only packages coming from Reef's own repository.
/// Reef must never update system packages; that is Settings' job, and doing
/// it from a third-party store is how phones get half-upgraded.
pub fn reef_updates(updates: &[PackageInfo], alias: &str) -> Vec<PackageInfo> {
    updates
        .iter()
        .filter(|p| p.id.from_repo(alias))
        .cloned()
        .collect()
}

/// Resolves `name` to the id to install: the newest available build from
/// Reef's repository.
pub async fn resolve_installable<P: PackageManager>(
    pm: &P,
    name: &str,
    alias: &str,
) -> Result<PackageId> {
    let found = pm
        .resolve(&[name], &[Filter::NotInstalled, Filter::Newest])
        .await?;
    found
        .into_iter()
        .map(|p| p.id)
        .find(|id| id.name == name && id.from_repo(alias))
        .ok_or_else(|| Error::NotFound(name.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pkg(id: &str) -> PackageInfo {
        PackageInfo {
            info: Info::Normal,
            id: PackageId::parse(id).unwrap(),
            summary: String::new(),
        }
    }

    #[test]
    fn only_reef_updates_are_offered() {
        let updates = [
            pkg("shipwright-keel-silica;0.2.0-1;aarch64;shipwright-reef"),
            pkg("sailfish-browser;3.0-1;aarch64;jolla"),
            pkg("qt6-qtbase;6.8.3-1;aarch64;sailfishos-chum"),
        ];
        let reef = reef_updates(&updates, "shipwright-reef");
        assert_eq!(reef.len(), 1);
        assert_eq!(reef[0].id.name, "shipwright-keel-silica");
    }
}
