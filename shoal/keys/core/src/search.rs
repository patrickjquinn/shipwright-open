// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

//! Search over entries: every whitespace-separated term must match
//! (case-insensitive substring) in the title, user name, a URL, a tag, the
//! group path, a custom field name or the notes. Passwords, TOTP secrets
//! and custom field values are never searched, so typing part of a
//! password never reveals which entry holds it.
//!
//! Results are ranked: title prefix, then title, then user name or URL,
//! then tags and group, then notes and field names; ties keep input order
//! (which is by title).

use crate::entry::EntryData;

fn score_term(e: &EntryData, term: &str) -> Option<u32> {
    let title = e.title.to_lowercase();
    if title.starts_with(term) {
        return Some(100);
    }
    if title.contains(term) {
        return Some(80);
    }
    if e.username.to_lowercase().contains(term)
        || e.urls
            .iter()
            .any(|u| host_of(u).contains(term) || u.to_lowercase().contains(term))
    {
        return Some(60);
    }
    if e.tags.iter().any(|t| t.to_lowercase().contains(term))
        || e.group.to_lowercase().contains(term)
    {
        return Some(40);
    }
    if e.notes.to_lowercase().contains(term)
        || e.fields
            .iter()
            .any(|f| f.name.to_lowercase().contains(term))
    {
        return Some(20);
    }
    None
}

/// The host part of a URL, lower case, for matching "github" against
/// `"https://github.com/login"`.
pub fn host_of(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.rsplit_once('@').map_or(host, |(_, h)| h);
    host.split(':').next().unwrap_or("").to_lowercase()
}

/// Filters and ranks `entries` for `query`. An empty query returns all.
pub fn search(entries: Vec<EntryData>, query: &str) -> Vec<EntryData> {
    let terms: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    if terms.is_empty() {
        return entries;
    }
    let mut scored: Vec<(u32, usize, EntryData)> = entries
        .into_iter()
        .enumerate()
        .filter_map(|(i, e)| {
            let mut total = 0;
            for t in &terms {
                total += score_term(&e, t)?;
            }
            Some((total, i, e))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.into_iter().map(|(_, _, e)| e).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::CustomField;

    fn e(title: &str, user: &str, url: &str) -> EntryData {
        {
            let mut e = EntryData::default();
            e.title = title.into();
            e.username = user.into();
            e.password = "hunter2".into();
            e.urls = vec![url.into()];
            e
        }
    }

    #[test]
    fn ranks_and_filters() {
        let list = vec![
            e("Bank", "me", "https://bank.example"),
            e("GitHub", "octo", "https://github.com/login"),
            e("Work mail", "me@github.com", "https://mail.example"),
            e("My GitHub mirror", "x", "https://git.example"),
        ];
        let r = search(list.clone(), "git");
        let titles: Vec<&str> = r.iter().map(|e| e.title.as_str()).collect();
        assert_eq!(titles, ["GitHub", "My GitHub mirror", "Work mail"]);
        assert_eq!(search(list.clone(), "").len(), 4);
        assert_eq!(search(list.clone(), "GITHUB octo").len(), 1);
        assert!(search(list, "nothing").is_empty());
    }

    #[test]
    fn never_matches_secrets() {
        let mut x = e("Bank", "me", "https://bank.example");
        x.otp = "JBSWY3DPEHPK3PXP".into();
        x.fields.push(CustomField {
            name: "PIN".into(),
            value: "987654".into(),
            protected: true,
        });
        assert!(search(vec![x.clone()], "hunter").is_empty());
        assert!(search(vec![x.clone()], "jbsw").is_empty());
        assert!(search(vec![x.clone()], "9876").is_empty());
        assert_eq!(search(vec![x], "pin").len(), 1);
    }

    #[test]
    fn host_extraction() {
        assert_eq!(host_of("https://user@Example.COM:8443/x?y"), "example.com");
        assert_eq!(host_of("example.org/path"), "example.org");
    }
}
