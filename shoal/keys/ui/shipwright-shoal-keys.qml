// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

// Shoal Keys, on Keel (Sailfish.Silica 1.0 under Qt6). All state comes from
// the `ShoalKeys` singleton (Shipwright.Keys 1.0), a CXX-Qt object over
// shoal-keys-core; see ../README.md.

import QtQuick
import Sailfish.Silica 1.0
import Shipwright.Keys 1.0
import "actions"
import "cover"

ApplicationWindow {
    id: app

    initialPage: Qt.resolvedUrl("pages/UnlockPage.qml")
    cover: Component {
        CoverPage {
            onSearchRequested: app.showSearch()
        }
    }
    allowedOrientations: defaultAllowedOrientations

    // Cover action "Search": the app comes up on the entry list with the
    // search field focused.
    function showSearch() {
        activate()
        if (ShoalKeys.locked)
            return
        var main = pageStack.find(function(p) { return p.objectName === "mainPage" })
        if (!main)
            return
        if (pageStack.currentPage !== main)
            pageStack.pop(main, PageStackAction.Immediate)
        main.focusSearch()
    }

    // Back to the unlock page whenever the vault locks (manually, on a
    // timer, or with the device), dropping every page that showed secrets.
    Connections {
        target: ShoalKeys
        function onLockedChanged() {
            autoLockTimer.rearm()
            if (ShoalKeys.locked)
                app.pageStack.replaceAbove(null, Qt.resolvedUrl("pages/UnlockPage.qml"), {},
                                       PageStackAction.Immediate)
        }
        function onAutofillRequested(request) {
            app.pendingAutofill = app.pendingAutofill.concat([request])
            app.activate()
            app.showAutofill()
        }
        function onCopied() {
            if (ShoalKeys.clipboardClearSecs > 0) {
                clipboardTimer.interval = ShoalKeys.clipboardClearSecs * 1000
                clipboardTimer.restart()
            }
        }
    }

    onApplicationActiveChanged: {
        ShoalKeys.setActive(applicationActive)
        autoLockTimer.rearm()
    }

    // Requests from other apps (autofill) waiting to be shown: after an
    // unlock if the vault is locked, one at a time.
    property var pendingAutofill: []
    function showAutofill() {
        if (ShoalKeys.locked || pendingAutofill.length === 0 || pageStack.busy)
            return
        if (pageStack.currentPage && pageStack.currentPage.objectName === "autofillPage")
            return
        // Requests that timed out or were declined meanwhile are skipped.
        var waiting = JSON.parse(ShoalKeys.autofillPending())
        var left = pendingAutofill.filter(function (r) { return waiting.indexOf(r) >= 0 })
        if (left.length === 0) {
            pendingAutofill = []
            return
        }
        var next = left[0]
        pendingAutofill = left.slice(1)
        pageStack.push(Qt.resolvedUrl("pages/AutofillPage.qml"), { request: next })
    }
    Timer {
        // After an unlock the main page replaces the unlock page; show the
        // request once that settles, and the next after each answer.
        interval: 300
        repeat: true
        running: app.pendingAutofill.length > 0
        onTriggered: app.showAutofill()
    }

    // Keel Actions for Pilot and other MCP clients (actions/KeysActions.qml).
    KeysActions {}

    // Clears a copied secret after the configured delay, only if the
    // clipboard still holds it.
    Timer {
        id: clipboardTimer
        repeat: false
        onTriggered: ShoalKeys.clearClipboard()
    }

    // Auto-lock clock: set for when the next timed lock is due, not a
    // once-a-second poll. The core measures time with CLOCK_BOOTTIME, so time
    // spent suspended counts; this timer does not run then, so it waits at
    // most 30 s at a time and a lock that fell due while suspended follows
    // soon after the phone wakes. Input only pushes the lock later, so it
    // needs no new timer; unlocking, the app's state and settings do.
    Timer {
        id: autoLockTimer
        repeat: false
        function rearm() {
            var ms = ShoalKeys.locked ? -1 : ShoalKeys.msUntilAutoLock()
            if (ms < 0) {
                stop()
                return
            }
            interval = Math.max(250, Math.min(ms, 30000))
            restart()
        }
        onTriggered: {
            if (!ShoalKeys.checkAutoLock())
                rearm()
        }
    }

    // Any touch counts as activity; the press still reaches the page.
    MouseArea {
        anchors.fill: parent
        z: 10000
        propagateComposedEvents: true
        onPressed: function(mouse) {
            ShoalKeys.touch()
            mouse.accepted = false
        }
    }
}
