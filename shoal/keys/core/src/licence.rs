// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

//! The optional licence, and the feature it unlocks.
//!
//! Shoal Keys is free: storing, generating, TOTP, import, export and
//! auto-lock never need a licence, and a lapsed licence never locks anyone
//! out of their data. The licence unlocks the **password health report**
//! ([`health_report`]): weak, reused and old passwords, and logins that
//! have no one-time password.
//!
//! The policy shared by every paid Shipwright app is in
//! `reef/licence/README.md`, "Licence policy for Shipwright apps". The
//! licence is a `reef-licence` v1 token for app id [`crate::APP_ID`], one-off
//! or subscription (any valid token for the app), verified offline against
//! [`EMBEDDED_KEYS`] with the default time rules (5 minutes of skew, 7 days
//! of subscription grace). At start-up, and whenever the app wants to
//! re-check (for example when it returns to the foreground after a purchase
//! in Reef), it asks Reef over the session bus ([`adopt_from_reef`], the
//! licence hand-off in `reef_licence::handoff`); [`adopt`] does not ask at
//! all while a fully valid licence is stored, so a re-check is cheap. The
//! user can still paste the token in Settings. Either way it is kept as
//! `licence.token` next to the vault.

use std::collections::HashMap;
use std::path::Path;

use reef_licence::{KeySet, Licence, Policy, Standing};

use crate::entry::EntryData;
use crate::generator::{estimate_entropy, Strength};
use crate::Error;

/// Licence-service public keys, `(kid, base64url)`, set at build time from
/// `SHIPWRIGHT_LICENCE_KEYS` (`kid:base64url`, comma-separated, as
/// `shipwright-licence-service public-key` prints it; see build.rs and
/// `reef_licence::build`). Unset, the set is empty and rejects every token,
/// the safe failure. A release build refuses `staging*` and `test*` key ids.
pub const EMBEDDED_KEYS: &[(&str, &str)] = include!(concat!(env!("OUT_DIR"), "/licence_keys.rs"));

pub const LICENCE_FILE: &str = "licence.token";

/// What the app knows about its licence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LicenceState {
    /// No token stored.
    Free,
    /// A token that verified. `grace_ends` is set for a subscription past
    /// expiry that is still usable.
    Licensed {
        licence_id: String,
        grace_ends: Option<i64>,
    },
    /// A stored token that does not verify (wrong app, expired, bad
    /// signature). Features stay locked; the message says why.
    Invalid(String),
}

impl LicenceState {
    pub fn is_licensed(&self) -> bool {
        matches!(self, LicenceState::Licensed { .. })
    }

    /// One line for the settings page.
    pub fn describe(&self) -> String {
        match self {
            LicenceState::Free => "Free".into(),
            LicenceState::Licensed {
                grace_ends: None, ..
            } => "Licensed".into(),
            LicenceState::Licensed {
                grace_ends: Some(_),
                ..
            } => "Licensed (renewal due)".into(),
            LicenceState::Invalid(why) => format!("Licence not valid: {why}"),
        }
    }
}

/// Verifies `token` for Shoal Keys.
pub fn check(token: &str, keys: &KeySet, now: i64) -> LicenceState {
    let token = token.trim();
    if token.is_empty() {
        return LicenceState::Free;
    }
    match Licence::verify(token, crate::APP_ID, keys, &Policy::default(), now) {
        Ok(l) => LicenceState::Licensed {
            licence_id: l.claims.lid,
            grace_ends: match l.standing {
                Standing::Active => None,
                Standing::Grace { grace_ends, .. } => Some(grace_ends),
            },
        },
        Err(e) => LicenceState::Invalid(e.to_string()),
    }
}

/// The keys this build trusts.
pub fn embedded_keys() -> KeySet {
    KeySet::from_embedded(EMBEDDED_KEYS).unwrap_or_default()
}

/// Reads and checks `<dir>/licence.token`.
pub fn load(dir: &Path, keys: &KeySet, now: i64) -> LicenceState {
    match std::fs::read_to_string(dir.join(LICENCE_FILE)) {
        Ok(t) => check(&t, keys, now),
        Err(_) => LicenceState::Free,
    }
}

