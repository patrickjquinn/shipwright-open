// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

// The entry list with search. Searching matches titles, user names, hosts,
// tags, groups, notes and field names, never passwords.

import QtQuick
import Sailfish.Silica 1.0
import Shipwright.Keys 1.0
import "text.js" as Untrusted

Page {
    id: page
    objectName: "mainPage"

    property string query: ""
    // Re-read whenever the vault changes (ShoalKeys.revision) or the query does.
    // Grouped by KeePass group (entries without one last), then by title, so
    // each group's section header appears once.
    property var rows: {
        var _ = ShoalKeys.revision
        var list = JSON.parse(ShoalKeys.search(query))
        // Search results keep their relevance order.
        if (query.length > 0)
            return list
        var collator = function(a, b) { return a.localeCompare(b) }
        list.sort(function(a, b) {
            var ga = a.group || "", gb = b.group || ""
            if (ga !== gb)
                return ga === "" ? 1 : gb === "" ? -1 : collator(ga, gb)
            return collator(a.title || "", b.title || "")
        })
        return list
    }
    // Section headers only when the vault uses groups at all.
    readonly property bool grouped: query.length === 0 && rows.some(function(r) { return (r.group || "").length > 0 })

    // The header's search field (set by the header when it is created).
    property SearchField searchField: null

    function focusSearch() {
        list.positionViewAtBeginning()
        if (searchField)
            searchField.forceActiveFocus()
    }

    SilicaListView {
        id: list
        anchors.fill: parent
        model: page.rows

        PullDownMenu {
            MenuItem {
                text: qsTr("Settings")
                onClicked: pageStack.push(Qt.resolvedUrl("SettingsPage.qml"))
            }
            MenuItem {
                text: qsTr("Import and export")
                onClicked: pageStack.push(Qt.resolvedUrl("ImportExportPage.qml"))
            }
            MenuItem {
                text: qsTr("Password generator")
                onClicked: pageStack.push(Qt.resolvedUrl("GeneratorPage.qml"))
            }
            MenuItem {
                text: qsTr("Add entry")
                onClicked: pageStack.push(Qt.resolvedUrl("EditDialog.qml"))
            }
        }
        PushUpMenu {
            MenuItem {
                objectName: "healthItem"
                text: qsTr("Password health")
                visible: ShoalKeys.licensed
                onClicked: pageStack.push(Qt.resolvedUrl("HealthPage.qml"))
            }
            MenuItem {
                text: qsTr("Lock")
                onClicked: ShoalKeys.lock()
            }
        }

        header: Column {
            width: list.width
            Component.onCompleted: page.searchField = search
            PageHeader {
                title: ShoalKeys.vaultName.length ? Untrusted.escape(ShoalKeys.vaultName) : qsTr("Keys")
                description: qsTr("%n (entry|entries)", "", ShoalKeys.entryCount)
            }
            SearchField {
                id: search
                objectName: "searchField"
                width: parent.width
                placeholderText: qsTr("Search")
                inputMethodHints: Qt.ImhNoAutoUppercase | Qt.ImhNoPredictiveText
                onTextChanged: page.query = text
            }
        }

        section.property: page.grouped ? "group" : ""
        section.delegate: SectionHeader {
            required property string section
            text: section.length ? Untrusted.escape(section) : qsTr("Other")
        }

        delegate: ListItem {
            id: item
            required property var modelData
            contentHeight: Theme.itemSizeMedium
            onClicked: pageStack.push(Qt.resolvedUrl("EntryPage.qml"), { entryId: modelData.id })

            Column {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                anchors.verticalCenter: parent.verticalCenter
                Label {
                    width: parent.width
                    text: item.modelData.title
                    textFormat: Text.PlainText
                    truncationMode: TruncationMode.Fade
                    color: item.highlighted ? Theme.highlightColor : Theme.primaryColor
                }
                Label {
                    width: parent.width
                    text: [item.modelData.username, item.modelData.host].filter(function(s) { return s.length > 0 }).join(" · ")
                    textFormat: Text.PlainText
                    truncationMode: TruncationMode.Fade
                    font.pixelSize: Theme.fontSizeExtraSmall
                    color: item.highlighted ? Theme.secondaryHighlightColor : Theme.secondaryColor
                }
            }

            menu: Component {
                ContextMenu {
                    MenuItem {
                        text: qsTr("Copy user name")
                        visible: item.modelData.username.length > 0
                        onClicked: ShoalKeys.copy(item.modelData.username)
                    }
                    MenuItem {
                        text: qsTr("Copy password")
                        onClicked: ShoalKeys.copy(JSON.parse(ShoalKeys.entry(item.modelData.id)).password || "")
                    }
                    MenuItem {
                        text: qsTr("Copy one-time code")
                        visible: item.modelData.hasTotp
                        onClicked: ShoalKeys.copy(JSON.parse(ShoalKeys.totp(item.modelData.id)).code || "")
                    }
                    MenuItem {
                        text: qsTr("Delete")
                        onClicked: item.remorseDelete(function() { ShoalKeys.deleteEntry(item.modelData.id) })
                    }
                }
            }
        }

        ViewPlaceholder {
            enabled: list.count === 0
            text: page.query.length ? qsTr("No matches") : qsTr("No entries yet")
            hintText: page.query.length ? "" : qsTr("Pull down to add one or to import")
        }

        VerticalScrollDecorator { }
    }
}
