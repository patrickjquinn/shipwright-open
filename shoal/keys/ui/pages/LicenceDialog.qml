// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

// Paste a Shoal Keys licence token from Reef. The settings page below
// installs `token` when the dialog is accepted and reports the result.

import QtQuick
import Sailfish.Silica 1.0

Dialog {
    id: dialog
    objectName: "licenceDialog"

    readonly property string token: tokenField.text.trim()

    canAccept: token.length > 0

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column
            width: parent.width

            DialogHeader {
                acceptText: qsTr("Add")
                title: qsTr("Add licence")
            }

            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                bottomPadding: Theme.paddingLarge
                wrapMode: Text.Wrap
                font.pixelSize: Theme.fontSizeSmall
                color: Theme.secondaryHighlightColor
                text: qsTr("Buy Shoal Keys in Reef, then copy the licence from Reef's Licences page and paste it here. Keys checks it on this phone and sends nothing.")
            }

            TextArea {
                id: tokenField
                objectName: "tokenField"
                width: parent.width
                label: qsTr("Licence")
                placeholderText: qsTr("Paste your licence")
                inputMethodHints: Qt.ImhNoAutoUppercase | Qt.ImhNoPredictiveText
            }
        }
        VerticalScrollDecorator { }
    }
}
