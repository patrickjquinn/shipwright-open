// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: MIT

//! What Keel knows about QML modules, Silica types and SailfishApp APIs.
//!
//! Only public names appear here. Silica statuses follow
//! keel/silica/COMPATIBILITY.md for Keel 0.1 (implemented, partial, missing);
//! the Nemo modules and SailfishApp APIs follow keel/nemo-compat and
//! keel/sailfishapp. (Before Keel shipped, a `planned` status stood for "in
//! the Keel v1 scope"; JSON schema 2 dropped it.)

use std::fmt;

use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    /// Works under Qt6 + Keel today.
    Supported,
    /// Provided by Keel today, with documented gaps (the entry's note says
    /// which). Reachable, but flagged in reports.
    Partial,
    /// Not provided by Keel, or needing a source change.
    Missing,
    /// Not in the catalogue.
    Unknown,
    /// Provided by the app itself (its own C++ types or bundled QML
    /// plugin); rebuilt with the app, so neither reachable nor a blocker.
    App,
}

impl Status {
    /// Whether the item is reachable with Keel without source changes
    /// (supported or partial).
    pub fn reachable(self) -> bool {
        matches!(self, Status::Supported | Status::Partial)
    }

    /// Whether the item counts towards the score and blocker list.
    pub fn scored(self) -> bool {
        self != Status::App
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(match self {
            Status::Supported => "supported",
            Status::Partial => "partial",
            Status::Missing => "missing",
            Status::Unknown => "unknown",
            Status::App => "app",
        })
    }
}

pub struct Entry {
    pub name: &'static str,
    pub status: Status,
    pub note: &'static str,
}

const fn e(name: &'static str, status: Status, note: &'static str) -> Entry {
    Entry { name, status, note }
}

pub const SILICA_MODULE: &str = "Sailfish.Silica";

