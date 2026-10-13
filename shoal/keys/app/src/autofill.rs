// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

//! Logins for other apps (Pacific, the browser): the
//! `org.shipwright.Keys.Autofill1` D-Bus interface.
//!
//! ```text
//! bus name   org.shipwright.shoal-keys.Autofill
//! object     /org/shipwright/Keys/Autofill
//! interface  org.shipwright.Keys.Autofill1
//!
//! Logins(s origin) -> s json
//!     [{"id", "title", "username"}] (at most 20) of the entries for the
//!     page's origin; never a secret. Fails with ...Error.Locked while the
//!     vault is locked (the caller then uses Fill with an empty id).
//! Fill(s origin, s id) -> s json
//!     {"username", "password"}. Keys comes to the front and asks the
//!     person: unlock if needed, then choose (an empty id) or confirm (an
//!     entry from Logins) the login to send. Fails with ...Error.Denied if
//!     they decline, ...Error.Invalid for an origin that is not a web page
//!     or an id that does not belong to it.
//! Save(s origin, s username, s password) -> s status
//!     "saved", "updated" or Denied: Keys asks the person first.
//! ```
//!
//! Nothing is ever sent without the person: `Logins` answers only while the
//! vault is open, and only titles and user names; every secret leaves
//! through a page in Keys that names the site, and only after a tap.
//!
//! A request waits for the person at most [`REQUEST_TTL`] (the D-Bus
//! default reply timeout, after which the caller has given up): then it is
//! declined and its page closes. At most [`MAX_WAITING`] wait at once; more
//! are declined at once. Locking the vault declines every waiting request.
//! Passwords held for a request, and the answer that carries one, are
//! wiped from memory when dropped ([`Secret`], [`zeroize::Zeroizing`]).
//!
//! Matching: an entry's URL matches a page origin when both have the same
//! scheme and host, or both are HTTPS and share the registrable domain (the
//! public suffix list: `alice.github.io` and `bob.github.io` do not). Keys
//! stores bare hosts too (`github.com`), which count as HTTPS.
//!
//! Sailjail: callers need the `ShoalKeysAutofill` permission (shipped by
//! Keys, `packaging/sailjail/ShoalKeysAutofill.permission`); Keys owns the
//! bus name and is D-Bus activated when not running.

use std::collections::HashMap;
use std::fmt;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::json;
use shoal_keys_core::EntryData;
use zeroize::Zeroizing;

pub const BUS_NAME: &str = "org.shipwright.shoal-keys.Autofill";
pub const OBJECT_PATH: &str = "/org/shipwright/Keys/Autofill";
pub const MAX_LOGINS: usize = 20;
/// How long a request waits for the person: the D-Bus default reply
/// timeout (libdbus and `GDBus`, 25 s), after which the caller has stopped
/// listening.
pub const REQUEST_TTL: Duration = Duration::from_secs(25);
/// Requests waiting at once; any more are declined straight away.
pub const MAX_WAITING: usize = 8;

/// A D-Bus string answer that may carry a password: wiped when dropped,
/// and never shown by `Debug`.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Secret(Zeroizing<String>);

