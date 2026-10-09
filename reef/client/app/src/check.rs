// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

//! `shipwright-reef --check-updates`: the background update check behind
//! the "Check in the background" setting. No window and no Qt: the systemd
//! user timer `shipwright-reef-check.timer` (packaging/systemd/) starts it
//! daily, and it decides from the setting whether a check is due (never,
//! daily, or weekly). A check is the same refresh the app runs: catalogue
//! (cached for the next start), `ssu lr`, `PackageKit`'s Reef updates, and
//! licence refreshes. When Reef updates are waiting, it posts one
//! notification through `org.freedesktop.Notifications` (Lipstick on a
//! phone).

use std::path::{Path, PathBuf};

use reef_backend::catalogue::Catalogue;
use reef_backend::licences::LicenceStore;
use reef_backend::release;

use crate::jobs::{self, RefreshInput};
use crate::settings::Settings;

/// Whether a check is due: `days` is the setting (0 never, 1 daily, 7
/// weekly), `last` the time of the last completed check. An hour of slack
/// keeps a daily timer that fires a little early from skipping a day. A
/// last check in the future means the clock was wrong when it was recorded
/// (a phone's RTC at boot): it is due, rather than waiting until the clock
/// catches up.
pub fn due(days: i32, last: Option<i64>, now: i64) -> bool {
    const SLACK: i64 = 3600;
    match (days, last) {
        (d, _) if d <= 0 => false,
        (_, None) => true,
        (_, Some(last)) if last > now => true,
        (d, Some(last)) => now.saturating_sub(last) >= i64::from(d) * 86_400 - SLACK,
    }
}

/// `$XDG_CACHE_HOME/shipwright-reef/last-check`: Unix seconds of the last
/// completed check.
pub fn stamp_path() -> Option<PathBuf> {
    jobs::cache_dir().map(|d| d.join("last-check"))
}

fn read_stamp(path: &Path) -> Option<i64> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

/// Runs one background check if the setting says it is due. Returns the
/// process exit status: 0 for "nothing to do" and for a check that ran
/// (its errors are logged), 1 when the phone's release is unknown.
pub fn run() -> i32 {
    let settings = Settings::default_path()
        .as_deref()
        .map(Settings::load)
        .unwrap_or_default();
    let now = jobs::unix_now();
    let stamp = stamp_path();
    let last = stamp.as_deref().and_then(read_stamp);
    if !due(settings.background_check_days, last, now) {
        log::info!(
            "background check not due (every {} day(s); last {last:?})",
            settings.background_check_days
        );
        return 0;
    }
    let root = jobs::sysroot();
    let Some(release) = release::detect(&root) else {
        log::error!("cannot determine the installed Sailfish OS release");
        return 1;
    };
    let arch = release::native_arch().to_string();
    let cache_path = jobs::cache_path();
    let previous = cache_path
        .as_deref()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| Catalogue::parse(&b, &arch).ok());
    let out = jobs::refresh(&RefreshInput {
        config: jobs::repo_config(&root),
        release: Some(release),
        arch,
        previous,
        licence_dir: LicenceStore::default_dir(),
        licence_server: settings.licence_server.clone(),
        cache_path,
    });
    for e in &out.errors {
        log::warn!("background check: {e}");
    }
    if let Some(path) = &stamp {
        let written = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(path, now.to_string()));
        if let Err(e) = written {
            log::warn!("recording the check in {}: {e}", path.display());
        }
    }
    let updates = out.pk.as_ref().map_or(0, |pk| pk.updates.len());
    log::info!("background check done: {updates} Reef update(s)");
    if updates > 0 {
        if let Err(e) = notify(updates) {
            log::warn!("posting the update notification: {e}");
        }
    }
    0
}

/// One notification: "Reef: N updates". The text is English: this path
/// runs without Qt, so it has no translations (see the app README).
fn notify(count: usize) -> Result<(), String> {
    use std::collections::HashMap;
    futures_lite::future::block_on(async {
        let conn = zbus::Connection::session()
            .await
            .map_err(|e| e.to_string())?;
        let summary = "Reef";
        let body = if count == 1 {
            "1 app update is available".to_string()
        } else {
            format!("{count} app updates are available")
        };
        let hints: HashMap<&str, zbus::zvariant::Value<'_>> = HashMap::new();
        conn.call_method(
            Some("org.freedesktop.Notifications"),
            "/org/freedesktop/Notifications",
            Some("org.freedesktop.Notifications"),
            "Notify",
            &(
                "shipwright-reef",
                0u32,
                "icon-lock-information",
                summary,
                body.as_str(),
                Vec::<&str>::new(),
                hints,
                -1i32,
            ),
        )
        .await
        .map(drop)
        .map_err(|e| e.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = 86_400;

    #[test]
    fn checks_follow_the_setting() {
        let now = 1_790_000_000;
        // Never.
        assert!(!due(0, None, now));
        assert!(!due(0, Some(now - 30 * DAY), now));
        // First run.
        assert!(due(1, None, now));
        assert!(due(7, None, now));
        // Daily, with an hour of slack for a timer that fires early.
        assert!(!due(1, Some(now - 3600), now));
        assert!(due(1, Some(now - DAY + 1800), now));
        assert!(due(1, Some(now - 2 * DAY), now));
        // Weekly.
        assert!(!due(7, Some(now - 3 * DAY), now));
        assert!(due(7, Some(now - 7 * DAY), now));
        // A recorded time in the future (a wrong clock then): due now. The
        // next check records the corrected time, so this does not repeat.
        assert!(due(1, Some(now + DAY), now));
        assert!(due(7, Some(now + 365 * DAY), now));
        assert!(!due(0, Some(now + DAY), now));
    }
}
