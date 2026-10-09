// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

//! Importers. Each turns another manager's export into [`Imported`] items,
//! which [`crate::Vault::merge_imported`] adds (skipping duplicates).
//!
//! | Format | Source | Notes |
//! | --- | --- | --- |
//! | KDBX 3.1 / 4.x | KeePass, KeePassXC, KeePassDX, Strongbox | needs the file's password (and key file) |
//! | Bitwarden JSON | Bitwarden "Export vault", `.json` (unencrypted) | password-protected exports are refused with a message |
//! | 1Password 1PUX | 1Password 8 "Export", `.1pux` (zip) | archived items are imported into an "Archive" group; trashed ones are skipped |
//! | 1Password CSV | 1Password 7 and 8 CSV | columns matched by header name |
//! | Chrome CSV | Chrome, Edge, Brave, Vivaldi "Export passwords" | `name,url,username,password,note` |
//! | Firefox CSV | Firefox "Export Logins" | `url,username,password,httpRealm,...` |
//!
//! Non-login items (cards, identities, secure notes, SSH keys) become entries
//! whose details are custom fields; sensitive ones (card number, CVV, private
//! key) are protected fields.
//!
//! Every importer works on bytes already in memory; the caller reads the
//! file and should delete the plaintext export afterwards (the UI offers to).

use std::io::Read;

use serde_json::Value as Json;
use zeroize::Zeroizing;

use crate::entry::{CustomField, EntryData};
use crate::vault::{CompositeKey, Vault};
use crate::Error;

/// One imported entry plus its earlier passwords, oldest first.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Imported {
    pub entry: EntryData,
    pub password_history: Vec<String>,
}

impl Imported {
    fn new(entry: EntryData) -> Self {
        Self {
            entry,
            password_history: Vec::new(),
        }
    }
}

/// The formats [`import`] understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Kdbx,
    BitwardenJson,
    OnePux,
    OnePasswordCsv,
    ChromeCsv,
    FirefoxCsv,
}

impl Format {
    pub fn label(self) -> &'static str {
        match self {
            Format::Kdbx => "KeePass (KDBX)",
            Format::BitwardenJson => "Bitwarden (JSON)",
            Format::OnePux => "1Password (1PUX)",
            Format::OnePasswordCsv => "1Password (CSV)",
            Format::ChromeCsv => "Chrome (CSV)",
            Format::FirefoxCsv => "Firefox (CSV)",
        }
    }

    /// Whether the format itself is encrypted and needs a password.
    pub fn needs_password(self) -> bool {
        self == Format::Kdbx
    }
}

/// Result of parsing an export.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportResult {
    pub items: Vec<Imported>,
    /// Items in the export that were not imported (trashed, unsupported).
    pub skipped: usize,
}

/// Guesses the format from the content (and, for CSV, the header row).
pub fn detect(bytes: &[u8]) -> Option<Format> {
    if bytes.len() >= 8 && bytes[..4] == [0x03, 0xd9, 0xa2, 0x9a] {
        return Some(Format::Kdbx);
    }
    if bytes.starts_with(b"PK\x03\x04") {
        return Some(Format::OnePux);
    }
    let text = std::str::from_utf8(bytes).ok()?;
    let text = text.trim_start_matches('\u{feff}').trim_start();
    if text.starts_with('{') {
        return Some(Format::BitwardenJson);
    }
    let header = csv_header(text)?;
    let has = |n: &str| header.iter().any(|h| h == n);
    if has("httprealm") || has("formactionorigin") {
        Some(Format::FirefoxCsv)
    } else if has("otpauth") || has("archived") || (has("title") && has("password")) {
        Some(Format::OnePasswordCsv)
    } else if has("name") && has("url") && has("password") {
        Some(Format::ChromeCsv)
    } else {
        None
    }
}

/// Parses an export. `key` is needed for KDBX only.
pub fn import(
    format: Format,
    bytes: &[u8],
    key: Option<&CompositeKey>,
) -> Result<ImportResult, Error> {
    match format {
        Format::Kdbx => {
            let key = key.ok_or_else(|| Error::Import("this file needs its password".into()))?;
            kdbx(bytes, key)
        }
        Format::BitwardenJson => bitwarden_json(bytes),
        Format::OnePux => onepux(bytes),
        Format::OnePasswordCsv => onepassword_csv(bytes),
        Format::ChromeCsv => chrome_csv(bytes),
        Format::FirefoxCsv => firefox_csv(bytes),
    }
}

