// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

//! The store view the UI shows: catalogue entries joined with what
//! `PackageKit` says is installed and upgradable.

use std::collections::BTreeSet;
use std::path::PathBuf;

use crate::catalogue::{Catalogue, Package};
use crate::packagekit::{
    reef_updates, resolve_installable, Filter, PackageInfo, PackageManager, Progress, Result,
};
use crate::release::SailfishRelease;

/// Where a catalogue package stands on this phone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallState {
    NotInstalled,
    Installed {
        version: String,
    },
    UpdateAvailable {
        installed: String,
        available: String,
    },
}

/// One row of the catalogue page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry<'a> {
    pub package: &'a Package,
    pub state: InstallState,
}

/// Joins visible catalogue packages with installed and update lists.
/// Pure, so the rules are unit-tested without a bus.
pub fn entries<'a>(
    catalogue: &'a Catalogue,
    release: &SailfishRelease,
    installed: &[PackageInfo],
    updates: &[PackageInfo],
) -> Vec<Entry<'a>> {
    catalogue
        .visible(release)
        .into_iter()
        .map(|package| {
            let inst = installed
                .iter()
                .find(|p| p.id.name == package.name && p.id.is_installed());
            let upd = updates.iter().find(|p| p.id.name == package.name);
            let state = match (inst, upd) {
                (None, _) => InstallState::NotInstalled,
                (Some(i), Some(u)) => InstallState::UpdateAvailable {
                    installed: i.id.version.clone(),
                    available: u.id.version.clone(),
                },
                (Some(i), None) => InstallState::Installed {
                    version: i.id.version.clone(),
                },
            };
            Entry { package, state }
        })
        .collect()
}

/// The packages Reef itself installed, kept on the phone so ownership does
/// not depend on the Reef repository still offering them (disabled
/// repository, withdrawn build). One JSON array of RPM names, written
/// atomically after each successful install or removal.
#[derive(Debug, Clone)]
pub struct InstallLedger {
    path: PathBuf,
}

impl InstallLedger {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// `$XDG_DATA_HOME/shipwright-reef/installed.json`, or under
    /// `~/.local/share`.
    pub fn default_path() -> Option<PathBuf> {
        let env = |v: &str| {
            std::env::var_os(v)
                .filter(|x| !x.is_empty())
                .map(PathBuf::from)
        };
        let base = env("XDG_DATA_HOME").or_else(|| env("HOME").map(|h| h.join(".local/share")))?;
        Some(base.join("shipwright-reef/installed.json"))
    }

