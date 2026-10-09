// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

//! The CXX-Qt bridge: the `ShoalKeys` QML singleton (`import Shipwright.Keys 1.0`).
//!
//! Structured values cross to QML as JSON strings (`JSON.parse` on the QML
//! side). Operations that run Argon2 (`create`, `unlock`, `recover`,
//! `importFile`, `exportKdbx`, `changePassword`, and the save after an edit)
//! run on a worker thread and report through `finished(op, ok, message)`;
//! `busy` is true meanwhile. The Reef licence hand-off and the Downloads scan
//! for importable files run on workers too. Everything else returns at once
//! and never waits on the vault mutex (see `controller::Shared`).

// cxx-qt accepts no lint attributes on or inside the bridge module, so this
// exemption covers the file: `generatePassword` (declared in the bridge and
// implemented below) takes one bool per toggle of the generator page.
#![allow(
    clippy::fn_params_excessive_bools,
    reason = "the QML signature of generatePassword: one switch per generator toggle"
)]

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = cxx_qt_lib::QStringList;
        include!("cxx-qt-lib/qguiapplication.h");
        type QGuiApplication = cxx_qt_lib::QGuiApplication;
    }

    unsafe extern "C++" {
        include!("shipwright-shoal-keys/cpp/launcher.h");
        /// Loads `qml_path` into Keel's `SailfishApp::createView()` and shows
        /// it (full screen under keel-shell). False, with the QML errors
        /// printed, if it does not load. See cpp/launcher.cpp.
        fn keel_show_main_view(qml_path: &QString) -> bool;
        /// Deletes the view (and its engine and the QML objects in it)
        /// after the event loop ends, so that their threads stop before the
        /// process exits. See cpp/launcher.cpp.
        fn keel_destroy_main_view();
        /// Keel's application object (Keel::application()): booster-keel's
        /// when boosted, otherwise created on the first call.
        fn keel_application() -> *mut QGuiApplication;
        /// The app's command line (Keel::launchArguments()); see
        /// tools/keel-launcher/launcher.rs.
        fn keel_launch_arguments() -> QStringList;
    }

    unsafe extern "C++" {
        include!("shipwright-shoal-keys/cpp/clipboard.h");
        /// Puts `text` on the clipboard, marked as a secret for clipboard
        /// managers (`x-kde-passwordManagerHint: secret`).
        fn keys_clipboard_set(text: &QString);
        /// Clears the clipboard if it still holds `text`. True if cleared.
        fn keys_clipboard_clear_if(text: &QString) -> bool;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(bool, vault_exists, cxx_name = "vaultExists", READ, NOTIFY)]
        #[qproperty(bool, locked, READ, NOTIFY)]
        #[qproperty(bool, busy, READ, NOTIFY)]
        #[qproperty(i32, entry_count, cxx_name = "entryCount", READ, NOTIFY)]
        #[qproperty(i32, revision, READ, NOTIFY)]
        #[qproperty(QString, vault_name, cxx_name = "vaultName", READ, NOTIFY)]
        #[qproperty(QString, keystore_name, cxx_name = "keystoreName", READ, CONSTANT)]
        #[qproperty(QString, lock_reason, cxx_name = "lockReason", READ, NOTIFY)]
        #[qproperty(bool, licensed, READ, NOTIFY)]
        #[qproperty(QString, licence_text, cxx_name = "licenceText", READ, NOTIFY)]
        #[qproperty(
            i32,
            clipboard_clear_secs,
            cxx_name = "clipboardClearSecs",
            READ,
            NOTIFY
        )]
        #[qproperty(i32, idle_lock_secs, cxx_name = "idleLockSecs", READ, NOTIFY)]
        #[qproperty(
            i32,
            background_lock_secs,
            cxx_name = "backgroundLockSecs",
            READ,
            NOTIFY
        )]
        #[qproperty(bool, lock_on_screen_lock, cxx_name = "lockOnScreenLock", READ, NOTIFY)]
        type ShoalKeys = super::KeysRust;

        // Async (worker thread).
        #[qinvokable]
        fn create(self: Pin<&mut ShoalKeys>, name: &QString, password: &QString);
        #[qinvokable]
        fn unlock(self: Pin<&mut ShoalKeys>, password: &QString);
        #[qinvokable]
        fn recover(self: Pin<&mut ShoalKeys>, password: &QString, keyfile: &QString);
        #[qinvokable]
        #[cxx_name = "importFile"]
        fn import_file(self: Pin<&mut ShoalKeys>, path: &QString, password: &QString);
        #[qinvokable]
        #[cxx_name = "exportKdbx"]
        fn export_kdbx(self: Pin<&mut ShoalKeys>, path: &QString, password: &QString);
        #[qinvokable]
        #[cxx_name = "changePassword"]
        fn change_password(self: Pin<&mut ShoalKeys>, current: &QString, new_password: &QString);

        /// After an async operation. `op` is its name ("unlock", "save", ...).
        #[qsignal]
        fn finished(self: Pin<&mut ShoalKeys>, op: QString, ok: bool, message: QString);

        // Quick.
        #[qinvokable]
        fn lock(self: Pin<&mut ShoalKeys>);
        #[qinvokable]
        fn search(self: &ShoalKeys, query: &QString) -> QString;
        #[qinvokable]
        fn entry(self: &ShoalKeys, id: &QString) -> QString;
        #[qinvokable]
        fn history(self: &ShoalKeys, id: &QString) -> QString;
        #[qinvokable]
        fn totp(self: &ShoalKeys, id: &QString) -> QString;
        /// Adds or updates from JSON and starts a save. Returns
        /// `{"id": ...}` or `{"error": ...}`.
        #[qinvokable]
        #[cxx_name = "saveEntry"]
        fn save_entry(self: Pin<&mut ShoalKeys>, json: &QString) -> QString;
        /// Returns an error message, empty on success.
        #[qinvokable]
        #[cxx_name = "deleteEntry"]
        fn delete_entry(self: Pin<&mut ShoalKeys>, id: &QString) -> QString;
        #[qinvokable]
        #[cxx_name = "generatePassword"]
        fn generate_password(
            self: &ShoalKeys,
            length: i32,
            lower: bool,
            upper: bool,
            digits: bool,
            symbols: bool,
            no_ambiguous: bool,
        ) -> QString;
        #[qinvokable]
        #[cxx_name = "generatePassphrase"]
        fn generate_passphrase(
            self: &ShoalKeys,
            words: i32,
            separator: &QString,
            capitalize: bool,
            add_digit: bool,
        ) -> QString;
        #[qinvokable]
        fn strength(self: &ShoalKeys, password: &QString) -> QString;
        /// Why `otp` is not a usable TOTP secret or URI; empty if it is
        /// (or is empty).
        #[qinvokable]
        #[cxx_name = "checkOtp"]
        fn check_otp(self: &ShoalKeys, otp: &QString) -> QString;
        /// Starts a scan of Downloads for importable files; the result
        /// arrives in `importCandidates`.
        #[qinvokable]
        #[cxx_name = "scanImports"]
        fn scan_imports(self: Pin<&mut ShoalKeys>);
        /// `[{path, name, format, needsPassword}]` as JSON.
        #[qsignal]
        #[cxx_name = "importCandidates"]
        fn import_candidates(self: Pin<&mut ShoalKeys>, json: QString);
        #[qinvokable]
        #[cxx_name = "defaultExportPath"]
        fn default_export_path(self: &ShoalKeys) -> QString;
        #[qinvokable]
        #[cxx_name = "recoveryKeyPath"]
        fn recovery_key_path(self: &ShoalKeys) -> QString;
        #[qinvokable]
        #[cxx_name = "writeRecoveryKey"]
        fn write_recovery_key(self: &ShoalKeys, path: &QString) -> QString;
        #[qinvokable]
        #[cxx_name = "setSettings"]
        fn set_settings(
            self: Pin<&mut ShoalKeys>,
            clipboard_secs: i32,
            idle_secs: i32,
            background_secs: i32,
            lock_on_screen_lock: bool,
        ) -> QString;
        #[qinvokable]
        fn touch(self: Pin<&mut ShoalKeys>);
        #[qinvokable]
        #[cxx_name = "setActive"]
        fn set_active(self: Pin<&mut ShoalKeys>, active: bool);
        #[qinvokable]
        #[cxx_name = "checkAutoLock"]
        fn check_auto_lock(self: Pin<&mut ShoalKeys>) -> bool;
        #[qinvokable]
        #[cxx_name = "installLicence"]
        fn install_licence(self: Pin<&mut ShoalKeys>, token: &QString) -> QString;
        #[qinvokable]
        #[cxx_name = "removeLicence"]
        fn remove_licence(self: Pin<&mut ShoalKeys>) -> QString;
        #[qinvokable]
        #[cxx_name = "healthReport"]
        fn health_report(self: &ShoalKeys) -> QString;
        /// Emitted after `copy`; the window arms its clear timer.
        #[qsignal]
        fn copied(self: Pin<&mut ShoalKeys>);

        /// Copies a secret and emits `copied`.
        #[qinvokable]
        fn copy(self: Pin<&mut ShoalKeys>, text: &QString);
        /// Clears the clipboard if it still holds what we copied.
        #[qinvokable]
        #[cxx_name = "clearClipboard"]
        fn clear_clipboard(self: Pin<&mut ShoalKeys>) -> bool;

        /// Another app (Pacific) asks for a login, or to save one: the
        /// window comes up and shows the request (`autofillRequest`).
        #[qsignal]
        #[cxx_name = "autofillRequested"]
        fn autofill_requested(self: Pin<&mut ShoalKeys>, request: i32);
        /// A waiting request as JSON: `{request, kind: "fill"|"save",
        /// origin, host, logins: [{id, title, username}], preselected}` or
        /// `{..., username, update}`; `{}` when it is gone. Logins only
        /// while unlocked.
        #[qinvokable]
        #[cxx_name = "autofillRequest"]
        fn autofill_request(self: &ShoalKeys, request: i32) -> QString;
        /// The ids of the requests still waiting, as JSON.
        #[qinvokable]
        #[cxx_name = "autofillPending"]
        fn autofill_pending(self: &ShoalKeys) -> QString;
        /// The person chose entry `entry_id` (fill) or tapped Save (save).
        /// Returns an error message, empty on success.
        #[qinvokable]
        #[cxx_name = "autofillAnswer"]
        fn autofill_answer(self: Pin<&mut ShoalKeys>, request: i32, entry_id: &QString) -> QString;
        #[qinvokable]
        #[cxx_name = "autofillDecline"]
        fn autofill_decline(self: Pin<&mut ShoalKeys>, request: i32);
    }

    impl cxx_qt::Threading for ShoalKeys {}
    impl cxx_qt::Initialize for ShoalKeys {}
}