impl Secret {
    pub fn new(s: String) -> Self {
        Self(Zeroizing::new(s))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for Secret {
    fn from(s: &str) -> Self {
        Self::new(s.to_owned())
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(..)")
    }
}

impl Serialize for Secret {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

impl zbus::zvariant::Type for Secret {
    const SIGNATURE: &'static zbus::zvariant::Signature = &zbus::zvariant::Signature::Str;
}

/// The `Fill` answer, `{"username", "password"}`, written straight into a
/// buffer that is wiped on drop and large enough never to be reallocated
/// (a reallocation would leave a copy behind in freed memory).
pub fn fill_answer(username: &str, password: &str) -> Secret {
    #[derive(Serialize)]
    struct Creds<'a> {
        username: &'a str,
        password: &'a str,
    }
    // JSON escapes a character as at most six bytes (\uXXXX).
    let cap = 32 + 6 * (username.len() + password.len());
    let mut buf = Zeroizing::new(Vec::with_capacity(cap));
    if serde_json::to_writer(&mut *buf, &Creds { username, password }).is_err() {
        return Secret::default();
    }
    // The bytes move into the String without a copy; serde_json writes UTF-8.
    let bytes = std::mem::take(&mut *buf);
    Secret::new(String::from_utf8(bytes).unwrap_or_default())
}

/// A login offered to a caller: never a secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Login {
    pub id: String,
    pub title: String,
    pub username: String,
}

fn parse_web(url: &str) -> Option<url::Url> {
    let u = url::Url::parse(url).ok()?;
    matches!(u.scheme(), "http" | "https")
        .then_some(u)
        .filter(|u| u.host_str().is_some())
}

/// A page origin as Keys accepts it: `scheme://host[:port]` of an http or
/// https URL.
pub fn normalise_origin(origin: &str) -> Option<String> {
    parse_web(origin.trim()).map(|u| u.origin().ascii_serialization())
}

fn site(host: &str) -> String {
    let h = host.trim_end_matches('.').to_ascii_lowercase();
    psl::domain_str(&h).map_or(h.clone(), str::to_owned)
}

/// Whether the entry URL `entry_url` belongs to the page `origin`.
pub fn entry_matches(entry_url: &str, origin: &str) -> bool {
    let entry = parse_web(entry_url).or_else(|| {
        // A bare host ("github.com", "github.com/login").
        (!entry_url.contains("://") && !entry_url.trim().is_empty())
            .then(|| parse_web(&format!("https://{}", entry_url.trim())))
            .flatten()
    });
    let (Some(e), Some(o)) = (entry, parse_web(origin)) else {
        return false;
    };
    let (Some(eh), Some(oh)) = (e.host_str(), o.host_str()) else {
        return false;
    };
    if e.scheme() == o.scheme() && eh.eq_ignore_ascii_case(oh) {
        return true;
    }
    e.scheme() == "https" && o.scheme() == "https" && site(eh) == site(oh)
}

pub fn entry_matches_origin(e: &EntryData, origin: &str) -> bool {
    e.urls.iter().any(|u| entry_matches(u, origin))
}

/// The logins for `origin`, by title.
pub fn logins_for(entries: &[EntryData], origin: &str) -> Vec<Login> {
    let mut out: Vec<Login> = entries
        .iter()
        .filter(|e| entry_matches_origin(e, origin))
        .map(|e| Login {
            id: e.id.clone(),
            title: e.title.clone(),
            username: e.username.clone(),
        })
        .collect();
    out.sort_by(|a, b| {
        a.title
            .to_lowercase()
            .cmp(&b.title.to_lowercase())
            .then(a.username.cmp(&b.username))
    });
    out.truncate(MAX_LOGINS);
    out
}

/// What saving a login would do: update the entry with this site and
/// user name (its id), or add a new one (None).
pub fn save_target(entries: &[EntryData], origin: &str, username: &str) -> Option<String> {
    entries
        .iter()
        .find(|e| entry_matches_origin(e, origin) && e.username == username)
        .map(|e| e.id.clone())
}

/// A new entry for a login saved from a page.
pub fn new_entry(origin: &str, username: &str, password: &str) -> EntryData {
    let host = parse_web(origin)
        .and_then(|u| u.host_str().map(str::to_owned))
        .unwrap_or_default();
    let title = host.strip_prefix("www.").unwrap_or(&host).to_owned();
    let mut e = EntryData::default();
    e.title = title;
    username.clone_into(&mut e.username);
    password.clone_into(&mut e.password);
    e.urls = vec![origin.to_owned()];
    e
}

/// Why an autofill call did not give the caller what it asked.
#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.shipwright.Keys.Autofill1.Error")]
pub enum AutofillError {
    #[zbus(error)]
    ZBus(zbus::Error),
    /// The vault is locked (Logins only).
    Locked(String),
    /// The person declined, or did not answer.
    Denied(String),
    /// Not a web origin, or an id that is not this origin's.
    Invalid(String),
}

/// What a waiting request asks the person.
#[derive(Clone, PartialEq, Eq)]
pub enum Ask {
    Fill {
        origin: String,
        id: String,
    },
    Save {
        origin: String,
        username: String,
        password: Zeroizing<String>,
    },
}

impl fmt::Debug for Ask {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Fill { origin, id } => f
                .debug_struct("Fill")
                .field("origin", origin)
                .field("id", id)
                .finish(),
            Self::Save {
                origin, username, ..
            } => f
                .debug_struct("Save")
                .field("origin", origin)
                .field("username", username)
                .finish_non_exhaustive(),
        }
    }
}

impl Ask {
    pub fn origin(&self) -> &str {
        match self {
            Self::Fill { origin, .. } | Self::Save { origin, .. } => origin,
        }
    }

    /// The request as the page in Keys sees it (no password).
    pub fn describe(&self, request: u32, logins: &[Login], update: bool) -> String {
        let host = parse_web(self.origin())
            .and_then(|u| u.host_str().map(str::to_owned))
            .unwrap_or_default();
        match self {
            Self::Fill { id, .. } => json!({
                "request": request, "kind": "fill", "origin": self.origin(), "host": host,
                "preselected": id, "logins": logins,
            }),
            Self::Save { username, .. } => json!({
                "request": request, "kind": "save", "origin": self.origin(), "host": host,
                "username": username, "update": update,
            }),
        }
        .to_string()
    }
}

