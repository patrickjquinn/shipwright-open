// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

//! Password and passphrase generation, and strength estimates.
//!
//! Randomness comes from the operating system (`getrandom`), and every pick
//! is uniform: indexes are drawn by rejection sampling, never by a biased
//! modulo. Passphrases use the EFF large wordlist (7776 words, 12.9 bits a
//! word), embedded at build time from `data/eff_large_wordlist.txt`.

use zeroize::Zeroizing;

use crate::Error;

const WORDLIST_RAW: &str = include_str!("../data/eff_large_wordlist.txt");
pub const WORDLIST_LEN: usize = 7776;

const LOWER: &str = "abcdefghijklmnopqrstuvwxyz";
const UPPER: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const DIGITS: &str = "0123456789";
const SYMBOLS: &str = "!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~";
/// Characters easily confused when read or typed on a phone keyboard.
const AMBIGUOUS: &str = "Il1|O0o`'\"";

/// The EFF wordlist, one word per dice roll (the dice numbers are dropped).
pub fn wordlist() -> Vec<&'static str> {
    WORDLIST_RAW
        .lines()
        .filter_map(|l| l.split('\t').nth(1))
        .map(str::trim)
        .collect()
}

/// Options for a random character password.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent switches, one per toggle in the generator UI"
)]
pub struct PasswordOptions {
    pub length: usize,
    pub lower: bool,
    pub upper: bool,
    pub digits: bool,
    pub symbols: bool,
    pub exclude_ambiguous: bool,
}

impl Default for PasswordOptions {
    fn default() -> Self {
        Self {
            length: 20,
            lower: true,
            upper: true,
            digits: true,
            symbols: true,
            exclude_ambiguous: false,
        }
    }
}

/// Options for a passphrase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PassphraseOptions {
    pub words: usize,
    pub separator: String,
    pub capitalize: bool,
    /// Append one random digit to one random word (for sites that insist).
    pub add_digit: bool,
}

impl Default for PassphraseOptions {
    fn default() -> Self {
        Self {
            words: 6,
            separator: "-".into(),
            capitalize: false,
            add_digit: false,
        }
    }
}

/// A generated secret and its entropy in bits.
#[derive(Clone)]
pub struct Generated {
    pub value: Zeroizing<String>,
    pub entropy_bits: f64,
}

impl std::fmt::Debug for Generated {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Generated")
            .field("value", &"[redacted]")
            .field("entropy_bits", &self.entropy_bits)
            .finish()
    }
}

/// `n` as a float for entropy arithmetic. Lengths and list sizes here are
/// far below 2^32, where the conversion is exact; larger values saturate.
fn count_f64(n: usize) -> f64 {
    u32::try_from(n).map_or(f64::from(u32::MAX), f64::from)
}

/// Uniform integer in `0..n` from the OS RNG, by rejection sampling.
pub fn uniform(n: usize) -> Result<usize, Error> {
    if n == 0 {
        return Err(Error::Generator("empty range".into()));
    }
    let n = n as u64;
    let zone = u64::MAX - (u64::MAX % n);
    loop {
        let mut b = [0u8; 8];
        getrandom::fill(&mut b).map_err(|e| Error::Generator(e.to_string()))?;
        let v = u64::from_le_bytes(b);
        if v < zone {
            // v % n < n, and n came from a usize, so this cannot fail.
            return usize::try_from(v % n).map_err(|e| Error::Generator(e.to_string()));
        }
    }
}

fn class_chars(s: &str, exclude_ambiguous: bool) -> Vec<char> {
    s.chars()
        .filter(|c| !exclude_ambiguous || !AMBIGUOUS.contains(*c))
        .collect()
}

/// A random password with at least one character from every chosen class.
///
/// Entropy is reported as `length * log2(pool)`, a slight overestimate
/// because of the one-of-each rule (by less than 1 bit for the defaults).
pub fn password(opts: &PasswordOptions) -> Result<Generated, Error> {
    let mut classes: Vec<Vec<char>> = Vec::new();
    for (on, set) in [
        (opts.lower, LOWER),
        (opts.upper, UPPER),
        (opts.digits, DIGITS),
        (opts.symbols, SYMBOLS),
    ] {
        if on {
            classes.push(class_chars(set, opts.exclude_ambiguous));
        }
    }
    if classes.is_empty() {
        return Err(Error::Generator(
            "choose at least one character class".into(),
        ));
    }
    if opts.length < classes.len() || opts.length > 256 {
        return Err(Error::Generator(format!(
            "length must be {} to 256",
            classes.len()
        )));
    }
    let pool: Vec<char> = classes.iter().flatten().copied().collect();
    // Draw until every class is represented. With 4 classes and length 20
    // this retries less than 1% of the time; it never biases the result
    // towards any position.
    loop {
        let mut out = Zeroizing::new(String::with_capacity(opts.length));
        for _ in 0..opts.length {
            out.push(pool[uniform(pool.len())?]);
        }
        if classes
            .iter()
            .all(|c| out.chars().any(|ch| c.contains(&ch)))
        {
            return Ok(Generated {
                entropy_bits: count_f64(opts.length) * count_f64(pool.len()).log2(),
                value: out,
            });
        }
    }
}