use core::pin::Pin;
use std::path::PathBuf;

use cxx_qt::{CxxQtThread, CxxQtType, Threading};
use cxx_qt_lib::QString;

use shoal_keys_core::generator::PasswordOptions;

use crate::autofill::{logins_for, normalise_origin, Ask, AutofillError, Pending};
use crate::controller::{
    adopt_reef_licence, import_candidates, keystore_from_env, Controller, KeysError, KeysResult,
    Paths, Settings, Shared,
};

#[allow(
    clippy::struct_excessive_bools,
    reason = "backing fields of independent QML properties"
)]
pub struct KeysRust {
    vault_exists: bool,
    locked: bool,
    busy: bool,
    entry_count: i32,
    revision: i32,
    vault_name: QString,
    keystore_name: QString,
    lock_reason: QString,
    licensed: bool,
    licence_text: QString,
    clipboard_clear_secs: i32,
    idle_lock_secs: i32,
    background_lock_secs: i32,
    lock_on_screen_lock: bool,
    ctl: Option<Controller>,
    autofill: Pending,
}

impl Default for KeysRust {
    fn default() -> Self {
        Self {
            vault_exists: false,
            locked: true,
            busy: false,
            entry_count: 0,
            revision: 0,
            vault_name: QString::default(),
            keystore_name: QString::default(),
            lock_reason: QString::default(),
            licensed: false,
            licence_text: QString::default(),
            clipboard_clear_secs: 30,
            idle_lock_secs: 300,
            background_lock_secs: 30,
            lock_on_screen_lock: true,
            ctl: None,
            autofill: Pending::default(),
        }
    }
}

