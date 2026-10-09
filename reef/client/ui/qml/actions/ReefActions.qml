// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

// Reef's Keel Actions (ADR-0018): search the store and install an app.
// Installing always asks the person first (confirm), downloads from the
// Reef repository (open world) and answers when the installation finished.

import QtQuick
import Keel.Actions 1.0
import Shipwright.Reef 1.0

Item {
    id: actions

    property var pendingInstall: null
    property string pendingName: ""

    function rows() {
        // get() answers {} past the last row.
        const model = Reef.catalogueModel
        const out = []
        for (let i = 0; i < 10000; ++i) {
            const row = model.get(i)
            if (!row.name)
                break
            out.push(row)
        }
        return out
    }

    function summary(row) {
        return { id: row.name, title: row.title || row.name, summary: row.summary || "",
                 category: row.category || "", version: row.version || "",
                 installed: row.installState === "installed" }
    }

    function matches(query) {
        const q = query.toLowerCase()
        return rows().filter(r => (r.name + " " + (r.title || "") + " " + (r.summary || "") + " " + (r.category || ""))
                             .toLowerCase().indexOf(q) >= 0)
    }

    Connections {
        target: Reef
        function onBusyChanged() {
            if (Reef.busy || !actions.pendingInstall)
                return
            const call = actions.pendingInstall
            const details = Reef.packageDetails(actions.pendingName)
            actions.pendingInstall = null
            if (details.installState === "installed")
                call.reply({ installed: true, version: details.installedVersion || details.version || "" })
            else
                call.fail("Failed", Reef.lastError || "the installation did not finish")
        }
    }

    KeelEntity {
        id: appEntity
        type: "app"
        title: "Store app"
        description: "An app in the Reef store: its name, summary, category, version and whether it is installed."
        KeelParam { name: "summary"; type: "string"; maxLength: 200; summarisable: true; indexable: true }
        KeelParam { name: "category"; type: "string"; maxLength: 64; summarisable: true; indexable: true }
        KeelParam { name: "version"; type: "string"; maxLength: 64; summarisable: true }
        KeelParam { name: "installed"; type: "boolean"; summarisable: true }
        onResolve: (id, call) => {
            const row = actions.rows().find(r => r.name === id)
            call.reply(row ? actions.summary(row) : null)
        }
        onFind: (query, limit, call) => call.reply(actions.matches(query).slice(0, limit).map(actions.summary))
    }

    KeelAction {
        name: "reef.search"
        title: qsTr("Search the store")
        description: "Search the Reef store's catalogue by name, summary or category. Returns matching apps and whether each is installed."
        readOnly: true
        idempotent: true
        parameters: [ KeelParam { name: "query"; type: "string"; required: true; maxLength: 256 } ]
        returns: KeelResult {
            type: "array"; itemType: "object"; maxItems: 50
            KeelParam { name: "app"; type: "entity"; entity: "app"; required: true }
            KeelParam { name: "title"; type: "string"; maxLength: 256; required: true }
            KeelParam { name: "summary"; type: "string"; maxLength: 200; required: true }
            KeelParam { name: "installed"; type: "boolean"; required: true }
        }
        onInvoked: (args, call) => call.reply(actions.matches(args.query).slice(0, 50).map(r => {
            const s = actions.summary(r)
            return { app: appEntity.uri(s.id), title: s.title, summary: s.summary, installed: s.installed }
        }))
    }

    KeelAction {
        name: "reef.install"
        title: qsTr("Install an app")
        description: "Install an app from the Reef store on this phone. The person confirms first; the package is downloaded from the Reef repository."
        confirm: true
        openWorld: true
        idempotent: true
        timeout: 600000
        parameters: [ KeelParam { name: "app"; type: "entity"; entity: "app"; required: true } ]
        returns: KeelResult {
            type: "object"
            KeelParam { name: "installed"; type: "boolean"; required: true }
            KeelParam { name: "version"; type: "string"; maxLength: 64; required: true }
        }
        onInvoked: (args, call) => {
            const name = args.app.substring(args.app.lastIndexOf("/") + 1)
            const details = Reef.packageDetails(name)
            if (!details.name) {
                call.fail("Failed", "no app " + name + " in the store")
                return
            }
            if (details.installState === "installed") {
                call.reply({ installed: true, version: details.installedVersion || details.version || "" })
                return
            }
            if (Reef.busy || actions.pendingInstall) {
                call.fail("NotAvailable", "Reef is busy with another package")
                return
            }
            actions.pendingInstall = call
            actions.pendingName = name
            Reef.install(name)
        }
    }
}
