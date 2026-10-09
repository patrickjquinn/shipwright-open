// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

//! Pure conversions from reef-backend types to what the QML sees: list
//! model rows, the `packageDetails()` map, and the small strings behind the
//! `Reef` properties (`repoState`, `busyText`, `progress`). Nothing here
//! touches Qt, so every rule is unit-tested without a QML engine; the bridge
//! turns [`Value`]s into `QVariant`s.
//!
//! The role names and values are the contract in reef/client/ui/README.md
//! ("Backend object API").

use reef_backend::catalogue::{parse_date, Catalogue, LicenceModel, Package};
use reef_backend::licences::StoredLicence;
use reef_backend::packagekit::{PackageInfo, Status};
use reef_backend::release::SailfishRelease;
use reef_backend::repo::{PinState, SsuRepo};
use reef_backend::store::{entries, InstallState};
use reef_licence::{Claims, Error as LicenceError, Plan, Standing};

/// One cell of a row, or one value of the details map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Str(String),
    Bool(bool),
    Int(i64),
    List(Vec<String>),
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::Str(s.to_string())
    }
}

impl From<String> for Value {
    fn from(s: String) -> Self {
        Value::Str(s)
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Value::Bool(b)
    }
}

/// Roles of `catalogueModel` and `installedModel`, in column order.
pub const PACKAGE_ROLES: &[&str] = &[
    "name",
    "title",
    "summary",
    "category",
    "version",
    "installState",
    "installedVersion",
    "licenceModel",
    "licensed",
    "publisher",
    "iconPath",
    "featured",
    "isNew",
];

/// A package is "new" for this many days after its `added` date.
pub const NEW_DAYS: i64 = 30;

/// Roles of `licenceModel`, in column order.
pub const LICENCE_ROLES: &[&str] = &[
    "appId",
    "title",
    "plan",
    "status",
    "expiresText",
    "licenceId",
];

/// One row of `catalogueModel` / `installedModel`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageRow {
    pub name: String,
    pub title: String,
    pub summary: String,
    pub category: String,
    pub version: String,
    pub install_state: &'static str,
    pub installed_version: String,
    pub licence_model: &'static str,
    pub licensed: bool,
    pub publisher: String,
    /// Local path of the cached icon, or empty.
    pub icon_path: String,
    pub featured: bool,
    pub is_new: bool,
}

impl PackageRow {
    /// Cells in [`PACKAGE_ROLES`] order.
    pub fn values(&self) -> Vec<Value> {
        vec![
            self.name.as_str().into(),
            self.title.as_str().into(),
            self.summary.as_str().into(),
            self.category.as_str().into(),
            self.version.as_str().into(),
            self.install_state.into(),
            self.installed_version.as_str().into(),
            self.licence_model.into(),
            self.licensed.into(),
            self.publisher.as_str().into(),
            self.icon_path.as_str().into(),
            self.featured.into(),
            self.is_new.into(),
        ]
    }
}

/// Days since the epoch for a Unix time, the unit `is_new` compares in.
pub fn today(unix_now: i64) -> i64 {
    unix_now.div_euclid(86_400)
}

/// Whether a package added on `added` (`YYYY-MM-DD`) is still new today.
pub fn is_new(added: Option<&str>, today: i64) -> bool {
    added
        .and_then(parse_date)
        .is_some_and(|day| (0..=NEW_DAYS).contains(&(today - day)))
}

/// The rows whose title, summary, category, publisher or RPM name contain
/// every whitespace-separated word of `query`, case-insensitively; all rows
/// for a blank query.
pub fn filter_rows(rows: &[PackageRow], query: &str) -> Vec<PackageRow> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    rows.iter()
        .filter(|r| {
            let hay = format!(
                "{}\n{}\n{}\n{}\n{}",
                r.title, r.summary, r.category, r.publisher, r.name
            )
            .to_lowercase();
            words.iter().all(|w| hay.contains(w.as_str()))
        })
        .cloned()
        .collect()
}

/// `featuredModel`: the featured rows, in the catalogue's order.
pub fn featured_rows(rows: &[PackageRow], catalogue: &Catalogue) -> Vec<PackageRow> {
    catalogue
        .featured
        .iter()
        .filter_map(|n| rows.iter().find(|r| &r.name == n))
        .cloned()
        .collect()
}