macro_rules! set_notify {
    ($obj:expr, $field:ident, $notify:ident, $value:expr) => {{
        let value = $value;
        if $obj.$field != value {
            $obj.as_mut().rust_mut().$field = value;
            $obj.as_mut().$notify();
        }
    }};
}

fn q(s: &str) -> QString {
    QString::from(s)
}

fn err_json(msg: &str) -> QString {
    q(&serde_json::json!({ "error": msg }).to_string())
}

impl cxx_qt::Initialize for qobject::ShoalKeys {
    fn initialize(mut self: Pin<&mut Self>) {
        // Test-only: cheap Argon2 so the smoke test does not take minutes.
        let kdf = std::env::var("SHOAL_KEYS_TEST_FAST_KDF")
            .ok()
            .filter(|v| v == "1")
            .map(|_| shoal_keys_core::KdfParams::insecure_for_tests());
        let ctl = Controller::new(Paths::from_env(), keystore_from_env(), kdf);
        let (data, keys, generation) = ctl.licence_handoff();
        self.as_mut().rust_mut().keystore_name = q(ctl.keystore_name());
        self.as_mut().rust_mut().ctl = Some(ctl);
        self.as_mut().sync_settings();
        self.as_mut().sync_state();
        self.as_mut().start_screen_lock_watch();
        serve_autofill(self.qt_thread());
        // The Reef hand-off is a session-bus call (with activation): off the
        // GUI thread. `licensed` updates when it is back.
        let thread = self.qt_thread();
        let spawned = std::thread::Builder::new()
            .name("keys-licence".into())
            .spawn(move || {
                let state = adopt_reef_licence(&data, &keys);
                let _ = thread.queue(move |mut o| {
                    // Not over a licence installed or removed meanwhile.
                    if o.as_mut().ctl_mut().apply_reef_licence(generation, state) {
                        o.as_mut().sync_state();
                    }
                });
            });
        if let Err(e) = spawned {
            eprintln!("shipwright-shoal-keys: no licence hand-off thread: {e}");
        }
    }
}

