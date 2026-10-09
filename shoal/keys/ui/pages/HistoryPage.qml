// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

// Earlier passwords of an entry, newest first. Tap to copy one; the eye
// button shows it.

import QtQuick
import Sailfish.Silica 1.0
import Shipwright.Keys 1.0

Page {
    id: page
    objectName: "historyPage"
    property string entryId

    SilicaListView {
        id: list
        anchors.fill: parent
        model: {
            var _ = ShoalKeys.revision
            return JSON.parse(ShoalKeys.history(page.entryId))
        }
        header: PageHeader {
            title: qsTr("Password history")
            description: qsTr("Tap a password to copy it")
        }
        delegate: CopyItem {
            required property var modelData
            width: list.width
            label: modelData.modified ? Qt.formatDateTime(new Date(modelData.modified * 1000),
                                                                       "d MMM yyyy, " + Qt.locale().timeFormat(Locale.ShortFormat))
                                      : qsTr("Earlier version")
            property bool shown: false
            value: modelData.password
            secret: !shown
            revealable: true
            onRevealToggled: shown = !shown
        }
        ViewPlaceholder {
            enabled: list.count === 0
            text: qsTr("No earlier versions")
        }
        VerticalScrollDecorator { }
    }
}