/// `installState` and `installedVersion`.
pub fn install_state(state: &InstallState) -> (&'static str, String) {
    match state {
        InstallState::NotInstalled => ("not_installed", String::new()),
        InstallState::Installed { version } => ("installed", version.clone()),
        InstallState::UpdateAvailable { installed, .. } => ("update_available", installed.clone()),
    }
}

/// `updateVersion`: the version `PackageKit` would update to, which a cached
/// catalogue can lag behind (it may even list an older build than the one
/// installed); empty unless an update is available.
pub fn update_version(state: &InstallState) -> String {
    match state {
        InstallState::UpdateAvailable { available, .. } => available.clone(),
        _ => String::new(),
    }
}

/// `licenceModel`: `free`, `one_off` or `subscription`.
pub fn licence_model(model: &LicenceModel) -> &'static str {
    match model {
        LicenceModel::Free => "free",
        LicenceModel::OneOff { .. } => "one_off",
        LicenceModel::Subscription { .. } => "subscription",
    }
}

fn purchase_url(model: &LicenceModel) -> &str {
    match model {
        LicenceModel::Free => "",
        LicenceModel::OneOff { purchase_url, .. }
        | LicenceModel::Subscription { purchase_url, .. } => purchase_url,
    }
}

/// Whether the package is paid for and a usable licence for it is stored.
/// Free packages report false: there is nothing to license.
pub fn is_licensed(package: &Package, licensed_app: &dyn Fn(&str) -> bool) -> bool {
    package.licence.app_id().is_some_and(licensed_app)
}

/// `catalogueModel`: visible packages joined with install state
/// (`catalogue.visible` through `store::entries`), sorted by category and
/// then title, so the list's category sections each appear once. `asset`
/// maps a repository-relative icon path to its cached local path (or
/// empty); `today` is [`today`]'s value.
pub fn package_rows(
    catalogue: &Catalogue,
    release: &SailfishRelease,
    installed: &[PackageInfo],
    updates: &[PackageInfo],
    licensed_app: &dyn Fn(&str) -> bool,
    asset: &dyn Fn(&str) -> String,
    today: i64,
) -> Vec<PackageRow> {
    entries(catalogue, release, installed, updates)
        .into_iter()
        .map(|e| {
            let (state, installed_version) = install_state(&e.state);
            PackageRow {
                name: e.package.name.clone(),
                title: e.package.title.clone(),
                summary: e.package.summary.clone(),
                category: e.package.category.clone(),
                version: e.package.version.clone(),
                install_state: state,
                installed_version,
                licence_model: licence_model(&e.package.licence),
                licensed: is_licensed(e.package, licensed_app),
                publisher: e.package.publisher.clone(),
                icon_path: e.package.icon.as_deref().map(asset).unwrap_or_default(),
                featured: catalogue.featured.contains(&e.package.name),
                is_new: is_new(e.package.added.as_deref(), today),
            }
        })
        .collect()
}

/// `installedModel`: the same rows, only those installed.
pub fn installed_rows(rows: &[PackageRow]) -> Vec<PackageRow> {
    rows.iter()
        .filter(|r| r.install_state != "not_installed")
        .cloned()
        .collect()
}

/// The `packageDetails(name)` map, in the README's key order. `asset` is as
/// for [`package_rows`]; screenshots not yet cached are left out.
pub fn package_details(
    package: &Package,
    state: &InstallState,
    licensed: bool,
    asset: &dyn Fn(&str) -> String,
) -> Vec<(&'static str, Value)> {
    let (install_state, installed_version) = install_state(state);
    vec![
        ("name", package.name.as_str().into()),
        ("title", package.title.as_str().into()),
        ("summary", package.summary.as_str().into()),
        ("description", package.description.as_str().into()),
        ("category", package.category.as_str().into()),
        ("version", package.version.as_str().into()),
        ("installState", install_state.into()),
        ("installedVersion", installed_version.into()),
        ("updateVersion", update_version(state).into()),
        ("testedOn", Value::List(package.tested_on.clone())),
        ("deviceTested", package.device_tested.into()),
        ("permissions", Value::List(package.permissions.clone())),
        ("sandboxed", package.sandboxed.into()),
        ("licenceModel", licence_model(&package.licence).into()),
        ("licensed", licensed.into()),
        ("purchaseUrl", purchase_url(&package.licence).into()),
        ("spdx", package.spdx.clone().unwrap_or_default().into()),
        ("sizeText", size_text(package.size).into()),
        (
            "keelTier",
            package.keel_tier.clone().unwrap_or_default().into(),
        ),
        (
            "homepage",
            package.homepage.clone().unwrap_or_default().into(),
        ),
        ("source", package.source.clone().unwrap_or_default().into()),
        ("publisher", package.publisher.as_str().into()),
        (
            "iconPath",
            package
                .icon
                .as_deref()
                .map(asset)
                .unwrap_or_default()
                .into(),
        ),
        (
            "screenshotPaths",
            Value::List(
                package
                    .screenshots
                    .iter()
                    .map(|s| asset(s))
                    .filter(|p| !p.is_empty())
                    .collect(),
            ),
        ),
        ("added", package.added.clone().unwrap_or_default().into()),
    ]
}

