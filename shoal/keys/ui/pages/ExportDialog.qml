// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

// Export a portable KeePass (KDBX 4) copy protected by its own password,
// without the device key. The result is reported on the import and export
// page below, which listens for ShoalKeys.finished("export").

import QtQuick
import Sailfish.Silica 1.0
import Shipwright.Keys 1.0

Dialog {
    id: dialog
    objectName: "exportDialog"

    canAccept: !ShoalKeys.busy && path.text.trim().length > 0 && exportPassword.text.length > 0
               && exportPassword.text === exportConfirm.text

    onAccepted: ShoalKeys.exportKdbx(path.text.trim(), exportPassword.text)

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column
            width: parent.width

            DialogHeader {
                acceptText: qsTr("Export")
                title: qsTr("Export a KeePass copy")
            }

            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                bottomPadding: Theme.paddingLarge
                wrapMode: Text.Wrap
                font.pixelSize: Theme.fontSizeSmall
                color: Theme.secondaryHighlightColor
                text: qsTr("The copy opens in KeePassXC, KeePassDX and Strongbox with the password you set here. Anyone with the file and that password can read your passwords.")
            }

            TextField {
                id: path
                objectName: "exportPath"
                width: parent.width
                label: qsTr("File")
                placeholderText: qsTr("File")
                text: ShoalKeys.defaultExportPath()
                inputMethodHints: Qt.ImhNoAutoUppercase | Qt.ImhNoPredictiveText | Qt.ImhUrlCharactersOnly
                EnterKey.iconSource: "image://theme/icon-m-enter-next"
                EnterKey.onClicked: exportPassword.focus = true
            }
            PasswordField {
                id: exportPassword
                width: parent.width
                label: qsTr("Password for the copy")
                placeholderText: qsTr("Password for the copy")
                description: text.length ? JSON.parse(ShoalKeys.strength(text)).label : ""
                EnterKey.iconSource: "image://theme/icon-m-enter-next"
                EnterKey.onClicked: exportConfirm.focus = true
            }
            PasswordField {
                id: exportConfirm
                width: parent.width
                label: qsTr("Repeat the password")
                placeholderText: qsTr("Repeat the password")
                errorHighlight: text.length > 0 && text !== exportPassword.text
                description: errorHighlight ? qsTr("The passwords do not match") : ""
                EnterKey.iconSource: "image://theme/icon-m-enter-accept"
                EnterKey.enabled: dialog.canAccept
                EnterKey.onClicked: dialog.accept()
            }
        }
        VerticalScrollDecorator { }
    }
}