    /// The recorded names; empty when the file is missing or unreadable.
    pub fn names(&self) -> BTreeSet<String> {
        std::fs::read(&self.path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    fn update(&self, f: impl FnOnce(&mut BTreeSet<String>)) -> std::io::Result<()> {
        let mut names = self.names();
        f(&mut names);
        let bytes = serde_json::to_vec(&names)?;
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        // A name unique to this process: the app and the background check
        // may write at the same time.
        let tmp = self
            .path
            .with_extension(format!("json.{}.tmp", std::process::id()));
        std::fs::write(&tmp, bytes)?;
        std::fs::rename(tmp, &self.path)
    }

    pub fn record(&self, name: &str) -> std::io::Result<()> {
        self.update(|n| {
            n.insert(name.to_string());
        })
    }

    pub fn forget(&self, name: &str) -> std::io::Result<()> {
        self.update(|n| {
            n.remove(name);
        })
    }
}

/// Store operations on top of a [`PackageManager`], scoped to Reef's repo.
pub struct Store<P> {
    pub pm: P,
    pub alias: String,
    /// What Reef installed; see [`InstallLedger`]. Without one, ownership
    /// rests on `PackageKit`'s data alone.
    pub ledger: Option<InstallLedger>,
}

impl<P: PackageManager> Store<P> {
    pub fn new(pm: P, alias: impl Into<String>) -> Self {
        Self {
            pm,
            alias: alias.into(),
            ledger: None,
        }
    }

    /// Records installs in, and reads ownership from, `ledger`.
    #[must_use]
    pub fn with_ledger(mut self, ledger: Option<InstallLedger>) -> Self {
        self.ledger = ledger;
        self
    }

    fn recorded(&self) -> BTreeSet<String> {
        self.ledger
            .as_ref()
            .map(InstallLedger::names)
            .unwrap_or_default()
    }

    /// Installed packages among the catalogue's names that are Reef's: see
    /// [`reef_owned`]. A catalogue entry that names an OS package (the
    /// catalogue is not signed) does not make Reef show it as installed.
    pub async fn installed(&self, catalogue: &Catalogue) -> Result<Vec<PackageInfo>> {
        let names: Vec<&str> = catalogue.packages.iter().map(|p| p.name.as_str()).collect();
        if names.is_empty() {
            return Ok(Vec::new());
        }
        let found = resolve_with_offers(&self.pm, &names).await?;
        Ok(reef_owned(&found, &self.alias, &self.recorded()))
    }

    /// Updates from Reef's repository only.
    pub async fn updates(&self) -> Result<Vec<PackageInfo>> {
        Ok(reef_updates(&self.pm.get_updates().await?, &self.alias))
    }

    /// Installs a catalogue package by name. The caller has already checked
    /// it is visible for this release (the UI only offers visible ones).
    pub async fn install(&self, name: &str, progress: &mut dyn FnMut(Progress)) -> Result<()> {
        let id = resolve_installable(&self.pm, name, &self.alias).await?;
        self.pm.install(&[&id.raw], progress).await?;
        if let Some(ledger) = &self.ledger {
            // The install happened; a ledger that cannot be written only
            // weakens ownership to the PackageKit rules.
            let _ = ledger.record(name);
        }
        Ok(())
    }

    /// Applies the Reef update for one package. `Ok(false)` when Reef has
    /// no update for it (already up to date); updates from other
    /// repositories are never applied.
    pub async fn update(&self, name: &str, progress: &mut dyn FnMut(Progress)) -> Result<bool> {
        let updates = self.updates().await?;
        let Some(u) = updates.iter().find(|p| p.id.name == name) else {
            return Ok(false);
        };
        self.pm.update(&[u.id.raw.as_str()], progress).await?;
        Ok(true)
    }

    /// Applies every Reef update in one transaction.
    pub async fn update_all(&self, progress: &mut dyn FnMut(Progress)) -> Result<usize> {
        let updates = self.updates().await?;
        if updates.is_empty() {
            return Ok(0);
        }
        let ids: Vec<&str> = updates.iter().map(|p| p.id.raw.as_str()).collect();
        self.pm.update(&ids, progress).await?;
        Ok(updates.len())
    }

    /// Removes an installed package, only if it is Reef's ([`reef_owned`]):
    /// Reef never removes an OS package, whatever the catalogue says.
    pub async fn remove(&self, name: &str, progress: &mut dyn FnMut(Progress)) -> Result<()> {
        let found = resolve_with_offers(&self.pm, &[name]).await?;
        let owned = reef_owned(&found, &self.alias, &self.recorded());
        let id = owned
            .iter()
            .find(|p| p.id.name == name)
            .ok_or_else(|| crate::packagekit::Error::NotFound(name.into()))?;
        self.pm.remove(&[&id.id.raw], progress).await?;
        if let Some(ledger) = &self.ledger {
            let _ = ledger.forget(name);
        }
        Ok(())
    }
}

/// An unfiltered `Resolve` plus the repositories' offers. The zypp backend
/// leaves a repository's copy out of an unfiltered `Resolve` when the same
/// version is installed, so [`reef_owned`]'s "the Reef repository also
/// offers it" fallback missed every app installed at the repository's
/// current version (Pilot showed Install while installed, on the Jolla
/// Phone); a `NotInstalled` `Resolve` lists those copies.
async fn resolve_with_offers<P: PackageManager>(
    pm: &P,
    names: &[&str],
) -> Result<Vec<PackageInfo>> {
    let mut found = pm.resolve(names, &[Filter::None]).await?;
    for offer in pm.resolve(names, &[Filter::NotInstalled]).await? {
        if !found.iter().any(|p| p.id.raw == offer.id.raw) {
            found.push(offer);
        }
    }
    Ok(found)
}

/// The installed packages in `found` (an unfiltered `Resolve` and the
/// repositories' offers, [`resolve_with_offers`]) that are
/// Reef's: Reef recorded installing them (`recorded`, the
/// [`InstallLedger`], which does not depend on the repository), or
/// `PackageKit` names the origin (`installed:<alias>`), or, as the zypp
/// backend reports a plain `installed`, the Reef repository also offers the
/// same name (the fallback for installs made before the ledger, or by
/// another tool). An installed package matching none of these (an OS
/// package) is left out.
pub fn reef_owned(
    found: &[PackageInfo],
    alias: &str,
    recorded: &BTreeSet<String>,
) -> Vec<PackageInfo> {
    found
        .iter()
        .filter(|p| p.id.is_installed())
        .filter(|p| {
            recorded.contains(&p.id.name)
                || p.id.from_repo(alias)
                || (p.id.data == "installed"
                    && found
                        .iter()
                        .any(|q| q.id.name == p.id.name && q.id.data == alias))
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::tests::SAMPLE;
    use crate::packagekit::{Info, PackageId};
    use std::cell::RefCell;

    fn pkg(id: &str) -> PackageInfo {
        PackageInfo {
            info: Info::Normal,
            id: PackageId::parse(id).unwrap(),
            summary: String::new(),
        }
    }

    #[test]
    fn joins_install_state() {
        let cat = Catalogue::parse(SAMPLE.as_bytes(), "aarch64").unwrap();
        let release = SailfishRelease::parse("5.2.0.17").unwrap();
        let installed = [
            pkg("shipwright-keel-silica;0.1.0-1;aarch64;installed"),
            pkg("shipwright-shoal-bridge-airpods-ui;0.9.0-1;aarch64;installed"),
        ];
        let updates = [pkg(
            "shipwright-shoal-bridge-airpods-ui;1.0.0-1;aarch64;shipwright-reef",
        )];
        let rows = entries(&cat, &release, &installed, &updates);
        assert_eq!(rows.len(), 2);
        // Sorted by category: Audio (the bridge) before Libraries (Keel).
        assert_eq!(
            rows[0].state,
            InstallState::UpdateAvailable {
                installed: "0.9.0-1".into(),
                available: "1.0.0-1".into()
            }
        );
        assert_eq!(
            rows[1].state,
            InstallState::Installed {
                version: "0.1.0-1".into()
            }
        );
        let none = entries(&cat, &release, &[], &[]);
        assert!(none.iter().all(|e| e.state == InstallState::NotInstalled));
    }

    /// In-memory package manager recording calls.
    #[derive(Default)]
    struct Fake {
        calls: RefCell<Vec<String>>,
        available: Vec<PackageInfo>,
        updates: Vec<PackageInfo>,
        /// Copies only a `NotInstalled` resolve lists (zypp leaves a
        /// repository's copy of an installed version out of the others).
        offers: Vec<PackageInfo>,
    }

    impl PackageManager for Fake {
        async fn resolve(&self, names: &[&str], filters: &[Filter]) -> Result<Vec<PackageInfo>> {
            self.calls
                .borrow_mut()
                .push(format!("resolve {names:?} {filters:?}"));
            let offers = filters
                .contains(&Filter::NotInstalled)
                .then_some(&self.offers);
            Ok(self
                .available
                .iter()
                .chain(offers.into_iter().flatten())
                .filter(|p| names.contains(&p.id.name.as_str()))
                .cloned()
                .collect())
        }
        async fn install(&self, ids: &[&str], _: &mut dyn FnMut(Progress)) -> Result<()> {
            self.calls.borrow_mut().push(format!("install {ids:?}"));
            Ok(())
        }
        async fn update(&self, ids: &[&str], _: &mut dyn FnMut(Progress)) -> Result<()> {
            self.calls.borrow_mut().push(format!("update {ids:?}"));
            Ok(())
        }
        async fn remove(&self, ids: &[&str], _: &mut dyn FnMut(Progress)) -> Result<()> {
            self.calls.borrow_mut().push(format!("remove {ids:?}"));
            Ok(())
        }
        async fn get_updates(&self) -> Result<Vec<PackageInfo>> {
            Ok(self.updates.clone())
        }
        async fn refresh_repo(&self, _: &str) -> Result<()> {
            Ok(())
        }
    }

    #[test]
    fn only_reef_packages_count_as_installed_or_removable() {
        // B-06: the catalogue names an OS package; Reef must not claim it.
        let json = r#"{"schema":1,"release":"5.2.0.17","arch":"aarch64","packages":[
            {"name":"bash","version":"5-1","title":"Bash","summary":"s",
             "tested_on":["5.2.0.17"],"licence":{"model":"free"}},
            {"name":"weather","version":"1-1","title":"Weather","summary":"s",
             "tested_on":["5.2.0.17"],"licence":{"model":"free"}},
            {"name":"notes","version":"1-1","title":"Notes","summary":"s",
             "tested_on":["5.2.0.17"],"licence":{"model":"free"}}]}"#;
        let cat = Catalogue::parse(json.as_bytes(), "aarch64").unwrap();
        let fake = Fake {
            available: vec![
                pkg("bash;5.2-1;aarch64;installed"),
                pkg("bash;5.2-1;aarch64;jolla"),
                // zypp: plain "installed", with the Reef repository's copy.
                pkg("weather;1-1;aarch64;installed"),
                pkg("weather;1-1;aarch64;shipwright-reef"),
                // A backend that names the origin.
                pkg("notes;1-1;aarch64;installed:shipwright-reef"),
            ],
            ..Fake::default()
        };
        let store = Store::new(fake, "shipwright-reef");
        futures_lite::future::block_on(async {
            let names: Vec<String> = store
                .installed(&cat)
                .await
                .unwrap()
                .into_iter()
                .map(|p| p.id.name)
                .collect();
            assert_eq!(names, ["weather", "notes"]);
            assert!(matches!(
                store.remove("bash", &mut |_| {}).await,
                Err(crate::packagekit::Error::NotFound(_))
            ));
            store.remove("weather", &mut |_| {}).await.unwrap();
        });
        let calls = store.pm.calls.borrow();
        assert!(!calls
            .iter()
            .any(|c| c.starts_with("remove") && c.contains("bash")));
        assert_eq!(
            calls.last().unwrap(),
            r#"remove ["weather;1-1;aarch64;installed"]"#
        );
    }

    #[test]
    fn installs_stay_reefs_without_the_repository() {
        // The Reef repository is disabled (or the build withdrawn): Resolve
        // returns only the installed copy, with zypp's plain "installed".
        let dir = std::env::temp_dir().join(format!("reef-ledger-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let ledger = InstallLedger::new(dir.join("installed.json"));
        let json = r#"{"schema":1,"release":"5.2.0.17","arch":"aarch64","packages":[
            {"name":"weather","version":"1-1","title":"Weather","summary":"s",
             "tested_on":["5.2.0.17"],"licence":{"model":"free"}},
            {"name":"bash","version":"5-1","title":"Bash","summary":"s",
             "tested_on":["5.2.0.17"],"licence":{"model":"free"}}]}"#;
        let cat = Catalogue::parse(json.as_bytes(), "aarch64").unwrap();
        // Installed through Reef while the repository was there.
        let installing = Store::new(
            Fake {
                available: vec![pkg("weather;1-1;aarch64;shipwright-reef")],
                ..Fake::default()
            },
            "shipwright-reef",
        )
        .with_ledger(Some(ledger.clone()));
        futures_lite::future::block_on(installing.install("weather", &mut |_| {})).unwrap();
        assert!(ledger.names().contains("weather"));

        let store = Store::new(
            Fake {
                available: vec![
                    pkg("weather;1-1;aarch64;installed"),
                    pkg("bash;5.2-1;aarch64;installed"),
                ],
                ..Fake::default()
            },
            "shipwright-reef",
        )
        .with_ledger(Some(ledger.clone()));
        futures_lite::future::block_on(async {
            let names: Vec<String> = store
                .installed(&cat)
                .await
                .unwrap()
                .into_iter()
                .map(|p| p.id.name)
                .collect();
            assert_eq!(names, ["weather"], "bash is still not Reef's");
            store.remove("weather", &mut |_| {}).await.unwrap();
            assert!(store.remove("bash", &mut |_| {}).await.is_err());
        });
        assert!(!ledger.names().contains("weather"), "removal forgets it");
        // Without the ledger the old rule applies: no repo copy, not Reef's.
        let bare = Store::new(
            Fake {
                available: vec![pkg("weather;1-1;aarch64;installed")],
                ..Fake::default()
            },
            "shipwright-reef",
        );
        let names = futures_lite::future::block_on(bare.installed(&cat)).unwrap();
        assert!(names.is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn an_app_at_the_repository_version_is_installed() {
        // zypp: the installed copy alone in an unfiltered resolve; the Reef
        // repository's identical copy only in a NotInstalled one.
        let json = r#"{"schema":1,"release":"5.2.0.18","arch":"aarch64","packages":[
            {"name":"shoal-pilot","version":"0.1.0-5","title":"Pilot","summary":"s",
             "tested_on":["5.2.0.18"],"licence":{"model":"free"}}]}"#;
        let cat = Catalogue::parse(json.as_bytes(), "aarch64").unwrap();
        let store = Store::new(
            Fake {
                available: vec![pkg("shoal-pilot;0.1.0-5;aarch64;installed")],
                offers: vec![pkg("shoal-pilot;0.1.0-5;aarch64;shipwright-reef")],
                ..Fake::default()
            },
            "shipwright-reef",
        );
        let names: Vec<String> = futures_lite::future::block_on(store.installed(&cat))
            .unwrap()
            .into_iter()
            .map(|p| p.id.name)
            .collect();
        assert_eq!(names, ["shoal-pilot"]);
    }

    #[test]
    fn install_picks_reef_build_and_update_skips_system() {
        let fake = Fake {
            available: vec![
                // Same name in another repository: must not be chosen.
                pkg("shipwright-keel-silica;9.9-1;aarch64;someone-else"),
                pkg("shipwright-keel-silica;0.1.0-1;aarch64;shipwright-reef"),
            ],
            updates: vec![
                pkg("shipwright-keel-silica;0.2.0-1;aarch64;shipwright-reef"),
                pkg("glibc;2.99-1;aarch64;jolla"),
            ],
            ..Fake::default()
        };
        let store = Store::new(fake, "shipwright-reef");
        futures_lite::future::block_on(async {
            store
                .install("shipwright-keel-silica", &mut |_| {})
                .await
                .unwrap();
            assert_eq!(store.update_all(&mut |_| {}).await.unwrap(), 1);
            assert!(store.install("absent", &mut |_| {}).await.is_err());
            // A-06: one package, Reef's update only.
            assert!(store
                .update("shipwright-keel-silica", &mut |_| {})
                .await
                .unwrap());
            assert!(!store.update("glibc", &mut |_| {}).await.unwrap());
        });
        let calls = store.pm.calls.borrow();
        assert!(calls[1].contains("shipwright-keel-silica;0.1.0-1;aarch64;shipwright-reef"));
        assert_eq!(
            calls[2],
            r#"update ["shipwright-keel-silica;0.2.0-1;aarch64;shipwright-reef"]"#
        );
        // calls[3] resolves "absent"; then Store::update's one transaction.
        assert_eq!(calls[4], calls[2]);
        assert_eq!(calls.len(), 5, "glibc is never updated: {calls:?}");
    }
}