/// The answer a request waits for.
pub type Reply = Result<Secret, AutofillError>;

struct Waiting {
    ask: Ask,
    reply: async_channel::Sender<Reply>,
    since: Instant,
}

impl Waiting {
    /// The caller has stopped waiting: its time is up, or it hung up.
    fn gone(&self, now: Instant) -> bool {
        self.reply.is_closed() || now.saturating_duration_since(self.since) >= REQUEST_TTL
    }
}

/// Requests waiting for the person, by number.
#[derive(Default)]
pub struct Pending {
    next: u32,
    waiting: HashMap<u32, Waiting>,
}

impl Pending {
    /// Queues a request; `None` when [`MAX_WAITING`] already wait, and the
    /// request has been declined.
    pub fn add(
        &mut self,
        ask: Ask,
        reply: async_channel::Sender<Reply>,
        now: Instant,
    ) -> Option<u32> {
        self.expire(now);
        if self.waiting.len() >= MAX_WAITING {
            let _ = reply.try_send(Err(AutofillError::Denied(
                "Shoal Keys has too many requests waiting".into(),
            )));
            return None;
        }
        self.next = self.next.wrapping_add(1).max(1);
        self.waiting.insert(
            self.next,
            Waiting {
                ask,
                reply,
                since: now,
            },
        );
        Some(self.next)
    }

    pub fn take(&mut self, request: u32) -> Option<(Ask, async_channel::Sender<Reply>)> {
        self.waiting.remove(&request).map(|w| (w.ask, w.reply))
    }

    /// The request, while its caller still waits.
    pub fn ask(&self, request: u32, now: Instant) -> Option<&Ask> {
        self.waiting
            .get(&request)
            .filter(|w| !w.gone(now))
            .map(|w| &w.ask)
    }

    /// Declines the requests whose caller has stopped waiting; whether
    /// there were any.
    pub fn expire(&mut self, now: Instant) -> bool {
        let before = self.waiting.len();
        self.waiting.retain(|_, w| {
            if w.gone(now) {
                let _ = w
                    .reply
                    .try_send(Err(AutofillError::Denied("no answer in time".into())));
                false
            } else {
                true
            }
        });
        self.waiting.len() != before
    }

    /// Declines everything (the vault locked, the app quits).
    pub fn decline_all(&mut self) {
        for (_, w) in self.waiting.drain() {
            let _ = w
                .reply
                .try_send(Err(AutofillError::Denied("Shoal Keys locked".into())));
        }
    }

    /// The requests whose caller still waits.
    pub fn ids(&self, now: Instant) -> Vec<u32> {
        let mut v: Vec<u32> = self
            .waiting
            .iter()
            .filter(|(_, w)| !w.gone(now))
            .map(|(k, _)| *k)
            .collect();
        v.sort_unstable();
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, title: &str, user: &str, urls: &[&str]) -> EntryData {
        let mut e = EntryData::default();
        e.id = id.into();
        e.title = title.into();
        e.username = user.into();
        e.password = format!("pw-{id}");
        e.urls = urls.iter().map(|u| (*u).to_string()).collect();
        e
    }

    #[test]
    fn origins_and_matching() {
        assert_eq!(
            normalise_origin("https://Login.Example.com:8443/a?b").as_deref(),
            Some("https://login.example.com:8443")
        );
        assert_eq!(normalise_origin("file:///etc/passwd"), None);
        assert_eq!(normalise_origin("javascript:alert(1)"), None);
        assert!(entry_matches(
            "https://example.com/login",
            "https://accounts.example.com"
        ));
        assert!(entry_matches("github.com", "https://github.com"));
        assert!(entry_matches("github.com/login", "https://github.com"));
        assert!(entry_matches("http://router.lan/", "http://router.lan"));
        assert!(!entry_matches(
            "https://example.com/",
            "https://example.org"
        ));
        assert!(!entry_matches(
            "http://example.com/",
            "https://a.example.com"
        ));
        assert!(!entry_matches(
            "https://alice.github.io/",
            "https://mallory.github.io"
        ));
        assert!(!entry_matches("", "https://example.com"));
        assert!(!entry_matches("https://example.com", "not an origin"));
    }