/// Verifies `token` and, only if it is valid, stores it in `dir`.
pub fn install(dir: &Path, token: &str, keys: &KeySet, now: i64) -> Result<LicenceState, Error> {
    let state = check(token, keys, now);
    match &state {
        LicenceState::Licensed { .. } => {
            std::fs::create_dir_all(dir)?;
            crate::store::write_atomic(&dir.join(LICENCE_FILE), token.trim().as_bytes())?;
            Ok(state)
        }
        LicenceState::Invalid(why) => Err(Error::Licence(why.clone())),
        LicenceState::Free => Err(Error::Licence("no token given".into())),
    }
}

/// Start-up licence hand-off: when no fully valid licence is stored (none,
/// invalid, or a subscription in grace), asks `fetch` for a token and
/// installs it if it verifies. Returns the state afterwards. A token from
/// `fetch` is treated exactly as a pasted one; a bad one is ignored.
pub fn adopt(
    dir: &Path,
    keys: &KeySet,
    now: i64,
    fetch: impl FnOnce() -> Option<String>,
) -> LicenceState {
    let current = load(dir, keys, now);
    if let LicenceState::Licensed {
        grace_ends: None, ..
    } = current
    {
        return current;
    }
    match fetch() {
        Some(token) => match check(&token, keys, now) {
            LicenceState::Licensed { .. } => install(dir, &token, keys, now).unwrap_or(current),
            _ => current,
        },
        None => current,
    }
}

/// [`adopt`] with the token Reef holds, fetched over the session bus.
/// Without Reef, or without the `ShipwrightLicences` Sailjail permission,
/// this changes nothing and the user pastes the token as before.
#[cfg(feature = "reef-handoff")]
pub fn adopt_from_reef(dir: &Path, keys: &KeySet, now: i64) -> LicenceState {
    adopt(dir, keys, now, || {
        reef_licence::handoff::fetch(crate::APP_ID).unwrap_or_else(|e| {
            eprintln!("shoal-keys: {e}");
            None
        })
    })
}

/// Removes the stored token.
pub fn remove(dir: &Path) -> Result<(), Error> {
    match std::fs::remove_file(dir.join(LICENCE_FILE)) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}

/// A problem the health report found with one entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Issue {
    /// Estimated "Weak" or "Very weak" (below 36 bits).
    Weak,
    /// The same password is used by another entry.
    Reused,
    /// Not changed for more than [`OLD_AFTER_SECS`].
    Old,
    /// A login with a URL but no one-time password.
    NoTotp,
}

impl Issue {
    pub fn label(self) -> &'static str {
        match self {
            Issue::Weak => "Weak password",
            Issue::Reused => "Reused password",
            Issue::Old => "Not changed for over a year",
            Issue::NoTotp => "No one-time password",
        }
    }
}

pub const OLD_AFTER_SECS: i64 = 365 * 24 * 60 * 60;

/// One entry with its issues. No password is included.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthItem {
    pub id: String,
    pub title: String,
    pub issues: Vec<Issue>,
}

