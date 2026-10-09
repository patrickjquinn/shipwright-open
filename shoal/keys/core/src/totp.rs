// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

//! Time-based one-time passwords: RFC 4226 (HOTP) truncation, RFC 6238
//! (TOTP) time steps, and the `otpauth://` key URI format that KeePassXC,
//! Bitwarden and authenticator apps exchange.
//!
//! KeePass stores TOTP settings in several ways; [`Totp::from_kdbx_fields`]
//! reads all three that are in use:
//! - `otp`: an `otpauth://totp/...` URI (KeePassXC, KeePassDX, Bitwarden export).
//! - `TimeOtp-Secret-Base32` plus `TimeOtp-Length`, `TimeOtp-Period` and
//!   `TimeOtp-Algorithm` (KeePass 2.47 and later).
//! - `TOTP Seed` plus `TOTP Settings` as `period;digits` (KeeOtp, old KeePassXC).
//!
//! Shoal Keys always writes the `otp` URI form.

use std::fmt::Write as _;

use hmac::{Hmac, KeyInit, Mac};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::Error;

/// HMAC hash used by the generator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Zeroize)]
pub enum Algorithm {
    Sha1,
    Sha256,
    Sha512,
}

impl Algorithm {
    fn uri_name(self) -> &'static str {
        match self {
            Algorithm::Sha1 => "SHA1",
            Algorithm::Sha256 => "SHA256",
            Algorithm::Sha512 => "SHA512",
        }
    }

    fn parse(s: &str) -> Result<Self, Error> {
        let norm: String = s
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .collect::<String>()
            .to_ascii_uppercase();
        match norm.as_str() {
            "SHA1" | "HMACSHA1" => Ok(Algorithm::Sha1),
            "SHA256" | "HMACSHA256" => Ok(Algorithm::Sha256),
            "SHA512" | "HMACSHA512" => Ok(Algorithm::Sha512),
            _ => Err(Error::Totp(format!("unsupported algorithm {s:?}"))),
        }
    }
}

/// A TOTP generator. The secret is zeroised on drop.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct Totp {
    secret: Vec<u8>,
    pub algorithm: Algorithm,
    pub digits: u32,
    pub period: u64,
    pub issuer: String,
    pub account: String,
}

impl std::fmt::Debug for Totp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Totp")
            .field("secret", &"[redacted]")
            .field("algorithm", &self.algorithm)
            .field("digits", &self.digits)
            .field("period", &self.period)
            .field("issuer", &self.issuer)
            .field("account", &self.account)
            .finish()
    }
}

/// One generated code and when it expires.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TotpCode {
    pub code: String,
    /// Seconds until the next code, 1..=period.
    pub remaining: u64,
    pub period: u64,
}

impl Totp {
    /// A generator from raw key bytes.
    pub fn new(
        secret: Vec<u8>,
        algorithm: Algorithm,
        digits: u32,
        period: u64,
    ) -> Result<Self, Error> {
        if secret.is_empty() {
            return Err(Error::Totp("empty secret".into()));
        }
        if !(6..=10).contains(&digits) {
            return Err(Error::Totp(format!("digits must be 6 to 10, not {digits}")));
        }
        if period == 0 || period > 3600 {
            return Err(Error::Totp(format!(
                "period must be 1 to 3600 s, not {period}"
            )));
        }
        Ok(Self {
            secret,
            algorithm,
            digits,
            period,
            issuer: String::new(),
            account: String::new(),
        })
    }

    /// Parses what a user or an importer gives us: an `otpauth://` URI, or a
    /// bare base32 secret (spaces, dashes and lower case tolerated), which
    /// gets the RFC 6238 defaults (SHA-1, 6 digits, 30 s).
    pub fn parse(input: &str) -> Result<Self, Error> {
        let input = input.trim();
        if input.len() >= 10 && input[..10].eq_ignore_ascii_case("otpauth://") {
            Self::from_uri(input)
        } else {
            Self::new(decode_base32(input)?, Algorithm::Sha1, 6, 30)
        }
    }