    #[test]
    fn logins_without_secrets_and_save_targets() {
        let v = vec![
            entry("1", "Work mail", "me@work", &["https://mail.example.com"]),
            entry("2", "Bank", "alice", &["https://bank.example"]),
            entry("3", "Example", "bob", &["example.com"]),
        ];
        let l = logins_for(&v, "https://www.example.com");
        assert_eq!(
            l.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(),
            ["3", "1"]
        );
        let json = serde_json::to_string(&l).unwrap();
        assert!(!json.contains("pw-"));
        assert_eq!(
            save_target(&v, "https://www.example.com", "bob").as_deref(),
            Some("3")
        );
        assert_eq!(save_target(&v, "https://www.example.com", "carol"), None);
        let n = new_entry("https://www.shop.example", "carol", "secret");
        assert_eq!(
            (n.title.as_str(), n.urls[0].as_str()),
            ("shop.example", "https://www.shop.example")
        );
    }

    fn fill(origin: &str) -> Ask {
        Ask::Fill {
            origin: origin.into(),
            id: String::new(),
        }
    }

    #[test]
    fn pending_requests() {
        let now = Instant::now();
        let mut p = Pending::default();
        let (tx, rx) = async_channel::bounded(1);
        let id = p.add(fill("https://a.example"), tx, now).unwrap();
        let d = p.ask(id, now).unwrap().describe(id, &[], false);
        assert!(d.contains("\"kind\":\"fill\"") && d.contains("\"host\":\"a.example\""));
        let (tx2, rx2) = async_channel::bounded(1);
        let save = Ask::Save {
            origin: "https://a.example".into(),
            username: "u".into(),
            password: Zeroizing::new("secret".into()),
        };
        assert!(!format!("{save:?}").contains("secret"));
        let id2 = p.add(save, tx2, now).unwrap();
        assert!(!p
            .ask(id2, now)
            .unwrap()
            .describe(id2, &[], true)
            .contains("secret"));
        assert_eq!(p.ids(now), [id, id2]);
        let (_, tx) = p.take(id).unwrap();
        tx.try_send(Ok("x".into())).unwrap();
        assert_eq!(rx.try_recv().unwrap().unwrap().as_str(), "x");
        p.decline_all();
        assert!(matches!(
            rx2.try_recv().unwrap(),
            Err(AutofillError::Denied(_))
        ));
        assert!(p.ids(now).is_empty());
    }

    #[test]
    fn requests_expire_with_the_callers_timeout() {
        let t0 = Instant::now();
        let mut p = Pending::default();
        let (tx, rx) = async_channel::bounded(1);
        let id = p.add(fill("https://a.example"), tx, t0).unwrap();
        let almost = t0 + REQUEST_TTL.saturating_sub(Duration::from_millis(1));
        assert!(p.ask(id, almost).is_some());
        assert!(!p.expire(almost));
        let late = t0 + REQUEST_TTL;
        // Gone for the page at once, declined to the caller on the next sweep.
        assert!(p.ask(id, late).is_none());
        assert!(p.ids(late).is_empty());
        assert!(p.expire(late));
        assert!(matches!(
            rx.try_recv().unwrap(),
            Err(AutofillError::Denied(_))
        ));
        assert!(p.take(id).is_none());
    }

    #[test]
    fn a_caller_that_hung_up_is_dropped() {
        let now = Instant::now();
        let mut p = Pending::default();
        let (tx, rx) = async_channel::bounded(1);
        let id = p.add(fill("https://a.example"), tx, now).unwrap();
        drop(rx);
        assert!(p.ask(id, now).is_none());
        assert!(p.expire(now));
        assert!(p.ids(now).is_empty());
    }

    #[test]
    fn the_queue_is_capped() {
        let now = Instant::now();
        let mut p = Pending::default();
        let mut keep = Vec::new();
        for _ in 0..MAX_WAITING {
            let (tx, rx) = async_channel::bounded(1);
            assert!(p.add(fill("https://a.example"), tx, now).is_some());
            keep.push(rx);
        }
        let (tx, rx) = async_channel::bounded(1);
        assert!(p.add(fill("https://a.example"), tx, now).is_none());
        assert!(matches!(
            rx.try_recv().unwrap(),
            Err(AutofillError::Denied(_))
        ));
        assert_eq!(p.ids(now).len(), MAX_WAITING);
        // Room again once the oldest have timed out.
        let later = now + REQUEST_TTL;
        let (tx, _rx) = async_channel::bounded(1);
        assert!(p.add(fill("https://a.example"), tx, later).is_some());
        assert_eq!(p.ids(later).len(), 1);
    }

    #[test]
    fn fill_answer_is_json_and_hidden_from_debug() {
        let a = fill_answer("me", "p\"w\u{1}\u{e9}");
        let v: serde_json::Value = serde_json::from_str(a.as_str()).unwrap();
        assert_eq!(v["username"], "me");
        assert_eq!(v["password"], "p\"w\u{1}\u{e9}");
        assert_eq!(format!("{a:?}"), "Secret(..)");
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            serde_json::to_string(a.as_str()).unwrap()
        );
    }
}
