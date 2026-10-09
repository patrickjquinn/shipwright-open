// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: Apache-2.0
//
// New file, added by Shipwright in 2026 (not part of upstream xmatic).

//! Matching Matrix and bridged users to the device's address book.
//!
//! Opt-in (Privacy › "Match people with my contacts"). The C++ side reads
//! the address book read-only through QtContacts (`src/contactsbridge.cpp`,
//! the qtcontacts-sqlite backend, Sailjail permission `Contacts`) and hands
//! the few fields that can match to `contacts.setIndex`. The index lives in
//! this process's memory only: never written to disk, never logged, never
//! sent anywhere. Switching the option off (or signing out) drops it.
//!
//! What can match, in order of confidence:
//!
//! 1. A Matrix ID stored on the contact (an online-account or IM detail, a
//!    `matrix:u/...` or `https://matrix.to/#/@...` URL) equal to the user.
//! 2. For a bridge ghost (`@signal_*`, `@telegram_*`): the phone numbers,
//!    usernames and e-mail addresses the bridge publishes in the member
//!    event's `com.beeper.bridge.identifiers` (`tel:+49...`,
//!    `telegram:username`, `mailto:...`), and a display name that is itself
//!    a phone number (mautrix-signal's name template falls back to the
//!    number when the contact has no profile name).
//!
//! Note on the hosted bridges: `services/deploy` sets
//! `bridge.phone_numbers_in_profile: false`, because ghost profiles are
//! global on a multi-user bridge and would show one subscriber's contacts'
//! numbers to every other subscriber. So `tel:` identifiers do not arrive
//! from our cell, and phone matching works only through display names that
//! are numbers. See README, "Native contacts".

use std::collections::BTreeMap;
use std::sync::RwLock;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::dialling::Plan;

/// How many trailing digits identify a number when one side has no country
/// code. qtcontacts-sqlite matches on 7 by default; one more makes accidental
/// matches rarer, and a match is only taken when it is unique.
const SUFFIX_DIGITS: usize = 8;

/// One address-book entry, as the C++ side sends it.
#[derive(Clone, Default, Deserialize)]
pub struct Contact {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub avatar: String,
    #[serde(default)]
    pub phones: Vec<String>,
    #[serde(default)]
    pub emails: Vec<String>,
    /// Matrix IDs found on the contact (online accounts, IM, URLs).
    #[serde(default, rename = "matrixIds")]
    pub matrix_ids: Vec<String>,
    /// Other messaging accounts: `{service, handle}`, e.g. Telegram usernames.
    #[serde(default)]
    pub accounts: Vec<Account>,
}

#[derive(Clone, Default, Deserialize)]
pub struct Account {
    #[serde(default)]
    pub service: String,
    #[serde(default)]
    pub handle: String,
}

// Personal data: a debug print shows how much, never what.
impl std::fmt::Debug for Contact {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Contact(<redacted>, {} phones)", self.phones.len())
    }
}

/// A phone number reduced for comparison: `+<digits>` where the country is
/// known, the bare digits where it is not. `plan` reads numbers saved
/// without a country code (`dialling.rs`). `None` for text with too few
/// digits to be a number.
pub fn normalize_phone(raw: &str, plan: Option<&Plan>) -> Option<String> {
    // "+44 (0)20 ...": the bracketed trunk zero is not dialled from abroad.
    let raw = raw.trim().replace("(0)", "");
    // Extensions and pauses end the number.
    let raw = raw
        .split([',', ';', 'x', 'X', 'p'])
        .next()
        .unwrap_or_default();
    let plus = raw.starts_with('+');
    if raw
        .chars()
        .any(|c| !(c.is_ascii_digit() || " -.()/+\u{a0}".contains(c)))
    {
        return None;
    }
    let digits: String = raw.chars().filter(char::is_ascii_digit).collect();
    if digits.len() < 5 {
        return None;
    }
    if plus {
        return Some(format!("+{digits}"));
    }
    if let Some(international) = plan.and_then(|plan| plan.international(&digits)) {
        return Some(international);
    }
    if let Some(rest) = digits.strip_prefix("00") {
        return Some(format!("+{rest}"));
    }
    Some(digits)
}

