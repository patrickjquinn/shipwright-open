// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

.pragma library

// Untrusted text (entry titles, groups, the vault name: all can come from
// an imported file) for a property Keel renders as Text.AutoText without a
// way to switch it to PlainText: PageHeader's title and description. AutoText
// turns a string with a tag in it into StyledText, and StyledText fetches
// `<img src="https://...">` from the network. So `<`, `>` and `&` are
// escaped when the string has a `<`; a string without one cannot form a tag
// and is returned unchanged. Line breaks become spaces (one-line headers).
// Drop this once Keel's PageHeader takes a text format.
function escape(text) {
    var s = (text === undefined || text === null) ? "" : String(text)
    s = s.replace(/[\r\n]+/g, " ")
    if (s.indexOf("<") < 0)
        return s
    return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;")
}
