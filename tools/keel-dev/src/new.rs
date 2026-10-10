// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: MIT

//! `keel new`: writes a complete app skeleton from the embedded templates.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::template::{self, Vars};

/// Licences `keel new --license` ships the text of (from Shipwright's
/// LICENSES/). Any `LicenseRef-<name>` is accepted too, with a stub text.
const LICENSES: &[(&str, &str)] = &[
    ("MIT", include_str!("../../../LICENSES/MIT.txt")),
    (
        "Apache-2.0",
        include_str!("../../../LICENSES/Apache-2.0.txt"),
    ),
    (
        "BSD-3-Clause",
        include_str!("../../../LICENSES/BSD-3-Clause.txt"),
    ),
    // The GPL text is the same for -only and -or-later.
    (
        "GPL-3.0-or-later",
        include_str!("../../../LICENSES/GPL-3.0-only.txt"),
    ),
    (
        "GPL-3.0-only",
        include_str!("../../../LICENSES/GPL-3.0-only.txt"),
    ),
    (
        "LGPL-2.1-or-later",
        include_str!("../../../LICENSES/LGPL-2.1-or-later.txt"),
    ),
];

/// What `keel new` needs to know.
pub struct Options {
    pub name: String,
    pub dir: Option<PathBuf>,
    pub id: Option<String>,
    pub license: String,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub copyright: Option<String>,
    pub render_icons: bool,
}

/// Checks an app id: what Reef accepts as a package and binary name
/// (lower-case letters, digits and single hyphens, starting with a letter).
pub fn validate_name(name: &str) -> Result<(), String> {
    let ok = (2..=64).contains(&name.len())
        && name.starts_with(|c: char| c.is_ascii_lowercase())
        && !name.ends_with('-')
        && !name.contains("--")
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if ok {
        Ok(())
    } else {
        Err(format!(
            "invalid app name {name:?}: use 2 to 64 lower-case letters, digits and single \
             hyphens, starting with a letter (it becomes the package and binary name, \
             e.g. weather-now)"
        ))
    }
}

/// Checks a reverse-DNS id (`org.example.WeatherNow`): at least two dotted
/// segments, each a letter followed by letters, digits or underscores. It
/// is the QML module URI, and gives Sailjail's `OrganizationName` (all but
/// the last segment) and `ApplicationName` (the last).
pub fn validate_id(id: &str) -> Result<(), String> {
    let segments: Vec<&str> = id.split('.').collect();
    let ok = segments.len() >= 2
        && id.len() <= 128
        && segments.iter().all(|s| {
            s.starts_with(|c: char| c.is_ascii_alphabetic())
                && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        });
    if ok {
        Ok(())
    } else {
        Err(format!(
            "invalid id {id:?}: use reverse-DNS form such as org.example.WeatherNow (dotted \
             segments of letters, digits and underscores, each starting with a letter)"
        ))
    }
}

fn validate_text(what: &str, text: &str) -> Result<(), String> {
    if text.trim().is_empty() || text.len() > 80 {
        return Err(format!("{what} must be 1 to 80 characters"));
    }
    if text
        .chars()
        .any(|c| c.is_control() || matches!(c, '"' | '\\' | '{' | '}' | '%'))
    {
        return Err(format!(
            "{what} {text:?} may not contain quotes, backslashes, braces, % or control characters"
        ));
    }
    Ok(())
}

/// `weather-now` -> `WeatherNow`.
fn pascal(name: &str) -> String {
    name.split('-')
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_ascii_uppercase().to_string() + c.as_str())
                .unwrap_or_default()
        })
        .collect()
}

/// `weather-now` -> `Weather Now`.
fn title_case(name: &str) -> String {
    name.split('-')
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_ascii_uppercase().to_string() + c.as_str())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The licence text for `id`, or an error naming the supported ones.
fn license_text(id: &str) -> Result<String, String> {
    if let Some((_, text)) = LICENSES.iter().find(|(l, _)| *l == id) {
        return Ok((*text).to_owned());
    }
    if let Some(rest) = id.strip_prefix("LicenseRef-") {
        if !rest.is_empty()
            && rest
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
        {
            return Ok(format!(
                "{id}\n\nWrite the terms of your licence here. This file is named after the\n\
                 licence identifier used in every file's header (REUSE).\n"
            ));
        }
    }
    let known: Vec<&str> = LICENSES.iter().map(|(l, _)| *l).collect();
    Err(format!(
        "unsupported licence {id:?}: use one of {}, or LicenseRef-<name> for your own",
        known.join(", ")
    ))
}