/// A random passphrase from the EFF large wordlist.
pub fn passphrase(opts: &PassphraseOptions) -> Result<Generated, Error> {
    if !(3..=20).contains(&opts.words) {
        return Err(Error::Generator("a passphrase needs 3 to 20 words".into()));
    }
    let list = wordlist();
    let mut words: Vec<Zeroizing<String>> = Vec::with_capacity(opts.words);
    for _ in 0..opts.words {
        let w = list[uniform(list.len())?];
        let w = if opts.capitalize {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        } else {
            w.to_string()
        };
        words.push(Zeroizing::new(w));
    }
    let mut entropy = count_f64(opts.words) * count_f64(list.len()).log2();
    if opts.add_digit {
        let i = uniform(words.len())?;
        let d = DIGITS.as_bytes()[uniform(DIGITS.len())?];
        words[i].push(char::from(d));
        entropy += (10.0f64).log2() + count_f64(opts.words).log2();
    }
    let mut out = Zeroizing::new(String::new());
    for (i, w) in words.iter().enumerate() {
        if i > 0 {
            out.push_str(&opts.separator);
        }
        out.push_str(w);
    }
    Ok(Generated {
        value: out,
        entropy_bits: entropy,
    })
}

/// Rough strength of a password a person chose (not one we generated).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Strength {
    VeryWeak,
    Weak,
    Fair,
    Strong,
    VeryStrong,
}

impl Strength {
    pub fn from_bits(bits: f64) -> Self {
        match bits {
            b if b < 28.0 => Strength::VeryWeak,
            b if b < 36.0 => Strength::Weak,
            b if b < 60.0 => Strength::Fair,
            b if b < 100.0 => Strength::Strong,
            _ => Strength::VeryStrong,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Strength::VeryWeak => "Very weak",
            Strength::Weak => "Weak",
            Strength::Fair => "Fair",
            Strength::Strong => "Strong",
            Strength::VeryStrong => "Very strong",
        }
    }
}

/// Estimated entropy in bits of an arbitrary password.
///
/// A deliberately conservative heuristic, not a cracking model: the
/// character-pool estimate (`length * log2(pool)`) is reduced for repeated
/// characters, runs such as `abcd` or `4321`, and keyboard rows, and a
/// password that is a single wordlist word (with optional digits or
/// capitalisation) is scored as one word plus its decorations. It will
/// overrate a clever but predictable password; it never scores `password1`
/// or `qwertyuiop` as strong.
pub fn estimate_entropy(pw: &str) -> f64 {
    const ROWS: [&str; 4] = ["qwertyuiop", "asdfghjkl", "zxcvbnm", "1234567890"];
    let chars: Vec<char> = pw.chars().collect();
    if chars.is_empty() {
        return 0.0;
    }
    let lower = chars.iter().any(char::is_ascii_lowercase);
    let upper = chars.iter().any(char::is_ascii_uppercase);
    let digit = chars.iter().any(char::is_ascii_digit);
    let symbol = chars.iter().any(|c| c.is_ascii_punctuation() || *c == ' ');
    let other = chars.iter().any(|c| !c.is_ascii());
    let pool = 26 * u32::from(lower)
        + 26 * u32::from(upper)
        + 10 * u32::from(digit)
        + 33 * u32::from(symbol)
        + 100 * u32::from(other);
    let per_char = f64::from(pool.max(1)).log2();

    // Effective length: a character that repeats the previous one, or
    // continues a run (+1/-1) or a keyboard row, counts a quarter.
    let mut effective = 1.0;
    for w in chars.windows(2) {
        let (a, b) = (w[0].to_ascii_lowercase(), w[1].to_ascii_lowercase());
        let step = b as i64 - a as i64;
        let row = ROWS.iter().any(|r| {
            r.find(a)
                .zip(r.find(b))
                .is_some_and(|(i, j)| i.abs_diff(j) == 1)
        });
        effective += if a == b || step.abs() == 1 || row {
            0.25
        } else {
            1.0
        };
    }
    let mut bits = effective * per_char;

    // Dictionary word with decorations: word + optional digits/symbols.
    let core: String = chars
        .iter()
        .filter(|c| c.is_alphabetic())
        .map(char::to_ascii_lowercase)
        .collect();
    if core.len() >= 3 {
        let list = wordlist();
        let common = COMMON.contains(&core.as_str());
        if common || list.binary_search(&core.as_str()).is_ok() {
            let base = if common {
                4.0
            } else {
                count_f64(WORDLIST_LEN).log2()
            };
            let decorations =
                count_f64(chars.len() - core.len()) * 3.3 + if upper { 1.0 } else { 0.0 };
            bits = bits.min(base + decorations);
        }
    }
    bits
}