/// A Matrix user ID from the many ways a contact can carry one.
pub fn matrix_id_from(text: &str) -> Option<String> {
    let text = text.trim();
    let candidate = if let Some(rest) = text.strip_prefix("matrix:u/") {
        format!("@{}", rest.split(['?', '#']).next().unwrap_or_default())
    } else if let Some(at) = text.find("matrix.to/#/") {
        let rest = &text[at + "matrix.to/#/".len()..];
        rest.split(['?', '/']).next().unwrap_or_default().to_owned()
    } else {
        text.to_owned()
    };
    let candidate = candidate
        .replace("%40", "@")
        .replace("%3A", ":")
        .replace("%3a", ":");
    let (local, server) = candidate.strip_prefix('@')?.split_once(':')?;
    if local.is_empty() || server.is_empty() || candidate.contains(char::is_whitespace) {
        return None;
    }
    Some(candidate)
}

fn handle(text: &str) -> String {
    text.trim().trim_start_matches('@').to_lowercase()
}

/// What is known about one Matrix user that might match.
#[derive(Debug, Default)]
pub struct Identity {
    pub user_id: String,
    pub display_name: String,
    /// `com.beeper.bridge.identifiers` from the member event.
    pub identifiers: Vec<String>,
}

impl Identity {
    /// From a member event's content (`m.room.member`, any extra fields).
    pub fn from_member_content(user_id: &str, content: &Value) -> Self {
        let identifiers = content
            .get("com.beeper.bridge.identifiers")
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        Self {
            user_id: user_id.to_owned(),
            display_name: content
                .get("displayname")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            identifiers,
        }
    }
}

/// Which network a user belongs to, from the ghost naming of the hosted
/// bridges.
pub fn network_of(user_id: &str) -> &'static str {
    let local = user_id.strip_prefix('@').unwrap_or(user_id);
    if local.starts_with("signal_") {
        "signal"
    } else if local.starts_with("telegram_") {
        "telegram"
    } else {
        "matrix"
    }
}

/// A match, for the UI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Match {
    pub contact_id: String,
    pub name: String,
    pub avatar: String,
    /// "matrix", "phone", "username" or "email".
    pub via: &'static str,
}

impl Match {
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.contact_id,
            "name": self.name,
            "avatar": self.avatar,
            "via": self.via,
        })
    }
}

/// The address book, reduced to lookup tables.
#[derive(Default)]
pub struct Index {
    contacts: Vec<Contact>,
    by_matrix_id: BTreeMap<String, usize>,
    by_phone: BTreeMap<String, Vec<usize>>,
    by_suffix: BTreeMap<String, Vec<usize>>,
    by_email: BTreeMap<String, Vec<usize>>,
    by_telegram: BTreeMap<String, Vec<usize>>,
    plan: Option<Plan>,
}

impl Index {
    /// `plan`: how numbers without a country code are read, `None` where
    /// the country is not known.
    pub fn new(contacts: Vec<Contact>, plan: Option<Plan>) -> Self {
        let mut index = Index {
            plan,
            ..Index::default()
        };
        for (at, contact) in contacts.iter().enumerate() {
            for raw in &contact.matrix_ids {
                if let Some(id) = matrix_id_from(raw) {
                    index.by_matrix_id.entry(id).or_insert(at);
                }
            }
            for raw in &contact.phones {
                if let Some(number) = normalize_phone(raw, index.plan.as_ref()) {
                    if let Some(suffix) = suffix(&number) {
                        push_unique(&mut index.by_suffix, suffix, at);
                    }
                    push_unique(&mut index.by_phone, number, at);
                }
            }
            for raw in &contact.emails {
                let email = raw.trim().to_lowercase();
                if email.contains('@') {
                    push_unique(&mut index.by_email, email, at);
                }
            }
            for account in &contact.accounts {
                if account.service.to_lowercase().contains("telegram")
                    && !account.handle.trim().is_empty()
                {
                    push_unique(&mut index.by_telegram, handle(&account.handle), at);
                }
                // A Matrix account stored as a generic IM account.
                if let Some(id) = matrix_id_from(&account.handle) {
                    index.by_matrix_id.entry(id).or_insert(at);
                }
            }
        }
        index.contacts = contacts;
        index
    }