/// Install state of one visible package, for `packageDetails`.
pub fn state_of(
    catalogue: &Catalogue,
    release: &SailfishRelease,
    installed: &[PackageInfo],
    updates: &[PackageInfo],
    name: &str,
) -> Option<InstallState> {
    entries(catalogue, release, installed, updates)
        .into_iter()
        .find(|e| e.package.name == name)
        .map(|e| e.state)
}

/// Pre-formatted size (Keel v1 has no `Format`). Binary units, one decimal
/// below 10, as Silica's `Format.formatFileSize` shows them.
pub fn size_text(bytes: Option<u64>) -> String {
    const UNITS: [&str; 4] = ["kB", "MB", "GB", "TB"];
    let Some(bytes) = bytes else {
        return String::new();
    };
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    #[allow(
        clippy::cast_precision_loss,
        reason = "a size shown with at most one decimal; f64 is exact up to 8 PiB"
    )]
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if value < 10.0 {
        format!("{value:.1} {}", UNITS[unit])
    } else {
        format!("{value:.0} {}", UNITS[unit])
    }
}

/// `busyText` for a `PackageKit` status: a key the UI translates
/// (reef/client/ui/qml/components/Texts.js, `busyText`). The jobs the
/// bridge starts use the keys `checking`, `starting`, `refreshing`,
/// `repairing` and `refreshing_licences`.
pub fn status_text(status: Status) -> &'static str {
    match status {
        Status::Wait => "waiting",
        Status::Setup => "starting",
        Status::Running => "running",
        Status::Query => "querying",
        Status::Remove => "removing",
        Status::RefreshCache => "refreshing",
        Status::Download => "downloading",
        Status::Install => "installing",
        Status::Update => "updating",
        Status::Cleanup => "cleaning_up",
        Status::DepResolve => "resolving",
        Status::SigCheck => "checking_signatures",
        Status::Commit => "committing",
        Status::Finished => "finished",
        Status::Other(_) => "working",
    }
}

/// Marks a message the UI translates: `\u{1e}<key>` followed by its
/// arguments, each after a `\u{1f}`. reef/client/ui/qml/components/Texts.js
/// maps each key to a `qsTr` string; text without the marker (errors from
/// `PackageKit`, the network or reef-licence) is shown as it is.
pub const MESSAGE_MARK: char = '\u{1e}';
const MESSAGE_ARG: char = '\u{1f}';

/// A translatable message (see [`MESSAGE_MARK`]). Keys: `busy`,
/// `no_release`, `no_licence_store`, `licence_remove_failed` (error),
/// `catalogue_missing` (release, arch), `catalogue_download_failed`
/// (error), `no_licence_server`, `revocations_unsigned`, `updated` (count),
/// `up_to_date` (package), `worker_failed` (error), `licence_add_failed`
/// (error), `claim_failed` (error).
pub fn message(key: &str, args: &[&str]) -> String {
    let mut s = String::from(MESSAGE_MARK);
    s.push_str(key);
    for a in args {
        s.push(MESSAGE_ARG);
        // An argument cannot end the message early or add one.
        s.extend(
            a.chars()
                .filter(|c| *c != MESSAGE_MARK && *c != MESSAGE_ARG),
        );
    }
    s
}