/// A handful of the most common passwords' letter cores. Enough to stop
/// the estimate calling them strong; this is not a breach list.
const COMMON: &[&str] = &[
    "password",
    "passw",
    "qwerty",
    "qwertyuiop",
    "letmein",
    "welcome",
    "admin",
    "iloveyou",
    "monkey",
    "dragon",
    "football",
    "baseball",
    "sunshine",
    "princess",
    "master",
    "abc",
    "abcdef",
    "login",
    "starwars",
    "trustno",
    "secret",
    "shadow",
    "superman",
    "michael",
    "hello",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wordlist_is_complete_and_sorted() {
        let l = wordlist();
        assert_eq!(l.len(), WORDLIST_LEN);
        assert_eq!(l[0], "abacus");
        assert_eq!(l[WORDLIST_LEN - 1], "zoom");
        assert!(
            l.windows(2).all(|w| w[0] < w[1]),
            "sorted, for binary search"
        );
    }

    #[test]
    fn password_respects_options() {
        for _ in 0..50 {
            let g = password(&PasswordOptions {
                length: 12,
                symbols: false,
                exclude_ambiguous: true,
                ..Default::default()
            })
            .unwrap();
            assert_eq!(g.value.chars().count(), 12);
            assert!(g.value.chars().all(|c| c.is_ascii_alphanumeric()));
            assert!(g.value.chars().any(|c| c.is_ascii_lowercase()));
            assert!(g.value.chars().any(|c| c.is_ascii_uppercase()));
            assert!(g.value.chars().any(|c| c.is_ascii_digit()));
            assert!(!g.value.chars().any(|c| AMBIGUOUS.contains(c)));
        }
    }

    #[test]
    fn password_entropy() {
        let g = password(&PasswordOptions::default()).unwrap();
        // 94 printable characters, 20 of them.
        assert!((g.entropy_bits - 20.0 * 94f64.log2()).abs() < 1e-9);
        let digits = password(&PasswordOptions {
            length: 10,
            lower: false,
            upper: false,
            symbols: false,
            ..Default::default()
        })
        .unwrap();
        assert!((digits.entropy_bits - 10.0 * 10f64.log2()).abs() < 1e-9);
    }

    #[test]
    fn password_rejects_bad_options() {
        let none = PasswordOptions {
            lower: false,
            upper: false,
            digits: false,
            symbols: false,
            ..Default::default()
        };
        assert!(password(&none).is_err());
        assert!(password(&PasswordOptions {
            length: 3,
            ..Default::default()
        })
        .is_err());
        assert!(password(&PasswordOptions {
            length: 1000,
            ..Default::default()
        })
        .is_err());
    }

    #[test]
    fn passphrase_shape_and_entropy() {
        let g = passphrase(&PassphraseOptions::default()).unwrap();
        let words: Vec<&str> = g.value.split('-').collect();
        assert_eq!(words.len(), 6);
        let list = wordlist();
        assert!(words.iter().all(|w| list.binary_search(w).is_ok()));
        assert!((g.entropy_bits - 6.0 * 7776f64.log2()).abs() < 1e-9);
        assert!(g.entropy_bits > 77.0);

        let g = passphrase(&PassphraseOptions {
            words: 4,
            separator: " ".into(),
            capitalize: true,
            add_digit: true,
        })
        .unwrap();
        assert_eq!(g.value.split(' ').count(), 4);
        assert!(g
            .value
            .split(' ')
            .all(|w| w.chars().next().unwrap().is_uppercase()));
        assert_eq!(g.value.chars().filter(char::is_ascii_digit).count(), 1);
        assert!(passphrase(&PassphraseOptions {
            words: 2,
            ..Default::default()
        })
        .is_err());
    }

    #[test]
    fn uniform_covers_range() {
        let mut seen = [false; 7];
        for _ in 0..500 {
            seen[uniform(7).unwrap()] = true;
        }
        assert!(seen.iter().all(|s| *s));
        assert!(uniform(0).is_err());
    }

    #[test]
    fn estimate_orders_sensibly() {
        let weak = [
            "",
            "password",
            "Password1",
            "qwertyuiop",
            "aaaaaaaaaa",
            "abcdefgh",
            "12345678",
        ];
        for w in weak {
            assert!(
                Strength::from_bits(estimate_entropy(w)) <= Strength::Weak,
                "{w:?} scored {}",
                estimate_entropy(w)
            );
        }
        assert!(
            Strength::from_bits(estimate_entropy("correct-horse-battery-staple"))
                >= Strength::Strong
        );
        assert!(Strength::from_bits(estimate_entropy("x7#Kq9!vL2@pZ4&w")) >= Strength::Strong);
        assert!(estimate_entropy("Tr0ub4dor&3") > estimate_entropy("troubador"));
    }
}