    pub fn len(&self) -> usize {
        self.contacts.len()
    }

    /// The dialling plan in use, for the privacy page.
    pub fn plan_json(&self) -> Value {
        match &self.plan {
            Some(plan) => json!({
                "region": plan.region,
                "callingCode": plan.calling_code,
                "source": plan.source,
            }),
            None => json!({ "source": "none" }),
        }
    }

    /// The contact `name` from the address book for one user, as the room
    /// list and push banners need it: `display_name` is all that is known
    /// without loading the member event.
    pub fn name_for(&self, user_id: &str, display_name: &str) -> Option<String> {
        let identity = Identity {
            user_id: user_id.to_owned(),
            display_name: display_name.to_owned(),
            identifiers: Vec::new(),
        };
        self.lookup(&identity).map(|found| found.name)
    }

    fn found(&self, at: usize, via: &'static str) -> Match {
        let contact = &self.contacts[at];
        Match {
            contact_id: contact.id.clone(),
            name: contact.name.clone(),
            avatar: contact.avatar.clone(),
            via,
        }
    }

    /// The one person under `key` in `table`. Several entries with the same
    /// name count as one person: the address book can hand out a merged
    /// contact next to the per-account entries it was merged from.
    fn unique(&self, table: &BTreeMap<String, Vec<usize>>, key: &str) -> Option<usize> {
        let found = table.get(key)?;
        let first = *found.first()?;
        let name = self.contacts[first].name.trim().to_lowercase();
        found
            .iter()
            .all(|&at| self.contacts[at].name.trim().to_lowercase() == name)
            .then_some(first)
    }

    fn by_number(&self, raw: &str) -> Option<usize> {
        let number = normalize_phone(raw, self.plan.as_ref())?;
        if let Some(at) = self.unique(&self.by_phone, &number) {
            return Some(at);
        }
        // One side without a country code: the trailing digits, if only one
        // contact has them.
        self.unique(&self.by_suffix, &suffix(&number)?)
    }

    /// The contact `identity` belongs to, if exactly one fits.
    pub fn lookup(&self, identity: &Identity) -> Option<Match> {
        if let Some(&at) = self.by_matrix_id.get(&identity.user_id) {
            return Some(self.found(at, "matrix"));
        }
        // Everything below comes from a bridge; a native Matrix user's display
        // name is theirs to choose and proves nothing.
        let network = network_of(&identity.user_id);
        if network == "matrix" {
            return None;
        }
        for identifier in &identity.identifiers {
            let (scheme, value) = match identifier.split_once(':') {
                Some(parts) => parts,
                None => continue,
            };
            let at = match scheme {
                "tel" => self.by_number(value).map(|at| (at, "phone")),
                "mailto" => self
                    .unique(&self.by_email, &value.trim().to_lowercase())
                    .map(|at| (at, "email")),
                "telegram"
                    if network == "telegram" && !value.chars().all(|c| c.is_ascii_digit()) =>
                {
                    self.unique(&self.by_telegram, &handle(value))
                        .map(|at| (at, "username"))
                }
                _ => None,
            };
            if let Some((at, via)) = at {
                return Some(self.found(at, via));
            }
        }
        // mautrix-signal shows the number when the person has no profile name.
        let name = identity.display_name.trim();
        if name.starts_with('+') {
            if let Some(at) = self.by_number(name) {
                return Some(self.found(at, "phone"));
            }
        }
        None
    }
}