/// The licensed password health report. Returns [`Error::Licence`] when
/// `state` is not licensed.
pub fn health_report(
    state: &LicenceState,
    entries: &[EntryData],
    now: i64,
) -> Result<Vec<HealthItem>, Error> {
    if !state.is_licensed() {
        return Err(Error::Licence(
            "the password health report needs a Shoal Keys licence".into(),
        ));
    }
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for e in entries.iter().filter(|e| !e.password.is_empty()) {
        *counts.entry(e.password.as_str()).or_default() += 1;
    }
    let mut out = Vec::new();
    for e in entries {
        let mut issues = Vec::new();
        if !e.password.is_empty() {
            if matches!(
                Strength::from_bits(estimate_entropy(&e.password)),
                Strength::VeryWeak | Strength::Weak
            ) {
                issues.push(Issue::Weak);
            }
            if counts.get(e.password.as_str()).copied().unwrap_or(0) > 1 {
                issues.push(Issue::Reused);
            }
            if e.modified > 0 && now - e.modified > OLD_AFTER_SECS {
                issues.push(Issue::Old);
            }
        }
        if !e.urls.is_empty() && !e.password.is_empty() && e.otp.trim().is_empty() {
            issues.push(Issue::NoTotp);
        }
        if !issues.is_empty() {
            out.push(HealthItem {
                id: e.id.clone(),
                title: e.title.clone(),
                issues,
            });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use reef_licence::{Claims, Plan, Signer};

    const NOW: i64 = 1_800_000_000;

    fn signer() -> Signer {
        Signer::from_seed("test-k", [5; 32]).unwrap()
    }

    fn keys() -> KeySet {
        let mut k = KeySet::new();
        k.insert_text(&signer().public_key_text()).unwrap();
        k
    }

    fn token(app: &str, plan: Plan, exp: Option<i64>) -> String {
        signer()
            .issue(&Claims {
                app: app.into(),
                exp,
                iat: NOW - 40 * 86_400,
                kid: "test-k".into(),
                lid: "lic_abc".into(),
                plan,
            })
            .unwrap()
    }

    #[test]
    fn verifies_tokens() {
        assert_eq!(check("", &keys(), NOW), LicenceState::Free);
        let ok = check(
            &token("shipwright-shoal-keys", Plan::OneOff, None),
            &keys(),
            NOW,
        );
        assert!(ok.is_licensed());
        assert_eq!(ok.describe(), "Licensed");
        assert!(matches!(
            check(&token("shoal-mail", Plan::OneOff, None), &keys(), NOW),
            LicenceState::Invalid(_)
        ));
        // Right app, untrusted key.
        assert!(matches!(
            check(
                &token("shipwright-shoal-keys", Plan::OneOff, None),
                &KeySet::new(),
                NOW
            ),
            LicenceState::Invalid(_)
        ));
        assert!(matches!(
            check("v1.garbage", &keys(), NOW),
            LicenceState::Invalid(_)
        ));
        // A subscription just past expiry is in grace.
        let grace = check(
            &token(
                "shipwright-shoal-keys",
                Plan::Subscription,
                Some(NOW - 3600),
            ),
            &keys(),
            NOW,
        );
        assert!(matches!(
            grace,
            LicenceState::Licensed {
                grace_ends: Some(_),
                ..
            }
        ));
    }

    #[test]
    fn adopt_takes_a_valid_token_from_reef_once() {
        let dir = crate::store::tests::tempdir("adopt");
        let good = token("shipwright-shoal-keys", Plan::OneOff, None);
        // Nothing from Reef, or another app's token: nothing changes.
        assert_eq!(adopt(&dir, &keys(), NOW, || None), LicenceState::Free);
        let other = token("shipwright-shoal-mail", Plan::OneOff, None);
        assert_eq!(
            adopt(&dir, &keys(), NOW, || Some(other)),
            LicenceState::Free
        );
        assert!(!dir.join(LICENCE_FILE).exists());
        // A valid one is installed ...
        assert!(adopt(&dir, &keys(), NOW, || Some(good.clone())).is_licensed());
        assert!(load(&dir, &keys(), NOW).is_licensed());
        // ... and once licensed, Reef is not asked again.
        let state = adopt(&dir, &keys(), NOW, || panic!("must not ask Reef"));
        assert!(state.is_licensed());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn subscriptions_are_accepted_and_renewed_in_grace() {
        let dir = crate::store::tests::tempdir("adopt-sub");
        // A current subscription installs like a one-off.
        let current = token(
            "shipwright-shoal-keys",
            Plan::Subscription,
            Some(NOW + 30 * 86_400),
        );
        let state = install(&dir, &current, &keys(), NOW).unwrap();
        assert_eq!(
            state,
            LicenceState::Licensed {
                licence_id: "lic_abc".into(),
                grace_ends: None
            }
        );
        let state = adopt(&dir, &keys(), NOW, || panic!("must not ask Reef"));
        assert!(state.is_licensed());
        // Past its expiry, inside the grace period: still licensed (the
        // health report works), and the hand-off asks Reef for a renewal.
        let later = NOW + 32 * 86_400;
        let grace = load(&dir, &keys(), later);
        assert!(matches!(
            grace,
            LicenceState::Licensed {
                grace_ends: Some(_),
                ..
            }
        ));
        assert_eq!(grace.describe(), "Licensed (renewal due)");
        assert!(health_report(&grace, &[], later).is_ok());
        let renewed = token(
            "shipwright-shoal-keys",
            Plan::Subscription,
            Some(NOW + 400 * 86_400),
        );
        let mut asked = false;
        let state = adopt(&dir, &keys(), later, || {
            asked = true;
            Some(renewed)
        });
        assert!(asked);
        assert_eq!(
            state,
            LicenceState::Licensed {
                licence_id: "lic_abc".into(),
                grace_ends: None
            }
        );
        // Reef holding nothing better leaves the grace licence in place.
        let dir2 = crate::store::tests::tempdir("adopt-sub-2");
        install(&dir2, &current, &keys(), NOW).unwrap();
        assert!(adopt(&dir2, &keys(), later, || None).is_licensed());
        // After the grace period it is no longer valid; only the report
        // locks again, the vault never does.
        let after = NOW + 30 * 86_400 + 300 + 7 * 86_400 + 1;
        assert!(matches!(
            load(&dir2, &keys(), after),
            LicenceState::Invalid(_)
        ));
        std::fs::remove_dir_all(dir).ok();
        std::fs::remove_dir_all(dir2).ok();
    }

    #[cfg(feature = "reef-handoff")]
    #[test]
    fn adopt_from_reef_without_reef_changes_nothing() {
        // On the build host there is no Reef service on any session bus (or
        // no bus at all): the call must fail softly.
        let dir = crate::store::tests::tempdir("adopt-reef");
        assert_eq!(adopt_from_reef(&dir, &keys(), NOW), LicenceState::Free);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn embedded_keys_parse() {
        // Whatever SHIPWRIGHT_LICENCE_KEYS embedded (nothing by default)
        // must load at run time exactly as build.rs checked it.
        let keys = KeySet::from_embedded(EMBEDDED_KEYS).unwrap();
        assert_eq!(keys.key_ids().count(), EMBEDDED_KEYS.len());
    }

    #[test]
    fn install_only_valid_tokens() {
        let dir = crate::store::tests::tempdir("lic");
        assert_eq!(load(&dir, &keys(), NOW), LicenceState::Free);
        assert!(install(&dir, "v1.bad", &keys(), NOW).is_err());
        assert!(!dir.join(LICENCE_FILE).exists());
        install(
            &dir,
            &token("shipwright-shoal-keys", Plan::OneOff, None),
            &keys(),
            NOW,
        )
        .unwrap();
        assert!(load(&dir, &keys(), NOW).is_licensed());
        remove(&dir).unwrap();
        remove(&dir).unwrap();
        assert_eq!(load(&dir, &keys(), NOW), LicenceState::Free);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn health_report_is_gated_and_finds_issues() {
        let mk = |title: &str, pw: &str, url: &str, otp: &str, modified: i64| {
            let mut e = EntryData::default();
            e.id = title.into();
            e.title = title.into();
            e.password = pw.into();
            if !url.is_empty() {
                e.urls.push(url.into());
            }
            e.otp = otp.into();
            e.modified = modified;
            e
        };
        let entries = vec![
            mk("a", "password1", "https://a", "", NOW),
            mk("b", "password1", "", "", NOW),
            mk(
                "c",
                "Xq7#vR2!mZ9$kL4@pW8&",
                "https://c",
                "JBSWY3DPEHPK3PXP",
                NOW - 2 * OLD_AFTER_SECS,
            ),
            mk(
                "d",
                "Xq7#vR2!mZ9$kL4@pW8&yT",
                "https://d",
                "JBSWY3DPEHPK3PXP",
                NOW,
            ),
        ];
        assert!(health_report(&LicenceState::Free, &entries, NOW).is_err());
        let lic = LicenceState::Licensed {
            licence_id: "x".into(),
            grace_ends: None,
        };
        let r = health_report(&lic, &entries, NOW).unwrap();
        let get = |id: &str| r.iter().find(|h| h.id == id).map(|h| h.issues.clone());
        assert_eq!(
            get("a"),
            Some(vec![Issue::Weak, Issue::Reused, Issue::NoTotp])
        );
        assert_eq!(get("b"), Some(vec![Issue::Weak, Issue::Reused]));
        assert_eq!(get("c"), Some(vec![Issue::Old]));
        assert_eq!(get("d"), None);
    }
}