/// `progress`: 0 to 100, or -1 when `PackageKit` does not know.
pub fn progress_value(percentage: Option<u32>) -> i32 {
    percentage.map_or(-1, |p| i32::try_from(p.min(100)).unwrap_or(100))
}

/// `repoState` string.
pub fn repo_state(state: &PinState) -> &'static str {
    match state {
        PinState::Pinned => "pinned",
        PinState::Missing => "missing",
        PinState::Disabled => "disabled",
        PinState::Drifted { .. } => "drifted",
    }
}

/// `registeredRepoUrl`: whatever ssu has under our alias, or empty.
pub fn registered_url(repos: &[SsuRepo], alias: &str) -> String {
    repos
        .iter()
        .find(|r| r.alias == alias)
        .map(|r| r.url.clone())
        .unwrap_or_default()
}

/// One row of `licenceModel`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LicenceRow {
    pub app_id: String,
    pub title: String,
    pub plan: &'static str,
    pub status: &'static str,
    pub expires_text: String,
    pub licence_id: String,
}

impl LicenceRow {
    /// Cells in [`LICENCE_ROLES`] order.
    pub fn values(&self) -> Vec<Value> {
        vec![
            self.app_id.as_str().into(),
            self.title.as_str().into(),
            self.plan.into(),
            self.status.into(),
            self.expires_text.as_str().into(),
            self.licence_id.as_str().into(),
        ]
    }
}

fn plan_str(plan: Plan) -> &'static str {
    match plan {
        Plan::OneOff => "one_off",
        Plan::Subscription => "subscription",
    }
}

/// `status` role: `active`, `grace`, `expired`, `revoked` or `invalid`.
pub fn licence_status(stored: &StoredLicence) -> &'static str {
    match &stored.status {
        Ok(_) if stored.revoked => "revoked",
        Ok(l) => match l.standing {
            Standing::Active => "active",
            Standing::Grace { .. } => "grace",
        },
        Err(LicenceError::Expired { .. }) => "expired",
        Err(_) => "invalid",
    }
}

/// Whether a stored licence unlocks its app now (active or in grace, not
/// revoked).
pub fn licence_usable(stored: &StoredLicence) -> bool {
    stored.status.is_ok() && !stored.revoked
}

/// Claims to show and refresh with: verified ones, or for an expired token
/// (whose signature did verify, but which `verify_any_app` no longer
/// returns claims for) the payload read by [`Claims::peek_unverified`],
/// which is display-only and never grants anything.
pub fn display_claims(stored: &StoredLicence) -> Option<Claims> {
    match &stored.status {
        Ok(l) => Some(l.claims.clone()),
        Err(LicenceError::Expired { .. }) => Claims::peek_unverified(&stored.token),
        Err(_) => None,
    }
}

/// `licenceModel` row. `title_for` looks the app id up in the catalogue.
pub fn licence_row(
    stored: &StoredLicence,
    title_for: &dyn Fn(&str) -> Option<String>,
) -> LicenceRow {
    let claims = display_claims(stored);
    LicenceRow {
        app_id: stored.app_id.clone(),
        title: title_for(&stored.app_id).unwrap_or_else(|| stored.app_id.clone()),
        plan: claims.as_ref().map_or("", |c| plan_str(c.plan)),
        status: licence_status(stored),
        expires_text: claims
            .as_ref()
            .and_then(|c| c.exp)
            .map(date_text)
            .unwrap_or_default(),
        licence_id: claims.map(|c| c.lid).unwrap_or_default(),
    }
}

/// Catalogue title for a licence app id (any package, visible or not).
pub fn title_for_app(catalogue: Option<&Catalogue>, app_id: &str) -> Option<String> {
    catalogue?
        .packages
        .iter()
        .find(|p| p.licence.app_id() == Some(app_id))
        .map(|p| p.title.clone())
}