pub static MODULES: &[Entry] = &[
    e("QtQuick", Status::Supported, "Qt6"),
    e("QtTest", Status::Supported, "Qt6 (app tests)"),
    e("QtQuick.Layouts", Status::Supported, "Qt6"),
    e("QtQuick.Window", Status::Supported, "Qt6"),
    e("QtQuick.LocalStorage", Status::Supported, "Qt6"),
    e(
        "QtQuick.Controls",
        Status::Supported,
        "Qt6 has Controls 2 only; 1.x imports need source changes",
    ),
    e(
        "QtQuick.Dialogs",
        Status::Supported,
        "Qt6; API differs from Qt5's 1.x",
    ),
    e("QtQml", Status::Supported, "Qt6"),
    e("QtQml.Models", Status::Supported, "Qt6"),
    e(
        "Qt.labs.settings",
        Status::Supported,
        "Qt6 (deprecated for QtCore Settings)",
    ),
    e(
        "QtPositioning",
        Status::Supported,
        "Qt6; check Chum packages it",
    ),
    e(
        "QtWebSockets",
        Status::Supported,
        "Qt6; check Chum packages it",
    ),
    e(
        "QtSensors",
        Status::Supported,
        "Qt6; check Chum packages it",
    ),
    e(
        "QtMultimedia",
        Status::Partial,
        "5.x imports via keel/qt5compat: MediaPlayer, Audio, VideoOutput, SoundEffect, Camera (no recording); no Playlist, Radio, Torch or Video",
    ),
    e(
        "QtGraphicalEffects",
        Status::Supported,
        "keel/qt5compat re-exports Qt5Compat.GraphicalEffects (Chum qt6-qt5compat)",
    ),
    e(
        "QtFeedback",
        Status::Supported,
        "keel/qt5compat: effects accepted, no haptics yet (device NGF mapping pending)",
    ),
    e(
        "QtMobility.feedback",
        Status::Missing,
        "Qt Mobility; no Qt6 module",
    ),
    e(SILICA_MODULE, Status::Supported, "keel/silica (Keel 0.1)"),
    e(
        "Keel",
        Status::Supported,
        "Keel extensions (Ambience, Shell), keel/silica",
    ),
    e(
        "Keel.Actions",
        Status::Supported,
        "keel/actions: app actions for Pilot and MCP",
    ),
    e(
        "Sailfish.Silica.private",
        Status::Missing,
        "private Silica API; not provided by Keel",
    ),
    e("Nemo.DBus", Status::Supported, "keel/nemo-compat"),
    e("Nemo.Notifications", Status::Supported, "keel/nemo-compat"),
    e("Nemo.KeepAlive", Status::Supported, "keel/nemo-compat"),
    e(
        "Nemo.Configuration",
        Status::Supported,
        "keel/nemo-compat (dconf through mlite-qt6 on the device)",
    ),
    e(
        "Nemo.Thumbnailer",
        Status::Supported,
        "keel/platform: nemo-qml-plugin-thumbnailer built for Qt 6 (videos and PDFs through thumbnaild)",
    ),
    e(
        "Nemo.Policy",
        Status::Supported,
        "keel/nemo-compat: nemo-qml-plugin-policy on libresourceqt (Qt 6) with a QtDBus resource-manager engine; local grants without a manager",
    ),
    e(
        "Nemo.Ngf",
        Status::Supported,
        "keel/nemo-compat: libngf-qt's plugin; events played by ngfd",
    ),
    e(
        "Nemo.Mce",
        Status::Supported,
        "keel/platform: libmce-qt built for Qt 6 (MCE display, lock, battery, charger and call state)",
    ),
    // Pre-Nemo URIs of the same plugins; keel/nemo-compat registers the
    // legacy URIs of the modules it provides.
    e(
        "org.nemomobile.dbus",
        Status::Supported,
        "legacy URI of Nemo.DBus, keel/nemo-compat",
    ),
    e(
        "org.nemomobile.notifications",
        Status::Supported,
        "legacy URI of Nemo.Notifications, keel/nemo-compat",
    ),
    e(
        "org.nemomobile.keepalive",
        Status::Supported,
        "legacy URI of Nemo.KeepAlive, keel/nemo-compat",
    ),
    e(
        "org.nemomobile.configuration",
        Status::Supported,
        "legacy URI of Nemo.Configuration, keel/nemo-compat",
    ),
    e(
        "org.nemomobile.policy",
        Status::Supported,
        "keel/nemo-compat: nemo-qml-plugin-policy on libresourceqt (Qt 6) with a QtDBus resource-manager engine; local grants without a manager",
    ),
    e(
        "org.nemomobile.ngf",
        Status::Supported,
        "legacy URI of Nemo.Ngf, keel/nemo-compat",
    ),
    e(
        "org.nemomobile.mpris",
        Status::Supported,
        "keel/platform: qtmpris built for Qt 6 (MprisPlayer, MprisManager, Mpris)",
    ),
    e(
        "org.nemomobile.accounts",
        Status::Supported,
        "keel/platform: nemo-qml-plugin-accounts on libaccounts-qt6 (system accounts database)",
    ),
    e(
        "org.nemomobile.lipstick",
        Status::Supported,
        "keel/nemo-compat: lipstick's launcher types (LauncherItem, LauncherModel, LauncherWatcherModel, LauncherFolderModel) for Qt 6; the home screen's types are not for apps",
    ),
    e(
        "Sailfish.Pickers",
        Status::Supported,
        "keel/platform: all documented pages and dialogs; Tracker 3, else a file system scan",
    ),
    e(
        "Sailfish.Share",
        Status::Supported,
        "keel/silica: ShareAction opens the system share dialog (org.sailfishos.share), clipboard fallback; ShareProvider receives shares",
    ),
    e(
        "Sailfish.WebView",
        Status::Supported,
        "keel/platform: the WebView API on Qt WebEngine (Chum qt6-qtwebengine), else opens pages in the browser; dialogs and pickers through sailfish-components-webview's popups; frame scripts in an isolated world; security from Keel's own TLS check",
    ),
    e(
        "Sailfish.WebView.Popups",
        Status::Supported,
        "keel/platform: sailfish-components-webview's popups (MPL-2.0) for Qt WebEngine's dialog, permission and context menu requests; PopupProvider",
    ),
    e(
        "Sailfish.WebView.Pickers",
        Status::Supported,
        "keel/platform: sailfish-components-webview's pickers (MPL-2.0) for file uploads and colours",
    ),
    e(
        "Sailfish.WebView.Controls",
        Status::Supported,
        "keel/platform: sailfish-components-webview's controls (MPL-2.0): PermissionModel, PermissionManager over remembered site permissions; TextSelectionController inert (Chromium's own selection)",
    ),
    e(
        "Sailfish.WebEngine",
        Status::Supported,
        "keel/platform: WebEngine's documented API on Qt WebEngine (observers in-app plus clipboard, site permission and download topics; user style sheets; GPU switch); WebEngineSettings applied to Qt WebEngine, other Gecko preferences kept without effect",
    ),
    e(
        "Sailfish.Media",
        Status::Supported,
        "keel/platform: MediaKey (Lipstick GRABBED_KEYS), the documented media player types and MprisPlayerControls (over Amber.Mpris)",
    ),
    e(
        "Sailfish.Secrets",
        Status::Supported,
        "keel/platform: sailfish-secrets' plugin built for Qt 6 (sailfishsecretsd)",
    ),
    e(
        "Sailfish.Crypto",
        Status::Supported,
        "keel/platform: sailfish-secrets' plugin built for Qt 6 (sailfishsecretsd)",
    ),
    e(
        "Sailfish.Accounts",
        Status::Supported,
        "keel/platform: sailfish-components-accounts (BSD) for Qt 6 with libsignon-qt and libbuteosyncfw; sign-in through signond",
    ),
    e(
        "Sailfish.Policy",
        Status::Supported,
        "keel/platform: PolicyValue and AccessPolicy reading the MDM policy store, read only and fail-closed",
    ),
    e(
        "Amber.Mpris",
        Status::Supported,
        "keel/platform: amber-mpris built for Qt 6 (MprisPlayer, MprisController), or Chum's amber-qml-plugin-mpris-qt6",
    ),
    e(
        "Amber.Web.Authorization",
        Status::Supported,
        "keel/platform: amber-web-authorization built for Qt 6 (OAuth 1.0a / 2.0, RedirectListener)",
    ),
];

