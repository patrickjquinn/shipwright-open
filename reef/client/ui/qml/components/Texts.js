// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

// Text and link helpers shared by the pages.
//
// The Rust side sends keys, not English, for everything it says itself
// (reef/client/app/src/rows.rs: status_text and message), so the text is
// translated here with qsTr. Errors that come from PackageKit, the network
// or the licence library are shown as they are.
//
// Links in the catalogue are developer-controlled: only https:// URLs are
// ever opened.

.pragma library

// busyText: "checking", "downloading", ...
function busyText(code) {
    switch (code) {
    case "": return ""
    case "checking": return qsTr("Checking")
    case "starting": return qsTr("Starting")
    case "refreshing": return qsTr("Refreshing")
    case "repairing": return qsTr("Repairing")
    case "refreshing_licences": return qsTr("Refreshing licences")
    case "waiting": return qsTr("Waiting")
    case "running": return qsTr("Running")
    case "querying": return qsTr("Querying")
    case "removing": return qsTr("Removing")
    case "downloading": return qsTr("Downloading")
    case "installing": return qsTr("Installing")
    case "updating": return qsTr("Updating")
    case "cleaning_up": return qsTr("Cleaning up")
    case "resolving": return qsTr("Resolving dependencies")
    case "checking_signatures": return qsTr("Checking signatures")
    case "committing": return qsTr("Committing")
    case "finished": return qsTr("Finished")
    default: return qsTr("Working")
    }
}

// busyText with the percentage when PackageKit reports one.
function progressText(code, progress) {
    var text = busyText(code)
    return progress >= 0 ? qsTr("%1 %2%").arg(text).arg(progress) : text
}

function messageLine(line) {
    if (line.charAt(0) !== "\u001e")
        return line
    var p = line.substring(1).split("\u001f")
    switch (p[0]) {
    case "busy": return qsTr("Another operation is running.")
    case "no_release": return qsTr("Cannot determine the installed Sailfish OS release.")
    case "no_licence_store": return qsTr("No place to store licences (HOME is not set).")
    case "licence_remove_failed": return qsTr("Could not remove the licence: %1").arg(p[1])
    case "catalogue_missing":
        return qsTr("Reef has no catalogue for Sailfish OS %1 (%2).").arg(p[1]).arg(p[2])
    case "catalogue_download_failed": return qsTr("Could not download the catalogue: %1").arg(p[1])
    case "no_licence_server": return qsTr("No licence service is configured.")
    case "revocations_unsigned": return qsTr("The list of refunded licences is not signed.")
    case "updated": return qsTr("%n package(s) updated", "", parseInt(p[1], 10))
    case "up_to_date": return qsTr("%1 is up to date").arg(p[1])
    case "licence_add_failed": return qsTr("The licence was not added: %1").arg(p[1])
    case "claim_failed": return qsTr("Could not start the purchase: %1").arg(p[1])
    case "worker_failed": return qsTr("Could not start the background work: %1").arg(p[1])
    default: return p.join(" ")
    }
}

// lastError and operationFinished messages; lastError may hold several
// lines.
function message(text) {
    if (!text)
        return ""
    return text.split("\n").map(messageLine).join("\n")
}

// An absolute https:// URL with a host and no whitespace or control
// characters (the rule reef-backend's catalogue parser applies too).
function isHttps(url) {
    return typeof url === "string" && url.length <= 2048
            && /^https:\/\/[^\/]/.test(url) && !/[\s\u0000-\u001f\u007f]/.test(url)
}

function openHttps(url) {
    if (isHttps(url))
        Qt.openUrlExternally(url)
}

// A plain https:// URL for the licence service setting.
function isServiceUrl(url) {
    return isHttps(url) && url.indexOf("?") < 0 && url.indexOf("#") < 0
}