/// Entries of another KDBX file, with their password history.
pub fn kdbx(bytes: &[u8], key: &CompositeKey) -> Result<ImportResult, Error> {
    let v = Vault::open(bytes, key)?;
    let items = v
        .entries()
        .into_iter()
        .map(|e| {
            let mut olds: Vec<String> = v
                .history(&e.id)
                .into_iter()
                .map(|h| h.password.clone())
                .filter(|p| !p.is_empty())
                .collect();
            olds.reverse(); // history is newest first
            olds.dedup();
            let mut entry = e;
            entry.id.clear();
            Imported {
                entry,
                password_history: olds,
            }
        })
        .collect();
    Ok(ImportResult { items, skipped: 0 })
}

fn s(v: &Json, key: &str) -> String {
    match v.get(key) {
        Some(Json::String(s)) => s.clone(),
        Some(Json::Number(n)) => n.to_string(),
        Some(Json::Bool(b)) => b.to_string(),
        _ => String::new(),
    }
}

fn field(name: &str, value: impl Into<String>, protected: bool) -> Option<CustomField> {
    let value = value.into();
    if value.trim().is_empty() {
        return None;
    }
    Some(CustomField {
        name: name.to_string(),
        value,
        protected,
    })
}

/// Adds `f` to `fields`, renaming it "Name (2)" if the name is taken.
fn push_field(fields: &mut Vec<CustomField>, f: Option<CustomField>) {
    let Some(mut f) = f else { return };
    let base = if f.name.trim().is_empty() {
        "Field".to_string()
    } else {
        f.name.trim().to_string()
    };
    let reserved = ["Title", "UserName", "Password", "URL", "Notes", "otp"];
    let mut name = base.clone();
    let mut n = 2;
    while reserved.contains(&name.as_str()) || fields.iter().any(|x| x.name == name) {
        name = format!("{base} ({n})");
        n += 1;
    }
    f.name = name;
    fields.push(f);
}

// Bitwarden.

/// Fills `e` from the part of a Bitwarden item that depends on its type
/// (login, note, card, identity, SSH key). False for an unknown type.
fn bitwarden_typed(e: &mut EntryData, it: &Json) -> bool {
    match it.get("type").and_then(Json::as_i64).unwrap_or(0) {
        1 => {
            let login = it.get("login").cloned().unwrap_or(Json::Null);
            e.username = s(&login, "username");
            e.password = s(&login, "password");
            e.otp = s(&login, "totp");
            if let Some(uris) = login.get("uris").and_then(Json::as_array) {
                e.urls = uris
                    .iter()
                    .map(|u| s(u, "uri"))
                    .filter(|u| !u.trim().is_empty())
                    .collect();
            }
        }
        2 => e.tags.push("Note".into()),
        3 => {
            let c = it.get("card").cloned().unwrap_or(Json::Null);
            e.tags.push("Card".into());
            push_field(
                &mut e.fields,
                field("Cardholder", s(&c, "cardholderName"), false),
            );
            push_field(&mut e.fields, field("Brand", s(&c, "brand"), false));
            push_field(&mut e.fields, field("Number", s(&c, "number"), true));
            let exp = match (s(&c, "expMonth"), s(&c, "expYear")) {
                (m, y) if !m.is_empty() && !y.is_empty() => format!("{m}/{y}"),
                (m, y) => format!("{m}{y}"),
            };
            push_field(&mut e.fields, field("Expiry", exp, false));
            push_field(&mut e.fields, field("Security code", s(&c, "code"), true));
        }
        4 => {
            let id = it.get("identity").cloned().unwrap_or(Json::Null);
            e.tags.push("Identity".into());
            e.username = s(&id, "username");
            for (k, label, prot) in [
                ("title", "Title prefix", false),
                ("firstName", "First name", false),
                ("middleName", "Middle name", false),
                ("lastName", "Last name", false),
                ("email", "Email", false),
                ("phone", "Phone", false),
                ("company", "Company", false),
                ("address1", "Address", false),
                ("address2", "Address line 2", false),
                ("address3", "Address line 3", false),
                ("city", "City", false),
                ("state", "State", false),
                ("postalCode", "Postal code", false),
                ("country", "Country", false),
                ("ssn", "National ID", true),
                ("passportNumber", "Passport number", true),
                ("licenseNumber", "Licence number", true),
            ] {
                push_field(&mut e.fields, field(label, s(&id, k), prot));
            }
        }
        5 => {
            let k = it.get("sshKey").cloned().unwrap_or(Json::Null);
            e.tags.push("SSH key".into());
            push_field(
                &mut e.fields,
                field("Private key", s(&k, "privateKey"), true),
            );
            push_field(
                &mut e.fields,
                field("Public key", s(&k, "publicKey"), false),
            );
            push_field(
                &mut e.fields,
                field("Fingerprint", s(&k, "keyFingerprint"), false),
            );
        }
        _ => return false,
    }
    true
}