/// Public Silica types: the 103 rows of keel/silica/COMPATIBILITY.md
/// (implemented -> Supported, partial -> Partial, missing -> Missing), which
/// come from the Sailfish Silica reference ("All Sailfish Silica types") plus
/// the enum and singleton names its pages document. A test keeps the two in
/// step.
pub static SILICA_TYPES: &[Entry] = &[
    e("AddAnimation", Status::Supported, "Keel 0.1"),
    e("ApplicationWindow", Status::Supported, "Keel 0.1"),
    e("BackgroundItem", Status::Supported, "Keel 0.1"),
    e("BusyIndicator", Status::Supported, "Keel 0.1"),
    e("BusyIndicatorSize", Status::Supported, "Keel 0.1"),
    e("BusyLabel", Status::Supported, "Keel 0.1"),
    e("Button", Status::Supported, "Keel 0.1"),
    e("ButtonLayout", Status::Supported, "Keel 0.1"),
    e("Clipboard", Status::Supported, "Keel 0.1"),
    e("ColorPicker", Status::Supported, "Keel 0.1"),
    e("ColorPickerDialog", Status::Supported, "Keel 0.1"),
    e("ColorPickerPage", Status::Supported, "Keel 0.1"),
    e("ColumnView", Status::Supported, "Keel 0.1"),
    e("ComboBox", Status::Supported, "Keel 0.1"),
    e("ContextMenu", Status::Supported, "Keel 0.1"),
    e("Cover", Status::Supported, "Keel 0.1"),
    e("CoverAction", Status::Supported, "Keel 0.1"),
    e("CoverActionList", Status::Supported, "Keel 0.1"),
    e("CoverBackground", Status::Supported, "Keel 0.1"),
    e("CoverPlaceholder", Status::Supported, "Keel 0.1"),
    e("CutoutMode", Status::Supported, "Keel 0.1"),
    e("DatePicker", Status::Supported, "Keel 0.1"),
    e("DatePickerDialog", Status::Supported, "Keel 0.1"),
    e("DetailItem", Status::Supported, "Keel 0.1"),
    e("Dialog", Status::Supported, "Keel 0.1"),
    e("DialogHeader", Status::Supported, "Keel 0.1"),
    e("DialogResult", Status::Supported, "Keel 0.1"),
    e("DialogStatus", Status::Supported, "Keel 0.1"),
    e("DockedPanel", Status::Supported, "Keel 0.1"),
    e("Drawer", Status::Supported, "Keel 0.1"),
    e("EnterKey", Status::Supported, "Keel 0.1"),
    e("ExpandingSection", Status::Supported, "Keel 0.1"),
    e("ExpandingSectionGroup", Status::Supported, "Keel 0.1"),
    e("FadeAnimation", Status::Supported, "Keel 0.1"),
    e("FadeAnimator", Status::Supported, "Keel 0.1"),
    e("FirstTimeUseCounter", Status::Supported, "Keel 0.1"),
    e("Format", Status::Supported, "Keel 0.1"),
    e("GlassItem", Status::Supported, "Keel 0.1"),
    e("GridItem", Status::Supported, "Keel 0.1"),
    e("HighlightImage", Status::Supported, "Keel 0.1"),
    e("HorizontalScrollDecorator", Status::Supported, "Keel 0.1"),
    e("Icon", Status::Supported, "Keel 0.1"),
    e("IconButton", Status::Supported, "Keel 0.1"),
    e("IconTextSwitch", Status::Supported, "Keel 0.1"),
    e("InfoLabel", Status::Supported, "Keel 0.1"),
    e("InteractionHintLabel", Status::Supported, "Keel 0.1"),
    e("Keypad", Status::Supported, "Keel 0.1"),
    e("Label", Status::Supported, "Keel 0.1"),
    e("LinkedLabel", Status::Supported, "Keel 0.1"),
    e("ListItem", Status::Supported, "Keel 0.1"),
    e("MenuItem", Status::Supported, "Keel 0.1"),
    e("MenuLabel", Status::Supported, "Keel 0.1"),
    e("Notice", Status::Supported, "Keel 0.1"),
    e("Notices", Status::Supported, "Keel 0.1"),
    e("OpacityRampEffect", Status::Supported, "Keel 0.1"),
    e("Orientation", Status::Supported, "Keel 0.1"),
    e("Page", Status::Supported, "Keel 0.1"),
    e("PageBusyIndicator", Status::Supported, "Keel 0.1"),
    e("PageHeader", Status::Supported, "Keel 0.1"),
    e("PageNavigation", Status::Supported, "Keel 0.1"),
    e("PageStack", Status::Supported, "Keel 0.1"),
    e("PageStackAction", Status::Supported, "Keel 0.1"),
    e("PageStatus", Status::Supported, "Keel 0.1"),
    e("PagedView", Status::Supported, "Keel 0.1: Keel's own PagedView (documented members, attached properties, drags, wrapping, cache)"),
    e("Palette", Status::Supported, "Keel 0.1: the palette of Silica items, and an app's own Palette (a Palette declared by the app derives from the items' palette type)"),
    e("PasswordField", Status::Supported, "Keel 0.1"),
    e("ProgressBar", Status::Supported, "Keel 0.1"),
    e("ProgressCircle", Status::Supported, "Keel 0.1"),
    e("PullDownMenu", Status::Supported, "Keel 0.1"),
    e("PushUpMenu", Status::Supported, "Keel 0.1"),
    e("Remorse", Status::Supported, "Keel 0.1"),
    e("RemorseItem", Status::Supported, "Keel 0.1"),
    e("RemorsePopup", Status::Supported, "Keel 0.1"),
    e("RemoveAnimation", Status::Supported, "Keel 0.1"),
    e("Screen", Status::Supported, "Keel 0.1: Silica's members plus Qt Quick's Screen members; an unqualified Screen is Silica's under import QtQuick 2.0-2.14 (keel/qt5compat shims); elsewhere write S.Screen for Silica-only members (see the Screen.* rows)"),
    e("ScrollDecorator", Status::Supported, "Keel 0.1"),
    e("SearchField", Status::Supported, "Keel 0.1"),
    e("SectionHeader", Status::Supported, "Keel 0.1"),
    e("Separator", Status::Supported, "Keel 0.1"),
    e("SilicaControl", Status::Supported, "Keel 0.1"),
    e("SilicaFlickable", Status::Supported, "Keel 0.1"),
    e("SilicaGridView", Status::Supported, "Keel 0.1"),
    e("SilicaItem", Status::Supported, "Keel 0.1"),
    e("SilicaListView", Status::Supported, "Keel 0.1"),
    e(
        "SilicaWebView",
        Status::Supported,
        "Keel 0.1: Silica's members and QtWebKit's WebView API on Keel's Sailfish.WebView (shipwright-keel-platform-webview); the page scrolls in the web engine, pulley menus open at the page's ends",
    ),
    e("SlideshowView", Status::Supported, "Keel 0.1"),
    e("Slider", Status::Supported, "Keel 0.1"),
    e("StandardPaths", Status::Supported, "Keel 0.1"),
    e("Switch", Status::Supported, "Keel 0.1"),
    e("TapInteractionHint", Status::Supported, "Keel 0.1"),
    e("TextArea", Status::Supported, "Keel 0.1"),
    e("TextField", Status::Supported, "Keel 0.1"),
    e("TextSwitch", Status::Supported, "Keel 0.1"),
    e("Theme", Status::Supported, "Keel 0.1"),
    e("TimePicker", Status::Supported, "Keel 0.1"),
    e("TimePickerDialog", Status::Supported, "Keel 0.1"),
    e("TouchBlocker", Status::Supported, "Keel 0.1"),
    e("TouchInteraction", Status::Supported, "Keel 0.1"),
    e("TouchInteractionHint", Status::Supported, "Keel 0.1"),
    e("TruncationMode", Status::Supported, "Keel 0.1"),
    e("ValueButton", Status::Supported, "Keel 0.1"),
    e("VerticalScrollDecorator", Status::Supported, "Keel 0.1"),
    e("ViewPlaceholder", Status::Supported, "Keel 0.1"),
    // Silica-only `Screen` members used through an unqualified `Screen`.
    // The scanner records these as `Screen.<member>` only when `Screen` is Qt
    // Quick's in the file: QtQuick without the 2.x shims (no version, 2.15,
    // 6.x) or QtQuick.Window is imported before Sailfish.Silica.
    e("Screen.sizeCategory", Status::Missing, SCREEN_SHADOWED),
    e("Screen.widthRatio", Status::Missing, SCREEN_SHADOWED),
    e("Screen.topCutout", Status::Missing, SCREEN_SHADOWED),
    e("Screen.Small", Status::Missing, SCREEN_SHADOWED),
    e("Screen.Medium", Status::Missing, SCREEN_SHADOWED),
    e("Screen.Large", Status::Missing, SCREEN_SHADOWED),
    e("Screen.ExtraLarge", Status::Missing, SCREEN_SHADOWED),
];

