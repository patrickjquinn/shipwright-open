// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

// Harness for autofill-test.sh: a vault with one GitHub login, and a
// stand-in for the person who answers every autofill request (the first
// login for a fill, Save for a save). Prints "AUTOFILL ready" when the vault
// is set up and "AUTOFILL asked <kind> <host>" per request.

import QtQuick
import Shipwright.Keys 1.0

QtObject {
    id: h

    property Connections conn: Connections {
        target: ShoalKeys
        function onFinished(op, ok, message) {
            if (!ok) {
                console.log("AUTOFILL FAIL " + op + ": " + message)
                Qt.exit(1)
                return
            }
            if (op === "create")
                ShoalKeys.saveEntry(JSON.stringify({ title: "GitHub", username: "octocat", password: "hunter2",
                                                     urls: ["https://github.com/login"] }))
            else if (op === "save" && !h.ready) {
                h.ready = true
                console.log("AUTOFILL ready")
            }
        }
        function onAutofillRequested(request) { h.handle(request) }
    }
    function handle(request) {
            var r = JSON.parse(ShoalKeys.autofillRequest(request))
            if (r.waiting) {
                h.later(function() { h.handle(request) })
                return
            }
            console.log("AUTOFILL asked " + r.kind + " " + r.host + " " + (r.logins || []).length)
            if (r.kind === "fill" && r.host === "decline.example")
                ShoalKeys.autofillDecline(request)
            else
                h.answer(request, r.kind === "fill" && r.logins.length > 0 ? r.logins[0].id : "")
    }
    property bool ready: false

    function later(f) {
        var t = Qt.createQmlObject("import QtQuick; Timer { interval: 100 }", h)
        t.triggered.connect(function() { t.destroy(); f() })
        t.start()
    }

    // As a person would: a save still running answers "busy"; try again.
    function answer(request, id) {
        var error = ShoalKeys.autofillAnswer(request, id)
        if (error.indexOf("Busy") === 0)
            h.later(function() { h.answer(request, id) })
    }

    Component.onCompleted: ShoalKeys.create("Test", "master")
}