/// Bitwarden's unencrypted JSON export (personal or organisation).
pub fn bitwarden_json(bytes: &[u8]) -> Result<ImportResult, Error> {
    let root: Json = serde_json::from_slice(bytes)
        .map_err(|e| Error::Import(format!("not Bitwarden JSON: {e}")))?;
    if root.get("encrypted").and_then(Json::as_bool) == Some(true) {
        return Err(Error::Import(
            "this is an encrypted Bitwarden export; export again choosing the unencrypted .json format".into(),
        ));
    }
    let items = root
        .get("items")
        .and_then(Json::as_array)
        .ok_or_else(|| Error::Import("not a Bitwarden export (no items)".into()))?;
    let folder_name = |id: &str| -> String {
        for key in ["folders", "collections"] {
            if let Some(list) = root.get(key).and_then(Json::as_array) {
                if let Some(f) = list.iter().find(|f| s(f, "id") == id) {
                    return s(f, "name");
                }
            }
        }
        String::new()
    };

    let mut out = ImportResult::default();
    for it in items {
        let mut e = {
            let mut ed = EntryData::default();
            ed.title = s(it, "name");
            ed.notes = s(it, "notes");
            ed
        };
        let folder = s(it, "folderId");
        if !folder.is_empty() {
            e.group = folder_name(&folder);
        } else if let Some(c) = it
            .get("collectionIds")
            .and_then(Json::as_array)
            .and_then(|a| a.first())
        {
            e.group = folder_name(c.as_str().unwrap_or_default());
        }
        if it.get("favorite").and_then(Json::as_bool) == Some(true) {
            e.tags.push("Favourite".into());
        }
        if !bitwarden_typed(&mut e, it) {
            out.skipped += 1;
            continue;
        }
        if let Some(fields) = it.get("fields").and_then(Json::as_array) {
            for f in fields {
                // 0 text, 1 hidden, 2 boolean, 3 linked (skipped: it only
                // points at another field of the same item).
                let t = f.get("type").and_then(Json::as_i64).unwrap_or(0);
                if t == 3 {
                    continue;
                }
                push_field(&mut e.fields, field(&s(f, "name"), s(f, "value"), t == 1));
            }
        }
        let mut item = Imported::new(e);
        if let Some(hist) = it.get("passwordHistory").and_then(Json::as_array) {
            let mut h: Vec<(String, String)> = hist
                .iter()
                .map(|p| (s(p, "lastUsedDate"), s(p, "password")))
                .collect();
            // ISO 8601 dates sort as strings; oldest first.
            h.sort();
            item.password_history = h
                .into_iter()
                .map(|(_, p)| p)
                .filter(|p| !p.is_empty())
                .collect();
        }
        out.items.push(item);
    }
    Ok(out)
}

// 1Password.

/// The largest `export.data` a 1PUX import accepts, declared or inflated.
const MAX_1PUX_DATA: u64 = 256 * 1024 * 1024;

/// 1Password's 1PUX export: a zip holding `export.data` (JSON).
pub fn onepux(bytes: &[u8]) -> Result<ImportResult, Error> {
    onepux_limited(bytes, MAX_1PUX_DATA)
}

fn onepux_limited(bytes: &[u8], max_data: u64) -> Result<ImportResult, Error> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| Error::Import(format!("not a 1PUX file: {e}")))?;
    let mut data = Zeroizing::new(Vec::new());
    {
        let mut f = zip
            .by_name("export.data")
            .map_err(|e| Error::Import(format!("not a 1PUX file (no export.data): {e}")))?;
        if f.size() > max_data {
            return Err(Error::Import("export.data is implausibly large".into()));
        }
        // The declared size is the archive's word for it: bound the read
        // too, so a crafted entry cannot inflate past the limit.
        (&mut f)
            .take(max_data + 1)
            .read_to_end(&mut data)
            .map_err(|e| Error::Import(format!("reading export.data: {e}")))?;
        if data.len() as u64 > max_data {
            return Err(Error::Import("export.data is implausibly large".into()));
        }
    }
    let root: Json =
        serde_json::from_slice(&data).map_err(|e| Error::Import(format!("export.data: {e}")))?;
    let mut out = ImportResult::default();
    let accounts = root
        .get("accounts")
        .and_then(Json::as_array)
        .cloned()
        .unwrap_or_default();
    let several_vaults = accounts
        .iter()
        .map(|a| a.get("vaults").and_then(Json::as_array).map_or(0, Vec::len))
        .sum::<usize>()
        > 1;
    for account in &accounts {
        for vault in account
            .get("vaults")
            .and_then(Json::as_array)
            .into_iter()
            .flatten()
        {
            let vault_name = vault.get("attrs").map(|a| s(a, "name")).unwrap_or_default();
            for it in vault
                .get("items")
                .and_then(Json::as_array)
                .into_iter()
                .flatten()
            {
                match s(it, "state").as_str() {
                    "trashed" | "deleted" => out.skipped += 1,
                    "archived" => {
                        let mut item = onepux_item(it);
                        item.entry.group =
                            join_group(several_vaults.then_some(vault_name.as_str()), "Archive");
                        out.items.push(item);
                    }
                    _ => {
                        let mut item = onepux_item(it);
                        item.entry.group =
                            join_group(None, if several_vaults { &vault_name } else { "" });
                        out.items.push(item);
                    }
                }
            }
        }
    }
    Ok(out)
}