/// Members of Silica's `Screen` that Qt Quick's `Screen` lacks.
pub const SCREEN_SILICA_MEMBERS: &[&str] = &[
    "sizeCategory",
    "widthRatio",
    "topCutout",
    "Small",
    "Medium",
    "Large",
    "ExtraLarge",
];

const SCREEN_SHADOWED: &str =
    "On Qt 6 an unqualified Screen is Qt Quick's when QtQuick (without a version, 2.15 or 6.x) or \
     QtQuick.Window is imported before Sailfish.Silica (QtQuick 2.0-2.14 goes through keel/qt5compat's \
     shims, which make it Silica's), so this Silica-only member is undefined. Source change: import \
     Sailfish.Silica 1.0 as S and write S.Screen.<member> (moving the Silica import before QtQuick \
     fixes Screen but lets QtQuick's names win over Silica's components)";

pub static SAILFISHAPP_APIS: &[Entry] = &[
    e("SailfishApp::main", Status::Supported, "keel/sailfishapp"),
    e("SailfishApp::pathTo", Status::Supported, "keel/sailfishapp"),
    e(
        "SailfishApp::application",
        Status::Supported,
        "keel/sailfishapp",
    ),
    e(
        "SailfishApp::createView",
        Status::Supported,
        "keel/sailfishapp",
    ),
    e(
        "SailfishApp::pathToMainQml",
        Status::Supported,
        "keel/sailfishapp",
    ),
];