impl qobject::ShoalKeys {
    fn ctl(&self) -> &Controller {
        #[expect(
            clippy::expect_used,
            reason = "set in initialize, before QML sees the object"
        )]
        self.ctl.as_ref().expect("initialised")
    }

    fn ctl_mut(self: Pin<&mut Self>) -> &mut Controller {
        #[expect(
            clippy::expect_used,
            reason = "set in initialize, before QML sees the object"
        )]
        self.rust_mut().get_mut().ctl.as_mut().expect("initialised")
    }

    fn shared(&self) -> Shared {
        self.ctl().shared.clone()
    }

    /// Re-reads everything the properties show.
    fn sync_state(mut self: Pin<&mut Self>) {
        let (exists, locked, count, name, licensed, licence_text, reason) = {
            let c = self.ctl();
            (
                c.vault_exists(),
                c.is_locked(),
                c.entry_count(),
                c.vault_name(),
                c.licence.is_licensed(),
                c.licence.describe(),
                c.lock_reason_text().to_string(),
            )
        };
        set_notify!(self, vault_exists, vault_exists_changed, exists);
        set_notify!(self, locked, locked_changed, locked);
        set_notify!(self, entry_count, entry_count_changed, count);
        set_notify!(self, vault_name, vault_name_changed, q(&name));
        set_notify!(self, licensed, licensed_changed, licensed);
        set_notify!(self, licence_text, licence_text_changed, q(&licence_text));
        set_notify!(self, lock_reason, lock_reason_changed, q(&reason));
    }

    fn bump_revision(mut self: Pin<&mut Self>) {
        let r = self.revision.wrapping_add(1);
        self.as_mut().rust_mut().revision = r;
        self.as_mut().revision_changed();
    }

    fn sync_settings(mut self: Pin<&mut Self>) {
        let s = self.ctl().settings.clone();
        set_notify!(
            self,
            clipboard_clear_secs,
            clipboard_clear_secs_changed,
            i32::try_from(s.clipboard_clear_secs).unwrap_or(i32::MAX)
        );
        set_notify!(
            self,
            idle_lock_secs,
            idle_lock_secs_changed,
            i32::try_from(s.idle_lock_secs).unwrap_or(i32::MAX)
        );
        set_notify!(
            self,
            background_lock_secs,
            background_lock_secs_changed,
            s.background_lock_secs
        );
        set_notify!(
            self,
            lock_on_screen_lock,
            lock_on_screen_lock_changed,
            s.lock_on_screen_lock
        );
    }

    /// Runs `work` on a worker thread, then `finished(op, ok, message)`.
    fn run(
        mut self: Pin<&mut Self>,
        op: &'static str,
        work: impl FnOnce(Shared) -> KeysResult<String> + Send + 'static,
    ) {
        if self.busy {
            self.as_mut()
                .finished(q(op), false, q(&KeysError::Busy.to_string()));
            return;
        }
        if matches!(op, "create" | "unlock" | "recover") {
            self.shared().clear_lock_request();
        }
        set_notify!(self, busy, busy_changed, true);
        let shared = self.shared();
        let thread: CxxQtThread<qobject::ShoalKeys> = self.qt_thread();
        let spawned = std::thread::Builder::new()
            .name(format!("keys-{op}"))
            .spawn(move || {
                let result = work(shared);
                let _ = thread.queue(move |mut o| {
                    set_notify!(o, busy, busy_changed, false);
                    if matches!(op, "create" | "unlock" | "recover") && result.is_ok() {
                        o.as_mut().ctl_mut().touch();
                        o.as_mut().ctl_mut().last_lock_reason = None;
                    }
                    o.as_mut().sync_state();
                    o.as_mut().bump_revision();
                    let (ok, msg) = match result {
                        Ok(m) => (true, m),
                        Err(e) => (false, e.to_string()),
                    };
                    o.as_mut().finished(q(op), ok, q(&msg));
                });
            });
        if spawned.is_err() {
            set_notify!(self, busy, busy_changed, false);
            self.as_mut()
                .finished(q(op), false, q(&KeysError::NoWorker.to_string()));
        }
    }

    /// Watches MCE's tklock signal on the system bus so the vault can lock
    /// with the device. **[device verification]** Silently does nothing
    /// without a system bus (desktop, smoke test).
    fn start_screen_lock_watch(self: Pin<&mut Self>) {
        let thread = self.qt_thread();
        let _ = std::thread::Builder::new()
            .name("keys-tklock".into())
            .spawn(move || {
                let Ok(conn) = zbus::blocking::Connection::system() else {
                    return;
                };
                let Ok(rule) = zbus::MatchRule::builder()
                    .msg_type(zbus::message::Type::Signal)
                    .interface("com.nokia.mce.signal")
                    .and_then(|b| b.member("tklock_mode_ind"))
                    .map(zbus::match_rule::Builder::build)
                else {
                    return;
                };
                let Ok(iter) =
                    zbus::blocking::MessageIterator::for_match_rule(rule, &conn, Some(8))
                else {
                    return;
                };
                for msg in iter.flatten() {
                    if let Ok(mode) = msg.body().deserialize::<String>() {
                        let locked = mode == "locked";
                        if thread
                            .queue(move |mut o| {
                                o.as_mut().ctl_mut().set_screen_locked(locked);
                                o.as_mut().check_auto_lock();
                            })
                            .is_err()
                        {
                            return;
                        }
                    }
                }
            });
    }

    fn create(self: Pin<&mut Self>, name: &QString, password: &QString) {
        let (name, pw) = (
            name.to_string(),
            zeroize::Zeroizing::new(password.to_string()),
        );
        self.run("create", move |s| {
            s.create(&name, &pw).map(|()| String::new())
        });
    }

    fn unlock(self: Pin<&mut Self>, password: &QString) {
        let pw = zeroize::Zeroizing::new(password.to_string());
        self.run("unlock", move |s| s.unlock(&pw).map(|()| String::new()));
    }

    fn recover(self: Pin<&mut Self>, password: &QString, keyfile: &QString) {
        let (pw, path) = (
            zeroize::Zeroizing::new(password.to_string()),
            PathBuf::from(keyfile.to_string()),
        );
        self.run("recover", move |s| {
            s.recover(&pw, &path).map(|()| String::new())
        });
    }

    fn import_file(self: Pin<&mut Self>, path: &QString, password: &QString) {
        let (path, pw) = (
            PathBuf::from(path.to_string()),
            zeroize::Zeroizing::new(password.to_string()),
        );
        self.run("import", move |s| s.import_file(&path, &pw));
    }

    fn export_kdbx(self: Pin<&mut Self>, path: &QString, password: &QString) {
        let (path, pw) = (
            PathBuf::from(path.to_string()),
            zeroize::Zeroizing::new(password.to_string()),
        );
        self.run("export", move |s| {
            s.export_kdbx(&path, &pw)?;
            Ok(format!("Exported to {}", path.display()))
        });
    }

    fn change_password(self: Pin<&mut Self>, current: &QString, new_password: &QString) {
        let (cur, new) = (
            zeroize::Zeroizing::new(current.to_string()),
            zeroize::Zeroizing::new(new_password.to_string()),
        );
        self.run("changePassword", move |s| {
            s.change_password(&cur, &new).map(|()| String::new())
        });
    }

    /// Never blocks: if a worker is saving, the vault is dropped when it
    /// finishes, and reads as locked meanwhile.
    fn lock(mut self: Pin<&mut Self>) {
        self.as_mut().clear_clipboard();
        self.as_mut().ctl_mut().lock();
        self.as_mut().sync_state();
        self.as_mut().bump_revision();
    }

    fn search(&self, query: &QString) -> QString {
        q(&self.ctl().search(&query.to_string()))
    }

    fn entry(&self, id: &QString) -> QString {
        q(&self.ctl().entry(&id.to_string()))
    }

    fn history(&self, id: &QString) -> QString {
        q(&self.ctl().history(&id.to_string()))
    }

    fn totp(&self, id: &QString) -> QString {
        q(&self.ctl().totp(&id.to_string()))
    }

    fn save_entry(mut self: Pin<&mut Self>, json: &QString) -> QString {
        if self.busy {
            return err_json(&KeysError::Busy.to_string());
        }
        let res = self.as_mut().ctl_mut().put_entry(&json.to_string());
        match res {
            Ok(id) => {
                self.as_mut().sync_state();
                self.as_mut().bump_revision();
                self.as_mut()
                    .run("save", |s| s.save().map(|()| String::new()));
                q(&serde_json::json!({ "id": id }).to_string())
            }
            Err(e) => err_json(&e.to_string()),
        }
    }

    fn delete_entry(mut self: Pin<&mut Self>, id: &QString) -> QString {
        if self.busy {
            return q(&KeysError::Busy.to_string());
        }
        let res = self.as_mut().ctl_mut().delete_entry(&id.to_string());
        match res {
            Ok(()) => {
                self.as_mut().sync_state();
                self.as_mut().bump_revision();
                self.as_mut()
                    .run("save", |s| s.save().map(|()| String::new()));
                QString::default()
            }
            Err(e) => q(&e.to_string()),
        }
    }

    fn generate_password(
        &self,
        length: i32,
        lower: bool,
        upper: bool,
        digits: bool,
        symbols: bool,
        no_ambiguous: bool,
    ) -> QString {
        q(&self.ctl().generate_password(PasswordOptions {
            // Negative lengths become 0, which the controller clamps to 4.
            length: usize::try_from(length).unwrap_or(0),
            lower,
            upper,
            digits,
            symbols,
            exclude_ambiguous: no_ambiguous,
        }))
    }

    fn generate_passphrase(
        &self,
        words: i32,
        separator: &QString,
        capitalize: bool,
        add_digit: bool,
    ) -> QString {
        q(&self
            .ctl()
            .generate_passphrase(words, &separator.to_string(), capitalize, add_digit))
    }

    fn strength(&self, password: &QString) -> QString {
        q(&self.ctl().strength(&password.to_string()))
    }

    #[allow(
        clippy::unused_self,
        reason = "a QML invokable of the ShoalKeys object; cxx-qt binds it as a method"
    )]
    fn check_otp(&self, otp: &QString) -> QString {
        let otp = otp.to_string();
        if otp.trim().is_empty() {
            return QString::default();
        }
        match shoal_keys_core::totp::Totp::parse(otp.trim()) {
            Ok(_) => QString::default(),
            Err(e) => q(&e.to_string()),
        }
    }

    fn scan_imports(self: Pin<&mut Self>) {
        let downloads = self.ctl().paths.downloads.clone();
        let thread = self.qt_thread();
        let spawned = std::thread::Builder::new()
            .name("keys-scan".into())
            .spawn(move || {
                let json = import_candidates(&downloads);
                let _ = thread.queue(move |o| o.import_candidates(q(&json)));
            });
        if spawned.is_err() {
            self.import_candidates(q("[]"));
        }
    }

    fn default_export_path(&self) -> QString {
        q(&self.ctl().default_export_path())
    }

    fn recovery_key_path(&self) -> QString {
        q(&self
            .ctl()
            .paths
            .downloads
            .join("shoal-keys-recovery.keyx")
            .to_string_lossy())
    }

    fn write_recovery_key(&self, path: &QString) -> QString {
        match self
            .ctl()
            .write_recovery_key(&PathBuf::from(path.to_string()))
        {
            Ok(()) => QString::default(),
            Err(e) => q(&e.to_string()),
        }
    }

    fn set_settings(
        mut self: Pin<&mut Self>,
        clipboard_secs: i32,
        idle_secs: i32,
        background_secs: i32,
        lock_on_screen_lock: bool,
    ) -> QString {
        let s = Settings {
            clipboard_clear_secs: u32::try_from(clipboard_secs).unwrap_or(0),
            idle_lock_secs: u32::try_from(idle_secs).unwrap_or(0),
            background_lock_secs: background_secs,
            lock_on_screen_lock,
        };
        let res = self.as_mut().ctl_mut().set_settings(s);
        self.as_mut().sync_settings();
        match res {
            Ok(()) => QString::default(),
            Err(e) => q(&e.to_string()),
        }
    }

    fn touch(self: Pin<&mut Self>) {
        self.ctl_mut().touch();
    }

    fn set_active(self: Pin<&mut Self>, active: bool) {
        self.ctl_mut().set_active(active);
    }

    fn check_auto_lock(mut self: Pin<&mut Self>) -> bool {
        if self.busy {
            return false;
        }
        let locked = self.as_mut().ctl_mut().check_autolock();
        if locked {
            self.as_mut().clear_clipboard();
            self.as_mut().sync_state();
            self.as_mut().bump_revision();
        }
        locked
    }

    fn install_licence(mut self: Pin<&mut Self>, token: &QString) -> QString {
        let res = self.as_mut().ctl_mut().install_licence(&token.to_string());
        self.as_mut().sync_state();
        match res {
            Ok(()) => QString::default(),
            Err(e) => q(&e.to_string()),
        }
    }

    fn remove_licence(mut self: Pin<&mut Self>) -> QString {
        let res = self.as_mut().ctl_mut().remove_licence();
        self.as_mut().sync_state();
        match res {
            Ok(()) => QString::default(),
            Err(e) => q(&e.to_string()),
        }
    }

    fn health_report(&self) -> QString {
        q(&self.ctl().health_report())
    }

    fn copy(mut self: Pin<&mut Self>, text: &QString) {
        qobject::keys_clipboard_set(text);
        self.as_mut().ctl_mut().copied = Some(zeroize::Zeroizing::new(text.to_string()));
        self.as_mut().copied();
    }

    fn clear_clipboard(mut self: Pin<&mut Self>) -> bool {
        let Some(copied) = self.as_mut().ctl_mut().copied.take() else {
            return false;
        };
        qobject::keys_clipboard_clear_if(&q(&copied))
    }
}