fn join_group(a: Option<&str>, b: &str) -> String {
    match a {
        Some(a) if !a.is_empty() => format!("{a}/{b}"),
        _ => b.to_string(),
    }
}

/// Adds one 1PUX section field, named `name`, whose value is `val` of type
/// `kind`; the first TOTP becomes the entry's own.
fn onepux_field(e: &mut EntryData, name: &str, kind: &str, val: &Json) {
    match kind {
        "totp" => {
            let t = val.as_str().unwrap_or_default().to_string();
            if e.otp.is_empty() {
                e.otp = t;
            } else {
                push_field(&mut e.fields, field(name, t, true));
            }
        }
        "concealed" | "creditCardNumber" => push_field(
            &mut e.fields,
            field(name, val.as_str().unwrap_or_default(), true),
        ),
        "email" => {
            let addr = val
                .get("email_address")
                .and_then(Json::as_str)
                .or_else(|| val.as_str())
                .unwrap_or_default();
            push_field(&mut e.fields, field(name, addr, false));
        }
        "address" => {
            let parts: Vec<String> = ["street", "city", "state", "zip", "country"]
                .iter()
                .map(|k| s(val, k))
                .filter(|x| !x.is_empty())
                .collect();
            push_field(&mut e.fields, field(name, parts.join(", "), false));
        }
        "sshKey" => {
            let pk = val
                .get("privateKey")
                .and_then(Json::as_str)
                .or_else(|| val.as_str())
                .unwrap_or_default();
            push_field(&mut e.fields, field(name, pk, true));
        }
        _ => {
            let text = match val {
                Json::String(x) => x.clone(),
                Json::Number(n) => n.to_string(),
                Json::Bool(b) => b.to_string(),
                _ => String::new(),
            };
            push_field(&mut e.fields, field(name, text, false));
        }
    }
}

/// The username, password and other form fields of a 1PUX login.
fn onepux_login_fields(e: &mut EntryData, details: &Json) {
    for lf in details
        .get("loginFields")
        .and_then(Json::as_array)
        .into_iter()
        .flatten()
    {
        let value = s(lf, "value");
        match s(lf, "designation").as_str() {
            "username" if e.username.is_empty() => e.username = value,
            "password" if e.password.is_empty() => e.password = value,
            _ => {
                // Other form fields (e.g. "remember me") are rarely useful but
                // kept, as fields named after the form's field.
                let prot = s(lf, "fieldType") == "P";
                if !value.is_empty() && value != "✓" {
                    push_field(&mut e.fields, field(&s(lf, "name"), value, prot));
                }
            }
        }
    }
}