/// Sailfish platform build integration: qmake `CONFIG` features, pkg-config
/// modules (from qmake `PKGCONFIG` or CMake) and native headers. Reported for
/// tier D planning but not scored; unlisted pkg-config modules are ignored.
pub static BUILD: &[Entry] = &[
    e(
        "CONFIG sailfishapp",
        Status::Missing,
        "qmake feature from libsailfishapp; not provided by Keel, use PKGCONFIG += sailfishapp",
    ),
    e(
        "CONFIG sailfishapp_qml",
        Status::Missing,
        "pure-QML app run by sailfish-qml; Keel needs a Qt6 launcher",
    ),
    e(
        "CONFIG sailfishapp_i18n",
        Status::Missing,
        "qmake translation helper",
    ),
    e(
        "CONFIG sailfishapp_i18n_idbased",
        Status::Missing,
        "qmake translation helper",
    ),
    e(
        "CONFIG sailfishapp_i18n_unfinished",
        Status::Missing,
        "qmake translation helper",
    ),
    e(
        "CONFIG sailfishapp_i18n_include_obsolete",
        Status::Missing,
        "qmake translation helper",
    ),
    e(
        "CONFIG sailfishapp_nodata",
        Status::Missing,
        "qmake install option",
    ),
    e(
        "CONFIG sailfishapp_no_deploy_qml",
        Status::Missing,
        "qmake install option",
    ),
    e(
        "pkg-config sailfishapp",
        Status::Supported,
        "keel/sailfishapp (sailfishapp.pc and keel-sailfishapp.pc)",
    ),
    e("header sailfishapp", Status::Supported, "keel/sailfishapp"),
    e(
        "pkg-config mlite5",
        Status::Missing,
        "MGConfItem, MDesktopEntry; Qt5 only, not in Keel v1",
    ),
    e(
        "header mlite5",
        Status::Missing,
        "MGConfItem, MDesktopEntry; Qt5 only, not in Keel v1",
    ),
    e(
        "pkg-config keepalive",
        Status::Missing,
        "C++ KeepAlive; keel/nemo-compat provides the QML module only",
    ),
    e(
        "header keepalive",
        Status::Missing,
        "C++ KeepAlive; keel/nemo-compat provides the QML module only",
    ),
    e(
        "pkg-config nemonotifications-qt5",
        Status::Missing,
        "C++ Notification; keel/nemo-compat provides the QML module only",
    ),
    e(
        "header nemonotifications-qt5",
        Status::Missing,
        "C++ Notification; keel/nemo-compat provides the QML module only",
    ),
    e(
        "pkg-config sailfishsilica",
        Status::Missing,
        "Silica C++ API; not in Keel v1",
    ),
    e(
        "header sailfishsilica",
        Status::Missing,
        "Silica C++ API; not in Keel v1",
    ),
    e(
        "pkg-config qt5-boostable",
        Status::Missing,
        "mapplauncherd booster; Qt5 only",
    ),
    e(
        "pkg-config nemodbus",
        Status::Missing,
        "C++ Nemo DBus; not in Keel v1",
    ),
    e(
        "pkg-config sailfishsecrets",
        Status::Missing,
        "Qt 5 library; Keel ships the Qt 6 build as pkg-config sailfishsecrets-qt6 (keel/platform)",
    ),
    e(
        "pkg-config sailfishcrypto",
        Status::Missing,
        "Qt 5 library; Keel ships the Qt 6 build as pkg-config sailfishcrypto-qt6 (keel/platform)",
    ),
    e(
        "pkg-config accounts-qt5",
        Status::Missing,
        "Qt 5 library; Keel ships libaccounts-qt 1.17 for Qt 6 as pkg-config accounts-qt6 (keel/platform)",
    ),
    e(
        "pkg-config amberwebauthorization",
        Status::Missing,
        "Keel ships Amber.Web.Authorization only as a QML plugin (keel/platform), no C++ library",
    ),
    e(
        "pkg-config ambermpris",
        Status::Missing,
        "Qt 5 library; Keel ships Amber.Mpris only as a QML plugin (keel/platform); Chum's amber-mpris-qt6-devel has the Qt 6 library (pkg-config ambermpris6)",
    ),
    e(
        "pkg-config nemothumbnailer-qt5",
        Status::Missing,
        "Qt 5 library; Keel ships Nemo.Thumbnailer only as a QML plugin (keel/platform)",
    ),
    e(
        "pkg-config mce-qt5",
        Status::Missing,
        "Qt 5 library; Keel ships Nemo.Mce only as a QML plugin (keel/platform)",
    ),
    e(
        "pkg-config sailfishaccounts",
        Status::Missing,
        "Qt 5 library; Keel ships Sailfish.Accounts only as a QML plugin (keel/platform)",
    ),
    e(
        "pkg-config libsignon-qt5",
        Status::Missing,
        "Qt 5 library; Keel builds libsignon-qt for Qt 6 inside Sailfish.Accounts only (keel/platform)",
    ),
];