impl qobject::ShoalKeys {
    /// GUI thread: a request from the D-Bus thread waits for the person.
    fn autofill_ask(
        mut self: Pin<&mut Self>,
        ask: Ask,
        reply: async_channel::Sender<crate::autofill::Reply>,
    ) {
        let id = self.as_mut().rust_mut().autofill.add(ask, reply);
        self.as_mut()
            .autofill_requested(i32::try_from(id).unwrap_or(i32::MAX));
    }

    fn autofill_request(&self, request: i32) -> QString {
        let id = u32::try_from(request).unwrap_or(0);
        let Some(ask) = self.autofill.ask(id) else {
            return q("{}");
        };
        // A save in progress holds the vault for a moment: say so, and the
        // page asks again when `busy` changes.
        let (entries, waiting) = match self.ctl().autofill_entries() {
            Ok(e) => (e, false),
            Err(KeysError::Busy) => (Vec::new(), true),
            Err(_) => (Vec::new(), false),
        };
        let logins = logins_for(&entries, ask.origin());
        let update = match ask {
            Ask::Save {
                origin, username, ..
            } => crate::autofill::save_target(&entries, origin, username).is_some(),
            Ask::Fill { .. } => false,
        };
        let mut v: serde_json::Value =
            serde_json::from_str(&ask.describe(id, &logins, update)).unwrap_or_default();
        v["waiting"] = serde_json::json!(waiting || self.busy);
        q(&v.to_string())
    }