/// `YYYY-MM-DD` (UTC) for Unix seconds. ISO 8601 rather than a localised
/// date: the Rust side has no locale data and Keel v1 has no `Format`.
pub fn date_text(unix: i64) -> String {
    // Howard Hinnant's civil_from_days.
    let days = unix.div_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use reef_backend::packagekit::{Info, PackageId};
    use reef_licence::{KeySet, Licence, Policy, Signer};

    const CATALOGUE: &str = r#"{
      "schema": 1, "release": "5.2.0.17", "arch": "aarch64",
      "packages": [
        {"name": "shipwright-keel-silica", "version": "0.1.0-1",
         "title": "Keel", "summary": "Silica for Qt6", "category": "System",
         "tested_on": ["5.2.0.17"], "sandboxed": false,
         "licence": {"model": "free"}, "size": 1536000,
         "spdx": "BSD-3-Clause", "homepage": "https://example.invalid/keel"},
        {"name": "shipwright-shoal-bridge-airpods-ui", "version": "1.0.0-1",
         "title": "Shoal Bridge", "summary": "AirPods", "category": "Audio",
         "description": "Battery and modes.", "tested_on": ["5.2.0.17", "5.2.0.18"],
         "permissions": ["Bluetooth", "Audio"],
         "licence": {"model": "one_off", "app_id": "shoal-bridge",
                     "purchase_url": "https://example.invalid/buy"}},
        {"name": "shipwright-shoal-messages", "version": "0.3.0-1",
         "title": "Messages", "summary": "Matrix", "category": "Communication",
         "tested_on": ["5.2.0.18"],
         "licence": {"model": "subscription", "app_id": "shoal-messages",
                     "purchase_url": "https://example.invalid/sub"}}
      ]}"#;

    fn cat() -> Catalogue {
        Catalogue::parse(CATALOGUE.as_bytes(), "aarch64").unwrap()
    }

    fn rel(s: &str) -> SailfishRelease {
        SailfishRelease::parse(s).unwrap()
    }

    fn pkg(id: &str) -> PackageInfo {
        PackageInfo {
            info: Info::Normal,
            id: PackageId::parse(id).unwrap(),
            summary: String::new(),
        }
    }

    fn no_asset(_: &str) -> String {
        String::new()
    }

    #[test]
    fn rows_follow_visibility_install_state_and_licences() {
        let installed = [
            pkg("shipwright-keel-silica;0.1.0-1;aarch64;installed"),
            pkg("shipwright-shoal-bridge-airpods-ui;0.9.0-1;aarch64;installed:shipwright-reef"),
        ];
        let updates = [pkg(
            "shipwright-shoal-bridge-airpods-ui;1.0.0-1;aarch64;shipwright-reef",
        )];
        let licensed = |app: &str| app == "shoal-bridge";
        let rows = package_rows(
            &cat(),
            &rel("5.2.0.17"),
            &installed,
            &updates,
            &licensed,
            &no_asset,
            0,
        );
        // Messages is not tested on 5.2.0.17: hidden. Sorted by category
        // (Audio, System), then title.
        assert_eq!(
            rows.iter().map(|r| r.title.as_str()).collect::<Vec<_>>(),
            ["Shoal Bridge", "Keel"]
        );
        assert_eq!(rows[1].install_state, "installed");
        assert_eq!(rows[1].installed_version, "0.1.0-1");
        assert_eq!(rows[1].licence_model, "free");
        assert!(!rows[1].licensed, "free packages carry no licence");
        assert_eq!(rows[0].install_state, "update_available");
        assert_eq!(rows[0].installed_version, "0.9.0-1");
        assert_eq!(rows[0].version, "1.0.0-1");
        assert_eq!(rows[0].licence_model, "one_off");
        assert!(rows[0].licensed);

        let values = rows[0].values();
        assert_eq!(values.len(), PACKAGE_ROLES.len());
        assert_eq!(
            values[0],
            Value::Str("shipwright-shoal-bridge-airpods-ui".into())
        );
        assert_eq!(values[5], Value::Str("update_available".into()));
        assert_eq!(values[8], Value::Bool(true));

        // On 5.2.0.18 only Shoal Bridge and Messages show, nothing installed.
        let rows = package_rows(&cat(), &rel("5.2.0.18"), &[], &[], &|_| false, &no_asset, 0);
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|r| r.install_state == "not_installed"));
        assert!(installed_rows(&rows).is_empty());
        // Audio before Communication.
        assert_eq!(rows[1].licence_model, "subscription");
    }

    #[test]
    fn installed_model_keeps_installed_and_upgradable() {
        let installed = [pkg("shipwright-keel-silica;0.1.0-1;aarch64;installed")];
        let rows = package_rows(
            &cat(),
            &rel("5.2.0.17"),
            &installed,
            &[],
            &|_| false,
            &no_asset,
            0,
        );
        let inst = installed_rows(&rows);
        assert_eq!(inst.len(), 1);
        assert_eq!(inst[0].name, "shipwright-keel-silica");
    }

    #[test]
    fn search_featured_new_and_icons() {
        let json = CATALOGUE
            .replace(
                r#""title": "Keel", "summary": "Silica for Qt6", "category": "System","#,
                r#""title": "Keel", "summary": "Silica for Qt6", "category": "System",
                   "publisher": "Shipwright", "icon": "assets/keel.png", "added": "2026-10-01",
                   "screenshots": ["assets/shots/k1.jpg", "assets/shots/k2.jpg"],"#,
            )
            .replace(
                r#""packages": ["#,
                r#""featured": ["shipwright-shoal-bridge-airpods-ui", "shipwright-keel-silica"], "packages": ["#,
            );
        let c = Catalogue::parse(json.as_bytes(), "aarch64").unwrap();
        // Only keel.png and k1 are cached.
        let asset = |rel: &str| match rel {
            "assets/keel.png" => "/cache/assets__keel.png".to_string(),
            "assets/shots/k1.jpg" => "/cache/assets__shots__k1.jpg".to_string(),
            _ => String::new(),
        };
        let day = parse_date("2026-10-03").unwrap();
        let rows = package_rows(&c, &rel("5.2.0.17"), &[], &[], &|_| false, &asset, day);
        let keel = rows
            .iter()
            .find(|r| r.name == "shipwright-keel-silica")
            .unwrap();
        assert_eq!(keel.publisher, "Shipwright");
        assert_eq!(keel.icon_path, "/cache/assets__keel.png");
        assert!(keel.featured && keel.is_new);
        let bridge = &rows[0];
        assert!(bridge.featured && !bridge.is_new && bridge.icon_path.is_empty());
        assert_eq!(keel.values().len(), PACKAGE_ROLES.len());

        // New for 30 days, not before the date and not after.
        assert!(is_new(Some("2026-10-01"), day));
        assert!(is_new(Some("2026-09-03"), day));
        assert!(!is_new(Some("2026-09-02"), day));
        assert!(!is_new(Some("2026-10-04"), day), "not before it exists");
        assert!(!is_new(None, day) && !is_new(Some("nonsense"), day));
        assert_eq!(today(86_400 * 3 + 5), 3);

        // Featured keeps the catalogue's order, not the list's.
        let names = |rows: &[PackageRow]| rows.iter().map(|r| r.name.clone()).collect::<Vec<_>>();
        assert_eq!(
            names(&featured_rows(&rows, &c)),
            [
                "shipwright-shoal-bridge-airpods-ui",
                "shipwright-keel-silica"
            ]
        );

        // Search: every word must match, in any field, any case.
        assert_eq!(names(&filter_rows(&rows, "")), names(&rows));
        assert_eq!(names(&filter_rows(&rows, "  ")), names(&rows));
        assert_eq!(
            names(&filter_rows(&rows, "SILICA")),
            ["shipwright-keel-silica"]
        );
        assert_eq!(
            names(&filter_rows(&rows, "shipwright audio")),
            ["shipwright-shoal-bridge-airpods-ui"]
        );
        assert_eq!(
            names(&filter_rows(&rows, "qt6 shipwright")),
            ["shipwright-keel-silica"]
        );
        assert!(filter_rows(&rows, "zebra").is_empty());

        // Details carry the publisher, the icon and the cached screenshots only.
        let p = c
            .visible_package("shipwright-keel-silica", &rel("5.2.0.17"))
            .unwrap();
        let d = package_details(p, &InstallState::NotInstalled, false, &asset);
        let get = |k: &str| d.iter().find(|(key, _)| *key == k).unwrap().1.clone();
        assert_eq!(get("publisher"), Value::Str("Shipwright".into()));
        assert_eq!(
            get("iconPath"),
            Value::Str("/cache/assets__keel.png".into())
        );
        assert_eq!(
            get("screenshotPaths"),
            Value::List(vec!["/cache/assets__shots__k1.jpg".into()])
        );
        assert_eq!(get("added"), Value::Str("2026-10-01".into()));
    }

    #[test]
    fn details_map_has_every_documented_key() {
        let c = cat();
        let p = c
            .visible_package("shipwright-shoal-bridge-airpods-ui", &rel("5.2.0.17"))
            .unwrap();
        let d = package_details(p, &InstallState::NotInstalled, false, &no_asset);
        let keys: Vec<&str> = d.iter().map(|(k, _)| *k).collect();
        assert_eq!(
            keys,
            [
                "name",
                "title",
                "summary",
                "description",
                "category",
                "version",
                "installState",
                "installedVersion",
                "updateVersion",
                "testedOn",
                "deviceTested",
                "permissions",
                "sandboxed",
                "licenceModel",
                "licensed",
                "purchaseUrl",
                "spdx",
                "sizeText",
                "keelTier",
                "homepage",
                "source",
                "publisher",
                "iconPath",
                "screenshotPaths",
                "added"
            ]
        );
        let get = |k: &str| d.iter().find(|(key, _)| *key == k).unwrap().1.clone();
        assert_eq!(
            get("testedOn"),
            Value::List(vec!["5.2.0.17".into(), "5.2.0.18".into()])
        );
        assert_eq!(
            get("permissions"),
            Value::List(vec!["Bluetooth".into(), "Audio".into()])
        );
        assert_eq!(get("sandboxed"), Value::Bool(true));
        assert_eq!(
            get("purchaseUrl"),
            Value::Str("https://example.invalid/buy".into())
        );
        assert_eq!(get("installState"), Value::Str("not_installed".into()));
        assert_eq!(get("sizeText"), Value::Str(String::new()));

        let keel = c
            .visible_package("shipwright-keel-silica", &rel("5.2.0.17"))
            .unwrap();
        let d = package_details(
            keel,
            &InstallState::Installed {
                version: "0.1.0-1".into(),
            },
            false,
            &no_asset,
        );
        let get = |k: &str| d.iter().find(|(key, _)| *key == k).unwrap().1.clone();
        assert_eq!(get("sandboxed"), Value::Bool(false));
        assert_eq!(get("spdx"), Value::Str("BSD-3-Clause".into()));
        assert_eq!(get("sizeText"), Value::Str("1.5 MB".into()));
        assert_eq!(get("installedVersion"), Value::Str("0.1.0-1".into()));
        assert_eq!(get("updateVersion"), Value::Str(String::new()));
        assert_eq!(get("purchaseUrl"), Value::Str(String::new()));

        // An update names PackageKit's version, not the catalogue's, which a
        // stale cache can leave older than the installed build.
        let d = package_details(
            keel,
            &InstallState::UpdateAvailable {
                installed: "0.1.0-9".into(),
                available: "0.1.0-10".into(),
            },
            false,
            &no_asset,
        );
        let get = |k: &str| d.iter().find(|(key, _)| *key == k).unwrap().1.clone();
        assert_eq!(get("updateVersion"), Value::Str("0.1.0-10".into()));
    }

    #[test]
    fn state_of_matches_rows() {
        let installed = [pkg("shipwright-keel-silica;0.1.0-1;aarch64;installed")];
        let c = cat();
        let r = rel("5.2.0.17");
        assert_eq!(
            state_of(&c, &r, &installed, &[], "shipwright-keel-silica"),
            Some(InstallState::Installed {
                version: "0.1.0-1".into()
            })
        );
        // Hidden on this release, so no details.
        assert_eq!(
            state_of(&c, &r, &[], &[], "shipwright-shoal-messages"),
            None
        );
    }

    #[test]
    fn sizes_statuses_progress_and_repo_state() {
        assert_eq!(size_text(None), "");
        assert_eq!(size_text(Some(512)), "512 B");
        assert_eq!(size_text(Some(2048)), "2.0 kB");
        assert_eq!(size_text(Some(200 * 1024)), "200 kB");
        assert_eq!(size_text(Some(3 * 1024 * 1024 * 1024)), "3.0 GB");
        assert_eq!(status_text(Status::Download), "downloading");
        assert_eq!(status_text(Status::Other(99)), "working");
        assert_eq!(message("busy", &[]), "\u{1e}busy");
        assert_eq!(
            message("catalogue_missing", &["5.2.0.17", "aarch64"]),
            "\u{1e}catalogue_missing\u{1f}5.2.0.17\u{1f}aarch64"
        );
        // Arguments cannot smuggle in separators.
        assert_eq!(
            message("up_to_date", &["a\u{1f}b\u{1e}c"]),
            "\u{1e}up_to_date\u{1f}abc"
        );
        assert_eq!(progress_value(None), -1);
        assert_eq!(progress_value(Some(42)), 42);
        assert_eq!(repo_state(&PinState::Pinned), "pinned");
        assert_eq!(repo_state(&PinState::Missing), "missing");
        assert_eq!(repo_state(&PinState::Disabled), "disabled");
        assert_eq!(
            repo_state(&PinState::Drifted {
                registered: "x".into()
            }),
            "drifted"
        );
        let repos = [SsuRepo {
            alias: "shipwright-reef".into(),
            url: "https://h/sailfishos/5.2.0.15/%(arch)/".into(),
            enabled: true,
        }];
        assert_eq!(registered_url(&repos, "shipwright-reef"), repos[0].url);
        assert_eq!(registered_url(&repos, "other"), "");
    }

    #[test]
    fn dates() {
        assert_eq!(date_text(0), "1970-01-01");
        assert_eq!(date_text(1_790_000_000), "2026-09-21");
        assert_eq!(date_text(951_782_400), "2000-02-29");
    }

    const T0: i64 = 1_790_000_000;
    const DAY: i64 = 86_400;

    fn signer() -> (Signer, KeySet) {
        let s = Signer::from_seed("test", [7; 32]).unwrap();
        let mut k = KeySet::new();
        k.insert_text(&s.public_key_text()).unwrap();
        (s, k)
    }

    fn stored(token: String, keys: &KeySet, now: i64, revoked: bool) -> StoredLicence {
        let status = Licence::verify_any_app(&token, keys, &Policy::default(), now);
        StoredLicence {
            app_id: "shoal-messages".into(),
            token,
            status,
            revoked,
        }
    }

    #[test]
    fn licence_rows_map_every_status() {
        let (s, keys) = signer();
        let claims = Claims {
            app: "shoal-messages".into(),
            exp: Some(T0 + 30 * DAY),
            iat: T0,
            kid: "test".into(),
            lid: "lid-1".into(),
            plan: Plan::Subscription,
        };
        let token = s.issue(&claims).unwrap();
        let title = |app: &str| (app == "shoal-messages").then(|| "Messages".to_string());

        let row = licence_row(&stored(token.clone(), &keys, T0, false), &title);
        assert_eq!(row.status, "active");
        assert_eq!(row.title, "Messages");
        assert_eq!(row.plan, "subscription");
        assert_eq!(row.licence_id, "lid-1");
        assert_eq!(row.expires_text, date_text(T0 + 30 * DAY));
        assert_eq!(row.values().len(), LICENCE_ROLES.len());

        // Past expiry, inside the 7-day grace.
        let row = licence_row(&stored(token.clone(), &keys, T0 + 32 * DAY, false), &title);
        assert_eq!(row.status, "grace");

        // Past grace: expired, but the (unverified) claims still identify it
        // for a refresh.
        let st = stored(token.clone(), &keys, T0 + 60 * DAY, false);
        assert!(!licence_usable(&st));
        let row = licence_row(&st, &title);
        assert_eq!(row.status, "expired");
        assert_eq!(row.licence_id, "lid-1");
        assert_eq!(row.plan, "subscription");

        let st = stored(token.clone(), &keys, T0, true);
        assert_eq!(licence_status(&st), "revoked");
        assert!(!licence_usable(&st));

        // Unknown key: invalid, and nothing from the payload is shown.
        let row = licence_row(&stored(token, &KeySet::new(), T0, false), &|_| None);
        assert_eq!(row.status, "invalid");
        assert_eq!(row.title, "shoal-messages");
        assert_eq!(row.plan, "");
        assert_eq!(row.licence_id, "");
        assert_eq!(row.expires_text, "");
    }

    #[test]
    fn catalogue_titles_for_licences() {
        let c = cat();
        // Found even when the package is hidden on this release.
        assert_eq!(
            title_for_app(Some(&c), "shoal-messages").as_deref(),
            Some("Messages")
        );
        assert_eq!(title_for_app(Some(&c), "nope"), None);
        assert_eq!(title_for_app(None, "shoal-messages"), None);
    }
}
