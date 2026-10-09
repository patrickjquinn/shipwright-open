// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

//! The app's view of a password entry, and its mapping to KeePass fields.
//!
//! Mapping (compatible with KeePass 2 and KeePassXC):
//!
//! | EntryData      | KDBX                                                        |
//! | -------------- | ----------------------------------------------------------- |
//! | `title`        | `Title`                                                     |
//! | `username`     | `UserName`                                                  |
//! | `password`     | `Password` (protected)                                      |
//! | `urls[0]`      | `URL`                                                       |
//! | `urls[1..]`    | `KP2A_URL_1`, `KP2A_URL_2`, ... (KeePassXC's extra URLs)    |
//! | `notes`        | `Notes`                                                     |
//! | `otp`          | `otp` (protected `otpauth://` URI); read also from KeePass 2.47 `TimeOtp-*` and KeeOtp `TOTP Seed` |
//! | `fields`       | any other string field, protection flag kept                |
//! | `tags`         | entry tags                                                  |
//! | `group`        | path of the containing group below the root (read only)     |

use keepass::db::{Entry, Value};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::totp::Totp;

pub const TITLE: &str = "Title";
pub const USERNAME: &str = "UserName";
pub const PASSWORD: &str = "Password";
pub const URL: &str = "URL";
pub const NOTES: &str = "Notes";
pub const OTP: &str = "otp";
pub const EXTRA_URL_PREFIX: &str = "KP2A_URL";

/// Fields that hold TOTP settings in other KeePass clients. They are folded
/// into `otp` on read and not shown as custom fields.
const OTP_LEGACY_FIELDS: [&str; 6] = [
    "TimeOtp-Secret-Base32",
    "TimeOtp-Length",
    "TimeOtp-Period",
    "TimeOtp-Algorithm",
    "TOTP Seed",
    "TOTP Settings",
];

/// A custom string field.
#[derive(Debug, Clone, Default, PartialEq, Eq, Zeroize, ZeroizeOnDrop)]
pub struct CustomField {
    pub name: String,
    pub value: String,
    /// Protected fields are hidden in the UI and memory-protected in KDBX.
    pub protected: bool,
}

/// One entry, as the UI and importers see it. Zeroised on drop.
#[derive(Clone, Default, PartialEq, Eq, Zeroize, ZeroizeOnDrop)]
pub struct EntryData {
    /// KDBX UUID, hyphenated lower case. Empty for an entry not yet added.
    pub id: String,
    pub title: String,
    pub username: String,
    pub password: String,
    pub urls: Vec<String>,
    pub notes: String,
    /// `otpauth://` URI or bare base32 secret; empty for none.
    pub otp: String,
    pub fields: Vec<CustomField>,
    pub tags: Vec<String>,
    /// Group path below the root, "/"-separated. Importers set it to file
    /// entries into folders; the vault fills it on read.
    pub group: String,
    /// Last modification, Unix seconds (0 when unknown).
    pub modified: i64,
}

impl std::fmt::Debug for EntryData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EntryData")
            .field("id", &self.id)
            .field("title", &self.title)
            .field("username", &self.username)
            .field("password", &"[redacted]")
            .field("urls", &self.urls)
            .field(
                "otp",
                &if self.otp.is_empty() {
                    ""
                } else {
                    "[redacted]"
                },
            )
            .field("fields", &self.fields.len())
            .field("tags", &self.tags)
            .field("group", &self.group)
            .finish_non_exhaustive()
    }
}

impl EntryData {
    /// The TOTP generator for this entry, if it has a valid one.
    pub fn totp(&self) -> Option<Result<Totp, crate::Error>> {
        if self.otp.trim().is_empty() {
            None
        } else {
            Some(Totp::parse(&self.otp))
        }
    }

    /// True when every user-visible field is empty.
    pub fn is_blank(&self) -> bool {
        self.title.trim().is_empty()
            && self.username.trim().is_empty()
            && self.password.is_empty()
            && self.urls.iter().all(|u| u.trim().is_empty())
            && self.notes.trim().is_empty()
            && self.otp.trim().is_empty()
            && self.fields.is_empty()
    }

    /// Key used to spot duplicates on import: same title, user, password and
    /// first URL.
    pub(crate) fn dedup_key(&self) -> (String, String, String, String) {
        (
            self.title.trim().to_lowercase(),
            self.username.trim().to_string(),
            self.password.clone(),
            self.urls
                .first()
                .map(|u| u.trim().to_lowercase())
                .unwrap_or_default(),
        )
    }

