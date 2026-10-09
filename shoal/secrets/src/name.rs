// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Safe secret names.
//!
//! Sailfish Secrets documents no character set for secret names, and its
//! storage plugins hash or quote them differently. To stay on safe ground,
//! names sent to the daemon use only the safe set `[A-Za-z0-9.-]` plus `_`,
//! the escape character. [`encode`] maps any key to such a name reversibly:
//! every byte outside the safe set becomes `_XX` (upper-case hex), including
//! `_` itself. Names already in the safe set are unchanged,
//! so Shoal Keys' existing names (`shoalkeys-<uuid>`) keep working.

use std::fmt::Write as _;

/// True for characters passed through unchanged.
fn safe(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'.'
}

/// `key` as a name made of the safe set `[A-Za-z0-9.-]` plus `_`, the
/// escape character.
pub fn encode(key: &str) -> String {
    let mut out = String::with_capacity(key.len());
    for &b in key.as_bytes() {
        if safe(b) {
            out.push(b as char);
        } else {
            // Writing to a String cannot fail.
            let _ = write!(out, "_{b:02X}");
        }
    }
    out
}

/// The key [`encode`] made `name` from, or `None` if `name` is not an
/// encoding.
pub fn decode(name: &str) -> Option<String> {
    let bytes = name.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'_' => {
                let hex = name.get(i + 1..i + 3)?;
                if !hex
                    .bytes()
                    .all(|c| c.is_ascii_digit() || (b'A'..=b'F').contains(&c))
                {
                    return None;
                }
                let b = u8::from_str_radix(hex, 16).ok()?;
                if safe(b) {
                    return None; // not canonical
                }
                out.push(b);
                i += 3;
            }
            b if safe(b) => {
                out.push(b);
                i += 1;
            }
            _ => return None,
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_stays_safe() {
        for key in [
            "account/3f2a/password",
            "shoalkeys-0b6c-11",
            "a_b",
            "ümlaut key",
            "",
        ] {
            let n = encode(key);
            assert!(n.bytes().all(|b| safe(b) || b == b'_'), "{n}");
            assert_eq!(decode(&n).as_deref(), Some(key));
        }
        assert_eq!(encode("account/a1/password"), "account_2Fa1_2Fpassword");
        assert_eq!(encode("shoalkeys-abc"), "shoalkeys-abc");
        assert_eq!(decode("a_2"), None);
        assert_eq!(
            decode("a_41"),
            None,
            "an encoded safe byte is not canonical"
        );
        assert_eq!(decode("a/b"), None);
    }
}