    fn autofill_pending(&self) -> QString {
        q(&serde_json::to_string(&self.autofill.ids()).unwrap_or_else(|_| "[]".into()))
    }

    fn autofill_answer(mut self: Pin<&mut Self>, request: i32, entry_id: &QString) -> QString {
        let id = u32::try_from(request).unwrap_or(0);
        let Some(ask) = self.autofill.ask(id).cloned() else {
            return q("The request has gone.");
        };
        // A save in progress holds the vault: the request stays, and the
        // person (or the page) tries again.
        let reply = match ask {
            Ask::Fill { origin, .. } => match self
                .ctl()
                .autofill_credentials(&origin, &entry_id.to_string())
            {
                Ok((username, password)) => {
                    let body = zeroize::Zeroizing::new(
                        serde_json::json!({ "username": username, "password": password })
                            .to_string(),
                    );
                    Ok(body.to_string())
                }
                Err(KeysError::Busy) => return q(&KeysError::Busy.to_string()),
                Err(e) => Err(AutofillError::Invalid(e.to_string())),
            },
            Ask::Save {
                origin,
                username,
                password,
            } => {
                if self.busy {
                    return q(&KeysError::Busy.to_string());
                }
                let password = zeroize::Zeroizing::new(password);
                match self
                    .as_mut()
                    .ctl_mut()
                    .autofill_save(&origin, &username, &password)
                {
                    Ok((_, updated)) => {
                        self.as_mut().sync_state();
                        self.as_mut().bump_revision();
                        self.as_mut()
                            .run("save", |s| s.save().map(|()| String::new()));
                        Ok(if updated { "updated" } else { "saved" }.to_string())
                    }
                    Err(KeysError::Busy) => return q(&KeysError::Busy.to_string()),
                    Err(e) => Err(AutofillError::Denied(e.to_string())),
                }
            }
        };
        let message = match &reply {
            Ok(_) => QString::default(),
            Err(e) => q(&format!("{e:?}")),
        };
        if let Some((_, tx)) = self.as_mut().rust_mut().autofill.take(id) {
            let _ = tx.try_send(reply);
        }
        message
    }

