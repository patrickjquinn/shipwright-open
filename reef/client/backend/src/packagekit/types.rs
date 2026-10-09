// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

//! `PackageKit`'s wire values. Numbers are from `PackageKit`'s `pk-enum.h`
//! (1.x); bitfields put bit `1 << value` for each enum value.

/// `PkFilterEnum` values used by Reef.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum Filter {
    None = 1,
    Installed = 2,
    NotInstalled = 3,
    Newest = 16,
    Arch = 18,
}

/// Builds a filter bitfield.
pub fn filter_bits(filters: &[Filter]) -> u64 {
    filters.iter().fold(0, |acc, f| acc | (1u64 << (*f as u32)))
}

/// `PkTransactionFlagEnum`: `ONLY_TRUSTED` is 1, so its bit is `1 << 1`.
/// Every install and update Reef starts sets it, so `PackageKit` refuses
/// packages whose signature does not verify against an imported key.
pub const TRANSACTION_FLAG_ONLY_TRUSTED: u64 = 1 << 1;

/// `PkInfoEnum`: what a `Package` signal says about a package.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Info {
    Installed,
    Available,
    Low,
    Enhancement,
    Normal,
    Bugfix,
    Important,
    Security,
    Downloading,
    Updating,
    Installing,
    Removing,
    Finished,
    Other(u32),
}

impl From<u32> for Info {
    fn from(v: u32) -> Self {
        match v {
            1 => Info::Installed,
            2 => Info::Available,
            3 => Info::Low,
            4 => Info::Enhancement,
            5 => Info::Normal,
            6 => Info::Bugfix,
            7 => Info::Important,
            8 => Info::Security,
            10 => Info::Downloading,
            11 => Info::Updating,
            12 => Info::Installing,
            13 => Info::Removing,
            18 => Info::Finished,
            other => Info::Other(other),
        }
    }
}

/// `PkStatusEnum`: the transaction's current phase, for progress text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Wait,
    Setup,
    Running,
    Query,
    Remove,
    RefreshCache,
    Download,
    Install,
    Update,
    Cleanup,
    DepResolve,
    SigCheck,
    Commit,
    Finished,
    Other(u32),
}

impl From<u32> for Status {
    fn from(v: u32) -> Self {
        match v {
            1 => Status::Wait,
            2 => Status::Setup,
            3 => Status::Running,
            4 => Status::Query,
            6 => Status::Remove,
            7 => Status::RefreshCache,
            8 => Status::Download,
            9 => Status::Install,
            10 => Status::Update,
            11 => Status::Cleanup,
            13 => Status::DepResolve,
            14 => Status::SigCheck,
            16 => Status::Commit,
            18 => Status::Finished,
            other => Status::Other(other),
        }
    }
}

/// `PkExitEnum`: how a transaction ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    Success,
    Failed,
    Cancelled,
    KeyRequired,
    EulaRequired,
    Killed,
    NeedUntrusted,
    Other(u32),
}

impl From<u32> for Exit {
    fn from(v: u32) -> Self {
        match v {
            1 => Exit::Success,
            2 => Exit::Failed,
            3 => Exit::Cancelled,
            4 => Exit::KeyRequired,
            5 => Exit::EulaRequired,
            6 => Exit::Killed,
            8 => Exit::NeedUntrusted,
            other => Exit::Other(other),
        }
    }
}

/// A `PackageKit` package id, `name;version;arch;data`. For available
/// packages `data` is the repository alias; for installed ones it is
/// `installed` (possibly with the origin repository after a colon).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PackageId {
    pub raw: String,
    pub name: String,
    pub version: String,
    pub arch: String,
    pub data: String,
}

impl PackageId {
    pub fn parse(raw: &str) -> Option<Self> {
        let mut it = raw.split(';');
        let (name, version, arch, data) = (it.next()?, it.next()?, it.next()?, it.next()?);
        if name.is_empty() || it.next().is_some() {
            return None;
        }
        Some(Self {
            raw: raw.to_string(),
            name: name.into(),
            version: version.into(),
            arch: arch.into(),
            data: data.into(),
        })
    }

    pub fn is_installed(&self) -> bool {
        self.data == "installed" || self.data.starts_with("installed:")
    }

    /// Whether this package comes from repository `alias`.
    pub fn from_repo(&self, alias: &str) -> bool {
        self.data == alias || self.data.strip_prefix("installed:") == Some(alias)
    }
}

/// One `Package` signal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageInfo {
    pub info: Info,
    pub id: PackageId,
    pub summary: String,
}

/// Progress of a running transaction, for the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Progress {
    /// 0 to 100, or `None` when `PackageKit` does not know (it sends 101).
    pub percentage: Option<u32>,
    pub status: Status,
    /// The package id `ItemProgress` refers to, if any.
    pub item: Option<String>,
}

pub(crate) fn percentage(raw: u32) -> Option<u32> {
    (raw <= 100).then_some(raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_bitfields_match_packagekit_qt() {
        // PackageKit-Qt: FilterInstalled = 0x4, FilterNotInstalled = 0x8,
        // FilterNewest = 0x10000.
        assert_eq!(filter_bits(&[Filter::Installed]), 0x4);
        assert_eq!(filter_bits(&[Filter::NotInstalled]), 0x8);
        assert_eq!(filter_bits(&[Filter::Newest]), 0x10000);
        assert_eq!(filter_bits(&[Filter::None]), 0x2);
        assert_eq!(TRANSACTION_FLAG_ONLY_TRUSTED, 0x2);
    }

    #[test]
    fn parses_package_ids() {
        let id =
            PackageId::parse("shipwright-keel-silica;0.1.0-1;aarch64;shipwright-reef").unwrap();
        assert_eq!(id.name, "shipwright-keel-silica");
        assert!(id.from_repo("shipwright-reef"));
        assert!(!id.is_installed());
        let inst = PackageId::parse("bash;5.0-1;aarch64;installed").unwrap();
        assert!(inst.is_installed());
        assert!(!inst.from_repo("shipwright-reef"));
        let origin = PackageId::parse("x;1;noarch;installed:shipwright-reef").unwrap();
        assert!(origin.is_installed() && origin.from_repo("shipwright-reef"));
        assert!(PackageId::parse("a;b;c").is_none());
        assert!(PackageId::parse(";1;a;d").is_none());
        assert!(PackageId::parse("a;1;a;d;e").is_none());
    }

    #[test]
    fn enums_map_unknowns() {
        assert_eq!(Exit::from(1), Exit::Success);
        assert_eq!(Exit::from(99), Exit::Other(99));
        assert_eq!(Info::from(8), Info::Security);
        assert_eq!(Status::from(14), Status::SigCheck);
        assert_eq!(percentage(101), None);
        assert_eq!(percentage(42), Some(42));
    }
}