/// Days since 1970-01-01 to (year, month, day), proleptic Gregorian.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = u32::try_from(doy - (153 * mp + 2) / 5 + 1).unwrap_or(1);
    let month = u32::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).unwrap_or(1);
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// (year, RPM changelog date such as `Thu Oct 01 2026`) for today, UTC.
fn today() -> (String, String) {
    const WEEKDAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let days = i64::try_from(secs / 86_400).unwrap_or(0);
    let (y, m, d) = civil_from_days(days);
    let weekday = WEEKDAYS[usize::try_from(days.rem_euclid(7)).unwrap_or(0)];
    let month = MONTHS[usize::try_from(m - 1).unwrap_or(0)];
    (y.to_string(), format!("{weekday} {month} {d:02} {y}"))
}

fn git_user_name() -> Option<String> {
    let out = Command::new("git")
        .args(["config", "user.name"])
        .output()
        .ok()?;
    let name = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    (out.status.success() && !name.is_empty()).then_some(name)
}

/// Icon gradient for a name: one of a few calm pairs, picked by a hash.
fn icon_colors(name: &str) -> (&'static str, &'static str) {
    const PAIRS: [(&str, &str); 6] = [
        ("4F8EDC", "1F3F8F"),
        ("5BBF8A", "1E6B4F"),
        ("E39B4C", "9A4A16"),
        ("B07CD8", "5A2F86"),
        ("E06B7A", "8E2338"),
        ("4FB3C8", "175E70"),
    ];
    let hash = name
        .bytes()
        .fold(0u32, |h, b| h.wrapping_mul(31).wrapping_add(u32::from(b)));
    PAIRS[usize::try_from(hash).unwrap_or(0) % PAIRS.len()]
}

/// Placeholder values for an app.
pub fn vars(opts: &Options) -> Result<Vars, String> {
    validate_name(&opts.name)?;
    let pascal_name = pascal(&opts.name);
    let id = opts
        .id
        .clone()
        .unwrap_or_else(|| format!("org.example.{pascal_name}"));
    validate_id(&id)?;
    let title = opts.title.clone().unwrap_or_else(|| title_case(&opts.name));
    validate_text("--title", &title)?;
    let summary = opts
        .summary
        .clone()
        .unwrap_or_else(|| format!("{title}, a Sailfish OS app on Keel"));
    validate_text("--summary", &summary)?;
    let copyright = opts
        .copyright
        .clone()
        .or_else(git_user_name)
        .unwrap_or_else(|| format!("The {title} authors"));
    validate_text("--copyright", &copyright)?;
    license_text(&opts.license)?;

    let (org, app_name) = id.rsplit_once('.').unwrap_or(("org.example", &id));
    let (year, changelog_date) = today();
    let (color_a, color_b) = icon_colors(&opts.name);
    let initial = title
        .chars()
        .find(char::is_ascii_alphanumeric)
        .map_or('K', |c| c.to_ascii_uppercase());

    let mut v = Vars::new();
    v.insert("name", opts.name.clone());
    // The Rust crate name: the app's library is lib{{crate}}.so.
    v.insert("crate", opts.name.replace('-', "_"));
    v.insert("pascal", pascal_name);
    v.insert("id", id.clone());
    v.insert("qml_uri", id.clone());
    v.insert("org", org.to_owned());
    v.insert("app_name", app_name.to_owned());
    v.insert("title", title);
    v.insert("summary", summary);
    v.insert("copyright", copyright);
    v.insert("license", opts.license.clone());
    v.insert("year", year);
    v.insert("changelog_date", changelog_date);
    v.insert("icon_color_a", color_a.to_owned());
    v.insert("icon_color_b", color_b.to_owned());
    v.insert("initial", initial.to_string());
    Ok(v)
}