    /// Reads a KeePass entry.
    pub(crate) fn from_kdbx(e: &Entry, id: String, group: String) -> Self {
        let get = |k: &str| e.get(k).unwrap_or_default().to_string();
        let mut urls = Vec::new();
        let main_url = get(URL);
        if !main_url.trim().is_empty() {
            urls.push(main_url);
        }
        let mut extra: Vec<(&String, &Value<String>)> = e
            .fields
            .iter()
            .filter(|(k, _)| k.starts_with(EXTRA_URL_PREFIX))
            .collect();
        extra.sort_by(|a, b| natural_key(a.0).cmp(&natural_key(b.0)));
        for (_, v) in extra {
            let v = v.get();
            if !v.trim().is_empty() && !urls.contains(v) {
                urls.push(v.clone());
            }
        }

        let otp = match Totp::from_kdbx_fields(|k| e.get(k)) {
            Some(Ok(t)) if e.get(OTP).is_none() => t.to_uri(),
            // Keep what is stored verbatim, even if we cannot parse it, so
            // saving never destroys another client's data.
            _ => e.get(OTP).unwrap_or_default().to_string(),
        };
        let legacy_otp_folded = e.get(OTP).is_none() && !otp.is_empty();

        let mut fields: Vec<CustomField> = e
            .fields
            .iter()
            .filter(|(k, _)| {
                ![TITLE, USERNAME, PASSWORD, URL, NOTES, OTP].contains(&k.as_str())
                    && !k.starts_with(EXTRA_URL_PREFIX)
                    && (!legacy_otp_folded || !OTP_LEGACY_FIELDS.contains(&k.as_str()))
            })
            .map(|(k, v)| CustomField {
                name: k.clone(),
                value: v.get().clone(),
                protected: v.is_protected(),
            })
            .collect();
        fields.sort_by(|a, b| a.name.cmp(&b.name));

        let modified = e
            .times
            .last_modification
            .map_or(0, |t| t.and_utc().timestamp());

        EntryData {
            id,
            title: get(TITLE),
            username: get(USERNAME),
            password: get(PASSWORD),
            urls,
            notes: get(NOTES),
            otp,
            fields,
            tags: e.tags.clone(),
            group,
            modified,
        }
    }

    /// Writes this entry's fields into a KeePass entry, replacing the
    /// string fields it had. Returns false (and writes nothing) when nothing
    /// would change, so that no history item is recorded for a no-op save.
    pub(crate) fn write_kdbx(&self, e: &mut Entry) -> bool {
        let mut want: Vec<(String, Value<String>)> = vec![
            (TITLE.into(), Value::unprotected(self.title.clone())),
            (USERNAME.into(), Value::unprotected(self.username.clone())),
            (PASSWORD.into(), Value::protected(self.password.clone())),
            (
                URL.into(),
                Value::unprotected(self.urls.first().cloned().unwrap_or_default()),
            ),
            (NOTES.into(), Value::unprotected(self.notes.clone())),
        ];
        for (i, u) in self.urls.iter().skip(1).enumerate() {
            let name = if i == 0 {
                EXTRA_URL_PREFIX.to_string()
            } else {
                format!("{EXTRA_URL_PREFIX}_{i}")
            };
            want.push((name, Value::unprotected(u.clone())));
        }
        let otp = self.otp.trim();
        if !otp.is_empty() {
            // Normalise a bare secret to a URI so every client reads it.
            let uri = match Totp::parse(otp) {
                Ok(t) if !otp.to_ascii_lowercase().starts_with("otpauth://") => t.to_uri(),
                _ => otp.to_string(),
            };
            want.push((OTP.into(), Value::protected(uri)));
        }
        for f in &self.fields {
            let name = f.name.trim();
            if name.is_empty() || [TITLE, USERNAME, PASSWORD, URL, NOTES, OTP].contains(&name) {
                continue;
            }
            let v = if f.protected {
                Value::protected(f.value.clone())
            } else {
                Value::unprotected(f.value.clone())
            };
            want.push((name.to_string(), v));
        }
        let mut tags: Vec<String> = self
            .tags
            .iter()
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();
        tags.dedup();

        let unchanged = want.len() == e.fields.len()
            && want.iter().all(|(k, v)| {
                e.fields.get(k).is_some_and(|cur| {
                    cur.get() == v.get() && cur.is_protected() == v.is_protected()
                })
            })
            && e.tags == tags;
        if unchanged {
            return false;
        }
        e.fields.clear();
        e.fields.extend(want);
        e.tags = tags;
        true
    }
}

/// Sort key for `KP2A_URL`, `KP2A_URL_1`, `KP2A_URL_10` in numeric order.
fn natural_key(name: &str) -> (usize, String) {
    let n = name
        .rsplit('_')
        .next()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(0);
    (n, name.to_string())
}