/// Maps a native `#include` target to a catalogued platform library.
pub fn library_for_header(header: &str) -> Option<&'static str> {
    let file = header.rsplit('/').next().unwrap_or(header);
    if file == "sailfishapp.h" {
        Some("sailfishapp")
    } else if header.starts_with("mlite5/")
        || matches!(
            file,
            "MGConfItem" | "MDesktopEntry" | "mgconfitem.h" | "mdesktopentry.h"
        )
    {
        Some("mlite5")
    } else if header.starts_with("keepalive/") {
        Some("keepalive")
    } else if header.starts_with("nemonotifications-qt5/") {
        Some("nemonotifications-qt5")
    } else if header.starts_with("silica")
        || header.starts_with("Silica")
        || header.starts_with("sailfishsilica/")
    {
        Some("sailfishsilica")
    } else {
        None
    }
}

pub fn lookup(table: &'static [Entry], name: &str) -> Option<&'static Entry> {
    table.iter().find(|entry| entry.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMPATIBILITY_MD: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../keel/silica/COMPATIBILITY.md"
    ));

    /// `| Name | status | notes |` rows of the Types table.
    fn compatibility_rows() -> Vec<(&'static str, &'static str)> {
        COMPATIBILITY_MD
            .lines()
            .filter_map(|line| {
                let cells: Vec<&str> = line.split('|').map(str::trim).collect();
                match cells.as_slice() {
                    ["", name, status @ ("implemented" | "partial" | "missing"), ..]
                        if name.chars().all(|c| c.is_ascii_alphabetic()) =>
                    {
                        Some((*name, *status))
                    }
                    _ => None,
                }
            })
            .collect()
    }

    #[test]
    fn silica_types_follow_keel_compatibility_md() {
        let rows = compatibility_rows();
        assert_eq!(rows.len(), 103, "COMPATIBILITY.md type rows");
        for (name, status) in &rows {
            let expected = match *status {
                "implemented" => Status::Supported,
                "partial" => Status::Partial,
                _ => Status::Missing,
            };
            let entry = lookup(SILICA_TYPES, name)
                .unwrap_or_else(|| panic!("{name} is in COMPATIBILITY.md but not catalogued"));
            assert_eq!(
                entry.status, expected,
                "{name}: COMPATIBILITY.md says {status}; update catalog.rs"
            );
        }
        // Every undotted catalogue entry is a COMPATIBILITY.md row.
        for entry in SILICA_TYPES.iter().filter(|e| !e.name.contains('.')) {
            assert!(
                rows.iter().any(|(name, _)| *name == entry.name),
                "{} is catalogued but not in COMPATIBILITY.md",
                entry.name
            );
        }
        let count = |s| {
            SILICA_TYPES
                .iter()
                .filter(|e| !e.name.contains('.') && e.status == s)
                .count()
        };
        assert_eq!(
            (
                count(Status::Supported),
                count(Status::Partial),
                count(Status::Missing)
            ),
            (103, 0, 0)
        );
    }

    /// `| Module | status | notes |` rows of the "Platform modules" table.
    fn compatibility_module_rows() -> Vec<(&'static str, &'static str)> {
        let section = COMPATIBILITY_MD
            .split("## Platform modules")
            .nth(1)
            .expect("COMPATIBILITY.md has a Platform modules section");
        let section = section.split("\n## ").next().unwrap_or(section);
        section
            .lines()
            .filter_map(|line| {
                let cells: Vec<&str> = line.split('|').map(str::trim).collect();
                match cells.as_slice() {
                    ["", name, status @ ("implemented" | "partial" | "missing"), ..]
                        if name.contains('.') =>
                    {
                        Some((*name, *status))
                    }
                    _ => None,
                }
            })
            .collect()
    }

    fn is_platform_module(name: &str) -> bool {
        name.starts_with("Nemo.")
            || name.starts_with("org.nemomobile.")
            || name.starts_with("Amber.")
            || (name.starts_with("Sailfish.") && !name.starts_with(SILICA_MODULE))
    }

    #[test]
    fn platform_modules_follow_keel_compatibility_md() {
        let rows = compatibility_module_rows();
        assert_eq!(rows.len(), 31, "COMPATIBILITY.md platform module rows");
        for (name, status) in &rows {
            let expected = match *status {
                "implemented" => Status::Supported,
                "partial" => Status::Partial,
                _ => Status::Missing,
            };
            let entry = lookup(MODULES, name)
                .unwrap_or_else(|| panic!("{name} is in COMPATIBILITY.md but not catalogued"));
            assert_eq!(
                entry.status, expected,
                "{name}: COMPATIBILITY.md says {status}; update catalog.rs"
            );
        }
        for entry in MODULES.iter().filter(|e| is_platform_module(e.name)) {
            assert!(
                rows.iter().any(|(name, _)| *name == entry.name),
                "{} is catalogued but not in COMPATIBILITY.md",
                entry.name
            );
        }
        let count = |s| {
            MODULES
                .iter()
                .filter(|e| is_platform_module(e.name) && e.status == s)
                .count()
        };
        assert_eq!(
            (
                count(Status::Supported),
                count(Status::Partial),
                count(Status::Missing)
            ),
            (31, 0, 0)
        );
    }

    #[test]
    fn partial_entries_say_why() {
        for entry in SILICA_TYPES.iter().filter(|e| e.status == Status::Partial) {
            assert!(
                entry.note.len() > "Keel 0.1, partial: ".len(),
                "{} has no reason",
                entry.name
            );
        }
        assert!(Status::Partial.reachable());
        assert!(Status::Partial.scored());
    }

    #[test]
    fn nemo_modules_and_sailfishapp_are_supported() {
        for name in [
            "Sailfish.Silica",
            "Nemo.DBus",
            "Nemo.Notifications",
            "Nemo.KeepAlive",
            "org.nemomobile.dbus",
            "org.nemomobile.notifications",
            "org.nemomobile.keepalive",
            "Nemo.Configuration",
            "org.nemomobile.configuration",
            "Nemo.Ngf",
            "org.nemomobile.ngf",
            "Sailfish.Secrets",
            "Sailfish.Crypto",
            "Sailfish.Pickers",
            "org.nemomobile.accounts",
            "org.nemomobile.mpris",
            "Amber.Mpris",
            "Amber.Web.Authorization",
            "Nemo.Thumbnailer",
            "Nemo.Policy",
            "org.nemomobile.policy",
            "Nemo.Mce",
            "Sailfish.Share",
            "Sailfish.Accounts",
            "Sailfish.Media",
            "org.nemomobile.lipstick",
        ] {
            assert_eq!(
                lookup(MODULES, name).unwrap().status,
                Status::Supported,
                "{name}"
            );
        }
        assert!(SAILFISHAPP_APIS
            .iter()
            .all(|e| e.status == Status::Supported));
        assert_eq!(SAILFISHAPP_APIS.len(), 5);
    }

    #[test]
    fn every_silica_only_screen_member_is_a_blocker_entry() {
        for member in SCREEN_SILICA_MEMBERS {
            let entry = lookup(SILICA_TYPES, &format!("Screen.{member}")).unwrap();
            assert_eq!(entry.status, Status::Missing);
            assert!(entry.note.contains("S.Screen"));
        }
    }
}