    /// Parses an `otpauth://totp/Label?secret=...` key URI.
    pub fn from_uri(uri: &str) -> Result<Self, Error> {
        let rest = uri
            .get(..10)
            .filter(|p| p.eq_ignore_ascii_case("otpauth://"))
            .map(|_| &uri[10..])
            .ok_or_else(|| Error::Totp("not an otpauth URI".into()))?;
        let (kind, rest) = rest
            .split_once('/')
            .ok_or_else(|| Error::Totp("otpauth URI has no type".into()))?;
        if !kind.eq_ignore_ascii_case("totp") {
            return Err(Error::Totp(format!("only TOTP is supported, not {kind:?}")));
        }
        let (label, query) = rest.split_once('?').unwrap_or((rest, ""));
        let label = percent_decode(label)?;
        let (label_issuer, account) = match label.split_once(':') {
            Some((i, a)) => (i.trim().to_string(), a.trim().to_string()),
            None => (String::new(), label.trim().to_string()),
        };
        let mut secret = None;
        let mut algorithm = Algorithm::Sha1;
        let mut digits = 6;
        let mut period = 30;
        let mut issuer = label_issuer;
        for pair in query.split('&').filter(|p| !p.is_empty()) {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            let v = Zeroizing::new(percent_decode(v)?);
            match k.to_ascii_lowercase().as_str() {
                "secret" => secret = Some(decode_base32(&v)?),
                "algorithm" => algorithm = Algorithm::parse(&v)?,
                "digits" => {
                    digits = v
                        .parse()
                        .map_err(|e| Error::Totp(format!("bad digits {:?}: {e}", v.as_str())))?;
                }
                "period" => {
                    period = v
                        .parse()
                        .map_err(|e| Error::Totp(format!("bad period {:?}: {e}", v.as_str())))?;
                }
                "issuer" => issuer = v.to_string(),
                // "encoder=steam" and the like: not supported, refuse rather
                // than show wrong codes.
                "encoder" => {
                    return Err(Error::Totp(format!("unsupported encoder {:?}", v.as_str())))
                }
                _ => {}
            }
        }
        let secret = secret.ok_or_else(|| Error::Totp("otpauth URI has no secret".into()))?;
        let mut t = Self::new(secret, algorithm, digits, period)?;
        t.issuer = issuer;
        t.account = account;
        Ok(t)
    }

    /// Reads TOTP settings from KeePass entry fields (see the module docs).
    /// `get` returns a field's value by name.
    pub fn from_kdbx_fields<'a>(
        get: impl Fn(&str) -> Option<&'a str>,
    ) -> Option<Result<Self, Error>> {
        if let Some(v) = get("otp").filter(|v| !v.trim().is_empty()) {
            return Some(Self::parse(v));
        }
        if let Some(secret) = get("TimeOtp-Secret-Base32").filter(|v| !v.trim().is_empty()) {
            return Some((|| {
                let digits = get("TimeOtp-Length")
                    .map(str::trim)
                    .filter(|s| !s.is_empty());
                let period = get("TimeOtp-Period")
                    .map(str::trim)
                    .filter(|s| !s.is_empty());
                let algorithm = get("TimeOtp-Algorithm")
                    .map(str::trim)
                    .filter(|s| !s.is_empty());
                Self::new(
                    decode_base32(secret)?,
                    algorithm
                        .map(Algorithm::parse)
                        .transpose()?
                        .unwrap_or(Algorithm::Sha1),
                    digits
                        .map(|d| {
                            d.parse()
                                .map_err(|e| Error::Totp(format!("bad TimeOtp-Length: {e}")))
                        })
                        .transpose()?
                        .unwrap_or(6),
                    period
                        .map(|d| {
                            d.parse()
                                .map_err(|e| Error::Totp(format!("bad TimeOtp-Period: {e}")))
                        })
                        .transpose()?
                        .unwrap_or(30),
                )
            })());
        }
        if let Some(seed) = get("TOTP Seed").filter(|v| !v.trim().is_empty()) {
            return Some((|| {
                let (mut period, mut digits) = (30, 6);
                if let Some(settings) = get("TOTP Settings") {
                    let mut it = settings.split(';');
                    if let Some(p) = it.next().filter(|s| !s.is_empty()) {
                        period = p
                            .trim()
                            .parse()
                            .map_err(|e| Error::Totp(format!("bad TOTP Settings: {e}")))?;
                    }
                    if let Some(d) = it.next().filter(|s| !s.is_empty()) {
                        if d.trim() == "S" {
                            return Err(Error::Totp("Steam codes are not supported".into()));
                        }
                        digits = d
                            .trim()
                            .parse()
                            .map_err(|e| Error::Totp(format!("bad TOTP Settings: {e}")))?;
                    }
                }
                Self::new(decode_base32(seed)?, Algorithm::Sha1, digits, period)
            })());
        }
        None
    }

    /// The canonical `otpauth://` URI, as stored in the KDBX `otp` field.
    pub fn to_uri(&self) -> String {
        let mut label = String::new();
        if !self.issuer.is_empty() {
            label.push_str(&percent_encode(&self.issuer));
            label.push(':');
        }
        label.push_str(&percent_encode(&self.account));
        let secret = Zeroizing::new(data_encoding::BASE32_NOPAD.encode(&self.secret));
        let mut uri = format!(
            "otpauth://totp/{label}?secret={}&period={}&digits={}",
            secret.as_str(),
            self.period,
            self.digits
        );
        if self.algorithm != Algorithm::Sha1 {
            uri.push_str("&algorithm=");
            uri.push_str(self.algorithm.uri_name());
        }
        if !self.issuer.is_empty() {
            uri.push_str("&issuer=");
            uri.push_str(&percent_encode(&self.issuer));
        }
        uri
    }

    /// RFC 4226 HOTP value for a counter.
    pub fn hotp(&self, counter: u64) -> String {
        let msg = counter.to_be_bytes();
        let digest: Zeroizing<Vec<u8>> = Zeroizing::new(match self.algorithm {
            Algorithm::Sha1 => mac::<Hmac<sha1::Sha1>>(&self.secret, &msg),
            Algorithm::Sha256 => mac::<Hmac<sha2::Sha256>>(&self.secret, &msg),
            Algorithm::Sha512 => mac::<Hmac<sha2::Sha512>>(&self.secret, &msg),
        });
        // Dynamic truncation (RFC 4226 section 5.3).
        let offset = (digest[digest.len() - 1] & 0x0f) as usize;
        let bin = u32::from_be_bytes([
            digest[offset] & 0x7f,
            digest[offset + 1],
            digest[offset + 2],
            digest[offset + 3],
        ]);
        let modulus = 10u64.pow(self.digits);
        format!(
            "{:0width$}",
            u64::from(bin) % modulus,
            width = self.digits as usize
        )
    }

    /// The code at Unix time `unix_secs`.
    pub fn at(&self, unix_secs: u64) -> TotpCode {
        let step = unix_secs / self.period;
        TotpCode {
            code: self.hotp(step),
            remaining: self.period - unix_secs % self.period,
            period: self.period,
        }
    }

    /// The code now, by the system clock.
    pub fn now(&self) -> TotpCode {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        self.at(secs)
    }
}