fn suffix(number: &str) -> Option<String> {
    let digits: String = number.chars().filter(char::is_ascii_digit).collect();
    (digits.len() >= SUFFIX_DIGITS).then(|| digits[digits.len() - SUFFIX_DIGITS..].to_owned())
}

fn push_unique(table: &mut BTreeMap<String, Vec<usize>>, key: String, at: usize) {
    let entry = table.entry(key).or_default();
    if !entry.contains(&at) {
        entry.push(at);
    }
}

/// The index of this process. `None` while the option is off.
static INDEX: RwLock<Option<Index>> = RwLock::new(None);

/// Replaces the index (option switched on, or the address book changed).
pub fn set_index(index: Index) -> usize {
    let count = index.len();
    *INDEX
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(index);
    count
}

/// Drops the index: option off, or signed out.
pub fn clear() {
    *INDEX
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
}

pub fn active() -> bool {
    INDEX.read().map(|index| index.is_some()).unwrap_or(false)
}

/// A number in international form for `start-chat`, read with the index's
/// dialling plan. `None` where the country cannot be told.
pub fn international(raw: &str) -> Option<String> {
    let guard = INDEX.read().ok()?;
    let plan = guard.as_ref().and_then(|index| index.plan.as_ref());
    normalize_phone(raw, plan).filter(|number| number.starts_with('+'))
}

/// The address-book name for one user, if matching is on and exactly one
/// contact fits.
pub fn name_for(user_id: &str, display_name: &str) -> Option<String> {
    let guard = INDEX.read().ok()?;
    guard.as_ref()?.name_for(user_id, display_name)
}

/// Looks `identity` up in the current index.
pub fn lookup(identity: &Identity) -> Option<Match> {
    let guard = INDEX.read().ok()?;
    guard.as_ref()?.lookup(identity)
}

