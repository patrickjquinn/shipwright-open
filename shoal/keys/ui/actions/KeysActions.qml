// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

// Shoal Keys' Keel Actions (ADR-0018): what Pilot and other MCP clients may
// do with the vault. Everything is read-only and works only while the vault
// is unlocked. No secret ever leaves the app: search results are marked
// sensitive (shown to the person, never given to a model), and copying puts
// a secret on the clipboard, which is cleared as configured, and answers
// only which field was copied.

import QtQuick
import Keel.Actions 1.0
import Shipwright.Keys 1.0

Item {
    id: actions

    function locked(call) {
        if (ShoalKeys.locked) {
            call.fail("NotAvailable", "Shoal Keys is locked: open it and unlock the vault first")
            return true
        }
        return false
    }

    function idOf(uri) {
        return uri.substring(uri.lastIndexOf("/") + 1)
    }

    function summary(row) {
        return { id: row.id, title: row.title, host: row.host || "", group: row.group || "" }
    }

    KeelEntity {
        id: entryEntity
        type: "entry"
        title: "Password entry"
        description: "A password manager entry: a title, a user name and the site it is for. Secrets are never part of it."
        KeelParam { name: "host"; type: "string"; maxLength: 253; summarisable: true; indexable: true; description: "Site the entry is for" }
        KeelParam { name: "group"; type: "string"; maxLength: 256; summarisable: true; description: "Folder of the entry" }
        KeelParam { name: "username"; type: "string"; maxLength: 1024; description: "User name (not given to models)" }
        onResolve: (id, call) => {
            if (actions.locked(call))
                return
            const e = JSON.parse(ShoalKeys.entry(id))
            if (e.id === undefined) {
                call.reply(null)
                return
            }
            const url = (e.urls || [])[0] || ""
            const host = (/^[a-z][a-z0-9+.-]*:\/\/([^/:?#]+)/i.exec(url) || ["", ""])[1]
            call.reply({ id: e.id, title: e.title || "(untitled)", host: host, group: e.group || "", username: e.username || "" })
        }
        onFind: (query, limit, call) => {
            if (actions.locked(call))
                return
            call.reply(JSON.parse(ShoalKeys.search(query)).slice(0, limit).map(actions.summary))
        }
    }

    KeelAction {
        name: "keys.search"
        title: qsTr("Search passwords")
        description: "Search the password vault by title, user name or site. The matches are shown to the person on the device; they are never given to a model."
        readOnly: true
        idempotent: true
        sensitive: true
        parameters: [
            KeelParam { name: "query"; type: "string"; required: true; maxLength: 256; description: "Text to look for" }
        ]
        returns: KeelResult {
            type: "array"; itemType: "object"; maxItems: 50
            KeelParam { name: "entry"; type: "entity"; entity: "entry"; required: true }
            KeelParam { name: "title"; type: "string"; maxLength: 512; required: true }
            KeelParam { name: "username"; type: "string"; maxLength: 1024 }
            KeelParam { name: "host"; type: "string"; maxLength: 253 }
        }
        onInvoked: (args, call) => {
            if (actions.locked(call))
                return
            call.reply(JSON.parse(ShoalKeys.search(args.query)).slice(0, 50).map(row => ({
                entry: entryEntity.uri(row.id), title: row.title, username: row.username || "", host: row.host || ""
            })))
        }
    }

    KeelAction {
        name: "keys.copy"
        title: qsTr("Copy from an entry")
        description: "Copy an entry's password, user name or current one-time code to the clipboard, which is cleared after the delay set in Shoal Keys. Answers which field was copied, never the secret."
        readOnly: true
        sensitive: true
        parameters: [
            KeelParam { name: "entry"; type: "entity"; entity: "entry"; required: true },
            KeelParam { name: "field"; type: "string"; maxLength: 16; values: ["password", "username", "otp"]; defaultValue: "password" }
        ]
        returns: KeelResult {
            type: "object"
            KeelParam { name: "copied"; type: "string"; maxLength: 16; required: true }
            KeelParam { name: "clearsAfterSeconds"; type: "integer"; minimum: 0; maximum: 86400; required: true }
        }
        onInvoked: (args, call) => {
            if (actions.locked(call))
                return
            const id = actions.idOf(args.entry)
            const field = args.field || "password"
            const e = JSON.parse(ShoalKeys.entry(id))
            if (e.id === undefined) {
                call.fail("Failed", "no such entry")
                return
            }
            let value = field === "username" ? e.username : e.password
            if (field === "otp") {
                const code = JSON.parse(ShoalKeys.totp(id))
                if (!code.code) {
                    call.fail("Failed", "the entry has no one-time code")
                    return
                }
                value = code.code
            }
            if (!value) {
                call.fail("Failed", "the entry has no " + field)
                return
            }
            ShoalKeys.copy(value)
            call.reply({ copied: field, clearsAfterSeconds: ShoalKeys.clipboardClearSecs })
        }
    }

    KeelAction {
        name: "keys.lock"
        title: qsTr("Lock the vault")
        description: "Lock the password vault now; it then needs the master password again."
        idempotent: true
        notDestructiveBecause: "Locking keeps every entry; it only asks for the master password again."
        onInvoked: (args, call) => ShoalKeys.lock()
    }
}