fn mac<M: Mac + KeyInit>(key: &[u8], msg: &[u8]) -> Vec<u8> {
    // HMAC accepts keys of any length, so this cannot fail.
    let mut m = <M as KeyInit>::new_from_slice(key).expect("HMAC takes any key length");
    m.update(msg);
    m.finalize().into_bytes().to_vec()
}

/// Decodes RFC 4648 base32, tolerating lower case, spaces, dashes and
/// missing padding, as authenticator secrets are usually written.
pub fn decode_base32(input: &str) -> Result<Vec<u8>, Error> {
    let mut clean: Zeroizing<String> = Zeroizing::new(
        input
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '-' && *c != '=')
            .map(|c| c.to_ascii_uppercase())
            .collect(),
    );
    if clean.is_empty() {
        return Err(Error::Totp("empty secret".into()));
    }
    // BASE32_NOPAD rejects lengths that cannot occur; pad-free lengths of
    // 1, 3 and 6 (mod 8) are invalid in RFC 4648 anyway.
    let out = data_encoding::BASE32_NOPAD
        .decode(clean.as_bytes())
        .map_err(|e| Error::Totp(format!("secret is not valid base32: {e}")));
    clean.zeroize();
    out
}

fn percent_decode(s: &str) -> Result<String, Error> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                let hex = s
                    .get(i + 1..i + 3)
                    .and_then(|h| u8::from_str_radix(h, 16).ok())
                    .ok_or_else(|| Error::Totp("bad percent escape in otpauth URI".into()))?;
                out.push(hex);
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8(out).map_err(|e| Error::Totp(format!("otpauth URI is not UTF-8: {e}")))
}

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~' | b'@') {
            out.push(b as char);
        } else {
            // Writing to a String cannot fail.
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // RFC 6238 appendix B: the seeds are the ASCII strings below, sized to
    // each hash's output length, and codes have 8 digits.
    const SEED20: &[u8] = b"12345678901234567890";
    const SEED32: &[u8] = b"12345678901234567890123456789012";
    const SEED64: &[u8] = b"1234567890123456789012345678901234567890123456789012345678901234";

    #[test]
    fn rfc6238_vectors() {
        let cases: [(u64, &str, &str, &str); 6] = [
            (59, "94287082", "46119246", "90693936"),
            (1_111_111_109, "07081804", "68084774", "25091201"),
            (1_111_111_111, "14050471", "67062674", "99943326"),
            (1_234_567_890, "89005924", "91819424", "93441116"),
            (2_000_000_000, "69279037", "90698825", "38618901"),
            (20_000_000_000, "65353130", "77737706", "47863826"),
        ];
        let s1 = Totp::new(SEED20.to_vec(), Algorithm::Sha1, 8, 30).unwrap();
        let s256 = Totp::new(SEED32.to_vec(), Algorithm::Sha256, 8, 30).unwrap();
        let s512 = Totp::new(SEED64.to_vec(), Algorithm::Sha512, 8, 30).unwrap();
        for (t, c1, c256, c512) in cases {
            assert_eq!(s1.at(t).code, c1, "SHA1 at {t}");
            assert_eq!(s256.at(t).code, c256, "SHA256 at {t}");
            assert_eq!(s512.at(t).code, c512, "SHA512 at {t}");
        }
    }

    #[test]
    fn rfc4226_hotp_vectors() {
        // RFC 4226 appendix D, 6 digits.
        let expected = [
            "755224", "287082", "359152", "969429", "338314", "254676", "287922", "162583",
            "399871", "520489",
        ];
        let h = Totp::new(SEED20.to_vec(), Algorithm::Sha1, 6, 30).unwrap();
        for (i, e) in expected.iter().enumerate() {
            assert_eq!(h.hotp(i as u64), *e);
        }
    }

    #[test]
    fn remaining_counts_down() {
        let t = Totp::new(SEED20.to_vec(), Algorithm::Sha1, 6, 30).unwrap();
        assert_eq!(t.at(0).remaining, 30);
        assert_eq!(t.at(29).remaining, 1);
        assert_eq!(t.at(30).remaining, 30);
    }

    #[test]
    fn uri_round_trip() {
        let uri = "otpauth://totp/ACME%20Co:john@example.com?secret=HXDMVJECJJWSRB3HWIZR4IFUGFTMXBOZ&issuer=ACME%20Co&algorithm=SHA256&digits=8&period=60";
        let t = Totp::parse(uri).unwrap();
        assert_eq!(t.issuer, "ACME Co");
        assert_eq!(t.account, "john@example.com");
        assert_eq!(
            (t.algorithm, t.digits, t.period),
            (Algorithm::Sha256, 8, 60)
        );
        let again = Totp::parse(&t.to_uri()).unwrap();
        assert_eq!(again.secret, t.secret);
        assert_eq!(
            (again.algorithm, again.digits, again.period),
            (Algorithm::Sha256, 8, 60)
        );
        assert_eq!(again.issuer, "ACME Co");
    }

    #[test]
    fn bare_secret_with_spaces() {
        let t = Totp::parse("jbsw y3dp ehpk 3pxp").unwrap();
        assert_eq!(t.secret, b"Hello!\xde\xad\xbe\xef");
        assert_eq!((t.digits, t.period), (6, 30));
    }

    #[test]
    fn rejects_bad_input() {
        assert!(Totp::parse("").is_err());
        assert!(Totp::parse("not base32!").is_err());
        assert!(Totp::parse("otpauth://hotp/x?secret=JBSWY3DPEHPK3PXP&counter=1").is_err());
        assert!(Totp::parse("otpauth://totp/x?digits=6").is_err());
        assert!(Totp::parse("otpauth://totp/x?secret=JBSWY3DPEHPK3PXP&digits=3").is_err());
        assert!(Totp::parse("otpauth://totp/x?secret=JBSWY3DPEHPK3PXP&encoder=steam").is_err());
    }

    #[test]
    fn keepass_native_fields() {
        let fields = [
            ("TimeOtp-Secret-Base32", "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ"),
            ("TimeOtp-Length", "8"),
            ("TimeOtp-Algorithm", "HMAC-SHA-1"),
        ];
        let t = Totp::from_kdbx_fields(|k| fields.iter().find(|f| f.0 == k).map(|f| f.1))
            .unwrap()
            .unwrap();
        assert_eq!(t.at(59).code, "94287082");

        let keeotp = [
            ("TOTP Seed", "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ"),
            ("TOTP Settings", "30;8"),
        ];
        let t = Totp::from_kdbx_fields(|k| keeotp.iter().find(|f| f.0 == k).map(|f| f.1))
            .unwrap()
            .unwrap();
        assert_eq!(t.at(1_111_111_109).code, "07081804");
        assert!(Totp::from_kdbx_fields(|_| None).is_none());
    }

    #[test]
    fn debug_redacts_secret() {
        let t = Totp::parse("JBSWY3DPEHPK3PXP").unwrap();
        assert!(!format!("{t:?}").contains("72, 101"));
        assert!(format!("{t:?}").contains("redacted"));
    }
}