/// Parses `contacts.setIndex`'s list. Entries that do not parse are skipped.
pub fn parse_contacts(list: &[Value]) -> Vec<Contact> {
    list.iter()
        .filter_map(|value| serde_json::from_value(value.clone()).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contact(id: &str, name: &str) -> Contact {
        Contact {
            id: id.to_owned(),
            name: name.to_owned(),
            ..Contact::default()
        }
    }

    fn book() -> Index {
        let mut anna = contact("1", "Anna");
        anna.phones = vec!["0170 123 4567".to_owned()];
        anna.emails = vec!["Anna@Example.org".to_owned()];
        let mut ben = contact("2", "Ben");
        ben.matrix_ids = vec!["https://matrix.to/#/@ben:example.org".to_owned()];
        ben.accounts = vec![Account {
            service: "Telegram".to_owned(),
            handle: "@BenTG".to_owned(),
        }];
        let mut cara = contact("3", "Cara");
        cara.phones = vec![
            "+44 20 7946 0958".to_owned(),
            "+44 (0)20 7946-0958".to_owned(),
        ];
        cara.accounts = vec![Account {
            service: "jabber".to_owned(),
            handle: "matrix:u/cara:example.org".to_owned(),
        }];
        // Two contacts sharing a number: neither is chosen.
        let mut d1 = contact("4", "Office A");
        d1.phones = vec!["+1 555 0100 200".to_owned()];
        let mut d2 = contact("5", "Office B");
        d2.phones = vec!["+1 555 0100 200".to_owned()];
        // The same person twice (a merged entry and its source): still one.
        let mut e1 = contact("6", "Dan");
        e1.phones = vec!["+33 6 12 34 56 78".to_owned()];
        let mut e2 = contact("7", "dan ");
        e2.phones = vec!["+33612345678".to_owned()];
        Index::new(
            vec![anna, ben, cara, d1, d2, e1, e2],
            crate::dialling::for_region("DE"),
        )
    }

    fn ghost(user_id: &str, name: &str, identifiers: &[&str]) -> Identity {
        Identity {
            user_id: user_id.to_owned(),
            display_name: name.to_owned(),
            identifiers: identifiers.iter().map(|s| (*s).to_owned()).collect(),
        }
    }

    #[test]
    fn normalizes_numbers() {
        let de = crate::dialling::for_region("DE");
        let de = de.as_ref();
        assert_eq!(
            normalize_phone("+49 170 1234567", None).as_deref(),
            Some("+491701234567")
        );
        assert_eq!(
            normalize_phone("0049-170/1234567", None).as_deref(),
            Some("+491701234567")
        );
        assert_eq!(
            normalize_phone("0170 1234567", de).as_deref(),
            Some("+491701234567")
        );
        assert_eq!(
            normalize_phone("0170 1234567", None).as_deref(),
            Some("01701234567")
        );
        assert_eq!(
            normalize_phone("(030) 123456;ext=12", de).as_deref(),
            Some("+4930123456")
        );
        assert_eq!(normalize_phone("112", de), None);
        assert_eq!(normalize_phone("call me", de), None);
    }

    /// The plans the old locale table did not have: the NANP, where numbers
    /// are written without their trunk prefix, and Italy, which has none.
    #[test]
    fn normalizes_numbers_with_the_sims_plan() {
        let us = crate::dialling::for_mcc("310");
        assert_eq!(
            normalize_phone("(202) 555-0123", us.as_ref()).as_deref(),
            Some("+12025550123")
        );
        assert_eq!(
            normalize_phone("1 202 555 0123", us.as_ref()).as_deref(),
            Some("+12025550123")
        );
        let it = crate::dialling::for_mcc("222");
        assert_eq!(
            normalize_phone("06 1234 5678", it.as_ref()).as_deref(),
            Some("+390612345678")
        );
        // A local number without its area code stays bare (tail matching).
        let de = crate::dialling::for_mcc("262");
        assert_eq!(
            normalize_phone("123 4567", de.as_ref()).as_deref(),
            Some("1234567")
        );
    }

    #[test]
    fn a_us_contact_matches_a_signal_ghost_named_by_its_number() {
        let mut dana = contact("1", "Dana");
        dana.phones = vec!["(202) 555-0123".to_owned()];
        let index = Index::new(vec![dana], crate::dialling::for_mcc("310"));
        let found = index
            .lookup(&ghost("@signal_us:s", "+1 202 555 0123", &[]))
            .unwrap();
        assert_eq!((found.name.as_str(), found.via), ("Dana", "phone"));
        assert_eq!(index.plan_json()["callingCode"], "1");
        assert_eq!(index.plan_json()["source"], "sim");
        assert_eq!(
            index.name_for("@signal_us:s", "+12025550123").as_deref(),
            Some("Dana")
        );
        assert_eq!(index.name_for("@bob:s", "+12025550123"), None);
    }

    #[test]
    fn reads_matrix_ids_in_their_usual_shapes() {
        assert_eq!(matrix_id_from("@a:b.org").as_deref(), Some("@a:b.org"));
        assert_eq!(
            matrix_id_from("matrix:u/a:b.org?action=chat").as_deref(),
            Some("@a:b.org")
        );
        assert_eq!(
            matrix_id_from("https://matrix.to/#/%40a%3Ab.org").as_deref(),
            Some("@a:b.org")
        );
        assert_eq!(matrix_id_from("a@b.org"), None);
        assert_eq!(matrix_id_from("@:b.org"), None);
    }

    #[test]
    fn matches_a_matrix_id_on_the_contact() {
        let index = book();
        let found = index
            .lookup(&ghost("@ben:example.org", "Benjamin", &[]))
            .unwrap();
        assert_eq!((found.name.as_str(), found.via), ("Ben", "matrix"));
        let found = index.lookup(&ghost("@cara:example.org", "", &[])).unwrap();
        assert_eq!(found.contact_id, "3");
    }

    #[test]
    fn a_native_users_display_name_proves_nothing() {
        let index = book();
        assert_eq!(
            index.lookup(&ghost(
                "@mallory:evil.org",
                "+49 170 1234567",
                &["tel:+491701234567"]
            )),
            None
        );
    }

    #[test]
    fn matches_bridge_identifiers() {
        let index = book();
        let signal = ghost(
            "@signal_0f1e:shoal.example",
            "Anna S",
            &["signal:0f1e", "tel:+491701234567"],
        );
        assert_eq!(index.lookup(&signal).unwrap().via, "phone");
        let telegram = ghost(
            "@telegram_777:shoal.example",
            "B",
            &["telegram:777", "telegram:bentg"],
        );
        let found = index.lookup(&telegram).unwrap();
        assert_eq!((found.name.as_str(), found.via), ("Ben", "username"));
        let mail = ghost(
            "@telegram_9:shoal.example",
            "x",
            &["mailto:anna@example.org"],
        );
        assert_eq!(index.lookup(&mail).unwrap().via, "email");
    }

    #[test]
    fn matches_a_signal_ghost_named_by_its_number() {
        let index = book();
        // The hosted cell publishes no tel: identifiers; the name template
        // falls back to the number.
        let found = index
            .lookup(&ghost("@signal_ab:shoal.example", "+44 20 7946 0958", &[]))
            .unwrap();
        assert_eq!((found.name.as_str(), found.via), ("Cara", "phone"));
        // A national number on the contact, a full one from the bridge.
        let found = index
            .lookup(&ghost("@signal_cd:shoal.example", "+491701234567", &[]))
            .unwrap();
        assert_eq!(found.name, "Anna");
    }

    #[test]
    fn ambiguous_numbers_match_nobody() {
        let index = book();
        assert_eq!(
            index.lookup(&ghost("@signal_x:s", "+15550100200", &[])),
            None
        );
        assert_eq!(index.lookup(&ghost("@signal_y:s", "Somebody", &[])), None);
        let found = index
            .lookup(&ghost("@signal_z:s", "+33612345678", &[]))
            .unwrap();
        assert_eq!(found.contact_id, "6");
    }

    #[test]
    fn suffix_matching_needs_a_unique_tail() {
        // No country code known: the contact's number is national.
        let mut anna = contact("1", "Anna");
        anna.phones = vec!["01701234567".to_owned()];
        let index = Index::new(vec![anna], None);
        assert_eq!(
            index
                .lookup(&ghost("@signal_a:s", "+49 170 1234567", &[]))
                .unwrap()
                .name,
            "Anna"
        );
        assert_eq!(
            index.lookup(&ghost("@signal_b:s", "+49 171 1234567", &[])),
            None
        );
    }

    #[test]
    fn reads_identifiers_from_the_member_event() {
        let content = json!({
            "membership": "join",
            "displayname": "+491701234567",
            "com.beeper.bridge.identifiers": ["signal:0f1e", "tel:+491701234567"],
        });
        let identity = Identity::from_member_content("@signal_0f1e:s", &content);
        assert_eq!(identity.identifiers.len(), 2);
        assert_eq!(identity.display_name, "+491701234567");
        assert_eq!(network_of("@telegram_1:s"), "telegram");
        assert_eq!(network_of("@bob:s"), "matrix");
    }

    #[test]
    fn the_index_parses_and_never_prints_data() {
        let parsed = parse_contacts(&[
            json!({ "id": "1", "name": "Anna", "phones": ["+491701234567"] }),
            json!("not a contact"),
        ]);
        assert_eq!(parsed.len(), 1);
        let debug = format!("{:?}", parsed[0]);
        assert!(!debug.contains("Anna") && !debug.contains("4917"));
    }
}