    fn autofill_decline(mut self: Pin<&mut Self>, request: i32) {
        let id = u32::try_from(request).unwrap_or(0);
        if let Some((_, tx)) = self.as_mut().rust_mut().autofill.take(id) {
            let _ = tx.try_send(Err(AutofillError::Denied("declined".into())));
        }
    }
}

impl Drop for KeysRust {
    fn drop(&mut self) {
        self.autofill.decline_all();
    }
}

/// The autofill interface (src/autofill.rs), on its own session-bus
/// connection: every call hops to the GUI thread, where the vault is.
struct AutofillServer {
    thread: CxxQtThread<qobject::ShoalKeys>,
}

impl AutofillServer {
    async fn on_gui<T: Send + 'static>(
        &self,
        f: impl FnOnce(Pin<&mut qobject::ShoalKeys>, async_channel::Sender<Result<T, AutofillError>>)
            + Send
            + 'static,
    ) -> Result<T, AutofillError> {
        let (tx, rx) = async_channel::bounded(1);
        self.thread
            .queue(move |o| f(o, tx))
            .map_err(|_| AutofillError::Denied("Shoal Keys is closing".into()))?;
        rx.recv()
            .await
            .unwrap_or_else(|_| Err(AutofillError::Denied("Shoal Keys is closing".into())))
    }
}