/// Writes the app; returns its directory.
pub fn generate(opts: &Options) -> Result<PathBuf, String> {
    let vars = vars(opts)?;
    let dir = opts
        .dir
        .clone()
        .unwrap_or_else(|| PathBuf::from(&opts.name));
    if dir.exists()
        && fs::read_dir(&dir)
            .map_err(|e| format!("{}: {e}", dir.display()))?
            .next()
            .is_some()
    {
        return Err(format!(
            "{} exists and is not empty; pick another --dir",
            dir.display()
        ));
    }

    let mut files: Vec<(PathBuf, String, bool)> = Vec::new();
    for t in template::APP {
        let path = template::render(t.path, &vars)?;
        let text = template::render(t.text, &vars).map_err(|e| format!("{}: {e}", t.path))?;
        let left = template::leftover_placeholders(&text);
        if !left.is_empty() {
            return Err(format!("{path}: unsubstituted placeholders {left:?}"));
        }
        files.push((PathBuf::from(path), text, t.executable));
    }
    files.push((
        PathBuf::from(format!("LICENSES/{}.txt", opts.license)),
        license_text(&opts.license)?,
        false,
    ));
    // Copied open-source files (build/precompile_qml.rs) keep their MIT
    // notice, so the app carries that licence text as well.
    let copies_open = files
        .iter()
        .any(|(_, text, _)| template::has_open_licence(text));
    if copies_open && opts.license != "MIT" {
        files.push((
            PathBuf::from("LICENSES/MIT.txt"),
            license_text("MIT")?,
            false,
        ));
    }

    for (rel, text, executable) in &files {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
        if *executable {
            set_executable(&path)?;
        }
    }

    if opts.render_icons {
        render_icons(&dir);
    }
    Ok(dir)
}

#[cfg(unix)]
fn set_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
        .map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> Result<(), String> {
    Ok(())
}

/// Renders the launcher PNGs with the app's own script, if `rsvg-convert`
/// is installed; otherwise says how.
fn render_icons(dir: &Path) {
    let have_rsvg = Command::new("rsvg-convert")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    if have_rsvg {
        let ok = Command::new("sh")
            .arg(dir.join("icons/render-icons.sh"))
            .stdout(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        if ok {
            println!("keel new: rendered the launcher icons into icons/hicolor/");
            return;
        }
    }
    println!(
        "keel new: icons/hicolor/ PNGs not rendered (needs rsvg-convert); run \
         icons/render-icons.sh before packaging"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        for ok in ["hello-keel", "ab", "weather-now2"] {
            assert!(validate_name(ok).is_ok(), "{ok}");
        }
        for bad in [
            "", "a", "Hello", "1app", "a--b", "app-", "app_x", "app.x", "-a",
        ] {
            assert!(validate_name(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn ids() {
        for ok in ["org.example.Name", "com.acme.weather_now", "a.b"] {
            assert!(validate_id(ok).is_ok(), "{ok}");
        }
        for bad in [
            "Name",
            "org..x",
            "org.example.weather-now",
            "1org.x",
            "org.example.",
        ] {
            assert!(validate_id(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn derived_names() {
        assert_eq!(pascal("weather-now"), "WeatherNow");
        assert_eq!(title_case("weather-now"), "Weather Now");
    }

    #[test]
    fn licences() {
        assert!(license_text("MIT").unwrap().contains("MIT License"));
        assert!(license_text("LicenseRef-Acme").is_ok());
        assert!(license_text("WTFPL").is_err());
        assert!(license_text("LicenseRef-").is_err());
    }

    #[test]
    fn dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        // 2026-10-01 was a Thursday.
        assert_eq!(civil_from_days(20_727), (2026, 10, 1));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
    }

    #[test]
    fn text_rules() {
        assert!(validate_text("--title", "Weather Now").is_ok());
        assert!(validate_text("--title", "Say \"hi\"").is_err());
        assert!(validate_text("--title", " ").is_err());
    }
}
