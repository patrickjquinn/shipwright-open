// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

// Restore access after a factory reset or on a new phone: the master
// password plus the recovery key file saved earlier from Import and export.

import QtQuick
import Sailfish.Silica 1.0
import Sailfish.Pickers 1.0
import Shipwright.Keys 1.0

Dialog {
    id: dialog
    objectName: "recoverDialog"

    // Where the recovery key was saved by default; the picker changes it.
    property string keyFile: ShoalKeys.recoveryKeyPath()

    canAccept: password.text.length > 0 && keyFile.length > 0

    onAccepted: ShoalKeys.recover(password.text, keyFile)

    Component {
        id: keyPicker
        FilePickerPage {
            title: qsTr("Recovery key file")
            nameFilters: ["*.keyx", "*.key", "*.xml"]
            onSelectedContentPropertiesChanged: {
                if (selectedContentProperties && selectedContentProperties.filePath)
                    dialog.keyFile = selectedContentProperties.filePath
            }
        }
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column
            width: parent.width

            DialogHeader {
                acceptText: qsTr("Restore")
                title: qsTr("Restore with recovery key")
            }
            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                bottomPadding: Theme.paddingLarge
                wrapMode: Text.Wrap
                font.pixelSize: Theme.fontSizeSmall
                color: Theme.secondaryHighlightColor
                text: qsTr("Opens your vault with the recovery key file you saved, and links it to this phone again.")
            }
            PasswordField {
                id: password
                width: parent.width
                label: qsTr("Master password")
                placeholderText: qsTr("Master password")
                EnterKey.iconSource: "image://theme/icon-m-enter-close"
                EnterKey.onClicked: focus = false
            }
            ValueButton {
                objectName: "keyFileButton"
                label: qsTr("Recovery key file")
                value: dialog.keyFile.length ? dialog.keyFile.substring(dialog.keyFile.lastIndexOf("/") + 1)
                                             : qsTr("Choose")
                description: dialog.keyFile
                onClicked: pageStack.push(keyPicker)
            }
        }
        VerticalScrollDecorator { }
    }
}
