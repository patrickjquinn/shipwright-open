// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

use thiserror::Error;

/// Every error the core returns. Messages never contain secret material;
/// they are shown to the user as they are (Shoal Keys' UI), so they are
/// sentences: capitalised, and complete ones end with a full stop.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// Wrong master password or key file, or a damaged file whose integrity
    /// check failed (KDBX cannot tell the two apart).
    #[error("Wrong password or key file, or the file is damaged.")]
    WrongKey,
    #[error("Vault file: {0}")]
    Kdbx(String),
    #[error("Key wrapping: {0}")]
    Wrap(String),
    #[error("Platform key store: {0}")]
    KeyStore(String),
    #[error("No vault here.")]
    NoVault,
    #[error("A vault already exists here.")]
    VaultExists,
    #[error("The vault is locked.")]
    Locked,
    #[error("No entry with id {0}.")]
    NotFound(String),
    #[error("One-time password: {0}")]
    Totp(String),
    #[error("Import: {0}")]
    Import(String),
    #[error("Generator: {0}")]
    Generator(String),
    #[error("Licence: {0}")]
    Licence(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::Error;

    /// The UI shows these as they are: they read as sentences.
    #[test]
    fn messages_are_capitalised() {
        let all = [
            Error::WrongKey,
            Error::Kdbx("x".into()),
            Error::Wrap("x".into()),
            Error::KeyStore("x".into()),
            Error::NoVault,
            Error::VaultExists,
            Error::Locked,
            Error::NotFound("1".into()),
            Error::Totp("x".into()),
            Error::Import("x".into()),
            Error::Generator("x".into()),
            Error::Licence("x".into()),
        ];
        for e in all {
            let m = e.to_string();
            assert!(m.chars().next().is_some_and(char::is_uppercase), "{m}");
        }
        assert_eq!(
            Error::WrongKey.to_string(),
            "Wrong password or key file, or the file is damaged."
        );
    }
}
