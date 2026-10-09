// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

//! reef-backend: the Rust half of the Reef store client.
//!
//! The QML UI (reef/client/ui) talks to one backend object exposed through
//! CXX-Qt (`reef/client/app/src/bridge.rs`); that object is a thin shell
//! over this crate:
//!
//! * [`release`]: which Sailfish release is installed (not `ssu re`).
//! * [`repo`]: the exact ssu/rpm/pkcon commands that register the Reef
//!   repository pinned to that release, and drift detection after an OS
//!   update.
//! * [`catalogue`]: the per-release, per-arch catalogue, hiding packages not
//!   tested on this release.
//! * [`packagekit`]: resolve, install, update and remove through `PackageKit`
//!   over D-Bus, with progress.
//! * [`licences`]: storage and offline verification of licence tokens.
//! * [`claim_codes`]: the codes that fetch a licence bought on the web.
//! * [`licenced`]: the session-bus service that hands tokens to sandboxed
//!   apps (`shipwright-reef-licenced`, src/bin).
//! * [`store`]: the catalogue joined with install state, for the UI.
//!
//! No networking lives here: the UI layer downloads the catalogue, tokens
//! and revocation list and passes the bytes in.

pub mod catalogue;
pub mod claim_codes;
pub mod licenced;
pub mod licences;
pub mod packagekit;
pub mod release;
pub mod repo;
pub mod store;