fn onepux_item(it: &Json) -> Imported {
    let null = Json::Null;
    let overview = it.get("overview").unwrap_or(&null);
    let details = it.get("details").unwrap_or(&null);
    let mut e = {
        let mut ed = EntryData::default();
        ed.title = s(overview, "title");
        ed.notes = s(details, "notesPlain");
        ed.modified = it.get("updatedAt").and_then(Json::as_i64).unwrap_or(0);
        ed
    };
    let main_url = s(overview, "url");
    if !main_url.trim().is_empty() {
        e.urls.push(main_url);
    }
    for u in overview
        .get("urls")
        .and_then(Json::as_array)
        .into_iter()
        .flatten()
    {
        let u = s(u, "url");
        if !u.trim().is_empty() && !e.urls.contains(&u) {
            e.urls.push(u);
        }
    }
    e.tags = overview
        .get("tags")
        .and_then(Json::as_array)
        .map(|t| {
            t.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    if it.get("favIndex").and_then(Json::as_i64).unwrap_or(0) > 0 {
        e.tags.push("Favourite".into());
    }
    onepux_login_fields(&mut e, details);
    // Password items keep the password at details.password.
    if e.password.is_empty() {
        e.password = s(details, "password");
    }
    for section in details
        .get("sections")
        .and_then(Json::as_array)
        .into_iter()
        .flatten()
    {
        let section_title = s(section, "title");
        for f in section
            .get("fields")
            .and_then(Json::as_array)
            .into_iter()
            .flatten()
        {
            let label = {
                let t = s(f, "title");
                if t.is_empty() {
                    s(f, "id")
                } else {
                    t
                }
            };
            let name = if section_title.is_empty() {
                label
            } else {
                format!("{section_title}: {label}")
            };
            let Some(v) = f.get("value").and_then(Json::as_object) else {
                continue;
            };
            let Some((kind, val)) = v.iter().next() else {
                continue;
            };
            onepux_field(&mut e, &name, kind, val);
        }
    }
    let mut item = Imported::new(e);
    let mut hist: Vec<(i64, String)> = details
        .get("passwordHistory")
        .and_then(Json::as_array)
        .into_iter()
        .flatten()
        .map(|h| {
            (
                h.get("time").and_then(Json::as_i64).unwrap_or(0),
                s(h, "value"),
            )
        })
        .collect();
    hist.sort();
    item.password_history = hist
        .into_iter()
        .map(|(_, p)| p)
        .filter(|p| !p.is_empty())
        .collect();
    item
}

// CSV.

fn csv_header(text: &str) -> Option<Vec<String>> {
    let mut r = csv::ReaderBuilder::new()
        .flexible(true)
        .from_reader(text.as_bytes());
    let h = r.headers().ok()?;
    Some(h.iter().map(|x| x.trim().to_lowercase()).collect())
}

/// Rows as (lower-case header -> value) lookups.
fn csv_rows(bytes: &[u8]) -> Result<(Vec<String>, Vec<Vec<String>>), Error> {
    let text = std::str::from_utf8(bytes)
        .map_err(|e| Error::Import(format!("the CSV file is not UTF-8: {e}")))?;
    let text = text.trim_start_matches('\u{feff}');
    let mut r = csv::ReaderBuilder::new()
        .flexible(true)
        .from_reader(text.as_bytes());
    let headers: Vec<String> = r
        .headers()
        .map_err(|e| Error::Import(format!("CSV: {e}")))?
        .iter()
        .map(|x| x.trim().to_lowercase())
        .collect();
    let mut rows = Vec::new();
    for rec in r.records() {
        let rec = rec.map_err(|e| Error::Import(format!("CSV: {e}")))?;
        rows.push(rec.iter().map(String::from).collect());
    }
    Ok((headers, rows))
}

struct Row<'a> {
    headers: &'a [String],
    values: &'a [String],
}

impl Row<'_> {
    fn get(&self, names: &[&str]) -> String {
        for n in names {
            if let Some(i) = self.headers.iter().position(|h| h == n) {
                if let Some(v) = self.values.get(i) {
                    return v.clone();
                }
            }
        }
        String::new()
    }
}

fn url_list(u: &str) -> Vec<String> {
    if u.trim().is_empty() {
        Vec::new()
    } else {
        vec![u.trim().to_string()]
    }
}

/// Chrome, Edge, Brave, Vivaldi: `name,url,username,password,note`.
pub fn chrome_csv(bytes: &[u8]) -> Result<ImportResult, Error> {
    let (headers, rows) = csv_rows(bytes)?;
    let mut out = ImportResult::default();
    for values in &rows {
        let r = Row {
            headers: &headers,
            values,
        };
        let url = r.get(&["url", "origin"]);
        let mut title = r.get(&["name"]);
        if title.trim().is_empty() {
            title = crate::search::host_of(&url);
        }
        out.items.push(Imported::new({
            let mut ed = EntryData::default();
            ed.title = title;
            ed.username = r.get(&["username"]);
            ed.password = r.get(&["password"]);
            ed.urls = url_list(&url);
            ed.notes = r.get(&["note", "notes"]);
            ed
        }));
    }
    Ok(out)
}

/// Firefox "Export Logins": `url,username,password,httpRealm,formActionOrigin,guid,timeCreated,timeLastUsed,timePasswordChanged`.
pub fn firefox_csv(bytes: &[u8]) -> Result<ImportResult, Error> {
    let (headers, rows) = csv_rows(bytes)?;
    let mut out = ImportResult::default();
    for values in &rows {
        let r = Row {
            headers: &headers,
            values,
        };
        let url = r.get(&["url"]);
        let mut e = {
            let mut ed = EntryData::default();
            ed.title = crate::search::host_of(&url);
            ed.username = r.get(&["username"]);
            ed.password = r.get(&["password"]);
            ed.urls = url_list(&url);
            ed
        };
        let realm = r.get(&["httprealm"]);
        push_field(&mut e.fields, field("HTTP realm", realm, false));
        // Firefox times are Unix milliseconds.
        e.modified = r
            .get(&["timepasswordchanged"])
            .parse::<i64>()
            .map(|ms| ms / 1000)
            .unwrap_or(0);
        out.items.push(Imported::new(e));
    }
    Ok(out)
}