fn web_origin(origin: &str) -> Result<String, AutofillError> {
    normalise_origin(origin).ok_or_else(|| AutofillError::Invalid("not a web page".into()))
}

#[zbus::interface(name = "org.shipwright.Keys.Autofill1")]
impl AutofillServer {
    #[zbus(name = "Logins")]
    async fn logins(&self, origin: String) -> Result<String, AutofillError> {
        let origin = web_origin(&origin)?;
        self.on_gui(move |o, tx| {
            let r =
                match o.ctl().autofill_entries() {
                    Ok(e) => Ok(serde_json::to_string(&logins_for(&e, &origin))
                        .unwrap_or_else(|_| "[]".into())),
                    Err(KeysError::Locked | KeysError::Busy) => {
                        Err(AutofillError::Locked("Shoal Keys is locked".into()))
                    }
                    Err(e) => Err(AutofillError::Denied(e.to_string())),
                };
            let _ = tx.try_send(r);
        })
        .await
    }

    #[zbus(name = "Fill")]
    async fn fill(&self, origin: String, id: String) -> Result<String, AutofillError> {
        let origin = web_origin(&origin)?;
        self.on_gui(move |o, tx| o.autofill_ask(Ask::Fill { origin, id }, tx))
            .await
    }

    #[zbus(name = "Save")]
    async fn save(
        &self,
        origin: String,
        username: String,
        password: String,
    ) -> Result<String, AutofillError> {
        let origin = web_origin(&origin)?;
        self.on_gui(move |o, tx| {
            o.autofill_ask(
                Ask::Save {
                    origin,
                    username,
                    password,
                },
                tx,
            );
        })
        .await
    }
}

/// Serves the interface from a background thread; without a session bus
/// (or with another instance holding the name) it only logs.
fn serve_autofill(thread: CxxQtThread<qobject::ShoalKeys>) {
    let spawned = std::thread::Builder::new()
        .name("keys-autofill".into())
        .spawn(move || {
            let conn = zbus::blocking::connection::Builder::session()
                .and_then(|b| b.name(crate::autofill::BUS_NAME))
                .and_then(|b| b.serve_at(crate::autofill::OBJECT_PATH, AutofillServer { thread }))
                .and_then(zbus::blocking::connection::Builder::build);
            match conn {
                // Kept for the life of the process.
                Ok(c) => std::mem::forget(c),
                Err(e) => eprintln!("shipwright-shoal-keys: autofill is not served: {e}"),
            }
        });
    if let Err(e) = spawned {
        eprintln!("shipwright-shoal-keys: no autofill thread: {e}");
    }
}

// Keel's launcher: launch_args() and keel_application().
include!("../../../../tools/keel-launcher/launcher.rs");