/// 1Password 7 and 8 CSV. 8 writes `Title,Url,Username,Password,OTPAuth,Favorite,Archived,Tags,Notes`;
/// 7 lets the user pick columns, so every known header is accepted.
pub fn onepassword_csv(bytes: &[u8]) -> Result<ImportResult, Error> {
    let (headers, rows) = csv_rows(bytes)?;
    let mut out = ImportResult::default();
    for values in &rows {
        let r = Row {
            headers: &headers,
            values,
        };
        let mut e = {
            let mut ed = EntryData::default();
            ed.title = r.get(&["title", "name"]);
            ed.username = r.get(&["username", "user name"]);
            ed.password = r.get(&["password"]);
            ed.urls = url_list(&r.get(&["url", "urls", "website", "login_uri"]));
            ed.notes = r.get(&["notes", "notesplain", "note"]);
            ed.otp = r.get(&["otpauth", "one-time password", "totp"]);
            ed
        };
        e.tags = r
            .get(&["tags"])
            .split([',', ';'])
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();
        if r.get(&["favorite"]).eq_ignore_ascii_case("true") {
            e.tags.push("Favourite".into());
        }
        if r.get(&["archived"]).eq_ignore_ascii_case("true") {
            e.group = "Archive".into();
        }
        out.items.push(Imported::new(e));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::KdfParams;
    use std::io::Write;

    #[test]
    fn detects_formats() {
        assert_eq!(
            detect(b"name,url,username,password,note\n"),
            Some(Format::ChromeCsv)
        );
        assert_eq!(
            detect(
                b"\"url\",\"username\",\"password\",\"httpRealm\",\"formActionOrigin\",\"guid\"\n"
            ),
            Some(Format::FirefoxCsv)
        );
        assert_eq!(
            detect(b"Title,Url,Username,Password,OTPAuth,Favorite,Archived,Tags,Notes\n"),
            Some(Format::OnePasswordCsv)
        );
        assert_eq!(
            detect(b"\xef\xbb\xbf{\"items\":[]}"),
            Some(Format::BitwardenJson)
        );
        assert_eq!(detect(b"PK\x03\x04rest"), Some(Format::OnePux));
        assert_eq!(detect(b"hello\n"), None);
        let bytes = include_bytes!("../tests/fixtures/test_db_kdbx4_with_totp_entry.kdbx");
        assert_eq!(detect(bytes), Some(Format::Kdbx));
    }

    #[test]
    fn chrome() {
        let csv = "name,url,username,password,note\n\
                   GitHub,https://github.com/login,octo,\"pa,ss\"\"word\",\"multi\nline\"\n\
                   ,https://example.org/,me,pw,\n";
        let r = import(Format::ChromeCsv, csv.as_bytes(), None).unwrap();
        assert_eq!(r.items.len(), 2);
        let e = &r.items[0].entry;
        assert_eq!(e.title, "GitHub");
        assert_eq!(e.password, "pa,ss\"word");
        assert_eq!(e.notes, "multi\nline");
        assert_eq!(e.urls, vec!["https://github.com/login".to_string()]);
        assert_eq!(
            r.items[1].entry.title, "example.org",
            "title falls back to host"
        );
    }

    #[test]
    fn firefox() {
        let csv = "\"url\",\"username\",\"password\",\"httpRealm\",\"formActionOrigin\",\"guid\",\"timeCreated\",\"timeLastUsed\",\"timePasswordChanged\"\n\
                   \"https://accounts.example.com\",\"bob\",\"hunter2\",,\"https://accounts.example.com\",\"{abc}\",\"1700000000000\",\"1700000000000\",\"1700000500000\"\n";
        let r = firefox_csv(csv.as_bytes()).unwrap();
        let e = &r.items[0].entry;
        assert_eq!(e.title, "accounts.example.com");
        assert_eq!(e.username, "bob");
        assert_eq!(e.password, "hunter2");
        assert_eq!(e.modified, 1_700_000_500);
        assert!(e.fields.is_empty());
    }

    #[test]
    fn onepassword_csv_8() {
        let csv = "Title,Url,Username,Password,OTPAuth,Favorite,Archived,Tags,Notes\n\
                   Bank,https://bank.example,alice,s3cret,otpauth://totp/Bank?secret=JBSWY3DPEHPK3PXP,true,false,\"money,home\",note\n\
                   Old,,,x,,false,true,,\n";
        let r = onepassword_csv(csv.as_bytes()).unwrap();
        let e = &r.items[0].entry;
        assert_eq!(e.title, "Bank");
        assert!(e.totp().unwrap().is_ok());
        assert_eq!(e.tags, vec!["money", "home", "Favourite"]);
        assert_eq!(r.items[1].entry.group, "Archive");
    }

    const BITWARDEN: &str = r#"{
      "encrypted": false,
      "folders": [{"id": "f1", "name": "Work"}],
      "items": [
        {"id": "1", "folderId": "f1", "type": 1, "name": "Mail", "notes": "n", "favorite": true,
         "fields": [{"name": "PIN", "value": "1234", "type": 1}, {"name": "Region", "value": "EU", "type": 0},
                    {"name": "link", "value": null, "type": 3, "linkedId": 100}],
         "login": {"username": "me@example.com", "password": "new",
                   "totp": "otpauth://totp/Mail?secret=JBSWY3DPEHPK3PXP",
                   "uris": [{"match": null, "uri": "https://mail.example.com"}, {"uri": "https://example.com"}]},
         "passwordHistory": [{"lastUsedDate": "2024-02-01T00:00:00.000Z", "password": "older"},
                             {"lastUsedDate": "2023-01-01T00:00:00.000Z", "password": "oldest"}]},
        {"id": "2", "folderId": null, "type": 3, "name": "Visa",
         "card": {"cardholderName": "A B", "brand": "Visa", "number": "4111111111111111", "expMonth": "1", "expYear": "2030", "code": "123"}},
        {"id": "3", "folderId": null, "type": 2, "name": "Wifi", "notes": "code 42", "secureNote": {"type": 0}},
        {"id": "4", "type": 99, "name": "future"}
      ]
    }"#;

    #[test]
    fn bitwarden() {
        let r = bitwarden_json(BITWARDEN.as_bytes()).unwrap();
        assert_eq!(r.items.len(), 3);
        assert_eq!(r.skipped, 1);
        let m = &r.items[0];
        assert_eq!(m.entry.group, "Work");
        assert_eq!(m.entry.username, "me@example.com");
        assert_eq!(m.entry.urls.len(), 2);
        assert!(m.entry.totp().unwrap().is_ok());
        assert_eq!(m.entry.fields.len(), 2);
        assert!(m
            .entry
            .fields
            .iter()
            .any(|f| f.name == "PIN" && f.protected));
        assert_eq!(m.password_history, vec!["oldest", "older"]);
        let card = &r.items[1].entry;
        assert!(card
            .fields
            .iter()
            .any(|f| f.name == "Number" && f.protected));
        assert!(card
            .fields
            .iter()
            .any(|f| f.name == "Expiry" && f.value == "1/2030"));
        assert!(bitwarden_json(br#"{"encrypted": true, "data": "x"}"#).is_err());
        assert!(bitwarden_json(b"[]").is_err());
    }

    fn make_1pux(json: &str) -> Vec<u8> {
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut buf);
            let opts = zip::write::SimpleFileOptions::default();
            z.start_file("export.attributes", opts).unwrap();
            z.write_all(br#"{"version":3,"description":"1Password Unencrypted Export"}"#)
                .unwrap();
            z.start_file("export.data", opts).unwrap();
            z.write_all(json.as_bytes()).unwrap();
            z.finish().unwrap();
        }
        buf.into_inner()
    }

    const ONEPUX: &str = r#"{"accounts":[{"attrs":{"name":"Me"},"vaults":[{"attrs":{"name":"Private"},"items":[
      {"uuid":"a","favIndex":1,"createdAt":1600000000,"updatedAt":1650000000,"state":"active","categoryUuid":"001",
       "details":{"loginFields":[
          {"value":"alice","id":"","name":"email","fieldType":"E","designation":"username"},
          {"value":"pw2","id":"","name":"password","fieldType":"P","designation":"password"}],
        "notesPlain":"hello",
        "sections":[{"title":"Security","name":"s","fields":[
          {"title":"one-time password","id":"TOTP_1","value":{"totp":"otpauth://totp/X?secret=JBSWY3DPEHPK3PXP"}},
          {"title":"recovery code","id":"r","value":{"concealed":"abcd-efgh"}},
          {"title":"backup email","id":"e","value":{"email":{"email_address":"b@example.com","provider":null}}}]}],
        "passwordHistory":[{"value":"pw1","time":1610000000},{"value":"pw0","time":1600000000}]},
       "overview":{"subtitle":"alice","urls":[{"label":"","url":"https://a.example"},{"label":"","url":"https://b.example"}],
                   "title":"Example","url":"https://a.example","tags":["web"]}},
      {"uuid":"b","favIndex":0,"createdAt":1,"updatedAt":2,"state":"archived","categoryUuid":"005",
       "details":{"loginFields":[],"password":"only-a-password","sections":[]},
       "overview":{"title":"Router","tags":[]}},
      {"uuid":"c","state":"trashed","details":{},"overview":{"title":"gone"}}
    ]}]}]}"#;

    /// Sets the declared uncompressed size of every entry (local and central
    /// headers) to `size`: an archive that lies about what it inflates to.
    fn lie_about_sizes(mut zip: Vec<u8>, size: u32) -> Vec<u8> {
        let mut i = 0;
        while i + 30 <= zip.len() {
            let at = match &zip[i..i + 4] {
                [0x50, 0x4b, 0x03, 0x04] => Some(i + 22),
                [0x50, 0x4b, 0x01, 0x02] => Some(i + 24),
                _ => None,
            };
            if let Some(at) = at {
                zip[at..at + 4].copy_from_slice(&size.to_le_bytes());
            }
            i += 1;
        }
        zip
    }

    #[test]
    fn onepux_inflation_is_bounded_whatever_the_declared_size() {
        let big = format!(r#"{{"accounts":[],"pad":"{}"}}"#, "x".repeat(50_000));
        let honest = make_1pux(&big);
        assert!(
            onepux_limited(&honest, 1000).is_err(),
            "declared size over the limit"
        );
        let liar = lie_about_sizes(honest, 10);
        let e = onepux_limited(&liar, 1000).unwrap_err().to_string();
        assert!(e.contains("implausibly large"), "{e}");
        assert!(onepux(&make_1pux(ONEPUX)).is_ok());
    }

    #[test]
    fn onepux_export() {
        let bytes = make_1pux(ONEPUX);
        assert_eq!(detect(&bytes), Some(Format::OnePux));
        let r = import(Format::OnePux, &bytes, None).unwrap();
        assert_eq!(r.items.len(), 2);
        assert_eq!(r.skipped, 1);
        let a = &r.items[0];
        assert_eq!(a.entry.title, "Example");
        assert_eq!(a.entry.username, "alice");
        assert_eq!(a.entry.password, "pw2");
        assert_eq!(a.entry.urls, vec!["https://a.example", "https://b.example"]);
        assert_eq!(a.entry.tags, vec!["web", "Favourite"]);
        assert!(a.entry.totp().unwrap().is_ok());
        assert!(a
            .entry
            .fields
            .iter()
            .any(|f| f.name == "Security: recovery code" && f.protected && f.value == "abcd-efgh"));
        assert!(a.entry.fields.iter().any(|f| f.value == "b@example.com"));
        assert_eq!(a.password_history, vec!["pw0", "pw1"]);
        let b = &r.items[1];
        assert_eq!(b.entry.password, "only-a-password");
        assert_eq!(b.entry.group, "Archive");
        assert!(onepux(b"PK\x03\x04garbage").is_err());
    }

    #[test]
    fn kdbx_import_keeps_history_and_merge_dedups() {
        let mut src = Vault::new("Src", KdfParams::insecure_for_tests());
        let mut e = EntryData::default();
        e.title = "Site".into();
        e.password = "one".into();
        let id = src.add(&e);
        let mut cur = src.entry(&id).unwrap();
        cur.password = "two".into();
        src.update(&cur).unwrap();
        cur.password = "three".into();
        src.update(&cur).unwrap();
        let key = CompositeKey::password("x");
        let bytes = src.save(&key).unwrap();

        assert!(import(Format::Kdbx, &bytes, None).is_err());
        let r = import(Format::Kdbx, &bytes, Some(&key)).unwrap();
        assert_eq!(r.items[0].entry.password, "three");
        assert_eq!(r.items[0].password_history, vec!["one", "two"]);

        let mut dst = Vault::new("Dst", KdfParams::insecure_for_tests());
        let rep = dst.merge_imported(&r.items);
        assert_eq!(rep.added, 1);
        let new_id = dst.entries()[0].id.clone();
        let h: Vec<String> = dst
            .history(&new_id)
            .iter()
            .map(|x| x.password.clone())
            .collect();
        assert_eq!(h, vec!["two", "one"], "history newest first");
        assert_eq!(dst.merge_imported(&r.items).duplicates, 1);
    }

    #[test]
    fn bitwarden_into_vault_round_trip() {
        let r = bitwarden_json(BITWARDEN.as_bytes()).unwrap();
        let mut v = Vault::new("V", KdfParams::insecure_for_tests());
        assert_eq!(v.merge_imported(&r.items).added, 3);
        let key = CompositeKey::password("k");
        let bytes = v.save(&key).unwrap();
        let v2 = Vault::open(&bytes, &key).unwrap();
        let mail = v2.search("mail").into_iter().next().unwrap();
        assert_eq!(mail.group, "Work");
        assert_eq!(v2.history(&mail.id).len(), 2);
    }
}
